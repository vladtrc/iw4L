use bevy::prelude::Resource;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapScriptSoundFacts {
    pub ambient_alias: Option<String>,

    pub attackers: Option<String>,

    pub defenders: Option<String>,

    pub script: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub struct SessionMapScriptSound(pub MapScriptSoundFacts);

#[derive(Clone, Debug, Default)]
pub struct MapScriptSoundSource {
    facts: MapScriptSoundFacts,
}

impl MapScriptSoundSource {
    pub fn capture(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        if self.facts.script.is_some() || !is_map_main_script(name) {
            return;
        }
        let owned;
        let bytes = if zlib_compressed {
            match asset_transport::inflate_zlib(data) {
                Ok(inflated) => {
                    owned = inflated;
                    owned.as_slice()
                }
                Err(_) => return,
            }
        } else {
            data
        };
        let Ok(text) = core::str::from_utf8(bytes) else {
            return;
        };
        let ambient = ambient_play_alias(text);
        let attackers = game_string_assignment(text, "attackers");
        let defenders = game_string_assignment(text, "defenders");
        if ambient.is_none() && attackers.is_none() && defenders.is_none() {
            return;
        }
        self.facts = MapScriptSoundFacts {
            ambient_alias: ambient.map(str::to_owned),
            attackers: attackers.map(str::to_owned),
            defenders: defenders.map(str::to_owned),
            script: Some(name.to_owned()),
        };
    }

    pub fn capture_iw5(&mut self, name: &str, compressed_stack: &[u8], bytecode: &[u8]) {
        let script_name = if name.ends_with(".gsc") {
            name.to_owned()
        } else {
            format!("{name}.gsc")
        };
        if self.facts.script.is_some() || !is_map_main_script(&script_name) {
            return;
        }
        let Ok(stack) = asset_transport::inflate_zlib(compressed_stack) else {
            return;
        };
        if let Some((attackers, defenders)) = iw5_initial_teams(&stack, bytecode) {
            self.facts.attackers = Some(attackers.to_owned());
            self.facts.defenders = Some(defenders.to_owned());
            self.facts.script = Some(name.to_owned());
        }
    }

    pub fn finish(self) -> MapScriptSoundFacts {
        self.facts
    }
}

pub fn is_map_main_script(name: &str) -> bool {
    let n = name.replace('\\', "/");
    let Some(file) = n.strip_prefix("maps/mp/") else {
        return false;
    };
    if file.starts_with('_') || !file.ends_with(".gsc") {
        return false;
    }
    if file.ends_with("_fx.gsc") || file.ends_with("_precache.gsc") {
        return false;
    }
    file.starts_with("mp_") && !file.contains('/')
}

pub fn ambient_play_alias(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let call = lower.find("ambientplay")?;
    quoted_after(script, call + "ambientplay".len())
}

pub fn game_string_assignment<'a>(script: &'a str, key: &str) -> Option<&'a str> {
    game_nested_string_assignment(script, &[key])
}

pub fn game_nested_string_assignment<'a>(script: &'a str, keys: &[&str]) -> Option<&'a str> {
    if keys.is_empty() {
        return None;
    }
    let lower = script.to_ascii_lowercase();
    let mut from = 0;
    'next: while let Some(rel) = lower[from..].find("game[") {
        let mut pos = from + rel + "game[".len();
        from = pos;
        for (i, key) in keys.iter().enumerate() {
            let Some((key_end, assigned_key)) = quoted_after_span(script, pos) else {
                continue 'next;
            };
            if !assigned_key.eq_ignore_ascii_case(key) {
                continue 'next;
            }
            pos = key_end;
            if i + 1 < keys.len() {
                let rest = script.get(pos..).unwrap_or("");
                let Some(open) = rest.find('[') else {
                    continue 'next;
                };
                pos += open + 1;
            }
        }
        return quoted_after(script, pos);
    }
    None
}

fn quoted_after(script: &str, from: usize) -> Option<&str> {
    quoted_after_span(script, from).map(|(_, s)| s)
}

fn quoted_after_span(script: &str, from: usize) -> Option<(usize, &str)> {
    let rest = script.get(from..)?;
    let open = rest.find('"')?;
    let start = from + open + 1;
    let len = script.get(start..)?.find('"')?;
    let value = &script[start..start + len];
    if value.is_empty() {
        return None;
    }
    Some((start + len + 1, value))
}

// Decode the unconditional prefix of main. Branches and unsupported instructions
// end the extraction; strings elsewhere in a script are not team declarations.
fn iw5_initial_teams<'a>(mut stack: &'a [u8], bytecode: &[u8]) -> Option<(&'a str, &'a str)> {
    fn take<'a>(data: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
        let (head, tail) = data.split_at_checked(count)?;
        *data = tail;
        Some(head)
    }
    fn string<'a>(data: &mut &'a [u8]) -> Option<&'a str> {
        let n = data.iter().position(|&b| b == 0)?;
        std::str::from_utf8(&take(data, n + 1)?[..n]).ok()
    }
    fn token(data: &mut &[u8]) -> Option<()> {
        let id = u16::from_le_bytes(take(data, 2)?.try_into().ok()?);
        if id == 0 {
            string(data)?;
        }
        Some(())
    }
    let len = u32::from_le_bytes(take(&mut stack, 4)?.try_into().ok()?) as usize;
    let main = u16::from_le_bytes(take(&mut stack, 2)?.try_into().ok()?);
    if main != 0x672 && (main != 0 || string(&mut stack)? != "main") {
        return None;
    }
    let mut code = bytecode.get(1..1usize.checked_add(len)?)?;
    let mut literals = Vec::new();
    let mut reference = 0;
    let mut attackers = None;
    let mut defenders = None;
    while !code.is_empty() {
        let op = take(&mut code, 1)?[0];
        match op {
            0x0a => {
                if reference != 0 {
                    return None;
                }
                take(&mut code, 2)?;
                literals.push(string(&mut stack)?);
                continue;
            }
            0x41 if literals.len() == 2 => {
                reference = 1;
                continue;
            }
            0x1c if reference == 1 => {
                reference = 2;
                continue;
            }
            0x7b if reference == 2 => {
                let value = literals[0];
                if matches!(value, "allies" | "axis") {
                    match literals[1] {
                        "attackers" => attackers = Some(value),
                        "defenders" => defenders = Some(value),
                        _ => {}
                    }
                }
                if let (Some(a), Some(d)) = (attackers, defenders) {
                    return (a != d).then_some((a, d));
                }
            }
            0x28..=0x2e | 0x65 => {
                token(&mut stack)?;
                token(&mut stack)?;
                take(&mut code, if (0x2b..=0x2e).contains(&op) { 4 } else { 3 })?;
            }
            0x21..=0x27 | 0x64 => {
                take(&mut code, if (0x24..=0x27).contains(&op) { 4 } else { 3 })?;
            }
            0x02 | 0x03 => {
                take(&mut code, 1)?;
            }
            0x04 | 0x05 | 0x07 | 0x08 | 0x84..=0x89 | 0x8b..=0x90 => {
                take(&mut code, 2)?;
            }
            0x06 | 0x09 => {
                take(&mut code, 4)?;
            }
            0x38 => {
                take(&mut code, 12)?;
            }
            0x8a | 0x91 => {
                take(&mut code, 3)?;
            }
            0x0b | 0x0c | 0x20 | 0x75 | 0x76 | 0x93 => {}
            _ => return None,
        }
        literals.clear();
        reference = 0;
    }
    None
}
