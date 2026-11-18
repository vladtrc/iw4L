use bevy::prelude::*;
use bevy::render::RenderStartup;
use bevy::render::render_phase::TrackedRenderPass;
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderDevice;
use bevy::shader::Shader;

const SHADER_PATH: &str = "embedded://render_gpu/drawsurf/floatz.wesl";
pub(super) const EFFECT_DEPTH_FORMAT: TextureFormat = TextureFormat::R32Float;
pub(super) const FAR_DEPTH: f64 = 1.0e7;

struct ExactFloatZGpu {
    _texture: Texture,
    view: TextureView,
    width: u32,
    height: u32,
    samples: u32,
}

#[derive(Resource)]
pub struct ExactFloatZResolve {
    pub resolved_frame: Option<u64>,
    shader: Handle<Shader>,
    layout_single: BindGroupLayoutDescriptor,
    layout_msaa: BindGroupLayoutDescriptor,
    target: Option<ExactFloatZGpu>,
    effect: Option<ExactFloatZGpu>,
    prepared: Option<PreparedFloatZBlit>,
}

struct PreparedFloatZBlit {
    source: TextureViewId,
    pipeline: CachedRenderPipelineId,
    bind_group: BindGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FloatZBlitKey {
    multisampled: bool,
}

impl SpecializedRenderPipeline for ExactFloatZResolve {
    type Key = FloatZBlitKey;
    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let shader_defs = if key.multisampled {
            vec!["MULTISAMPLED".into()]
        } else {
            Vec::new()
        };
        RenderPipelineDescriptor {
            label: Some("iw4_floatz_resolve".into()),
            layout: vec![if key.multisampled {
                self.layout_msaa.clone()
            } else {
                self.layout_single.clone()
            }],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: shader_defs.clone(),
                entry_point: Some("vs_fullscreen".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: Some("fs_floatz".into()),
                targets: vec![Some(ColorTargetState {
                    format: EFFECT_DEPTH_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::RED,
                })],
                ..default()
            }),
            ..default()
        }
    }
}

fn init_pipeline(mut commands: Commands, asset_server: Res<AssetServer>) {
    let layout = |multisampled| {
        BindGroupLayoutDescriptor::new(
            "iw4_effect_depth",
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled,
                },
                count: None,
            }],
        )
    };
    commands.insert_resource(ExactFloatZResolve {
        prepared: None,
        shader: asset_server.load(SHADER_PATH),
        layout_single: layout(false),
        layout_msaa: layout(true),
        target: None,
        effect: None,
        resolved_frame: None,
    });
}

pub(super) fn ensure_target(
    resolve: &mut ExactFloatZResolve,
    device: &RenderDevice,
    width: u32,
    height: u32,
    samples: u32,
) -> Option<TextureView> {
    if width == 0 || height == 0 {
        return None;
    }
    for (slot, count, label) in [
        (&mut resolve.target, 1, "iw4_floatz"),
        (&mut resolve.effect, samples, "iw4_effect_depth"),
    ] {
        if slot
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height && t.samples == count)
        {
            continue;
        }
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: count,
            dimension: TextureDimension::D2,
            format: EFFECT_DEPTH_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | if count == 1 {
                    TextureUsages::COPY_SRC
                } else {
                    TextureUsages::empty()
                },
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        *slot = Some(ExactFloatZGpu {
            _texture: texture,
            view,
            width,
            height,
            samples: count,
        });
    }
    resolve.target.as_ref().map(|t| t.view.clone())
}

pub(super) fn prepare_blit(
    resolve: &mut ExactFloatZResolve,
    cache: &PipelineCache,
    device: &RenderDevice,
    specialized: &mut SpecializedRenderPipelines<ExactFloatZResolve>,
) {
    let Some(effect) = resolve.effect.as_ref() else {
        resolve.prepared = None;
        return;
    };
    let source = effect.view.clone();
    let multisampled = effect.samples > 1;
    let pipeline = specialized.specialize(cache, &*resolve, FloatZBlitKey { multisampled });
    if resolve
        .prepared
        .as_ref()
        .is_some_and(|p| p.source == source.id() && p.pipeline == pipeline)
    {
        return;
    }
    let layout = if multisampled {
        &resolve.layout_msaa
    } else {
        &resolve.layout_single
    };
    let bind_group = device.create_bind_group(
        "iw4_floatz_effect_depth",
        &cache.get_bind_group_layout(layout),
        &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&source),
        }],
    );
    resolve.prepared = Some(PreparedFloatZBlit {
        source: source.id(),
        pipeline,
        bind_group,
    });
}

pub(super) fn forget_blit(resolve: &mut ExactFloatZResolve) {
    resolve.prepared = None;
}

pub(super) fn resolve_depth(
    encoder: &mut CommandEncoder,
    cache: &PipelineCache,
    device: &RenderDevice,
    resolve: &ExactFloatZResolve,
) -> bool {
    let (Some(target), Some(prepared)) = (resolve.target.as_ref(), resolve.prepared.as_ref())
    else {
        return false;
    };
    let Some(pipeline) = cache.get_render_pipeline(prepared.pipeline) else {
        return false;
    };
    let attachments = [Some(RenderPassColorAttachment {
        view: &target.view,
        resolve_target: None,
        ops: Operations {
            load: LoadOp::Clear(LinearRgba::BLACK.into()),
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    let mut pass = TrackedRenderPass::new(
        device,
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("iw4_floatz_resolve"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }),
    );
    pass.set_render_pipeline(pipeline);
    pass.set_bind_group(0, &prepared.bind_group, &[]);
    pass.draw(0..3, 0..1);
    true
}

pub(super) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "floatz.wesl");
    if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
        render_app
            .init_resource::<SpecializedRenderPipelines<ExactFloatZResolve>>()
            .add_systems(RenderStartup, init_pipeline);
    }
}

impl ExactFloatZResolve {
    pub fn view(&self) -> Option<&TextureView> {
        self.target.as_ref().map(|t| &t.view)
    }
    pub(super) fn effect_view(&self) -> Option<&TextureView> {
        self.effect.as_ref().map(|t| &t.view)
    }
    pub(super) fn clear_effect_depth(&self, encoder: &mut CommandEncoder) {
        let Some(effect) = self.effect_view() else {
            return;
        };
        let attachments = [Some(RenderPassColorAttachment {
            view: effect,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(LinearRgba::new(FAR_DEPTH as f32, 0., 0., 0.).into()),
                store: StoreOp::Store,
            },
            depth_slice: None,
        })];
        drop(encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("clear effect depth"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
    }
}
