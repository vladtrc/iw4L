use crate::material::t6_techset::{NativeTechniqueSets, T6MaterialBindings};
use asset_material::MaterialCatalog;
use std::path::Path;

pub struct T6MaterialCapture {
    pub native: Option<T6NativeMaterial>,
}

#[derive(Clone)]
pub struct T6NativeMaterial {
    pub header: Vec<u8>,
    pub technique_set: String,
    pub textures: Vec<crate::material::t6_techset::T6Texture>,
    pub constants: Vec<asset_material::MaterialConstant>,
    pub state: crate::material::t6_techset::T6MaterialState,
}

pub fn native_material_definition(
    path: &Path,
    name: &str,
    h: &[u8],
    technique_set: &str,
    is_sky: bool,
) -> Result<asset_material::AuthoredMaterial, String> {
    if h.len() < 89 {
        return Err(format!("T6 material {name}: truncated header"));
    }
    Ok(asset_material::AuthoredMaterial {
        name: asset_core::AssetRef::Real(name.to_owned()),
        namespace: crate::ID,
        technique_set: asset_core::AssetRef::Real(
            crate::material::t6_techset::T6Draw::Lit.technique_set_name(technique_set),
        ),
        technique_set_edge: Default::default(),
        draw_surf: u64::from_le_bytes(h[16..24].try_into().unwrap()),
        sort_key: if is_sky {
            asset_iw4::SORT_KEY_SKYBOX
        } else {
            h[9]
        },
        info_game_flags: h[4],
        texture_atlas: Some([h[10], h[11]]),
        surface_type_bits: Some(u32::from_le_bytes(h[24..28].try_into().unwrap())),
        layered_surface_types: Some(u32::from_le_bytes(h[28..32].try_into().unwrap())),
        state_flags: h[87],
        camera_region: if is_sky {
            asset_iw4::CAMERA_REGION_LIT_OPAQUE
        } else {
            camera_region(h[88])
        },
        state_bits: Vec::new(),
        state_bits_entry: None,
        native_state_bits_entry: None,

        technique_table: None,
        route: None,
        textures: Vec::new(),
        constants: Vec::new(),
        zone: asset_core::ZoneOwner::from_zone_path(path),
    })
}

fn camera_region(t6: u8) -> u8 {
    match t6 {
        0 | 2 | 6 => asset_iw4::CAMERA_REGION_LIT_OPAQUE,
        1 => asset_iw4::CAMERA_REGION_LIT_TRANS,
        3..=5 => asset_iw4::CAMERA_REGION_EMISSIVE,
        7 => asset_iw4::CAMERA_REGION_DEPTH_HACK,
        _ => asset_iw4::CAMERA_REGION_NONE,
    }
}

pub fn bind_native_materials(
    path: &std::path::Path,
    captures: &std::collections::BTreeMap<String, T6MaterialCapture>,
    techsets: &std::collections::BTreeMap<String, crate::material::t6_techset::T6TechniqueSet>,
    materials: &mut MaterialCatalog,
    refusals: &mut Vec<String>,
) -> std::collections::BTreeMap<String, usize> {
    let mut bound = std::collections::BTreeMap::new();
    let mut linked_techsets = std::collections::BTreeSet::new();
    for (name, capture) in captures {
        let result = (|| {
            let native = capture
                .native
                .as_ref()
                .ok_or("native technique capture missing")?;
            let set = techsets
                .get(&native.technique_set)
                .ok_or("native technique set missing")?;
            let draw = if native
                .state
                .first_bits(crate::material::t6_techset::T6_TECHNIQUE_LIT)
                .is_none()
                && native
                    .state
                    .first_bits(crate::material::t6_techset::T6_TECHNIQUE_EMISSIVE)
                    .is_some()
            {
                crate::material::t6_techset::T6Draw::Emissive
            } else {
                crate::material::t6_techset::T6Draw::Lit
            };
            let mut report = Vec::new();
            if linked_techsets.insert(draw.technique_set_name(&set.name)) {
                materials.link_t6_technique_set(set, draw, &mut report);
            }
            let definition =
                native_material_definition(path, name, &native.header, &set.name, false)?;
            let seed = materials.link_material(definition);
            T6MaterialBindings {
                textures: &native.textures,
                constants: native.constants.clone(),
                state: &native.state,
            }
            .link(materials, seed, name, set, draw, &mut report)
            .map_err(|e| format!("{e:?}"))
        })();
        match result {
            Ok(index) => {
                bound.insert(name.clone(), index);
            }
            Err(reason) => refusals.push(format!("T6 material {name}: {reason}")),
        }
    }
    bound
}
