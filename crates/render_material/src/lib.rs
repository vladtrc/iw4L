pub mod argument;
pub mod catalog;
pub mod code_sources;
pub mod execute;
pub mod pass_color_space;
pub mod prepared;
pub mod setup;
pub mod sm3;
pub mod sm3_abi;
pub mod stage;
pub mod vertex_decl;
pub mod vertex_layout;

pub use argument::RuntimeArgumentBinding;
pub use catalog::{
    CatalogBuildError, MaterialAssetId, MaterialGenerationId, PortId, RemapResolution,
    RuntimeImageId, RuntimeMaterial, RuntimeMaterialCatalog, RuntimePass, RuntimeProgramIdentity,
    RuntimeShaderPair, RuntimeShaderProgram, RuntimeShaderProgramId, RuntimeSortedMaterialTable,
    RuntimeTechnique, RuntimeTechniqueSet, RuntimeTechniqueSetId, RuntimeTextureBinding,
    SortedMaterialOrdinal, TECHNIQUE_SLOT_COUNT, retail_sort_band, sort_pass_args_retail,
};
pub use code_sources::{CodeSourceError, CodeSourceLookup, LayeredCodeSources, RuntimeCodeSources};
pub use execute::{
    ExecutablePass, ExecutablePassView, MaterialDrawKey, MaterialExecution, MaterialRefusal,
    PackedCodeConstantLane, PackedCodeConstants, PackedCodeSamplerLane, PackedCodeSamplers,
    StableMaterialShell, StablePassShell, UnsupportedStateFields, add_surf_has_technique,
    capture_stable_shell, draw_binds_code_texture, draw_code_sampler_mask, execute_material,
    execute_material_with_shell, prepared_draw_technique, rebind_stable_material,
    resolve_material_technique, resolve_sorted_material, smodel_tess_vertex_type,
    world_tess_vertex_type, world_tess_vertex_type_authored,
};
pub use pass_color_space::{PassColorSpace, pass_color_space};
pub use prepared::{
    AdmittedPortFacts, ExecutableArgumentBinding, GfxPassStateBits, PackedCodeArg,
    PackedLocalBanks, PackedLocalSamplerLane, PackedLocalSamplers, PreparedArgBelts,
    PreparedArgSlice, PreparedMaterial, PreparedMaterialTable, PreparedPass, PreparedTableCensus,
    PreparedTechnique, pack_local_banks, pack_local_samplers,
};
pub use setup::{SetupArm, TechType};
pub use sm3::{
    OpcodeSurfaceCoverage, SUPPORTED_OPCODE_SURFACE, Sm3Destination, Sm3Instruction,
    Sm3InstructionBody, Sm3IrError, Sm3LoweringRefusal, Sm3Opcode, Sm3ProgramIr, Sm3Register,
    Sm3RegisterFile, Sm3RelativeAddress, Sm3Source, Sm3SourceModifier, decode_sm3_program,
    measure_shader_surface_coverage, named_sm3_opcode_count, validate_supported_opcode_surface,
};
pub use sm3_abi::{
    ConstantBinding, ConstantSource, PassAbiRefusal, PassProgramAbi, SamplerBinding, SamplerSource,
    SamplerTextureDimension, Semantic, VaryingBinding, VertexAttribute, VertexInputBinding,
    build_pass_abi, format_vertex_semantic,
};
pub use stage::RuntimeShaderStage;
pub use vertex_decl::RuntimeVertexDecl;
pub use vertex_layout::{T5_WORLD_LAYER_HOST_STRIDE, VertexLayoutFamily};
