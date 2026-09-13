#![no_std]
#![forbid(unsafe_code)]

mod gfxworld;
pub mod img_format;
pub mod iwi;
mod lightgrid;
pub mod material;
pub mod polygon_offset;
pub mod size;
pub mod snd_alias;
mod snd_pick;
mod snd_voice;
pub mod state_bits;
pub mod state_bits1;
pub mod vertex_decl;
pub mod wavelet;
mod xmodel;
mod xsurface_collision;

pub use gfxworld::GfxWorld;
pub use img_format::{ImgFormatInfo, ImgFormatKind, img_format_info, wavelet_pixel_stride};
pub use iwi::{
    IWI_FLAG_NO_MIPMAPS, IWI_MAGIC, IWI_USAGE_COLOR, IWI_USAGE_NORMAL, IWI_USAGE_SKYBOX_A,
    IWI_USAGE_SKYBOX_B, IWI_V8_HEADER_LEN, IWI_VERSION_V8, IwiHeader, IwiHeaderError,
};
pub use lightgrid::GfxLightGrid;
pub use material::{
    CAMERA_REGION_DEPTH_HACK, CAMERA_REGION_EMISSIVE, CAMERA_REGION_LIT_OPAQUE,
    CAMERA_REGION_LIT_TRANS, CAMERA_REGION_NONE, ColorPassAgreement, MATERIAL_CAMERA_REGION,
    MATERIAL_STATE_BITS_COUNT, MATERIAL_STATE_BITS_ENTRY, MATERIAL_STATE_BITS_TABLE,
    MATERIAL_STATE_FLAGS, MATERIAL_SURFACE_TYPE_BITS, MaterialDrawRoute, MaterialPass,
    SORT_KEY_SKY, SORT_KEY_SKYBOX, TECHNIQUE_COLOR_BAND_FIRST, TECHNIQUE_LIT_MASK,
    TECHNIQUE_MODEL_LIGHTING, TECHNIQUE_SHADOW_MASK, TECHNIQUE_UNIVERSAL_MASK,
    color_pass_agreement, color_pass_row_for_tech_type, color_pass_row_for_tech_type_pass,
    lit_band_decode_conflicts,
};
pub use polygon_offset::{
    GFXS1_POLYGON_OFFSET_MASK, GFXS1_POLYGON_OFFSET_SHADOWMAP_LEVEL, GFXS1_POLYGON_OFFSET_SHIFT,
    POLYGON_OFFSET_BIAS_TO_D3D, R_POLYGON_OFFSET_BIAS_DEFAULT, R_POLYGON_OFFSET_BIAS_MIN,
    R_POLYGON_OFFSET_MAX, R_POLYGON_OFFSET_SCALE_DEFAULT, R_POLYGON_OFFSET_SCALE_MIN,
    SM_POLYGON_OFFSET_BIAS_DEFAULT, SM_POLYGON_OFFSET_BIAS_MAX, SM_POLYGON_OFFSET_SCALE_DEFAULT,
    SM_POLYGON_OFFSET_SCALE_MAX, d3d_depth_bias_to_wgpu_constant, polygon_offset_d3d,
    polygon_offset_d3d_defaults, polygon_offset_level, polygon_offset_wgpu_defaults,
};
pub use snd_alias::{
    SND_ALIAS_ALIAS_NAME, SND_ALIAS_CENTER_PERCENTAGE, SND_ALIAS_CHAIN, SND_ALIAS_DIST_MAX,
    SND_ALIAS_DIST_MIN, SND_ALIAS_ENVELOP_MAX, SND_ALIAS_ENVELOP_MIN, SND_ALIAS_ENVELOP_PERCENTAGE,
    SND_ALIAS_FLAG_CHANNEL_MASK, SND_ALIAS_FLAG_CHANNEL_SHIFT, SND_ALIAS_FLAG_FULL_REVERB,
    SND_ALIAS_FLAG_LOOPING, SND_ALIAS_FLAG_MASTER, SND_ALIAS_FLAG_RANDOM_START,
    SND_ALIAS_FLAG_SLAVE, SND_ALIAS_FLAG_TYPE_MASK, SND_ALIAS_FLAG_TYPE_SHIFT, SND_ALIAS_FLAGS,
    SND_ALIAS_LFE_PERCENTAGE, SND_ALIAS_MIXER_GROUP, SND_ALIAS_PITCH_MAX, SND_ALIAS_PITCH_MIN,
    SND_ALIAS_PROBABILITY, SND_ALIAS_SECONDARY, SND_ALIAS_SEQUENCE, SND_ALIAS_SLAVE_PERCENTAGE,
    SND_ALIAS_SOUND_FILE, SND_ALIAS_SPEAKER_MAP, SND_ALIAS_START_DELAY, SND_ALIAS_SUBTITLE,
    SND_ALIAS_VELOCITY_MIN, SND_ALIAS_VOL_MAX, SND_ALIAS_VOL_MIN, SND_ALIAS_VOLUME_FALLOFF_CURVE,
    SND_CURVE_DEFAULT_ASSET_NAME, SND_CURVE_EVAL_OUT_OF_RANGE, SND_CURVE_FILENAME,
    SND_CURVE_KNOT_COUNT, SND_CURVE_KNOT_STRIDE, SND_CURVE_KNOTS, SND_CURVE_MAX_KNOTS,
    SND_ENTCHANNEL_DEFAULT_MAX_VOICES, SND_ENTCHANNEL_FILE, SND_ENTCHANNEL_MAX, SndAliasFlags,
    SndAliasSampleKind, snd_attenuate, snd_curve_eval,
};
pub use snd_pick::{
    SND_LCG_ADD, SND_LCG_MUL, SND_LCG_UNIT_SCALE, lerp_range, pick_weighted_variant_index,
    snd_advance_lcg, snd_unit_random,
};
pub use snd_voice::{
    SND_VOICE_FINISHED_FRACTION, SndVoiceOccupant, SndVoiceRequest, snd_entity_channel_matches,
    snd_has_free_voice, snd_pick_voice_slot, snd_voice_metric_2d,
};
pub use state_bits::{
    AlphaTest, D3DCULL_CCW, D3DCULL_CW, D3DCULL_NONE, D3DRS_ALPHAFUNC, D3DRS_ALPHAREF,
    D3DRS_ALPHATESTENABLE, D3DRS_CULLMODE, GFXS0_ATEST_DISABLE, GFXS0_ATEST_GE_128,
    GFXS0_ATEST_GT_0, GFXS0_ATEST_LT_128, GFXS0_ATEST_MASK, GFXS0_ATEST_SHIFT, GFXS0_CULL_BACK,
    GFXS0_CULL_FRONT, GFXS0_CULL_MASK, GFXS0_CULL_NONE, GFXS0_CULL_SHIFT, Gfxs0AlphaTest,
    Gfxs0CullFace, S_ALPHA_TEST_TABLE, S_CULL_TABLE, alpha_test_from_state_bits,
    cull_face_from_state_bits, d3d_cull_mode_from_state_bits,
};
pub use state_bits1::{
    D3DCMP_ALWAYS, D3DCMP_EQUAL, D3DCMP_LESS, D3DCMP_LESSEQUAL, D3DRS_ZENABLE, D3DRS_ZFUNC,
    D3DRS_ZWRITEENABLE, GFXS1_DEPTHTEST_ALWAYS, GFXS1_DEPTHTEST_DISABLE, GFXS1_DEPTHTEST_EQUAL,
    GFXS1_DEPTHTEST_LESS, GFXS1_DEPTHTEST_LESSEQUAL, GFXS1_DEPTHTEST_MASK, GFXS1_DEPTHTEST_SHIFT,
    GFXS1_DEPTHWRITE, S_DEPTH_TEST_TABLE, d3d_zfunc_from_word1, depth_state_from_state_bits,
    depth_test_enable, depth_test_field, depth_write_enable,
};
pub use wavelet::{
    WAVELET_BOOK_CROSS_HEAD, WAVELET_BOOK_CROSS_TAIL, WAVELET_BOOK_DETAIL_HEAD,
    WAVELET_BOOK_DETAIL_TAIL, WAVELET_BOOK_PLAIN_HEAD, WAVELET_BOOK_PLAIN_TAIL, WAVELET_ESCAPE,
    WaveletBits, WaveletError, wavelet_check_header, wavelet_decompress_level, wavelet_level_size,
    wavelet_lift_block, wavelet_top_level,
};
pub use xmodel::XModel;
pub use xsurface_collision::{
    XSurfaceCollisionLeaf, XSurfaceCollisionNode, XSurfaceCollisionRangeError,
    validate_xsurface_collision_ranges,
};
