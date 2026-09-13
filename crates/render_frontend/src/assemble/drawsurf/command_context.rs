use bevy::math::Mat4;
use bevy::prelude::*;
pub use render_frame::code_math::{
    camera_relative_view_projection, code_transpose_matrix_row4, code_transpose_matrix_rows,
    float4_bits,
};

use super::frame_products::MaterialFrameInputs;
use super::material_runtime::{RuntimeCodeSources, RuntimeImageId};
use crate::prepare::scene::camera::FpvLens;
use crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas;
use crate::prepare::scene::view_parms::PreparedSceneView;
use hud_iw4::{GfxCmdBufSource2d, gfx_scene_def_float_time, r_begin_view};
use net::CgFrameClock;

pub use crate::prepare::scene::view_parms::{host_clip_from_view, pack_live_view_parms};

pub const FIRST_CODE_MATRIX: u16 = 0x4c;

pub const CODE_LIGHT_POSITION: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_POSITION;

pub const CODE_LIGHT_DIFFUSE: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_DIFFUSE;

pub const CODE_LIGHT_SPECULAR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPECULAR;

pub const CODE_LIGHT_SPOTDIR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTDIR;

pub const CODE_LIGHT_SPOTFACTORS: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTFACTORS;

pub const CODE_LIGHT_FALLOFF_PLACEMENT: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT;

pub const CODE_GAMETIME: u16 = 0x07;

const GAMETIME_TWO_PI: f64 = f64::from_bits(0x4019_21FB_6000_0000);

const GAMETIME_WRAP: f64 = f64::from_bits(0x40E5_1800_0000_0000);

pub const CODE_FOG: u16 = 0x25;

pub const CODE_FOG_COLOR_LINEAR: u16 = 0x26;

pub const CODE_FOG_COLOR_GAMMA: u16 = 0x27;

pub const CODE_FOG_SUN_CONSTS: u16 = 0x28;

pub const CODE_FOG_SUN_COLOR_LINEAR: u16 = 0x29;

pub const CODE_FOG_SUN_COLOR_GAMMA: u16 = 0x2a;

pub const CODE_FOG_SUN_DIR: u16 = 0x2b;

pub const CODE_RENDER_TARGET_SIZE: u16 = 0x16;

pub const CODE_CLIP_SPACE_LOOKUP_SCALE: u16 = 0x3f;

pub const CODE_CLIP_SPACE_LOOKUP_OFFSET: u16 = 0x40;

const VIEWPORT_ONE: f32 = 1.0;

const VIEWPORT_HALF: f32 = 0.5;

pub const CODE_LIGHTING_LOOKUP_SCALE: u16 = 0x22;

pub const CODE_BASE_LIGHTING_COORDS: u16 = 0x3a;

pub const CODE_LIGHT_PROBE_AMBIENT: u16 = 0x3b;

pub const CODE_MESH_ARG_0: u16 = 0x4a;

pub const CODE_MESH_ARG_1: u16 = 0x4b;

pub const CODE_LEFTOVER_IW5_EYEOFFSET: u16 =
    assets::LEFTOVER_IW5_CODE_BASE + assets::IW5_CODE_EYEOFFSET;

pub const CODE_LEFTOVER_IW5_SAT_R: u16 =
    assets::LEFTOVER_IW5_CODE_BASE + assets::IW5_CODE_COLOR_SATURATION_R;

pub const CODE_LEFTOVER_IW5_SAT_G: u16 =
    assets::LEFTOVER_IW5_CODE_BASE + assets::IW5_CODE_COLOR_SATURATION_G;

pub const CODE_LEFTOVER_IW5_SAT_B: u16 =
    assets::LEFTOVER_IW5_CODE_BASE + assets::IW5_CODE_COLOR_SATURATION_B;

pub const CODE_LEFTOVER_T5_VPOSX: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_VPOSX_TO_WORLD;

pub const CODE_LEFTOVER_T5_VPOSY: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_VPOSY_TO_WORLD;

pub const CODE_LEFTOVER_T5_VPOS1: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_VPOS1_TO_WORLD;

pub const CODE_LEFTOVER_T5_EYEOFFSET: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_EYEOFFSET;

pub const CODE_LEFTOVER_T5_LIGHT_ATTENUATION: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_ATTENUATION;

pub const CODE_LEFTOVER_T5_LIGHT_FALLOFF_A: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_FALLOFF_A;

pub const CODE_LEFTOVER_T5_LIGHT_FALLOFF_B: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_FALLOFF_B;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_MATRIX0;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_MATRIX1;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_MATRIX2;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_MATRIX3;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_AABB: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_AABB;

pub const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_CONE_CONTROL1;

pub const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_CONE_CONTROL2;

pub const CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_SPOT_COOKIE_SLIDE;

pub const CODE_LEFTOVER_T5_SUN_POSITION: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_SUN_POSITION;

pub const CODE_LEFTOVER_T5_SUN_DIFFUSE: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_SUN_DIFFUSE;

pub const CODE_LEFTOVER_T5_SUN_SPECULAR: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_SUN_SPECULAR;

pub const CODE_LEFTOVER_T5_HDRCONTROL_0: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_HDRCONTROL_0;

pub const CODE_LEFTOVER_T5_HDRCONTROL_1: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_HDRCONTROL_1;

pub const CODE_LEFTOVER_T5_LIGHT_HERO_SCALE: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_LIGHT_HERO_SCALE;

pub const CODE_LEFTOVER_T5_HERO_LIGHTING_R: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_HERO_LIGHTING_R;

pub const CODE_LEFTOVER_T5_HERO_LIGHTING_G: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_HERO_LIGHTING_G;

pub const CODE_LEFTOVER_T5_HERO_LIGHTING_B: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_HERO_LIGHTING_B;

pub const CODE_LEFTOVER_T5_GENERIC_PARAM4: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_GENERIC_PARAM4;

pub const CODE_LEFTOVER_T5_GENERIC_PARAM5: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_GENERIC_PARAM5;

pub const CODE_LEFTOVER_T5_GENERIC_PARAM6: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_GENERIC_PARAM6;

pub const CODE_LEFTOVER_T5_WIND_DIRECTION: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_WIND_DIRECTION;

pub const CODE_LEFTOVER_T5_GRASS_WIND_FORCE0: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_GRASS_WIND_FORCE0;

pub const CODE_LEFTOVER_T5_VARIANT_WIND_SPRING_0: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_VARIANT_WIND_SPRING_0;

pub const CODE_LEFTOVER_T5_TREECANOPY_PARMS: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_TREECANOPY_PARMS;

pub const CODE_LEFTOVER_T5_CUSTOMWIND_CENTER: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_CUSTOMWIND_CENTER;

pub const CODE_LEFTOVER_T5_CUSTOMWIND_SPRING: u16 =
    assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_CUSTOMWIND_SPRING;

pub const T5_HDRCONTROL_EXPOSURE_DIVISOR: f32 = 8.0;

pub const T5_HDRCONTROL_HOST_EXPOSURE: f32 = 1.0;

pub const T5_LIGHT_ATTENUATION_EPSILON: f32 = 0.000015287891;

pub const T5_LIGHT_ATTENUATION_DEFAULT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

pub const T5_LIGHT_FALLOFF_NEAR: f32 = 0.0;

pub const T5_LIGHT_AABB_DEFAULT: [f32; 4] = [0.75, 1.0, 0.75, 1.0];

pub const T5_LIGHT_SPOT_ROLL_DEFAULT: f32 = 0.0;

pub const T5_LIGHT_COOKIE_DEFAULT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

pub const R_FILM_TWEAK_SATURATION_DEFAULT: f32 = 1.0;

pub const CODE_TRANSPOSE_VIEW_PROJECTION: u16 = 0x56;

pub const CODE_TRANSPOSE_WORLD0: u16 = 0x62;

pub const CODE_TRANSPOSE_WORLD1: u16 = 0x6e;

pub const CODE_TRANSPOSE_WORLD2: u16 = 0x7a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0: u16 = 0x6a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1: u16 = 0x76;
pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2: u16 = 0x82;

pub const CODE_TRANSPOSE_WORLD_VIEW0: u16 = 0x66;
pub const CODE_TRANSPOSE_WORLD_VIEW1: u16 = 0x72;
pub const CODE_TRANSPOSE_WORLD_VIEW2: u16 = 0x7e;

pub const CODE_INVERSE_TRANSPOSE_WORLD_VIEW0: u16 = 0x67;

pub const CODE_MATERIAL_COLOR: u16 = 0x24;

pub const CODE_TRANSPOSE_PROJECTION: u16 = 0x52;

pub const CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP: u16 = 0x5e;

pub const CODE_TEXTURE_OUTDOOR: u32 = 0x0e;

pub const CODE_TEXTURE_OUTDOOR_SAMPLER: u8 = 0x62;

pub const CODE_DEPTH_FROM_CLIP: u16 = 0x49;

pub const R_ZNEAR_DEPTHHACK: f32 = hud_iw4::R_ZNEAR_DEPTHHACK_DEFAULT;

pub const CODE_TEXTURE_MODEL_LIGHTING: u32 = lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING as u32;

pub const CODE_TEXTURE_MODEL_LIGHTING_SAMPLER: u8 = 0xe2;

pub const CODE_TEXTURE_LIGHT_ATTENUATION: u32 =
    lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION as u32;

pub const CODE_TEXTURE_FLOATZ: u32 = 0x0f;

pub const CODE_TEXTURE_RESOLVED_POST_SUN: u32 = 9;

pub const CODE_TEXTURE_FLOATZ_SAMPLER: u8 = 0x61;

pub use super::fog::MapFrameFog;

pub use crate::prepare::scene::world::{
    LightAttenuationBind, MapDirPrimaryLight, T5LightFalloffPack,
};

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapT5SunParseExposure {
    pub exposure: f32,
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapT5TreeScatter {
    pub intensity: f32,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapOutdoor {
    pub image: Option<RuntimeImageId>,
    pub lookup: [u32; 16],
}

#[derive(Clone, Debug, Default, Resource)]
pub struct MapPrimaryLights {
    pub lights: Vec<lighting_iw4::GfxLightPack>,

    pub attenuation: Vec<LightAttenuationBind>,

    pub t5_falloff: Vec<T5LightFalloffPack>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandContextRefusal {
    MissingViewProjection,
    MissingFrameFog,
    PrimaryLightNotDir,
    ViewportHasZeroRenderTarget,
}

pub fn default_world_transpose_rows(view_origin: Vec3) -> Vec<[u32; 4]> {
    code_transpose_matrix_rows(Mat4::from_translation(-view_origin))
}

#[must_use]
pub fn world_transpose_rows_from_set3d(world: [f32; 16]) -> Vec<[u32; 4]> {
    code_transpose_matrix_rows(Mat4::from_cols_array(&world))
}

pub fn fog_color_linear_and_gamma(rgb: [f32; 3], alpha: f32) -> ([f32; 4], [f32; 4]) {
    let pack = |c: f32| -> u8 { (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };

    let bytes = [pack(rgb[2]), pack(rgb[1]), pack(rgb[0]), pack(alpha)];
    const INV_255: f32 = 0.003_921_568_9;
    let gamma = [
        f32::from(bytes[2]) * INV_255,
        f32::from(bytes[1]) * INV_255,
        f32::from(bytes[0]) * INV_255,
        f32::from(bytes[3]) * INV_255,
    ];
    let linear = [
        lighting_iw4::color_srgb_to_linear(gamma[0]),
        lighting_iw4::color_srgb_to_linear(gamma[1]),
        lighting_iw4::color_srgb_to_linear(gamma[2]),
        gamma[3],
    ];
    (linear, gamma)
}

pub fn produce_frame_fog(
    sources: &mut RuntimeCodeSources,
    fog: &assets::ExpFog,
    r_fog_enabled: bool,
) -> Result<(), CommandContextRefusal> {
    let density = fog.density();
    let fog_row = [
        0.0,
        1.0 - fog.max_opacity,
        -density,
        fog.start_dist * density,
    ];
    sources.set_constant_rows(CODE_FOG, &[float4_bits(fog_row)]);

    let (linear, gamma) = fog_color_linear_and_gamma(fog.color_rgb, 1.0);
    sources.set_constant_rows(CODE_FOG_COLOR_LINEAR, &[float4_bits(linear)]);
    sources.set_constant_rows(CODE_FOG_COLOR_GAMMA, &[float4_bits(gamma)]);

    match fog.sun.as_ref() {
        Some(sun) => produce_iw4_sun_fog(sources, fog.density(), sun),
        None => {}
    }
    if !r_fog_enabled {
        sources.set_constant_rows(CODE_FOG, &[float4_bits([0.0, 1.0, 0.0, 0.0])]);
    }
    Ok(())
}

fn produce_iw4_sun_fog(sources: &mut RuntimeCodeSources, density: f32, sun: &assets::SunFog) {
    let begin = (sun.begin_angle_deg.to_radians()).cos();
    let end = (sun.end_angle_deg.to_radians()).cos();
    let mut slope = 100.0;
    if end < begin {
        slope = 1.0 / (begin - end);
    }
    let sun_consts = [-density * sun.scale, end, slope, -density];
    sources.set_constant_rows(CODE_FOG_SUN_CONSTS, &[float4_bits(sun_consts)]);

    let (sun_linear, sun_gamma) = fog_color_linear_and_gamma(sun.color_rgb, 1.0);
    sources.set_constant_rows(CODE_FOG_SUN_COLOR_LINEAR, &[float4_bits(sun_linear)]);
    sources.set_constant_rows(CODE_FOG_SUN_COLOR_GAMMA, &[float4_bits(sun_gamma)]);

    let length = math_iw4::vec3_length(sun.sun_dir);
    let dir = if length > 0.0 {
        sun.sun_dir.map(|v| v / length)
    } else {
        sun.sun_dir
    };
    sources.set_constant_rows(
        CODE_FOG_SUN_DIR,
        &[float4_bits([dir[0], dir[1], dir[2], 0.0])],
    );
}

pub fn produce_update_viewport(
    sources: &mut RuntimeCodeSources,
    rt_width: i32,
    rt_height: i32,
    viewport: GfxViewport,
) -> Result<(), CommandContextRefusal> {
    if rt_width <= 0 || rt_height <= 0 {
        return Err(CommandContextRefusal::ViewportHasZeroRenderTarget);
    }
    let inv_w = VIEWPORT_ONE / rt_width as f32;
    let inv_h = VIEWPORT_ONE / rt_height as f32;
    let scale_x = inv_w * viewport.width as f32 * VIEWPORT_HALF;
    let scale_y = inv_h * viewport.height as f32 * VIEWPORT_HALF;
    sources.set_constant_rows(
        CODE_RENDER_TARGET_SIZE,
        &[float4_bits([
            rt_width as f32,
            rt_height as f32,
            inv_w,
            inv_h,
        ])],
    );
    sources.set_constant_rows(
        CODE_CLIP_SPACE_LOOKUP_SCALE,
        &[float4_bits([scale_x, -scale_y, 0.0, 1.0])],
    );
    sources.set_constant_rows(
        CODE_CLIP_SPACE_LOOKUP_OFFSET,
        &[float4_bits([
            VIEWPORT_HALF * inv_w + viewport.x as f32 * inv_w + scale_x,
            VIEWPORT_HALF * inv_h + scale_y + viewport.y as f32 * inv_h,
            0.0,
            0.0,
        ])],
    );
    Ok(())
}

pub fn produce_lighting_lookup_scale(sources: &mut RuntimeCodeSources, inv_image_height: f32) {
    let scale = lighting_iw4::model_lighting_lookup_scale(inv_image_height);
    sources.set_constant_rows(CODE_LIGHTING_LOOKUP_SCALE, &[float4_bits(scale.as_array())]);
}

pub fn produce_model_lighting_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        CODE_TEXTURE_MODEL_LIGHTING,
        CODE_TEXTURE_MODEL_LIGHTING_SAMPLER,
    )
}

pub fn produce_floatz_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(CODE_TEXTURE_FLOATZ, CODE_TEXTURE_FLOATZ_SAMPLER)
}

pub fn produce_sun_shadow_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        super::CODE_TEXTURE_SHADOWMAP_SUN,
        super::SunShadowSamplingPath::Fallback.code_image_sampler(),
    )
}

pub fn produce_spot_shadow_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        super::CODE_TEXTURE_SHADOWMAP_SPOT,
        super::SunShadowSamplingPath::Fallback.code_image_sampler(),
    )
}

pub fn produce_sun_shadow_receiver_constants(
    sources: &mut RuntimeCodeSources,
    frame: super::SunShadowForcedFrame,
) {
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_SWITCH_PARTITION,
        &[float4_bits(frame.receiver.switch_partition)],
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_SCALE,
        &[float4_bits(frame.receiver.shadowmap_scale)],
    );
    sources.set_constant_rows(
        super::CODE_SUN_SHADOWMAP_PIXEL_ADJUST,
        &[float4_bits(frame.pixel_adjust)],
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_LOOKUP_TRANSPOSE,
        &code_transpose_matrix_row4(frame.lookup.transpose()),
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_POLYGON_OFFSET,
        &[float4_bits(frame.partitions[0].polygon_offset)],
    );
}

pub fn produce_world_view_family(sources: &mut RuntimeCodeSources, world_view: Mat4) {
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW0,
        &code_transpose_matrix_row4(world_view),
    );
    sources.set_constant_rows(
        CODE_INVERSE_TRANSPOSE_WORLD_VIEW0,
        &code_transpose_matrix_row4(world_view.inverse()),
    );
}

pub fn produce_material_color_cmdbuf_init(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_MATERIAL_COLOR,
        &[float4_bits([f32::MAX, f32::MAX, f32::MAX, 0.0])],
    );
}

pub fn produce_top_pair_command_context(
    sources: &mut RuntimeCodeSources,
    clip_from_world: Mat4,
    view_from_world: Mat4,
    view_origin: Vec3,
    world0: Vec<[u32; 4]>,
    fog: Option<&assets::ExpFog>,
    fog_enabled: bool,
) -> Result<(), CommandContextRefusal> {
    sources.set_constant_rows(
        CODE_TRANSPOSE_VIEW_PROJECTION,
        &code_transpose_matrix_row4(camera_relative_view_projection(
            clip_from_world,
            view_origin,
        )),
    );
    sources.set_constant(CODE_TRANSPOSE_WORLD0, world0);
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        &code_transpose_matrix_row4(clip_from_world),
    );
    produce_world_view_family(sources, view_from_world);
    produce_material_color_cmdbuf_init(sources);
    produce_depth_from_clip(sources, false);
    produce_leftover_iw5_code_consts(sources, view_origin);
    let Some(frame_fog) = fog else {
        return Err(CommandContextRefusal::MissingFrameFog);
    };
    super::t5_fog::produce(sources, frame_fog, view_origin.z, fog_enabled);
    produce_frame_fog(sources, frame_fog, fog_enabled)
}

pub fn produce_dir_primary_light(
    sources: &mut RuntimeCodeSources,
    light: &MapDirPrimaryLight,
) -> Result<(), CommandContextRefusal> {
    if light.light_type != lighting_iw4::GFX_LIGHT_TYPE_DIR {
        return Err(CommandContextRefusal::PrimaryLightNotDir);
    }
    let position = lighting_iw4::dir_light_position(light.direction);
    let diffuse = light
        .t5_diffuse_color
        .map(|v| [v[0], v[1], v[2], 1.0])
        .unwrap_or_else(|| lighting_iw4::light_diffuse(light.color, light.diffuse_color_scale));
    let specular = light
        .t5_specular_color
        .map(|v| [v[0], v[1], v[2], 1.0])
        .unwrap_or_else(|| lighting_iw4::light_specular(light.color, light.specular_color_scale));
    sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
    sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
    sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
    produce_leftover_t5_sun_constants(sources, light);
    Ok(())
}

fn produce_leftover_t5_sun_constants(sources: &mut RuntimeCodeSources, light: &MapDirPrimaryLight) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_POSITION,
        &[float4_bits(lighting_iw4::dir_light_position(
            light.direction,
        ))],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_DIFFUSE,
        &[float4_bits(leftover_t5_sun_color(
            light.t5_diffuse_color,
            light.color,
        ))],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_SPECULAR,
        &[float4_bits(leftover_t5_sun_color(
            light.t5_specular_color,
            light.color,
        ))],
    );
}

fn leftover_t5_sun_color(t5: Option<[f32; 4]>, color: [f32; 3]) -> [f32; 4] {
    match t5 {
        Some(v) => [v[0], v[1], v[2], 1.0],
        None => [color[0], color[1], color[2], 1.0],
    }
}

pub fn produce_leftover_t5_hdrcontrol(sources: &mut RuntimeCodeSources, exposure: f32) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HDRCONTROL_0,
        &[float4_bits([
            exposure / T5_HDRCONTROL_EXPOSURE_DIVISOR,
            0.0,
            0.0,
            0.0,
        ])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HDRCONTROL_1,
        &[float4_bits([1.0, 0.0, 0.0, 0.0])],
    );
}

fn produce_t5_sky_constants(sources: &mut RuntimeCodeSources, authored: [f32; 4], forward_z: f32) {
    for (index, row) in [
        (18, [1.0, 0.0, 0.0, 0.0]),
        (19, [0.0, 1.0, 0.0, 0.0]),
        (20, [0.0, 0.0, 1.0, 0.0]),
    ] {
        sources.set_constant_rows(index, &[float4_bits(row)]);
    }
    sources.set_constant_rows(
        assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_SKY_TRANSITION,
        &[float4_bits([0.0; 4])],
    );
    let intensity = t5_sky_intensity(authored, forward_z);
    sources.set_constant_rows(
        assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_SKY_COLOR_MULTIPLIER,
        &[float4_bits([intensity; 4])],
    );
}

fn t5_sky_intensity([angle0, angle1, factor0, factor1]: [f32; 4], forward_z: f32) -> f32 {
    let radians = f32::from_bits(0x3c8efa35);
    let cos0 = (((90.0 - angle0) * radians) as f64).cos() as f32;
    let cos1 = (((90.0 - angle1) * radians) as f64).cos() as f32;
    let delta = cos1 - cos0;
    let blend = if delta.abs() <= f32::from_bits(0x38d1b717) {
        0.0
    } else {
        let t = ((forward_z - cos0) / delta).clamp(0.0, 1.0);
        t * t
    };
    (1.0 - blend) * factor0 + blend * factor1
}

fn produce_leftover_t5_initial_water_waves(sources: &mut RuntimeCodeSources, time: f32) {
    let wave_number = f32::from_bits(0x40c9_0fdb);
    let gravity = f32::from_bits(0x43c1_1c29);
    let phase = ((wave_number * gravity) as f64).sqrt() * f64::from(time);
    let rows = [
        [wave_number, 0.0, 1.0, 0.0],
        [wave_number, 0.0, 1.0, 0.0],
        [wave_number, 0.0, 1.0, 0.0],
        [wave_number, 0.0, 1.0, 0.0],
        [phase as f32; 4],
        [0.0; 4],
        [0.0; 4],
    ];
    for (row, values) in rows.into_iter().enumerate() {
        sources.set_constant_rows(
            assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_POSTFX_CONTROL0 + row as u16,
            &[float4_bits(values)],
        );
    }
}

pub fn produce_leftover_t5_light_hero_scale(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_HERO_SCALE,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}

pub fn produce_leftover_t5_hero_lighting_matrix(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_R,
        &[float4_bits([1.0, 0.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_G,
        &[float4_bits([0.0, 1.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_B,
        &[float4_bits([0.0, 0.0, 1.0, 0.0])],
    );
}

pub fn produce_leftover_t5_generic_param4(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM4,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}

pub fn produce_leftover_t5_generic_param5(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM5,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}

pub fn produce_leftover_t5_generic_param6(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM6,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}

pub fn produce_leftover_t5_wind_shader_constants(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_WIND_DIRECTION,
        &[float4_bits([1.0, 0.0, 0.0, 0.0])],
    );
    for index in 0u16..16 {
        sources.set_constant_rows(
            CODE_LEFTOVER_T5_VARIANT_WIND_SPRING_0 + index,
            &[float4_bits([0.0, 0.0, 0.0, 0.0])],
        );
    }
}

pub fn produce_leftover_t5_custom_wind_constants(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_CUSTOMWIND_CENTER,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_CUSTOMWIND_SPRING,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
}

pub fn produce_leftover_t5_grass_wind_force0(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GRASS_WIND_FORCE0,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
}

pub fn produce_leftover_t5_treecanopy_parms(
    sources: &mut RuntimeCodeSources,
    intensity: f32,
    amount: f32,
) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_TREECANOPY_PARMS,
        &[float4_bits([intensity, amount, 0.0, 0.0])],
    );
}

pub fn produce_depth_from_clip(sources: &mut RuntimeCodeSources, viewmodel: bool) {
    let w = if viewmodel { -1.0 } else { 1.0 };
    sources.set_constant_rows(CODE_DEPTH_FROM_CLIP, &[float4_bits([0.0, 0.0, 0.0, w])]);
}

pub fn produce_game_time(sources: &mut RuntimeCodeSources, game_time: f32) {
    let frac = game_time - game_time.floor();
    let angle = (f64::from(frac) * GAMETIME_TWO_PI) as f32;
    let (sin, cos) = angle.sin_cos();
    let wrapped = (f64::from(game_time) % GAMETIME_WRAP) as f32;
    sources.set_constant_rows(CODE_GAMETIME, &[float4_bits([sin, cos, frac, wrapped])]);
}

pub fn color_saturation_matrix(saturation: f32) -> [[f32; 4]; 3] {
    let r = (1.0 - saturation) * 0.25;
    let g = (1.0 - saturation) * 0.5;
    [
        [r + saturation, r, r, 0.0],
        [g, g + saturation, g, 0.0],
        [r, r, r + saturation, 0.0],
    ]
}

pub fn produce_leftover_iw5_code_consts(sources: &mut RuntimeCodeSources, view_origin: Vec3) {
    sources.set_constant_rows(
        CODE_LEFTOVER_IW5_EYEOFFSET,
        &[float4_bits([
            view_origin.x,
            view_origin.y,
            view_origin.z,
            1.0,
        ])],
    );
    let rows = color_saturation_matrix(R_FILM_TWEAK_SATURATION_DEFAULT);
    sources.set_constant_rows(CODE_LEFTOVER_IW5_SAT_R, &[float4_bits(rows[0])]);
    sources.set_constant_rows(CODE_LEFTOVER_IW5_SAT_G, &[float4_bits(rows[1])]);
    sources.set_constant_rows(CODE_LEFTOVER_IW5_SAT_B, &[float4_bits(rows[2])]);
}

pub fn produce_leftover_t5_code_consts(
    sources: &mut RuntimeCodeSources,
    view_origin: Vec3,
    clip_from_view: Mat4,
    world_from_view: Mat4,
    rt_width: i32,
    rt_height: i32,
) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_EYEOFFSET,
        &[float4_bits([
            view_origin.x,
            view_origin.y,
            view_origin.z,
            1.0,
        ])],
    );
    if rt_width <= 0 || rt_height <= 0 {
        return;
    }
    let inv_w = VIEWPORT_ONE / rt_width as f32;
    let inv_h = VIEWPORT_ONE / rt_height as f32;
    let p00 = clip_from_view.x_axis.x;
    let p11 = clip_from_view.y_axis.y;
    if p00.abs() < 1e-12 || p11.abs() < 1e-12 {
        return;
    }
    let scale_x = (-2.0 * inv_w) / p00;
    let scale_y = (2.0 * inv_h) / p11;

    let vposx = world_from_view.x_axis * scale_x;
    let vposy = world_from_view.y_axis * scale_y;
    let vpos1 = world_from_view * Vec4::new(1.0 / p00, -1.0 / p11, 1.0, 0.0);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOSX, &[float4_bits(vposx.to_array())]);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOSY, &[float4_bits(vposy.to_array())]);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOS1, &[float4_bits(vpos1.to_array())]);
}

pub(crate) fn update_command_context_code_sources(
    mut runtime: ResMut<MaterialFrameInputs>,
    fog: Option<Res<MapFrameFog>>,
    fog_dvars: Res<super::fog::FogDvars>,
    mut dfog: ResMut<super::DrawMethodDfog>,
    dir_light: Option<Res<MapDirPrimaryLight>>,
    t5_exposure: Option<Res<MapT5SunParseExposure>>,
    t5_tree_scatter: Option<Res<MapT5TreeScatter>>,
    outdoor: Option<Res<MapOutdoor>>,
    lighting: Option<Res<WorldModelLightingAtlas>>,
    cg_clock: Option<Res<CgFrameClock>>,
    prepared: Res<PreparedSceneView>,
    cameras: Query<&Camera, With<FpvLens>>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
) {
    if !prepared.ready {
        return;
    }
    let view = prepared.view_from_world;
    let clip_from_view = prepared.clip_from_view;
    let clip_from_world = prepared.clip_from_world;
    let viewmodel_clip_from_world =
        host_clip_from_view(prepared.fov, prepared.aspect, prepared.depth_hack_near) * view;
    let eye = prepared.eye;
    let mat_frame = &mut *runtime;
    mat_frame.view_origin = eye;
    mat_frame.clip_from_world = Some(clip_from_world);
    mat_frame.view_from_world = Some(view);
    mat_frame.clip_from_view = Some(clip_from_view);
    mat_frame.viewmodel_clip_from_world = Some(viewmodel_clip_from_world);
    mat_frame.viewmodel_near = Some(prepared.depth_hack_near);
    mat_frame.outdoor = outdoor.as_deref().copied();

    mat_frame.code_sources = RuntimeCodeSources::default();
    mat_frame.sun_shadow = None;
    let float_time_ms = cg_clock.map(|c| c.time()).unwrap_or(0);
    mat_frame.float_time = float_time_ms as f32 / 1000.0;
    let float_time = mat_frame.float_time;
    let sampled_fog = fog.as_ref().map(|fog| fog.sample(float_time_ms));

    dfog.0 = sampled_fog.as_ref().is_some_and(|fog| fog.sun.is_some());

    let mut source = GfxCmdBufSource2d::default();
    source.render_target_width = prepared.rt_w;
    source.render_target_height = prepared.rt_h;
    source.scene_viewport = prepared.scene_viewport;
    let full_vp = hud_iw4::GfxViewport {
        x: 0,
        y: 0,
        width: prepared.rt_w,
        height: prepared.rt_h,
    };
    source.viewport_select =
        if prepared.rt_w <= 0 || prepared.rt_h <= 0 || prepared.scene_viewport == full_vp {
            hud_iw4::GFX_VIEWPORT_FULL
        } else {
            0
        };
    let begun = r_begin_view(
        &mut source,
        &gfx_scene_def_float_time(float_time),
        &prepared.parms,
    );
    let world0 = world_transpose_rows_from_set3d(begun.set_3d.world);
    match produce_top_pair_command_context(
        &mut mat_frame.code_sources,
        clip_from_world,
        view,
        eye,
        world0,
        sampled_fog.as_ref(),
        fog_dvars.enabled,
    ) {
        Ok(()) => {}
        Err(CommandContextRefusal::MissingFrameFog) => {}
        Err(CommandContextRefusal::MissingViewProjection) => {}
        Err(CommandContextRefusal::PrimaryLightNotDir) => {}
        Err(CommandContextRefusal::ViewportHasZeroRenderTarget) => {}
    }
    if let Ok(camera) = cameras.single()
        && let (Some(rt), Some(vp)) = (
            camera.physical_target_size(),
            camera.physical_viewport_rect(),
        )
    {
        let size = vp.size();
        if let (Ok(rt_w), Ok(rt_h), Ok(x), Ok(y), Ok(w), Ok(h)) = (
            i32::try_from(rt.x),
            i32::try_from(rt.y),
            i32::try_from(vp.min.x),
            i32::try_from(vp.min.y),
            i32::try_from(size.x),
            i32::try_from(size.y),
        ) {
            let _ = produce_update_viewport(
                &mut mat_frame.code_sources,
                rt_w,
                rt_h,
                GfxViewport {
                    x,
                    y,
                    width: w,
                    height: h,
                },
            );
        }
    }
    if let Some(light) = dir_light.as_deref() {
        let _ = produce_dir_primary_light(&mut mat_frame.code_sources, light);
    }
    let hdr_exposure = t5_exposure
        .as_deref()
        .map(|e| e.exposure)
        .unwrap_or(T5_HDRCONTROL_HOST_EXPOSURE);
    produce_leftover_t5_hdrcontrol(&mut mat_frame.code_sources, hdr_exposure);
    if let Some(authored) = scene.as_ref().and_then(|s| s.t5_sky_dynamic_intensity) {
        let forward_z = view.inverse().transform_vector3(Vec3::NEG_Z).z;
        produce_t5_sky_constants(&mut mat_frame.code_sources, authored, forward_z);
    }
    produce_leftover_t5_light_hero_scale(&mut mat_frame.code_sources);
    produce_leftover_t5_hero_lighting_matrix(&mut mat_frame.code_sources);

    for index in [
        assets::T5_CODE_GENERIC_PARAM0,
        assets::T5_CODE_GENERIC_PARAM1,
    ] {
        mat_frame
            .code_sources
            .set_constant_rows(assets::LEFTOVER_T5_CODE_BASE + index, &[[0; 4]]);
    }

    mat_frame.code_sources.set_constant_rows(
        assets::LEFTOVER_T5_CODE_BASE + assets::T5_CODE_EXTRA_CAM_PARAM,
        &[[0; 4]],
    );
    produce_leftover_t5_initial_water_waves(&mut mat_frame.code_sources, float_time);
    produce_leftover_t5_generic_param4(&mut mat_frame.code_sources);
    produce_leftover_t5_generic_param5(&mut mat_frame.code_sources);
    produce_leftover_t5_generic_param6(&mut mat_frame.code_sources);
    produce_leftover_t5_wind_shader_constants(&mut mat_frame.code_sources);
    produce_leftover_t5_custom_wind_constants(&mut mat_frame.code_sources);
    produce_leftover_t5_grass_wind_force0(&mut mat_frame.code_sources);
    if let Some(scatter) = t5_tree_scatter.as_deref() {
        produce_leftover_t5_treecanopy_parms(
            &mut mat_frame.code_sources,
            scatter.intensity,
            scatter.amount,
        );
    }

    produce_game_time(&mut mat_frame.code_sources, begun.float_time);
    produce_leftover_iw5_code_consts(&mut mat_frame.code_sources, eye);
    produce_leftover_t5_code_consts(
        &mut mat_frame.code_sources,
        eye,
        clip_from_view,
        prepared.view_from_world.inverse(),
        prepared.rt_w,
        prepared.rt_h,
    );
    if let Some(lighting) = lighting.as_deref()
        && let Some(inv_h) =
            lighting_iw4::model_lighting_inv_image_height(lighting.dims.image_height)
    {
        produce_lighting_lookup_scale(&mut mat_frame.code_sources, inv_h);
        let _ = produce_model_lighting_code_texture(&mut mat_frame.code_sources);
    }
    let _ = produce_floatz_code_texture(&mut mat_frame.code_sources);
    let _ = produce_spot_shadow_code_texture(&mut mat_frame.code_sources);
    let _ = mat_frame
        .code_sources
        .set_texture(CODE_TEXTURE_RESOLVED_POST_SUN, 0x62);
    if let Some(light) = dir_light.as_deref() {
        let _ = produce_sun_shadow_code_texture(&mut mat_frame.code_sources);

        if let Some(bounds) = scene.as_deref().and_then(|scene| scene.world_bounds) {
            let world_mid = [bounds[0], bounds[1], bounds[2]];
            let world_half = [bounds[3], bounds[4], bounds[5]];
            let shadow_forward = super::sun_shadow_forward_from_light_dir(light.direction);
            let world = prepared.view_from_world.inverse();
            let forward = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
            let right = world.transform_vector3(Vec3::X).normalize_or_zero();
            let up = world.transform_vector3(Vec3::Y).normalize_or_zero();
            let (tx, ty) = super::tan_half_fov_from_clip(clip_from_view);
            let frame = super::forced_fallback_frame(
                shadow_forward,
                super::SunShadowCamera {
                    origin: eye.to_array(),
                    forward: forward.to_array(),
                    right: right.to_array(),
                    up: up.to_array(),
                    tan_half_fov_x: tx,
                    tan_half_fov_y: ty,
                },
                world_mid,
                world_half,
            );
            produce_sun_shadow_receiver_constants(&mut mat_frame.code_sources, frame);
            mat_frame.sun_shadow = Some(frame);
        }
    }
}
