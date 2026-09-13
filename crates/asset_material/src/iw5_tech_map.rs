use crate::t5_tech_map::{IW4_TECHNIQUE_TYPE_COUNT, IW4_TECHNIQUE_TYPE_NAMES};

pub const IW5_TECHNIQUE_TYPE_COUNT: usize = 54;

pub const IW5_TECHNIQUE_TYPE_NAMES: [&str; IW5_TECHNIQUE_TYPE_COUNT] = [
    "depth prepass",
    "build floatz",
    "build shadowmap depth",
    "build shadowmap color",
    "unlit",
    "emissive",
    "emissive dfog",
    "emissive shadow",
    "emissive shadow dfog",
    "lit",
    "lit dfog",
    "lit sun",
    "lit sun dfog",
    "lit sun shadow",
    "lit sun shadow dfog",
    "lit spot",
    "lit spot dfog",
    "lit spot shadow",
    "lit spot shadow dfog",
    "lit spot shadow cucoloris",
    "lit spot shadow cucoloris dfog",
    "lit omni",
    "lit omni dfog",
    "lit omni shadow",
    "lit omni shadow dfog",
    "lit instanced",
    "lit instanced dfog",
    "lit instanced sun",
    "lit instanced sun dfog",
    "lit instanced sun shadow",
    "lit instanced sun shadow dfog",
    "lit instanced spot",
    "lit instanced spot dfog",
    "lit instanced spot shadow",
    "lit instanced spot shadow dfog",
    "lit instanced spot shadow cucoloris",
    "lit instanced spot shadow cucoloris dfog",
    "lit instanced omni",
    "lit instanced omni dfog",
    "lit instanced omni shadow",
    "lit instanced omni shadow dfog",
    "light spot",
    "light omni",
    "light spot shadow",
    "light spot shadow cucoloris",
    "fakelight normal",
    "fakelight view",
    "sunlight preview",
    "case texture",
    "solid wireframe",
    "shaded wireframe",
    "thermal",
    "debug bumpmap",
    "debug bumpmap instanced",
];

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn iw5_name_match_table() -> [Option<u8>; IW5_TECHNIQUE_TYPE_COUNT] {
    let mut out = [None; IW5_TECHNIQUE_TYPE_COUNT];
    let mut i = 0;
    while i < IW5_TECHNIQUE_TYPE_COUNT {
        let name = IW5_TECHNIQUE_TYPE_NAMES[i].as_bytes();
        let mut j = 0;
        while j < IW4_TECHNIQUE_TYPE_COUNT {
            if bytes_eq(name, IW4_TECHNIQUE_TYPE_NAMES[j].as_bytes()) {
                out[i] = Some(j as u8);
                break;
            }
            j += 1;
        }
        i += 1;
    }
    out
}

pub const IW5_NAME_MATCH_IW4_SLOT: [Option<u8>; IW5_TECHNIQUE_TYPE_COUNT] = iw5_name_match_table();

pub const STATE_BITS_ENTRY_UNUSED: u8 = 0xFF;

pub fn iw5_slot_to_iw4(iw5_slot: usize) -> Option<usize> {
    IW5_NAME_MATCH_IW4_SLOT
        .get(iw5_slot)
        .copied()
        .flatten()
        .map(usize::from)
}

pub fn leftover_iw5_slots() -> impl Iterator<Item = (usize, &'static str)> {
    IW5_NAME_MATCH_IW4_SLOT
        .iter()
        .enumerate()
        .filter_map(|(slot, mapped)| {
            mapped
                .is_none()
                .then_some((slot, IW5_TECHNIQUE_TYPE_NAMES[slot]))
        })
}

#[must_use]
pub fn iw4_slot_to_iw5(iw4_slot: usize) -> Option<usize> {
    let want = u8::try_from(iw4_slot).ok()?;
    IW5_NAME_MATCH_IW4_SLOT
        .iter()
        .position(|mapped| *mapped == Some(want))
}

#[must_use]
pub fn remap_shader_arg_type(iw5_type: u16) -> Option<u16> {
    use asset_iw4::size::mtl_arg as iw4;
    use fastfile_iw5::size::mtl_arg as iw5;
    Some(match iw5_type {
        x if x == iw5::MATERIAL_VERTEX_CONST => iw4::MATERIAL_VERTEX_CONST,
        x if x == iw5::LITERAL_VERTEX_CONST => iw4::LITERAL_VERTEX_CONST,
        x if x == iw5::MATERIAL_VERTEX_SAMPLER => return None,
        x if x == iw5::MATERIAL_PIXEL_SAMPLER => iw4::MATERIAL_PIXEL_SAMPLER,
        x if x == iw5::CODE_VERTEX_CONST => iw4::CODE_VERTEX_CONST,
        x if x == iw5::CODE_PIXEL_SAMPLER => iw4::CODE_PIXEL_SAMPLER,
        x if x == iw5::CODE_PIXEL_CONST => iw4::CODE_PIXEL_CONST,
        x if x == iw5::MATERIAL_PIXEL_CONST => iw4::MATERIAL_PIXEL_CONST,
        x if x == iw5::LITERAL_PIXEL_CONST => iw4::LITERAL_PIXEL_CONST,
        _ => return None,
    })
}

const IW4_CODE_CONST: &[(&str, u16)] = &[
    ("LIGHT_POSITION", 0x0),
    ("LIGHT_DIFFUSE", 0x1),
    ("LIGHT_SPECULAR", 0x2),
    ("LIGHT_SPOTDIR", 0x3),
    ("LIGHT_SPOTFACTORS", 0x4),
    ("LIGHT_FALLOFF_PLACEMENT", 0x5),
    ("PARTICLE_CLOUD_COLOR", 0x6),
    ("GAMETIME", 0x7),
    ("PIXEL_COST_FRACS", 0x8),
    ("PIXEL_COST_DECODE", 0x9),
    ("FILTER_TAP_0", 0xa),
    ("FILTER_TAP_1", 0xb),
    ("FILTER_TAP_2", 0xc),
    ("FILTER_TAP_3", 0xd),
    ("FILTER_TAP_4", 0xe),
    ("FILTER_TAP_5", 0xf),
    ("FILTER_TAP_6", 0x10),
    ("FILTER_TAP_7", 0x11),
    ("COLOR_MATRIX_R", 0x12),
    ("COLOR_MATRIX_G", 0x13),
    ("COLOR_MATRIX_B", 0x14),
    ("SHADOWMAP_POLYGON_OFFSET", 0x15),
    ("RENDER_TARGET_SIZE", 0x16),
    ("DOF_EQUATION_VIEWMODEL_AND_FAR_BLUR", 0x17),
    ("DOF_EQUATION_SCENE", 0x18),
    ("DOF_LERP_SCALE", 0x19),
    ("DOF_LERP_BIAS", 0x1a),
    ("DOF_ROW_DELTA", 0x1b),
    ("MOTION_MATRIX_X", 0x1c),
    ("MOTION_MATRIX_Y", 0x1d),
    ("MOTION_MATRIX_W", 0x1e),
    ("SHADOWMAP_SWITCH_PARTITION", 0x1f),
    ("SHADOWMAP_SCALE", 0x20),
    ("ZNEAR", 0x21),
    ("LIGHTING_LOOKUP_SCALE", 0x22),
    ("DEBUG_BUMPMAP", 0x23),
    ("MATERIAL_COLOR", 0x24),
    ("FOG", 0x25),
    ("FOG_COLOR_LINEAR", 0x26),
    ("FOG_COLOR_GAMMA", 0x27),
    ("FOG_SUN_CONSTS", 0x28),
    ("FOG_SUN_COLOR_LINEAR", 0x29),
    ("FOG_SUN_COLOR_GAMMA", 0x2a),
    ("FOG_SUN_DIR", 0x2b),
    ("GLOW_SETUP", 0x2c),
    ("GLOW_APPLY", 0x2d),
    ("COLOR_BIAS", 0x2e),
    ("COLOR_TINT_BASE", 0x2f),
    ("COLOR_TINT_DELTA", 0x30),
    ("COLOR_TINT_QUADRATIC_DELTA", 0x31),
    ("OUTDOOR_FEATHER_PARMS", 0x32),
    ("ENVMAP_PARMS", 0x33),
    ("SUN_SHADOWMAP_PIXEL_ADJUST", 0x34),
    ("SPOT_SHADOWMAP_PIXEL_ADJUST", 0x35),
    ("COMPOSITE_FX_DISTORTION", 0x36),
    ("POSTFX_FADE_EFFECT", 0x37),
    ("VIEWPORT_DIMENSIONS", 0x38),
    ("FRAMEBUFFER_READ", 0x39),
    ("BASE_LIGHTING_COORDS", 0x3a),
    ("LIGHT_PROBE_AMBIENT", 0x3b),
    ("NEARPLANE_ORG", 0x3c),
    ("NEARPLANE_DX", 0x3d),
    ("NEARPLANE_DY", 0x3e),
    ("CLIP_SPACE_LOOKUP_SCALE", 0x3f),
    ("CLIP_SPACE_LOOKUP_OFFSET", 0x40),
    ("PARTICLE_CLOUD_MATRIX0", 0x41),
    ("PARTICLE_CLOUD_MATRIX1", 0x42),
    ("PARTICLE_CLOUD_MATRIX2", 0x43),
    ("PARTICLE_CLOUD_SPARK_COLOR0", 0x44),
    ("PARTICLE_CLOUD_SPARK_COLOR1", 0x45),
    ("PARTICLE_CLOUD_SPARK_COLOR2", 0x46),
    ("PARTICLE_FOUNTAIN_PARM0", 0x47),
    ("PARTICLE_FOUNTAIN_PARM1", 0x48),
    ("DEPTH_FROM_CLIP", 0x49),
    ("CODE_MESH_ARG_0", 0x4a),
    ("CODE_MESH_ARG_1", 0x4b),
    ("VIEW_MATRIX", 0x4c),
    ("INVERSE_VIEW_MATRIX", 0x4d),
    ("TRANSPOSE_VIEW_MATRIX", 0x4e),
    ("INVERSE_TRANSPOSE_VIEW_MATRIX", 0x4f),
    ("PROJECTION_MATRIX", 0x50),
    ("INVERSE_PROJECTION_MATRIX", 0x51),
    ("TRANSPOSE_PROJECTION_MATRIX", 0x52),
    ("INVERSE_TRANSPOSE_PROJECTION_MATRIX", 0x53),
    ("VIEW_PROJECTION_MATRIX", 0x54),
    ("INVERSE_VIEW_PROJECTION_MATRIX", 0x55),
    ("TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x56),
    ("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x57),
    ("SHADOW_LOOKUP_MATRIX", 0x58),
    ("INVERSE_SHADOW_LOOKUP_MATRIX", 0x59),
    ("TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x5a),
    ("INVERSE_TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x5b),
    ("WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5c),
    ("INVERSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5d),
    ("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5e),
    ("INVERSE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5f),
    ("WORLD_MATRIX0", 0x60),
    ("INVERSE_WORLD_MATRIX0", 0x61),
    ("TRANSPOSE_WORLD_MATRIX0", 0x62),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX0", 0x63),
    ("WORLD_VIEW_MATRIX0", 0x64),
    ("INVERSE_WORLD_VIEW_MATRIX0", 0x65),
    ("TRANSPOSE_WORLD_VIEW_MATRIX0", 0x66),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0", 0x67),
    ("WORLD_VIEW_PROJECTION_MATRIX0", 0x68),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x69),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x6a),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x6b),
    ("WORLD_MATRIX1", 0x6c),
    ("INVERSE_WORLD_MATRIX1", 0x6d),
    ("TRANSPOSE_WORLD_MATRIX1", 0x6e),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX1", 0x6f),
    ("WORLD_VIEW_MATRIX1", 0x70),
    ("INVERSE_WORLD_VIEW_MATRIX1", 0x71),
    ("TRANSPOSE_WORLD_VIEW_MATRIX1", 0x72),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX1", 0x73),
    ("WORLD_VIEW_PROJECTION_MATRIX1", 0x74),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x75),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x76),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x77),
    ("WORLD_MATRIX2", 0x78),
    ("INVERSE_WORLD_MATRIX2", 0x79),
    ("TRANSPOSE_WORLD_MATRIX2", 0x7a),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX2", 0x7b),
    ("WORLD_VIEW_MATRIX2", 0x7c),
    ("INVERSE_WORLD_VIEW_MATRIX2", 0x7d),
    ("TRANSPOSE_WORLD_VIEW_MATRIX2", 0x7e),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX2", 0x7f),
    ("WORLD_VIEW_PROJECTION_MATRIX2", 0x80),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x81),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x82),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x83),
];

const IW5_CODE_CONST: &[(&str, u16)] = &[
    ("LIGHT_POSITION", 0x0),
    ("LIGHT_DIFFUSE", 0x1),
    ("LIGHT_SPECULAR", 0x2),
    ("LIGHT_SPOTDIR", 0x3),
    ("LIGHT_SPOTFACTORS", 0x4),
    ("LIGHT_FALLOFF_PLACEMENT", 0x5),
    ("PARTICLE_CLOUD_COLOR", 0x6),
    ("GAMETIME", 0x7),
    ("EYEOFFSET", 0x8),
    ("COLOR_SATURATION_R", 0x9),
    ("COLOR_SATURATION_G", 0xa),
    ("COLOR_SATURATION_B", 0xb),
    ("SSAO_PARMS", 0xc),
    ("PIXEL_COST_FRACS", 0xd),
    ("PIXEL_COST_DECODE", 0xe),
    ("FILTER_TAP_0", 0xf),
    ("FILTER_TAP_1", 0x10),
    ("FILTER_TAP_2", 0x11),
    ("FILTER_TAP_3", 0x12),
    ("FILTER_TAP_4", 0x13),
    ("FILTER_TAP_5", 0x14),
    ("FILTER_TAP_6", 0x15),
    ("FILTER_TAP_7", 0x16),
    ("COLOR_MATRIX_R", 0x17),
    ("COLOR_MATRIX_G", 0x18),
    ("COLOR_MATRIX_B", 0x19),
    ("SHADOWMAP_POLYGON_OFFSET", 0x1a),
    ("RENDER_TARGET_SIZE", 0x1b),
    ("RENDER_SOURCE_SIZE", 0x1c),
    ("DOF_EQUATION_VIEWMODEL_AND_FAR_BLUR", 0x1d),
    ("DOF_EQUATION_SCENE", 0x1e),
    ("DOF_LERP_SCALE", 0x1f),
    ("DOF_LERP_BIAS", 0x20),
    ("DOF_ROW_DELTA", 0x21),
    ("MOTION_MATRIX_X", 0x22),
    ("MOTION_MATRIX_Y", 0x23),
    ("MOTION_MATRIX_W", 0x24),
    ("SHADOWMAP_SWITCH_PARTITION", 0x25),
    ("SHADOWMAP_SCALE", 0x26),
    ("ZNEAR", 0x27),
    ("LIGHTING_LOOKUP_SCALE", 0x28),
    ("DEBUG_BUMPMAP", 0x29),
    ("MATERIAL_COLOR", 0x2a),
    ("FOG", 0x2b),
    ("FOG_COLOR_LINEAR", 0x2c),
    ("FOG_COLOR_GAMMA", 0x2d),
    ("FOG_SUN_CONSTS", 0x2e),
    ("FOG_SUN_COLOR_LINEAR", 0x2f),
    ("FOG_SUN_COLOR_GAMMA", 0x30),
    ("FOG_SUN_DIR", 0x31),
    ("GLOW_SETUP", 0x32),
    ("GLOW_APPLY", 0x33),
    ("COLOR_BIAS", 0x34),
    ("COLOR_TINT_BASE", 0x35),
    ("COLOR_TINT_DELTA", 0x36),
    ("COLOR_TINT_QUADRATIC_DELTA", 0x37),
    ("OUTDOOR_FEATHER_PARMS", 0x38),
    ("ENVMAP_PARMS", 0x39),
    ("SUN_SHADOWMAP_PIXEL_ADJUST", 0x3a),
    ("SPOT_SHADOWMAP_PIXEL_ADJUST", 0x3b),
    ("COMPOSITE_FX_DISTORTION", 0x3c),
    ("POSTFX_FADE_EFFECT", 0x3d),
    ("VIEWPORT_DIMENSIONS", 0x3e),
    ("FRAMEBUFFER_READ", 0x3f),
    ("THERMAL_COLOR_OFFSET", 0x40),
    ("PLAYLIST_POPULATION_PARAMS", 0x41),
    ("BASE_LIGHTING_COORDS", 0x42),
    ("LIGHT_PROBE_AMBIENT", 0x43),
    ("NEARPLANE_ORG", 0x44),
    ("NEARPLANE_DX", 0x45),
    ("NEARPLANE_DY", 0x46),
    ("CLIP_SPACE_LOOKUP_SCALE", 0x47),
    ("CLIP_SPACE_LOOKUP_OFFSET", 0x48),
    ("PARTICLE_CLOUD_MATRIX0", 0x49),
    ("PARTICLE_CLOUD_MATRIX1", 0x4a),
    ("PARTICLE_CLOUD_MATRIX2", 0x4b),
    ("PARTICLE_CLOUD_SPARK_COLOR0", 0x4c),
    ("PARTICLE_CLOUD_SPARK_COLOR1", 0x4d),
    ("PARTICLE_CLOUD_SPARK_COLOR2", 0x4e),
    ("PARTICLE_FOUNTAIN_PARM0", 0x4f),
    ("PARTICLE_FOUNTAIN_PARM1", 0x50),
    ("DEPTH_FROM_CLIP", 0x51),
    ("CODE_MESH_ARG_0", 0x52),
    ("CODE_MESH_ARG_1", 0x53),
    ("VIEW_MATRIX", 0x54),
    ("INVERSE_VIEW_MATRIX", 0x55),
    ("TRANSPOSE_VIEW_MATRIX", 0x56),
    ("INVERSE_TRANSPOSE_VIEW_MATRIX", 0x57),
    ("PROJECTION_MATRIX", 0x58),
    ("INVERSE_PROJECTION_MATRIX", 0x59),
    ("TRANSPOSE_PROJECTION_MATRIX", 0x5a),
    ("INVERSE_TRANSPOSE_PROJECTION_MATRIX", 0x5b),
    ("VIEW_PROJECTION_MATRIX", 0x5c),
    ("INVERSE_VIEW_PROJECTION_MATRIX", 0x5d),
    ("TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x5e),
    ("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x5f),
    ("SHADOW_LOOKUP_MATRIX", 0x60),
    ("INVERSE_SHADOW_LOOKUP_MATRIX", 0x61),
    ("TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x62),
    ("INVERSE_TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x63),
    ("WORLD_OUTDOOR_LOOKUP_MATRIX", 0x64),
    ("INVERSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x65),
    ("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x66),
    ("INVERSE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x67),
    ("WORLD_MATRIX0", 0x68),
    ("INVERSE_WORLD_MATRIX0", 0x69),
    ("TRANSPOSE_WORLD_MATRIX0", 0x6a),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX0", 0x6b),
    ("WORLD_VIEW_MATRIX0", 0x6c),
    ("INVERSE_WORLD_VIEW_MATRIX0", 0x6d),
    ("TRANSPOSE_WORLD_VIEW_MATRIX0", 0x6e),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0", 0x6f),
    ("WORLD_VIEW_PROJECTION_MATRIX0", 0x70),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x71),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x72),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x73),
    ("WORLD_MATRIX1", 0x74),
    ("INVERSE_WORLD_MATRIX1", 0x75),
    ("TRANSPOSE_WORLD_MATRIX1", 0x76),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX1", 0x77),
    ("WORLD_VIEW_MATRIX1", 0x78),
    ("INVERSE_WORLD_VIEW_MATRIX1", 0x79),
    ("TRANSPOSE_WORLD_VIEW_MATRIX1", 0x7a),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX1", 0x7b),
    ("WORLD_VIEW_PROJECTION_MATRIX1", 0x7c),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x7d),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x7e),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x7f),
    ("WORLD_MATRIX2", 0x80),
    ("INVERSE_WORLD_MATRIX2", 0x81),
    ("TRANSPOSE_WORLD_MATRIX2", 0x82),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX2", 0x83),
    ("WORLD_VIEW_MATRIX2", 0x84),
    ("INVERSE_WORLD_VIEW_MATRIX2", 0x85),
    ("TRANSPOSE_WORLD_VIEW_MATRIX2", 0x86),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX2", 0x87),
    ("WORLD_VIEW_PROJECTION_MATRIX2", 0x88),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x89),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x8a),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x8b),
];

const IW4_CODE_TEXTURE: &[(&str, u16)] = &[
    ("BLACK", 0x0),
    ("WHITE", 0x1),
    ("IDENTITY_NORMAL_MAP", 0x2),
    ("MODEL_LIGHTING", 0x3),
    ("LIGHTMAP_PRIMARY", 0x4),
    ("LIGHTMAP_SECONDARY", 0x5),
    ("SHADOWMAP_SUN", 0x6),
    ("SHADOWMAP_SPOT", 0x7),
    ("FEEDBACK", 0x8),
    ("RESOLVED_POST_SUN", 0x9),
    ("RESOLVED_SCENE", 0xa),
    ("POST_EFFECT_0", 0xb),
    ("POST_EFFECT_1", 0xc),
    ("LIGHT_ATTENUATION", 0xd),
    ("OUTDOOR", 0xe),
    ("FLOATZ", 0xf),
    ("PROCESSED_FLOATZ", 0x10),
    ("RAW_FLOATZ", 0x11),
    ("HALF_PARTICLES", 0x12),
    ("HALF_PARTICLES_Z", 0x13),
    ("CASE_TEXTURE", 0x14),
    ("CINEMATIC_Y", 0x15),
    ("CINEMATIC_CR", 0x16),
    ("CINEMATIC_CB", 0x17),
    ("CINEMATIC_A", 0x18),
    ("REFLECTION_PROBE", 0x19),
    ("ALTERNATE_SCENE", 0x1a),
];

const IW5_CODE_TEXTURE: &[(&str, u16)] = &[
    ("BLACK", 0x0),
    ("WHITE", 0x1),
    ("IDENTITY_NORMAL_MAP", 0x2),
    ("MODEL_LIGHTING", 0x3),
    ("LIGHTMAP_PRIMARY", 0x4),
    ("LIGHTMAP_SECONDARY", 0x5),
    ("SHADOWMAP_SUN", 0x6),
    ("SHADOWMAP_SPOT", 0x7),
    ("FEEDBACK", 0x8),
    ("RESOLVED_POST_SUN", 0x9),
    ("RESOLVED_SCENE", 0xa),
    ("POST_EFFECT_0", 0xb),
    ("POST_EFFECT_1", 0xc),
    ("LIGHT_ATTENUATION", 0xd),
    ("LIGHT_CUCOLORIS", 0xe),
    ("OUTDOOR", 0xf),
    ("FLOATZ", 0x10),
    ("PROCESSED_FLOATZ", 0x11),
    ("RAW_FLOATZ", 0x12),
    ("HALF_PARTICLES", 0x13),
    ("HALF_PARTICLES_Z", 0x14),
    ("CASE_TEXTURE", 0x15),
    ("CINEMATIC_Y", 0x16),
    ("CINEMATIC_CR", 0x17),
    ("CINEMATIC_CB", 0x18),
    ("CINEMATIC_A", 0x19),
    ("REFLECTION_PROBE", 0x1a),
    ("PIP_SCENE", 0x1b),
    ("COLOR_MANIPULATION", 0x1c),
    ("STREAMING_LOADING", 0x1d),
];
const IW5_CODE_CONST_LIMIT: usize = 0x8c;
const IW5_CODE_TEXTURE_LIMIT: usize = 0x1e;

const fn name_eq(a: &str, b: &str) -> bool {
    bytes_eq(a.as_bytes(), b.as_bytes())
}

const fn build_const_remap() -> [Option<u16>; IW5_CODE_CONST_LIMIT] {
    let mut out = [None; IW5_CODE_CONST_LIMIT];
    let mut i = 0;
    while i < IW5_CODE_CONST.len() {
        let (name, iw5) = IW5_CODE_CONST[i];
        let slot = iw5 as usize;
        let mut j = 0;
        while j < IW4_CODE_CONST.len() {
            if name_eq(name, IW4_CODE_CONST[j].0) {
                out[slot] = Some(IW4_CODE_CONST[j].1);
                break;
            }
            j += 1;
        }
        i += 1;
    }
    out
}

const fn build_texture_remap() -> [Option<u16>; IW5_CODE_TEXTURE_LIMIT] {
    let mut out = [None; IW5_CODE_TEXTURE_LIMIT];
    let mut i = 0;
    while i < IW5_CODE_TEXTURE.len() {
        let (name, iw5) = IW5_CODE_TEXTURE[i];
        let slot = iw5 as usize;
        let mut j = 0;
        while j < IW4_CODE_TEXTURE.len() {
            if name_eq(name, IW4_CODE_TEXTURE[j].0) {
                out[slot] = Some(IW4_CODE_TEXTURE[j].1);
                break;
            }
            j += 1;
        }
        i += 1;
    }
    out
}

const IW5_TO_IW4_CODE_CONST: [Option<u16>; IW5_CODE_CONST_LIMIT] = build_const_remap();
const IW5_TO_IW4_CODE_TEXTURE: [Option<u16>; IW5_CODE_TEXTURE_LIMIT] = build_texture_remap();

#[must_use]
pub fn remap_code_const_index(iw5: u16) -> Option<u16> {
    IW5_TO_IW4_CODE_CONST
        .get(usize::from(iw5))
        .copied()
        .flatten()
}

pub const LEFTOVER_IW5_CODE_BASE: u16 = 0x100;

pub const IW5_CODE_EYEOFFSET: u16 = 0x8;

pub const IW5_CODE_COLOR_SATURATION_R: u16 = 0x9;

pub const IW5_CODE_COLOR_SATURATION_G: u16 = 0xa;

pub const IW5_CODE_COLOR_SATURATION_B: u16 = 0xb;

#[must_use]
pub fn leftover_iw5_code_bank(iw5: u16) -> Option<u16> {
    match iw5 {
        IW5_CODE_EYEOFFSET
        | IW5_CODE_COLOR_SATURATION_R
        | IW5_CODE_COLOR_SATURATION_G
        | IW5_CODE_COLOR_SATURATION_B => Some(LEFTOVER_IW5_CODE_BASE.checked_add(iw5)?),
        _ => None,
    }
}

#[must_use]
pub fn remap_code_texture_index(iw5: u32) -> Option<u32> {
    let index = u16::try_from(iw5).ok()?;
    IW5_TO_IW4_CODE_TEXTURE
        .get(usize::from(index))
        .copied()
        .flatten()
        .map(u32::from)
}

#[must_use]
pub fn remap_occupancy_bits(iw5: u64) -> u64 {
    let mut out = 0u64;
    for iw5_slot in 0..IW5_TECHNIQUE_TYPE_COUNT {
        if iw5 & (1u64 << iw5_slot) == 0 {
            continue;
        }
        if let Some(iw4_slot) = iw5_slot_to_iw4(iw5_slot) {
            out |= 1u64 << iw4_slot;
        }
    }
    out
}

#[must_use]
pub fn remap_pass_count_by_slot(
    iw5: &[u8; IW5_TECHNIQUE_TYPE_COUNT],
) -> [u8; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [0u8; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw5_slot, mapped) in IW5_NAME_MATCH_IW4_SLOT.iter().enumerate() {
        if let Some(iw4_slot) = *mapped {
            out[usize::from(iw4_slot)] = iw5[iw5_slot];
        }
    }
    out
}

#[must_use]
pub fn remap_technique_flags_by_slot(
    iw5: &[u16; IW5_TECHNIQUE_TYPE_COUNT],
) -> [u16; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [0u16; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw5_slot, mapped) in IW5_NAME_MATCH_IW4_SLOT.iter().enumerate() {
        if let Some(iw4_slot) = *mapped {
            out[usize::from(iw4_slot)] = iw5[iw5_slot];
        }
    }
    out
}

pub fn remap_state_bits_entry(
    iw5: &[u8; IW5_TECHNIQUE_TYPE_COUNT],
) -> [u8; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [STATE_BITS_ENTRY_UNUSED; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw5_slot, mapped) in IW5_NAME_MATCH_IW4_SLOT.iter().enumerate() {
        if let Some(iw4_slot) = *mapped {
            out[usize::from(iw4_slot)] = iw5[iw5_slot];
        }
    }
    out
}

pub fn leftover_selector_census<'a>(
    entries: impl Iterator<Item = &'a [u8; IW5_TECHNIQUE_TYPE_COUNT]>,
) -> Option<String> {
    let mut mats = 0u32;
    let mut leftover_sel = 0u32;
    let mut slot_hits = [0u32; IW5_TECHNIQUE_TYPE_COUNT];
    for raw in entries {
        mats = mats.saturating_add(1);
        for (slot, _) in leftover_iw5_slots() {
            if raw[slot] != STATE_BITS_ENTRY_UNUSED {
                leftover_sel = leftover_sel.saturating_add(1);
                slot_hits[slot] = slot_hits[slot].saturating_add(1);
            }
        }
    }
    if mats == 0 {
        return None;
    }
    let mut slots = String::new();
    for (slot, name) in leftover_iw5_slots() {
        if slot_hits[slot] == 0 {
            continue;
        }
        if !slots.is_empty() {
            slots.push(',');
        }
        slots.push_str(&format!("{slot}:{name}={}", slot_hits[slot]));
    }
    if slots.is_empty() {
        slots.push_str("none");
    }
    Some(format!(
        "mats={mats} leftover_sel={leftover_sel} occupied={slots}"
    ))
}
