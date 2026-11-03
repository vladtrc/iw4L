use asset_game::*;
use asset_material::MaterialCatalog;
pub const POLICY: WeaponCamouflagePolicy = WeaponCamouflagePolicy {
    model: Some(WeaponModelCamouflagePolicy {
        table: "mp/camoTable.csv",
        label: "IW5 camouflage",
        choices,
    }),
    materials: None,
};
fn choices(
    table: &CapturedStringTable,
    materials: &MaterialCatalog,
) -> Vec<WeaponCamouflageChoice> {
    let choices: Vec<_> = (0..table.rows as i32)
        .filter_map(|at| {
            let slot = table
                .cell(at, 0)
                .parse::<u8>()
                .ok()
                .filter(|slot| *slot != 0)?;
            let name = table.cell(at, 1);
            if name.is_empty() {
                return None;
            }
            Some(WeaponCamouflageChoice {
                slot,
                name: name.to_owned(),
                caption_key: table.cell(at, 2).to_owned(),
                preview: materials
                    .material_index_by_ns(crate::ID, table.cell(at, 4))
                    .and_then(|index| materials.materials.get(index.order()))
                    .and_then(|material| materials.hud_image_name(material))
                    .map(|image| format!("iw5:material/{image}"))
                    .or_else(|| {
                        table
                            .cell(at, 4)
                            .strip_prefix("ui_camoskin_")
                            .map(|name| format!("iw5:material/weapon_camo_menu_{name}"))
                    })
                    .unwrap_or(String::new()),
            })
        })
        .collect();
    choices
}
