use bevy::prelude::*;

use super::sm3_abi::{SamplerSource, SamplerTextureDimension};
use super::{
    MaterialGenerationId, MaterialRefusal, PortId, PreparedMaterialTable, RemapResolution,
    RuntimeCodeSources, RuntimeMaterial, RuntimeMaterialCatalog, RuntimeProgramPort,
    RuntimeProgramRegistry, RuntimeTechnique, RuntimeTechniqueSetId, StableMaterialShell, TechType,
};

pub const ALTERNATE_FILM_MATERIAL: &str = "postfx_color2";

pub const POSTFX_MATERIALS: &[&str] = &[
    ALTERNATE_FILM_MATERIAL,
    "dof_downsample",
    "dof_near_coc",
    "small_blur",
    "postfx_dof_color2",
    "filter_symmetric_1",
    "filter_symmetric_2",
    "filter_symmetric_3",
    "filter_symmetric_4",
    "filter_symmetric_5",
    "filter_symmetric_6",
    "filter_symmetric_7",
    "filter_symmetric_8",
];
pub const POSTFX_TECH_TYPE: u8 = 4;
pub const POSTFX_VERTEX_TYPE: u8 = 0;
pub const CODE_TEXTURE_RESOLVED_SCENE: u32 = 10;
pub const RESOLVED_SCENE_SAMPLER: u8 = 0x62;

const STANDARD_FILM_STATE: [u32; 2] = [0x1812_8812, 0xe00e_0002];

pub const GLOW_SETUP_MATERIAL: &str = "glow_consistent_setup_color2";

pub const GLOW_APPLY_MATERIAL: &str = "glow_apply_bloom";
pub const GLOW_MATERIALS: &[&str] = &[GLOW_SETUP_MATERIAL, GLOW_APPLY_MATERIAL];

pub const BLOOD_MATERIALS: &[&str] = &["splatter_alt", "splatter_alt_sp"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostFxAdmissionRefusal {
    MaterialMissing,
    SortedOrdinalMissing,
    RemapMissing { pointer_identity: u32 },
    RemapCycle { first_set: RuntimeTechniqueSetId },
    TechniqueSetOutOfRange { set: RuntimeTechniqueSetId },
    TechniqueNamespaceMismatch,
    TechniqueMissing,
    PassCount { actual: usize },
    Execute(MaterialRefusal),
    StableShellMissing,
    PortMissing { port: PortId },
    ShaderHandleMissing { port: PortId },
    StateBitsMismatch { actual: [u32; 2] },
    AlphaTestEnabled,
    SamplerAbiMismatch,
    CodeSource(String),
}

#[derive(Clone, Debug)]
pub struct RuntimePostFx {
    pub name: &'static str,
    pub generation: MaterialGenerationId,
    pub port: RuntimeProgramPort,
    pub shader: Handle<bevy::shader::Shader>,
    pub shell: StableMaterialShell,
}

#[derive(Clone, Debug)]
pub struct RuntimeBloodMaterial {
    pub film: RuntimePostFx,
    pub texture_slots: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub enum RuntimePostFxResources {
    #[default]
    Uninstalled,
    Refused(PostFxAdmissionRefusal),
    Ready(Vec<RuntimePostFx>),
}

fn postfx_material_location<'a>(
    catalog: &'a RuntimeMaterialCatalog,
    material_name: &str,
) -> Result<(&'a RuntimeMaterial, usize, &'a RuntimeTechnique), PostFxAdmissionRefusal> {
    let name = asset_core::AssetRef::bare_name(material_name);
    let material = catalog
        .parts()
        .materials
        .iter()
        .find(|material| material.name == name)
        .ok_or(PostFxAdmissionRefusal::MaterialMissing)?;
    let set = match material.remap {
        RemapResolution::SelfSet => material.local_technique_set,
        RemapResolution::Resolved(set) => set,
        RemapResolution::Missing { pointer_identity } => {
            return Err(PostFxAdmissionRefusal::RemapMissing { pointer_identity });
        }
        RemapResolution::Cycle { first_set } => {
            return Err(PostFxAdmissionRefusal::RemapCycle { first_set });
        }
    };
    let set_i = usize::try_from(set.0)
        .map_err(|_| PostFxAdmissionRefusal::TechniqueSetOutOfRange { set })?;
    let technique_set = catalog
        .parts()
        .technique_sets
        .get(set_i)
        .ok_or(PostFxAdmissionRefusal::TechniqueSetOutOfRange { set })?;
    if technique_set.namespace != material.namespace || !material.draw_rules.postfx_host_supported {
        return Err(PostFxAdmissionRefusal::TechniqueNamespaceMismatch);
    }
    let technique = technique_set
        .technique(TechType(POSTFX_TECH_TYPE))
        .ok_or(PostFxAdmissionRefusal::TechniqueMissing)?;
    if technique.passes.len() != 1 {
        return Err(PostFxAdmissionRefusal::PassCount {
            actual: technique.passes.len(),
        });
    }
    Ok((material, set_i, technique))
}

pub(crate) fn postfx_compile_location(
    catalog: &RuntimeMaterialCatalog,
    material_name: &str,
) -> Result<(usize, usize), PostFxAdmissionRefusal> {
    let (_, set_i, _) = postfx_material_location(catalog, material_name)?;
    Ok((set_i, 0))
}

fn float4_bits(row: [f32; 4]) -> [u32; 4] {
    row.map(f32::to_bits)
}

pub fn film_sources(
    width: u32,
    height: u32,
    vision: Option<asset_world::FilmVision>,
) -> Result<RuntimeCodeSources, String> {
    let width_i32 =
        i32::try_from(width).map_err(|_| format!("invalid postfx dimensions {width}x{height}"))?;
    let height_i32 =
        i32::try_from(height).map_err(|_| format!("invalid postfx dimensions {width}x{height}"))?;
    let projection = hud_iw4::cmd_buf_set_2d_projection(width_i32, height_i32)
        .ok_or_else(|| format!("invalid postfx dimensions {width}x{height}"))?;
    let mut sources = RuntimeCodeSources::default();
    sources.set_constant(
        super::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        super::code_transpose_matrix_rows(Mat4::from_cols_array(&projection)),
    );
    sources.set_constant_rows(
        22,
        &[float4_bits([
            width as f32,
            height as f32,
            1.0 / width as f32,
            1.0 / height as f32,
        ])],
    );
    render_scene::film::set_film_vision(&mut sources, vision);
    sources
        .set_texture(CODE_TEXTURE_RESOLVED_SCENE, RESOLVED_SCENE_SAMPLER)
        .map_err(|cause| format!("postfx code source: {cause:?}"))?;
    Ok(sources)
}

fn build_postfx_material(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
    material_name: &'static str,
    require_film_state: bool,
) -> RuntimePostFxResources {
    let film = match admit_unlit_2d(
        catalog,
        prepared,
        programs,
        shaders,
        material_name,
        require_film_state,
    ) {
        Ok(film) => film,
        Err(cause) => return RuntimePostFxResources::Refused(cause),
    };
    let sampler_ok = film.port.abi().samplers.iter().all(|binding| {
        binding.dimension == SamplerTextureDimension::D2
            && matches!(
                binding.source,
                SamplerSource::CodeTexture {
                    index: 8 | 10 | 11 | 12 | 15
                }
            )
    });
    if !sampler_ok {
        return RuntimePostFxResources::Refused(PostFxAdmissionRefusal::SamplerAbiMismatch);
    }
    RuntimePostFxResources::Ready(vec![film])
}

pub(crate) fn build_runtime_blood(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
    profile: asset_core::RuntimeProfile,
) -> Result<RuntimeBloodMaterial, PostFxAdmissionRefusal> {
    let material_name = asset_game::BloodOverlayAssets::for_profile(profile).material;
    let film = admit_unlit_2d(catalog, prepared, programs, shaders, material_name, false)?;
    let (material, _, _) = postfx_material_location(catalog, material_name)?;
    let texture_slots = film
        .port
        .abi()
        .samplers
        .iter()
        .map(|binding| match binding.source {
            SamplerSource::MaterialTexture { name_hash }
                if binding.dimension == SamplerTextureDimension::D2 =>
            {
                material
                    .textures
                    .iter()
                    .position(|(hash, _)| *hash == name_hash)
                    .and_then(|slot| u8::try_from(slot).ok())
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(PostFxAdmissionRefusal::SamplerAbiMismatch)?;
    Ok(RuntimeBloodMaterial {
        film,
        texture_slots,
    })
}

fn admit_unlit_2d(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
    material_name: &'static str,
    require_film_state: bool,
) -> Result<RuntimePostFx, PostFxAdmissionRefusal> {
    let (material, _, _) = postfx_material_location(catalog, material_name)?;
    let Some(ordinal) = catalog.ordinal_for_material_name(material_name) else {
        return Err(PostFxAdmissionRefusal::SortedOrdinalMissing);
    };
    let packed = render_material::MaterialDrawKey::new(
        dpvs_iw4::pack(dpvs_iw4::GfxDrawSurfFields {
            material_sorted_index: ordinal.sort_band(),
            primary_sort_key: material.sort_key,
            ..Default::default()
        })
        .packed,
        ordinal.get(),
    );
    let sources = match admission_sources() {
        Ok(sources) => sources,
        Err(cause) => {
            return Err(PostFxAdmissionRefusal::CodeSource(cause));
        }
    };
    let execution = match super::execute_material(
        catalog,
        prepared,
        &sources,
        packed,
        TechType(POSTFX_TECH_TYPE),
        POSTFX_VERTEX_TYPE,
    ) {
        Ok(execution) => execution,
        Err(cause) => {
            return Err(PostFxAdmissionRefusal::Execute(cause));
        }
    };
    let Some(pass) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
        return Err(PostFxAdmissionRefusal::PassCount {
            actual: execution.pass_count(),
        });
    };
    let actual_state = pass.state.authored_words();
    if require_film_state && actual_state != STANDARD_FILM_STATE {
        return Err(PostFxAdmissionRefusal::StateBitsMismatch {
            actual: actual_state,
        });
    }
    if pass.state.alpha_test().is_some() {
        return Err(PostFxAdmissionRefusal::AlphaTestEnabled);
    }
    let Some(port) = programs.get(pass.port).cloned() else {
        return Err(PostFxAdmissionRefusal::PortMissing { port: pass.port });
    };
    let Some(shell) = super::capture_stable_shell(
        catalog,
        prepared,
        packed,
        TechType(POSTFX_TECH_TYPE),
        POSTFX_VERTEX_TYPE,
        &execution,
    ) else {
        return Err(PostFxAdmissionRefusal::StableShellMissing);
    };
    let Some((_, shader)) = programs
        .ports()
        .iter()
        .zip(shaders)
        .find(|(candidate, _)| candidate.id() == pass.port)
    else {
        return Err(PostFxAdmissionRefusal::ShaderHandleMissing { port: pass.port });
    };
    Ok(RuntimePostFx {
        name: material_name,
        generation: catalog.generation_id(),
        port,
        shader: shader.clone(),
        shell,
    })
}

fn admission_sources() -> Result<RuntimeCodeSources, String> {
    let mut sources = film_sources(1, 1, None)?;

    for index in 22..=27 {
        sources.set_constant_rows(index, &[[0; 4]]);
    }
    for index in 10..18 {
        sources.set_constant_rows(index, &[[0; 4]]);
    }
    for index in [8, 11, 12, 15] {
        sources
            .set_texture(index, if index == 15 { 0x61 } else { 0x62 })
            .map_err(|cause| format!("postfx code source: {cause:?}"))?;
    }
    Ok(sources)
}

pub(crate) fn build_runtime_postfx(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
) -> RuntimePostFxResources {
    let mut passes = Vec::new();
    for &name in POSTFX_MATERIALS {
        match build_postfx_material(catalog, prepared, programs, shaders, name, true) {
            RuntimePostFxResources::Ready(mut ready) => passes.append(&mut ready),
            refused => {
                diag::warn!(
                    World,
                    "post-fx admission: RED material={name} cause={refused:?}"
                );
                return refused;
            }
        }
    }
    for &name in GLOW_MATERIALS {
        match build_postfx_material(catalog, prepared, programs, shaders, name, false) {
            RuntimePostFxResources::Ready(mut ready) => passes.append(&mut ready),
            RuntimePostFxResources::Refused(PostFxAdmissionRefusal::MaterialMissing) => {}
            refused => {
                diag::warn!(
                    World,
                    "post-fx glow admission skipped material={name} cause={refused:?}"
                );
            }
        }
    }
    RuntimePostFxResources::Ready(passes)
}
