use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::prelude::*;
use bevy::render::RenderStartup;
use bevy::render::render_resource::binding_types::{
    sampler, texture_2d, texture_depth_2d, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent,
    BlendFactor, BlendOperation, BlendState, Buffer, BufferDescriptor, BufferUsages,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState,
    FrontFace, LoadOp, Operations, PipelineCache, PrimitiveState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, SpecializedRenderPipeline, SpecializedRenderPipelines, StoreOp, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, VertexState,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::{ExtractedView, ViewDepthTexture, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::Shader;
use render_frame::SunEffectsFrame;
use std::num::NonZeroU64;

use super::gpu_resources::RuntimeUploadedImageRegistry;
use super::postfx::PostFxSet;

const SHADER_PATH: &str = "embedded://render_gpu/drawsurf/sun_effects.wgsl";
const PARAMS_SIZE: u64 = 80;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SunParamsGpu {
    clip: [f32; 4],
    viewport: [f32; 4],
    fade: [f32; 4],
    blind: [f32; 4],
    glare: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SunPass {
    History,
    Sprite,
    Flare,
    Blind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SunPipelineKey {
    target: TextureFormat,
    samples: u32,
    pass: SunPass,
}

struct HistoryTarget {
    _texture: Texture,
    view: TextureView,
}

struct WhiteTarget {
    _texture: Texture,
    view: TextureView,
}

#[derive(Resource)]
struct SunEffectsGpu {
    shader: Handle<Shader>,
    layout: BindGroupLayoutDescriptor,
    params: Buffer,
    sampler: Sampler,
    white: WhiteTarget,
    history: [HistoryTarget; 2],
    slot: u32,
    pipelines: [Option<CachedRenderPipelineId>; 4],
}

impl SpecializedRenderPipeline for SunEffectsGpu {
    type Key = SunPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let (vertex, fragment, blend, label, defs) = match key.pass {
            SunPass::History => (
                "vs_history",
                "fs_history",
                None,
                "iw4_sun_history",
                Vec::new(),
            ),
            SunPass::Sprite => (
                "vs_billboard",
                "fs_sprite",
                Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                }),
                "iw4_sun_sprite",
                Vec::new(),
            ),
            SunPass::Flare => (
                "vs_billboard",
                "fs_flare",
                Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                }),
                "iw4_sun_flare",
                vec!["SUN_FLARE".into()],
            ),
            SunPass::Blind => ("vs_blind", "fs_blind", None, "iw4_sun_blind", Vec::new()),
        };
        RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![self.layout.clone()],
            immediate_size: 0,
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs.clone(),
                entry_point: Some(vertex.into()),
                buffers: Vec::new(),
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: defs,
                entry_point: Some(fragment.into()),
                targets: vec![Some(ColorTargetState {
                    format: key.target,
                    blend,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                ..default()
            },
            depth_stencil: None,
            multisample: bevy::render::render_resource::MultisampleState {
                count: key.samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            zero_initialize_workgroup_memory: false,
        }
    }
}

fn history_target(device: &RenderDevice, label: &'static str) -> HistoryTarget {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    HistoryTarget {
        _texture: texture,
        view,
    }
}

fn white_target(device: &RenderDevice, queue: &RenderQueue) -> WhiteTarget {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("iw4_sun_white"),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[255, 255, 255, 255],
        bevy::render::render_resource::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&TextureViewDescriptor::default());
    WhiteTarget {
        _texture: texture,
        view,
    }
}

fn init_sun_effects(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let params = device.create_buffer(&BufferDescriptor {
        label: Some("iw4_sun_params"),
        size: PARAMS_SIZE,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sun_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("iw4_sun_sampler"),
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let layout = BindGroupLayoutDescriptor::new(
        "iw4_sun_effects",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                uniform_buffer_sized(false, NonZeroU64::new(PARAMS_SIZE)),
                texture_depth_2d(),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    commands.insert_resource(SunEffectsGpu {
        shader: asset_server.load(SHADER_PATH),
        layout,
        params,
        sampler: sun_sampler,
        white: white_target(&device, &queue),
        history: [
            history_target(&device, "iw4_sun_history_a"),
            history_target(&device, "iw4_sun_history_b"),
        ],
        slot: 0,
        pipelines: [None; 4],
    });
}

fn prepare_sun_effects(
    gpu: Option<ResMut<SunEffectsGpu>>,
    cache: Res<PipelineCache>,
    mut specialized: ResMut<SpecializedRenderPipelines<SunEffectsGpu>>,
) {
    let Some(mut gpu) = gpu else {
        return;
    };
    if gpu.pipelines.iter().all(Option::is_some) {
        return;
    }
    let keys = [
        SunPass::History,
        SunPass::Sprite,
        SunPass::Flare,
        SunPass::Blind,
    ];
    let mut ids = [None; 4];
    for (i, pass) in keys.into_iter().enumerate() {
        let format = if pass == SunPass::History {
            TextureFormat::Rgba16Float
        } else {
            TextureFormat::Rgba8Unorm
        };
        ids[i] = Some(specialized.specialize(
            &cache,
            &gpu,
            SunPipelineKey {
                target: format,
                samples: 1,
                pass,
            },
        ));
    }
    gpu.pipelines = ids;
}

fn uploaded_view<'a>(
    uploaded: &'a RuntimeUploadedImageRegistry,
    image: Option<u32>,
) -> Option<&'a TextureView> {
    let slot = image? as usize;
    uploaded
        .material_images
        .get(slot)?
        .as_ref()?
        .as_ref()
        .ok()
        .map(|view| &view.view)
}

fn pack_params(frame: &SunEffectsFrame) -> SunParamsGpu {
    SunParamsGpu {
        clip: frame.clip,
        viewport: [
            frame.viewport_w,
            frame.viewport_h,
            frame.sprite_size,
            frame.flare_size,
        ],
        fade: [
            frame.flare_alpha,
            frame.dt_ms as f32,
            frame.flare_fade_in_ms as f32,
            frame.flare_fade_out_ms as f32,
        ],
        blind: [
            frame.blind_goal,
            frame.blind_fade_in_ms as f32,
            frame.blind_fade_out_ms as f32,
            frame.glare_goal,
        ],
        glare: [
            frame.glare_fade_in_ms as f32,
            frame.glare_fade_out_ms as f32,
            if frame.camera_cut { 1.0 } else { 0.0 },
            if frame.behind_camera { 1.0 } else { 0.0 },
        ],
    }
}

fn bind_sun<'a>(
    gpu: &'a SunEffectsGpu,
    cache: &PipelineCache,
    device: &RenderDevice,
    depth: &TextureView,
    history: &TextureView,
    color: &TextureView,
    scene: &TextureView,
) -> bevy::render::render_resource::BindGroup {
    device.create_bind_group(
        Some("iw4_sun_effects"),
        &cache.get_bind_group_layout(&gpu.layout),
        &BindGroupEntries::sequential((
            gpu.params.as_entire_buffer_binding(),
            depth,
            history,
            color,
            &gpu.sampler,
            scene,
        )),
    )
}

fn draw_sun_effects(
    view: ViewQuery<(&ViewTarget, &ExtractedView, &ViewDepthTexture)>,
    frame: Res<super::PublishedRenderFrame>,
    gpu: Option<ResMut<SunEffectsGpu>>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    uploaded: Res<RuntimeUploadedImageRegistry>,
    mut context: RenderContext,
) {
    let Some(mut gpu) = gpu else {
        return;
    };
    let Some(def) = frame.world().sun_effects else {
        return;
    };
    let Some(sun) = frame.sun_effects else {
        return;
    };
    let pipelines = gpu.pipelines;
    let ready = |pass: usize| {
        pipelines[pass].and_then(|id| cache.get_render_pipeline(id).map(|pipe| (id, pipe)))
    };
    let Some((_, history_pipe)) = ready(0) else {
        return;
    };
    let sprite_pipe = ready(1);
    let flare_pipe = ready(2);
    let blind_pipe = ready(3);
    let (target, extracted, depth) = view.into_inner();
    if extracted.viewport.z == 0 || extracted.viewport.w == 0 {
        return;
    }
    let colour_ok = target.main_texture_format() == TextureFormat::Rgba8Unorm;
    let write = gpu.slot as usize;
    let read = 1 - write;
    queue.write_buffer(&gpu.params, 0, bytemuck::bytes_of(&pack_params(&sun)));
    let encoder = context.command_encoder();
    let history_bind = bind_sun(
        &gpu,
        &cache,
        &device,
        depth.view(),
        &gpu.history[read].view,
        &gpu.white.view,
        &gpu.white.view,
    );
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("iw4_sun_history"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &gpu.history[write].view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(LinearRgba::BLACK.into()),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(history_pipe);
        pass.set_bind_group(0, &history_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    let in_front = sun.view_dot > 0.0 && !sun.behind_camera;
    if colour_ok
        && let (Some((_, pipeline)), Some(color), true) = (
            sprite_pipe,
            uploaded_view(&uploaded, def.sprite_image),
            in_front && sun.sprite_size > 0.0,
        )
    {
        let bind = bind_sun(
            &gpu,
            &cache,
            &device,
            depth.view(),
            &gpu.history[write].view,
            color,
            &gpu.white.view,
        );
        let attachments = [Some(target.get_unsampled_color_attachment())];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("iw4_sun_sprite"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..1);
    }

    if colour_ok
        && let (Some((_, pipeline)), Some(color), true) = (
            flare_pipe,
            uploaded_view(&uploaded, def.flare_image),
            in_front && sun.flare_alpha > 1e-4 && sun.flare_size > 0.0,
        )
    {
        let bind = bind_sun(
            &gpu,
            &cache,
            &device,
            depth.view(),
            &gpu.history[write].view,
            color,
            &gpu.white.view,
        );
        let attachments = [Some(target.get_unsampled_color_attachment())];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("iw4_sun_flare"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..1);
    }

    let need_blind = colour_ok && (def.blind_max_darken > 0.0 || def.glare_max_lighten > 0.0);
    if need_blind && let Some((_, pipeline)) = blind_pipe {
        let post = target.post_process_write();
        let bind = bind_sun(
            &gpu,
            &cache,
            &device,
            depth.view(),
            &gpu.history[write].view,
            &gpu.white.view,
            post.source,
        );
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("iw4_sun_blind"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post.destination,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(LinearRgba::BLACK.into()),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
    gpu.slot = 1 - gpu.slot;
}

pub(super) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "sun_effects.wgsl");
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<SpecializedRenderPipelines<SunEffectsGpu>>()
        .add_systems(RenderStartup, init_sun_effects)
        .add_systems(Render, prepare_sun_effects.in_set(RenderSystems::Prepare))
        .add_systems(
            Core3d,
            draw_sun_effects
                .in_set(Core3dSystems::PostProcess)
                .after(tonemapping)
                .before(PostFxSet),
        );
}
