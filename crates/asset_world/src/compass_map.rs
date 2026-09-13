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
        let Some(material) = setup_minimap_material(&text) else {
            return;
        };
        self.decl.material = Some(material.to_owned());
        self.decl.max_range = compass_max_range(&text);
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

pub fn setup_minimap_material(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let call = lower.find("setupminimap")?;
    quoted_after(script, call + "setupminimap".len())
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
