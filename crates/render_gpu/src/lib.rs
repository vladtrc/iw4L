pub use render_scene::SkinSupport;
pub mod diag;
pub mod drawsurf;
mod plugin;

pub use drawsurf::{
    AdmittedExactPort, CODE_BASE_LIGHTING_COORDS, CODE_SHADOWMAP_POLYGON_OFFSET,
    CODE_TEXTURE_FLOATZ, CODE_TEXTURE_OUTDOOR, CODE_TEXTURE_RESOLVED_POST_SUN,
    CODE_TEXTURE_SHADOWMAP_SPOT, CODE_TEXTURE_SHADOWMAP_SUN, CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
    ChangeState0Host, ChangeState1Host, ColourWorkingSet, ConstantPackRefusal, DecodedSampler,
    DepthOfField, DofFrame, ExtractedBlood, ExtractedDiagnosticGeometry, ExtractedFilm,
    ExtractedPostFx, ExtractedRenderFrameProducts, ExtractedRuntimeImageHandles,
    ExtractedStaticGeometry, FocusedOwnerSubmitReport, FocusedOwnerSubmitState,
    GLOW_APPLY_MATERIAL, GLOW_SETUP_MATERIAL, GfxPassState, GlowFrame, GpuSubmitReady,
    InstalledRenderWorld, LENS_VIEW_SIGNATURE, ModelLightingTileUpload, ModelLightingTileUploads,
    PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY, PackedCodeSamplerLane, PassConstantBuffers,
    PublishedRenderFrame, RETAIL_SAMPLER_PROFILE_2026_08_11, RETAIL_SAMPLER_WORDS_2026_08_11,
    RenderFrameData, RenderWorldData, RetainedDrawItem, RetainedDrawKind, RuntimeImageHandles,
    RuntimeImageHandlesData, RuntimeLightmapHandles, RuntimeTextureBinding,
    RuntimeUploadedImageRegistry, RuntimeUploadedLightmapViews, SHADOWMAP_SPOT_COLOR_FORMAT,
    SHADOWMAP_SPOT_DEPTH_FORMAT, SHADOWMAP_SPOT_RT10_LABEL, SHADOWMAP_SPOT_RT11_LABEL,
    SHADOWMAP_SUN_COLOR_FORMAT, SHADOWMAP_SUN_DEPTH_FORMAT, SHADOWMAP_SUN_LABEL,
    SUN_SHADOW_CASTER_TECH, SUN_SHADOW_PARTITION_COUNT, SamplerRendererInputs, SamplerSource,
    SamplerTable, SamplerTableBuildRefusal, SamplerTextureDimension, SmodelCacheGpu,
    StaticGeometryIdentity, SurfaceSamplerInputs, TEXTURE_TABLE_2D_CAPACITY,
    TEXTURE_TABLE_3D_CAPACITY, TEXTURE_TABLE_CUBE_CAPACITY, TEXTURE_TABLE_SAMPLER_CAPACITY,
    TechType, TextureBindRefusal, UPLOAD_GRANULE, UploadedTextureBind, UploadedTextureIdentity,
    UploadedTextureView, UploadedViewRefusal, ValidatedPassWgsl, WgpuBindLayoutEntry,
    WgpuBindingKind, WgpuLayoutRefusal, WgpuPassLayout, WgpuShaderVisibility, WgpuVertexAttribute,
    WgpuVertexBufferLayout, WgpuVertexFormat, WorldPipelineWarmup, alpha_test_fragment_entry,
    bind_group_layout_from_entries, cached_lighting_port_variant, code_transpose_matrix_row4,
    code_transpose_matrix_rows, colour_ports_static, derive_wgpu_pass_layout,
    dump_shader_program_names, dump_sorted_material_names, geometry_diagnostic_enabled,
    gpu_contract, overlay_packed_code_on_banks, padded_upload_len, require_image_generation,
    scissor_xywh, split_bind_layout, surface_bounds, texture_table_bind_entries,
    vertex_layouts_from_contract, write_buffer_padded, write_buffer_range,
};
pub use plugin::RenderGpuPlugin;

pub use drawsurf::exit_clip_mode;
