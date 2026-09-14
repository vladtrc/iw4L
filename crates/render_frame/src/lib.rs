pub mod code_math;
pub mod code_mesh;
pub mod entries;
pub mod exec;
pub mod geometry;
pub mod gpu_contract;
pub mod packet;
pub mod packing;
pub mod products;
pub mod retained;
pub mod sun_effects;
pub mod sun_shadow;
pub mod texture_bind;

pub use code_mesh::{
    CODE_MESH_ARGS_CAP, CODE_MESH_ARGS_STRIDE, CODE_MESH_INDEX_CAP, CODE_MESH_VERT_CAP,
    CODE_MESH_VERT_STRIDE, CODE_MESH_WARN_ARGS, CODE_MESH_WARN_INDS, CODE_MESH_WARN_VERTS,
    GfxMeshData, r_get_code_mesh_args, r_get_code_mesh_verts, r_reserve_code_mesh,
    r_reserve_code_mesh_indices, r_reserve_code_mesh_verts, r_shrink_code_mesh_verts,
};
pub use entries::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, SMODEL_RIGID_ENTRY_STRIDE,
    SmodelPretessRange, TRIANGLES_LIST_ENTRY_STRIDE, XMODEL_RIGID_ENTRY_STRIDE,
    smodel_rigid_index_run_continues, triangles_list_run_continues,
    xmodel_rigid_index_run_continues,
};
pub use exec::{MaterialExecFrame, OutdoorLookup, SpotShadowReceiver};
pub use geometry::{
    DYNAMIC_INDEX_BUFFER_CAPACITY, RetailPackedVertexRefusal, RetailWorldVertexRefusal,
    SmodelVertex, SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs, WorldVertex,
    xmodel_tess_info_packed_arm, xmodel_tess_info_vert_decl_type,
};
pub use gpu_contract::{
    TEXTURE_TABLE_2D_CAPACITY, TEXTURE_TABLE_3D_CAPACITY, TEXTURE_TABLE_CUBE_CAPACITY,
    TEXTURE_TABLE_SAMPLER_CAPACITY, WgpuBindLayoutEntry, WgpuBindingKind, WgpuLayoutRefusal,
    WgpuPassLayout, WgpuShaderVisibility, WgpuVertexAttribute, WgpuVertexBufferLayout,
    WgpuVertexFormat, derive_wgpu_pass_layout, texture_table_bind_entries,
};
pub use packet::{
    FRONTEND_DRAW_LISTS_DWORDS, LIST_TOKEN, PackedFrontendLists, SRC_PRETESS_COUNT,
    SRC_PRETESS_PTR, SRC_SMODEL_CACHED_BYTES, SRC_SMODEL_CACHED_PTR, SRC_SMODEL_PRETESS_BYTES,
    SRC_SMODEL_PRETESS_PTR, SRC_SMODEL_RIGID_COUNT, SRC_SMODEL_RIGID_PTR, SRC_SMODEL_SKINNED_BYTES,
    SRC_SMODEL_SKINNED_PTR, SRC_WORLD_COUNT, SRC_WORLD_PTR, SRC_XMODEL_RIGID_COUNT,
    SRC_XMODEL_RIGID_PTR,
};
pub use products::{
    FrameProduct, FrameProductKind, FrameProductStatus, FrameProductsSnapshot, MissingProductCause,
    PACKED_SEGMENT_OWNERS, PackedSegment, PackedSegments, ProductTarget, RenderFocusFrame,
    SourceRevisions, SpotShadowFrameSlot, publish_rows,
};
pub use retained::{
    BspCameraLane, LightAttenuationBind, RENDER_FX_DEPTH_HACK, RetainedDrawItem, RetainedDrawKind,
    T5LightFalloffPack, XMODEL_OBJECT_ID_VIEWMODEL, host_viewmodel_render_fx_flags,
};
pub use sun_effects::{SunEffectsDef, SunEffectsFrame, angular_lerp};
pub use sun_shadow::{
    SUN_SHADOW_CASTER_TECH, SUN_SHADOW_FORCED_PROFILE, SUN_SHADOW_PARTITION_COUNT,
    SunShadowAtlasProfile, SunShadowCasterLists, SunShadowForcedFrame, SunShadowPartition,
    SunShadowPartitionLists, SunShadowReceiverConstants, SunShadowViewport,
};
pub use texture_bind::TextureBindIdentity;
