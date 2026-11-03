use super::*;
use asset_core::MaterialKey;
use asset_game::WeaponCamouflage;
use ext_t6_fastfile::{LoadedAsset, ZoneLoad, weapon_camo::WeaponCamoView};

fn asset_name<'a>(load: &'a ZoneLoad, asset: &LoadedAsset) -> Option<&'a str> {
    header_str(
        load,
        &asset.header,
        if asset.ty == ext_t6_fastfile::AssetType::Image {
            IMAGE_NAME
        } else {
            0
        },
    )
    .map(|name| name.trim_start_matches(','))
}

fn key(name: &str) -> MaterialKey {
    MaterialKey {
        namespace: crate::ID,
        name: name.to_owned(),
    }
}

fn parameter(material: &mut T6NativeMaterial, index: u8, literal: [f32; 4]) {
    let name_hash = crate::material::t6_techset::weapon_parameter_hash(index);
    material
        .constants
        .retain(|constant| constant.name_hash != name_hash);
    material.constants.push(asset_material::MaterialConstant {
        name_hash,
        name: *b"weaponParam\0",
        literal,
    });
}

fn image_alias(name: &str) -> &str {
    match name {
        "multicam" => "devgru",
        "matuka" => "bloodshot",
        "ghostex" => "ghostex_delta6",
        "kryptek" => "kryptek_typhon",
        "elite1" => "elite",
        "collectors" => "bo2collectors",
        _ => name,
    }
}

fn material_alias(name: &str) -> &str {
    match name {
        "carbon" => "carbon_fiber",
        _ => name,
    }
}

fn image(
    load: &ZoneLoad,
    asset: &LoadedAsset,
    zones: &[&ZoneLoad],
    ipaks: &[crate::zone::ipak::IPak],
    decoded: &mut DecodedTextures,
) -> Option<(String, Arc<Image>)> {
    let name = asset_name(load, asset)?;
    let (load, asset) = if asset.header[4] == 0 {
        zones.iter().find_map(|zone| {
            zone.assets
                .iter()
                .find(|candidate| {
                    candidate.ty == ext_t6_fastfile::AssetType::Image
                        && candidate.header[4] != 0
                        && asset_name(zone, candidate) == Some(name)
                })
                .map(|asset| (*zone, asset))
        })?
    } else {
        (load, asset)
    };
    let texels = match decoded.get(name) {
        Some(texels) => Arc::clone(texels),
        None => {
            let texels = Arc::new(decode_map_image(load, asset, ipaks).ok()?);
            decoded.insert(name.to_owned(), Arc::clone(&texels));
            texels
        }
    };
    Some((name.to_owned(), texels))
}

struct Choice {
    slot: u8,
    name: String,
    caption_key: String,
    preview: String,
}

fn choices(zones: &[&ZoneLoad]) -> Vec<Choice> {
    let Some(table) = zones
        .iter()
        .flat_map(|zone| {
            zone.assets
                .iter()
                .filter_map(|asset| crate::weapons::weapon_t6::capture_t6_string_table(zone, asset))
        })
        .find(|table| table.name.eq_ignore_ascii_case("mp/attachmenttable.csv"))
    else {
        return Vec::new();
    };
    (0..table.rows as i32)
        .filter(|&row| table.cell(row, 1) == "camo" && table.cell(row, 0) != "0")
        .enumerate()
        .filter_map(|(index, row)| {
            let slot = u8::try_from(index + 1).ok().filter(|slot| *slot <= 63)?;
            Some(Choice {
                slot,
                name: table.cell(row, 4).strip_prefix("camo_")?.to_owned(),
                caption_key: table.cell(row, 3).to_owned(),
                preview: format!("t6:material/{}", table.cell(row, 6)),
            })
        })
        .collect()
}

struct CamouflageRecipe<'a> {
    camo: WeaponCamoView<'a>,
    keys: std::collections::BTreeSet<String>,
}

impl CamouflageRecipe<'_> {
    fn patterned(
        &self,
        load: &ZoneLoad,
        choice: &Choice,
        zones: &[&ZoneLoad],
        ipaks: &[crate::zone::ipak::IPak],
        decoded: &mut DecodedTextures,
        content: &mut T6Content,
    ) -> Vec<(MaterialKey, MaterialKey)> {
        let camo = self.camo;
        let keys = &self.keys;

        let alias = image_alias(&choice.name);
        let expected = format!("t6_camo_{alias}_pattern");
        let Some(set) = (0..camo.image_set_count())
            .filter_map(|index| camo.image_set(index))
            .find(|set| {
                set.pattern.and_then(|asset| asset_name(load, asset)) == Some(expected.as_str())
            })
        else {
            return Vec::new();
        };
        let Some(pattern) = set
            .pattern
            .and_then(|asset| image(load, asset, zones, ipaks, decoded))
        else {
            return Vec::new();
        };
        let solid = set
            .solid
            .and_then(|asset| image(load, asset, zones, ipaks, decoded));
        let pattern_base = camo
            .pattern_base()
            .and_then(|asset| asset_name(load, asset));
        let solid_base = camo.solid_base().and_then(|asset| asset_name(load, asset));
        let mut variants = Vec::new();
        for (name, capture) in &content.materials {
            if !keys.contains(name) {
                continue;
            }
            let Some(source) = &capture.native else {
                continue;
            };
            let mut material = source.clone();
            let mut changed = false;
            let mut complete = true;
            for texture in &mut material.textures {
                let replacement = if Some(texture.image.trim_start_matches(',')) == pattern_base {
                    Some(&pattern)
                } else if Some(texture.image.trim_start_matches(',')) == solid_base {
                    if solid.is_none() {
                        complete = false;
                    }
                    solid.as_ref()
                } else {
                    None
                };
                if let Some((name, texels)) = replacement {
                    texture.image.clone_from(name);
                    texture.texels = Arc::clone(texels);
                    changed = true;
                }
            }
            if changed && complete {
                parameter(
                    &mut material,
                    2,
                    [set.offset[0], set.offset[1], set.scale, 1.0],
                );
                let target = format!(
                    "{name}/camouflage/{}/{}",
                    camo.name().unwrap_or(""),
                    choice.name
                );
                variants.push((name.clone(), target, material));
            }
        }
        variants
            .into_iter()
            .map(|(from, to, native)| {
                content
                    .materials
                    .entry(to.clone())
                    .or_insert(T6MaterialCapture {
                        native: Some(native),
                    });
                (key(&from), key(&to))
            })
            .collect()
    }
}

fn special(
    load: &ZoneLoad,
    camo: WeaponCamoView<'_>,
    choice: &Choice,
    zones: &[&ZoneLoad],
    ipaks: &[crate::zone::ipak::IPak],
    decoded: &mut DecodedTextures,
    content: &mut T6Content,
) -> Vec<(MaterialKey, MaterialKey)> {
    let prefix = format!("mc/mtl_weapon_camo_{}", material_alias(&choice.name));
    let mut swaps = Vec::new();
    for slot in 0..camo.material_set_count() {
        let Ok(set) = camo.material_set(slot) else {
            continue;
        };
        for recipe in set {
            for index in 0..recipe.material_count() {
                let Some((base, template)) = recipe.material_pair(index) else {
                    continue;
                };
                let Some(template_name) = asset_name(load, template).filter(|name| {
                    *name == prefix
                        || name
                            .strip_prefix(&prefix)
                            .is_some_and(|rest| rest.starts_with('_'))
                }) else {
                    continue;
                };
                let Some(base_name) = asset_name(load, base) else {
                    continue;
                };
                let Some(source) = content
                    .materials
                    .get(base_name)
                    .and_then(|capture| capture.native.as_ref())
                    .cloned()
                else {
                    continue;
                };
                let target = format!(
                    "{base_name}/camouflage/{}/{template_name}",
                    camo.name().unwrap_or("")
                );
                if !content.materials.contains_key(&target) {
                    let Some(mut native) = capture_material_header(
                        load,
                        zones,
                        None,
                        template,
                        &mut content.techsets,
                        &mut content.report,
                    )
                    .and_then(|header| {
                        header.decode(
                            load,
                            zones,
                            ColourMapAlpha::Gloss,
                            ipaks,
                            decoded,
                            &mut content.report,
                        )
                    }) else {
                        continue;
                    };
                    let flags = recipe.replace_flags();
                    let mut complete = true;
                    for texture in &mut native.textures {
                        let semantic = match texture.semantic {
                            12 if flags & 1 != 0 => Some(asset_material::TS_COLOR_MAP),
                            13 if flags & 2 != 0 => Some(asset_material::TS_NORMAL_MAP),
                            14 if flags & 4 != 0 => Some(asset_material::TS_SPECULAR_MAP),
                            _ => None,
                        };
                        if let Some(semantic) = semantic {
                            if let Some(base) = source
                                .textures
                                .iter()
                                .find(|base| base.semantic == semantic)
                            {
                                texture.image.clone_from(&base.image);
                                texture.texels = Arc::clone(&base.texels);
                            } else {
                                complete = false;
                            }
                        }
                    }
                    if !complete {
                        continue;
                    }
                    let constants = recipe.shader_constants();
                    parameter(&mut native, 6, constants[..4].try_into().unwrap());
                    parameter(&mut native, 7, constants[4..].try_into().unwrap());
                    content.materials.insert(
                        target.clone(),
                        T6MaterialCapture {
                            native: Some(native),
                        },
                    );
                }
                swaps.push((key(base_name), key(&target)));
            }
        }
    }
    swaps
}

fn mesh_keys(names: &[String], content: &T6Content) -> std::collections::BTreeSet<String> {
    content
        .models
        .iter()
        .filter(|model| names.contains(&model.skel.name))
        .flat_map(|model| model.surface_materials.iter().flatten().cloned())
        .collect()
}

pub(super) fn capture(
    load: &ZoneLoad,
    zones: &[&ZoneLoad],
    ipaks: &[crate::zone::ipak::IPak],
    decoded: &mut DecodedTextures,
    content: &mut T6Content,
) {
    let choices = choices(zones);
    let mut prepared = BTreeMap::new();
    for asset in load
        .assets
        .iter()
        .filter(|asset| asset.ty == ext_t6_fastfile::AssetType::Weapon)
    {
        let Some(weapon) = ext_t6_fastfile::weapon::WeaponView::new(load, asset) else {
            continue;
        };
        let Some(name) = weapon.name() else {
            continue;
        };
        let Some(camo) = weapon.camouflage() else {
            continue;
        };
        let mut meshes = [
            ext_t6_fastfile::weapon::def::GUN_XMODEL,
            ext_t6_fastfile::weapon::def::WORLD_MODEL,
        ]
        .into_iter()
        .filter_map(|field| weapon.def_asset_array_name(field, 0))
        .map(crate::weapons::weapon_catalog::captured::t6_model_name)
        .collect::<Vec<_>>();
        for view in [true, false] {
            for slot in 0..ext_t6_fastfile::weapon::variant::ATTACH_MODEL_COUNT {
                if let Some((name, _, _)) = weapon.attached_model(slot, view) {
                    meshes.push(crate::weapons::weapon_catalog::captured::t6_model_name(
                        name,
                    ));
                }
            }
        }
        let mut recipes = vec![CamouflageRecipe {
            camo,
            keys: mesh_keys(&meshes, content),
        }];
        for unique in weapon.attachment_uniques() {
            let Some(camo) = unique.camouflage() else {
                continue;
            };
            let meshes = [true, false]
                .into_iter()
                .flat_map(|view| {
                    crate::weapons::weapon_catalog::captured::t6_attachment_models(unique, view)
                        .into_iter()
                        .map(|model| model.copy)
                })
                .chain(
                    crate::weapons::weapon_catalog::captured::t6_attachment_ads_model(unique)
                        .map(|model| model.copy),
                )
                .collect::<Vec<_>>();
            recipes.push(CamouflageRecipe {
                camo,
                keys: mesh_keys(&meshes, content),
            });
        }
        let mut camos = Vec::new();
        for choice in &choices {
            let mut swaps = BTreeMap::new();
            for recipe in &recipes {
                let recipe_key = (
                    recipe.camo.name().unwrap_or("").to_owned(),
                    recipe.keys.clone(),
                    choice.slot,
                );
                let variants = prepared.entry(recipe_key).or_insert_with(|| {
                    let mut swaps = recipe.patterned(load, choice, zones, ipaks, decoded, content);
                    swaps.extend(special(
                        load,
                        recipe.camo,
                        choice,
                        zones,
                        ipaks,
                        decoded,
                        content,
                    ));
                    swaps
                });
                for (from, to) in variants {
                    swaps.insert(from.name.clone(), to.clone());
                }
            }
            if !swaps.is_empty() {
                camos.push(WeaponCamouflage {
                    slot: choice.slot,
                    name: choice.name.clone(),
                    caption_key: choice.caption_key.clone(),
                    preview: choice.preview.clone(),
                    materials: swaps
                        .into_iter()
                        .map(|(from, to)| (key(&from), to))
                        .collect(),
                });
            }
        }
        content.camouflages.insert(name.to_owned(), camos);
    }
    content.report.push(format!(
        "t6 camouflage capture: {} weapon recipes, {} choices",
        content.camouflages.len(),
        choices.len()
    ));
}
