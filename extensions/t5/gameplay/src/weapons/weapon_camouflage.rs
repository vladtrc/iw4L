use asset_core::{AssetRef, MaterialKey};
use asset_game::*;
use asset_material::{MaterialCatalog, TS_COLOR_MAP, TS_SPECULAR_MAP};
use std::collections::HashMap;
pub const POLICY: WeaponCamouflagePolicy = WeaponCamouflagePolicy {
    model: None,
    materials: Some(WeaponMaterialCamouflagePolicy {
        options: "mp/weaponoptions.csv",
        choices: "mp/attachmentTable.csv",
        label: "T5 camouflage",
        image_source: |root| {
            asset_transport::find_common_mp_for_envelope(root, ext_t5_fastfile::ZONE_VERSION_PC)
        },
        compiler: || {
            Box::new(Compiler {
                variants: HashMap::new(),
            })
        },
    }),
};
struct Compiler {
    variants: HashMap<(usize, u8, bool), MaterialKey>,
}
impl MaterialCamouflageCompiler for Compiler {
    fn prepare(
        &mut self,
        table: &CapturedStringTable,
        choices: &CapturedStringTable,
        parent: &str,
        keys: &[MaterialKey],
        materials: &mut MaterialCatalog,
    ) -> Vec<WeaponCamouflage> {
        prepare_material_variants(table, choices, parent, keys, materials, &mut self.variants)
    }
}
fn prepare_material_variants(
    table: &CapturedStringTable,
    choices: &CapturedStringTable,
    parent: &str,
    keys: &[MaterialKey],
    materials: &mut MaterialCatalog,
    variants: &mut HashMap<(usize, u8, bool), MaterialKey>,
) -> Vec<WeaponCamouflage> {
    let ns = crate::ID;
    let Some(weapon) = (0..table.rows as i32)
        .find(|&row| table.cell(row, 1) == "weapon" && table.cell(row, 2) == parent)
    else {
        return Vec::new();
    };
    let Some(base) = (0..table.rows as i32)
        .find(|&row| table.cell(row, 1) == "camo" && table.cell(row, 0) == "0")
    else {
        return Vec::new();
    };
    let image = |name: &str, materials: &MaterialCatalog| materials.image_index_by_key(ns, name);
    let source_materials: Vec<_> = keys
        .iter()
        .filter_map(|key| {
            let id = materials.material_index_by_key(key)?.order();
            Some((key.clone(), id, materials.materials.get(id)?.clone()))
        })
        .collect();
    let mut out = Vec::new();
    for choice in 0..choices.rows as i32 {
        if choices.cell(choice, 1) != "camo" {
            continue;
        }
        let Ok(slot @ 1..=63) = choices.cell(choice, 0).parse::<u8>() else {
            continue;
        };
        let Some(row) = (0..table.rows as i32).find(|&row| {
            table.cell(row, 1) == "camo" && table.cell(row, 0).parse::<u8>() == Ok(slot)
        }) else {
            continue;
        };
        let gold = table.cell(row, 2) == "gold";
        let mut swaps = Vec::new();
        for (key, id, source) in &source_materials {
            let mut material = source.clone();
            let mut changed = false;
            let mut complete = true;
            let mut gold_material = false;
            if gold {
                let group = if (5..=8)
                    .chain(13..=17)
                    .any(|col| key.name == format!("mc/{}", table.cell(weapon, col)))
                {
                    "gold"
                } else if (9..=12)
                    .chain(18..=22)
                    .any(|col| key.name == format!("mc/{}", table.cell(weapon, col)))
                {
                    "black"
                } else {
                    continue;
                };
                gold_material = group == "gold";
                let Some(palette) = (0..table.rows as i32).find(|&r| table.cell(r, 1) == group)
                else {
                    continue;
                };
                for texture in &mut material.textures {
                    let col = match texture.semantic {
                        TS_COLOR_MAP => 2,
                        TS_SPECULAR_MAP => 3,
                        27 => 4,
                        19 => 5,
                        _ => continue,
                    };
                    let name = table.cell(palette, col);
                    if name.is_empty() {
                        continue;
                    }
                    match image(name, materials) {
                        Some(to) => {
                            texture.image = Some(to);
                            changed = true;
                        }
                        None => complete = false,
                    }
                }
            } else {
                for texture in &mut material.textures {
                    let Some(from) = texture.image.and_then(|id| materials.images.get(id)) else {
                        continue;
                    };
                    let Some(column) = (2..table.columns as i32).find(|&col| {
                        let name = table.cell(base, col);
                        !name.is_empty()
                            && from.name.as_str() == name
                            && (name == table.cell(weapon, 3)
                                || name == table.cell(weapon, 4)
                                || col == 2)
                    }) else {
                        continue;
                    };
                    match image(table.cell(row, column), materials) {
                        Some(to) => {
                            texture.image = Some(to);
                            changed = true;
                        }
                        None => complete = false,
                    }
                }
            }
            if !changed || !complete {
                continue;
            }
            if let Some(variant) = variants.get(&(*id, slot, gold_material)) {
                swaps.push((key.clone(), variant.clone()));
                continue;
            }
            let role = if gold_material {
                "gold"
            } else if gold {
                "black"
            } else {
                "pattern"
            };
            let name = format!("{}/camouflage/{slot}/{role}", key.name);
            material.name = AssetRef::Real(name.clone());
            materials.link_material(material);
            let to = MaterialKey {
                namespace: ns,
                name,
            };
            variants.insert((*id, slot, gold_material), to.clone());
            swaps.push((key.clone(), to));
        }
        if !swaps.is_empty() {
            out.push(WeaponCamouflage {
                slot,
                name: choices
                    .cell(choice, 3)
                    .strip_prefix("MPUI_CAMO_")
                    .unwrap_or(choices.cell(choice, 4))
                    .to_ascii_lowercase(),
                caption_key: choices.cell(choice, 3).to_owned(),
                preview: format!("t5:material/{}", choices.cell(choice, 6)),
                materials: swaps,
            });
        }
    }
    out
}
