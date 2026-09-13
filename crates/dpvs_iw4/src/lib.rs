#![no_std]
#![forbid(unsafe_code)]

mod aabb;
mod admit;
mod bsp_drawsurf;
mod cell;
mod cell_caster;
mod chop;
mod clip;
mod convex_hull;
mod drawsurf;
mod dynent;
mod emit;
mod material_key;
mod portal;
mod portal_heap;
mod scene_ent;
mod scene_ent_bounds;
mod scene_ent_skin;
mod sort_key_uses;
mod stats;
mod sun_shadow_bsp;
mod sun_shadow_smodel;
mod vis;
mod walk;
mod xmodel_lod;

pub use aabb::{
    AABB_NODE_STRIDE, AabbCullStats, AabbNodeView, AabbSphereBits, AabbTreeCull, Bounds,
    DpvsVisData, MAX_CLIP_PLANES, SkyCullStats, aabb_tree_set_sorted_span_bits,
    add_aabb_tree_surfaces_in_frustum, add_sky_surfaces_dpvs, admit_cell_root_span, bounds_culled,
    bounds_from_origin_axis,
};
pub use admit::{
    portal_admits_eye, projected_winding_contains_coplanar_point, vec3_projection_coords,
};
pub use bsp_drawsurf::{
    BspDrawSurfCensus, BspDrawSurfKind, BspDrawSurfRanges, BspDrawSurfRun, BspSurfaceDrawFields,
    GfxSurfaceDrawFields, add_bsp_draw_surfs_camera, bsp_draw_surf_run_continues,
    bsp_draw_surf_setup_key,
};
pub use cell::DpvsPlanes;
pub use cell_caster::{
    cell_caster_matrix_words, cell_caster_row_words, generate_shadow_map_caster_cells,
    or_caster_rows_for_visible, visit_portals_no_frustum,
};
pub use chop::{chop_portal, chop_portal_winding};
pub use clip::{
    GfxMatrix, PORTAL_CLIP_PLANE_EPS, PORTAL_CLIP_SIDE_LIMIT, PORTAL_PROJECT_W_MIN, PortalBevels,
    add_bevel_planes, clip_space_winding_aabb, gfx_matrix_from_d3d_row_major,
    nearest_point_on_winding, portal_bevels_from_d3d_row_major, portal_clip_planes,
    portal_clip_planes_no_frustum, portal_hull_origin, portal_hull_point, portal_vert_hull_uv,
    rebuild_portal_hull_winding, side_plane_normals, unproject_clip_xy,
};
pub use convex_hull::{
    COM_CONVEX_HULL_MAX, PortalHullPoints, add_vert_to_portal_hull_points, com_convex_hull,
};
pub use drawsurf::{
    GfxDrawSurf, GfxDrawSurfFields, MAX_DRAWSURFS, SF_CODE_MESH, SF_GLASS_MESH, SF_MARK_MESH,
    SF_PARTICLE_CLOUD, SF_XMODEL_RIGID, SF_XMODEL_RIGID_SKINNED, material_rebind_count, pack,
    pack_code_mesh_draw_surf, pack_glass_mesh_draw_surf, pack_mark_mesh_draw_surf,
    pack_particle_cloud_draw_surf, pack_xmodel_rigid_draw_surf,
    pack_xmodel_rigid_skinned_draw_surf, sort_keys, unpack, with_object_id,
    with_primary_light_index, with_reflection_probe_index,
};
pub use dynent::{
    DynBrushVisWrite, add_dyn_ent_to_cell, cell_dyn_ent_words, dyn_brush_scene_list_admits,
    dyn_ent_cell_bits_len, dyn_ent_client_word_count, dyn_ent_in_cell,
    dyn_ent_primary_light_vis_word_count, filter_dyn_ent_into_cells,
    link_dyn_ent_primary_light_bit, unfilter_dyn_ent_from_cells,
    unlink_dyn_ent_from_primary_lights, unlink_dyn_ent_primary_light_bit_index,
    write_dyn_brush_vis,
};
pub use emit::{SurfRange, emit_draw_surfs_for_span, emit_visible_cell_roots, expand_sorted_span};
pub use material_key::{
    MaterialDrawSurfBakeInput, bake_material_draw_surf_key, custom_index_from_info_game_flags,
    fill_surface_materials, material_prepass, material_sort_key_row, surface_casts_sun_shadow_bit,
    world_surface_material,
};
pub use portal::{
    portal_behind_any_plane, portal_behind_plane, portal_eye_dist, should_skip_portal,
};
pub use portal_heap::{
    HULL_POINTS_POOL_BYTES, HULL_POINTS_POOL_STRIDE, HULL_POOL_NULL, PortalHeapNode,
    QUEUED_PORTAL_POOL, dpvs_view_plane, furthest_point_on_winding, heap_pop, heap_push,
};
pub use scene_ent::{
    SCENE_ENT_CELL_ROW_WORDS, SCENE_ENT_CELL_VIEW_BANKS, add_scene_ent_to_cell,
    add_scene_ent_to_cell_offset, dyn_pos_filter_bounds, filter_bmodel_into_cells,
    filter_dyn_pos_into_cells, filter_scene_ent_into_all_cells, filter_scene_ent_into_cells,
    scene_ent_box_reaches_cell, scene_ent_cell_bits_len, scene_ent_cell_row,
    scene_ent_cell_row_second_pass, scene_ent_cell_walk_bits, scene_ent_cell_walk_words,
    scene_ent_in_cell, unfilter_scene_ent_from_cells_view0,
};
pub use scene_ent_bounds::{
    DObjAnimMat, GFX_CFG_ENT_COUNT, SCENE_DOBJ_GATE_BOUNDED, SCENE_DOBJ_GATE_FAILED,
    SCENE_DOBJ_GATE_IDLE, SCENE_DOBJ_GATE_SKINNING, SCENE_DOBJ_GATE_UPDATING, SCENE_ENT_EMPTY_HALF,
    SCENE_ENT_SKINNED_HALF_PAD, XBoneInfoBounds, cell_frustum_cmd_plane_begin,
    mark_scene_ent_visible, or_shift_part_bits, part_bits_empty, scene_dobj_gate_begin,
    scene_dobj_initial_bounds, scene_ent_frustum_hides, scene_ent_inner_planes,
    scene_ent_is_visible, scene_ent_needs_bound_worker, scene_ent_sphere_hides,
    scene_index_alloc_bytes, set_scene_ent_part_bit, union_scene_ent_bone_aabb,
};
pub use scene_ent_skin::{
    GFX_D3DERR_DEVICELOST, GFX_D3DERR_DEVICENOTRESET, PreSkinModel, PreSkinSummary, PreSkinSurface,
    SCENE_DOBJ_GATE_SKINNED_BASE, SCENE_ENT_CULL_LOD_COUNT, SCENE_ENT_SKIN_ENTRY_BYTES,
    SCENE_ENT_SKIN_FRAME_BYTES, SCENE_ENT_SKIN_HIDDEN_BYTES, SCENE_ENT_SKIN_HIDDEN_TAG,
    SCENE_ENT_SKIN_LOCAL_BYTES, SCENE_ENT_SKIN_PLACEMENT_BYTES, SKIN_BLEND_WEIGHT_ONE,
    SKIN_BLEND_WEIGHT_SCALE, SKIN_DOBJ_ANIM_MAT_STRIDE, SKIN_DUAL_BLEND_WEIGHT_ONE,
    SKIN_DUAL_BLEND_WEIGHT_SCALE, SKIN_DUAL_DVAR_SIDECAR_STRIDE, SKIN_DUAL_DVAR_SRC_CURSOR,
    SKIN_DUAL_ESI_EDI_SRC_CURSOR, SKIN_DUAL_ESI_SRC_CURSOR, SKIN_DUAL_UNIT_VEC_BIAS,
    SKIN_DUAL_UNIT_VEC_SCALE, SKIN_DUAL_UNIT_VEC_W_SCALE, SKIN_PACKED_SRC_CURSOR,
    SKIN_PACKED_UNIT_VEC_BIAS, SKIN_PACKED_UNIT_VEC_SCALE, SKIN_PACKED_UNIT_VEC_W,
    SKIN_RIGID_HEADER_SCALE_BITS, SKIN_RIGID_VERT_LIST_BONE_SHIFT, SKIN_RIGID_VERT_LIST_STRIDE,
    SKIN_UNIT_VEC_DECODE, SKIN_UNIT_VEC_W_BIAS, SKIN_VERT_INFO_BLEND_STRIDE,
    SKIN_VERT_INFO_PACKED_STRIDE, SKIN_WEIGHTED_BUCKET_EXTRAS, SKIN_WORKER_CMD, SceneEntSkinEntry,
    gfx_testcoop_keeps_device, pre_skin_scene_ent, scene_dobj_gate_skin_begin,
    scene_dobj_gate_skinned, scene_dobj_surface_count, scene_ent_skin_cmd_bytes,
    scene_ent_skin_cmd_fills_cache, scene_ent_surface_hidden, skin_blend_weight,
    skin_cmd_fills_cache, skin_cmd_uses_dual_dvar, skin_cmd_walks, skin_dual_dvar_blend_weight,
    skin_dual_dvar_mad_extras, skin_dual_dvar_mad_one_extra, skin_dual_dvar_pack_unit_vec,
    skin_dual_dvar_rigid_bone_mat_off, skin_dual_dvar_sidecar_bytes,
    skin_dual_dvar_vert_info_blend_bytes, skin_packed_mad_extras, skin_packed_transform_point,
    skin_packed_transform_vector, skin_packed_weighted_point, skin_quat_normalize,
    skin_rigid_scaled_placement, skin_rigid_vert_list_bone_mat_off,
    skin_rigid_vert_list_packed_bytes, skin_skinned_cache_dest, skin_unpack_unit_vec,
    skin_vert_info_blend_bytes, skin_vert_info_bucket_blend_off, skin_vert_info_bucket_packed_off,
    skin_vert_info_packed_bytes,
};
pub use sort_key_uses::MATERIAL_SORT_KEY_ROW_MASK;
pub use stats::WalkStats;
pub use sun_shadow_bsp::{add_bsp_draw_surfs_range_sun_shadow_fast, surface_material_at};
pub use sun_shadow_smodel::{
    GfxStaticModelDrawInstShadow, STATIC_MODEL_FLAG_NO_CAST_SHADOW, add_smodel_range_sun_shadow,
    cull_smodel_sun_shadow_vis, smodel_cull_dist_hides,
};
pub use vis::{MsbBitIter, MsbBits, VisBits, msb_get, msb_iter, msb_set, words_for_bits};
pub use walk::{
    CellClipPlanes, CellPortalGraph, PortalView, WalkScratch, effective_portal_walk_limit,
    visit_cells, visit_cells_facing,
};
pub use xmodel_lod::{
    FLOAT64_ONE, xmodel_get_lod_for_dist, xmodel_lod_camera_dist, xmodel_lod_scaled_dists,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CPlane {
    pub normal: [f32; 3],
    pub dist: f32,

    pub r#type: u8,
}
