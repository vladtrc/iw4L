use core::cmp::Ordering;
use std::collections::BTreeMap;

use bevy::platform::collections::HashMap;

use super::setup::TechType;

pub use render_frame::{
    SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs, TextureBindIdentity,
};
pub use render_material::{
    AdmittedPortFacts, CatalogBuildError, CodeSourceError, CodeSourceLookup,
    ExecutableArgumentBinding, ExecutablePass, ExecutablePassView, GfxPassStateBits,
    LayeredCodeSources, MaterialAssetId, MaterialExecution, MaterialGenerationId, MaterialRefusal,
    PackedCodeArg, PackedCodeConstantLane, PackedCodeConstants, PackedCodeSamplerLane,
    PackedCodeSamplers, PackedLocalBanks, PackedLocalSamplerLane, PackedLocalSamplers, PortId,
    PreparedArgBelts, PreparedArgSlice, PreparedMaterial, PreparedMaterialTable, PreparedPass,
    PreparedTableCensus, PreparedTechnique, RemapResolution, RuntimeArgumentBinding,
    RuntimeCodeSources, RuntimeImageId, RuntimeMaterial, RuntimeMaterialCatalog, RuntimePass,
    RuntimeProgramIdentity, RuntimeShaderPair, RuntimeShaderProgram, RuntimeShaderProgramId,
    RuntimeShaderStage, RuntimeSortedMaterialTable, RuntimeTechnique, RuntimeTechniqueSet,
    RuntimeTechniqueSetId, RuntimeTextureBinding, RuntimeVertexDecl, SortedMaterialOrdinal,
    StableMaterialShell, StablePassShell, TECHNIQUE_SLOT_COUNT, add_surf_has_technique,
    capture_stable_shell, draw_binds_code_texture, draw_code_sampler_mask, execute_material,
    pack_local_banks, pack_local_samplers, prepared_draw_technique, rebind_stable_material,
    resolve_material_technique, resolve_sorted_material, smodel_tess_vertex_type,
    sort_pass_args_retail, world_tess_vertex_type, world_tess_vertex_type_authored,
};

pub use render_scene::runtime_cull_face;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeProgramPort {
    id: PortId,
    pair: RuntimeShaderPair,
    custom_sampler_flags: u8,
    t5_custom_sampler_flags: u8,
    arguments: Vec<RuntimeArgumentBinding>,
    color_space: super::pass_color_space::PassColorSpace,
    abi: super::sm3_abi::PassProgramAbi,
    module: std::sync::Arc<super::sm3_wgsl::ValidatedPassWgsl>,
    wgpu_layout: super::gpu_contract::WgpuPassLayout,
}

impl RuntimeProgramPort {
    pub fn compile(
        catalog: &RuntimeMaterialCatalog,
        pass: &RuntimePass,
        vertex_type: u8,
    ) -> Result<Self, ProgramRegistryError> {
        let pair = pass
            .shader_pair
            .ok_or(ProgramRegistryError::PassShaderPairMissing)?;
        let vertex_source = exact_stage_source(catalog, pair, RuntimeShaderStage::Vertex)?;
        let pixel_source = exact_stage_source(catalog, pair, RuntimeShaderStage::Pixel)?;
        let vertex =
            super::sm3::decode_sm3_program(&vertex_source.program, RuntimeShaderStage::Vertex)
                .map_err(|cause| ProgramRegistryError::Decode {
                    stage: RuntimeShaderStage::Vertex,
                    cause,
                })?;
        let pixel =
            super::sm3::decode_sm3_program(&pixel_source.program, RuntimeShaderStage::Pixel)
                .map_err(|cause| ProgramRegistryError::Decode {
                    stage: RuntimeShaderStage::Pixel,
                    cause,
                })?;
        let decl = catalog.vertex_decl(pair.vertex_decl_slot).ok_or(
            ProgramRegistryError::MissingVertexDeclaration {
                pointer_identity: pair.vertex_decl_slot,
            },
        )?;
        let abi = super::sm3_abi::build_pass_abi(
            &vertex,
            &pixel,
            decl,
            vertex_type,
            &pass.arguments,
            pass.custom_sampler_flags,
            pass.t5_custom_sampler_flags,
        )
        .map_err(ProgramRegistryError::Abi)?;
        Self::finish_compile(
            pair,
            pass,
            vertex_type,
            vertex,
            pixel,
            abi,
            &vertex_source.name,
            &pixel_source.name,
        )
    }

    fn finish_compile(
        pair: RuntimeShaderPair,
        pass: &RuntimePass,
        vertex_type: u8,
        vertex: super::sm3::Sm3ProgramIr,
        pixel: super::sm3::Sm3ProgramIr,
        abi: super::sm3_abi::PassProgramAbi,
        vertex_name: &str,
        pixel_name: &str,
    ) -> Result<Self, ProgramRegistryError> {
        let lowering = super::sm3_wgsl::pass_lowering_abi(&abi);
        let module = if let Some(cached) = super::wgsl_disk_cache::load(pair, &lowering) {
            cached
        } else {
            let module = super::sm3_wgsl::lower_pass_to_validated_wgsl(&abi, &vertex, &pixel)
                .map_err(ProgramRegistryError::Wgsl)?;
            super::wgsl_disk_cache::store(pair, &lowering, &module);
            module
        };
        diag::wgsl_dump::dump_pass_wgsl(vertex_name, pixel_name, &module.source);
        let wgpu_layout = super::gpu_contract::derive_wgpu_pass_layout(&abi, &module)
            .map_err(ProgramRegistryError::WgpuLayout)?;
        let id = PortId::from_pass(pass, vertex_type)
            .ok_or(ProgramRegistryError::PassShaderPairMissing)?;
        Ok(Self {
            id,
            pair,
            custom_sampler_flags: pass.custom_sampler_flags,
            t5_custom_sampler_flags: pass.t5_custom_sampler_flags,
            arguments: pass.arguments.clone(),
            color_space: pass.color_space,
            abi,
            module: std::sync::Arc::new(module),
            wgpu_layout,
        })
    }

    pub fn id(&self) -> PortId {
        self.id
    }

    pub fn accepts_pass(&self, pass: ExecutablePassView<'_>) -> bool {
        pass.port == self.id && pass.port.matches_shader_pair(pass.shader_pair)
    }

    pub fn pair(&self) -> RuntimeShaderPair {
        self.pair
    }

    pub fn abi(&self) -> &super::sm3_abi::PassProgramAbi {
        &self.abi
    }

    pub fn same_admission_identity(&self, other: &Self) -> bool {
        same_shader_pair_content(self.pair, other.pair)
            && self.abi == other.abi
            && self.module.source == other.module.source
            && self.custom_sampler_flags == other.custom_sampler_flags
            && self.t5_custom_sampler_flags == other.t5_custom_sampler_flags
            && self.arguments == other.arguments
            && self.color_space == other.color_space
            && self.abi.vertex_type == other.abi.vertex_type
    }

    pub fn shared_module(&self) -> std::sync::Arc<super::sm3_wgsl::ValidatedPassWgsl> {
        std::sync::Arc::clone(&self.module)
    }

    pub fn module(&self) -> &super::sm3_wgsl::ValidatedPassWgsl {
        &self.module
    }

    pub fn wgpu_layout(&self) -> &super::gpu_contract::WgpuPassLayout {
        &self.wgpu_layout
    }

    pub fn admitted_facts(&self) -> AdmittedPortFacts {
        AdmittedPortFacts {
            id: self.id,
            vertex_constant_len: self.module.vertex_constant_len,
            pixel_constant_len: self.module.pixel_constant_len,
        }
    }

    pub fn bind_surface_samplers(
        &self,
        inputs: SurfaceSamplerInputs,
    ) -> Result<Vec<BoundSurfaceSampler>, SurfaceSamplerRefusal> {
        let mut bound = Vec::new();
        for sampler in &self.abi.samplers {
            let resource = match sampler.source {
                super::sm3_abi::SamplerSource::SurfaceReflectionProbe => {
                    BoundSurfaceSampler::ReflectionProbe {
                        register: sampler.register,
                        id: inputs.reflection_probe.ok_or(
                            SurfaceSamplerRefusal::MissingReflectionProbe {
                                register: sampler.register,
                            },
                        )?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfacePrimaryLightmap => {
                    BoundSurfaceSampler::PrimaryLightmap {
                        register: sampler.register,
                        id: inputs.primary_lightmap.ok_or(
                            SurfaceSamplerRefusal::MissingPrimaryLightmap {
                                register: sampler.register,
                            },
                        )?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfaceSecondaryLightmap => {
                    BoundSurfaceSampler::SecondaryLightmap {
                        register: sampler.register,
                        id: inputs.secondary_lightmap.ok_or(
                            SurfaceSamplerRefusal::MissingSecondaryLightmap {
                                register: sampler.register,
                            },
                        )?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfaceSecondaryBLightmap => {
                    BoundSurfaceSampler::SecondaryBLightmap {
                        register: sampler.register,
                        id: inputs.secondary_lightmap.ok_or(
                            SurfaceSamplerRefusal::MissingSecondaryLightmap {
                                register: sampler.register,
                            },
                        )?,
                    }
                }
                super::sm3_abi::SamplerSource::MaterialTexture { .. }
                | super::sm3_abi::SamplerSource::CodeTexture { .. } => continue,
            };
            bound.push(resource);
        }
        Ok(bound)
    }

    pub fn matches(&self, pass: &RuntimePass, vertex_type: u8) -> bool {
        pass.shader_pair
            .is_some_and(|pair| same_shader_pair_content(pair, self.pair))
            && pass.custom_sampler_flags == self.custom_sampler_flags
            && pass.t5_custom_sampler_flags == self.t5_custom_sampler_flags
            && pass.arguments == self.arguments
            && pass.color_space == self.color_space
            && self.abi.vertex_type == vertex_type
    }
}

fn same_shader_pair_content(a: RuntimeShaderPair, b: RuntimeShaderPair) -> bool {
    a.vertex.program_hash == b.vertex.program_hash
        && a.pixel.program_hash == b.pixel.program_hash
        && a.vertex_decl_slot == b.vertex_decl_slot
}

fn exact_stage_source(
    catalog: &RuntimeMaterialCatalog,
    pair: RuntimeShaderPair,
    expected: RuntimeShaderStage,
) -> Result<&RuntimeShaderProgram, ProgramRegistryError> {
    let id = match expected {
        RuntimeShaderStage::Vertex => pair.vertex,
        RuntimeShaderStage::Pixel => pair.pixel,
    };
    let program = catalog
        .shader_program(id)
        .ok_or(ProgramRegistryError::MissingExactSource {
            pair,
            stage: expected,
        })?;
    if program.stage != expected {
        return Err(ProgramRegistryError::StageMismatch {
            pair,
            expected,
            actual: program.stage,
        });
    }
    Ok(program)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeProgramRegistry {
    ports: Vec<RuntimeProgramPort>,
    by_id: HashMap<PortId, usize>,
}

impl RuntimeProgramRegistry {
    pub fn catalog_unique_pass_indices(
        catalog: &RuntimeMaterialCatalog,
        tech_type: TechType,
    ) -> Vec<(usize, usize)> {
        Self::catalog_unique_pass_indices_in(catalog, tech_type, None)
    }

    pub fn catalog_unique_pass_indices_in(
        catalog: &RuntimeMaterialCatalog,
        tech_type: TechType,
        only_sets: Option<&std::collections::HashSet<usize>>,
    ) -> Vec<(usize, usize)> {
        let mut indices = Vec::<(usize, usize)>::new();
        let mut seen: HashMap<
            (
                RuntimeShaderPair,
                u8,
                u8,
                render_material::PassColorSpace,
                Vec<RuntimeArgumentBinding>,
            ),
            usize,
        > = HashMap::new();
        for (set_i, set) in catalog.technique_sets.iter().enumerate() {
            if only_sets.is_some_and(|sets| !sets.contains(&set_i)) {
                continue;
            }
            let Some(technique) = set.technique(tech_type) else {
                continue;
            };
            for (pass_i, pass) in technique.passes.iter().enumerate() {
                let Some(pair) = pass.shader_pair else {
                    continue;
                };
                let key = (
                    pair,
                    pass.custom_sampler_flags,
                    pass.t5_custom_sampler_flags,
                    pass.color_space,
                    pass.arguments.clone(),
                );
                if seen.contains_key(&key) {
                    continue;
                }
                seen.insert(key, indices.len());
                indices.push((set_i, pass_i));
            }
        }
        indices
    }

    pub fn catalog_pass<'a>(
        catalog: &'a RuntimeMaterialCatalog,
        tech_type: TechType,
        set_i: usize,
        pass_i: usize,
    ) -> Option<&'a RuntimePass> {
        catalog
            .technique_sets
            .get(set_i)?
            .technique(tech_type)?
            .passes
            .get(pass_i)
    }

    pub fn from_ports(admitted: Vec<RuntimeProgramPort>) -> Result<Self, ProgramRegistryError> {
        let mut by_id: HashMap<PortId, usize> = HashMap::with_capacity(admitted.len());
        for (index, port) in admitted.iter().enumerate() {
            if let Some(&existing) = by_id.get(&port.id) {
                let other = &admitted[existing];
                if other.same_admission_identity(port) {
                    return Err(ProgramRegistryError::DuplicatePort { pair: port.pair });
                }
                return Err(ProgramRegistryError::PortIdCollision {
                    pair: port.pair,
                    other: other.pair,
                });
            }
            by_id.insert(port.id, index);
        }
        Ok(Self {
            ports: admitted,
            by_id,
        })
    }

    pub fn ports(&self) -> &[RuntimeProgramPort] {
        &self.ports
    }

    pub fn get(&self, id: PortId) -> Option<&RuntimeProgramPort> {
        self.by_id.get(&id).map(|&index| &self.ports[index])
    }

    pub fn port_for(&self, pass: &RuntimePass, vertex_type: u8) -> Option<&RuntimeProgramPort> {
        let id = PortId::from_pass(pass, vertex_type)?;
        let port = self.get(id)?;
        debug_assert!(
            port.matches(pass, vertex_type),
            "PortId collision: same name, different admission identity"
        );
        port.matches(pass, vertex_type).then_some(port)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CatalogPortCompile {
    pub ports: Vec<RuntimeProgramPort>,
    pub refused: Vec<(RuntimeShaderPair, ProgramRegistryError)>,
}

impl ProgramRegistryError {
    pub fn census_key(&self) -> String {
        match self {
            Self::StageMismatch { .. } => "StageMismatch".into(),
            Self::Abi(refusal) => abi_census_key(*refusal),
            Self::Wgsl(error) => wgsl_census_key(error),
            Self::Decode { cause, .. } => format!("Decode({cause:?})")
                .split(['{', '('])
                .next()
                .unwrap_or("Decode")
                .trim()
                .to_owned(),
            other => {
                let text = format!("{other:?}");
                text.split(['{', '('])
                    .next()
                    .unwrap_or(&text)
                    .trim()
                    .to_owned()
            }
        }
    }
}

fn abi_census_key(refusal: super::sm3_abi::PassAbiRefusal) -> String {
    match refusal {
        super::sm3_abi::PassAbiRefusal::UnsupportedSamplerTextureType { texture_type, .. } => {
            format!("Abi/UnsupportedSamplerTextureType({texture_type})")
        }
        super::sm3_abi::PassAbiRefusal::UnknownArgumentType { argument_type } => {
            format!("Abi/UnknownArgumentType({argument_type})")
        }
        super::sm3_abi::PassAbiRefusal::VertexTypeMissingSource {
            vertex_type,
            source,
        } => format!("Abi/VertexTypeMissingSource({vertex_type},{source})"),
        super::sm3_abi::PassAbiRefusal::ConstantRegisterUnbound { stage, register } => {
            format!("Abi/ConstantRegisterUnbound({stage:?},{register})")
        }
        super::sm3_abi::PassAbiRefusal::VertexInputUnrouted { semantic, .. } => {
            format!("Abi/VertexInputUnrouted({semantic:?})")
        }
        super::sm3_abi::PassAbiRefusal::SamplerRegisterConflict { register } => {
            format!("Abi/SamplerRegisterConflict({register})")
        }
        super::sm3_abi::PassAbiRefusal::SamplerRegisterUnbound { register } => {
            format!("Abi/SamplerRegisterUnbound({register})")
        }
        other => {
            let text = format!("{other:?}");
            let name = text.split(['{', '(']).next().unwrap_or(&text).trim();
            format!("Abi/{name}")
        }
    }
}

fn wgsl_census_key(error: &super::sm3_wgsl::Sm3WgslError) -> String {
    match error {
        super::sm3_wgsl::Sm3WgslError::Emit(d3d9_sm3::Sm3WgslError::LoweringSurface(
            super::sm3::Sm3LoweringRefusal::UnknownOpcode { opcode, .. },
        )) => format!("UnknownOpcode({opcode:#x})"),
        super::sm3_wgsl::Sm3WgslError::Emit(
            d3d9_sm3::Sm3WgslError::UnsupportedInstructionControls {
                opcode, controls, ..
            },
        ) => format!("Wgsl/UnsupportedInstructionControls({opcode:?},{controls})"),
        super::sm3_wgsl::Sm3WgslError::Emit(
            d3d9_sm3::Sm3WgslError::UnsupportedSamplerTextureType { texture_type, .. },
        ) => format!("Wgsl/UnsupportedSamplerTextureType({texture_type})"),
        super::sm3_wgsl::Sm3WgslError::Emit(d3d9_sm3::Sm3WgslError::UnbalancedControlFlow {
            kind,
            ..
        }) => format!("Wgsl/UnbalancedControlFlow({kind})"),
        other => {
            let text = format!("{other:?}");
            if let Some(at) = text.find("UnknownOpcode") {
                if let Some(op) = text[at..].find("opcode:") {
                    let opcode = text[at + op + 7..]
                        .split([',', '}'])
                        .next()
                        .unwrap_or("")
                        .trim();
                    return format!("UnknownOpcode({opcode})");
                }
            }

            let inner = text
                .strip_prefix("Emit(")
                .map(|rest| rest.trim_end_matches(')'))
                .unwrap_or(text.as_str());
            let name = inner.split(['{', '(']).next().unwrap_or(inner).trim();
            format!("Wgsl/{name}")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundSurfaceSampler {
    ReflectionProbe {
        register: u16,
        id: SurfaceReflectionProbeId,
    },
    PrimaryLightmap {
        register: u16,
        id: SurfaceLightmapId,
    },
    SecondaryBLightmap {
        register: u16,
        id: SurfaceLightmapId,
    },
    SecondaryLightmap {
        register: u16,
        id: SurfaceLightmapId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceSamplerRefusal {
    MissingReflectionProbe { register: u16 },
    MissingPrimaryLightmap { register: u16 },
    MissingSecondaryLightmap { register: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramRegistryError {
    PassShaderPairMissing,
    MissingExactSource {
        pair: RuntimeShaderPair,
        stage: RuntimeShaderStage,
    },
    StageMismatch {
        pair: RuntimeShaderPair,
        expected: RuntimeShaderStage,
        actual: RuntimeShaderStage,
    },
    MissingVertexDeclaration {
        pointer_identity: u32,
    },
    Decode {
        stage: RuntimeShaderStage,
        cause: super::sm3::Sm3IrError,
    },
    Abi(super::sm3_abi::PassAbiRefusal),
    Wgsl(super::sm3_wgsl::Sm3WgslError),
    WgpuLayout(super::gpu_contract::WgpuLayoutRefusal),
    DuplicatePort {
        pair: RuntimeShaderPair,
    },

    PortIdCollision {
        pair: RuntimeShaderPair,
        other: RuntimeShaderPair,
    },
    ExactIdentityMissing {
        identity: RuntimeProgramIdentity,
    },
    AmbiguousExactIdentity {
        identity: RuntimeProgramIdentity,
        variants: usize,
    },
}

fn original_remap_resolution() -> RemapResolution {
    RemapResolution::SelfSet
}

fn source_generation_id(source: &assets::MaterialCatalog) -> MaterialGenerationId {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for material in &source.materials {
        for byte in material.name.as_str().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= material.draw_surf;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    MaterialGenerationId(hash)
}

fn program_hash(program: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in program {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Clone, Debug)]
struct ComparatorPassKey {
    pixel_shader_name: String,
    vertex_shader_name: String,
    code_pixel_constants: Vec<u16>,
    pixel_constants: Vec<(u16, [u32; 4])>,
}

#[derive(Clone, Debug)]
struct ComparatorRecord {
    asset_id: MaterialAssetId,
    name: String,
    technique_set_name: String,
    sort_key: u8,
    info_game_flags: u8,
    state_flags: u8,
    prepass: u8,
    slot5: Option<ComparatorPassKey>,
    slot9: Option<ComparatorPassKey>,
}

fn shader_name<'a>(
    source: &'a assets::MaterialCatalog,
    material: MaterialAssetId,
    slot: u8,
    reference: &assets::OwnedShaderRef,
) -> Result<&'a str, CatalogBuildError> {
    reference
        .shader
        .and_then(|index| source.shaders.get(index))
        .map(|shader| shader.name.as_str())
        .ok_or(CatalogBuildError::ShaderIdentityMissing { material, slot })
}

fn comparator_pass_key(
    source: &assets::MaterialCatalog,
    material: &assets::AuthoredMaterial,
    asset_id: MaterialAssetId,
    slot: u8,
    technique: &assets::OwnedTechnique,
) -> Result<ComparatorPassKey, CatalogBuildError> {
    let pass = technique
        .passes
        .first()
        .ok_or(CatalogBuildError::TechniqueBodyMissing {
            material: asset_id,
            slot,
        })?;
    let stable_start =
        usize::from(pass.per_prim_arg_count).saturating_add(usize::from(pass.per_obj_arg_count));
    let stable_end = stable_start.saturating_add(usize::from(pass.stable_arg_count));
    let stable = pass.arguments.get(stable_start..stable_end).ok_or(
        CatalogBuildError::TechniqueBodyMissing {
            material: asset_id,
            slot,
        },
    )?;

    let argument_type = |argument: &assets::OwnedShaderArgument| match argument {
        assets::OwnedShaderArgument::MaterialVertexConstant { .. } => 0,
        assets::OwnedShaderArgument::LiteralVertexConstant { .. } => 1,
        assets::OwnedShaderArgument::MaterialPixelSampler { .. } => 2,
        assets::OwnedShaderArgument::CodeVertexConstant { .. } => 3,
        assets::OwnedShaderArgument::CodePixelSampler { .. } => 4,
        assets::OwnedShaderArgument::CodePixelConstant { .. } => 5,
        assets::OwnedShaderArgument::MaterialPixelConstant { .. } => 6,
        assets::OwnedShaderArgument::LiteralPixelConstant { .. } => 7,
        assets::OwnedShaderArgument::Unknown { argument_type, .. } => *argument_type,
    };
    let mut cursor = stable
        .iter()
        .position(|argument| argument_type(argument) >= 5)
        .unwrap_or(stable.len());
    let mut code_pixel_constants = Vec::new();
    while let Some(assets::OwnedShaderArgument::CodePixelConstant { index, .. }) =
        stable.get(cursor)
    {
        code_pixel_constants.push(*index);
        cursor += 1;
    }

    cursor = stable
        .iter()
        .position(|argument| argument_type(argument) >= 6)
        .unwrap_or(stable.len());
    let mut pixel_constants = Vec::new();
    while let Some(assets::OwnedShaderArgument::MaterialPixelConstant {
        destination,
        name_hash,
    }) = stable.get(cursor)
    {
        let words = match material
            .constants
            .iter()
            .find(|constant| constant.name_hash == *name_hash)
            .map(|constant| constant.literal.map(f32::to_bits))
        {
            Some(words) => words,
            None if leftover_x_token_aliased(source, material) => {
                cursor += 1;
                continue;
            }
            None => {
                return Err(CatalogBuildError::MaterialConstantMissing {
                    material: asset_id,
                    name_hash: *name_hash,
                });
            }
        };
        pixel_constants.push((*destination, words));
        cursor += 1;
    }
    while let Some(assets::OwnedShaderArgument::LiteralPixelConstant { destination, words }) =
        stable.get(cursor)
    {
        pixel_constants.push((
            *destination,
            words.ok_or(CatalogBuildError::LiteralPixelConstantMissing {
                material: asset_id,
                slot,
            })?,
        ));
        cursor += 1;
    }
    pixel_constants.sort_by_key(|(destination, _)| *destination);

    Ok(ComparatorPassKey {
        pixel_shader_name: shader_name(source, asset_id, slot, &pass.pixel_shader)?.to_owned(),
        vertex_shader_name: shader_name(source, asset_id, slot, &pass.vertex_shader)?.to_owned(),
        code_pixel_constants,
        pixel_constants,
    })
}

fn leftover_x_token_aliased(
    source: &assets::MaterialCatalog,
    material: &assets::AuthoredMaterial,
) -> bool {
    if material.namespace != assets::AssetNamespace::T5 {
        return false;
    }
    let want = assets::AssetRef::bare_name(material.technique_set.as_str());
    let stripped = assets::t5_feature_token_stripped(want);
    stripped != want
        && source.technique_set_facts().iter().any(|facts| {
            facts.name.is_real() && facts.name.as_str() == stripped && facts.table.is_some()
        })
}

fn comparator_record(
    source: &assets::MaterialCatalog,
    index: usize,
) -> Result<ComparatorRecord, CatalogBuildError> {
    let asset_id = MaterialAssetId(
        u16::try_from(index).map_err(|_| CatalogBuildError::MaterialAssetIdOverflow { index })?,
    );
    let material = &source.materials[index];
    let key = assets::TechsetKey::new(material.namespace, material.technique_set.as_str());
    let facts = match source.resolve_technique_set(key) {
        assets::TechsetResolve::Hit { facts, .. } => facts,
        assets::TechsetResolve::GraphMissing { .. } => {
            return Err(CatalogBuildError::TechniqueGraphMissing { material: asset_id });
        }
        assets::TechsetResolve::Foreign { got, .. } => {
            return Err(CatalogBuildError::TechniqueSetNamespaceMismatch {
                material: asset_id,
                want: material.namespace,
                got,
            });
        }
        assets::TechsetResolve::Missing => {
            return Err(CatalogBuildError::TechniqueSetMissing { material: asset_id });
        }
    };
    let table = facts
        .table
        .as_ref()
        .ok_or(CatalogBuildError::TechniqueGraphMissing { material: asset_id })?;
    let graph = table
        .graph
        .as_ref()
        .ok_or(CatalogBuildError::TechniqueGraphMissing { material: asset_id })?;
    let pass_key = |slot: usize| -> Result<Option<ComparatorPassKey>, CatalogBuildError> {
        if table.slots & (1 << slot) == 0 {
            return Ok(None);
        }
        let technique = graph.slots.get(slot).and_then(Option::as_ref).ok_or(
            CatalogBuildError::TechniqueBodyMissing {
                material: asset_id,
                slot: slot as u8,
            },
        )?;
        comparator_pass_key(source, material, asset_id, slot as u8, technique).map(Some)
    };
    Ok(ComparatorRecord {
        asset_id,
        name: material.name.to_string(),
        technique_set_name: facts.name.to_string(),
        sort_key: material.sort_key,
        info_game_flags: material.info_game_flags,
        state_flags: material.state_flags,
        prepass: dpvs_iw4::material_prepass(
            table.slots & 1 == 0,
            table.slots & 2 != 0,
            material.state_flags,
            table.technique0_flags,
        ),
        slot5: pass_key(5)?,
        slot9: pass_key(9)?,
    })
}

fn compare_float_words(
    a: [u32; 4],
    b: [u32; 4],
    material: MaterialAssetId,
    slot: u8,
) -> Result<Ordering, CatalogBuildError> {
    for (a, b) in a.into_iter().zip(b) {
        let (a, b) = (f32::from_bits(a), f32::from_bits(b));
        let order = a
            .partial_cmp(&b)
            .ok_or(CatalogBuildError::NonFiniteComparatorConstant { material, slot })?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_pass_args(
    a: &ComparatorPassKey,
    b: &ComparatorPassKey,
    material: MaterialAssetId,
    slot: u8,
) -> Result<Ordering, CatalogBuildError> {
    let order = a.code_pixel_constants.cmp(&b.code_pixel_constants);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = a.pixel_constants.len().cmp(&b.pixel_constants.len());
    if order != Ordering::Equal {
        return Ok(order);
    }
    for ((a_destination, a_words), (b_destination, b_words)) in
        a.pixel_constants.iter().zip(&b.pixel_constants)
    {
        let order = a_destination.cmp(b_destination);
        if order != Ordering::Equal {
            return Ok(order);
        }
        let order = compare_float_words(*a_words, *b_words, material, slot)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_sorted_materials(
    a: &ComparatorRecord,
    b: &ComparatorRecord,
    slot_gap: &mut u32,
) -> Result<Ordering, CatalogBuildError> {
    let order = a.sort_key.cmp(&b.sort_key);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = if a.slot9.is_some() {
        ((b.info_game_flags >> 1) & 1).cmp(&((a.info_game_flags >> 1) & 1))
    } else {
        u8::from(b.slot5.is_some()).cmp(&u8::from(a.slot5.is_some()))
    };
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = a.prepass.cmp(&b.prepass);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = ((b.state_flags >> 3) & 1).cmp(&((a.state_flags >> 3) & 1));
    if order != Ordering::Equal {
        return Ok(order);
    }

    let mut compare_slot = |slot: u8,
                            a_pass: Option<&ComparatorPassKey>,
                            b_pass: Option<&ComparatorPassKey>,
                            compare_args: bool|
     -> Result<Ordering, CatalogBuildError> {
        let (Some(a_pass), Some(b_pass)) = (a_pass, b_pass) else {
            if a_pass.is_none() && b_pass.is_none() {
                return Ok(Ordering::Equal);
            }
            *slot_gap = slot_gap.saturating_add(1);
            if *slot_gap == 1 {
                diag::info!(
                    World,
                    "drawsurf sorted materials: slot {slot} occupancy gap \
                     {} vs {} — present-first (not ComparatorInvariant)",
                    a.name,
                    b.name
                );
            }
            return Ok(u8::from(b_pass.is_some()).cmp(&u8::from(a_pass.is_some())));
        };
        let order = a_pass.pixel_shader_name.cmp(&b_pass.pixel_shader_name);
        if order != Ordering::Equal {
            return Ok(order);
        }
        if compare_args {
            let order = compare_pass_args(a_pass, b_pass, a.asset_id, slot)?;
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(a_pass.vertex_shader_name.cmp(&b_pass.vertex_shader_name))
    };

    if a.slot9.is_some() {
        let order = compare_slot(
            9,
            a.slot9.as_ref(),
            b.slot9.as_ref(),
            a.state_flags & 8 != 0,
        )?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    } else if b.slot9.is_some() {
        let order = compare_slot(5, a.slot5.as_ref(), b.slot5.as_ref(), true)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    let order = a.technique_set_name.cmp(&b.technique_set_name);
    if order != Ordering::Equal {
        return Ok(order);
    }
    Ok(a.name.cmp(&b.name))
}

fn comparator_skippable(cause: &CatalogBuildError) -> bool {
    matches!(
        cause,
        CatalogBuildError::TechniqueSetMissing { .. }
            | CatalogBuildError::TechniqueSetNamespaceMismatch { .. }
            | CatalogBuildError::TechniqueGraphMissing { .. }
            | CatalogBuildError::TechniqueBodyMissing { .. }
            | CatalogBuildError::ShaderIdentityMissing { .. }
            | CatalogBuildError::MaterialConstantMissing { .. }
            | CatalogBuildError::LiteralPixelConstantMissing { .. }
    )
}

fn skip_cause_key(cause: &CatalogBuildError) -> &'static str {
    match cause {
        CatalogBuildError::TechniqueSetMissing { .. } => "TechniqueSetMissing",
        CatalogBuildError::TechniqueSetNamespaceMismatch { .. } => "TechniqueSetNamespaceMismatch",
        CatalogBuildError::TechniqueGraphMissing { .. } => "TechniqueGraphMissing",
        CatalogBuildError::TechniqueBodyMissing { .. } => "TechniqueBodyMissing",
        CatalogBuildError::ShaderIdentityMissing { .. } => "ShaderIdentityMissing",
        CatalogBuildError::MaterialConstantMissing { .. } => "MaterialConstantMissing",
        CatalogBuildError::LiteralPixelConstantMissing { .. } => "LiteralPixelConstantMissing",
        CatalogBuildError::SortedMaterialCapacity { .. } => "SortedMaterialCapacity",
        _ => "other",
    }
}

fn leftover_t5_common_yields_capacity(material: &assets::AuthoredMaterial) -> bool {
    leftover_t5_common_yields_capacity_parts(
        material.t5_state_bits_entry.is_some(),
        material.zone.as_str(),
    )
}

fn leftover_t5_common_yields_capacity_parts(has_t5_state_bits: bool, zone: &str) -> bool {
    has_t5_state_bits && zone == assets::ZoneOwner::COMMON_MP.as_str()
}

fn leftover_iw5_common_yields_capacity(material: &assets::AuthoredMaterial) -> bool {
    leftover_iw5_common_yields_capacity_parts(
        material.iw5_state_bits_entry.is_some(),
        material.zone.as_str(),
    )
}

fn leftover_iw5_common_yields_capacity_parts(has_iw5_state_bits: bool, zone: &str) -> bool {
    has_iw5_state_bits && zone == assets::ZoneOwner::COMMON_MP.as_str()
}

fn is_foreign_common_leftover(material: &assets::AuthoredMaterial) -> bool {
    leftover_t5_common_yields_capacity(material) || leftover_iw5_common_yields_capacity(material)
}

fn insertion_sort_by_comparator(
    records: &mut [ComparatorRecord],
    slot_gap_n: &mut u32,
) -> Result<(), CatalogBuildError> {
    for index in 1..records.len() {
        let mut cursor = index;
        while cursor != 0
            && compare_sorted_materials(&records[cursor], &records[cursor - 1], slot_gap_n)?
                == Ordering::Less
        {
            records.swap(cursor, cursor - 1);
            cursor -= 1;
        }
    }
    Ok(())
}

fn append_foreign_leftover(
    prefix: Vec<ComparatorRecord>,
    foreign_leftover: Vec<ComparatorRecord>,
) -> Result<Vec<ComparatorRecord>, CatalogBuildError> {
    if prefix.len() > SortedMaterialOrdinal::RETAIL_LIMIT {
        return Err(CatalogBuildError::SortedMaterialCapacity {
            count: prefix.len(),
            capacity: SortedMaterialOrdinal::RETAIL_LIMIT,
        });
    }
    let mut keep = prefix;
    keep.extend(foreign_leftover);
    if keep.len() > SortedMaterialOrdinal::LIMIT {
        return Err(CatalogBuildError::SortedMaterialCapacity {
            count: keep.len(),
            capacity: SortedMaterialOrdinal::LIMIT,
        });
    }
    Ok(keep)
}

fn skip_tech_label(counts: &BTreeMap<String, u32>) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let mut ranked: Vec<(&str, u32)> = counts
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(6)
            .map(|(name, count)| format!("{name}:{count}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn build_sorted_material_table(
    source: &assets::MaterialCatalog,
) -> Result<
    (
        Vec<MaterialAssetId>,
        Vec<Option<SortedMaterialOrdinal>>,
        u32,
        u32,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    CatalogBuildError,
> {
    let mut records = Vec::new();
    let mut skipped_n = 0u32;
    let mut slot_gap_n = 0u32;
    let mut first_skip: Option<(usize, CatalogBuildError)> = None;
    let mut skip_tech_counts = BTreeMap::<String, u32>::new();
    let mut skip_cause_counts = BTreeMap::<String, u32>::new();
    let mut first_x0_skip: Option<String> = None;
    for index in 0..source.materials.len() {
        match comparator_record(source, index) {
            Ok(record) => records.push(record),
            Err(cause) if comparator_skippable(&cause) => {
                let tech = source.materials[index].technique_set.as_str();
                *skip_tech_counts.entry(tech.to_owned()).or_default() += 1;
                *skip_cause_counts
                    .entry(skip_cause_key(&cause).to_owned())
                    .or_default() += 1;
                if first_x0_skip.is_none() && tech.contains("x0") {
                    first_x0_skip = Some(format!(
                        "{cause:?} {} tech={tech}",
                        source.materials[index].name
                    ));
                }
                if first_skip.is_none() {
                    first_skip = Some((index, cause));
                }
                skipped_n = skipped_n.saturating_add(1);
            }
            Err(cause) => return Err(cause),
        }
    }
    let mut iw4_records = Vec::new();
    let mut leftover_records = Vec::new();
    for record in records {
        let leftover = source
            .materials
            .get(usize::from(record.asset_id.0))
            .expect("comparator record asset id is a catalog slot");
        if is_foreign_common_leftover(leftover) {
            leftover_records.push(record);
        } else {
            iw4_records.push(record);
        }
    }
    insertion_sort_by_comparator(&mut iw4_records, &mut slot_gap_n)?;
    insertion_sort_by_comparator(&mut leftover_records, &mut slot_gap_n)?;
    let records = append_foreign_leftover(iw4_records, leftover_records)?;
    let sorted_first_skip = first_skip.as_ref().map(|(index, cause)| {
        let name = source
            .materials
            .get(*index)
            .map(|material| material.name.as_str())
            .unwrap_or("<out-of-range>");
        format!("{cause:?} {name}")
    });
    let skip_tech = skip_tech_label(&skip_tech_counts);
    let skip_cause = skip_tech_label(&skip_cause_counts);
    if sorted_first_skip.is_some() {
        diag::info!(
            World,
            "drawsurf sorted materials: skipped={skipped_n} ranked={} first_skip={} skip_cause={} skip_tech={} first_x0={}",
            records.len(),
            sorted_first_skip.as_deref().unwrap_or("-"),
            skip_cause.as_deref().unwrap_or("-"),
            skip_tech.as_deref().unwrap_or("-"),
            first_x0_skip.as_deref().unwrap_or("-"),
        );
    }
    let asset_ids_by_ordinal = records
        .iter()
        .map(|record| record.asset_id)
        .collect::<Vec<_>>();
    let mut ordinals_by_asset_id = vec![None; source.materials.len()];
    for (ordinal, record) in records.iter().enumerate() {
        ordinals_by_asset_id[usize::from(record.asset_id.0)] = Some(SortedMaterialOrdinal::new(
            u32::try_from(ordinal).map_err(|_| CatalogBuildError::SortedMaterialCapacity {
                count: records.len(),
                capacity: SortedMaterialOrdinal::LIMIT,
            })?,
        )?);
    }
    Ok((
        asset_ids_by_ordinal,
        ordinals_by_asset_id,
        skipped_n,
        slot_gap_n,
        sorted_first_skip,
        skip_tech,
        skip_cause,
    ))
}

pub fn capture_runtime_catalog(source: &assets::MaterialCatalog) -> RuntimeMaterialCatalog {
    let mut technique_sets = Vec::new();
    let mut vertex_decls = Vec::<(u32, RuntimeVertexDecl)>::new();
    for facts in source.technique_set_facts() {
        let mut slots = vec![None; TECHNIQUE_SLOT_COUNT];
        if let Some(graph) = facts.table.as_ref().and_then(|table| table.graph.as_ref()) {
            for (slot_index, technique) in graph.slots.iter().enumerate().take(TECHNIQUE_SLOT_COUNT)
            {
                let Some(technique) = technique else {
                    continue;
                };
                let passes = technique
                    .passes
                    .iter()
                    .map(|pass| {
                        let shader = |reference: &assets::OwnedShaderRef| {
                            reference
                                .shader
                                .and_then(|index| source.shaders.get(index))
                                .and_then(|shader| {
                                    u32::try_from(reference.shader?).ok().map(|asset_slot| {
                                        RuntimeShaderProgramId {
                                            asset_slot,
                                            program_hash: program_hash(&shader.program),
                                        }
                                    })
                                })
                        };

                        let vertex_decl_slot = pass
                            .vertex_decl
                            .and_then(|id| u32::try_from(id).ok())
                            .unwrap_or(u32::MAX);
                        if let Some(authored) = pass
                            .vertex_decl
                            .and_then(|index| source.vertex_decls.get(index))
                        {
                            let record = RuntimeVertexDecl {
                                family: authored.family,
                                name: authored.name.to_string(),
                                stream_count: authored.stream_count,
                                has_optional_source: authored.has_optional_source,
                                routing: authored.routing,
                            };
                            match vertex_decls
                                .binary_search_by_key(&vertex_decl_slot, |(identity, _)| *identity)
                            {
                                Ok(_) => {}
                                Err(at) => vertex_decls.insert(at, (vertex_decl_slot, record)),
                            }
                        }
                        let shader_pair = shader(&pass.vertex_shader)
                            .zip(shader(&pass.pixel_shader))
                            .map(|(vertex, pixel)| RuntimeShaderPair {
                                vertex,
                                pixel,
                                vertex_decl_slot,
                            });
                        let arguments = pass
                            .arguments
                            .iter()
                            .map(|argument| match argument {
                                assets::OwnedShaderArgument::MaterialVertexConstant {
                                    destination,
                                    name_hash,
                                } => RuntimeArgumentBinding::MaterialConstant {
                                    stage: RuntimeShaderStage::Vertex,
                                    destination: *destination,
                                    name_hash: *name_hash,
                                },
                                assets::OwnedShaderArgument::LiteralVertexConstant {
                                    destination,
                                    words,
                                } => RuntimeArgumentBinding::LiteralConstant {
                                    stage: RuntimeShaderStage::Vertex,
                                    destination: *destination,
                                    words: *words,
                                },
                                assets::OwnedShaderArgument::MaterialPixelSampler {
                                    destination,
                                    name_hash,
                                } => RuntimeArgumentBinding::MaterialTexture {
                                    destination: *destination,
                                    name_hash: *name_hash,
                                },
                                assets::OwnedShaderArgument::CodeVertexConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                } => RuntimeArgumentBinding::CodeConstant {
                                    stage: RuntimeShaderStage::Vertex,
                                    destination: *destination,
                                    index: *index,
                                    first_row: *first_row,
                                    row_count: *row_count,
                                },
                                assets::OwnedShaderArgument::CodePixelSampler {
                                    destination,
                                    index,
                                } => RuntimeArgumentBinding::CodeTexture {
                                    destination: *destination,
                                    index: *index,
                                },
                                assets::OwnedShaderArgument::CodePixelConstant {
                                    destination,
                                    index,
                                    first_row,
                                    row_count,
                                } => RuntimeArgumentBinding::CodeConstant {
                                    stage: RuntimeShaderStage::Pixel,
                                    destination: *destination,
                                    index: *index,
                                    first_row: *first_row,
                                    row_count: *row_count,
                                },
                                assets::OwnedShaderArgument::MaterialPixelConstant {
                                    destination,
                                    name_hash,
                                } => RuntimeArgumentBinding::MaterialConstant {
                                    stage: RuntimeShaderStage::Pixel,
                                    destination: *destination,
                                    name_hash: *name_hash,
                                },
                                assets::OwnedShaderArgument::LiteralPixelConstant {
                                    destination,
                                    words,
                                } => RuntimeArgumentBinding::LiteralConstant {
                                    stage: RuntimeShaderStage::Pixel,
                                    destination: *destination,
                                    words: *words,
                                },
                                assets::OwnedShaderArgument::Unknown { argument_type, raw } => {
                                    RuntimeArgumentBinding::Unknown {
                                        argument_type: *argument_type,
                                        raw: *raw,
                                    }
                                }
                            })
                            .collect();
                        let mut runtime_pass = RuntimePass {
                            shader_pair,
                            custom_sampler_flags: pass.custom_sampler_flags,
                            t5_custom_sampler_flags: pass.t5_custom_sampler_flags,
                            per_prim_arg_count: pass.per_prim_arg_count,
                            per_obj_arg_count: pass.per_obj_arg_count,
                            stable_arg_count: pass.stable_arg_count,
                            arguments,
                            color_space: super::pass_color_space::pass_color_space(
                                facts.namespace,
                                slot_index as u8,
                            ),
                        };
                        sort_pass_args_retail(&mut runtime_pass);
                        runtime_pass
                    })
                    .collect();
                slots[slot_index] = Some(RuntimeTechnique {
                    flags: technique.flags,
                    passes,
                });
            }
        }
        technique_sets.push(
            RuntimeTechniqueSet::new(slots)
                .expect("capture technique set is TECHNIQUE_SLOT_COUNT slots")
                .with_identity(
                    facts.world_vert_format,
                    facts.namespace,
                    facts.name.to_string(),
                ),
        );
    }

    let mut sorted_first_skip = None;
    let mut skip_tech = None;
    let mut skip_cause = None;
    let sorted_materials = match build_sorted_material_table(source) {
        Ok((
            asset_ids_by_ordinal,
            ordinals_by_asset_id,
            skipped_n,
            slot_gap_n,
            first,
            tech,
            cause,
        )) => {
            sorted_first_skip = first;
            skip_tech = tech;
            skip_cause = cause;
            RuntimeSortedMaterialTable::Ready {
                asset_ids_by_ordinal,
                ordinals_by_asset_id,
                skipped_n,
                slot_gap_n,
            }
        }
        Err(cause) => RuntimeSortedMaterialTable::BuildFailed(cause),
    };

    let materials: Vec<RuntimeMaterial> = source
        .materials
        .iter()
        .enumerate()
        .map(|(material_index, material)| {
            let key = assets::TechsetKey::new(material.namespace, material.technique_set.as_str());
            let local_index = match source.resolve_technique_set(key) {
                assets::TechsetResolve::Hit { index, .. } => Some(index),
                assets::TechsetResolve::GraphMissing { .. }
                | assets::TechsetResolve::Foreign { .. }
                | assets::TechsetResolve::Missing => None,
            };
            let local_technique_set = RuntimeTechniqueSetId(
                local_index
                    .and_then(|index| u32::try_from(index).ok())
                    .unwrap_or(u32::MAX),
            );
            let asset_id = MaterialAssetId(
                u16::try_from(material_index)
                    .expect("material catalog index exceeds the retail u16 asset-id domain"),
            );
            let baked_draw_surf =
                sorted_materials
                    .ordinal_for_asset_id(material_index)
                    .map(|ordinal| {
                        let table = material.technique_table.as_ref();
                        dpvs_iw4::bake_material_draw_surf_key(dpvs_iw4::MaterialDrawSurfBakeInput {
                            sort_key: material.sort_key,
                            info_game_flags: material.info_game_flags,
                            material_sorted_index: ordinal.retail_sort_band(),
                            technique0_absent: table.is_none_or(|table| table.slots & 1 == 0),
                            technique1_present: table.is_some_and(|table| table.slots & 2 != 0),
                            material_byte_4b: material.state_flags,
                            technique0_flags: table.map_or(0, |table| table.technique0_flags),
                        })
                        .packed
                    });
            let [uv_anim, falloff_parms, falloff_begin, falloff_end] =
                assets::MaterialCatalog::material_animation(material);
            RuntimeMaterial {
                asset_id,
                name: material.name.to_string(),
                namespace: material.namespace,
                technique_set: material.technique_set.to_string(),
                baked_draw_surf,
                local_technique_set,
                remap: original_remap_resolution(),
                state_bits_entry: material.state_bits_entry,
                state_bits_table: material.state_bits.clone(),
                camera_region: material.camera_region,
                sort_key: material.sort_key,
                info_game_flags: material.info_game_flags,
                state_flags: material.state_flags,
                surface_type_bits: material.surface_type_bits,
                unlit: source.is_unlit(material).unwrap_or(false),
                takes_model_lighting: source.takes_model_lighting(material) == Some(true),
                uses_model_lighting_const: material
                    .route
                    .is_some_and(|route| route.uses_model_lighting_const),
                square_color_map: source.color_map_transform(material)
                    == assets::ColorMapTransform::Square,
                shadow_only: source.is_shadowcaster(material),
                cull_mode: match source.cull_face(material) {
                    Some(assets::MaterialCullFace::Back) => Some(0),
                    Some(assets::MaterialCullFace::Front) => Some(1),
                    Some(assets::MaterialCullFace::None) | None => None,
                },
                uv_anim_bits: uv_anim.map(f32::to_bits),
                falloff_parms_bits: falloff_parms.map(f32::to_bits),
                falloff_begin_bits: falloff_begin.map(f32::to_bits),
                falloff_end_bits: falloff_end.map(f32::to_bits),
                env_map_parms_bits: material
                    .constants
                    .iter()
                    .find(|constant| constant.name.starts_with(b"envMapParms"))
                    .map(|constant| constant.literal.map(f32::to_bits))
                    .unwrap_or([0; 4]),
                textures: {
                    let mut textures = material
                        .textures
                        .iter()
                        .map(|texture| {
                            (
                                texture.name_hash,
                                texture.image.and_then(|index| {
                                    u32::try_from(index)
                                        .ok()
                                        .map(|image| RuntimeTextureBinding {
                                            image: RuntimeImageId(image),
                                            sampler_state: texture.sampler_state,
                                            semantic: texture.semantic,
                                        })
                                }),
                            )
                        })
                        .collect::<Vec<_>>();
                    textures.sort_by_key(|(hash, _)| *hash);
                    textures
                },
                constants: {
                    let mut constants = material
                        .constants
                        .iter()
                        .map(|constant| (constant.name_hash, constant.literal.map(f32::to_bits)))
                        .collect::<Vec<_>>();
                    constants.sort_by_key(|(hash, _)| *hash);
                    constants
                },
            }
        })
        .collect();
    let mut material_indices_by_name = BTreeMap::new();
    for (index, material) in materials.iter().enumerate() {
        material_indices_by_name
            .entry(material.name.clone())
            .or_insert(index);
    }
    let material_indices_by_name = material_indices_by_name.into_iter().collect();

    let shader_programs = source
        .shaders
        .iter()
        .enumerate()
        .map(|(asset_slot, shader)| {
            let stage = if shader.is_vertex() {
                RuntimeShaderStage::Vertex
            } else if shader.is_pixel() {
                RuntimeShaderStage::Pixel
            } else {
                panic!(
                    "material shader corpus contains non-shader asset type {:?} at slot {asset_slot}",
                    shader.kind
                );
            };
            let asset_slot = u32::try_from(asset_slot)
                .expect("shader catalog index exceeds the runtime u32 asset-slot domain");

            if shader.program.is_empty() {
                return None;
            }
            Some(RuntimeShaderProgram {
                id: RuntimeShaderProgramId {
                    asset_slot,
                    program_hash: program_hash(&shader.program),
                },
                stage,
                name: shader.name.to_string(),
                program: shader.program.clone(),
            })
        })
        .collect();

    RuntimeMaterialCatalog {
        generation_id: source_generation_id(source),
        materials,
        material_indices_by_name,
        technique_sets,
        shader_programs,
        vertex_decls,
        sorted_materials,
        iw5_remap: assets::leftover_selector_census(
            source
                .materials
                .iter()
                .filter_map(|material| material.iw5_state_bits_entry.as_ref()),
        ),
        t5_remap: assets::leftover_t5_selector_census(
            source
                .materials
                .iter()
                .filter_map(|material| material.t5_state_bits_entry.as_ref()),
        ),
        iw5_fallback_n: source
            .technique_set_facts()
            .iter()
            .filter(|facts| facts.iw5_fallback_table.is_some())
            .count() as u32,
        t5_fallback_n: source
            .technique_set_facts()
            .iter()
            .filter(|facts| facts.t5_fallback_table.is_some())
            .count() as u32,
        sorted_first_skip,
        skip_tech,
        skip_cause,
        leftover_iw5_arg_n: source.leftover_iw5_arg_n,
        leftover_iw5_arg: source.leftover_iw5_arg_top(),
        leftover_iw5_arg2: source.leftover_iw5_arg_ranked(1),
        leftover_t5_arg_n: source.leftover_t5_arg_n,
        leftover_t5_arg: source.leftover_t5_arg_top(),
        leftover_t5_arg2: source.leftover_t5_arg_ranked(1),
        leftover_t5_arg3: source.leftover_t5_arg_ranked(2),
        leftover_t5_arg4: source.leftover_t5_arg_ranked(3),
        leftover_t5_arg5: source.leftover_t5_arg_ranked(4),
        leftover_t5_dest6: source.leftover_t5_arg_dest(6),
        leftover_t5_dest7: source.leftover_t5_arg_dest(7),
        leftover_t5_dest17: source.leftover_t5_arg_dest(17),
        leftover_t5_dest18: source.leftover_t5_arg_dest(18),
        leftover_t5_dest19: source.leftover_t5_arg_dest(19),
        leftover_t5_dest9: source.leftover_t5_arg_dest(9),
        leftover_t5_dest10: source.leftover_t5_arg_dest(10),
        leftover_t5_dest11: source.leftover_t5_arg_dest(11),
        leftover_t5_dest20: source.leftover_t5_arg_dest(20),
        leftover_t5_dest24: source.leftover_t5_arg_dest(24),
        leftover_t5_dest25: source.leftover_t5_arg_dest(25),
        leftover_t5_dest26: source.leftover_t5_arg_dest(26),
        leftover_t5_dest27: source.leftover_t5_arg_dest(27),
        leftover_unknown_n: remaining_unknown_arg_n(source),
    }
}

fn remaining_unknown_arg_n(source: &assets::MaterialCatalog) -> u32 {
    let mut n = 0u32;
    for facts in source.technique_set_facts() {
        for table in facts
            .table
            .iter()
            .chain(facts.iw5_fallback_table.iter())
            .chain(facts.t5_fallback_table.iter())
        {
            let Some(graph) = &table.graph else {
                continue;
            };
            for technique in graph.slots.iter().flatten() {
                for pass in &technique.passes {
                    for argument in &pass.arguments {
                        if matches!(argument, assets::OwnedShaderArgument::Unknown { .. }) {
                            n = n.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    n
}
