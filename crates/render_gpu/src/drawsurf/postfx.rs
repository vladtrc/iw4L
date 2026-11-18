use super::AdmittedExactPort;
use super::gpu_prepare::{ConstantPackRefusal, split_bind_layout};
use super::gpu_resources::{
    RuntimeUploadedImageRegistry, TextureBindRefusal, TextureImageTables, UploadedTextureBind,
};
use super::postfx_dof::{
    self, DofFrame, DofPlanRefusal, GaussianPass, POSTFX_MATERIALS, POSTFX_VERTEX_TYPE,
    film_sources,
};
use super::sm3_wgsl::{PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY};
use super::texture_table::SamplerKey;
use crate::diag::render_frame_diag::GPU_SPAN_POSTFX;
use bevy::core_pipeline::{Core3d, Core3dSystems, tonemapping::tonemapping};
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};
use render_frame::water_sheeting::{WATER_SHEETING_MATERIAL, WaterSheetingFrame};
use render_material::{
    ExecutablePassView, MaterialGenerationId, MaterialRefusal, PortId, RuntimeCodeSources,
    SamplerSource, SamplerTextureDimension, StableMaterialShell, rebind_stable_material,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ExtractedFilm {
    pub name: &'static str,
    pub generation: MaterialGenerationId,
    pub catalog: std::sync::Arc<render_material::RuntimeMaterialCatalog>,
    pub port: AdmittedExactPort,
    pub shader: Handle<bevy::shader::Shader>,
    pub shell: StableMaterialShell,
}

#[derive(Clone, Debug)]
pub struct ExtractedBlood {
    pub film: ExtractedFilm,
    pub texture_slots: Vec<u8>,
}

#[derive(Resource, Default)]
pub struct ExtractedPostFx {
    pub films: Vec<ExtractedFilm>,
    pub blood: Option<ExtractedBlood>,
    pub sampler: Option<super::DecodedSampler>,
    pub depth_sampler: Option<super::DecodedSampler>,
    pub frame: DofFrame,
    pub water: Option<WaterSheetingFrame>,
    pub vision: Option<asset_world::FilmVision>,
    pub t6_vision: Option<asset_world::T6Vision>,
}

struct PreparedPostFxGpu {
    film: ExtractedFilm,
    pipeline: CachedRenderPipelineId,
    constants: BindGroup,
    textures_layout: BindGroupLayoutDescriptor,
    sampler: bevy::render::render_resource::Sampler,
    vertex: Buffer,
    index: Buffer,
    constants_arena: Buffer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Image {
    Scene,
    Depth,
    Downsample,
    Blurred,
    Coc,
    Small,
    Ping,
    Output,
    Graded,
    ScreenBlur,
    ScreenPing,
    PostProcessed,
}
struct FilterStep {
    name: &'static str,
    target: Image,
    images: Vec<(u32, Image)>,
    gaussian: Option<GaussianPass>,
    composite: bool,
}
fn filter_plan(
    frame: DofFrame,
    size: UVec2,
    quarter: UVec2,
    glow_ready: bool,
    water: bool,
) -> Vec<FilterStep> {
    let height = size.y;
    let step = |name, target, images| FilterStep {
        name,
        target,
        images,
        gaussian: None,
        composite: false,
    };
    let mut steps = if !frame.dof.active() {
        vec![step(
            "postfx_color2",
            Image::Output,
            vec![(10, Image::Scene)],
        )]
    } else {
        let mut steps = vec![step(
            "dof_downsample",
            Image::Downsample,
            vec![(10, Image::Scene), (15, Image::Depth)],
        )];
        let gaussian = postfx_dof::gaussian_chain(
            height as f32 * (frame.dof.near_blur * 0.25) / 480.0,
            quarter.x,
            quarter.y,
        );
        let mut source = Image::Downsample;
        let count = gaussian.len();
        for (i, pass) in gaussian.into_iter().enumerate() {
            let target = if (count - i) % 2 == 1 {
                Image::Blurred
            } else {
                Image::Ping
            };
            steps.push(FilterStep {
                name: POSTFX_MATERIALS[4 + pass.half_taps],
                target,
                images: vec![(8, source)],
                gaussian: Some(pass),
                composite: false,
            });
            source = target;
        }

        steps.push(step(
            "dof_near_coc",
            Image::Coc,
            vec![(11, source), (12, Image::Downsample)],
        ));
        steps.push(step("small_blur", Image::Small, vec![(8, Image::Coc)]));
        steps.push(step(
            "postfx_dof_color2",
            Image::Output,
            vec![
                (10, Image::Scene),
                (12, Image::Small),
                (11, source),
                (15, Image::Depth),
            ],
        ));
        steps
    };
    if glow_ready {
        append_glow(&mut steps, height, quarter, frame.glow.radius);
    }
    let mut blur_steps = Vec::new();
    let mut source = Image::Scene;
    for (i, pass) in postfx_dof::gaussian_chain(frame.blur * height as f32 / 480.0, size.x, size.y)
        .into_iter()
        .enumerate()
    {
        let target = if i % 2 == 0 {
            Image::ScreenBlur
        } else {
            Image::ScreenPing
        };
        blur_steps.push(FilterStep {
            name: POSTFX_MATERIALS[4 + pass.half_taps],
            target,
            images: vec![(8, source)],
            gaussian: Some(pass),
            composite: false,
        });
        source = target;
    }
    if source != Image::Scene {
        for step in &mut steps {
            for (_, image) in &mut step.images {
                if *image == Image::Scene {
                    *image = source;
                }
            }
        }
    }
    blur_steps.append(&mut steps);
    steps = blur_steps;
    if water {
        for step in &mut steps {
            if step.target == Image::Output {
                step.target = Image::PostProcessed;
            }
        }
        steps.push(step(
            WATER_SHEETING_MATERIAL,
            Image::Output,
            vec![(8, Image::PostProcessed)],
        ));
    }
    steps
}

fn append_glow(steps: &mut Vec<FilterStep>, height: u32, quarter: UVec2, radius: f32) {
    steps.push(FilterStep {
        name: postfx_dof::GLOW_SETUP_MATERIAL,
        target: Image::Downsample,
        images: vec![(10, Image::Scene)],
        gaussian: None,
        composite: false,
    });
    let gaussian = postfx_dof::gaussian_chain(
        height as f32 * (radius * 0.25) / 480.0,
        quarter.x,
        quarter.y,
    );
    let mut source = Image::Downsample;
    let count = gaussian.len();
    for (i, pass) in gaussian.into_iter().enumerate() {
        let target = if (count - i) % 2 == 1 {
            Image::Blurred
        } else {
            Image::Ping
        };
        steps.push(FilterStep {
            name: POSTFX_MATERIALS[4 + pass.half_taps],
            target,
            images: vec![(8, source)],
            gaussian: Some(pass),
            composite: false,
        });
        source = target;
    }
    steps.push(FilterStep {
        name: postfx_dof::GLOW_APPLY_MATERIAL,
        target: Image::Output,
        images: vec![(8, source)],
        gaussian: None,
        composite: true,
    });
}

struct Intermediate {
    _texture: Texture,
    view: TextureView,
    srgb_view: TextureView,
}
impl Intermediate {
    fn new(device: &RenderDevice, size: UVec2) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("iw4_dof_intermediate"),
            size: Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::Rgba8UnormSrgb],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let srgb_view = texture.create_view(&TextureViewDescriptor {
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..default()
        });
        Intermediate {
            _texture: texture,
            view,
            srgb_view,
        }
    }
}
struct PostFxTargets {
    size: UVec2,
    quarter: UVec2,
    images: Vec<Intermediate>,
}
impl PostFxTargets {
    fn new(device: &RenderDevice, size: UVec2) -> Self {
        let quarter = UVec2::new((size.x / 4).max(1), (size.y / 4).max(1));
        let images = (0..8)
            .map(|index| {
                let image_size = if index >= 5 { size } else { quarter };
                Intermediate::new(device, image_size)
            })
            .collect();
        Self {
            size,
            quarter,
            images,
        }
    }
    fn ensure_water(&mut self, device: &RenderDevice) {
        if self.images.len() == 8 {
            self.images.push(Intermediate::new(device, self.size));
        }
    }
    fn intermediate(&self, image: Image) -> &Intermediate {
        &self.images[match image {
            Image::Downsample => 0,
            Image::Blurred => 1,
            Image::Coc => 2,
            Image::Small => 3,
            Image::Ping => 4,
            Image::Graded => 5,
            Image::ScreenBlur => 6,
            Image::ScreenPing => 7,
            Image::PostProcessed => 8,
            _ => unreachable!("external image"),
        }]
    }
    fn view(&self, image: Image) -> &TextureView {
        &self.intermediate(image).view
    }
    fn attachment(&self, image: Image, srgb: bool) -> &TextureView {
        let image = self.intermediate(image);
        if srgb { &image.srgb_view } else { &image.view }
    }
}

#[derive(Resource, Default)]
struct ExactPostFxGpu {
    prepared: Vec<PreparedPostFxGpu>,
    inactive: Vec<PreparedPostFxGpu>,
    pipelines: PostFxPipelines,
    steps: Vec<FilterStep>,
    targets: Option<PostFxTargets>,
    depth_sampler: Option<bevy::render::render_resource::Sampler>,
    grade: Option<super::script_grade::GradeGpu>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum PostFxGpuRefusal {
    VertexLayout,
    EmptyConstantBlock,
    UnsupportedState {
        fields: super::state::UnsupportedStateFields,
        word0: u32,
        word1: u32,
    },
}
fn prepare_postfx_gpu(
    extracted: Res<ExtractedPostFx>,
    grade_shader: Res<super::script_grade::GradeShader>,
    mut gpu: ResMut<ExactPostFxGpu>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    views: Query<&ExtractedView>,
    mut missing: Local<Option<(MaterialGenerationId, &'static str)>>,
) {
    let (Some(sampler), Some(depth_sampler), Some(film)) = (
        extracted.sampler,
        extracted.depth_sampler,
        extracted.films.first(),
    ) else {
        gpu.prepared.clear();
        gpu.inactive.clear();
        gpu.pipelines.clear();
        gpu.steps.clear();
        gpu.targets = None;
        return;
    };
    // A plan change mid-match (a vision lerp moves the bloom radius onto another
    // gaussian width) must find its pipeline compiled: a pending one refuses the
    // whole post-fx frame, film grade included.
    let generation = film.generation;
    gpu.pipelines.retain(|(_, g), _| *g == generation);
    for film in &extracted.films {
        let ExactPostFxGpu { pipelines, .. } = &mut *gpu;
        let _ = postfx_pipeline(film, &cache, pipelines);
    }
    let Some(view) = views.iter().find(|v| v.viewport.z > 0 && v.viewport.w > 0) else {
        return;
    };
    let size = UVec2::new(view.viewport.z, view.viewport.w);
    if gpu.grade.as_ref().is_none_or(|grade| grade.size != size) {
        gpu.grade = Some(super::script_grade::GradeGpu::new(
            &device,
            &cache,
            grade_shader.0.clone(),
            size,
        ));
    }
    if gpu.targets.as_ref().is_none_or(|t| t.size != size) {
        gpu.targets = Some(PostFxTargets::new(&device, size));
    }
    if extracted.water.is_some() {
        gpu.targets.as_mut().unwrap().ensure_water(&device);
    }
    let mut glow_ready = extracted.frame.glow.using()
        && extracted
            .films
            .iter()
            .any(|film| film.name == postfx_dof::GLOW_SETUP_MATERIAL)
        && extracted
            .films
            .iter()
            .any(|film| film.name == postfx_dof::GLOW_APPLY_MATERIAL);
    let (steps, prepared) = loop {
        let steps = filter_plan(
            extracted.frame,
            size,
            gpu.targets.as_ref().unwrap().quarter,
            glow_ready,
            extracted.water.is_some(),
        );
        if gpu.prepared.len() == steps.len()
            && gpu
                .prepared
                .iter()
                .zip(&steps)
                .all(|(p, s)| p.film.name == s.name && p.film.generation == film.generation)
        {
            break (steps, std::mem::take(&mut gpu.prepared));
        }
        let mut available = std::mem::take(&mut gpu.prepared);
        available.append(&mut gpu.inactive);
        available.retain(|p| p.film.generation == film.generation);
        let mut prepared = Vec::new();
        let mut glow_failed = false;
        for step in &steps {
            let Some(film) = extracted.films.iter().find(|f| f.name == step.name) else {
                if glow_ready && step.name != WATER_SHEETING_MATERIAL {
                    glow_failed = true;
                    break;
                }
                if *missing != Some((film.generation, step.name)) {
                    diag::warn!(
                        World,
                        "post-fx GPU prepare: RED missing material={}",
                        step.name
                    );
                    *missing = Some((film.generation, step.name));
                }
                gpu.prepared.clear();
                gpu.inactive = available;
                gpu.steps.clear();
                return;
            };
            if let Some(index) = available.iter().position(|p| p.film.name == step.name) {
                prepared.push(available.swap_remove(index));
                continue;
            }
            match create_postfx_gpu(film.clone(), sampler, &device, &cache, &mut gpu.pipelines) {
                Ok(p) => prepared.push(p),
                Err(e) => {
                    if glow_ready
                        && (step.name == postfx_dof::GLOW_SETUP_MATERIAL
                            || step.name == postfx_dof::GLOW_APPLY_MATERIAL)
                    {
                        diag::warn!(
                            World,
                            "post-fx glow GPU skipped material={} cause={e:?}",
                            step.name
                        );
                        glow_failed = true;
                        break;
                    }
                    diag::warn!(
                        World,
                        "post-fx GPU prepare: RED material={} cause={e:?}",
                        step.name
                    );
                    gpu.prepared.clear();
                    gpu.inactive = available;
                    gpu.steps.clear();
                    return;
                }
            }
        }
        if glow_failed {
            gpu.prepared = available;
            gpu.prepared.append(&mut prepared);
            glow_ready = false;
            continue;
        }
        gpu.inactive = available;
        break (steps, prepared);
    };
    *missing = None;
    gpu.prepared = prepared;
    gpu.steps = steps;
    if gpu.depth_sampler.is_none() {
        gpu.depth_sampler = Some(device.create_sampler(&depth_sampler.descriptor()));
    }
}
type PostFxPipelines =
    HashMap<(&'static str, MaterialGenerationId), Result<CachedRenderPipelineId, PostFxGpuRefusal>>;

fn postfx_pipeline(
    film: &ExtractedFilm,
    cache: &PipelineCache,
    pipelines: &mut PostFxPipelines,
) -> Result<CachedRenderPipelineId, PostFxGpuRefusal> {
    pipelines
        .entry((film.name, film.generation))
        .or_insert_with(|| queue_postfx_pipeline(film, cache))
        .clone()
}

fn postfx_layouts(film: &ExtractedFilm) -> (BindGroupLayoutDescriptor, BindGroupLayoutDescriptor) {
    let (constant_entries, texture_entries) = split_bind_layout(film.port.wgpu_layout());
    (
        super::colour_submit::bind_group_layout_from_entries(
            "iw4_postfx_constants",
            &constant_entries,
            false,
        ),
        super::colour_submit::bind_group_layout_from_entries(
            "iw4_postfx_textures",
            &texture_entries,
            false,
        ),
    )
}

fn queue_postfx_pipeline(
    film: &ExtractedFilm,
    cache: &PipelineCache,
) -> Result<CachedRenderPipelineId, PostFxGpuRefusal> {
    let state = super::state::GfxPassState::from_prepared(film.shell.passes()[0].state());
    if let Some(fields) = state.unsupported_host_fields() {
        return Err(PostFxGpuRefusal::UnsupportedState {
            fields,
            word0: state.authored_words()[0],
            word1: state.authored_words()[1],
        });
    }
    let vertex_layouts =
        super::colour_submit::vertex_layouts_from_contract(film.port.wgpu_layout());
    if film.port.abi().vertex_type != POSTFX_VERTEX_TYPE
        || film.port.wgpu_layout().vertex_buffers.len() != 1
        || film.port.wgpu_layout().vertex_buffers[0].stream != 0
        || vertex_layouts.len() != 1
        || vertex_layouts[0].array_stride != hud_iw4::GFX_TESS_VERTEX_STRIDE as u64
    {
        return Err(PostFxGpuRefusal::VertexLayout);
    }
    if film.port.module().vertex_constant_len + film.port.module().pixel_constant_len == 0 {
        return Err(PostFxGpuRefusal::EmptyConstantBlock);
    }
    let (constants_layout, textures_layout) = postfx_layouts(film);
    let host_state = state.apply_change_state_0_host(false);
    Ok(cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("iw4_postfx_film".into()),
        layout: vec![constants_layout, textures_layout],
        immediate_size: 0,
        vertex: VertexState {
            shader: film.shader.clone(),
            shader_defs: Vec::new(),
            constants: Vec::new(),
            entry_point: Some(PASS_VERTEX_ENTRY.into()),
            buffers: vertex_layouts,
        },
        fragment: Some(FragmentState {
            shader: film.shader.clone(),
            shader_defs: Vec::new(),
            constants: Vec::new(),
            entry_point: Some(PASS_FRAGMENT_ENTRY.into()),
            targets: vec![Some(ColorTargetState {
                format: if state.srgb_write_enable() {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                },
                blend: host_state.blend.blend_state(),
                write_mask: host_state.colour_writes(),
            })],
        }),
        primitive: super::colour_submit::exact_primitive_state(
            host_state.cull,
            host_state.line_fill,
        ),
        depth_stencil: None,
        multisample: default(),
        zero_initialize_workgroup_memory: false,
    }))
}

fn create_postfx_gpu(
    film: ExtractedFilm,
    sampler: super::DecodedSampler,
    device: &RenderDevice,
    cache: &PipelineCache,
    pipelines: &mut PostFxPipelines,
) -> Result<PreparedPostFxGpu, PostFxGpuRefusal> {
    let pipeline = postfx_pipeline(&film, cache, pipelines)?;
    let module = film.port.module();
    let total_size = (module.vertex_constant_len + module.pixel_constant_len) * 16
        + d3d9_sm3::texture_slot_rows(module.sampler_count) * 16;
    let (constants_layout, textures_layout) = postfx_layouts(&film);
    let constants_arena = device.create_buffer(&BufferDescriptor {
        label: Some("iw4_postfx_constant_arena"),
        size: total_size as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bgl = cache.get_bind_group_layout(&constants_layout);
    let constants = device.create_bind_group(
        "iw4_postfx_constants",
        &bgl,
        &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &constants_arena,
                offset: 0,
                size: None,
            }),
        }],
    );
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("iw4_postfx_vertices"),
        size: (4 * hud_iw4::GFX_TESS_VERTEX_STRIDE) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("iw4_postfx_indices"),
        contents: bytemuck::cast_slice(&hud_iw4::RB_DRAW_STRETCHPIC_INDICES),
        usage: BufferUsages::INDEX,
    });
    Ok(PreparedPostFxGpu {
        film,
        pipeline,
        constants,
        textures_layout,
        sampler: device.create_sampler(&sampler.descriptor()),
        vertex,
        index,
        constants_arena,
    })
}

fn packed_fullscreen_vertices(width: u32, height: u32) -> Vec<u8> {
    hud_iw4::draw_stretch_pic_pack(
        hud_iw4::AppliedRect {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        },
        hud_iw4::TextureWindow {
            min: [0.0, 0.0],
            max: [1.0, 1.0],
        },
        0xffff_ffff,
    )
    .into_iter()
    .flat_map(hud_iw4::GfxTessVertex2d::to_bytes)
    .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum PostFxSubmitRefusal {
    TargetFormat(TextureFormat),
    PartialViewport {
        viewport: UVec4,
        target: UVec2,
    },
    FloatZNotResolvedForFrame,
    PipelinePending,
    DofPlan(DofPlanRefusal),
    FilmSource(String),
    Execute {
        material: &'static str,
        cause: MaterialRefusal,
    },
    PassCount,
    PortMismatch,
    Constants {
        material: &'static str,
        cause: ConstantPackRefusal,
    },
    MissingCodeImage(u32),
    MaterialTexture(TextureBindRefusal),
    SamplerTableMissing,
    SamplerMismatch,
    MissingSamplerRegister(u16),
    PipelineFailed {
        material: &'static str,
        error: String,
    },
}

fn require_postfx_pipeline(
    state: &CachedPipelineState,
    material: &'static str,
) -> Result<(), PostFxSubmitRefusal> {
    use bevy::shader::ShaderCacheError;
    match state {
        CachedPipelineState::Ok(_) => Ok(()),
        CachedPipelineState::Queued
        | CachedPipelineState::Creating(_)
        | CachedPipelineState::Err(
            ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable,
        ) => Err(PostFxSubmitRefusal::PipelinePending),
        CachedPipelineState::Err(error) => Err(PostFxSubmitRefusal::PipelineFailed {
            material,
            error: error.to_string(),
        }),
    }
}

/// The refusal as a number for the counter: the `Debug` string exists only in
/// the change-only warn below, never per frame. Numbers are stable — a report
/// compares them across runs, so a new variant goes at the end.
fn refusal_discriminant(cause: &PostFxSubmitRefusal) -> u8 {
    match cause {
        PostFxSubmitRefusal::TargetFormat(_) => 0,
        PostFxSubmitRefusal::PartialViewport { .. } => 1,
        PostFxSubmitRefusal::FloatZNotResolvedForFrame => 2,
        PostFxSubmitRefusal::PipelinePending => 3,
        PostFxSubmitRefusal::DofPlan(_) => 4,
        PostFxSubmitRefusal::FilmSource(_) => 5,
        PostFxSubmitRefusal::Execute { .. } => 6,
        PostFxSubmitRefusal::PassCount => 7,
        PostFxSubmitRefusal::PortMismatch => 8,
        PostFxSubmitRefusal::Constants { .. } => 9,
        PostFxSubmitRefusal::MissingCodeImage(_) => 10,
        PostFxSubmitRefusal::SamplerMismatch => 11,
        PostFxSubmitRefusal::MissingSamplerRegister(_) => 12,
        PostFxSubmitRefusal::PipelineFailed { .. } => 13,
        PostFxSubmitRefusal::MaterialTexture(_) => 14,
        PostFxSubmitRefusal::SamplerTableMissing => 15,
    }
}
#[derive(Default)]
struct PostFxTextureCache {
    owner: Option<(MaterialGenerationId, UVec2, u64)>,
    groups: HashMap<(PortId, Vec<TextureViewId>, Vec<SamplerKey>), BindGroup>,
}

struct PostFxPipelineWait {
    owner: MaterialGenerationId,
    started: std::time::Instant,
    reported: bool,
}

impl PostFxPipelineWait {
    fn update(
        wait: &mut Option<Self>,
        owner: MaterialGenerationId,
        now: std::time::Instant,
    ) -> Option<std::time::Duration> {
        if wait.as_ref().is_none_or(|wait| wait.owner != owner) {
            *wait = Some(Self {
                owner,
                started: now,
                reported: false,
            });
        }
        let wait = wait.as_mut()?;
        let elapsed = now.saturating_duration_since(wait.started);
        if !wait.reported && elapsed >= std::time::Duration::from_secs(20) {
            wait.reported = true;
            Some(elapsed)
        } else {
            None
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PostFxFrame<'w> {
    extracted: Res<'w, ExtractedPostFx>,
    floatz: Res<'w, super::floatz::ExactFloatZResolve>,
    colour_frame: Res<'w, super::PublishedRenderFrame>,
    uploaded: Res<'w, RuntimeUploadedImageRegistry>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PostFxPipelineLookup<'w> {
    gpu: Res<'w, ExactPostFxGpu>,
    cache: Res<'w, PipelineCache>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PostFxRecorder<'w, 's> {
    texture_table: ResMut<'w, super::texture_table::ExactTextureTable>,
    refusal: Local<'s, Option<PostFxSubmitRefusal>>,
    pipeline_wait: Local<'s, Option<PostFxPipelineWait>>,
    texture_cache: Local<'s, PostFxTextureCache>,
    submitted: Local<'s, Option<(u64, bool, bool, UVec2)>>,
}

impl PostFxFrame<'_> {
    fn active(&self) -> bool {
        self.extracted.frame.dof.active()
    }
    fn graded(&self) -> bool {
        self.extracted.frame.grading != [0.0, 1.0, 0.0, 1.0] || self.extracted.t6_vision.is_some()
    }
}

impl PostFxPipelineLookup<'_> {
    fn require(
        &self,
        pipeline: CachedRenderPipelineId,
        material: &'static str,
    ) -> Result<(), PostFxSubmitRefusal> {
        require_postfx_pipeline(self.cache.get_render_pipeline_state(pipeline), material)
    }
}

struct PostFxUpload {
    bytes: Vec<u8>,
    material_textures: Vec<UploadedTextureBind>,
}

fn water_sources(
    size: UVec2,
    water: WaterSheetingFrame,
    frame: &RuntimeCodeSources,
) -> Result<RuntimeCodeSources, PostFxSubmitRefusal> {
    let mut sources =
        film_sources(size.x, size.y, None).map_err(PostFxSubmitRefusal::FilmSource)?;
    let time = frame.constant_arc(7).ok_or_else(|| {
        PostFxSubmitRefusal::FilmSource("water sheeting needs frame game time".into())
    })?;
    sources.set_constant_rows(7, &time);
    sources.set_constant_rows(54, &[water.distortion.map(f32::to_bits)]);
    sources.set_constant_rows(55, &[[0.0, 1.0, 0.0, water.fade].map(f32::to_bits)]);
    sources.set_constant_rows(56, &[[0.0, 0.0, 1.0, 1.0].map(f32::to_bits)]);
    Ok(sources)
}

fn postfx_texture_slots(
    film: &ExtractedFilm,
    pass: ExecutablePassView<'_>,
    step: &FilterStep,
    world: &super::colour_submit::RenderWorldData,
    uploaded: &RuntimeUploadedImageRegistry,
) -> Result<(Vec<u32>, Vec<UploadedTextureBind>), PostFxSubmitRefusal> {
    let mut slots = vec![0; d3d9_sm3::texture_slot_rows(film.port.module().sampler_count) * 4];
    let mut material_textures = Vec::new();
    for (ordinal, binding) in film.port.abi().samplers.iter().enumerate() {
        if binding.dimension != SamplerTextureDimension::D2 {
            return Err(PostFxSubmitRefusal::SamplerMismatch);
        }
        let (image_index, sampler_index) = match binding.source {
            SamplerSource::CodeTexture { index } => {
                let lane = pass
                    .code_samplers
                    .iter()
                    .find(|lane| lane.register == binding.register && lane.index == index)
                    .ok_or(PostFxSubmitRefusal::MissingSamplerRegister(
                        binding.register,
                    ))?;
                let image = step
                    .images
                    .iter()
                    .position(|(code, _)| *code == index)
                    .ok_or(PostFxSubmitRefusal::MissingCodeImage(index))?;
                let expected = if index == 15 { 0x61 } else { 0x62 };
                if lane.sampler_state != expected || lane.image.is_some() {
                    return Err(PostFxSubmitRefusal::SamplerMismatch);
                }
                (image, usize::from(index == 15))
            }
            SamplerSource::MaterialTexture { name_hash } => {
                let samplers = world
                    .sampler_table
                    .as_ref()
                    .ok_or(PostFxSubmitRefusal::SamplerTableMissing)?;
                let images =
                    TextureImageTables::new(&world.image_handles, film.generation, uploaded)
                        .map_err(PostFxSubmitRefusal::MaterialTexture)?;
                let texture = images
                    .resolve_material_texture(
                        pass,
                        binding.register,
                        name_hash,
                        binding.dimension,
                        samplers,
                    )
                    .map_err(PostFxSubmitRefusal::MaterialTexture)?;
                let image = step.images.len() + material_textures.len();
                let sampler = 2 + material_textures.len();
                material_textures.push(texture);
                (image, sampler)
            }
            _ => return Err(PostFxSubmitRefusal::SamplerMismatch),
        };
        slots[ordinal] = d3d9_sm3::texture_slot_word(image_index as u16, sampler_index as u16);
    }
    Ok((slots, material_textures))
}

impl PostFxRecorder<'_, '_> {
    fn record(
        mut self,
        view: ViewQuery<(&ViewTarget, &ExtractedView)>,
        source: PostFxFrame,
        pipelines: PostFxPipelineLookup,
        device: Res<RenderDevice>,
        queue: Res<RenderQueue>,
        mut context: RenderContext,
    ) {
        let products = &source.colour_frame.frame_products;
        let (target, view) = view.into_inner();
        let Some(targets) = pipelines.gpu.targets.as_ref() else {
            *self.pipeline_wait = None;
            return;
        };
        if pipelines.gpu.prepared.is_empty()
            || pipelines.gpu.prepared.len() != pipelines.gpu.steps.len()
        {
            *self.pipeline_wait = None;
            return;
        }
        let active = source.active();
        let graded = source.graded();
        let prepare = || -> Result<Vec<PostFxUpload>, PostFxSubmitRefusal> {
            if target.main_texture_format() != TextureFormat::Rgba8Unorm {
                return Err(PostFxSubmitRefusal::TargetFormat(
                    target.main_texture_format(),
                ));
            }
            let size = UVec2::new(
                target.main_texture().width(),
                target.main_texture().height(),
            );
            if size != targets.size || view.viewport != UVec4::new(0, 0, size.x, size.y) {
                return Err(PostFxSubmitRefusal::PartialViewport {
                    viewport: view.viewport,
                    target: size,
                });
            }
            if active
                && (source.floatz.resolved_frame != Some(products.0.frame_id)
                    || source.floatz.view().is_none())
            {
                return Err(PostFxSubmitRefusal::FloatZNotResolvedForFrame);
            }
            if graded {
                let grade = pipelines
                    .gpu
                    .grade
                    .as_ref()
                    .ok_or(PostFxSubmitRefusal::PipelinePending)?;
                for pipeline in grade.required_pipelines(
                    source
                        .extracted
                        .t6_vision
                        .is_some_and(|vision| vision.bloom.is_some()),
                ) {
                    pipelines.require(pipeline, "vision grading")?;
                }
            }
            let mut uploads = Vec::new();
            for (ready, step) in pipelines.gpu.prepared.iter().zip(&pipelines.gpu.steps) {
                pipelines.require(ready.pipeline, ready.film.name)?;
                let size = if matches!(
                    step.target,
                    Image::Output
                        | Image::Graded
                        | Image::ScreenBlur
                        | Image::ScreenPing
                        | Image::PostProcessed
                ) {
                    targets.size
                } else {
                    targets.quarter
                };
                let mut sources: RuntimeCodeSources = if step.name == WATER_SHEETING_MATERIAL {
                    water_sources(
                        size,
                        source.extracted.water.expect("water plan requires frame"),
                        &source.colour_frame.exec_frame.code_sources,
                    )?
                } else if active {
                    postfx_dof::sources(
                        size.x,
                        size.y,
                        targets.size.y,
                        source.extracted.vision,
                        source.extracted.frame,
                    )
                    .map_err(PostFxSubmitRefusal::DofPlan)?
                } else {
                    film_sources(size.x, size.y, source.extracted.vision)
                        .map_err(PostFxSubmitRefusal::FilmSource)?
                };
                for (index, _) in &step.images {
                    sources
                        .set_texture(*index, if *index == 15 { 0x61 } else { 0x62 })
                        .map_err(|cause| {
                            PostFxSubmitRefusal::FilmSource(format!(
                                "postfx code source: {cause:?}"
                            ))
                        })?;
                }
                if let Some(gaussian) = &step.gaussian {
                    for (i, row) in gaussian.taps.iter().enumerate() {
                        sources.set_constant_rows(10 + i as u16, &[row.map(f32::to_bits)]);
                    }
                }
                let execution =
                    rebind_stable_material(&ready.film.shell, &ready.film.catalog, &sources)
                        .map_err(|cause| PostFxSubmitRefusal::Execute {
                            material: step.name,
                            cause,
                        })?;
                let Some(pass) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
                    return Err(PostFxSubmitRefusal::PassCount);
                };
                if pass.port != ready.film.port.id() {
                    return Err(PostFxSubmitRefusal::PortMismatch);
                }
                let constants = ready.film.port.pack_hit(pass).map_err(|cause| {
                    PostFxSubmitRefusal::Constants {
                        material: step.name,
                        cause,
                    }
                })?;
                let mut bytes = Vec::new();
                bytes.extend_from_slice(bytemuck::cast_slice(&constants.vertex));
                bytes.extend_from_slice(bytemuck::cast_slice(&constants.pixel));
                let (slots, material_textures) = postfx_texture_slots(
                    &ready.film,
                    pass,
                    step,
                    source.colour_frame.world(),
                    &source.uploaded,
                )?;
                bytes.extend_from_slice(bytemuck::cast_slice(&slots));
                uploads.push(PostFxUpload {
                    bytes,
                    material_textures,
                });
            }
            Ok(uploads)
        };
        let planned = pipelines.gpu.steps.len();
        perf::counter!("postfx_planned_steps", "iw4l.render", planned as f64);
        perf::counter!(
            "postfx_water_sheeting",
            "iw4l.render",
            f64::from(u8::from(source.extracted.water.is_some()))
        );
        let uploads = match prepare() {
            Ok(uploads) => uploads,
            Err(cause) => {
                perf::counter!(
                    "postfx_refusal",
                    "iw4l.render",
                    f64::from(refusal_discriminant(&cause))
                );
                if cause == PostFxSubmitRefusal::PipelinePending {
                    if let Some(elapsed) = PostFxPipelineWait::update(
                        &mut self.pipeline_wait,
                        pipelines.gpu.prepared[0].film.generation,
                        std::time::Instant::now(),
                    ) {
                        diag::warn!(
                            World,
                            "post-fx pipelines still pending after {:.0}s frame={} planned={planned}",
                            elapsed.as_secs_f32(),
                            products.0.frame_id
                        );
                    }
                } else {
                    *self.pipeline_wait = None;
                }
                if self.refusal.as_ref() != Some(&cause) {
                    if cause == PostFxSubmitRefusal::PipelinePending {
                        diag::info!(
                            World,
                            "post-fx submit: waiting for pipelines frame={} planned={planned}",
                            products.0.frame_id
                        );
                    } else {
                        diag::warn!(
                            World,
                            "post-fx submit: RED frame={} planned={planned} cause={cause:?}",
                            products.0.frame_id,
                        );
                    }
                    *self.refusal = Some(cause);
                }
                return;
            }
        };
        *self.pipeline_wait = None;
        perf::counter!("postfx_executed_steps", "iw4l.render", uploads.len() as f64);

        let owner = (
            pipelines.gpu.prepared[0].film.generation,
            targets.size,
            source.uploaded.replaced_revision(),
        );
        if self.texture_cache.owner != Some(owner) {
            self.texture_cache.groups.clear();
            self.texture_cache.owner = Some(owner);
        }
        let post = target.post_process_write();
        let diagnostics = context.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let encoder = context.command_encoder();
        let span = diagnostics.time_span(encoder, GPU_SPAN_POSTFX);
        if graded {
            let grade = pipelines.gpu.grade.as_ref().expect("grade prepared");
            let grade_target = targets.view(Image::Graded);
            grade
                .prepare(
                    &queue,
                    source.extracted.frame.grading,
                    source.extracted.t6_vision,
                )
                .record(
                    &device,
                    &pipelines.cache,
                    encoder,
                    post.source,
                    grade_target,
                );
        }
        for ((ready, step), upload) in pipelines
            .gpu
            .prepared
            .iter()
            .zip(&pipelines.gpu.steps)
            .zip(&uploads)
        {
            let size = if matches!(
                step.target,
                Image::Output
                    | Image::Graded
                    | Image::ScreenBlur
                    | Image::ScreenPing
                    | Image::PostProcessed
            ) {
                targets.size
            } else {
                targets.quarter
            };
            queue.write_buffer(&ready.constants_arena, 0, &upload.bytes);
            queue.write_buffer(
                &ready.vertex,
                0,
                &packed_fullscreen_vertices(size.x, size.y),
            );
            let mut views: Vec<_> = step
                .images
                .iter()
                .map(|(_, image)| match image {
                    Image::Scene => {
                        if graded {
                            targets.view(Image::Graded)
                        } else {
                            post.source
                        }
                    }
                    Image::Depth => source.floatz.view().expect("preflight checked depth"),
                    image => targets.view(*image),
                })
                .collect();
            views.extend(upload.material_textures.iter().map(|texture| &texture.view));
            let key = (
                ready.film.port.id(),
                views.iter().map(|view| view.id()).collect(),
                upload
                    .material_textures
                    .iter()
                    .map(|texture| SamplerKey::from(&texture.sampler))
                    .collect(),
            );
            let textures = self.texture_cache.groups.entry(key).or_insert_with(|| {
                let material_samplers: Vec<_> = upload
                    .material_textures
                    .iter()
                    .map(|texture| device.create_sampler(&texture.sampler.descriptor()))
                    .collect();
                let mut samplers = vec![
                    &ready.sampler,
                    pipelines.gpu.depth_sampler.as_ref().expect("prepared"),
                ];
                samplers.extend(material_samplers.iter());
                self.texture_table.views_bind_group(
                    &device,
                    &pipelines
                        .cache
                        .get_bind_group_layout(&ready.textures_layout),
                    "iw4_postfx_images",
                    &views,
                    &samplers,
                )
            });
            let destination = if step.target == Image::Output {
                post.destination
            } else {
                let state =
                    super::state::GfxPassState::from_prepared(ready.film.shell.passes()[0].state());
                targets.attachment(step.target, state.srgb_write_enable())
            };
            let attachments = [Some(RenderPassColorAttachment {
                view: destination,
                resolve_target: None,
                ops: Operations {
                    load: if step.composite {
                        LoadOp::Load
                    } else {
                        LoadOp::Clear(LinearRgba::BLACK.into())
                    },
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })];
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(step.name),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(
                pipelines
                    .cache
                    .get_render_pipeline(ready.pipeline)
                    .expect("preflight"),
            );
            pass.set_bind_group(0, &ready.constants, &[]);
            pass.set_bind_group(1, &*textures, &[]);
            pass.set_vertex_buffer(0, *ready.vertex.slice(..));
            pass.set_index_buffer(*ready.index.slice(..), IndexFormat::Uint16);
            pass.set_viewport(0.0, 0.0, size.x as f32, size.y as f32, 0.0, 1.0);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        span.end(encoder);
        let state = (
            pipelines.gpu.prepared[0].film.generation.get(),
            active,
            source.extracted.water.is_some(),
            targets.size,
        );
        if self.submitted.as_ref() != Some(&state) || self.refusal.is_some() {
            diag::info!(
                World,
                "post-fx submit: READY dof={active} water={} passes={} size={:?} focus={:?}",
                source.extracted.water.is_some(),
                pipelines.gpu.steps.len(),
                targets.size,
                source.extracted.frame
            );
            *self.submitted = Some(state);
        }
        *self.refusal = None;
    }
}

fn draw_postfx(
    view: ViewQuery<(&ViewTarget, &ExtractedView)>,
    source: PostFxFrame,
    pipelines: PostFxPipelineLookup,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    context: RenderContext,
    recorder: PostFxRecorder,
) {
    recorder.record(view, source, pipelines, device, queue, context);
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PostFxSet;

pub(super) fn register(app: &mut App) {
    if app.get_sub_app(RenderApp).is_none() {
        return;
    }
    let shader = app
        .world_mut()
        .resource_mut::<Assets<Shader>>()
        .add(Shader::from_wgsl(
            include_str!("script_grade.wgsl"),
            "script_grade.wgsl",
        ));
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .insert_resource(super::script_grade::GradeShader(shader))
        .init_resource::<ExtractedPostFx>()
        .init_resource::<ExactPostFxGpu>()
        .add_systems(Render, prepare_postfx_gpu.in_set(RenderSystems::Prepare))
        .add_systems(
            Core3d,
            draw_postfx
                .in_set(PostFxSet)
                .in_set(Core3dSystems::PostProcess)
                .after(tonemapping),
        );
}
