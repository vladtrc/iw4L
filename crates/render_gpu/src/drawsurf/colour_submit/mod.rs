use crate::drawsurf::scene_depth::{SCENE_DEPTH_FORMAT, SceneDepthTexture};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::upscaling::ViewUpscalingPipeline;
use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::mesh::VertexBufferLayout;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d, texture_3d, texture_cube, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, Buffer, BufferDescriptor,
    BufferInitDescriptor, BufferUsages, ColorTargetState, ColorWrites, CompareFunction,
    DepthBiasState, DepthStencilState, Face, FrontFace, MultisampleState, PipelineCache,
    PolygonMode, PrimitiveState, SamplerBindingType, ShaderStages, SpecializedRenderPipelines,
    TextureFormat, TextureSampleType, TextureView, TextureViewDescriptor, VertexAttribute,
    VertexFormat, VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::{ExtractedView, Msaa, ViewTarget};
use bevy::render::{Render, RenderSystems};

use super::ExtractedRenderFrameProducts;
use super::backend::{
    ModelIndexRingCopyRefuse, ModelIndexStream, PackDraw, PackedEmit, PackedListKind,
    SmodelRigidFlush, draw_spot_shadow_map, draw_sun_shadow_map_forced, draw_surf_list_work_colour,
    pack_sun_shadow_frontend, prim_args_from_world_flush, prim_args_u32_index_span,
};
use super::depth_range::{
    GFX_DEPTH_RANGE_VIEWMODEL, depth_range_type_for_draw, reverse_z_viewport_depth,
};
use super::exact_pipeline::{
    ExactModuleSource, ExactPipelinePlan, ExactPipelineRegistry, ExactPipelineSlot,
};
use super::floatz::{self, ExactFloatZResolve};
use super::gpu_contract::{
    WgpuBindLayoutEntry, WgpuBindingKind, WgpuShaderVisibility, WgpuVertexFormat,
};
use super::gpu_prepare::{
    ConstantPackRefusal, PassConstantBuffers, overlay_packed_code_on_banks, split_bind_layout,
};
use super::gpu_resources::{
    RuntimeUploadedImageRegistry, SamplerTable, TextureBindRefusal, TextureImageTables,
    UploadedTextureBind, padded_upload_len, write_buffer_padded,
};
use super::shadowmap_spot_gpu::{
    SHADOWMAP_SPOT_COLOR_FORMAT, SHADOWMAP_SPOT_DEPTH_FORMAT, ShadowmapSpotGpu,
    ensure_shadowmap_spot_targets,
};
use super::shadowmap_sun_gpu::{
    SHADOWMAP_SUN_COLOR_FORMAT, SHADOWMAP_SUN_DEPTH_FORMAT, ShadowmapSunGpu,
    ensure_shadowmap_sun_target,
};
use super::sm3_wgsl::{PASS_FRAGMENT_ENTRY, alpha_test_fragment_entry};
use super::smodel_cache_gpu::SmodelCacheGpu;
use super::smodel_cached::{
    cached_lighting_location, cached_lighting_vertex_buffers, inject_cached_lighting_attribute,
    lighting_texcoord_register,
};
use super::state::{ChangeState0Host, ChangeState1Host, GfxPassState};
use super::texture_table::{
    self, ExactTextureTable, SceneTextureTables, ShadowTextureTable, TableEpoch,
    TextureTableRefusal,
};
use crate::diag::render_frame_diag::{GPU_SPAN_COLOUR, GPU_SPAN_SPOT, GPU_SPAN_SUN};
use asset_iw4::{D3DCMP_ALWAYS, D3DCMP_EQUAL, D3DCMP_LESS, D3DCMP_LESSEQUAL};
use render_backend::{MaterialExecView, MaterialRunExecutor};
use render_frame::{BspCameraLane, RetainedDrawItem, RetainedDrawKind, SmodelPretessRange};
use render_frame::{
    FrameProduct, FrameProductKind, FrameProductStatus, PackedFrontendLists, SunShadowPartition,
    TextureBindIdentity,
};
use render_material::{
    ExecutablePassView, MaterialExecution, MaterialGenerationId, MaterialRefusal,
    PackedCodeConstantLane, PackedCodeConstants, PortId, PreparedMaterialTable, RuntimeCodeSources,
    RuntimeMaterialCatalog, RuntimeShaderPair, RuntimeShaderStage, RuntimeSortedMaterialTable,
    TechType, prepared_draw_technique,
};

pub const CODE_TEXTURE_FLOATZ: u32 = 0x0f;

pub const CODE_TEXTURE_RESOLVED_POST_SUN: u32 = 9;

pub const CODE_TEXTURE_SHADOWMAP_SUN: u32 = 6;

pub const CODE_TEXTURE_SHADOWMAP_SPOT: u32 = 7;

mod publication;
use publication::ExtractedColourRefs;
pub use publication::{
    ExtractedStaticGeometry, InstalledRenderWorld, PublishedRenderFrame, RenderFrameData,
    RenderWorldData, StaticGeometryIdentity, surface_bounds,
};

mod encode;
pub(super) mod portal_scene;
mod shadow_encode;
mod shadow_prepare;
pub(super) use shadow_prepare::ShadowSubmitScratch;
use shadow_prepare::{ResidentShadowStaticDraws, ShadowmapSpotArena, ShadowmapSunArena};
mod geometry;
use geometry::ExactColourGeometry;
mod indirect;
mod pipeline;
mod prepare_camera;
pub(super) use prepare_camera::ColourSubmitScratch;
mod diagnostics;
mod record;
pub(super) use diagnostics::ExactColourSubmitCensus;
use encode::RecordCensus;
mod residency;
mod skinning;
mod smodel_skinned;

pub use pipeline::cached_lighting_port_variant;

#[derive(bevy::ecs::system::SystemParam)]
struct SkinningDispatch<'w> {
    skin: ResMut<'w, skinning::SkinGpu>,
    pending: ResMut<'w, bevy::render::renderer::PendingCommandBuffers>,
    support: Res<'w, crate::SkinSupport>,
    failure: ResMut<'w, render_scene::SkinFailure>,
}

impl SkinningDispatch<'_> {
    fn run(
        &mut self,
        jobs: &[render_frame::skinning::PackedSkinJob],
        output: &residency::GpuStream,
        device: &RenderDevice,
    ) {
        let skinned = self.skin.dispatch(
            jobs,
            output.buffer(),
            output.len(),
            device,
            &mut self.pending,
            self.support.resident_budget,
        );
        let limits = device.limits();
        if self.failure.reason.is_none() {
            self.failure.reason = skinned.as_ref().err().copied();
        }
        self.support.report(
            render_frame::skinning::SkinBatchLimits {
                storage_bytes: limits.max_storage_buffer_binding_size,
                buffer_bytes: limits.max_buffer_size,
                storage_alignment: u64::from(limits.min_storage_buffer_offset_alignment),
                uniform_alignment: u64::from(limits.min_uniform_buffer_offset_alignment),
                dispatch_groups: limits.max_compute_workgroups_per_dimension,
            },
            self.skin.device_generation(),
        );
        let skinned = skinned.unwrap_or_else(|_| skinning::Skinned::default());
        perf::counter!("skin_jobs", "iw4l.render", skinned.jobs as f64);
        perf::counter!("skin_vertices", "iw4l.render", skinned.vertices as f64);
        perf::counter!(
            "skin_resident_bytes",
            "iw4l.render",
            self.skin.resident_bytes() as f64
        );
    }
}

fn upload_exact_geometry(
    frame: Res<PublishedRenderFrame>,
    mut geometry: ResMut<ExactColourGeometry>,
    mut smodel_cache_gpu: ResMut<SmodelCacheGpu>,
    mut census: ResMut<ExactColourSubmitCensus>,
    mut skinning: SkinningDispatch,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let result = geometry::upload_exact_geometry(
        &frame,
        &mut geometry,
        &mut smodel_cache_gpu,
        &device,
        &queue,
    );
    skinning.run(&frame.skin_jobs, &geometry.xmodel.vertex, &device);
    if let Some(kind) = result.code_mesh_gpu_kind {
        census.preparation.code_mesh_gpu_kind = Some(kind);
    }
}

pub const NEUTRAL_VERTEX_LIGHTING: [u8; 4] = [45, 45, 45, 255];

#[derive(Resource, Default)]
struct ExactColourPipeline {
    generation: MaterialGenerationId,
    ports: Vec<ExactColourPortGpu>,
    by_id: HashMap<PortId, usize>,
}

impl ExactColourPipeline {
    fn get(&self, id: PortId) -> Option<&ExactColourPortGpu> {
        self.by_id.get(&id).map(|&index| &self.ports[index])
    }

    fn rebuild_index(&mut self) {
        self.by_id.clear();
        self.by_id.reserve(self.ports.len());
        for (index, port) in self.ports.iter().enumerate() {
            self.by_id.insert(port.port.id(), index);
        }
    }
}

struct ExactColourPortGpu {
    constants_layout: BindGroupLayoutDescriptor,
    textures_layout: BindGroupLayoutDescriptor,
    vertex_buffers: Vec<VertexBufferLayout>,
    cached_source: Option<Arc<str>>,
    clipped_sources: [std::sync::OnceLock<Arc<d3d9_sm3::PassWgsl>>; 4],
    cached_vertex_buffers: Vec<VertexBufferLayout>,
    port: super::AdmittedExactPort,
}

mod constants;
use constants::{
    ArenaDirty, ArenaPack, GpuConstantArena, PackedArenaUpdate, PlacedConstants,
    append_constant_span, resolve_arena_base,
};

mod binding;
use binding::{BoundTextureKey, ColourBindingLanes};
pub(super) use binding::{ExactColourBindingCache, ExactShadowBindingCache};

#[derive(Resource, Default)]
pub(super) struct ExactPipelineKickCache {
    current: Vec<ExactPipelineSlot>,
    scheduled: HashSet<ExactColourPipelineKey>,
    generation: Option<MaterialGenerationId>,
    views: Vec<(TextureFormat, u32)>,
    demand_revision: u64,
    portal_mode: Option<d3d9_sm3::ExitClipMode>,
}

#[derive(Resource, Default)]
struct ExactConstantArena {
    generation: MaterialGenerationId,
    gpu: [GpuConstantArena; COLOUR_PREPARE_LANES],
}

const COLOUR_PREPARE_LANES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ExactColourPipelineKey {
    target: TextureFormat,
    depth_format: TextureFormat,
    samples: u32,
    state0: ChangeState0Host,
    state1: ChangeState1Host,

    pub(super) port: PortId,

    pub(super) cached_lighting: bool,

    mark_mesh: bool,

    forward_z: bool,
    portal_stencil: bool,
    pub(super) exit_clip: Option<d3d9_sm3::ExitClipMode>,
}

fn exact_colour_target_format(target: TextureFormat, srgb_write: bool) -> TextureFormat {
    if srgb_write {
        target.add_srgb_suffix()
    } else {
        target
    }
}

struct ExactColourTargetViews {
    main_raw: TextureView,
    main_srgb: TextureView,
    sampled_raw: Option<TextureView>,
    sampled_srgb: Option<TextureView>,
}

impl ExactColourTargetViews {
    fn new(target: &ViewTarget) -> Self {
        let raw_format = target.main_texture_format();
        let srgb_format = raw_format.add_srgb_suffix();
        Self {
            main_raw: target.main_texture_view().clone(),
            main_srgb: target.main_texture().create_view(&TextureViewDescriptor {
                label: Some("iw4_exact_colour_main_srgb"),
                format: Some(srgb_format),
                ..Default::default()
            }),
            sampled_raw: target.sampled_main_texture_view().cloned(),
            sampled_srgb: target.sampled_main_texture().map(|texture| {
                texture.create_view(&TextureViewDescriptor {
                    label: Some("iw4_exact_colour_sampled_srgb"),
                    format: Some(srgb_format),
                    ..Default::default()
                })
            }),
        }
    }

    fn attachment_views(&self, srgb_write: bool) -> (&TextureView, Option<&TextureView>) {
        let (main, sampled) = if srgb_write {
            (&self.main_srgb, self.sampled_srgb.as_ref())
        } else {
            (&self.main_raw, self.sampled_raw.as_ref())
        };
        match sampled {
            Some(sampled) => (sampled, Some(main)),
            None => (main, None),
        }
    }
}

fn exact_fragment_entry(
    target: TextureFormat,
    forward_z: bool,
    depth_to_colour: bool,
    alpha_test: Option<d3d9_state::AlphaTest>,
) -> String {
    if forward_z && target == TextureFormat::R32Float && !depth_to_colour {
        PASS_FRAGMENT_ENTRY.to_owned()
    } else {
        alpha_test_fragment_entry(alpha_test)
    }
}

fn exact_pipeline_plan(
    ports: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    key: ExactColourPipelineKey,
) -> (ExactModuleSource, ExactPipelinePlan) {
    let port = ports
        .get(key.port)
        .expect("ExactColourPipelineKey.port is an admitted PortId");
    let depth_compare = if key.state1.depth_test_enable {
        Some(if key.forward_z {
            d3d_zfunc(key.state1.depth_func)
        } else {
            d3d_zfunc_to_reverse_z(key.state1.depth_func)
        })
    } else {
        Some(CompareFunction::Always)
    };
    let depth_to_colour = port.port.abi().depth_to_colour;
    let cached_lighting = key.cached_lighting && port.cached_source.is_some();
    let source = if let Some(mode) = key.exit_clip {
        let index = usize::from(mode == d3d9_sm3::ExitClipMode::FragmentDiscard) * 2
            + usize::from(cached_lighting);
        let module = port.clipped_sources[index]
            .get_or_init(|| {
                let mut module = port
                    .port
                    .lowering
                    .lower_exit_clip(port.port.abi(), mode)
                    .expect("admitted material supports exit clipping");
                if cached_lighting {
                    let location = cached_lighting_location(port.port.abi())
                        .expect("cached lighting location");
                    let register = lighting_texcoord_register(port.port.abi())
                        .expect("cached lighting register");
                    module.source =
                        inject_cached_lighting_attribute(&module.source, location, Some(&register))
                            .expect("clipped cached lighting signature");
                }
                Arc::new(module)
            })
            .clone();
        ExactModuleSource::Clipped(module)
    } else {
        match (cached_lighting, port.cached_source.as_ref()) {
            (true, Some(cached)) => ExactModuleSource::CachedLighting(cached.clone()),
            _ => ExactModuleSource::Port(port.port.shared_module()),
        }
    };
    let effect_depth = !key.forward_z;
    let mut fragment_entry = exact_fragment_entry(
        key.target,
        key.forward_z,
        depth_to_colour,
        key.state0.alpha_test,
    );
    if effect_depth {
        fragment_entry.push_str("_effect_depth");
    }
    let plan = ExactPipelinePlan {
        label: format!(
            "iw4_exact_colour/{:016x}/{}",
            key.port.vertex_program_hash, key.port.vertex_type
        ),
        fragment_entry,
        vertex_buffers: if cached_lighting && !port.cached_vertex_buffers.is_empty() {
            port.cached_vertex_buffers.clone()
        } else {
            let mut buffers = port.vertex_buffers.clone();

            if key.mark_mesh
                && let Some(buffer) = buffers.first_mut()
            {
                buffer.array_stride = asset_iw4::size::GFX_WORLD_VERTEX as u64;
            }
            buffers
        },
        targets: {
            let mut targets = vec![Some(ColorTargetState {
                format: key.target,

                blend: if key.target == TextureFormat::R32Float {
                    None
                } else {
                    key.state0.blend.blend_state()
                },
                write_mask: if key.target == TextureFormat::R32Float && depth_to_colour {
                    ColorWrites::RED
                } else if key.target == TextureFormat::R32Float {
                    key.state0.colour_writes() & ColorWrites::RED
                } else {
                    key.state0.colour_writes()
                },
            })];
            if effect_depth {
                targets.push(Some(ColorTargetState {
                    format: floatz::EFFECT_DEPTH_FORMAT,
                    blend: None,
                    write_mask: if key.state1.depth_write {
                        ColorWrites::RED
                    } else {
                        ColorWrites::empty()
                    },
                }));
            }
            targets
        },
        primitive: exact_primitive_state(key.state0.cull, key.state0.line_fill),
        depth_stencil: DepthStencilState {
            format: key.depth_format,
            depth_write_enabled: Some(key.state1.depth_write),
            depth_compare,
            stencil: if key.portal_stencil {
                portal_scene::material_stencil(key.state1.stencil)
            } else {
                super::scene_depth::stencil_state(key.state1.stencil)
            },
            bias: if key.forward_z {
                polygon_offset_bias_forward_z(key.state1.polyoffset_level)
            } else {
                polygon_offset_bias(key.state1.polyoffset_level)
            },
        },
        multisample: MultisampleState {
            count: key.samples,
            ..Default::default()
        },
        constants_layout: registry.bind_group_layout(device, &port.constants_layout),
        textures_layout: registry.bind_group_layout(device, &port.textures_layout),
        exit_clip_layout: key
            .exit_clip
            .map(|_| portal_scene::exit_plane_layout(device)),
    };
    (source, plan)
}

fn request_exact_pipeline(
    registry: &mut ExactPipelineRegistry,
    ports: &ExactColourPipeline,
    device: &RenderDevice,
    key: ExactColourPipelineKey,
) -> ExactPipelineSlot {
    if let Some(slot) = registry.slot(&key) {
        return slot;
    }
    let (source, plan) = exact_pipeline_plan(ports, registry, device, key);
    registry.request(key, source, plan)
}

fn d3d_zfunc(depth_func_index: u8) -> CompareFunction {
    let d3d = asset_iw4::S_DEPTH_TEST_TABLE
        .get(usize::from(depth_func_index))
        .copied()
        .unwrap_or(D3DCMP_LESSEQUAL);
    match d3d {
        D3DCMP_ALWAYS => CompareFunction::Always,
        D3DCMP_LESS => CompareFunction::Less,
        D3DCMP_EQUAL => CompareFunction::Equal,
        D3DCMP_LESSEQUAL => CompareFunction::LessEqual,
        _ => CompareFunction::LessEqual,
    }
}

fn d3d_zfunc_to_reverse_z(depth_func_index: u8) -> CompareFunction {
    let d3d = asset_iw4::S_DEPTH_TEST_TABLE
        .get(usize::from(depth_func_index))
        .copied()
        .unwrap_or(D3DCMP_LESSEQUAL);
    match d3d {
        D3DCMP_ALWAYS => CompareFunction::Always,
        D3DCMP_LESS => CompareFunction::Greater,
        D3DCMP_EQUAL => CompareFunction::Equal,
        D3DCMP_LESSEQUAL => CompareFunction::GreaterEqual,
        _ => CompareFunction::GreaterEqual,
    }
}

pub(super) fn exact_primitive_state(cull: u8, line_fill: bool) -> PrimitiveState {
    PrimitiveState {
        front_face: FrontFace::Cw,
        cull_mode: match cull {
            1 => Some(Face::Back),
            2 => Some(Face::Front),
            _ => None,
        },
        polygon_mode: if line_fill {
            PolygonMode::Line
        } else {
            PolygonMode::Fill
        },
        ..Default::default()
    }
}

fn polygon_offset_bias(level: u8) -> DepthBiasState {
    let (slope_scale, constant) = asset_iw4::polygon_offset_wgpu_defaults(u32::from(level));
    DepthBiasState {
        constant: -constant,
        slope_scale: -slope_scale,
        clamp: 0.0,
    }
}

fn polygon_offset_bias_forward_z(level: u8) -> DepthBiasState {
    let (slope_scale, constant) = asset_iw4::polygon_offset_wgpu_defaults(u32::from(level));
    DepthBiasState {
        constant,
        slope_scale,
        clamp: 0.0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GpuSubmitRefusal {
    PortalSliceCameraMissing,
    NoExactPort {
        pair: RuntimeShaderPair,
    },
    ConstantPack(ConstantPackRefusal),
    TextureBind(TextureBindRefusal),
    TextureTable(TextureTableRefusal),
    ProductDependencyNotReady {
        product: FrameProductKind,
    },
    UnsupportedTessKind,

    MultipleCameraViews {
        views: u32,
    },
    WorldVertex(render_frame::WorldVertexRefusal),
    PackedVertex(render_frame::PackedVertexRefusal),
    MissingSurfaceRange {
        surf: u16,
    },
    MissingSmodelRange {
        surface: u32,
    },
    MissingXModelRange {
        surface: u32,
    },
    MissingCodeMeshRange {
        draw: u32,
    },
    MissingParticleCloudRange {
        draw: u32,
    },
    MissingMarkMeshRange {
        draw: u32,
    },
    MissingGlassMeshRange {
        draw: u32,
    },
    EmptyIndexRange {
        surf: u16,
    },
    EmptySmodelIndexRange {
        surface: u32,
    },
    EmptyXModelIndexRange {
        surface: u32,
    },
    EmptyCodeMeshIndexRange {
        draw: u32,
    },
    EmptyParticleCloudIndexRange {
        draw: u32,
    },
    EmptyMarkMeshIndexRange {
        draw: u32,
    },
    EmptyGlassMeshIndexRange {
        draw: u32,
    },
    PipelineNotReady,
    SmodelCacheIndexEmpty {
        placement: u32,
    },
    SmodelCacheIndicesMissing {
        cache_index: u16,
    },
    SmodelXSurfacePathUnread {
        placement: u32,
    },

    SmodelCachedWithoutDestRange {
        placement: u32,
    },
    SmodelSkinnedDestMissing {
        placement: u32,
    },
    ConstantArenaMissing,

    WorldPretessSpanBeyondLimit {
        start: u32,
        count: u32,
        logical_len: u32,
        epoch: u64,
    },

    WorldPretessEpochMismatch {
        span_epoch: u64,
        layout_epoch: u64,
    },
}

fn exec_tables(
    extracted: ExtractedColourRefs<'_>,
) -> Option<(&RuntimeMaterialCatalog, &PreparedMaterialTable)> {
    Some((
        extracted.world.catalog.as_deref()?,
        extracted.world.prepared.as_deref()?,
    ))
}

fn colour_census_clock(on: bool) -> Option<Instant> {
    on.then(Instant::now)
}

fn colour_census_ms(start: Option<Instant>) -> Option<f32> {
    start.map(|t| t.elapsed().as_secs_f32() * 1000.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldPretessKey {
    world_products: frame::WorldProducts,
    geometry_generation: MaterialGenerationId,
    world_run_revision: u64,
    colour_world: u64,
    light_world: u64,
    emissive_world: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ColourPackKey {
    colour_digest: u64,
    light_digest: u64,
    emissive_digest: u64,
    pretess: WorldPretessKey,
    smodel_index_count: usize,
    xmodel_topology_revision: u64,
    xmodel_index_count: usize,
}

struct ColourRowPlan {
    draw_index: u32,
    world_surface_override: Option<u16>,

    technique: TechType,
    index_span: Option<(u32, u32)>,

    layout_epoch: u64,
}

struct ColourPackPlan {
    key: ColourPackKey,
    packed: PackedFrontendLists,
    work: render_backend::ColourDrawListWork,

    world_rows: Vec<Option<WorldPackedRowMeta>>,

    row_plan: Vec<ColourRowPlan>,
}

fn colour_pack_key(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    pretess: WorldPretessKey,
    smodel_index_count: usize,
    xmodel_topology_revision: u64,
    xmodel_index_count: usize,
) -> ColourPackKey {
    ColourPackKey {
        colour_digest: colour.list_digest,
        light_digest: light.list_digest,
        emissive_digest: emissive.list_digest,
        pretess,
        smodel_index_count,
        xmodel_topology_revision,
        xmodel_index_count,
    }
}

fn mix_u64(id: &mut u64, word: u64) {
    *id ^= word;
    *id = id.wrapping_mul(0x0000_0100_0000_01b3);
}

fn mix_surface_samplers_gpu(id: &mut u64, samplers: super::SurfaceSamplerInputs) {
    let encode = |value: Option<u8>| value.map_or(0, |value| u64::from(value) + 1);
    mix_u64(id, encode(samplers.reflection_probe.map(|value| value.0)));
    mix_u64(id, encode(samplers.primary_lightmap.map(|value| value.0)));
    mix_u64(id, encode(samplers.secondary_lightmap.map(|value| value.0)));
}

fn expanded_world_id(product: &FrameProduct, run_surfs: &[u16]) -> u64 {
    let mut id = 0xcbf2_9ce4_8422_2325;
    for draw in &product.ordered_draws {
        let RetainedDrawKind::World {
            surf, run, run_off, ..
        } = draw.kind
        else {
            continue;
        };
        mix_u64(&mut id, u64::from(surf));
        mix_u64(&mut id, u64::from(run));
        mix_u64(&mut id, u64::from(run_off));
        mix_u64(&mut id, draw.key);
        mix_surface_samplers_gpu(&mut id, draw.surface_samplers);
        let members = run.max(1);
        for offset in 0..members {
            let member = if run <= 1 {
                surf
            } else {
                run_surfs
                    .get(run_off as usize + usize::from(offset))
                    .copied()
                    .unwrap_or(u16::MAX)
            };
            mix_u64(&mut id, u64::from(member));
        }
    }
    id
}

fn world_pretess_key(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    run_surfs: &[u16],
    world_products: frame::WorldProducts,
    geometry_generation: MaterialGenerationId,
    world_run_revision: u64,
) -> WorldPretessKey {
    WorldPretessKey {
        world_products,
        geometry_generation,
        world_run_revision,
        colour_world: expanded_world_id(colour, run_surfs),
        light_world: expanded_world_id(light, run_surfs),
        emissive_world: expanded_world_id(emissive, run_surfs),
    }
}

#[derive(Resource, Default)]
struct CameraPrepareState {
    active: bool,
    samples: u32,
    needs_floatz: bool,
    needs_resolved_scene: bool,
    world_ib_skip: bool,
    ready_draws: u32,
    refused_draws: u32,
    pipeline_not_ready: u32,
    last_refusal: Option<GpuSubmitRefusal>,
    submit_refusals: BTreeMap<(&'static str, &'static str), u32>,
    exec_refused: u32,
    exec_refusals: BTreeMap<(&'static str, &'static str), u32>,
    unsupported_state: UnsupportedStateCensus,
    bsp_submit_refused_surfaces: [u32; 4],
    pipeline_wait: PipelineWaitCensus,
    bind_smodel_mats: BTreeMap<String, u32>,
    last_markmesh_refusal: Option<GpuSubmitRefusal>,
    last_markmesh_exec_skip: Option<&'static str>,
    markmesh_missing_58: u32,
    last_glassmesh_exec_skip: Option<&'static str>,
    last_glassmesh_refusal: Option<GpuSubmitRefusal>,
    last_mark_packed_custom: Option<u8>,
    last_mark_packed_scene_light: Option<u8>,
    last_mark_lmap_sampler: Option<u32>,
    last_glass_packed_probe: Option<u8>,
    last_glass_probe_sampler: Option<u32>,
    markmesh_hits: usize,
    glassmesh_hits: usize,
    prepared_hits: u32,
    prepare_cost: PrepareCost,
    colour_run_census: render_backend::MaterialRunCensus,
    focused_object_id: Option<u16>,
    viewmodel_held: usize,
}

pub fn bind_group_layout_from_entries(
    label: &'static str,
    entries: &[WgpuBindLayoutEntry],
    dynamic_uniforms: bool,
) -> BindGroupLayoutDescriptor {
    let mut built = Vec::with_capacity(entries.len());
    for entry in entries {
        let visibility = match entry.visibility {
            WgpuShaderVisibility::Vertex => ShaderStages::VERTEX,
            WgpuShaderVisibility::Fragment => ShaderStages::FRAGMENT,
            WgpuShaderVisibility::VertexFragment => ShaderStages::VERTEX_FRAGMENT,
        };
        let builder = match entry.kind {
            WgpuBindingKind::UniformBuffer { min_size } => {
                uniform_buffer_sized(dynamic_uniforms, NonZeroU64::new(min_size))
                    .visibility(visibility)
            }
            WgpuBindingKind::ReadOnlyStorageBuffer => {
                storage_buffer_read_only::<[u32; 4]>(false).visibility(visibility)
            }
            WgpuBindingKind::TextureArray { dimension, count } => {
                let sample = TextureSampleType::Float { filterable: true };
                match dimension {
                    super::SamplerTextureDimension::D2 => texture_2d(sample),
                    super::SamplerTextureDimension::Cube => texture_cube(sample),
                    super::SamplerTextureDimension::D3 => texture_3d(sample),
                }
                .visibility(visibility)
                .count(texture_table::array_count(count))
            }
            WgpuBindingKind::SamplerArray { count } => sampler(SamplerBindingType::Filtering)
                .visibility(visibility)
                .count(texture_table::array_count(count)),
        };
        built.push(builder.build(u32::from(entry.binding), visibility));
    }
    BindGroupLayoutDescriptor::new(label, &built)
}

pub fn vertex_layouts_from_contract(
    layout: &super::gpu_contract::WgpuPassLayout,
) -> Vec<VertexBufferLayout> {
    let last_stream = layout
        .vertex_buffers
        .iter()
        .map(|buffer| buffer.stream)
        .max();
    (0..last_stream.map_or(0, |stream| usize::from(stream) + 1))
        .map(|stream| {
            let Some(buffer) = layout
                .vertex_buffers
                .iter()
                .find(|buffer| usize::from(buffer.stream) == stream)
            else {
                return VertexBufferLayout {
                    array_stride: 0,
                    step_mode: VertexStepMode::Vertex,
                    attributes: Vec::new(),
                };
            };
            VertexBufferLayout {
                array_stride: buffer.array_stride,
                step_mode: VertexStepMode::Vertex,
                attributes: buffer
                    .attributes
                    .iter()
                    .map(|attribute| VertexAttribute {
                        format: match attribute.format {
                            WgpuVertexFormat::Float32x2 => VertexFormat::Float32x2,
                            WgpuVertexFormat::Float16x2 => VertexFormat::Float16x2,
                            WgpuVertexFormat::Float32x3 => VertexFormat::Float32x3,
                            WgpuVertexFormat::Float32x4 => VertexFormat::Float32x4,
                            WgpuVertexFormat::Unorm8x4 => VertexFormat::Unorm8x4,
                            WgpuVertexFormat::Uint8x4 => VertexFormat::Uint8x4,
                        },
                        offset: attribute.offset,
                        shader_location: attribute.location,
                    })
                    .collect(),
            }
        })
        .collect()
}

pub fn colour_ports_static(
    extracted_generation: MaterialGenerationId,
    extracted_port_len: usize,
    cpu_generation: MaterialGenerationId,
    cpu_port_len: usize,
) -> bool {
    extracted_generation == cpu_generation
        && extracted_port_len == cpu_port_len
        && extracted_port_len > 0
}

fn is_viewmodel_colour_draw(kind: &RetainedDrawKind, key: u64) -> bool {
    depth_range_type_for_draw(kind, key) == GFX_DEPTH_RANGE_VIEWMODEL
}

fn viewmodel_colour_submits_when_pipelines_ready(any_viewmodel_pipeline_not_ready: bool) -> bool {
    !any_viewmodel_pipeline_not_ready
}

fn sun_shadow_view_missing(view_ready: bool, binds_sun_shadow: bool) -> bool {
    binds_sun_shadow && !view_ready
}

fn sun_shadow_content_missing(sun_recorded: bool, binds_sun_shadow: bool) -> bool {
    binds_sun_shadow && !sun_recorded
}

fn retain_camera_draws_with_sun_content(
    prepared: &mut Vec<PreparedExactDraw>,
    sun_recorded: bool,
    spot_recorded: bool,
) -> u32 {
    let mut held = 0u32;
    prepared.retain(|draw| {
        if sun_shadow_content_missing(sun_recorded, draw.shadow_dependencies.sun)
            || (draw.shadow_dependencies.spot && !spot_recorded)
        {
            held = held.saturating_add(1);
            false
        } else {
            true
        }
    });
    held
}

fn publish_this_frame_sun_shadow_view(
    products: &ExtractedRenderFrameProducts,
    uploaded: &mut RuntimeUploadedImageRegistry,
    shadowmap: &mut ShadowmapSunGpu,
    device: &RenderDevice,
) -> bool {
    let sun = products.0.product(FrameProductKind::SunShadow);
    if sun.ordered_draws.is_empty() {
        uploaded.publish_frame_target(|registry| &mut registry.sun_shadow, None);
        return false;
    }

    let (color_view, _resized) = ensure_shadowmap_sun_target(shadowmap, device);
    uploaded.publish_frame_target(|registry| &mut registry.sun_shadow, color_view);
    uploaded.sun_shadow.is_some()
}

fn spot_rt_for_light(products: &ExtractedRenderFrameProducts, light_index: u8) -> Option<u8> {
    products
        .0
        .product(FrameProductKind::SpotShadow)
        .spot_slots
        .iter()
        .find(|slot| slot.emitted.light_index == light_index)
        .map(|slot| slot.emitted.plan.render_target_id)
}

fn spot_shadow_view_missing(
    uploaded: &RuntimeUploadedImageRegistry,
    binds: bool,
    select: Option<u8>,
) -> bool {
    if !binds {
        return false;
    }
    let Some(rt) = select else {
        return true;
    };
    match rt {
        lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE => uploaded.spot_shadow_rt10.is_none(),
        lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL => uploaded.spot_shadow_rt11.is_none(),
        _ => true,
    }
}

fn publish_this_frame_spot_shadow_views(
    products: &ExtractedRenderFrameProducts,
    uploaded: &mut RuntimeUploadedImageRegistry,
    shadowmap: &mut ShadowmapSpotGpu,
    device: &RenderDevice,
) -> bool {
    let spot = products.0.product(FrameProductKind::SpotShadow);
    if spot.spot_slots.is_empty() {
        uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt10, None);
        uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt11, None);
        return false;
    }
    ensure_shadowmap_spot_targets(shadowmap, device);

    let large = shadowmap
        .color_view(lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE)
        .cloned();
    let small = shadowmap
        .color_view(lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL)
        .cloned();
    uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt10, large);
    uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt11, small);
    uploaded.spot_shadow_rt10.is_some() || uploaded.spot_shadow_rt11.is_some()
}

fn drawsurf_object_id(key: u64) -> u16 {
    dpvs_iw4::GfxDrawSurf::from_packed(key).object_id()
}

#[derive(Resource, Default)]
pub struct FocusedOwnerSubmitState {
    emitted_frame_id: u64,
}

pub struct FocusedOwnerSubmitReport {
    pub terminal: &'static str,
    pub prepared_surfaces: u32,
    pub prepared_passes: u32,
    pub drawn_passes: u32,
}

impl FocusedOwnerSubmitState {
    pub fn emit(
        &mut self,
        products: &ExtractedRenderFrameProducts,
        material_context: Option<(&[String], &super::gpu_resources::RuntimeImageHandles)>,
        tables: Option<(&RuntimeMaterialCatalog, &PreparedMaterialTable)>,
        report: FocusedOwnerSubmitReport,
    ) {
        let FocusedOwnerSubmitReport {
            terminal,
            prepared_surfaces,
            prepared_passes,
            drawn_passes,
        } = report;

        let Some(focus) = products.0.focus() else {
            return;
        };
        if self.emitted_frame_id == products.0.frame_id {
            return;
        }
        let mut product_surfaces = 0u32;
        let mut execution_ready_surfaces = 0u32;
        let mut materials = BTreeSet::new();
        let mut material_textures = BTreeSet::new();
        if let Some(object_id) = focus.object_id {
            for product in [
                products.0.product(FrameProductKind::Colour),
                products.0.product(FrameProductKind::Light),
                products.0.product(FrameProductKind::Emissive),
            ] {
                for (index, item) in product.ordered_draws.iter().enumerate() {
                    if drawsurf_object_id(item.key) != object_id {
                        continue;
                    }
                    product_surfaces = product_surfaces.saturating_add(1);
                    let resolved = tables.and_then(|(catalog, prepared)| {
                        let tech = product.draw_tech.get(index).copied()?;
                        prepared_draw_technique(
                            catalog,
                            prepared,
                            render_material::MaterialDrawKey::new(item.key, item.material_rank)
                                .with_material_id(item.material_id),
                            tech,
                        )
                        .ok()
                    });
                    execution_ready_surfaces =
                        execution_ready_surfaces.saturating_add(u32::from(resolved.is_some()));
                    let ordinal = world_material_sorted(item.key);
                    let material = material_context
                        .and_then(|(names, _)| names.get(usize::from(ordinal)))
                        .map_or_else(|| ordinal.to_string(), |name| format!("{ordinal}:{name}"));
                    materials.insert(material);
                    if let Some((_, prepared_tech)) = resolved {
                        for (pass_index, pass) in prepared_tech.passes.iter().enumerate() {
                            let Some(local) = pass.local_samplers.as_ref() else {
                                continue;
                            };
                            for lane in &local.lanes {
                                let image_index = lane.texture.image.0 as usize;
                                let image =
                                    material_context.map_or_else(String::new, |(_, images)| {
                                        let image_name = images
                                            .material_names
                                            .get(image_index)
                                            .map(String::as_str)
                                            .unwrap_or("<out-of-range>");
                                        let retained = images
                                            .material_images
                                            .get(image_index)
                                            .is_some_and(Option::is_some);
                                        format!(
                                            ",image_name={image_name},retained={}",
                                            u8::from(retained)
                                        )
                                    });
                                material_textures.insert(format!(
                                "material={ordinal},pass={pass_index},register={},name_hash=0x{:08x},image={},semantic={},sampler=0x{:02x}{image}",
                                lane.register,
                                lane.name_hash,
                                lane.texture.image.0,
                                lane.texture.semantic,
                                lane.texture.sampler_state,
                            ));
                            }
                        }
                    }
                }
            }
        }
        let materials =
            (!materials.is_empty()).then(|| materials.into_iter().collect::<Vec<_>>().join(";"));
        let material_textures = (!material_textures.is_empty())
            .then(|| material_textures.into_iter().collect::<Vec<_>>().join(";"));
        let outcome = if focus.outcome == "planned_empty" {
            "planned_empty"
        } else if focus.outcome != "planned" {
            "plan_refused"
        } else if terminal != "complete" {
            terminal
        } else if product_surfaces == 0 {
            "product_missing"
        } else if execution_ready_surfaces == 0 {
            "material_refused"
        } else if execution_ready_surfaces < product_surfaces {
            "material_partial"
        } else if prepared_surfaces == 0 {
            "gpu_prepare_refused"
        } else if prepared_surfaces < execution_ready_surfaces {
            "gpu_prepare_partial"
        } else if drawn_passes == 0 {
            "draw_refused"
        } else if drawn_passes < prepared_passes {
            "draw_partial"
        } else {
            "drawn"
        };
        perf::instant!("render_owner_submit", "iw4l.render", |ctx| {
            let (frame_id, owner_kind, owner_id, object_id, materials, material_textures): (
                u64,
                &str,
                u32,
                Option<u16>,
                Option<&str>,
                Option<&str>,
            ) = (
                products.0.frame_id,
                "script_model",
                focus.owner_id,
                focus.object_id,
                materials.as_deref(),
                material_textures.as_deref(),
            );
            ctx.arg("frame_id", frame_id as i64);
            ctx.arg("owner_kind", owner_kind);
            ctx.arg("owner_id", i64::from(owner_id));
            ctx.arg("object_id", object_id.map(i64::from));
            ctx.arg("outcome", outcome);
            ctx.arg("materials", materials);
            ctx.arg("material_textures", material_textures);
            ctx.arg("product_surfaces", i64::from(product_surfaces));
            ctx.arg(
                "execution_ready_surfaces",
                i64::from(execution_ready_surfaces),
            );
            ctx.arg("prepared_surfaces", i64::from(prepared_surfaces));
            ctx.arg("prepared_passes", i64::from(prepared_passes));
            ctx.arg("drawn_passes", i64::from(drawn_passes));
        });
        self.emitted_frame_id = products.0.frame_id;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UnsupportedStateCensus {
    unknown_blend_factor: u32,
    unknown_blend_operation: u32,
    stencil: u32,
}

impl UnsupportedStateCensus {
    fn absorb(&mut self, other: Self) {
        self.unknown_blend_factor = self
            .unknown_blend_factor
            .saturating_add(other.unknown_blend_factor);
        self.unknown_blend_operation = self
            .unknown_blend_operation
            .saturating_add(other.unknown_blend_operation);
        self.stencil = self.stencil.saturating_add(other.stencil);
    }

    fn note(&mut self, fields: super::state::UnsupportedStateFields) {
        self.unknown_blend_factor = self
            .unknown_blend_factor
            .saturating_add(u32::from(fields.unknown_blend_factor));
        self.unknown_blend_operation = self
            .unknown_blend_operation
            .saturating_add(u32::from(fields.unknown_blend_operation));
        self.stencil = self.stencil.saturating_add(u32::from(fields.stencil));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AuthoredStateCensus {
    non_add_blend: u32,
    independent_alpha_blend: u32,
    partial_colour_write: u32,
    line_fill: u32,
    stencil: u32,
}

impl AuthoredStateCensus {
    fn note(&mut self, fields: super::state::AuthoredStateFields) {
        self.non_add_blend = self
            .non_add_blend
            .saturating_add(u32::from(fields.non_add_blend));
        self.independent_alpha_blend = self
            .independent_alpha_blend
            .saturating_add(u32::from(fields.independent_alpha_blend));
        self.partial_colour_write = self
            .partial_colour_write
            .saturating_add(u32::from(fields.partial_colour_write));
        self.line_fill = self.line_fill.saturating_add(u32::from(fields.line_fill));
        self.stencil = self.stencil.saturating_add(u32::from(fields.stencil));
    }
}

fn as_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn world_pretess_dest_ib(
    device: &RenderDevice,
    queue: &RenderQueue,
    reuse: Option<Buffer>,
    indices: &[u32],
) -> Option<Buffer> {
    if indices.is_empty() {
        return None;
    }
    let bytes: &[u8] = bytemuck::cast_slice(indices);
    let need = padded_upload_len(bytes.len()) as u64;
    let buf = match reuse {
        Some(existing) if existing.size() >= need => existing,
        _ => device.create_buffer(&BufferDescriptor {
            label: Some("iw4_world_pretess_dest_ib"),
            size: need,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
    };
    write_buffer_padded(queue, &buf, bytes);
    Some(buf)
}

fn smodel_pretess_submit_refusal(
    extracted: ExtractedColourRefs<'_>,
    placement: u32,
    cache_index: u16,
    range: SmodelPretessRange,
) -> Result<(ExactTessBind, u32, u32, u8), GpuSubmitRefusal> {
    if cache_index == 0 {
        return Err(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement });
    }
    if !extracted.world.smc_index_baked.contains(&cache_index) {
        return Err(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index });
    }
    let end = range
        .start
        .checked_add(range.count)
        .and_then(|end| usize::try_from(end).ok());
    if range.count == 0
        || range.count % 3 != 0
        || end.is_none_or(|end| end > extracted.world.smodel_pretess_indices.len())
    {
        return Err(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index });
    }
    Ok((
        ExactTessBind::SmodelCached,
        range.start,
        range.count,
        asset_iw4::vertex_decl::STATICMODELCACHE_VERTEX_TYPE,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExactTessBind {
    World,
    Smodel,
    SmodelCached,
    SmodelSkinned,
    XModel,
    CodeMesh,
    ParticleCloud,
    MarkMesh,
    Glass,
}

#[derive(Clone, Copy)]
struct PreparedDrawSource {
    input_vertex_type: u8,
    world_pretess: bool,
    tess: ExactTessBind,
    start: u32,
    count: u32,
    vertex_type: u8,
    smc_stream_off: u64,
    bsp_kind: Option<BspCameraLane>,
    bsp_run_first: u16,
    bsp_first_surf: u16,
    bsp_surf_count: u16,
}

#[derive(Clone)]
struct PreparedExactDraw {
    slice_clip: Option<Arc<BindGroup>>,
    slice_world: Option<[f64; 4]>,
    aperture_world: Option<render_frame::PortalEntityAperture>,
    pipeline: ExactPipelineSlot,
    port: PortId,

    constants: Option<Arc<PassConstantBuffers>>,

    constant_base: Option<u32>,

    texture_slots: Arc<[u32]>,
    start: u32,
    count: u32,
    tess: ExactTessBind,

    after_scene_resolve: bool,

    owner_object_id: Option<u16>,

    depth_min: f32,
    depth_max: f32,

    ring_epoch: u32,

    smc_stream_off: u64,

    state: super::state::GfxPassState,

    bsp_kind: Option<BspCameraLane>,
    bsp_run_first: u16,
    bsp_first_surf: u16,
    bsp_surf_count: u16,

    bsp_counted: bool,

    shadow_dependencies: ShadowProductDependencies,

    indirect_arg: Option<u32>,
    arena_lane: u8,

    /// Unfolded view plus one; zero is a camera-only draw.
    portal_tag: u8,
}

impl PreparedExactDraw {
    /// The first instance is the unmodified constant-arena row.
    fn first_instance(&self) -> Option<u32> {
        self.constant_base
    }
}

fn bsp_draw_source(kind: &RetainedDrawKind) -> (Option<BspCameraLane>, u16, u16, u16) {
    match *kind {
        RetainedDrawKind::World {
            surf,
            run,
            bsp_kind: Some(kind),
            bsp_run_first: Some(run_first),
            ..
        } => (Some(kind), run_first, surf, run.max(1)),
        RetainedDrawKind::World {
            bsp_kind: Some(_),
            bsp_run_first: None,
            ..
        } => panic!("BSP world draw is missing its producer run identity"),
        _ => (None, 0, 0, 0),
    }
}

const fn bsp_kind_index(kind: BspCameraLane) -> usize {
    match kind {
        BspCameraLane::LitOpaque => 0,
        BspCameraLane::LitTrans => 1,
        BspCameraLane::Emissive => 2,
        BspCameraLane::Decal => 3,
    }
}

fn submit_refusal_family(kind: &RetainedDrawKind, viewmodel: bool) -> &'static str {
    match kind {
        RetainedDrawKind::World { .. } => "world",
        RetainedDrawKind::Smodel { .. } => "smodel",
        RetainedDrawKind::XModel { .. } if viewmodel => "xmodel/fpv",
        RetainedDrawKind::XModel { .. } => "xmodel",
        RetainedDrawKind::CodeMesh { .. } => "codemesh",
        RetainedDrawKind::ParticleCloud { .. } => "particlecloud",
        RetainedDrawKind::MarkMesh { .. } => "markmesh",
        RetainedDrawKind::Glass { .. } => "glassmesh",
    }
}

fn submit_refusal_class(cause: &GpuSubmitRefusal) -> &'static str {
    match cause {
        GpuSubmitRefusal::PortalSliceCameraMissing => "PortalSliceCameraMissing",
        GpuSubmitRefusal::NoExactPort { .. } => "NoExactPort",
        GpuSubmitRefusal::ConstantPack(_) => "ConstantPack",
        GpuSubmitRefusal::TextureBind(_) => "TextureBind",
        GpuSubmitRefusal::TextureTable(_) => "TextureTable",
        GpuSubmitRefusal::ProductDependencyNotReady { .. } => "ProductDependencyNotReady",
        GpuSubmitRefusal::UnsupportedTessKind => "UnsupportedTessKind",
        GpuSubmitRefusal::MultipleCameraViews { .. } => "MultipleCameraViews",
        GpuSubmitRefusal::WorldVertex(_) => "WorldVertex",
        GpuSubmitRefusal::PackedVertex(_) => "PackedVertex",
        GpuSubmitRefusal::MissingSurfaceRange { .. } => "MissingSurfaceRange",
        GpuSubmitRefusal::MissingSmodelRange { .. } => "MissingSmodelRange",
        GpuSubmitRefusal::MissingXModelRange { .. } => "MissingXModelRange",
        GpuSubmitRefusal::MissingCodeMeshRange { .. } => "MissingCodeMeshRange",
        GpuSubmitRefusal::MissingParticleCloudRange { .. } => "MissingParticleCloudRange",
        GpuSubmitRefusal::MissingMarkMeshRange { .. } => "MissingMarkMeshRange",
        GpuSubmitRefusal::MissingGlassMeshRange { .. } => "MissingGlassMeshRange",
        GpuSubmitRefusal::EmptyIndexRange { .. } => "EmptyIndexRange",
        GpuSubmitRefusal::EmptySmodelIndexRange { .. } => "EmptySmodelIndexRange",
        GpuSubmitRefusal::EmptyXModelIndexRange { .. } => "EmptyXModelIndexRange",
        GpuSubmitRefusal::EmptyCodeMeshIndexRange { .. } => "EmptyCodeMeshIndexRange",
        GpuSubmitRefusal::EmptyParticleCloudIndexRange { .. } => "EmptyParticleCloudIndexRange",
        GpuSubmitRefusal::EmptyMarkMeshIndexRange { .. } => "EmptyMarkMeshIndexRange",
        GpuSubmitRefusal::EmptyGlassMeshIndexRange { .. } => "EmptyGlassMeshIndexRange",
        GpuSubmitRefusal::PipelineNotReady => "PipelineNotReady",
        GpuSubmitRefusal::SmodelCacheIndexEmpty { .. } => "SmodelCacheIndexEmpty",
        GpuSubmitRefusal::SmodelCacheIndicesMissing { .. } => "SmodelCacheIndicesMissing",
        GpuSubmitRefusal::SmodelXSurfacePathUnread { .. } => "SmodelXSurfacePathUnread",
        GpuSubmitRefusal::SmodelCachedWithoutDestRange { .. } => "SmodelCachedWithoutDestRange",
        GpuSubmitRefusal::SmodelSkinnedDestMissing { .. } => "SmodelSkinnedDestMissing",
        GpuSubmitRefusal::ConstantArenaMissing => "ConstantArenaMissing",
        GpuSubmitRefusal::WorldPretessSpanBeyondLimit { .. } => "WorldPretessSpanBeyondLimit",
        GpuSubmitRefusal::WorldPretessEpochMismatch { .. } => "WorldPretessEpochMismatch",
    }
}

fn exec_refusal_row(
    kind: &RetainedDrawKind,
    key: u64,
    cause: &MaterialRefusal,
) -> (&'static str, &'static str) {
    let viewmodel = is_viewmodel_colour_draw(kind, key);
    (
        submit_refusal_family(kind, viewmodel),
        material_refusal_class(cause),
    )
}

struct DrawRefusalCensus {
    on: bool,
    submit: BTreeMap<(&'static str, &'static str), u32>,
    exec: BTreeMap<(&'static str, &'static str), u32>,
}

impl DrawRefusalCensus {
    fn new(on: bool) -> Self {
        Self {
            on,
            submit: BTreeMap::new(),
            exec: BTreeMap::new(),
        }
    }

    fn taking(
        on: bool,
        submit: BTreeMap<(&'static str, &'static str), u32>,
        exec: BTreeMap<(&'static str, &'static str), u32>,
    ) -> Self {
        Self { on, submit, exec }
    }

    fn note_exec(&mut self, kind: &RetainedDrawKind, key: u64, cause: &MaterialRefusal) {
        if !self.on {
            return;
        }
        *self
            .exec
            .entry(exec_refusal_row(kind, key, cause))
            .or_default() += 1;
    }

    fn note_submit(&mut self, kind: &RetainedDrawKind, viewmodel: bool, cause: &GpuSubmitRefusal) {
        if !self.on {
            return;
        }
        *self
            .submit
            .entry((
                submit_refusal_family(kind, viewmodel),
                submit_refusal_class(cause),
            ))
            .or_default() += 1;
    }

    fn note_submit_class(&mut self, family: &'static str, class: &'static str, n: u32) {
        if !self.on || n == 0 {
            return;
        }
        *self.submit.entry((family, class)).or_default() += n;
    }
}

fn material_refusal_class(cause: &MaterialRefusal) -> &'static str {
    match cause {
        MaterialRefusal::SortedMaterialTableMissing { .. } => "SortedMaterialTableMissing",
        MaterialRefusal::SortedMaterialOrdinalOutOfRange { .. } => {
            "SortedMaterialOrdinalOutOfRange"
        }
        MaterialRefusal::SortedMaterialBuildFailed { .. } => "SortedMaterialBuildFailed",
        MaterialRefusal::StaleMaterialGeneration { .. } => "StaleMaterialGeneration",
        MaterialRefusal::MissingLightBindings { .. } => "MissingLightBindings",
        MaterialRefusal::MaterialOutOfRange { .. } => "MaterialOutOfRange",
        MaterialRefusal::LocalTechniqueSetOutOfRange { .. } => "LocalTechniqueSetOutOfRange",
        MaterialRefusal::RemappedTechniqueSetOutOfRange { .. } => "RemappedTechniqueSetOutOfRange",
        MaterialRefusal::RemapMissing { .. } => "RemapMissing",
        MaterialRefusal::RemapCycle { .. } => "RemapCycle",
        MaterialRefusal::TechniqueAbsent { .. } => "TechniqueAbsent",
        MaterialRefusal::EmptyTechnique { .. } => "EmptyTechnique",
        MaterialRefusal::StateEntriesMissing => "StateEntriesMissing",
        MaterialRefusal::StateEntryOutOfRange { .. } => "StateEntryOutOfRange",
        MaterialRefusal::StateRowOutOfRange { .. } => "StateRowOutOfRange",
        MaterialRefusal::UnsupportedState { .. } => "UnsupportedState",
        MaterialRefusal::PassIndexOverflow { .. } => "PassIndexOverflow",
        MaterialRefusal::ShaderProgramMissing { .. } => "ShaderProgramMissing",
        MaterialRefusal::UnsupportedShaderPair { .. } => "UnsupportedShaderPair",
        MaterialRefusal::MissingTexture { .. } => "MissingTexture",
        MaterialRefusal::MissingMaterialConstant { .. } => "MissingMaterialConstant",
        MaterialRefusal::MissingCodeConstant { .. } => "MissingCodeConstant",
        MaterialRefusal::CodeConstantRowsOutOfRange { .. } => "CodeConstantRowsOutOfRange",
        MaterialRefusal::MissingCodeTexture { .. } => "MissingCodeTexture",
        MaterialRefusal::MissingLiteralConstant { .. } => "MissingLiteralConstant",
        MaterialRefusal::UnknownArgumentType { .. } => "UnknownArgumentType",
        MaterialRefusal::PassArgCountMismatch { .. } => "PassArgCountMismatch",
        MaterialRefusal::TechniqueSetNamespaceMismatch { .. } => "TechniqueSetNamespaceMismatch",
    }
}

pub fn dump_sorted_material_names(catalog: &RuntimeMaterialCatalog) -> Vec<String> {
    match &catalog.parts().sorted_materials {
        RuntimeSortedMaterialTable::Ready {
            asset_ids_by_ordinal,
            ..
        } => asset_ids_by_ordinal
            .iter()
            .map(|id| {
                catalog
                    .parts()
                    .materials
                    .get(usize::from(id.0))
                    .map(|material| material.name.clone())
                    .unwrap_or_else(|| format!("id{}", id.0))
            })
            .collect(),
        RuntimeSortedMaterialTable::Missing { .. } | RuntimeSortedMaterialTable::BuildFailed(_) => {
            Vec::new()
        }
    }
}

pub fn dump_shader_program_names(catalog: &RuntimeMaterialCatalog) -> Vec<Option<String>> {
    catalog
        .parts()
        .shader_programs
        .iter()
        .map(|slot| slot.as_ref().map(|program| program.name.clone()))
        .collect()
}

fn rank_count_map(map: &BTreeMap<String, u32>, take: usize) -> Option<String> {
    if map.is_empty() {
        return None;
    }
    let mut ranked: Vec<(&String, &u32)> = map.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(take)
            .map(|(name, n)| format!("{name}:{n}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn rank_pair_map(map: &BTreeMap<(&'static str, &'static str), u32>, take: usize) -> Option<String> {
    if map.is_empty() {
        return None;
    }
    let mut ranked: Vec<_> = map.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(take)
            .map(|((family, cause), n)| format!("{family}:{cause}:{n}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

#[derive(Default)]
struct PipelineWaitCensus {
    smodel_mats: BTreeMap<String, u32>,
    world_mats: BTreeMap<String, u32>,
    smodel_ps: BTreeMap<String, u32>,
    world_ps: BTreeMap<String, u32>,
    smodel_keys: HashSet<u64>,
    world_keys: HashSet<u64>,
    ports: HashSet<PortId>,
}

impl PipelineWaitCensus {
    fn record(
        &mut self,
        kind: &RetainedDrawKind,
        viewmodel: bool,
        key: u64,
        execution: &MaterialExecution,
        extracted: ExtractedColourRefs<'_>,
    ) {
        let smodel_mats = &mut self.smodel_mats;
        let world_mats = &mut self.world_mats;
        let smodel_ps = &mut self.smodel_ps;
        let world_ps = &mut self.world_ps;
        let smodel_keys = &mut self.smodel_keys;
        let world_keys = &mut self.world_keys;
        let ports = &mut self.ports;

        let family = submit_refusal_family(kind, viewmodel);
        let ordinal = world_material_sorted(key);
        let name = extracted
            .world
            .sorted_material_names
            .get(usize::from(ordinal))
            .cloned()
            .unwrap_or_else(|| format!("ord{ordinal}"));
        let ps = execution
            .pass(0)
            .map(|pass| {
                extracted
                    .world
                    .shader_program_names
                    .get(pass.shader_pair.pixel.asset_slot as usize)
                    .and_then(|slot| slot.clone())
                    .unwrap_or_else(|| format!("{:016x}", pass.port.pixel_program_hash))
            })
            .unwrap_or_else(|| "<no pass>".to_owned());
        for pass in execution.iter_passes() {
            ports.insert(pass.port);
        }
        match family {
            "smodel" => {
                *smodel_mats.entry(name).or_default() += 1;
                *smodel_ps.entry(ps).or_default() += 1;
                smodel_keys.insert(key);
            }
            "world" => {
                *world_mats.entry(name).or_default() += 1;
                *world_ps.entry(ps).or_default() += 1;
                world_keys.insert(key);
            }
            _ => {}
        }
    }

    fn absorb(&mut self, next: Self) {
        for (key, n) in next.smodel_mats {
            *self.smodel_mats.entry(key).or_default() += n;
        }
        for (key, n) in next.world_mats {
            *self.world_mats.entry(key).or_default() += n;
        }
        for (key, n) in next.smodel_ps {
            *self.smodel_ps.entry(key).or_default() += n;
        }
        for (key, n) in next.world_ps {
            *self.world_ps.entry(key).or_default() += n;
        }
        self.smodel_keys.extend(next.smodel_keys);
        self.world_keys.extend(next.world_keys);
        self.ports.extend(next.ports);
    }
}

fn execution_binds_code_texture(execution: &MaterialExecution, index: u32) -> bool {
    execution
        .iter_passes()
        .any(|pass| pass.code_samplers.iter().any(|lane| lane.index == index))
}

#[derive(Clone, Copy)]
struct ShadowProductDependencies {
    sun: bool,
    spot: bool,
}

impl ShadowProductDependencies {
    const NONE: Self = Self {
        sun: false,
        spot: false,
    };

    fn merge(&mut self, other: Self) {
        self.sun |= other.sun;
        self.spot |= other.spot;
    }

    fn from_execution(execution: &MaterialExecution) -> Self {
        Self {
            sun: execution_binds_code_texture(execution, CODE_TEXTURE_SHADOWMAP_SUN),
            spot: execution_binds_code_texture(execution, CODE_TEXTURE_SHADOWMAP_SPOT),
        }
    }
}

fn identity_placement_kind(kind: &RetainedDrawKind) -> bool {
    match *kind {
        RetainedDrawKind::World { .. }
        | RetainedDrawKind::CodeMesh { .. }
        | RetainedDrawKind::MarkMesh { .. }
        | RetainedDrawKind::Glass { .. } => true,
        RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => world_from_local == Mat4::IDENTITY,
        _ => false,
    }
}

#[derive(Default)]
struct RunPackCache {
    serial: u64,
    exit_clip: Option<d3d9_sm3::ExitClipMode>,
    passes: Vec<Option<Arc<PassConstantBuffers>>>,

    pipelines: Vec<Option<(usize, ExactPipelineSlot)>>,
    intern: HashMap<PackedBankKey, (Arc<PassConstantBuffers>, u32)>,
    intern_stamp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PackedBankKey {
    port: PortId,
    local: usize,
    code: u64,
}

fn packed_bank_key(
    port: PortId,
    local: Option<&Arc<render_material::PackedLocalBanks>>,
    code: u64,
) -> PackedBankKey {
    PackedBankKey {
        port,
        local: local.map(|banks| Arc::as_ptr(banks) as usize).unwrap_or(0),
        code,
    }
}

fn intern_packed_bank<E>(
    intern: &mut HashMap<PackedBankKey, (Arc<PassConstantBuffers>, u32)>,
    stamp: u32,
    key: PackedBankKey,
    miss: impl FnOnce() -> Result<Arc<PassConstantBuffers>, E>,
) -> Result<(Arc<PassConstantBuffers>, bool), E> {
    if let Some((existing, seen)) = intern.get_mut(&key) {
        *seen = stamp;
        return Ok((Arc::clone(existing), true));
    }
    let packed = miss()?;
    intern.insert(key, (Arc::clone(&packed), stamp));
    Ok((packed, false))
}

impl RunPackCache {
    fn begin(&mut self, serial: u64, pass_len: usize, exit_clip: Option<d3d9_sm3::ExitClipMode>) {
        if self.serial != serial || self.exit_clip != exit_clip {
            self.exit_clip = exit_clip;
            self.serial = serial;
            self.passes.clear();
            self.pipelines.clear();
        }
        if self.passes.len() < pass_len {
            self.passes.resize(pass_len, None);
            self.pipelines.resize(pass_len, None);
        }
    }

    fn begin_pack_intern_frame(&mut self) {
        self.intern_stamp = self.intern_stamp.wrapping_add(1);
    }

    fn sweep_pack_intern(&mut self) {
        self.intern
            .retain(|_, (_, seen)| *seen == self.intern_stamp);
    }

    fn pack(
        &mut self,
        pass_index: usize,
        port: &super::AdmittedExactPort,
        executable: ExecutablePassView<'_>,
        prepare_cost: &mut PrepareCost,
    ) -> Result<Arc<PassConstantBuffers>, ConstantPackRefusal> {
        if let Some(Some(existing)) = self.passes.get(pass_index) {
            prepare_cost.note_intern_hit();
            return Ok(Arc::clone(existing));
        }
        let key = packed_bank_key(
            executable.port,
            executable.local_banks,
            executable.code_constant_id,
        );
        let (packed, hit) = intern_packed_bank(&mut self.intern, self.intern_stamp, key, || {
            port.pack_hit(executable).map(Arc::new)
        })?;
        if hit {
            prepare_cost.note_intern_hit();
        } else {
            prepare_cost.note_intern_miss();
            prepare_cost.note_pack_seed(executable.local_banks.is_some());
        }
        if let Some(slot) = self.passes.get_mut(pass_index) {
            *slot = Some(Arc::clone(&packed));
        }
        Ok(packed)
    }
}

impl ExactPrepare<'_> {
    fn bind_hit_textures(
        &mut self,
        executable: ExecutablePassView<'_>,
        surface: super::SurfaceSamplerInputs,
        after_scene_resolve: bool,
    ) -> Result<Arc<[u32]>, GpuSubmitRefusal> {
        let port_gpu = self
            .pipeline_res
            .get(executable.port)
            .expect("exact port was admitted before texture binding");
        let key = BoundTextureKey {
            identity: TextureBindIdentity {
                port: executable.port,
                local: executable
                    .local_samplers
                    .map(|packed| packed.id)
                    .unwrap_or(0),
                code: executable.code_sampler_id,
                surface,
            },
            spot_shadow_select: self.spot_shadow_select,
        };
        let scene_index = texture_table::scene_table_index(
            after_scene_resolve,
            GfxPassState::from_prepared(executable.state).srgb_write_enable(),
        );
        let extracted = self.extracted;
        let uploaded = self.uploaded;
        let sampler_table = self.sampler_table;
        let spot_shadow_select = self.spot_shadow_select;
        let device = self.device;
        let resolve = |table: &mut ExactTextureTable| -> Result<Arc<[u32]>, GpuSubmitRefusal> {
            let images = TextureImageTables::new(
                &extracted.world.image_handles,
                extracted.world.generation,
                uploaded,
            )
            .map_err(GpuSubmitRefusal::TextureBind)?;
            let textures = images
                .resolve(
                    &port_gpu.port,
                    executable,
                    surface,
                    sampler_table,
                    spot_shadow_select,
                )
                .map_err(GpuSubmitRefusal::TextureBind)?;
            texture_slot_words(device, table, &textures)
        };
        let mut shared_guard;
        let (texture_slots, table) = match &mut self.textures {
            PrepareTextureTables::SceneShared(shared) => {
                shared_guard = shared
                    .lock()
                    .expect("scene texture tables are never poisoned");
                let refs = &mut *shared_guard;
                (&mut refs.slots[scene_index], &mut refs.tables[scene_index])
            }
            PrepareTextureTables::Shadow { slots, table } => (&mut **slots, &mut **table),
        };
        if let Some(slots) = texture_slots.get(&key) {
            self.cost.tex_bind_hit_n = self.cost.tex_bind_hit_n.saturating_add(1);
            return Ok(Arc::clone(slots));
        }
        self.cost.tex_bind_miss_n = self.cost.tex_bind_miss_n.saturating_add(1);
        let slots = resolve(table)?;
        texture_slots.insert(key, Arc::clone(&slots));
        Ok(slots)
    }
}

fn texture_slot_words(
    device: &RenderDevice,
    table: &mut ExactTextureTable,
    textures: &[UploadedTextureBind],
) -> Result<Arc<[u32]>, GpuSubmitRefusal> {
    textures
        .iter()
        .map(|lane| {
            table
                .slot_word(device, lane)
                .map_err(GpuSubmitRefusal::TextureTable)
        })
        .collect::<Result<Vec<u32>, _>>()
        .map(Arc::from)
}

#[derive(Default)]
struct PrepareCost {
    source_build_n: u32,
    source_reuse_n: u32,
    pack_seed_n: u32,
    pack_walk_n: u32,
    tex_bind_hit_n: u32,
    tex_bind_miss_n: u32,
    intern_hit_n: u32,
    intern_miss_n: u32,
}

#[derive(Default)]
struct SceneTextureState {
    slots: ColourBindingLanes,
    tables: [ExactTextureTable; 4],
}

enum PrepareTextureTables<'a> {
    SceneShared(&'a std::sync::Mutex<SceneTextureState>),
    Shadow {
        slots: &'a mut HashMap<BoundTextureKey, Arc<[u32]>>,
        table: &'a mut ExactTextureTable,
    },
}

struct ExactPrepare<'a> {
    portal_view: Option<u8>,
    slice_plane: Option<[f64; 4]>,
    instance: u16,
    slice_groups: HashMap<[u32; 28], Arc<BindGroup>>,
    extracted: ExtractedColourRefs<'a>,
    geometry: &'a ExactColourGeometry,

    pretess: Option<&'a CameraWorldPretess>,
    pipeline_res: &'a ExactColourPipeline,
    registry: &'a ExactPipelineRegistry,
    device: &'a RenderDevice,
    uploaded: &'a RuntimeUploadedImageRegistry,

    spot_shadow_select: Option<u8>,
    sampler_table: &'a SamplerTable,

    textures: PrepareTextureTables<'a>,

    arena: Option<&'a mut ArenaPack>,
    run_pack: RunPackCache,
    cost: PrepareCost,
    skinned_tess: Option<&'a mut smodel_skinned::SmodelSkinnedTess>,
    skinned_shared: Option<&'a std::sync::Mutex<smodel_skinned::SmodelSkinnedTess>>,
}

#[derive(Clone, Copy)]
struct ExactPrepareTarget {
    color: TextureFormat,
    samples: u32,
    depth: TextureFormat,
    forward_z: bool,
    use_world_pretess: bool,
}

impl PrepareCost {
    fn note_pack_seed(&mut self, seeded: bool) {
        if seeded {
            self.pack_seed_n = self.pack_seed_n.saturating_add(1);
        } else {
            self.pack_walk_n = self.pack_walk_n.saturating_add(1);
        }
    }
    fn absorb(&mut self, other: Self) {
        self.source_build_n = self.source_build_n.saturating_add(other.source_build_n);
        self.source_reuse_n = self.source_reuse_n.saturating_add(other.source_reuse_n);
        self.pack_seed_n = self.pack_seed_n.saturating_add(other.pack_seed_n);
        self.pack_walk_n = self.pack_walk_n.saturating_add(other.pack_walk_n);
        self.tex_bind_hit_n = self.tex_bind_hit_n.saturating_add(other.tex_bind_hit_n);
        self.tex_bind_miss_n = self.tex_bind_miss_n.saturating_add(other.tex_bind_miss_n);
        self.intern_hit_n = self.intern_hit_n.saturating_add(other.intern_hit_n);
        self.intern_miss_n = self.intern_miss_n.saturating_add(other.intern_miss_n);
    }

    fn note_intern_hit(&mut self) {
        self.intern_hit_n = self.intern_hit_n.saturating_add(1);
    }
    fn note_intern_miss(&mut self) {
        self.intern_miss_n = self.intern_miss_n.saturating_add(1);
    }
}

#[derive(Default)]
struct WorldRunGather {
    indices: Vec<u32>,
    ranges: Vec<(u32, u32)>,

    index_gaps: u32,
}

fn empty_world_run_gather(ranges: Vec<(u32, u32)>, index_gaps: u32) -> WorldRunGather {
    WorldRunGather {
        indices: Vec::new(),
        ranges,
        index_gaps,
    }
}

fn world_submit_ranges_for_pass<'a>(
    geometry: &'a ExactColourGeometry,
    pretess: Option<&'a CameraWorldPretess>,
    use_world_pretess: bool,
) -> &'a [(u32, u32)] {
    match pretess.filter(|_| use_world_pretess) {
        Some(pretess) => pretess.ranges(),
        None => &geometry.world_surface_ranges,
    }
}

fn gather_world_run_indices(
    hits: impl IntoIterator<Item = (u16, u64, super::SurfaceSamplerInputs)>,
    src_indices: &[u32],
    src_ranges: &[(u32, u32)],
) -> WorldRunGather {
    let mut ranges = vec![(0, 0); src_ranges.len()];
    let mut indices = Vec::new();
    let mut prev: Option<(u64, super::SurfaceSamplerInputs, u32)> = None;
    let mut index_gaps = 0u32;
    for (surf, key, samplers) in hits {
        let Some(&(start, count)) = src_ranges.get(usize::from(surf)) else {
            prev = None;
            continue;
        };
        if count == 0 {
            prev = None;
            continue;
        }
        let dest = indices.len() as u32;
        let Some(slice) = src_indices.get(start as usize..(start.saturating_add(count)) as usize)
        else {
            prev = None;
            continue;
        };
        if let Some((prev_key, prev_samplers, end)) = prev
            && prev_key == key
            && prev_samplers == samplers
            && end != start
        {
            index_gaps = index_gaps.saturating_add(1);
        }
        indices.extend_from_slice(slice);
        if let Some(slot) = ranges.get_mut(usize::from(surf)) {
            *slot = (dest, count);
        }
        prev = Some((key, samplers, start.saturating_add(count)));
    }
    WorldRunGather {
        indices,
        ranges,
        index_gaps,
    }
}

fn colour_tech_at(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    di: usize,
) -> Option<TechType> {
    let colour_n = colour.ordered_draws.len();
    let light_n = light.ordered_draws.len();
    if di < colour_n {
        colour.draw_tech.get(di).copied()
    } else if di < colour_n + light_n {
        light.draw_tech.get(di - colour_n).copied()
    } else {
        emissive.draw_tech.get(di - colour_n - light_n).copied()
    }
}

fn colour_draw_at<'a>(
    colour: &'a FrameProduct,
    light: &'a FrameProduct,
    emissive: &'a FrameProduct,
    di: usize,
) -> Option<&'a RetainedDrawItem> {
    let colour_n = colour.ordered_draws.len();
    let light_n = light.ordered_draws.len();
    if di < colour_n {
        colour.ordered_draws.get(di)
    } else if di < colour_n + light_n {
        light.ordered_draws.get(di - colour_n)
    } else {
        emissive.ordered_draws.get(di - colour_n - light_n)
    }
}

fn packed_draw_index(packed: &render_frame::PackedFrontendLists, emit: PackedEmit) -> Option<u32> {
    let i = emit.entry as usize;
    match emit.list {
        PackedListKind::World => packed.world_draw_indices.get(i).copied(),
        PackedListKind::XModel => packed.xmodel_draw_indices.get(i).copied(),
        PackedListKind::Smodel => packed.smodel_draw_indices.get(i).copied(),
        PackedListKind::Cached => packed.smodel_cached_draw_indices.get(i).copied(),
        PackedListKind::Pretess => packed.smodel_pretess_draw_indices.get(i).copied(),
        PackedListKind::SmodelSkinned => packed.smodel_skinned_draw_indices.get(i).copied(),
    }
}

struct WorldPackedSurf<'a> {
    item: &'a RetainedDrawItem,
    world_surf: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldPackedRowMeta {
    di: u32,
    world_surf: Option<u16>,
}

impl WorldPackedSurf<'_> {
    fn world_surf(&self) -> Option<u16> {
        self.world_surf.or_else(|| match self.item.kind {
            RetainedDrawKind::World { surf, .. } => Some(surf),
            _ => None,
        })
    }
}

fn world_packed_row_meta(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    packed: &render_frame::PackedFrontendLists,
    table: &[u16],
) -> Vec<Option<WorldPackedRowMeta>> {
    let skip_end = colour
        .ordered_draws
        .len()
        .saturating_add(light.ordered_draws.len())
        .saturating_add(emissive.ordered_draws.len());
    let mut prior = vec![0u16; skip_end];
    let mut rows = Vec::with_capacity(packed.world_draw_indices.len());
    for &di in &packed.world_draw_indices {
        let di_us = di as usize;
        let Some(item) = colour_draw_at(colour, light, emissive, di_us) else {
            rows.push(None);
            continue;
        };
        let seen = prior.get(di_us).copied().unwrap_or(0);
        let world_surf = match item.kind {
            RetainedDrawKind::World {
                surf, run, run_off, ..
            } if run > 1 => Some(
                table
                    .get(run_off as usize + usize::from(seen))
                    .copied()
                    .unwrap_or(surf),
            ),
            _ => None,
        };
        if let Some(slot) = prior.get_mut(di_us) {
            *slot = seen.saturating_add(1);
        }
        rows.push(Some(WorldPackedRowMeta { di, world_surf }));
    }
    rows
}

fn bind_world_packed_rows<'a>(
    colour: &'a FrameProduct,
    light: &'a FrameProduct,
    emissive: &'a FrameProduct,
    rows: &'a [Option<WorldPackedRowMeta>],
) -> impl Iterator<Item = Option<WorldPackedSurf<'a>>> + 'a {
    rows.iter().map(move |meta| {
        let meta = meta.as_ref()?;
        colour_draw_at(colour, light, emissive, meta.di as usize).map(|item| WorldPackedSurf {
            item,
            world_surf: meta.world_surf,
        })
    })
}

struct PreparedColourRow<'a> {
    item: &'a RetainedDrawItem,
    world_surf: Option<u16>,
}

impl PreparedColourRow<'_> {
    fn expanded_item(&self) -> Option<RetainedDrawItem> {
        let surf = self.world_surf?;
        let RetainedDrawKind::World {
            surf: first_surf,
            world_from_local,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            brush_model,
            ..
        } = self.item.kind
        else {
            return None;
        };
        let mut item = *self.item;
        item.kind = match bsp_kind {
            Some(kind) => RetainedDrawKind::World {
                surf,
                run: 1,
                run_off: 0,
                bsp_kind: Some(kind),
                bsp_run_first: Some(
                    bsp_run_first.expect("BSP world draw is missing its producer run identity"),
                ),
                setup_key_changed: setup_key_changed && surf == first_surf,
                world_from_local: Mat4::IDENTITY,
                brush_model,
            },
            None => match brush_model {
                Some(model) => RetainedDrawKind::brush_world(surf, model, world_from_local),
                None => RetainedDrawKind::world_with_pose(surf, world_from_local),
            },
        };
        Some(item)
    }
}

fn colour_entry_index_span(
    packed: &render_frame::PackedFrontendLists,
    emit: PackedEmit,
) -> Option<(u32, u32)> {
    let index = emit.entry as usize;
    if emit.list == PackedListKind::XModel {
        return packed
            .xmodel
            .get(index)
            .map(|entry| (entry.index_byte_offset / 2, u32::from(entry.tri_count) * 3));
    }
    let entries = match emit.list {
        PackedListKind::Smodel => &packed.smodel,
        PackedListKind::Cached => &packed.smodel_cached,
        PackedListKind::Pretess => &packed.smodel_pretess,
        PackedListKind::SmodelSkinned | PackedListKind::World | PackedListKind::XModel => {
            return None;
        }
    };
    entries
        .get(index)
        .map(|entry| (entry.index_byte_offset / 2, u32::from(entry.tri_count) * 3))
}

fn intended_colour_tech(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    di: usize,
) -> TechType {
    colour_tech_at(colour, light, emissive, di).unwrap_or(TechType(0))
}

fn build_colour_row_plan(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    packed: &render_frame::PackedFrontendLists,
    work: &super::backend::ColourDrawListWork,
    world_run_surfs: &[u16],
    pretess: &CameraWorldPretess,
) -> Vec<ColourRowPlan> {
    let layout_epoch = pretess.layout_epoch();
    let skip_end = colour
        .ordered_draws
        .len()
        .saturating_add(light.ordered_draws.len())
        .saturating_add(emissive.ordered_draws.len());
    let mut emitted = vec![false; skip_end];
    let mut rows = Vec::with_capacity(work.emit_order.len().max(skip_end));
    for emit in &work.emit_order {
        let Some(di) = packed_draw_index(packed, *emit) else {
            continue;
        };
        let already_emitted = emitted.get(di as usize).copied().unwrap_or(false);
        if emit.list == PackedListKind::World && already_emitted {
            continue;
        }
        if let Some(slot) = emitted.get_mut(di as usize) {
            *slot = true;
        }
        let tech = intended_colour_tech(colour, light, emissive, di as usize);
        if emit.list == PackedListKind::World {
            let Some(item) = colour_draw_at(colour, light, emissive, di as usize) else {
                continue;
            };
            if let Some(index_span) = world_emit_run_dest_span(item, world_run_surfs, pretess) {
                rows.push(ColourRowPlan {
                    draw_index: di,
                    world_surface_override: None,
                    technique: tech,
                    index_span: Some(index_span),
                    layout_epoch,
                });
            } else {
                push_expanded_world_row_plan(
                    item,
                    di,
                    tech,
                    world_run_surfs,
                    layout_epoch,
                    &mut rows,
                );
            }
            continue;
        }
        let span = colour_entry_index_span(packed, *emit);
        if colour_draw_at(colour, light, emissive, di as usize).is_none() {
            continue;
        }
        rows.push(ColourRowPlan {
            draw_index: di,
            world_surface_override: None,
            technique: tech,
            index_span: span,
            layout_epoch,
        });
    }
    for i in 0..skip_end {
        if emitted.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(item) = colour_draw_at(colour, light, emissive, i) else {
            continue;
        };
        let tech = intended_colour_tech(colour, light, emissive, i);
        push_expanded_world_row_plan(
            item,
            i as u32,
            tech,
            world_run_surfs,
            layout_epoch,
            &mut rows,
        );
    }
    rows
}

fn world_emit_run_dest_span(
    item: &RetainedDrawItem,
    table: &[u16],
    pretess: &CameraWorldPretess,
) -> Option<(u32, u32)> {
    let RetainedDrawKind::World {
        surf, run, run_off, ..
    } = item.kind
    else {
        return None;
    };
    let run = run.max(1);
    let mut span: Option<(u32, u32)> = None;
    for offset in 0..run {
        let surf = if run == 1 {
            surf
        } else {
            *table.get(run_off as usize + usize::from(offset))?
        };
        let &(start, count) = pretess.ranges().get(usize::from(surf))?;
        if count == 0 {
            return None;
        }
        span = Some(match span {
            None => (start, count),
            Some((span_start, span_count)) => {
                if span_start.saturating_add(span_count) != start {
                    return None;
                }
                (span_start, span_count.saturating_add(count))
            }
        });
    }
    span
}

fn push_expanded_world_row_plan(
    item: &RetainedDrawItem,
    draw_index: u32,
    tech: TechType,
    table: &[u16],
    layout_epoch: u64,
    out: &mut Vec<ColourRowPlan>,
) {
    if let RetainedDrawKind::World { run, run_off, .. } = item.kind
        && run > 1
        && let Some(surfs) = table.get(run_off as usize..run_off as usize + usize::from(run))
        && surfs.len() == usize::from(run)
    {
        for &s in surfs {
            out.push(ColourRowPlan {
                draw_index,
                world_surface_override: Some(s),
                technique: tech,
                index_span: None,
                layout_epoch,
            });
        }
        return;
    }
    out.push(ColourRowPlan {
        draw_index,
        world_surface_override: None,
        technique: tech,
        index_span: None,
        layout_epoch,
    });
}

fn world_material_sorted(packed: u64) -> u16 {
    dpvs_iw4::GfxDrawSurf { packed }.material_sorted_index()
}

struct WorldPretessLayout {
    key: WorldPretessKey,
    index: Option<Buffer>,
    ranges: Vec<(u32, u32)>,
    index_gaps: u32,
    logical_index_count: u32,
    epoch: u64,
}

#[derive(Resource, Default)]
struct CameraWorldPretess {
    layout: Option<WorldPretessLayout>,

    epoch: u64,
}

impl CameraWorldPretess {
    fn ranges(&self) -> &[(u32, u32)] {
        self.layout
            .as_ref()
            .map(|layout| layout.ranges.as_slice())
            .unwrap_or(&[])
    }

    fn index(&self) -> Option<&Buffer> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.index.as_ref())
    }

    fn logical_index_count(&self) -> u32 {
        self.layout
            .as_ref()
            .map(|layout| layout.logical_index_count)
            .unwrap_or(0)
    }

    fn layout_epoch(&self) -> u64 {
        self.layout.as_ref().map(|layout| layout.epoch).unwrap_or(0)
    }
}

fn exact_draw_run_continues(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    prev.after_scene_resolve == next.after_scene_resolve
        && prev.tess == next.tess
        && prev.pipeline == next.pipeline
        && prev.port == next.port
        && same_packed_constants(prev, next)
        && prev.portal_tag == next.portal_tag
        && prev.slice_clip.as_ref().map(|g| g.id()) == next.slice_clip.as_ref().map(|g| g.id())
        && same_texture_slots(prev, next)
        && prev.depth_min == next.depth_min
        && prev.depth_max == next.depth_max
        && prev.smc_stream_off == next.smc_stream_off
        && bsp_prepared_runs_continue(prev, next)
        && prev.start.saturating_add(prev.count) == next.start
}

fn bsp_prepared_runs_continue(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    bsp_run_identity_continues(
        prev.bsp_kind,
        prev.bsp_run_first,
        prev.bsp_first_surf,
        prev.bsp_surf_count,
        next.bsp_kind,
        next.bsp_run_first,
        next.bsp_first_surf,
    )
}

fn bsp_run_identity_continues(
    previous_kind: Option<BspCameraLane>,
    previous_run_first: u16,
    previous_first: u16,
    previous_count: u16,
    current_kind: Option<BspCameraLane>,
    current_run_first: u16,
    current_first: u16,
) -> bool {
    match (previous_kind, current_kind) {
        (None, None) => true,
        (Some(previous), Some(current)) => {
            previous == current
                && previous_run_first == current_run_first
                && previous_first.checked_add(previous_count) == Some(current_first)
        }
        _ => false,
    }
}

fn same_texture_slots(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    Arc::ptr_eq(&prev.texture_slots, &next.texture_slots)
        || prev.texture_slots == next.texture_slots
}

fn same_packed_constants(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    match (&prev.constants, &next.constants) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
        (None, None) => {
            prev.constant_base.is_some()
                && prev.constant_base == next.constant_base
                && prev.arena_lane == next.arena_lane
        }
        _ => false,
    }
}

fn coalesce_indexed_runs<T>(
    draws: &mut [T],
    can_extend: impl Fn(&T, &T) -> bool,
    extend: impl Fn(&mut T, &T),
) -> usize {
    let mut write = 0usize;
    let mut read = 0usize;
    while read < draws.len() {
        if write > 0 && can_extend(&draws[write - 1], &draws[read]) {
            let (head, tail) = draws.split_at_mut(read);
            extend(&mut head[write - 1], &tail[0]);
        } else {
            if write != read {
                draws.swap(write, read);
            }
            write += 1;
        }
        read += 1;
    }
    write
}

fn tess_flush_merge_slot<T>(
    out: &[T],
    draw: &T,
    continues: impl Fn(&T, &T) -> bool,
    look_past_sibling_pass: impl Fn(&T, &T) -> bool,
) -> Option<usize> {
    let mut i = out.len();
    while i > 0 {
        i -= 1;
        if continues(&out[i], draw) {
            return Some(i);
        }
        if look_past_sibling_pass(&out[i], draw) {
            continue;
        }
        break;
    }
    None
}

fn colour_look_past_sibling_pass(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    prev.after_scene_resolve == next.after_scene_resolve
        && prev.tess == next.tess
        && prev.pipeline != next.pipeline
        && (prev.start.saturating_add(prev.count) == next.start
            || (prev.start == next.start && prev.count == next.count))
}

fn coalesce_exact_draws(draws: &mut Vec<PreparedExactDraw>) {
    let mut len = 0usize;
    let mut read = 0usize;
    while read < draws.len() {
        let slot = {
            let (head, tail) = draws.split_at_mut(read);
            tess_flush_merge_slot(
                &head[..len],
                &tail[0],
                exact_draw_run_continues,
                colour_look_past_sibling_pass,
            )
        };
        match slot {
            Some(i) => {
                let count = draws[read].count;
                let bsp_surf_count = draws[read].bsp_surf_count;
                let bsp_counted = draws[read].bsp_counted;
                draws[i].count = draws[i].count.saturating_add(count);
                draws[i].bsp_surf_count = draws[i].bsp_surf_count.saturating_add(bsp_surf_count);
                draws[i].bsp_counted |= bsp_counted;
                let dependencies = draws[read].shadow_dependencies;
                draws[i].shadow_dependencies.merge(dependencies);
            }
            None => {
                if len != read {
                    draws.swap(len, read);
                }
                len += 1;
            }
        }
        read += 1;
    }
    draws.truncate(len);
}

fn append_with_rollback<T, E>(
    out: &mut Vec<T>,
    write: impl FnOnce(&mut Vec<T>) -> Result<(), E>,
) -> Result<usize, E> {
    let start = out.len();
    match write(out) {
        Ok(()) => Ok(out.len().saturating_sub(start)),
        Err(cause) => {
            out.truncate(start);
            Err(cause)
        }
    }
}

impl ExactPrepare<'_> {
    fn prepare_draw_source(
        &mut self,
        kind: RetainedDrawKind,
        input_vertex_type: u8,
        target: ExactPrepareTarget,
    ) -> Result<PreparedDrawSource, GpuSubmitRefusal> {
        let (tess, start, count, vertex_type) = match kind {
            RetainedDrawKind::World { surf, .. } => {
                if let Some(cause) = self.extracted.world.static_geometry.world_vertex_refusal {
                    return Err(GpuSubmitRefusal::WorldVertex(cause));
                }
                let &(start, count) = world_submit_ranges_for_pass(
                    self.geometry,
                    self.pretess,
                    target.use_world_pretess,
                )
                .get(usize::from(surf))
                .ok_or(GpuSubmitRefusal::MissingSurfaceRange { surf })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyIndexRange { surf });
                }
                if !self.geometry.world_ready() {
                    return Err(GpuSubmitRefusal::WorldVertex(
                        render_frame::WorldVertexRefusal::ForeignLayout {
                            source_layout: "exact colour world buffers absent",
                        },
                    ));
                }
                (ExactTessBind::World, start, count, input_vertex_type)
            }
            RetainedDrawKind::Smodel {
                surface,
                placement,
                stream,
                cache_index,
                pretess,
                world_from_local,
                ..
            } => match stream {
                None => return Err(GpuSubmitRefusal::SmodelXSurfacePathUnread { placement }),
                Some(lighting_iw4::SmodelSurfPath::Cached) => {
                    if let Some(range) = pretess {
                        smodel_pretess_submit_refusal(
                            self.extracted,
                            placement,
                            cache_index
                                .ok_or(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement })?,
                            range,
                        )?
                    } else {
                        return Err(GpuSubmitRefusal::SmodelCachedWithoutDestRange { placement });
                    }
                }
                Some(lighting_iw4::SmodelSurfPath::Pretess) => {
                    let cache_index =
                        cache_index.ok_or(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement })?;
                    let pretess = pretess
                        .ok_or(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index })?;
                    smodel_pretess_submit_refusal(self.extracted, placement, cache_index, pretess)?
                }

                Some(lighting_iw4::SmodelSurfPath::Rigid) => {
                    if let Some(cause) = self.extracted.world.static_geometry.smodel_vertex_refusal
                    {
                        return Err(GpuSubmitRefusal::PackedVertex(cause));
                    }
                    let &(start, count) = self
                        .geometry
                        .smodel_surface_ranges
                        .get(surface as usize)
                        .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
                    if count == 0 {
                        return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
                    }
                    if !self.geometry.smodel_ready() {
                        return Err(GpuSubmitRefusal::PackedVertex(
                            render_frame::PackedVertexRefusal::ForeignLayout {
                                source_layout: "exact colour smodel buffers absent",
                            },
                        ));
                    }
                    (
                        ExactTessBind::Smodel,
                        start,
                        count,
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                    )
                }
                Some(lighting_iw4::SmodelSurfPath::Skinned) => {
                    let (start, count) = match (self.skinned_tess.as_mut(), self.skinned_shared) {
                        (Some(tess), _) => {
                            tess.append_draw(self.extracted, placement, surface, world_from_local)?
                        }
                        (None, Some(shared)) => shared
                            .lock()
                            .expect("skinned static model cache is never poisoned")
                            .append_draw(self.extracted, placement, surface, world_from_local)?,
                        (None, None) => {
                            return Err(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement });
                        }
                    };
                    if count == 0 {
                        return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
                    }
                    (
                        ExactTessBind::SmodelSkinned,
                        start,
                        count,
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                    )
                }
            },
            RetainedDrawKind::XModel { surface, .. } => {
                if let Some(cause) = self.extracted.frame.xmodel_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .xmodel_surface_ranges
                    .get(surface as usize)
                    .ok_or(GpuSubmitRefusal::MissingXModelRange { surface })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyXModelIndexRange { surface });
                }
                if !self.geometry.xmodel_ready() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour xmodel buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::XModel,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::CodeMesh { draw, .. } => {
                if let Some(cause) = self.extracted.frame.fx_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .fx_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingCodeMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyCodeMeshIndexRange { draw });
                }
                if !self.geometry.fx_ready() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour code-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::CodeMesh,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::ParticleCloud { draw, .. } => {
                let &(start, count) = self
                    .geometry
                    .particle_cloud_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingParticleCloudRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyParticleCloudIndexRange { draw });
                }
                if !self.geometry.particle_cloud_ready() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour particle-cloud buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::ParticleCloud,
                    start,
                    count,
                    asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::MarkMesh { draw, packed, .. } => {
                let &(start, count) = self
                    .geometry
                    .mark_mesh_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingMarkMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyMarkMeshIndexRange { draw });
                }
                if !self.geometry.mark_mesh_ready() {
                    return Err(GpuSubmitRefusal::WorldVertex(
                        render_frame::WorldVertexRefusal::ForeignLayout {
                            source_layout: "exact colour mark-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::MarkMesh,
                    start,
                    count,
                    if packed {
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE
                    } else {
                        asset_iw4::vertex_decl::WORLD_VERTEX_TYPE
                    },
                )
            }
            RetainedDrawKind::Glass { draw, .. } => {
                if let Some(cause) = self.extracted.frame.glass_mesh_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .glass_mesh_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingGlassMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyGlassMeshIndexRange { draw });
                }
                if !self.geometry.glass_mesh_ready() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour glass-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::Glass,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
        };

        let smc_stream_off = if tess == ExactTessBind::SmodelCached {
            if let RetainedDrawKind::Smodel {
                cache_index: Some(cache_index),
                ..
            } = kind
            {
                u64::from(
                    lighting_iw4::smc_stream_source_byte_offset(cache_index)
                        .ok_or(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index })?,
                )
            } else {
                0
            }
        } else {
            0
        };

        let (bsp_kind, bsp_run_first, bsp_first_surf, bsp_surf_count) = bsp_draw_source(&kind);
        Ok(PreparedDrawSource {
            input_vertex_type,
            world_pretess: target.use_world_pretess,
            tess,
            start,
            count,
            vertex_type,
            smc_stream_off,
            bsp_kind,
            bsp_run_first,
            bsp_first_surf,
            bsp_surf_count,
        })
    }

    fn prepare_ready_hit(
        &mut self,
        item: &RetainedDrawItem,
        executor: &MaterialRunExecutor,
        target: ExactPrepareTarget,
        source_cache: &mut Option<PreparedDrawSource>,
        dependencies: ShadowProductDependencies,
        out: &mut Vec<PreparedExactDraw>,
    ) -> Result<usize, GpuSubmitRefusal> {
        let execution = executor.execution();
        let run_serial = executor.run_serial();
        let place_code = executor.place();
        let surface = item.surface_samplers;
        let after_scene_resolve = matches!(self.textures, PrepareTextureTables::SceneShared(_))
            && (item.camera_region == Some(asset_iw4::CAMERA_REGION_EMISSIVE)
                || matches!(item.kind, RetainedDrawKind::CodeMesh { .. })
                || matches!(item.kind, RetainedDrawKind::Glass { .. })
                || matches!(item.kind, RetainedDrawKind::MarkMesh { glass: true, .. })
                || execution_binds_code_texture(execution, CODE_TEXTURE_RESOLVED_POST_SUN)
                || execution_binds_code_texture(execution, CODE_TEXTURE_FLOATZ));
        let kind = item.kind;
        let key = item.key;
        let source = if let Some(source) = source_cache.as_ref().filter(|source| {
            source.input_vertex_type == execution.vertex_type
                && source.world_pretess == target.use_world_pretess
        }) {
            self.cost.source_reuse_n = self.cost.source_reuse_n.saturating_add(1);
            *source
        } else {
            let source = self.prepare_draw_source(kind, execution.vertex_type, target)?;
            self.cost.source_build_n = self.cost.source_build_n.saturating_add(1);
            *source_cache = Some(source);
            source
        };
        let PreparedDrawSource {
            tess,
            start,
            count,
            vertex_type,
            smc_stream_off,
            bsp_kind,
            bsp_run_first,
            bsp_first_surf,
            bsp_surf_count,
            ..
        } = source;

        let slice_clip = if !target.forward_z
            && let Some(plane) = self.slice_plane
        {
            let planes = self
                .portal_view
                .and_then(|tag| {
                    portal_scene::entity_slice_planes(
                        &self.extracted.frame.exec_frame,
                        tag,
                        plane,
                        match kind {
                            RetainedDrawKind::XModel {
                                portal_aperture, ..
                            } => portal_aperture,
                            _ => None,
                        },
                    )
                })
                .ok_or(GpuSubmitRefusal::PortalSliceCameraMissing)?;
            let key = std::array::from_fn(|i| planes[i / 4][i % 4].to_bits());
            Some(Arc::clone(self.slice_groups.entry(key).or_insert_with(
                || {
                    let buffer = self.device.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("portal entity slice"),
                        contents: bytemuck::cast_slice(&planes),
                        usage: BufferUsages::UNIFORM,
                    });
                    Arc::new(self.device.create_bind_group(
                        Some("portal entity slice"),
                        &portal_scene::exit_plane_layout(self.device),
                        &[BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        }],
                    ))
                },
            )))
        } else {
            None
        };
        let exit_clip = if target.forward_z {
            self.slice_plane
                .map(|_| portal_scene::exit_clip_mode(self.device))
        } else if slice_clip.is_some() {
            Some(portal_scene::exit_clip_mode(self.device))
        } else {
            self.portal_view
                .and_then(|tag| tag.checked_sub(1).map(|view| (tag, view)))
                .and_then(|(tag, view)| {
                    self.extracted
                        .frame
                        .exec_frame
                        .world_cameras
                        .get(view as usize)
                        .and_then(Option::as_ref)
                        .and_then(|c| c.exit_plane_clip)
                        .map(|_| tag)
                })
                .and_then(|tag| {
                    let mode = portal_scene::exit_clip_mode(self.device);
                    if mode == d3d9_sm3::ExitClipMode::FragmentDiscard
                        && portal_scene::exit_class(self.extracted, tag, &kind)
                            == portal_scene::ExitClass::Inside
                    {
                        None
                    } else {
                        Some(mode)
                    }
                })
        };
        self.run_pack
            .begin(run_serial, execution.pass_count(), exit_clip);
        append_with_rollback(out, |out| {
            for (pass_index, executable) in execution.iter_passes().enumerate() {
                debug_assert_eq!(
                    executable.port.vertex_type, vertex_type,
                    "ExecutablePass.port was admitted for a different tess vertex type"
                );
                let pipeline_res = self.pipeline_res;
                let (port_index, pipeline) = match self.run_pack.pipelines.get(pass_index).copied()
                {
                    Some(Some(hit)) => hit,
                    _ => {
                        let port_index = pipeline_res.by_id.get(&executable.port).copied().ok_or(
                            GpuSubmitRefusal::NoExactPort {
                                pair: executable.shader_pair,
                            },
                        )?;
                        let host_state = GfxPassState::from_prepared(executable.state);
                        let state0 = host_state.apply_change_state_0_host(false);
                        let state1 = host_state.apply_change_state_1_host();
                        let color = exact_colour_target_format(target.color, state0.srgb_write);
                        let key = ExactColourPipelineKey {
                            target: color,
                            depth_format: target.depth,
                            samples: target.samples,
                            state0,
                            state1,
                            port: executable.port,
                            cached_lighting: false,
                            mark_mesh: tess == ExactTessBind::MarkMesh
                                && vertex_type == asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                            forward_z: target.forward_z,
                            portal_stencil: !target.forward_z && self.portal_view.unwrap_or(0) != 0,
                            exit_clip,
                        };

                        let Some(pipeline) = self.registry.slot(&key) else {
                            self.registry.discover(key);
                            return Err(GpuSubmitRefusal::PipelineNotReady);
                        };
                        if !self.registry.is_ready(pipeline) {
                            return Err(GpuSubmitRefusal::PipelineNotReady);
                        }
                        if let Some(slot) = self.run_pack.pipelines.get_mut(pass_index) {
                            *slot = Some((port_index, pipeline));
                        }
                        (port_index, pipeline)
                    }
                };
                let port_gpu = &pipeline_res.ports[port_index];
                let texture_slots =
                    self.bind_hit_textures(executable, surface, after_scene_resolve)?;
                // Shadow placement is applied after preparation, independently for each
                // light and object. Its unplaced banks belong to the material run.
                let unplaced_shadow = matches!(self.textures, PrepareTextureTables::Shadow { .. })
                    && place_code.is_none();
                let (constants, constant_base) = if identity_placement_kind(&kind)
                    || unplaced_shadow
                {
                    let constants = self
                        .run_pack
                        .pack(pass_index, &port_gpu.port, executable, &mut self.cost)
                        .map_err(GpuSubmitRefusal::ConstantPack)?;
                    match self.arena.as_deref_mut() {
                        Some(arena) => (None, Some(arena.base_for(&constants, &texture_slots))),
                        None => (Some(constants), None),
                    }
                } else if let Some(arena) = self.arena.as_deref_mut() {
                    let overlay = place_code.and_then(|c| c.pass(pass_index)).unwrap_or(&[]);
                    let (base, interned) = arena
                        .append_hit(
                            &port_gpu.port,
                            executable,
                            &texture_slots,
                            PlacedConstants {
                                overlay,
                                drawsurf_key: key,
                                execution_pass_index: pass_index as u32,
                                placement_domain: (u32::from(self.portal_view.unwrap_or(0)) << 16)
                                    | u32::from(self.instance),
                            },
                        )
                        .map_err(GpuSubmitRefusal::ConstantPack)?;
                    if interned {
                        self.cost.note_intern_hit();
                    } else {
                        self.cost.note_intern_miss();
                        self.cost.note_pack_seed(true);
                    }
                    (None, Some(base))
                } else {
                    let mut packed = port_gpu
                        .port
                        .pack_hit(executable)
                        .map_err(GpuSubmitRefusal::ConstantPack)?;
                    if let Some(overlay) = place_code.and_then(|c| c.pass(pass_index)) {
                        overlay_packed_code_on_banks(
                            &mut packed.vertex,
                            &mut packed.pixel,
                            overlay,
                        )
                        .map_err(GpuSubmitRefusal::ConstantPack)?;
                    }
                    self.cost.note_pack_seed(executable.local_banks.is_some());
                    (Some(Arc::new(packed)), None)
                };
                let portal_tag = self
                    .portal_view
                    .unwrap_or_else(|| portal_draw_tag(&self.extracted.frame.exec_frame, &kind));
                let (depth_min, depth_max) = if target.forward_z {
                    (0.0, 1.0)
                } else {
                    reverse_z_viewport_depth(depth_range_type_for_draw(&kind, key))
                };
                out.push(PreparedExactDraw {
                    slice_clip: slice_clip.clone(),
                    slice_world: self.slice_plane,
                    aperture_world: match kind {
                        RetainedDrawKind::XModel {
                            portal_aperture, ..
                        } => portal_aperture,
                        _ => None,
                    },
                    after_scene_resolve,
                    pipeline,
                    port: executable.port,
                    constants,
                    constant_base,
                    texture_slots,
                    start,
                    count,
                    tess,
                    owner_object_id: matches!(kind, RetainedDrawKind::XModel { .. })
                        .then(|| drawsurf_object_id(key)),
                    depth_min,
                    depth_max,
                    state: GfxPassState::from_prepared(executable.state),
                    ring_epoch: 0,
                    smc_stream_off,
                    bsp_kind,
                    bsp_run_first,
                    bsp_first_surf,
                    bsp_surf_count,
                    bsp_counted: bsp_kind.is_some() && pass_index == 0,
                    shadow_dependencies: dependencies,

                    indirect_arg: None,
                    arena_lane: 0,
                    portal_tag,
                });
            }
            Ok(())
        })
    }
}

fn texture_table_layout(pipeline: &ExactColourPipeline) -> Option<&BindGroupLayoutDescriptor> {
    pipeline.ports.first().map(|port| &port.textures_layout)
}

fn prepare_constant_bases<'a>(
    draws: &mut [PreparedExactDraw],
    packed: &mut ArenaPack,
    pipeline: &'a ExactColourPipeline,
) -> Option<&'a BindGroupLayoutDescriptor> {
    for draw in draws.iter_mut() {
        if draw.constant_base.is_none()
            && let Some(ref constants) = draw.constants
        {
            draw.constant_base = Some(packed.base_for(constants, &draw.texture_slots));
        }
    }
    let reserved_rows = packed.reserve_identity();
    for draw in draws.iter_mut() {
        if let Some(base) = draw.constant_base.as_mut() {
            *base = resolve_arena_base(*base, reserved_rows);
        }
    }
    // Upload may replace the backing buffer and invalidate an existing bind
    // group. Supply its layout even when that group is valid before upload.
    draws
        .iter()
        .find_map(|draw| pipeline.get(draw.port).map(|port| &port.constants_layout))
}

pub(super) fn register(app: &mut App) {
    super::floatz::register(app);
    let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExactColourGeometry>()
        .init_resource::<skinning::SkinGpu>()
        .init_resource::<super::resolved_scene::ResolvedScene>()
        .init_resource::<SmodelCacheGpu>()
        .init_resource::<ExactColourPipeline>()
        .init_resource::<ExactColourBindingCache>()
        .init_resource::<ExactShadowBindingCache>()
        .init_resource::<ExactTextureTable>()
        .init_resource::<ShadowTextureTable>()
        .init_resource::<SceneTextureTables>()
        .init_resource::<ExactPipelineKickCache>()
        .init_resource::<ExactPipelineRegistry>()
        .init_resource::<ExactConstantArena>()
        .init_resource::<ExactColourSubmitCensus>()
        .init_resource::<ShadowmapSunGpu>()
        .init_resource::<ShadowmapSpotGpu>()
        .init_resource::<ShadowmapSunArena>()
        .init_resource::<ShadowmapSpotArena>()
        .init_resource::<ColourSubmitScratch>()
        .init_resource::<ShadowSubmitScratch>()
        .init_resource::<CameraPrepareState>()
        .init_resource::<CameraWorldPretess>()
        .init_resource::<prepare_camera::InstalledColourPass>()
        .init_resource::<indirect::ExactIndirectDraws>()
        .init_resource::<ResidentShadowStaticDraws>()
        .add_systems(
            Render,
            (
                pipeline::init_or_update_pipeline.in_set(RenderSystems::PrepareAssets),
                upload_exact_geometry.in_set(RenderSystems::PrepareResources),
                skinning::retire_submitted_buffers
                    .after(RenderSystems::Render)
                    .before(RenderSystems::Cleanup),
                pipeline::kick_extracted_colour_pipelines.in_set(RenderSystems::Prepare),
                prepare_camera::install_shared_colour_pass
                    .in_set(RenderSystems::Prepare)
                    .after(upload_exact_geometry)
                    .after(super::scene_depth::prepare_scene_depth)
                    .after(pipeline::kick_extracted_colour_pipelines)
                    .after(super::gpu_resources::prepare_uploaded_image_registry),
                prepare_camera::prepare_colour_lanes
                    .in_set(RenderSystems::Prepare)
                    .after(prepare_camera::install_shared_colour_pass),
            ),
        )
        .add_systems(
            Core3d,
            (
                record::draw_exact_colour
                    .in_set(Core3dSystems::MainPass)
                    .in_set(super::draw::ExactColourDrawSet)
                    .after(main_opaque_pass_3d),
                diagnostics::copy_submit_prepare_ms.after(super::draw::ExactColourDrawSet),
            ),
        );
}

/// The atlas world tag of a draw (`render_frame::PortalView::surface_tag`):
/// its world plus one, or 0 for a draw of no world.
pub(super) fn portal_draw_tag(
    frame: &render_frame::MaterialExecFrame,
    kind: &RetainedDrawKind,
) -> u8 {
    frame.portal.as_ref().map_or(0, |view| match *kind {
        RetainedDrawKind::World { surf, .. } => view.surface_tag(u32::from(surf)),
        RetainedDrawKind::Smodel {
            owner_world_from_local,
            ..
        } => view.point_tag(owner_world_from_local.w_axis.truncate().to_array()),
        RetainedDrawKind::XModel {
            world_from_local, ..
        } => view.point_tag(world_from_local.w_axis.truncate().to_array()),
        _ => 0,
    })
}

/// Distinct unfolded views may draw the same physical world more than once.
pub(super) fn portal_draw_tags(
    frame: &render_frame::MaterialExecFrame,
    kind: &RetainedDrawKind,
    run_surfaces: &[u16],
    key: u64,
) -> Vec<u8> {
    if is_viewmodel_colour_draw(kind, key) {
        return vec![0];
    }
    let tag = portal_draw_tag(frame, kind);
    match (frame.portal.as_ref(), tag.checked_sub(1)) {
        (Some(view), Some(world)) => view
            .draw_views(usize::from(world))
            .into_iter()
            .filter(|&tag| {
                view.visibility
                    .get(usize::from(tag - 1))
                    .and_then(Option::as_ref)
                    .is_some_and(|visible| visible.admits(kind, run_surfaces))
            })
            .collect(),
        _ => vec![0],
    }
}

pub use portal_scene::exit_clip_mode;
