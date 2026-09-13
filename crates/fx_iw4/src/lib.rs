#![no_std]
#![forbid(unsafe_code)]

mod angles;
mod at_rest;
mod atlas;
mod beam;
mod bolt;
mod collide;
mod cull;
mod draw;
mod effect_def;
mod elem;
mod elem_def;
mod emit;
mod flags;
mod glass;
mod glass_shatter;
mod gravity;
mod impact;
mod integrate;
mod laser;
mod life;
mod lighting;
mod orient;
mod orientation;
mod origin;
mod particle_cloud;
mod pool;
mod post_light;
mod quat;
mod random;
mod rotate_axis;
mod rotation;
mod sort;
mod spark_fountain;
mod spawn;
mod sprite_quad;
mod status;
mod system;
mod tail_draw;
mod trail;
mod trail_def;
mod trail_runtime;
mod vec;
mod velocity;
mod view;
mod vis_blocker;
mod visual;

pub use angles::{
    fx_angles_to_axis_radians, fx_get_elem_angles_axis, fx_mat3_mul, fx_sample_elem_angles,
};

pub use at_rest::{
    FX_AT_REST_BIAS, FX_AT_REST_SCALE, FX_ON_GROUND_NORMAL_Z, FX_RECIP_255_AT_REST,
    fx_get_at_rest_fraction, fx_msec_for_sampling_axis,
};
pub use atlas::{
    FX_ELEM_ATLAS_OFF, FX_ELEM_ATLAS_SIZE, FxSpriteAtlasUv, fx_sprite_atlas_cell,
    fx_sprite_atlas_uv,
};
pub use beam::{
    FX_BEAM_ADD_CAP, FX_BEAM_CLIP_DZ_AND, FX_BEAM_CLIP_PLANE_Z1, FX_BEAM_CLIP_T_MAX,
    FX_BEAM_CLIP_W_BIAS, FX_BEAM_FLAT_DELTA_MIN_LEN_SQ, FX_BEAM_MAX_SEGMENTS,
    FX_BEAM_VIEWER_SHUFFLE0, FX_BEAM_VIEWER_SHUFFLE1, FX_BEAM_VIEWER_SHUFFLE2,
    FX_BEAM_VIEWER_SHUFFLE3, FX_BEAM_WIGGLE, FX_CREATE_CLIP_ZNEAR, FX_INFINITE_PERSPECTIVE_K,
    FX_TRACER_FIRST_PERSON_MAX_WIDTH, FX_TRACER_MIN_DIST, FxBeamTess, FxBeamVert,
    fx_beam_clip_against_plane, fx_beam_clip_pair_z_planes, fx_beam_color_rgba, fx_beam_flat_basis,
    fx_beam_generate_verts, fx_beam_index_count, fx_beam_perp_from_flat, fx_beam_sample_color,
    fx_beam_segment_count, fx_beam_vert_count, fx_beam_viewer_pshufb, fx_create_clip_matrix,
    fx_generate_beam_get_flat_delta, fx_infinite_perspective_matrix, fx_mat4_mul, fx_mat4_mul_vec4,
    fx_matrix_for_viewer,
};
pub use bolt::{
    FX_BOLT_BONE_MASK, FX_BOLT_BONE_SHIFT, FX_BOLT_CENTITY_LIMIT, FX_BOLT_CENTITY_STRIDE,
    FX_BOLT_CENTITY_TELEPORT_MASK, FX_BOLT_DOBJ_MASK, FX_BOLT_FREE_NONE, FX_BOLT_HANDLE_NONE,
    FX_BOLT_INIT_LAST, FX_BOLT_LOST_OR, FX_BOLT_PARENT_DWORDS, FX_BOLT_PARENT_IDENTITY_QUAT,
    FX_BOLT_RECORD_CAPACITY, FX_BOLT_RECORD_OFF_PACKED, FX_BOLT_RECORD_OFF_PARENT,
    FX_BOLT_RECORD_STRIDE, FX_BOLT_TELEPORT_SHIFT, FX_BOLT_VIEWMODEL_DOBJ_BASE,
    FX_QUAT_TRANSFORM_SCALE, FxGetBoneOrientationRefuse, FxGetBoneOrientationRoute,
    FxUpdateEffectBolt, fx_begin_iterating_over_effects_exclusive, fx_bolt_alloc, fx_bolt_bone,
    fx_bolt_centity_teleport_for_compare, fx_bolt_compose_orientation, fx_bolt_dobj,
    fx_bolt_handle_is_none, fx_bolt_init_next_index, fx_bolt_init_parent_orientation,
    fx_bolt_mark_lost, fx_bolt_pack, fx_bolt_record_index_from_byte_delta,
    fx_bolt_spawn_teleport_bit, fx_bolt_teleport_bit, fx_end_iterating_over_effects,
    fx_end_iterating_runs_gc, fx_get_bone_orientation_route, fx_quat_mul, fx_quat_transform_vec,
    fx_stop_effect_has_owned, fx_stop_effect_non_recursive_allows, fx_update_effect_bolt,
};
pub use collide::{
    FX_COLLIDE_SUBSTEP_MS, FX_COLLISION_REFLECT_SCALE, FX_IMPACT_CHILD_MIN_SPEED_SQ, FX_TRACE_MASK,
    FX_TRACE_MASK_ITEM_CLIP, FxCollideSubstep, fx_collide_marks_at_rest, fx_collide_on_ground,
    fx_collide_substep_schedule, fx_collision_reflect_base_vel_delta, fx_impact_child_speed_allows,
    fx_sample_reflection_factor, fx_trace_mask,
};
pub use cull::{
    FX_ELEM_FLAG_CULL_DRAW_5_PLANES, fx_cull_cloud, fx_cull_cloud_plane_count,
    fx_cull_cloud_radius, fx_cull_elem_for_spawn_allows, fx_cull_elem_light, fx_cull_sphere,
    fx_elem_light_color_bgr,
};
pub use draw::{FX_DRAW_ELEM_HANDLER_PRESENT, FxElemType, fx_draw_elem_handler_present};
pub use effect_def::FxEffectDef;
pub use elem::{FX_ELEM_AT_REST_NONE, FxElem};
pub use elem_def::FxElemDef;
pub use emit::{
    FX_ELEM_EMIT_ORIENT_AXIS, FX_EMIT_CRT_RAND_SCALE, FX_EMIT_DIST_TO_RESIDUAL,
    FX_EMIT_RESIDUAL_ROUND_BIAS, FX_EMIT_RESIDUAL_TO_DIST, FX_EMIT_SPAWN_CAP, FxEmitSchedule,
    FxEmitSpawn, fx_emit_dist_range, fx_emit_lerp_origin, fx_emit_pack_residual,
    fx_emit_unpack_residual_start, fx_process_emitting_schedule,
};
pub use flags::{
    FX_ELEM_DIE_ON_TOUCH, FX_ELEM_RUN_MASK, FX_ELEM_RUN_NONE_ORIGIN,
    FX_ELEM_RUN_RELATIVE_TO_EFFECT, FX_ELEM_RUN_RELATIVE_TO_OFFSET, FX_ELEM_RUNNER_USES_RAND_ROT,
    FX_ELEM_SPAWN_FRUSTUM_CULL, FX_ELEM_UPDATE_HAS_VEL_GRAPH, FX_ELEM_USE_COLLISION,
    FX_ELEM_USE_MODEL_PHYSICS, FX_ELEM_VEL_LOCAL, FX_ELEM_VEL_WORLD, fx_elem_dies_on_touch,
    fx_elem_run_mode, fx_elem_skips_position_update, fx_elem_spawn_frustum_cull,
    fx_elem_update_has_velocity_graph, fx_elem_uses_collision, fx_elem_uses_vel_local,
    fx_elem_uses_vel_world,
};
pub use glass::{
    FX_GLASS_AVEL_HALF, FX_GLASS_DEF, FX_GLASS_DYN_AVEL, FX_GLASS_DYN_FALL_TIME,
    FX_GLASS_DYN_PHYS_OBJ, FX_GLASS_DYN_VEL, FX_GLASS_FALL_GRAVITY, FX_GLASS_FALL_TIME_NEVER,
    FX_GLASS_FREE_SENTINEL, FX_GLASS_GEOMETRY_DATA, FX_GLASS_INIT_AREA_X2, FX_GLASS_INIT_DEF_INDEX,
    FX_GLASS_INIT_FAN_DATA_COUNT, FX_GLASS_INIT_ORIGIN, FX_GLASS_INIT_PIECE_STATE,
    FX_GLASS_INIT_PLACE_BYTES, FX_GLASS_INIT_SUPPORT_MASK, FX_GLASS_INIT_TEXCOORD,
    FX_GLASS_INIT_VERT_COUNT, FX_GLASS_LINK_ORG_FREE, FX_GLASS_MSEC_TO_SEC,
    FX_GLASS_PIECE_DYNAMICS, FX_GLASS_PIECE_PLACE, FX_GLASS_PIECE_STATE, FX_GLASS_STATE_AREA_X2,
    FX_GLASS_STATE_CRACK_DATA_COUNT, FX_GLASS_STATE_DEF_INDEX, FX_GLASS_STATE_FAN_DATA_COUNT,
    FX_GLASS_STATE_FLAGS, FX_GLASS_STATE_GEO_DATA_START, FX_GLASS_STATE_HOLE_DATA_COUNT,
    FX_GLASS_STATE_INIT_INDEX, FX_GLASS_STATE_SUPPORT_MASK, FX_GLASS_STATE_VERT_COUNT,
    FX_GLASS_TRACE_INTERVAL_MSEC, FX_GLASS_VERT_SCALE, FxGlassIntactVert, FxGlassResetPiece,
    fx_glass_alloc_piece, fx_glass_ballistic_origin, fx_glass_clear_in_use,
    fx_glass_def_color_rgba, fx_glass_def_tex_vecs, fx_glass_dynamics_avel,
    fx_glass_dynamics_fall_time, fx_glass_dynamics_init_row, fx_glass_dynamics_phys_obj,
    fx_glass_dynamics_software_launch, fx_glass_dynamics_vel, fx_glass_free_piece,
    fx_glass_geo_vert, fx_glass_in_use_mask, fx_glass_in_use_word, fx_glass_init_origin,
    fx_glass_intact_fan_indices, fx_glass_intact_verts, fx_glass_is_in_use,
    fx_glass_last_trace_tick, fx_glass_pack_geo_vert, fx_glass_place_next_free,
    fx_glass_place_origin, fx_glass_place_quat, fx_glass_place_set_next_free,
    fx_glass_place_set_origin, fx_glass_place_set_quat, fx_glass_reset_copy_geo,
    fx_glass_reset_copy_piece, fx_glass_reset_free_list, fx_glass_set_in_use,
    fx_glass_software_rotate_quat, fx_glass_software_trace_due, fx_glass_state_area_x2,
    fx_glass_state_def_index, fx_glass_state_fan_count, fx_glass_state_flags,
    fx_glass_state_geo_start, fx_glass_state_init_index, fx_glass_state_set_flags,
    fx_glass_state_set_support_mask, fx_glass_state_support_mask, fx_glass_state_vert_count,
    fx_glass_trace_phase, fx_unit_quat_to_axis,
};
pub use glass_shatter::{
    FX_GLASS_ANGULAR_VEL_MAX, FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_FRINGE_MAXCOVERAGE,
    FX_GLASS_LINEAR_VEL_MAX, FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_SHATTER_BRANCH_SCALE,
    FX_GLASS_SHATTER_TWO_PI, FX_GLASS_SPLIT_MAX_CHILDREN, FX_GLASS_SPLIT_MAX_VERTS,
    FX_GLASS_STATE_FLAG_CHILD_CLEAR, FX_GLASS_STATE_FLAG_SHATTERED, FxGlassSplitLoop,
    fx_glass_fringe_cap, fx_glass_fringe_prune_knock_order, fx_glass_interior_angle_step,
    fx_glass_interior_branch_count, fx_glass_lerp_range, fx_glass_loop_area_x2,
    fx_glass_piece_speed_scale, fx_glass_point_in_convex, fx_glass_radial_split,
    fx_glass_shatter_rand,
};
pub use gravity::{
    FX_GRAVITY, fx_elem_gravity_accel_z, fx_elem_gravity_accel_z_sampled,
    fx_sample_gravity_authored,
};
pub use impact::{
    FX_IMPACT_ENTRY_SIZE, FX_IMPACT_EXIT_SURFACE_FLAG, FX_IMPACT_FLESH_COUNT,
    FX_IMPACT_NONFLESH_COUNT, FX_IMPACT_TABLE_ROWS, FX_SURF_TYPE_FLESH, FxImpactEntry,
    FxImpactTable, ImpactType, fx_flesh_effect_index, fx_flesh_hit_flags,
    fx_impact_entry_cell_offset, fx_impact_table_row, fx_surface_type_index,
};
pub use integrate::{
    FX_SPARKCLOUD_HISTORY_FAR_MS, FX_SPARKCLOUD_HISTORY_NEAR_MS, FxElemVec3Range,
    fx_integrate_velocity_graph, fx_particle_cloud_cell_count, fx_sample_vel_graph_at_age,
    fx_sparkcloud_history_lookback_ms,
};
pub use laser::{
    FX_LASER_POST_LIGHT_COLOR, FX_LASER_POST_LIGHT_HALF, FX_LASER_POST_LIGHT_MATERIAL,
    FX_LASER_POST_LIGHT_MIN_SPAN, FX_LASER_POST_LIGHT_PAD, FX_LASER_RADIUS_DIST_BIAS,
    FX_LASER_RADIUS_DIST_SCALE, FX_LASER_SURF_EXTRA_END, FX_LASER_TAG, FX_LASER_TRACE_BOUNDS,
    fx_laser_brush_trace_allows, fx_laser_from_brush_trace, fx_laser_from_tag_orientation,
    fx_laser_point_on_ray, fx_laser_post_light, fx_laser_post_light_allows,
    fx_laser_post_light_end_t, fx_laser_post_light_span, fx_laser_radius_from_dist,
};
pub use life::{
    fx_life_span_range_from_bytes, fx_sample_life_span_msec, fx_trail_elem_base_vel_z_pack,
    fx_trail_elem_keep, fx_trail_elem_norm_ages,
};
pub use lighting::{
    FX_LIGHTING_FRAC_CHANNEL_MAP, fx_apply_lighting_frac_bgra, fx_apply_lighting_frac_channel,
    fx_effect_def_needs_lighting_sample, fx_elem_uses_lighting_frac,
};
pub use orient::{fx_orientation_pos_from_world, fx_orientation_pos_to_world};
pub use orientation::{
    FX_ORIENT_UP_DOT_GATE, FxOrientFrame, FxOrientSpawnParams, FxOrientation, fx_get_orientation,
};
pub use origin::{
    FX_ELEM_SPAWN_OFFSET_CYLINDER, FX_ELEM_SPAWN_OFFSET_MASK, FX_ELEM_SPAWN_OFFSET_SPHERE,
    FX_ELEM_SPAWN_RELATIVE, FX_TWO_PI, FxSpawnOffsetMode, fx_apply_spawn_origin,
    fx_elem_spawn_offset_mode, fx_elem_spawn_relative, fx_offset_spawn_origin, fx_random_dir,
    fx_sample_float_range, fx_sample_spawn_origin_offset, fx_spawn_origin_world,
    fx_world_delta_to_local,
};
pub use particle_cloud::{
    FX_CODE_FOUNTAIN_PARM0, FX_CODE_FOUNTAIN_PARM1, FX_CODE_PARTICLE_CLOUD_COLOR,
    FX_CODE_PARTICLE_CLOUD_MATRIX0, FX_CODE_SPARK_COLOR0, FX_PARTICLE_CLOUD_CELL_ORIGIN,
    FX_PARTICLE_CLOUD_CELL_SCALE_XY, FX_PARTICLE_CLOUD_CELL_SCALE_Z,
    FX_PARTICLE_CLOUD_CRT_RAND_MAX, FX_PARTICLE_CLOUD_FLAG_SPARK, FX_PARTICLE_CLOUD_GRID_X,
    FX_PARTICLE_CLOUD_GRID_Y, FX_PARTICLE_CLOUD_GRID_Z, FX_PARTICLE_CLOUD_INDICES_PER_CELL,
    FX_PARTICLE_CLOUD_PRIM_TYPE, FX_PARTICLE_CLOUD_PRIMS_PER_CELL, FX_PARTICLE_CLOUD_QUAD_INDICES,
    FX_PARTICLE_CLOUD_TEMPLATE_CELLS, FX_PARTICLE_CLOUD_UV, FX_PARTICLE_CLOUD_VERT_DECL_TYPE,
    FX_PARTICLE_CLOUD_VERTS_PER_CELL, FX_PARTICLE_FOUNTAIN_AGE_BIAS_MS,
    FX_PARTICLE_FOUNTAIN_AGE_SCALE, FX_PARTICLE_SPARK_INDICES, FX_PARTICLE_SPARK_INDICES_PER_CELL,
    FX_PARTICLE_SPARK_PRIMS_PER_CELL, FX_PARTICLE_SPARK_UV, FX_PARTICLE_SPARK_VERTS_PER_CELL,
    FX_SPARK_CLOUD_HANDLE_NONE, FX_SPARK_CLOUD_HISTORY_CAPACITY, FX_SPARK_CLOUD_HISTORY_STRIDE,
    FX_SPARK_CLOUD_SAMPLE_MASK, FX_SPARK_CLOUD_SAMPLE_RING, FX_SPARKCLOUD_HISTORY_MIN_DT_MS,
    FX_SPARKCLOUD_UV_V_1_3, FX_SPARKCLOUD_UV_V_2_3, FxSparkCloudHistory, GFX_PARTICLE_CLOUD_STRIDE,
    GFX_POS_TEX_VERTEX_STRIDE, GfxParticleCloud, GfxPosTexVertex, MSVCRT_HOLDRAND_DEFAULT,
    fx_build_cloud, fx_empty_particle_cloud, fx_pack_gfx_color, fx_particle_cloud_cell_indices,
    fx_particle_cloud_cell_radius_sq, fx_particle_cloud_cell_verts, fx_particle_cloud_cell_xyz,
    fx_particle_cloud_color_const, fx_particle_cloud_compare_cell_radius,
    fx_particle_cloud_draw_cell_count, fx_particle_cloud_draw_counts,
    fx_particle_cloud_matrix_diag, fx_particle_cloud_particle_id, fx_particle_fountain_parm0,
    fx_particle_spark_cell_indices, fx_particle_spark_cell_verts, fx_spark_cloud_addr,
    fx_spark_cloud_handle_for_slot, fx_spark_cloud_handle_from_ptr_delta,
    fx_sparkcloud_build_triplet, fx_sparkcloud_fill_sample, fx_sparkcloud_history_advance,
    fx_sparkcloud_history_should_advance, fx_sparkcloud_lerp_sample, fx_sparkcloud_tent_weights,
    gfx_pos_tex_vertex_bytes, msvcrt_rand, msvcrt_rand01,
};
pub use pool::{
    FX_BUFFERS_OFF_EFFECTS, FX_BUFFERS_OFF_ELEMS, FX_BUFFERS_OFF_SPARK_CLOUD,
    FX_BUFFERS_POOL_STRIDE, FX_EFFECT_HANDLE_RING_MASK, FX_EFFECT_HANDLE_RING_SIZE,
    FX_EFFECT_POOL_BYTES, FX_EFFECT_POOL_CAPACITY, FX_EFFECT_SLOT_SIZE, FX_ELEM_POOL_CAPACITY,
    FX_ELEM_RUNTIME_STRIDE, FX_ENTITYNUM_WORLD, FX_PLAY_BOLT_NONE, FX_RAND_TABLE_MOD,
    FX_SPAWN_BOLT_NONE, FX_SPOT_LIGHT_LIMIT, FX_STATUS_UNIQUE_DONE, FX_STATUS_UNIQUE_MASK,
    FX_SYSTEM_STRIDE, FX_TRAIL_ELEM_POOL_CAPACITY, FX_TRAIL_ELEM_RUNTIME_STRIDE,
    FX_TRAIL_POOL_CAPACITY, FX_TRAIL_RUNTIME_STRIDE, FX_WARN_EFFECT_LIMIT, FX_WARN_ELEM_LIMIT,
    FX_WARN_SPARK_CLOUD_LIMIT, FX_WARN_TOO_MANY_SPOTLIGHTS, fx_effect_addr,
    fx_effect_byte_offset_from_handle, fx_effect_handle_for_slot,
    fx_effect_handle_from_byte_offset, fx_elem_addr, fx_elem_handle_from_ptr_delta, fx_trail_addr,
    fx_trail_elem_addr, fx_trail_elem_handle_for_slot, fx_trail_handle_for_slot,
    fx_trail_handle_from_byte_offset,
};
pub use post_light::{
    FX_POST_LIGHT_ADD_CAP, FX_POST_LIGHT_ANGLE_STEP, FX_POST_LIGHT_ARG_COUNT,
    FX_POST_LIGHT_DRAW_NAME, FX_POST_LIGHT_INDEX_COUNT, FX_POST_LIGHT_MIN_DELTA_SQ,
    FX_POST_LIGHT_POLYGON_RADIUS_GROW, FX_POST_LIGHT_STRIDE, FX_POST_LIGHT_VERT_COUNT, FxPostLight,
    FxPostLightTess, fx_post_light_add_allows, fx_post_light_generate_verts,
    fx_post_light_pack_vert,
};
pub use quat::{fx_axis_to_quat, fx_quat_nlerp, fx_quat_normalize};
pub use random::{
    FX_RAND_CH_ANG_VEL_PITCH, FX_RAND_CH_ANG_VEL_ROLL, FX_RAND_CH_ANG_VEL_YAW, FX_RAND_CH_ATLAS,
    FX_RAND_CH_COLOR, FX_RAND_CH_DELAY, FX_RAND_CH_EMIT_DIST, FX_RAND_CH_GRAVITY,
    FX_RAND_CH_INITIAL_ROTATION, FX_RAND_CH_LIFE, FX_RAND_CH_ONESHOT_COUNT, FX_RAND_CH_REFLECTION,
    FX_RAND_CH_ROTATION_DELTA, FX_RAND_CH_SCALE, FX_RAND_CH_SIZE0, FX_RAND_CH_SPAWN_ANGLES_PITCH,
    FX_RAND_CH_SPAWN_ANGLES_ROLL, FX_RAND_CH_SPAWN_ANGLES_YAW, FX_RAND_CH_SPAWN_OFFSET_HEIGHT,
    FX_RAND_CH_SPAWN_OFFSET_RADIUS, FX_RAND_CH_SPAWN_OFFSET_YAW, FX_RAND_CH_SPAWN_ORIGIN_X,
    FX_RAND_CH_SPAWN_ORIGIN_Y, FX_RAND_CH_SPAWN_ORIGIN_Z, FX_RAND_CH_VISUAL,
    FX_RANDOM_TABLE_FLOATS, fx_effect_random_seed_from_msec, fx_effect_random_seed_from_rand,
    fx_elem_random_seed, fx_elem_visual_index, fx_random_table_f32, fx_random_table_u16,
    fx_trail_random_seed,
};
pub use rotate_axis::{
    FX_DEG_TO_RAD, FX_RAD_TO_DEG, FX_RAND_ROT_DEGREES, fx_impact_mark_axis,
    fx_randomly_rotate_axis, fx_rotate_point_around_vector, fx_runner_rand_rot_degrees,
};
pub use rotation::{
    FX_RECIP_255, FX_ROT_TIME_EASE_LIMIT_MS, FX_ROT_TIME_EASE_RECIP, FX_ROT_TIME_MAX_LEAD_MS,
    fx_clamp_elem_rotation_time,
};
pub use sort::{FxInsertSortElem, fx_existing_elem_sorts_before_new, fx_sort_dist_to_cam_sq};
pub use spark_fountain::{
    FX_ELEM_FLAG_FOUNTAIN_WRITE_SPARK_N, FX_SPARK_FOUNTAIN_BALLISTIC_HALF,
    FX_SPARK_FOUNTAIN_BOOST_HALF_PI, FX_SPARK_FOUNTAIN_CELL_OFF_ORIGIN,
    FX_SPARK_FOUNTAIN_CELL_OFF_TIMES, FX_SPARK_FOUNTAIN_CELL_OFF_VEL, FX_SPARK_FOUNTAIN_CELLS,
    FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY, FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_KEYFRAME, FX_SPARK_FOUNTAIN_CLUSTER_OFF_MESH_IDX,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_READY, FX_SPARK_FOUNTAIN_CLUSTER_OFF_SPARK_N,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_WRITE, FX_SPARK_FOUNTAIN_CLUSTER_STRIDE,
    FX_SPARK_FOUNTAIN_CONE_DOT_GATE, FX_SPARK_FOUNTAIN_CONE_FLIP,
    FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT, FX_SPARK_FOUNTAIN_DEF_SIZE,
    FX_SPARK_FOUNTAIN_HANDLE_NONE, FX_SPARK_FOUNTAIN_HIT_TIME_EPS, FX_SPARK_FOUNTAIN_HIT_TIME_FOUR,
    FX_SPARK_FOUNTAIN_INDICES_PER_CELL, FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET,
    FX_SPARK_FOUNTAIN_INTEGRATE_CELLS, FX_SPARK_FOUNTAIN_KEYFRAME_STEP,
    FX_SPARK_FOUNTAIN_KEYFRAME_STRIDE, FX_SPARK_FOUNTAIN_MESH_CAPACITY,
    FX_SPARK_FOUNTAIN_MESH_STRIDE, FX_SPARK_FOUNTAIN_RAND_CUBE, FX_SPARK_FOUNTAIN_SAMPLES,
    FX_SPARK_FOUNTAIN_TIME_SENTINEL, FX_SPARK_FOUNTAIN_TRACE_BOUNDS, FX_SPARK_FOUNTAIN_TRACE_GROW,
    FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL, FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL,
    FX_SPARK_FOUNTAIN_TRACE_MASK, FX_SPARK_FOUNTAIN_TRACE_SUBSTEP,
    FX_SPARK_FOUNTAIN_VERTS_PER_SPARK, FxSparkFountainTrace, R_PARTICLE_CLOUD_CUSTOM_CAP,
    fx_spark_fountain_accel_from_gravity, fx_spark_fountain_atlas_uv, fx_spark_fountain_ballistic,
    fx_spark_fountain_boost, fx_spark_fountain_bounce_vel, fx_spark_fountain_cell_indices,
    fx_spark_fountain_cell_verts, fx_spark_fountain_cluster_draw_allows,
    fx_spark_fountain_cone_dir, fx_spark_fountain_cone_is_isotropic,
    fx_spark_fountain_def_allows_draw, fx_spark_fountain_draw_clouds_allows,
    fx_spark_fountain_generate_ribbon, fx_spark_fountain_handle_for_slot,
    fx_spark_fountain_hit_time, fx_spark_fountain_hit_time_abs, fx_spark_fountain_index_count,
    fx_spark_fountain_integrate_cell, fx_spark_fountain_integrate_cell_begin,
    fx_spark_fountain_integrate_miss_cell, fx_spark_fountain_isotropic_dir,
    fx_spark_fountain_mark_ready, fx_spark_fountain_prim_count, fx_spark_fountain_reserve_verts,
    fx_spark_fountain_same_sample_ribbon, fx_spark_fountain_sample_window,
    fx_spark_fountain_slot_for_handle, fx_spark_fountain_spark_n_clamped, fx_spark_fountain_speed,
    fx_spark_fountain_spray_dir, fx_spark_fountain_trace_start,
    fx_spark_fountain_trace_until_miss_or_hit, fx_spark_fountain_update_keyframe_cursor,
    fx_spark_fountain_vel_at_time, fx_spark_fountain_wrap_loop_time,
    r_add_particle_cloud_custom_allows, r_reserve_particle_cloud_verts_allows,
};
pub use spawn::{
    FX_ELEM_TYPE_SPARK_CLOUD, FX_ELEM_TYPE_SPARK_FOUNTAIN, FX_ELEM_TYPE_TRAIL, FxLoopingSpawn,
    FxLoopingSpawnSchedule, fx_looping_catchup_begin, fx_looping_spawn_schedule,
    fx_sample_oneshot_spawn_count, fx_spawn_def_from_bytes, fx_spawn_effect_status,
};
pub use sprite_quad::{
    FX_SPRITE_QUAD_INDICES, FX_SPRITE_QUAD_LOCAL_XY, FX_SPRITE_QUAD_UV_FULL, fx_sprite_quad_indices,
};
pub use status::{
    FX_STATUS_DEFER_UPDATE, FX_STATUS_HAS_PENDING_LOOP_ELEMS, FX_STATUS_IS_LOCKED,
    FX_STATUS_IS_LOCKED_MASK, FX_STATUS_OWNED_EFFECTS_MASK, FX_STATUS_REF_COUNT_MASK,
    FX_STATUS_REF_COUNT_MASK_IW4, fx_status_is_unique_done,
};
pub use system::{FxEffect, FxSystem};
pub use tail_draw::{fx_tail_anchor_origin, fx_tail_sprite_axes, fx_tail_sprite_full_extent};
pub use trail::{
    FX_CODE_MESH_BINORMAL_SIGN, FX_CODE_MESH_VERTEX_STRIDE, FX_TRAIL_BASIS_SCALE,
    FX_TRAIL_NORMAL_BIAS, FX_TRAIL_NORMAL_SCALE, FX_TRAIL_TANGENT_PACKED, FxTrailEmittedVert,
    FxTrailSegmentDrawState, fx_compress_basis_from_axis, fx_compress_basis_from_quat,
    fx_pack_code_mesh_vertex, fx_trail_compress_basis, fx_trail_compress_char, fx_trail_compute_u,
    fx_trail_emit_index_quad, fx_trail_emit_segment_vert, fx_trail_emit_segment_verts,
    fx_trail_index_quad_tris, fx_trail_pack_normal, fx_trail_pack_texcoord,
    fx_trail_uncompress_basis,
};
pub use trail_def::{FxTrailDef, FxTrailVertex};
pub use trail_runtime::{
    FX_TRAIL_SPLIT_UNIT, FxTrail, FxTrailElem, FxTrailSplit, fx_trail_split_interpolant_msec,
    fx_trail_split_interpolant_t, fx_trail_split_lerp_axis, fx_trail_split_lerp_origin,
    fx_trail_split_lerp_quat, fx_trail_split_skips_update, fx_trail_split_window,
};
pub use vec::{
    FX_PERP_VECTOR_UNIT, fx_effect_orient_arc, fx_perpendicular_vector, fx_vec3_distance,
    fx_vec3_length_sq, fx_vec3_normalize, fx_vector_vectors,
};
pub use velocity::{FX_VEL_AT_TIME_SCALE, fx_get_velocity_at_time};
pub use view::{
    FX_EFFECT_DEF_SIZE, FX_ELEM_DEF_STRIDE, FxEffectDefView, FxElemDefView, fx_effect_def_view,
    fx_elem_def_gravity_accel_z, fx_elem_def_view, fx_elem_def_view_x64,
};
pub use vis_blocker::{
    FX_CLIENT_VISIBILITY_THRESHOLD, FX_DISTANCE_FADE_BIAS, FX_DISTANCE_FADE_SCALE,
    FX_ELEM_FLAG_VIS_BLOCKER, FX_VIS_BLOCKER_BYTE_TO_UNIT, FX_VIS_BLOCKER_PARAM3_SCALE,
    FX_VIS_BLOCKER_PARAM4_INV_SCALE, FX_VIS_BLOCKER_REC_STRIDE, FX_VIS_BLOCKER_SLOT_CAP,
    FX_VIS_MIN_TRACE_DIST_DEFAULT, FxVisBlockerBuf, FxVisBlockerRec, fx_distance_fade_range,
    fx_evaluate_distance_fade, fx_get_client_visibility, fx_vis_blocker_add,
    fx_vis_blocker_add_prepared, fx_vis_blocker_generate_verts, fx_vis_blocker_param4,
};
pub use visual::{
    FX_ELEM_VIS_STATE_SAMPLE_SIZE, FX_ELEM_VISUAL_STATE_SIZE, FX_VIS_COLOR_OFF,
    FX_VIS_ROT_DELTA_OFF, FX_VIS_ROT_TOTAL_OFF, FX_VIS_SCALE_OFF, FX_VIS_SIZE0_OFF,
    FX_VIS_SIZE1_OFF, fx_elem_norm_time, fx_evaluate_color_bgra, fx_evaluate_rotation_total,
    fx_evaluate_scale, fx_evaluate_size0, fx_evaluate_size1, fx_evaluate_vis_alpha,
    fx_integrate_rotation_from_zero, fx_setup_visual_sample_point,
};
