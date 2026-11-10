use asset_material::MaterialWorldBindingInputs;
use asset_model::LightGridCodec;

pub(super) fn prepare(
    source_namespace: Option<asset_core::FamilyId>,
    codec: &LightGridCodec,
    exposure_stops: Option<f32>,
    sky_intensity: Option<[f32; 4]>,
) -> MaterialWorldBindingInputs {
    MaterialWorldBindingInputs {
        source_namespace,
        exposure_stops,
        model_lighting_decode_scale: asset_model::model_lighting_texel_decode_scale(codec),
        reflection_probe_alpha_weight: codec.reflection_probe_alpha_weight,
        sky_intensity,
    }
}

/// An atlas layer's inputs, made as its map's own would be were it the match's.
pub(super) fn layer(
    id: asset_core::atlas::WorldId,
    lighting: &asset_world::AtlasLayerLighting,
) -> asset_material::MaterialLayerBindingInputs {
    let sun = lighting
        .t5_sun_light
        .map(|sun| asset_material::MaterialSunInputs {
            direction: sun.direction,
            color: [sun.diffuse[0], sun.diffuse[1], sun.diffuse[2]],
            diffuse_color: Some(sun.diffuse),
            specular_color: Some(sun.specular),
        })
        .or_else(|| {
            lighting
                .sun_primary_light
                .as_ref()
                .map(|light| self::sun(&super::world::map_dir_primary_light(light)))
        });
    asset_material::MaterialLayerBindingInputs {
        id,
        world: prepare(
            lighting.source_namespace,
            lighting
                .light_grid_codec
                .unwrap_or(&asset_model::RGB8_LIGHT_GRID_CODEC),
            lighting.t6_exposure,
            lighting.sky_dynamic_intensity,
        ),
        exposure: lighting.t5_sun_parse_exposure,
        sun,
    }
}

pub(crate) fn fog(fog: &asset_world::ExpFog) -> asset_material::MaterialFogInputs {
    asset_material::MaterialFogInputs {
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        halfway_dist: fog.halfway_dist,
        start_dist: fog.start_dist,
        volumetric: fog
            .volumetric
            .map(|v| asset_material::MaterialFogVolumeInputs {
                halfway_height: v.halfway_height,
                base_height: v.base_height,
                color_scale: v.color_scale,
            }),
        sun: fog.sun.map(|s| asset_material::MaterialFogSunInputs {
            color_rgb: s.color_rgb,
            sun_dir: s.sun_dir,
            begin_angle_deg: s.begin_angle_deg,
            end_angle_deg: s.end_angle_deg,
        }),
    }
}

pub(crate) fn sun(light: &super::world::MapDirPrimaryLight) -> asset_material::MaterialSunInputs {
    asset_material::MaterialSunInputs {
        direction: light.direction,
        color: light.color,
        diffuse_color: light.t5_diffuse_color,
        specular_color: light.t5_specular_color,
    }
}
