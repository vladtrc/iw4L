use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};

#[derive(Resource)]
pub(super) struct GradeShader(pub Handle<Shader>);

pub(super) struct GradeGpu {
    pub pipeline: CachedRenderPipelineId,
    layout: BindGroupLayoutDescriptor,
    uniform: Buffer,
    sampler: Sampler,
    bloom_pipelines: Vec<CachedRenderPipelineId>,
    bloom_targets: Vec<(Texture, TextureView)>,
    pub size: UVec2,
}

impl GradeGpu {
    pub fn new(
        device: &RenderDevice,
        cache: &PipelineCache,
        shader: Handle<Shader>,
        size: UVec2,
    ) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "gsc_grade",
            &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );
        let pipeline_descriptor = |entry: &str, format| RenderPipelineDescriptor {
            label: Some("gsc_grade".into()),
            layout: vec![layout.clone()],
            vertex: VertexState {
                shader: shader.clone(),
                entry_point: Some("vertex".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader: shader.clone(),
                entry_point: Some(entry.to_owned().into()),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        };
        let pipeline =
            cache.queue_render_pipeline(pipeline_descriptor("fragment", TextureFormat::Rgba8Unorm));
        let bloom_pipelines = [
            "bloom_extract",
            "bloom_downsample",
            "bloom_blur_x",
            "bloom_blur_y",
            "bloom_combine",
        ]
        .map(|entry| {
            cache.queue_render_pipeline(pipeline_descriptor(entry, TextureFormat::Rgba16Float))
        })
        .to_vec();
        let bloom_targets = [4, 8, 16, 16, 16, 8, 8, 8]
            .into_iter()
            .map(|divisor| {
                let texture = device.create_texture(&TextureDescriptor {
                    label: Some("vision_bloom"),
                    size: Extent3d {
                        width: (size.x / divisor).max(1),
                        height: (size.y / divisor).max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&TextureViewDescriptor::default());
                (texture, view)
            })
            .collect();
        Self {
            pipeline,
            bloom_pipelines,
            bloom_targets,
            size,
            layout,
            uniform: device.create_buffer(&BufferDescriptor {
                label: Some("gsc_grade"),
                size: 416,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..default()
            }),
        }
    }

    pub fn prepare(
        &self,
        queue: &RenderQueue,
        grading: [f32; 4],
        native: Option<asset_world::T6Vision>,
    ) -> GradeDraw<'_> {
        let mut rows = [[0.0f32; 4]; 26];
        rows[0] = grading;
        if let Some(native) = native {
            rows[1][1] = native.tone_strength;
            if let Some(film) = native.film {
                rows[1][0] = film.strength;
                rows[2..16].copy_from_slice(&film.controls);
            }
            if let Some(bloom) = native.bloom {
                rows[1][2] = bloom.strength;
                rows[16..].copy_from_slice(&bloom.controls);
            }
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&rows));
        let has_bloom = native.is_some_and(|vision| vision.bloom.is_some());
        GradeDraw {
            gpu: self,
            has_bloom,
        }
    }

    pub fn required_pipelines(
        &self,
        bloom: bool,
    ) -> impl Iterator<Item = CachedRenderPipelineId> + '_ {
        std::iter::once(self.pipeline).chain(self.bloom_pipelines.iter().copied().take(if bloom {
            5
        } else {
            0
        }))
    }
}

pub(super) struct GradeDraw<'a> {
    gpu: &'a GradeGpu,
    has_bloom: bool,
}

impl GradeDraw<'_> {
    pub fn record(
        self,
        device: &RenderDevice,
        cache: &PipelineCache,
        encoder: &mut CommandEncoder,
        source: &TextureView,
        target: &TextureView,
    ) {
        let Self { gpu, has_bloom } = self;
        let mut draw_pass =
            |pipeline, source: &TextureView, second: &TextureView, target: &TextureView| {
                let group = device.create_bind_group(
                    "gsc_grade",
                    &cache.get_bind_group_layout(&gpu.layout),
                    &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::TextureView(source),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::Sampler(&gpu.sampler),
                        },
                        BindGroupEntry {
                            binding: 3,
                            resource: BindingResource::TextureView(second),
                        },
                        BindGroupEntry {
                            binding: 2,
                            resource: gpu.uniform.as_entire_binding(),
                        },
                    ],
                );
                let attachments = [Some(RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(LinearRgba::BLACK.into()),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })];
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("gsc_grade"),
                    color_attachments: &attachments,
                    ..default()
                });
                pass.set_pipeline(
                    cache
                        .get_render_pipeline(pipeline)
                        .expect("grade preflight"),
                );
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            };
        if has_bloom {
            let image = |index: usize| &gpu.bloom_targets[index].1;
            for (pipeline, input, second, output) in [
                (0, source, source, image(0)),
                (1, image(0), image(0), image(1)),
                (1, image(1), image(1), image(2)),
                (2, image(2), image(2), image(3)),
                (3, image(3), image(3), image(4)),
                (4, image(1), image(4), image(5)),
                (2, image(5), image(5), image(6)),
                (3, image(6), image(6), image(7)),
            ] {
                draw_pass(gpu.bloom_pipelines[pipeline], input, second, output);
            }
        }
        draw_pass(
            gpu.pipeline,
            source,
            if has_bloom {
                &gpu.bloom_targets[7].1
            } else {
                source
            },
            target,
        );
    }
}
