pub mod dip;
pub mod draw_list;
pub mod material_exec;
pub mod overlay;
pub mod pack;
pub mod partition;
pub mod spot;
pub mod tess;
pub mod tess_list;
pub mod work;

pub use dip::*;
pub use draw_list::{
    DEPTH_RANGE_BAND, DRAW_LIST_ENTRY_STRIDE, DRAW_LIST_REGISTRATION_ORDER,
    DrawListRegistrationGuard, DrawSurfListWorker, ENTRY_KIND, ENTRY_PEEK_FN, ENTRY_SORT_KEY,
    ENTRY_WORK_FN, GFX_DEPTH_RANGE_FULL, GFX_DEPTH_RANGE_SCENE, GfxCmdBufContext,
    GfxCmdBufDepthState, GfxDrawList, GfxDrawListEntry, GfxDrawListHeapRecord, GfxDrawSurfListKind,
    GfxEndDrawList, LIST_ARGS_DWORDS, LIST_BSP_SURFACES, LIST_CODE_MESH, LIST_KIND_1, LIST_KIND_2,
    LIST_KIND_3, LIST_KIND_4, LIST_KIND_5, LIST_KIND_7, LIST_KIND_9, LIST_KIND_10,
    LIST_STATIC_MODEL_CACHED, LIST_STATIC_MODEL_PRETESS, LIST_STATIC_MODEL_RIGID,
    LIST_STATIC_MODEL_SKINNED, LIST_WORLD_DRAWSURFS, LIST_XMODEL_RIGID, draw_list_heap_build,
    draw_list_heap_sift, draw_list_record_precedes, list_args_index, r_bind_draw_list_context,
    r_change_depth_range, r_dispatch_draw_list_records, r_dispatch_draw_surf_list_sorted,
    r_dispatch_draw_surf_list_unsorted, r_end_draw_list, r_end_draw_list_shadow,
    r_init_draw_surf_list_args, r_setup_draw_list,
};
pub use partition::*;
pub use spot::*;
pub use tess::*;

pub use material_exec::{
    MaterialExecView, MaterialRunCensus, MaterialRunExecutor, PlaceLanes, draw_vertex_type,
};
pub use overlay::OverlayCodeNeed;
pub use pack::{
    HOST_XMODEL_RIGID_TESS_INFO_PACKED_ARM, PackDraw, PackKind, pack_spot_shadow_frontend,
    pack_sun_shadow_frontend,
};
pub use render_frame::{MaterialExecFrame, OutdoorLookup};
pub use tess_list::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, SMODEL_RIGID_ENTRY_STRIDE,
    SmodelRigidFlush, SmodelRigidListStep, TRIANGLES_LIST_ENTRY_STRIDE, TrianglesListArm,
    TrianglesListFlush, WORLD_STREAM0_STRIDE, XMODEL_RIGID_ENTRY_STRIDE,
    XMODEL_TESS_LIGHTMAP_CODE_TEXTURE, XMODEL_TESS_SAMPLER_DEVICE_XOR,
    XMODEL_TESS_SAMPLER_IMAGE7_SHIFT, XMODEL_TESS_SAMPLER_PACKED_BASE,
    XMODEL_TESS_SET_SAMPLER_VTBL, XMODEL_TESS_SET_TEXTURE_VTBL, XMODEL_TESS_TEXTURE_CACHE_BASE,
    XModelRigidFlush, XmodelTessLightmapBinds, r_tess_static_model_rigid_draw_surf_lighting,
    r_tess_static_model_rigid_draw_surf_list, r_tess_triangles_list_generic,
    r_tess_xmodel_rigid_draw_surf_lighting, smodel_rigid_index_run_continues,
    smodel_rigid_list_step, triangles_list_run_continues, xmodel_rigid_index_run_continues,
    xmodel_tess_info_packed_arm, xmodel_tess_info_vert_decl_type, xmodel_tess_lightmap_binds,
    xmodel_tess_sampler_merge_low_byte, xmodel_tess_sampler_pack_dirty,
    xmodel_tess_sampler_packed_off, xmodel_tess_sampler_packed_word,
    xmodel_tess_sampler_xor_needs_device, xmodel_tess_texture_cache_dirty,
    xmodel_tess_texture_cache_off,
};
pub use work::{
    ColourDrawListWork, PackedEmit, PackedListKind, ShadowDrawListWork,
    r_draw_surf_list_work_colour, r_draw_surf_list_work_shadow,
};
