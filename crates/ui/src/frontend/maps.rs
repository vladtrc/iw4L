use crate::MenuMapList;

pub fn map_label(map: &str) -> String {
    map.split_once(':')
        .map_or(map, |(_, name)| name)
        .trim_start_matches("mp_")
        .replace('_', " ")
        .to_uppercase()
}

pub fn map_preview(map: &str) -> String {
    let (token, stem) = map.split_once(':').unwrap_or(("iw4", map));
    let Some(family) = asset_core::FamilyId::parse(token) else {
        return String::new();
    };
    let Some(policy) = asset_game::map_presentation_family(family) else {
        return String::new();
    };
    if stem.is_empty() {
        return String::new();
    }
    format!("{}:material/{}", family.as_str(), (policy.preview)(stem))
}

pub fn pack_maps(maps: &MenuMapList, pack: usize) -> &[String] {
    maps.0.get(pack).map_or(&[], |pack| pack.maps.as_slice())
}

#[derive(bevy::prelude::Resource, Default)]
pub struct MapPresentation {
    ui_images: asset_material::UiImagePublication,
    entries: std::collections::BTreeMap<String, (asset_core::AssetKey, String)>,
}

impl MapPresentation {
    pub fn from_tables(
        tables: &[(asset_core::FamilyId, asset_game::CapturedStringTable)],
        ui_images: asset_material::UiImagePublication,
    ) -> Self {
        let mut entries = std::collections::BTreeMap::new();
        for (namespace, table) in tables {
            let Some(schema) =
                asset_game::map_presentation_family(*namespace).and_then(|policy| policy.table)
            else {
                continue;
            };
            if !table.name.eq_ignore_ascii_case(schema.name) {
                continue;
            }
            for row in 0..table.rows as i32 {
                let map = table.cell(row, schema.map_column);
                if !map.starts_with(schema.map_prefix) {
                    continue;
                }
                let Ok(title) = asset_core::AssetKey::new(
                    *namespace,
                    asset_core::AssetKind::Localize,
                    table.cell(row, schema.title_column),
                ) else {
                    continue;
                };
                let Ok(image) = asset_core::AssetKey::new(
                    *namespace,
                    asset_core::AssetKind::Material,
                    table.cell(row, schema.image_column),
                ) else {
                    continue;
                };
                entries.insert(
                    format!("{}:{map}", namespace.as_str()),
                    (title, image.to_string()),
                );
            }
        }
        Self { entries, ui_images }
    }

    pub fn label(&self, map: &str, strings: Option<&asset_game::LocalizeCatalog>) -> String {
        self.entries
            .get(map)
            .and_then(|(title, _)| strings?.text_asset(title))
            .map(str::to_owned)
            .unwrap_or_else(|| map_label(map))
    }

    pub fn preview(&self, map: &str) -> String {
        self.entries
            .get(map)
            .map(|(_, image)| {
                let loaded = asset_core::AssetKey::parse(image)
                    .is_ok_and(|key| self.ui_images.has_zone_image(key.namespace, &key.name));
                if loaded {
                    image.clone()
                } else {
                    map_preview(map)
                }
            })
            .unwrap_or_else(|| map_preview(map))
    }
}
