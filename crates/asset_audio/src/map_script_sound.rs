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
