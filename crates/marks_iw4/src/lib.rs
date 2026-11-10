#![no_std]
#![forbid(unsafe_code)]

mod alloc;
mod allow;
mod bounds;
mod box_surfaces;
mod clip;
mod copy;
mod generate;
mod limits;
mod mesh;
mod pool;
mod xsurface;

pub use alloc::{
    FX_MARK_OFF_CONTEXT, FX_MARK_OFF_FRAME_COUNT_ALLOCED, FX_MARK_OFF_FRAME_COUNT_DRAWN,
    FX_MARK_OFF_MATERIAL, FX_MARK_OFF_NATIVE_COLOR, FX_MARK_OFF_ORIGIN, FX_MARK_OFF_POINT_COUNT,
    FX_MARK_OFF_POINTS, FX_MARK_OFF_RADIUS, FX_MARK_OFF_TEX_COORD_AXIS, FX_MARK_OFF_TRI_COUNT,
    FX_MARK_OFF_TRIS, FX_MARK_STAGING_POINT_STRIDE, FX_MARK_STAGING_TRI_STRIDE,
    FX_MARKS_INIT_ALLOCED_COUNT, FX_MARKS_INIT_FRAME_COUNT, FxAllocMarkRequest, FxMarkConstructed,
    alloc_and_construct_mark, mark_point_groups_for_count, mark_world_brushes_invokes_callback,
};
pub use allow::{
    FxMarkAllow, GFX_SURFACE_MATERIAL_OFF, MarkWorldAllowCensus, R_ALLOW_MARKS_GAME_FLAGS_MASK,
    R_ALLOW_MARKS_STATE_FLAGS_MASK, mark_allow, mark_include_in_world_clip,
    mark_material_allows_marks,
};
pub use bounds::{
    MarkWorldBoundsHits, mark_aabb_overlaps_bounds, mark_count_world_bounds_hits,
    mark_model_local_box, mark_sorted_bit_surf, mark_sphere_hits_bounds, mark_tri_aabb_hits_box,
};
pub use box_surfaces::{
    MarkBoxSurfaces, MarkBoxSurfacesCensus, MarkCellAabbTree, mark_box_surfaces,
    mark_box_surfaces_words,
};
pub use clip::{
    FxMarkEmitRefuse, FxMarkStagingPoint, FxMarkStagingTri, FxWorldMarkPoint, MarkProjection,
    MarkReceiverAttributes, MarkReceiverGeometry, MarkStagingBuffer, MarkTriangleAttributes,
    MarkWorldClipCensus, MarkWorldStaging, mark_chop_world_poly_behind_plane,
    mark_chop_world_triangle_points, mark_clip_world_triangle, mark_clip_world_triangle_points,
    mark_context_from_smodel, mark_context_from_world_surface, mark_context_lmap,
    mark_context_primary_light, mark_context_probe, mark_fragment_clip_planes,
    mark_is_triangle_rejected, mark_setup_world_clip_points,
};
pub use copy::{
    FX_MARK_CONTEXT_SIZE, FX_POINT_GROUP_CHAIN_NONE, FX_TRI_GROUP_CHAIN_NONE, FxPointGroup,
    FxTriGroup, copy_mark_points, copy_mark_tris, link_scratch_point_groups,
    link_scratch_tri_groups, mark_contexts_equal, mark_tri_groups_for_staging, mark_tri_pack_count,
};
pub use generate::{
    FxGenerateMarkVertsPacked, MarkFragmentsAgainst, MarkGenerateAddEntity, MarkGoDispatch,
    MarkReceiver, MarkWorldMesh, dyn_mark_verts_worker_gate, fill_generate_mark_verts_cmd,
    impact_mark_add_entity, impact_mark_calls_box_surfaces, impact_mark_generate_add_entity,
    impact_mark_material, impact_mark_models_generate, impact_mark_outer_gate,
    impact_mark_skip_world_from_stored_bolt, mark_go_dispatch,
};
pub use limits::{
    FX_WORLD_MARK_POINT_STRIDE, GFX_MARK_SURF_INDEX_LIMIT, GFX_MARK_SURF_LIMIT,
    GFX_MARK_SURF_VERT_LIMIT, GFX_SURFACE_LIGHTMAP_NONE, GFX_WORLD_VERTEX_STRIDE,
    R_MARK_CHOP_BEHIND, R_MARK_CHOP_MAX_POINTS, R_MARK_CHOP_ON_PLANE, R_MARK_CLIP_PLANE_COUNT,
    R_MARK_FRAGMENTS_CLIP_SURF_STRIDE, R_MARK_FRAGMENTS_MAX_POINTS, R_MARK_FRAGMENTS_MAX_TRIS,
    R_MARK_FRAGMENTS_WORLD_SURF_STACK, R_MARK_TRI_REJECT_LEN_SQ_SCALE,
};
pub use mesh::{
    GFX_MARK_MESH_BINORMAL_SIGN, GFX_MARK_MESH_VERTEX_STRIDE, GFX_MARK_SURF_STRIDE,
    GFX_MARK_SURF_TECHNIQUE_NIBBLE, GfxMarkMeshBudget, add_mark_mesh_draw_surf,
    generate_mark_verts_begin, mark_context_is_world_list, mark_mesh_index_reserve,
    pack_mark_model_vertex, pack_mark_world_vertex, reserve_mark_mesh_indices,
    reserve_mark_mesh_verts,
};
pub use pool::{
    FX_MARK_ENT_LIMIT, FX_MARK_HANDLE_NONE, FX_MARK_STRIDE, FX_MARKS_ACTIVE_LIMIT,
    FX_MARKS_CLIENT_STRIDE, FX_MARKS_LIMIT, FX_POINT_GROUP_LIMIT, FX_POINT_GROUP_NEXT_NONE,
    FX_POINT_GROUP_STRIDE, FX_TRI_GROUP_LIMIT, FX_TRI_GROUP_NEXT_NONE, FX_TRI_GROUP_STRIDE,
    init_mark_next_handle, init_point_next_slot, init_tri_next_slot, mark_handle_for_slot,
    mark_handle_from_byte_offset,
};
pub use xsurface::{
    XSurfaceCollisionLeaf, XSurfaceCollisionNode, XSurfaceCollisionTree, XSurfaceVisitError,
    xsurface_visit_triangles_in_aabb,
};
