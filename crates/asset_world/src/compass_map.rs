use asset_material::MaterialCatalog;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapCompassDeclaration {
    pub material: Option<String>,

    pub image: Option<String>,

    pub max_range: Option<f32>,

    pub script: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct MapCompassSource {
    decl: MapCompassDeclaration,
}

impl MapCompassSource {
    pub fn capture(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        if self.decl.material.is_some() || !name.ends_with(".gsc") {
            return;
        }
        let Some(text) = crate::decode_rawfile_text(data, zlib_compressed) else {
            return;
        };
        let text = strip_gsc_comments(&text);
        let Some(material) = setup_minimap_material(&text) else {
            return;
        };
        self.decl.material = Some(material.to_owned());
        self.decl.max_range = compass_max_range(&text);
        self.decl.script = Some(name.to_owned());
    }

    pub fn capture_iw5(&mut self, name: &str, compressed_stack: &[u8]) {
        if self.decl.material.is_some() {
            return;
        }
        let Ok(stack) = asset_transport::inflate_zlib(compressed_stack) else {
            return;
        };
        let Some(material) = iw5_minimap_literal(&stack) else {
            return;
        };
        self.decl.material = Some(material.to_owned());
        self.decl.script = Some(name.to_owned());
    }

    pub fn resolve(mut self, materials: &MaterialCatalog) -> MapCompassDeclaration {
        if let Some(name) = self.decl.material.as_deref() {
            self.decl.image = materials
                .materials
                .iter()
                .find(|m| m.name == name)
                .and_then(|m| materials.hud_image_name(m))
                .map(str::to_owned);
        }
        self.decl
    }
}

// T5 maps call setupMiniMap("…_wager") first, under `if xblive_wagermatch`;
// that image covers only the wager play area.
pub fn setup_minimap_material(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let mut calls = lower
        .match_indices("setupminimap")
        .filter_map(|(at, key)| quoted_after(script, at + key.len()));
    let first = calls.next()?;
    if !first.ends_with("_wager") {
        return Some(first);
    }
    calls.find(|m| !m.ends_with("_wager")).or(Some(first))
}

fn strip_gsc_comments(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut rest = script;
    while let Some(c) = rest.chars().next() {
        if c == '"' {
            let len = rest[1..].find('"').map_or(rest.len(), |n| n + 2);
            out.push_str(&rest[..len]);
            rest = &rest[len..];
        } else if rest.starts_with("//") {
            rest = &rest[rest.find('\n').unwrap_or(rest.len())..];
        } else if rest.starts_with("/*") {
            rest = rest[2..].find("*/").map_or("", |n| &rest[n + 4..]);
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

// Compiled IW5 map scripts keep setupMiniMap's argument only as a stack
// literal; the call itself is a token id, so the literal's prefix is the
// anchor. `resolve` rejects a name that is not a material in the zone.
fn iw5_minimap_literal(stack: &[u8]) -> Option<&str> {
    const PREFIX: &[u8] = b"compass_map_";
    let start = stack.windows(PREFIX.len()).position(|w| w == PREFIX)?;
    let len = stack[start..].iter().position(|&b| b == 0)?;
    std::str::from_utf8(&stack[start..start + len]).ok()
}

pub fn compass_max_range(script: &str) -> Option<f32> {
    let lower = script.to_ascii_lowercase();
    let key = lower.find("compassmaxrange")?;
    let after_key = key + "compassmaxrange".len();

    let close = script[after_key..].find('"')? + after_key + 1;
    quoted_after(script, close)?.trim().parse().ok()
}

fn quoted_after(script: &str, from: usize) -> Option<&str> {
    let rest = script.get(from..)?;
    let open = rest.find('"')? + 1;
    let len = rest[open..].find('"')?;
    Some(&rest[open..open + len]).filter(|s| !s.is_empty())
}
