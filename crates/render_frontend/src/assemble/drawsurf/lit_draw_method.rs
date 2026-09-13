use super::setup::TechType;

#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct MapPrimaryLightTypes {
    pub types: Vec<u8>,
}

#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawMethodDfog(pub bool);

#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SunShadowMapPresent(pub bool);

#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct SpotShadowMapLights(pub Vec<u8>);

pub fn with_scene_light_index(packed: u64, scene_light_index: u8) -> u64 {
    let mut fields = dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed });
    fields.scene_light_index = scene_light_index;
    dpvs_iw4::pack(fields).packed
}

pub fn colour_lit_technique(
    base: TechType,
    packed_drawsurf: u64,
    light_types: &[u8],
    dfog: bool,
    sun_shadow_map: bool,
    spot_shadowed: &[u8],
) -> TechType {
    if base.0 != lighting_iw4::GFX_DRAW_METHOD_LIT_BEGIN {
        return base;
    }
    let fields = dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf {
        packed: packed_drawsurf,
    });
    let gfx_light_type = light_types
        .get(usize::from(fields.scene_light_index))
        .copied()
        .unwrap_or(0);

    let gfx_light_type = if force_dir_light() {
        lighting_iw4::GFX_LIGHT_TYPE_DIR
    } else {
        gfx_light_type
    };
    let has_shadow_map = match gfx_light_type {
        lighting_iw4::GFX_LIGHT_TYPE_DIR => sun_shadow_map,
        lighting_iw4::GFX_LIGHT_TYPE_SPOT => spot_shadowed
            .iter()
            .any(|&light| light == fields.scene_light_index),
        _ => false,
    };
    TechType(lighting_iw4::lit_tech_type(
        base.0,
        fields.surf_type,
        gfx_light_type,
        dfog,
        has_shadow_map,
    ))
}

fn force_dir_light() -> bool {
    static FORCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCE.get_or_init(|| std::env::var_os("IW4L_LIT_FORCE_DIR").is_some())
}
