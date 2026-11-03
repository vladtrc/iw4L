use bevy::platform::collections::HashMap;

use super::setup::TechType;

pub use render_frame::{
    SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs, TextureBindIdentity,
};
pub use render_material::{
    AdmittedPortFacts, CatalogBuildError, CodeSourceError, CodeSourceLookup,
    ExecutableArgumentBinding, ExecutablePass, ExecutablePassView, LayeredCodeSources,
    MaterialAssetId, MaterialExecution, MaterialGenerationId, MaterialRefusal, PackedCodeArg,
    PackedCodeConstantLane, PackedCodeConstants, PackedCodeSamplerLane, PackedCodeSamplers,
    PackedLocalBanks, PackedLocalSamplerLane, PackedLocalSamplers, PortId, PreparedArgBelts,
    PreparedArgSlice, PreparedMaterial, PreparedMaterialTable, PreparedPass, PreparedTableCensus,
    PreparedTechnique, RemapResolution, RuntimeArgumentBinding, RuntimeCodeSources, RuntimeImageId,
    RuntimeMaterial, RuntimeMaterialCatalog, RuntimePass, RuntimeProgramIdentity,
    RuntimeShaderPair, RuntimeShaderProgram, RuntimeShaderProgramId, RuntimeShaderStage,
    RuntimeSortedMaterialTable, RuntimeTechnique, RuntimeTechniqueSet, RuntimeTechniqueSetId,
    RuntimeTextureBinding, RuntimeVertexDecl, SortedMaterialOrdinal, StableMaterialShell,
    StablePassShell, TECHNIQUE_SLOT_COUNT, add_surf_has_technique, capture_stable_shell,
    draw_binds_code_texture, draw_code_sampler_mask, execute_material, pack_local_banks,
    pack_local_samplers, prepared_draw_technique, rebind_stable_material,
    resolve_material_technique, resolve_sorted_material, smodel_tess_vertex_type, sort_pass_args,
    world_tess_vertex_type, world_tess_vertex_type_authored,
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
    hardware_shadow_compare: bool,
    abi: super::sm3_abi::PassProgramAbi,
    module: std::sync::Arc<super::shader_wgsl::ValidatedPassWgsl>,
    lowering: render_material::ProgramLoweringRecipe,
    wgpu_layout: super::gpu_contract::WgpuPassLayout,
}

impl RuntimeProgramPort {
    pub fn compile(
        catalog: &RuntimeMaterialCatalog,
        pass: &RuntimePass,
        vertex_type: u8,
    ) -> Result<Self, ProgramRegistryError> {
        let prepared = render_material::prepare_program_abi(catalog, pass, vertex_type)
            .map_err(ProgramRegistryError::Preparation)?;
        let id = prepared.id();
        let pair = prepared.pair();
        let (vertex_name, pixel_name) = prepared.names();
        let (abi, programs) = prepared.into_programs();
        let cache_key = programs.cache_key(&abi);
        let module = if let Some(cached) = cache_key.as_deref().and_then(|key| {
            super::wgsl_disk_cache::load(pair, abi.vertex_layout.owner.as_str(), key)
        }) {
            cached
        } else {
            let module = programs
                .lower(&abi)
                .map_err(ProgramRegistryError::Lowering)?;
            super::shader_wgsl::validate_wgsl(&module.source)
                .map_err(ProgramRegistryError::Wgsl)?;
            if let Some(key) = cache_key.as_deref() {
                super::wgsl_disk_cache::store(pair, abi.vertex_layout.owner.as_str(), key, &module);
            }
            module
        };
        diag::wgsl_dump::dump_pass_wgsl(vertex_name, pixel_name, &module.source);
        let wgpu_layout = super::gpu_contract::derive_wgpu_pass_layout(&abi)
            .map_err(ProgramRegistryError::WgpuLayout)?;
        Ok(Self {
            id,
            pair,
            custom_sampler_flags: pass.custom_sampler_flags,
            t5_custom_sampler_flags: pass.t5_custom_sampler_flags,
            arguments: pass.arguments.clone(),
            color_space: pass.color_space,
            hardware_shadow_compare: pass.hardware_shadow_compare,
            abi,
            module: std::sync::Arc::new(module),
            lowering: render_material::ProgramLoweringRecipe::new(id, programs),
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
            && self.hardware_shadow_compare == other.hardware_shadow_compare
            && self.abi.vertex_type == other.abi.vertex_type
    }

    pub fn lowering_recipe(&self) -> render_material::ProgramLoweringRecipe {
        self.lowering.clone()
    }

    pub fn shared_module(&self) -> std::sync::Arc<super::shader_wgsl::ValidatedPassWgsl> {
        std::sync::Arc::clone(&self.module)
    }

    pub fn module(&self) -> &super::shader_wgsl::ValidatedPassWgsl {
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
    ) -> Result<Vec<BoundSurfaceSampler>, &'static str> {
        let mut bound = Vec::new();
        for sampler in &self.abi.samplers {
            let resource = match sampler.source {
                super::sm3_abi::SamplerSource::SurfaceReflectionProbe => {
                    BoundSurfaceSampler::ReflectionProbe {
                        register: sampler.register,
                        id: inputs.reflection_probe.ok_or_else(|| {
                            diag::warn_once!(
                                World,
                                "missing reflection probe for sampler register {}",
                                sampler.register
                            );
                            "missing reflection probe"
                        })?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfacePrimaryLightmap => {
                    BoundSurfaceSampler::PrimaryLightmap {
                        register: sampler.register,
                        id: inputs.primary_lightmap.ok_or_else(|| {
                            diag::warn_once!(
                                World,
                                "missing primary lightmap for sampler register {}",
                                sampler.register
                            );
                            "missing primary lightmap"
                        })?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfaceSecondaryLightmap => {
                    BoundSurfaceSampler::SecondaryLightmap {
                        register: sampler.register,
                        id: inputs.secondary_lightmap.ok_or_else(|| {
                            diag::warn_once!(
                                World,
                                "missing secondary lightmap for sampler register {}",
                                sampler.register
                            );
                            "missing secondary lightmap"
                        })?,
                    }
                }
                super::sm3_abi::SamplerSource::SurfaceSecondaryBLightmap => {
                    BoundSurfaceSampler::SecondaryBLightmap {
                        register: sampler.register,
                        id: inputs.secondary_lightmap.ok_or_else(|| {
                            diag::warn_once!(
                                World,
                                "missing secondary lightmap for sampler register {}",
                                sampler.register
                            );
                            "missing secondary lightmap"
                        })?,
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
            && pass.hardware_shadow_compare == self.hardware_shadow_compare
            && pass.depth_to_colour == self.abi.depth_to_colour
            && self.abi.vertex_type == vertex_type
    }
}

fn same_shader_pair_content(a: RuntimeShaderPair, b: RuntimeShaderPair) -> bool {
    a.vertex.program_hash == b.vertex.program_hash
        && a.pixel.program_hash == b.pixel.program_hash
        && a.vertex_decl_slot == b.vertex_decl_slot
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
                bool,
                bool,
                Vec<RuntimeArgumentBinding>,
            ),
            usize,
        > = HashMap::new();
        for (set_i, set) in catalog.parts().technique_sets.iter().enumerate() {
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
                    pass.hardware_shadow_compare,
                    pass.depth_to_colour,
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
            .parts()
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
            Self::Preparation(refusal) => refusal.census_key(),
            Self::Wgsl(error) => error.census_key(),
            Self::Lowering(error) => error.census_key.clone(),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramRegistryError {
    Preparation(render_material::ProgramAbiRefusal),
    Lowering(render_material::ProgramLoweringError),
    Wgsl(super::shader_wgsl::WgslValidationError),
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
