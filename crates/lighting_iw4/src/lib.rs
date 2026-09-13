#![no_std]
#![forbid(unsafe_code)]

mod accumulate;
mod atapoint;
mod cull;
mod dlight;
mod entry;
mod expand;
mod fragment;
mod glow;
mod isvalid;
mod light_probe;
mod lit_tech;
mod lookup;
mod model_lighting_cache;
mod pick;
mod reflection_probe;
mod rle;
mod row_rle;
mod scene_light;
mod smodel_alloc;
mod smodel_cache;
mod smodel_cmd;
mod smodel_inst;
mod smodel_lighting;
mod smodel_skin;
mod spot_shadow;
mod spot_shadow_choose;
mod trace;
mod viewmodel_origin;

pub use accumulate::{
    LIGHT_GRID_COLORS_ACCUM_COUNT, LIGHT_GRID_COLORS_BYTE_COUNT, LIGHT_GRID_COMPRESS_R_OFFSETS,
    LIGHT_GRID_PACK_BIAS, LIGHT_GRID_TWEAK_ACCUM_CLAMP, LightGridCompressedColor,
    light_grid_accumulate_colors, light_grid_add_colors, light_grid_apply_contrast_tweaks,
    light_grid_apply_intensity_tweaks, light_grid_compress_colors, light_grid_expand_colors,
    light_grid_pack_colors, light_grid_tweak_fixed_from_float,
};
pub use atapoint::{
    LIGHT_GRID_ATPOINT_EMPTY_PRIMARY, LIGHT_GRID_COLORS_STRIDE, LIGHT_GRID_FIXED_WEIGHT_SCALE,
    LIGHT_GRID_FIXED_WEIGHT_SUM, LIGHT_GRID_ROUND_BIAS, LIGHT_GRID_SAMPLE_WEIGHT_SCALE,
    LightGridAtPointEmptyGate, LightGridAtPointPath, light_grid_atapoint_return_primary,
    light_grid_atapoint_select_path, light_grid_colors_byte_offset,
    light_grid_default_colors_index, light_grid_encode_sample_weight,
    light_grid_fixed_point_blend_weights,
};
pub use cull::{
    COM_PRIMARY_LIGHT_COS_HALF_FOV_EXPANDED, COM_PRIMARY_LIGHT_DIR, COM_PRIMARY_LIGHT_ORIGIN,
    COM_PRIMARY_LIGHT_RADIUS, ComPrimaryLightCull, DYN_ENT_PRIMARY_LIGHT_LINK_DIST2_INIT,
    DynEntPrimaryLightLink, LightRegionAxis, LightRegionHull, LightRegionHulls,
    NonSunPrimaryWalkTrace, cull_box_from_cone, cull_box_from_conic_section_of_sphere,
    cull_box_from_light_region_hull, cull_box_from_primary_light, cull_box_from_sphere,
    cull_point_from_cone_expanded, cull_point_from_light_region_hull,
    dyn_ent_links_to_primary_light, dyn_ent_primary_light_link, dyn_ent_primary_light_link_dist2,
    light_region_culls_box, light_region_culls_point, lighting_query_box_half,
    non_sun_primary_light_for_box, non_sun_primary_light_walk_trace,
};
pub use dlight::{
    R_DLIGHT_BACKEND_MAX, R_DLIGHT_LIMIT_DEFAULT, R_DLIGHT_LIMIT_MAX, SceneDlight,
    append_scene_dlights_to_backend, cull_point_and_radius_from_planes, dlight_copies_to_backend,
    dlight_hits_aabb, dlight_partition_prefers, dlight_select_visible, dlight_visible,
    spot_dlight0_frustum_culls, spot_dlight0_special_copy_allows,
};
pub use entry::GfxLightGridEntry;
pub use expand::{
    CONST_SRC_CODE_BASE_LIGHTING_COORDS, CONST_SRC_CODE_LIGHTING_LOOKUP_SCALE,
    LIGHT_GRID_COMPRESS_DIRS, LIGHT_GRID_EXPAND_SHELL_TO_TEXEL, LIGHT_GRID_SHELL_DIRS,
    MODEL_LIGHTING_ATLAS_DEPTH, MODEL_LIGHTING_ATLAS_WIDTH, MODEL_LIGHTING_COORD_BIAS,
    MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_LOOKUP_SCALE_U,
    MODEL_LIGHTING_LOOKUP_SCALE_V_FACTOR, MODEL_LIGHTING_LOOKUP_SCALE_W,
    MODEL_LIGHTING_PACKED_ROW_PITCH, MODEL_LIGHTING_PACKED_SLICE_PITCH, MODEL_LIGHTING_PACKED_W,
    MODEL_LIGHTING_SMODEL_ENTRY_LIMIT_FLOOR, MODEL_LIGHTING_TILE_BYTES, MODEL_LIGHTING_TILE_DIM,
    MODEL_LIGHTING_TILE_TEXELS, MODEL_LIGHTING_VOLUME_W, ModelLightingAtlasDims,
    ModelLightingCapExceeded, ModelLightingCoords, ModelLightingExpandPitch,
    ModelLightingLookupScale, ModelLightingPackedCoords, ModelLightingTileIndex,
    ModelLightingTileRgba, TECHNIQUE_LIT, TEXTURE_SRC_CODE_LIGHT_ATTENUATION,
    TEXTURE_SRC_CODE_MODEL_LIGHTING, check_smodel_lit_within_retail_cap,
    light_grid_expand_shell_to_tile_rgba, light_grid_expand_shell_to_tile_zyx,
    model_lighting_atlas_dims, model_lighting_atlas_slice_offset,
    model_lighting_coords_from_handle, model_lighting_entry_from_handle,
    model_lighting_expand_pitch, model_lighting_inv_image_height, model_lighting_lookup_scale,
    model_lighting_packed_coords_from_entry, model_lighting_packed_ground_rgbw,
    model_lighting_solid_tile_bgra, model_lighting_write_tile_to_atlas,
};
pub use fragment::{
    BASE_LIGHTING_COORDS_TEXCOORD, ENV_MAP_LOD_BIAS, ENV_MAP_LOD_SCALE,
    MODEL_LIGHTING_LOCAL_TILE_LOOKUP_SCALE, MODEL_LIGHTING_SAMPLER_DCL_USAGE,
    MODEL_LIGHTING_SAMPLER_STAGE, REFLECTION_PROBE_SAMPLER_STAGE, SPECULAR_MAP_SAMPLER_STAGE,
    VERTEX_COLOR_DCL_USAGE, decode_model_lighting_sample, lit_albedo, lit_fragment_color,
    lit_sun_lighting, model_lighting_env_intensity, model_lighting_env_lod,
    model_lighting_local_tile_nearest_index, model_lighting_local_tile_uvw,
    model_lighting_lookup_coords, model_lighting_reflect, model_lighting_specular_term,
    model_lighting_specular_term_square,
};
pub use glow::{
    CONST_SRC_CODE_GLOW_APPLY, CONST_SRC_CODE_GLOW_SETUP, GlowBloomConsts, GlowViewInfo,
    R_FULLBRIGHT_DEFAULT, R_GLOW_DEFAULT, R_GLOW_TWEAK_CUTOFF_DEFAULT, R_GLOW_TWEAK_ENABLE_DEFAULT,
    R_GLOW_TWEAK_INTENSITY0_DEFAULT, R_GLOW_TWEAK_INTENSITY0_MAX, R_GLOW_TWEAK_RADIUS0_DEFAULT,
    R_GLOW_TWEAK_RADIUS0_MAX, R_GLOW_USE_TWEAKS_DEFAULT, r_select_glow_view_info, r_set_glow_info,
    r_using_glow,
};
pub use isvalid::{
    LIGHT_GRID_CELL_TO_INCHES_XY, LIGHT_GRID_CELL_TO_INCHES_Z, LIGHT_GRID_ISVALID_ORIGIN_SUB,
    LIGHT_GRID_SIGHT_CONTENT_MASK, LIGHT_GRID_SIGHT_NUDGE, LightGridIsValidSegment,
    light_grid_cell_axis_to_inches, light_grid_corner_inch_deltas, light_grid_isvalid_corner_pos,
    light_grid_isvalid_nudged_target, light_grid_isvalid_sample, light_grid_isvalid_segment,
    light_grid_vec3_normalize,
};
pub use light_probe::{
    CONST_SRC_CODE_LIGHT_PROBE_AMBIENT, LIGHT_PROBE_AMBIENT_RGB_SCALE_BITS,
    LIGHT_PROBE_AMBIENT_WEIGHT_SCALE_BITS, light_probe_ambient_from_packed,
};
pub use lit_tech::{
    GFX_DRAW_METHOD_LIT_BEGIN, LIT_TECH_COL_COUNT, LIT_TECH_INSTANCED_ROW,
    LIT_TECH_INSTANCED_ROW_DFOG, LIT_TECH_INSTANCED_SURF_TYPES, LIT_TECH_NO_SHADOW_DIR_SLOTS,
    LIT_TECH_NO_SHADOW_LOCAL_SLOTS, LIT_TECH_SHADOW_COLUMN_BIAS, LIT_TECH_SHADOW_DIR_SLOTS,
    LIT_TECH_SHADOW_SPOT_SLOTS, LIT_TECH_STANDARD_ROW, LIT_TECH_STANDARD_ROW_DFOG,
    LIT_TECH_SURF_ROWS, TECHNIQUE_NONE, is_lit_remap_slot, lit_tech_column, lit_tech_type,
};
pub use lookup::{
    LightGridLookupColorAccum, LightGridLookupCorner, LightGridLookupWeights,
    light_grid_lookup_accumulate_corner, light_grid_lookup_accumulate_corners,
    light_grid_lookup_corner_wants_trace, light_grid_lookup_remap_primary,
};
pub use model_lighting_cache::{
    MODEL_LIGHTING_PIXEL_FREE_BITS_BUFFERS, ModelLightingCacheAlloc, ModelLightingCacheGlob,
    ModelLightingCacheGlobError, dyn_pixel_free_bits_index, lighting_info_from_bytes,
    lighting_info_reflection_probe_index, lighting_info_scene_light_index,
    model_lighting_cache_bit_mask, model_lighting_cache_free_bits_words,
    model_lighting_cache_free_slot_count, model_lighting_cache_handle, model_lighting_cache_slot,
    model_lighting_pixel_free_bits_size_bytes, model_lighting_pixel_free_bits_word_count,
    toggle_dyn_model_lighting_frame,
};
pub use pick::{
    LIGHT_GRID_CELL_ORIGIN_BIAS_I32, LIGHT_GRID_CORNER_WEIGHT_EPS, LIGHT_GRID_NEG_CELL_FLOAT_FIX,
    LIGHT_GRID_ORIGIN_BIAS, LIGHT_GRID_WEIGHT_ONE, LIGHT_GRID_XY_SCALE, LIGHT_GRID_Z_SCALE,
    LightGridCell, LightGridPickCorner, LightGridPickPrimary, LightGridPickRowRleCells,
    light_grid_axis_lerp, light_grid_axis_lerps, light_grid_cell_as_xyz,
    light_grid_cell_index_as_f32, light_grid_corner_is_suppressed, light_grid_corner_needs_trace,
    light_grid_corner_weight_keeps_entry, light_grid_corner_weights,
    light_grid_pick_default_grid_entry, light_grid_pick_merge_entry_quads,
    light_grid_pick_primary_from_corners, light_grid_pick_row_rle_cells,
    light_grid_primary_light_prefers, light_grid_primary_light_sun_band, light_grid_sample_cell,
};
pub use reflection_probe::{
    lighting_info_reflection_probe_for_point, nearest_reflection_probe_in_cell_list,
    nearest_reflection_probe_skip_0,
};
pub use rle::{
    LIGHT_GRID_RLE_BASE_STRIDE, light_grid_rle_run_stride, model_lighting_patch_swizzle,
};
pub use row_rle::{
    LIGHT_GRID_ROW_ABSENT, LIGHT_GRID_ROW_HEADER_SIZE, LightGridEntryQuad, LightGridRleCursor,
    LightGridRowHeader, light_grid_at_row_last_column, light_grid_column_before_row_start_quad,
    light_grid_column_entry_pair, light_grid_empty_run_entry_quad, light_grid_entry_byte_offset,
    light_grid_next_run_first_column_index, light_grid_rle_advance_to_column,
    light_grid_row_contains_cell, light_grid_solid_run_entry_index,
    light_grid_solid_run_entry_quad, light_grid_solid_run_has_entry, light_grid_solid_run_z_base,
};
pub use scene_light::{
    AddOmniLightRefuse, COLOR_SRGB_LINEAR_SCALE, COLOR_SRGB_OFFSET, COLOR_SRGB_POW_EXP,
    COLOR_SRGB_POW_SCALE, COLOR_SRGB_THRESHOLD, CONST_SRC_CODE_LIGHT_DIFFUSE,
    CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT, CONST_SRC_CODE_LIGHT_POSITION,
    CONST_SRC_CODE_LIGHT_SPECULAR, CONST_SRC_CODE_LIGHT_SPOTDIR, CONST_SRC_CODE_LIGHT_SPOTFACTORS,
    GFX_LIGHT_TYPE_DIR, GFX_LIGHT_TYPE_OMNI, GFX_LIGHT_TYPE_SPOT, GfxLightPack,
    LIGHT_FALLOFF_PLACEMENT_SCALE, LIGHT_PACK_ONE, R_COLOR_SCALE_DEFAULT, R_DLIGHT_SCENE_CAP,
    R_SPOT_LIGHT_BRIGHTNESS_DEFAULT, R_SPOT_LIGHT_DIR_SIGN, R_SPOT_LIGHT_END_RADIUS_DEFAULT,
    R_SPOT_LIGHT_EPS, R_SPOT_LIGHT_EXPONENT_DEFAULT, R_SPOT_LIGHT_FOV_INNER_FRACTION_DEFAULT,
    R_SPOT_LIGHT_START_RADIUS_DEFAULT, ShadowableLightPack, SpotLightConeDvars,
    color_srgb_to_linear, dir_light_position, light_diffuse, light_falloff_placement,
    light_specular, light_spot_dir, light_spot_factors, omni_spot_light_position,
    pack_shadowable_light, r_add_omni_light_to_scene_allows, r_omni_light_pack,
    r_spot_light_clamp_end, r_spot_light_offset, r_spot_light_pack,
};
pub use smodel_alloc::{
    SModelDirtyLightingAction, SModelLightingAlloc, SModelLightingCounters, SModelLightingGlob,
    SModelLightingGlobError, dirty_smodel_lighting_action, smodel_lighting_bits_words,
};
pub use smodel_cache::{
    CacheStaticModelSurface, SMC_BANK_N, SMC_BANK_VB_BYTES, SMC_BANK_VERTS, SMC_CLASS_N,
    SMC_IDLE_FRAMES, SMC_INDEX_U16_N, SMC_LEAF_N, SMC_LINK_N, SMC_PATCH_LOCK_NOOVERWRITE,
    SMC_PATCH_SURF_MAX, SMC_PATCH_VERT_MAX, SMC_SIZE_CLASS_VERTS, SMC_STORAGE_LENS, SMC_TREE_N,
    SMC_TREES_PER_BANK, SMC_VB_BYTES, SMC_VB_CREATE_USAGE, SMC_VB_VERTS, SMC_VERT_STRIDE,
    SMODEL_BUCKET_CACHED, SMODEL_BUCKET_CAP, SMODEL_BUCKET_LIST_N, SMODEL_BUCKET_PRETESS,
    SMODEL_BUCKET_RIGID, SMODEL_BUCKET_SKINNED, SMODEL_BUCKET_STRIDE, SmcAllocatorStorage,
    SmcCacheError, SmcDrawCacheIndex, SmcIndexBakeError, SmcLeafStorage, SmcLodCacheSpec,
    SmcPatchLock, SmcStorageLens, SmcTree, SmodelBucketPush, SmodelConsumedBucket,
    SmodelSurfBucketLists, SmodelSurfPath, StaticModelCache, r_add_static_model_surf_to_bucket,
    r_cache_static_model_indices, r_cache_static_model_indices_u16_slot, r_smc_draw_cache_index,
    r_smc_stream_source_byte_offset, r_smodel_bucket_mask, r_smodel_bucket_source_path,
    r_smodel_bucket_store_payload, r_smodel_dest_path, r_smodel_lod_is_rigid,
    r_smodel_surf_bucket_push, r_smodel_surf_type, rb_patch_static_model_cache_lock,
    rb_patch_static_model_cache_lock_for_miss, smc_cache_index_offset, smc_index,
    smc_lod_cache_spec, smc_size_class_for_verts, smodel_bucket_lists_consume,
    smodel_surf_sun_shadow_emits,
};
pub use smodel_cmd::{
    SMODEL_CACHED_CMD_MAX_BYTES, SMODEL_PRETESS_CMD_BYTES, SmodelCachedCmdPlan, SmodelCmdKind,
    SmodelPretessAlloc, SmodelPretessAllocError, smodel_cached_cmd_bytes, smodel_cached_cmd_plan,
    smodel_cmd_reserve, smodel_pretess_indices_copy, smodel_same_bank_count,
    write_smodel_cached_cmd, write_smodel_pretess_cmd,
};
pub use smodel_inst::GfxStaticModelInst;
pub use smodel_lighting::{
    GFX_MODEL_LIGHTING_PATCH_COLORS_SLOTS, GFX_MODEL_LIGHTING_PATCH_LIST_CAP,
    GFX_MODEL_LIGHTING_PATCH_SIZE, GFX_MODELLIGHT_EXTRAPOLATE, GFX_MODELLIGHT_SHOW_MISSING,
    GFX_STATIC_MODEL_DRAW_INST_CACHE_INDEX, GFX_STATIC_MODEL_DRAW_INST_FLAGS,
    GFX_STATIC_MODEL_DRAW_INST_LIGHTING_HANDLE, GFX_STATIC_MODEL_DRAW_INST_PACKED_LIGHTING,
    GFX_STATIC_MODEL_DRAW_INST_PRIMARY_LIGHT_INDEX,
    GFX_STATIC_MODEL_DRAW_INST_REFLECTION_PROBE_INDEX, GFX_STATIC_MODEL_DRAW_INST_SAMPLE_SELECTOR,
    GFX_STATIC_MODEL_DRAW_INST_SIZE, GFX_STATIC_MODEL_INST_LIGHTING_ORIGIN,
    GFX_STATIC_MODEL_INST_SIZE, MODEL_LIGHTING_WARN_CACHE_ALLOC_FAILED, SMC_CACHE_INDEX_LODS,
    SMODEL_LIGHTING_DEFER_FLAG, SMODEL_LIGHTING_FREEABLE_AGE_FRAMES,
    SMODEL_LIGHTING_RESERVED_ENTRY0_AFTER_WALK, SMODEL_LIGHTING_REUSE_MARKS_DIRTY,
    SMODEL_LIGHTING_WARN_TOO_MUCH, STATIC_MODEL_FLAG_NO_CAST_SHADOW, draw_inst_defers_lighting,
    draw_inst_lighting_handle_entry, light_grid_entry_needs_trace_bits,
    light_grid_entry_primary_light, lighting_origin_from_inst_bytes,
    model_lighting_handle_from_entry, smodel_lighting_bits_mask, smodel_lighting_msb_local_bit,
};
pub use smodel_skin::{
    SMC_UNIT_VEC_FIXED_SCALE, SMC_UNIT_VEC_OUT_W, SMC_UNIT_VEC_PACK_BIAS, SmcCachedVertLighting,
    SmcSkinError, SmcSkinSurface, local_transform_unit_vec, r_skin_cached_static_model_cmd,
    r_skin_cached_static_model_cmd_matrix, r_skin_xsurface_static_vert,
    r_skin_xsurface_static_verts, setup_transform_unit_vec,
};
pub use spot_shadow::{
    GFX_SHADOWABLE_SLOT_STRIDE, GFX_SPOT_SHADOW_CMDBUF_ROW_STRIDE, GFX_SPOT_SHADOW_RT_LARGE,
    GFX_SPOT_SHADOW_RT_SMALL, GFX_SPOT_SHADOW_VIEWPORT_LARGE, GFX_SPOT_SHADOW_VIEWPORT_SMALL,
    SPOT_SHADOW_BSP_PRETESS_CLAIM, SPOT_SHADOW_CFG_INDEX_ENTS, SPOT_SHADOW_ENT_FLAG_SKIP,
    SPOT_SHADOW_ENT_MARK_LEN, SPOT_SHADOW_ENT_SKIN_WAIT, SPOT_SHADOW_ENTITY_ORIGIN_TRACK_ENTS,
    SPOT_SHADOW_ENTITY_RELINK_THRESH_SQ, SPOT_SHADOW_ENTNUM_MASK, SPOT_SHADOW_HALF,
    SPOT_SHADOW_LINK_ENTITY_PAD, SPOT_SHADOW_LOOKUP_AFTER_VIEWPARMS, SPOT_SHADOW_PIXEL_SCALE,
    SPOT_SHADOW_PIXEL_SCALE_NEG, SPOT_SHADOW_PLAYER_LINK_CG_FLAGS,
    SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE, SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE, SPOT_SHADOW_ZNEAR_ADD,
    SpotShadowAddCaster, SpotShadowCasterIn, SpotShadowCasterKind, SpotShadowEntAdmit,
    SpotShadowOccupancyError, SpotShadowSceneSlot, SpotShadowSlotPlan, SpotShadowViewAxis,
    SpotShadowViewParms, spot_shadow_175d0_pose_ok, spot_shadow_17410_lod_ok,
    spot_shadow_add_caster, spot_shadow_consume_order, spot_shadow_dyn_brush_tess_admits,
    spot_shadow_dyn_brush_vis_bit_index, spot_shadow_efd0_partition_vis, spot_shadow_efd0_spot_vis,
    spot_shadow_efd0_zero_all, spot_shadow_emit_walks_casters, spot_shadow_ent_admit,
    spot_shadow_ent_cmd_mark, spot_shadow_ent_cmd_pose_acquire, spot_shadow_ent_flags_allow,
    spot_shadow_ent_marked, spot_shadow_entity_origin_track_index,
    spot_shadow_entity_should_relink, spot_shadow_entnum, spot_shadow_fill_occupancy,
    spot_shadow_fill_scene_dobj_vis, spot_shadow_fill_scene_model_vis,
    spot_shadow_filter_smodel_ids, spot_shadow_gpu_extent, spot_shadow_gpu_targets,
    spot_shadow_link_dyn_brush_vis, spot_shadow_link_entity, spot_shadow_link_extra_entity,
    spot_shadow_link_extra_player, spot_shadow_link_player, spot_shadow_link_primary_vis,
    spot_shadow_lookup_matrix, spot_shadow_mark_ent, spot_shadow_near_bias,
    spot_shadow_overlay_atlas_y, spot_shadow_overlay_matrix, spot_shadow_packed_lists_ready,
    spot_shadow_player_vis_link_allows, spot_shadow_primary_vis_bit,
    spot_shadow_primary_vis_bit_index, spot_shadow_primary_vis_word_count,
    spot_shadow_primary_vis_write, spot_shadow_scene_dobj_vis, spot_shadow_scene_dobj_vis_write,
    spot_shadow_scene_model_vis, spot_shadow_scene_model_vis_write, spot_shadow_slot_plan,
    spot_shadow_smodel_casts, spot_shadow_take_slot, spot_shadow_tan_half_fov,
    spot_shadow_tess_bsp_surfs, spot_shadow_tess_scene_dobj_indices,
    spot_shadow_tess_scene_model_indices, spot_shadow_tess_smodel_ids, spot_shadow_view_axis,
    spot_shadow_view_parms, spot_shadow_walk_casters, spot_shadow_xmodel_is_caster,
    spot_shadow_z_near,
};
pub use spot_shadow_choose::{
    GFX_WORLD_LIGHT_REGION_OFF, SM_ENABLE_DEFAULT, SM_SUN_ENABLE_DEFAULT,
    SPOT_SHADOW_CANDIDATE_STACK, SPOT_SHADOW_DIST_CULL_SAMPLE_SCALE, SPOT_SHADOW_FADE_DROP,
    SPOT_SHADOW_HISTORY_ENTRY_STRIDE, SPOT_SHADOW_HISTORY_STRIDE, SPOT_SHADOW_MSEC,
    SPOT_SHADOW_SCORE_LUMA, SPOT_SHADOW_SCORE_ONE, SPOT_SHADOW_SM_LIGHT_CAP, SPOT_SHADOW_TIME_WRAP,
    SPOT_SHADOW_VIEW_LIGHT_INDEX_BIAS, SpotShadowCandidateSkip, SpotShadowChoose,
    SpotShadowChooseDvars, SpotShadowEmit, SpotShadowEmittedSlot, SpotShadowFrontend,
    SpotShadowHistory, SpotShadowHistoryEntry, SpotShadowUsedForce, SpotShadowableLight,
    clear_used_bit, clear_used_bits_1_through_sun, generate_clears_used_bits_through_sun,
    generate_primary_light_copy_bytes, generate_runs_choose, set_used_bit,
    spot_shadow_add_candidate, spot_shadow_choose, spot_shadow_dist_cull,
    spot_shadow_dist_cull_extra, spot_shadow_emit_from_history, spot_shadow_emit_frontend,
    spot_shadow_fade_delta, spot_shadow_fade_out_history, spot_shadow_history_add,
    spot_shadow_score, spot_shadow_sun_in_front, used_bit,
};
pub use trace::{
    LIGHT_GRID_TRACE_ALWAYS_ALLOW_TYPE, light_grid_trace_allows, light_grid_trace_corner_pos,
    light_grid_trace_quantize_axis,
};
pub use viewmodel_origin::viewmodel_lighting_origin;
