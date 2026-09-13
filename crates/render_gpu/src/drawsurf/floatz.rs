use std::num::NonZeroU64;

use bevy::prelude::*;
use bevy::render::RenderStartup;
use bevy::render::render_phase::TrackedRenderPass;
use bevy::render::render_resource::binding_types::{
    texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BindingResource,
    Buffer, BufferBinding, BufferDescriptor, BufferUsages, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, CommandEncoder, Extent3d, FragmentState, LoadOp, Operations,
    PipelineCache, PrimitiveState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, ShaderStages, SpecializedRenderPipeline, SpecializedRenderPipelines,
    StoreOp, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewId, VertexState,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::shader::Shader;

const SHADER_PATH: &str = "embedded://render_gpu/drawsurf/floatz.wgsl";

const PARAMS_SIZE: u64 = 16;

struct ExactFloatZGpu {
    _texture: Texture,
    view: TextureView,
    width: u32,
    height: u32,
}

#[derive(Resource)]
pub struct ExactFloatZResolve {
    pub resolved_frame: Option<u64>,
    shader: Handle<Shader>,
    layout_single: BindGroupLayoutDescriptor,
    layout_msaa: BindGroupLayoutDescriptor,
    params: Buffer,
    target: Option<ExactFloatZGpu>,

    prepared: Option<PreparedFloatZBlit>,
}

struct PreparedFloatZBlit {
    depth_view: TextureViewId,
    multisampled: bool,
    pipeline: CachedRenderPipelineId,

    params: [f32; 4],
    bind_group: BindGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FloatZBlitKey {
    multisampled: bool,
}

impl SpecializedRenderPipeline for ExactFloatZResolve {
    type Key = FloatZBlitKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let layout = if key.multisampled {
            self.layout_msaa.clone()
        } else {
            self.layout_single.clone()
        };
        let shader_defs = if key.multisampled {
            vec!["MULTISAMPLED".into()]
        } else {
            Vec::new()
        };
        RenderPipelineDescriptor {
            label: Some("iw4_floatz_resolve".into()),
            layout: vec![layout],
            immediate_size: 0,
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: shader_defs.clone(),
                entry_point: Some("vs_fullscreen".into()),
                buffers: Vec::new(),
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: Some("fs_floatz".into()),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::R32Float,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: default(),
            zero_initialize_workgroup_memory: false,
        }
    }
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
) {
    let params = device.create_buffer(&BufferDescriptor {
        label: Some("iw4_floatz_params"),
        size: PARAMS_SIZE,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    commands.insert_resource(ExactFloatZResolve {
        prepared: None,
        shader: asset_server.load(SHADER_PATH),
        layout_single: BindGroupLayoutDescriptor::new(
            "iw4_floatz_depth_single",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_depth_2d(),
                    uniform_buffer_sized(false, NonZeroU64::new(PARAMS_SIZE)),
                ),
            ),
        ),
        layout_msaa: BindGroupLayoutDescriptor::new(
            "iw4_floatz_depth_msaa",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_depth_2d_multisampled(),
                    uniform_buffer_sized(false, NonZeroU64::new(PARAMS_SIZE)),
                ),
            ),
        ),
        params,
        target: None,
        resolved_frame: None,
    });
}

pub(super) fn ensure_target(
    resolve: &mut ExactFloatZResolve,
    device: &RenderDevice,
    width: u32,
    height: u32,
) -> (Option<TextureView>, bool) {
    if width == 0 || height == 0 {
        return (None, false);
    }
    let resized = resolve
        .target
        .as_ref()
        .is_none_or(|target| target.width != width || target.height != height);
    if resized {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("iw4_floatz"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor {
            label: Some("iw4_floatz_view"),
            ..Default::default()
        });
        resolve.target = Some(ExactFloatZGpu {
            _texture: texture,
            view: view.clone(),
            width,
            height,
        });
        (Some(view), true)
    } else {
        (
            resolve.target.as_ref().map(|target| target.view.clone()),
            false,
        )
    }
}

pub(super) fn znear_from_clip_from_view(clip_from_view: Mat4) -> Option<f32> {
    let znear = clip_from_view.w_axis.z;
    (znear > 0.0).then_some(znear)
}

pub(super) fn prepare_blit(
    resolve: &mut ExactFloatZResolve,
    cache: &PipelineCache,
    device: &RenderDevice,
    queue: &RenderQueue,
    specialized: &mut SpecializedRenderPipelines<ExactFloatZResolve>,
    depth_view: &TextureView,
    samples: u32,
    znear: f32,
    viewmodel_near: Option<f32>,
) {
    let Some(viewmodel_near) = viewmodel_near else {
        resolve.prepared = None;
        return;
    };
    if !znear.is_finite() || znear <= 0.0 || !viewmodel_near.is_finite() || viewmodel_near <= 0.0 {
        resolve.prepared = None;
        return;
    }
    if resolve.target.is_none() {
        resolve.prepared = None;
        return;
    }
    let multisampled = samples > 1;
    let pipeline = specialized.specialize(cache, &*resolve, FloatZBlitKey { multisampled });
    let params = [
        znear,
        viewmodel_near,
        1.0 - render_backend::DEPTH_RANGE_BAND,
        render_backend::DEPTH_RANGE_BAND,
    ];
    let depth_view_id = depth_view.id();
    if let Some(prepared) = resolve.prepared.as_mut()
        && prepared.depth_view == depth_view_id
        && prepared.multisampled == multisampled
        && prepared.pipeline == pipeline
    {
        if prepared.params != params {
            prepared.params = params;
            queue.write_buffer(&resolve.params, 0, bytemuck::bytes_of(&params));
        }
        return;
    }
    let layout = if multisampled {
        &resolve.layout_msaa
    } else {
        &resolve.layout_single
    };
    let bgl = cache.get_bind_group_layout(layout);
    queue.write_buffer(&resolve.params, 0, bytemuck::bytes_of(&params));
    let bind_group = device.create_bind_group(
        "iw4_floatz_blit_depth",
        &bgl,
        &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(depth_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &resolve.params,
                    offset: 0,
                    size: NonZeroU64::new(PARAMS_SIZE),
                }),
            },
        ],
    );
    resolve.prepared = Some(PreparedFloatZBlit {
        depth_view: depth_view_id,
        multisampled,
        pipeline,
        params,
        bind_group,
    });
}

pub(super) fn forget_blit(resolve: &mut ExactFloatZResolve) {
    resolve.prepared = None;
}

pub(super) fn blit_depth(
    context: &mut CommandEncoder,
    cache: &PipelineCache,
    device: &RenderDevice,
    resolve: &ExactFloatZResolve,
) -> bool {
    let (Some(target), Some(prepared)) = (resolve.target.as_ref(), resolve.prepared.as_ref())
    else {
        return false;
    };
    let Some(gpu_pipeline) = cache.get_render_pipeline(prepared.pipeline) else {
        return false;
    };
    let bind_group = &prepared.bind_group;
    let attachments = [Some(RenderPassColorAttachment {
        view: &target.view,
        resolve_target: None,
        ops: Operations {
            load: LoadOp::Clear(LinearRgba::BLACK.into()),
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    {
        let mut pass = TrackedRenderPass::new(
            device,
            context.begin_render_pass(&RenderPassDescriptor {
                label: Some("iw4_floatz_resolve"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }),
        );
        pass.set_render_pipeline(gpu_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    true
}

pub(super) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "floatz.wgsl");
    let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render_app
        .init_resource::<SpecializedRenderPipelines<ExactFloatZResolve>>()
        .add_systems(RenderStartup, init_pipeline);
}

impl ExactFloatZResolve {
    pub fn view(&self) -> Option<&TextureView> {
        self.target.as_ref().map(|t| &t.view)
    }
}
