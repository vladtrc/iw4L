//! Bounded snapshots of completed views, reprojected from their effect depth.
use super::{Aperture, ApertureFrame, ColourPassInputs, ExactColourTargetViews, MASK_SHADER};
use bevy::math::{Mat4, Vec3};
use bevy::render::view::ViewTarget;
use std::collections::HashMap;
use wgpu::util::DeviceExt;
use wgpu::*;

const COLOUR: TextureFormat = TextureFormat::Rgba16Float;
const DEPTH: TextureFormat = TextureFormat::R32Float;
const MAX_ENTRIES: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct Camera {
    pub generation: u64,
    pub frame: u64,
    pub time: f32,
    pub clip: Mat4,
    pub eye: Vec3,
    pub viewport: [f32; 4],
    pub tint: bool,
}

struct Image {
    _colour: Texture,
    _depth: Texture,
    colour: TextureView,
    depth: TextureView,
    size: [u32; 2],
}
impl Image {
    fn new(device: &Device, size: [u32; 2]) -> Self {
        let make = |format| {
            device.create_texture(&TextureDescriptor {
                label: Some("portal snapshot"),
                size: Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let c = make(COLOUR);
        let d = make(DEPTH);
        Self {
            colour: c.create_view(&Default::default()),
            depth: d.create_view(&Default::default()),
            _colour: c,
            _depth: d,
            size,
        }
    }
    fn attachments(&self) -> [Option<RenderPassColorAttachment<'_>>; 2] {
        [
            Some(RenderPassColorAttachment {
                view: &self.colour,
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            }),
            Some(RenderPassColorAttachment {
                view: &self.depth,
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 1e7,
                        g: 0.,
                        b: 0.,
                        a: 0.,
                    }),
                    store: StoreOp::Store,
                },
            }),
        ]
    }
}
struct Entry {
    image: Image,
    camera: Camera,
    rect: [u32; 4],
}
struct Scratch {
    image: Image,
    _depth: Texture,
    depth: TextureView,
}
struct Pipeline {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
}

#[derive(Default)]
pub(crate) struct PortalFallbackGpu {
    camera: Option<Camera>,
    entries: HashMap<Vec<u32>, Entry>,
    capture: HashMap<bool, Pipeline>,
    composite: HashMap<(TextureFormat, u32), Pipeline>,
    mesh: Option<Pipeline>,
    scratch: Option<Scratch>,
    empty: Option<Image>,
    captures: u32,
    hits: u32,
    misses: u32,
    oldest_age: f32,
}

fn buffer(device: &Device, values: &[f32]) -> Buffer {
    device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("portal reprojection constants"),
        contents: bytemuck::cast_slice(values),
        usage: BufferUsages::UNIFORM,
    })
}
fn binding(binding: u32, resource: BindingResource<'_>) -> BindGroupEntry<'_> {
    BindGroupEntry { binding, resource }
}
fn texture_entry(binding: u32, multisampled: bool) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: false },
            view_dimension: TextureViewDimension::D2,
            multisampled,
        },
        count: None,
    }
}
fn uniform_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
struct FallbackPipelineDescriptor<'a> {
    source: &'a str,
    vertex_entry: &'a str,
    fragment_entry: &'a str,
    bindings: &'a [BindGroupLayoutEntry],
    targets: &'a [Option<ColorTargetState>],
    depth: Option<DepthStencilState>,
    samples: u32,
    vertex_buffers: &'a [Option<VertexBufferLayout<'a>>],
}

impl FallbackPipelineDescriptor<'_> {
    fn create(self, device: &Device) -> Pipeline {
        let Self {
            source,
            vertex_entry: vs,
            fragment_entry: fs,
            bindings: entries,
            targets,
            depth,
            samples,
            vertex_buffers: buffers,
        } = self;
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("portal fallback"),
            source: ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("portal fallback"),
            entries,
        });
        let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("portal fallback"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("portal fallback"),
            layout: Some(&pl),
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vs),
                buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fs),
                targets,
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth,
            multisample: MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        Pipeline { layout, pipeline }
    }
}

fn targets() -> [Option<ColorTargetState>; 2] {
    [
        Some(ColorTargetState {
            format: COLOUR,
            blend: None,
            write_mask: ColorWrites::ALL,
        }),
        Some(ColorTargetState {
            format: DEPTH,
            blend: None,
            write_mask: ColorWrites::RED,
        }),
    ]
}

fn capture_pipeline(device: &Device, ms: bool) -> Pipeline {
    let source=CAPTURE.replace("DEPTH_DECL",if ms {"texture_multisampled_2d<f32>"} else {"texture_2d<f32>"})
                .replace("DEPTH_LOAD",if ms {"var d=1e7; for(var i=0u;i<u32(p.settings.z);i++){d=min(d,abs(textureLoad(z,pixel,i32(i)).x));}"} else {"let d=abs(textureLoad(z,pixel,0).x);"});
    FallbackPipelineDescriptor {
        source: &source,
        vertex_entry: "vs",
        fragment_entry: "fs",
        bindings: &[
            texture_entry(0, false),
            texture_entry(1, ms),
            uniform_entry(2),
        ],
        targets: &targets(),
        depth: None,
        samples: 1,
        vertex_buffers: &[],
    }
    .create(device)
}

fn composite_pipeline(device: &Device, format: TextureFormat, samples: u32) -> Pipeline {
    let shader = format!("{MASK_SHADER}\n{COMPOSITE}");
    let attrs = [
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 16,
            shader_location: 1,
        },
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 32,
            shader_location: 2,
        },
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 48,
            shader_location: 3,
        },
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 64,
            shader_location: 4,
        },
        VertexAttribute {
            format: VertexFormat::Float32,
            offset: 80,
            shader_location: 5,
        },
    ];
    let face = StencilFaceState {
        compare: CompareFunction::Equal,
        pass_op: StencilOperation::Zero,
        fail_op: StencilOperation::Keep,
        depth_fail_op: StencilOperation::Keep,
    };
    FallbackPipelineDescriptor {
        source: &shader,
        vertex_entry: "vs",
        fragment_entry: "cached_fallback",
        bindings: &[
            texture_entry(0, false),
            texture_entry(1, false),
            uniform_entry(2),
        ],
        targets: &[
            Some(ColorTargetState {
                format,
                blend: Some(BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            }),
            Some(ColorTargetState {
                format: DEPTH,
                blend: None,
                write_mask: ColorWrites::RED,
            }),
        ],
        depth: Some(DepthStencilState {
            format: super::super::SCENE_DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Greater),
            stencil: StencilState {
                front: face,
                back: face,
                read_mask: render_frame::portal_stencil::PORTAL_LEVEL_MASK,
                write_mask: render_frame::portal_stencil::MATERIAL_STENCIL_MASK,
            },
            bias: Default::default(),
        }),
        samples,
        vertex_buffers: &[Some(VertexBufferLayout {
            array_stride: 84,
            step_mode: VertexStepMode::Vertex,
            attributes: &attrs,
        })],
    }
    .create(device)
}

fn mesh_pipeline(device: &Device) -> Pipeline {
    FallbackPipelineDescriptor {
        source: MESH,
        vertex_entry: "vs",
        fragment_entry: "fs",
        bindings: &[
            texture_entry(0, false),
            texture_entry(1, false),
            uniform_entry(2),
        ],
        targets: &targets(),
        depth: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        samples: 1,
        vertex_buffers: &[],
    }
    .create(device)
}

impl PortalFallbackGpu {
    pub(super) fn begin(&mut self, camera: Option<Camera>) {
        if self.camera.zip(camera).is_none_or(|(a, b)| {
            a.generation != b.generation || a.tint != b.tint || b.time < a.time
        }) {
            self.entries.clear();
        }
        self.camera = camera;
        self.captures = 0;
        self.hits = 0;
        self.misses = 0;
        self.oldest_age = 0.;
    }
    pub(super) fn finish(&self) {
        perf::counter!(
            "portal_fallback_snapshot_age_ms",
            "iw4l.render",
            f64::from(self.oldest_age) * 1000.
        );
        perf::counter!(
            "portal_cache_captures",
            "iw4l.render",
            f64::from(self.captures)
        );
        perf::counter!(
            "portal_fallback_cached",
            "iw4l.render",
            f64::from(self.hits)
        );
        perf::counter!(
            "portal_fallback_uncached",
            "iw4l.render",
            f64::from(self.misses)
        );
    }

    pub(super) fn capture(
        &mut self,
        input: &ColourPassInputs<'_>,
        encoder: &mut CommandEncoder,
        target: &ViewTarget,
        aperture: &Aperture,
        samples: u32,
    ) {
        let _span = perf::span!("portal_cache_capture", "iw4l.render");
        let Some(camera) = self.camera else {
            return;
        };
        if aperture.rect[2] < 8
            || aperture.rect[3] < 8
            || aperture.plane[2].abs() < 1e-6
            || !camera.clip.inverse().is_finite()
        {
            return;
        }
        let device = input.device.wgpu_device();
        let ms = samples > 1;
        let pipeline = self
            .capture
            .entry(ms)
            .or_insert_with(|| capture_pipeline(device, ms));
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&aperture.key) {
            if let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.camera.frame)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&key);
            }
        }
        let size = [
            aperture.rect[2].min(256).div_ceil(32) * 32,
            aperture.rect[3].min(256).div_ceil(32) * 32,
        ];
        let entry = self
            .entries
            .entry(aperture.key.clone())
            .or_insert_with(|| Entry {
                image: Image::new(device, size),
                camera,
                rect: aperture.rect,
            });
        if entry.image.size[0] < size[0] || entry.image.size[1] < size[1] {
            entry.image = Image::new(
                device,
                [
                    entry.image.size[0].max(size[0]),
                    entry.image.size[1].max(size[1]),
                ],
            );
        }
        let size = entry.image.size;
        entry.camera = camera;
        entry.rect = aperture.rect;
        let mut values = camera.clip.inverse().to_cols_array().to_vec();
        values.extend(camera.viewport);
        values.extend(aperture.rect.map(|v| v as f32));
        values.extend(aperture.plane);
        values.extend([size[0] as f32, size[1] as f32, samples as f32, 0.]);
        let constants = buffer(device, &values);
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("portal snapshot"),
            layout: &pipeline.layout,
            entries: &[
                binding(0, BindingResource::TextureView(target.main_texture_view())),
                binding(1, BindingResource::TextureView(input.effect_depth)),
                binding(2, constants.as_entire_binding()),
            ],
        });
        let attachments = entry.image.attachments();
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("capture completed portal view"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        self.captures += 1;
    }

    pub(super) fn fallback(
        &mut self,
        input: &ColourPassInputs<'_>,
        encoder: &mut CommandEncoder,
        scene: &super::PortalScene<'_, '_>,
        masks: &ApertureFrame,
        aperture: &Aperture,
        level: u8,
    ) {
        let _span = perf::span!("portal_fallback_reproject", "iw4l.render");
        let Some(camera) = self.camera else {
            return;
        };
        let target = scene.target;
        let samples = scene.samples;
        let device = input.device.wgpu_device();
        let entry = self
            .entries
            .get(&aperture.key)
            .filter(|e| camera.eye.distance(e.camera.eye) <= 256.);
        if self.empty.is_none() {
            let empty = Image::new(device, [1, 1]);
            let attachments = empty.attachments();
            drop(encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("empty portal history"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            self.empty = Some(empty);
        }
        if entry.is_some() && self.scratch.is_none() {
            let size = [512, 512];
            let depth = device.create_texture(&TextureDescriptor {
                label: Some("portal reprojection visibility"),
                size: Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.scratch = Some(Scratch {
                image: Image::new(device, size),
                depth: depth.create_view(&Default::default()),
                _depth: depth,
            });
        }
        let output = if let Some(entry) = entry {
            let scratch = self.scratch.as_ref().unwrap();
            let attachments = scratch.image.attachments();
            let pipeline = self.mesh.get_or_insert_with(|| mesh_pipeline(device));
            let mut values = entry.camera.clip.inverse().to_cols_array().to_vec();
            values.extend(camera.clip.to_cols_array());
            values.extend(entry.camera.viewport);
            values.extend(entry.rect.map(|v| v as f32));
            values.extend(camera.viewport);
            values.extend(aperture.rect.map(|v| v as f32));
            let fade = ((256. - camera.eye.distance(entry.camera.eye)) / 64.).clamp(0., 1.);
            values.extend([
                entry.image.size[0] as f32,
                entry.image.size[1] as f32,
                fade,
                0.,
            ]);
            let constants = buffer(device, &values);
            let group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("portal reprojection"),
                layout: &pipeline.layout,
                entries: &[
                    binding(0, BindingResource::TextureView(&entry.image.colour)),
                    binding(1, BindingResource::TextureView(&entry.image.depth)),
                    binding(2, constants.as_entire_binding()),
                ],
            });
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("reproject cached portal geometry"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &scratch.depth,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(0.),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(
                0..((entry.image.size[0] - 1).div_ceil(2)
                    * (entry.image.size[1] - 1).div_ceil(2)
                    * 6),
                0..1,
            );
            drop(pass);
            self.hits += 1;
            self.oldest_age = self
                .oldest_age
                .max((camera.time - entry.camera.time).max(0.));
            &scratch.image
        } else {
            self.misses += 1;
            self.empty.as_ref().unwrap()
        };
        let format = target.main_texture_format();
        let pipeline = self
            .composite
            .entry((format, samples))
            .or_insert_with(|| composite_pipeline(device, format, samples));
        let inverse = camera.clip.inverse().to_cols_array_2d();
        let mut values = aperture.rect.map(|v| v as f32).to_vec();
        values.extend([inverse[0][3], inverse[1][3], inverse[2][3], inverse[3][3]]);
        let constants = buffer(device, &values);
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("cached portal boundary"),
            layout: &pipeline.layout,
            entries: &[
                binding(0, BindingResource::TextureView(&output.colour)),
                binding(1, BindingResource::TextureView(&output.depth)),
                binding(2, constants.as_entire_binding()),
            ],
        });
        let views = ExactColourTargetViews::new(target);
        let (view, resolve) = views.attachment_views(false);
        let attachments = [
            Some(RenderPassColorAttachment {
                view,
                resolve_target: resolve.map(|v| &**v),
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            }),
            Some(RenderPassColorAttachment {
                view: input.effect_depth,
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            }),
        ];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("cached or translucent portal boundary"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(input.depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y, w, h] = input.viewport;
        pass.set_viewport(x, y, w, h, 0., 1.);
        let [x, y, w, h] = aperture.rect;
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_stencil_reference(u32::from(level) << 6);
        pass.set_vertex_buffer(0, *masks.vertices.slice(..));
        pass.draw(aperture.start..aperture.start + aperture.count, 0..1);
    }
}

const CAPTURE: &str = r#"
struct Params { inverse:mat4x4<f32>,viewport:vec4<f32>,rect:vec4<f32>,plane:vec4<f32>,settings:vec4<f32> }
@group(0) @binding(0) var c:texture_2d<f32>;
@group(0) @binding(1) var z:DEPTH_DECL;
@group(0) @binding(2) var<uniform> p:Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let a=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(a[i],0.,1.);
}
struct O { @location(0) colour:vec4<f32>, @location(1) depth:f32 }
@fragment fn fs(@builtin(position) at:vec4<f32>)->O {
    let xy=p.rect.xy+at.xy/p.settings.xy*p.rect.zw;
    let pixel=clamp(vec2<i32>(xy),vec2(0),vec2<i32>(textureDimensions(c))-1);
    DEPTH_LOAD
    let ndc=(xy-p.viewport.xy)/p.viewport.zw*vec2(2.,-2.)+vec2(-1.,1.);
    let row=vec4(p.inverse[0].w,p.inverse[1].w,p.inverse[2].w,p.inverse[3].w);
    let dz=(1.-dot(row.xy,ndc*d)-row.w*d)/row.z/max(d,0.00001);
    let plane=-(dot(p.plane.xy,ndc)+p.plane.w)/p.plane.z;
    if d!=d || d<=0. || dz>plane+0.00001 {return O(vec4(0.),1e7);}
    return O(vec4(textureLoad(c,pixel,0).rgb,1.),d);
}
"#;

const MESH: &str = r#"
struct Params { inverse:mat4x4<f32>,current:mat4x4<f32>,viewport:vec4<f32>,rect:vec4<f32>,new_viewport:vec4<f32>,new_rect:vec4<f32>,settings:vec4<f32> }
@group(0) @binding(0) var c:texture_2d<f32>;
@group(0) @binding(1) var z:texture_2d<f32>;
@group(0) @binding(2) var<uniform> p:Params;
struct V { @builtin(position) pos:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) depth:f32 }
fn point(pixel:vec2<u32>)->vec4<f32> {
    let d=min(textureLoad(z,vec2<i32>(pixel),0).x,1e5);
    let xy=p.rect.xy+(vec2<f32>(pixel)+0.5)/p.settings.xy*p.rect.zw;
    let ndc=(xy-p.viewport.xy)/p.viewport.zw*vec2(2.,-2.)+vec2(-1.,1.);
    let row=vec4(p.inverse[0].w,p.inverse[1].w,p.inverse[2].w,p.inverse[3].w);
    let clip=vec4(ndc*d,(1.-dot(row.xy,ndc*d)-row.w*d)/row.z,d);
    return p.current*(p.inverse*clip);
}
@vertex fn vs(@builtin(vertex_index) i:u32)->V {
    let size=vec2<u32>(p.settings.xy);let grid=(size-1u+1u)/2u;
    let offset=array<vec2<u32>,6>(vec2(0u,0u),vec2(1u,0u),vec2(0u,1u),vec2(1u,0u),vec2(1u,1u),vec2(0u,1u));
    let cell=vec2((i/6u)%grid.x,(i/6u)/grid.x)*2u;
    let tri=(i%6u)/3u*3u;
    var lo=1e7;var hi=0.;var valid=true;
    for(var k=0u;k<3u;k++) {
        let at=min(cell+offset[tri+k]*2u,size-1u);
        let depth=textureLoad(z,vec2<i32>(at),0).x;
        lo=min(lo,depth);hi=max(hi,depth);
        valid=valid && textureLoad(c,vec2<i32>(at),0).a>0.5;
    }
    let at=min(cell+offset[i%6u]*2u,size-1u);let clip=point(at);
    let pixel=p.new_viewport.xy+(clip.xy/clip.w*vec2(0.5,-0.5)+0.5)*p.new_viewport.zw;
    let ndc=(pixel-p.new_rect.xy)/p.new_rect.zw*vec2(2.,-2.)+vec2(-1.,1.);
    var pos=vec4(ndc*clip.w,clip.z*0.984375,clip.w);
    if !valid || hi-lo>max(128.,lo*0.25) {pos=vec4(2.,2.,0.,1.);}
    let effect=select(clip.w,1e7,textureLoad(z,vec2<i32>(at),0).x>1e6);
    return V(pos,(vec2<f32>(at)+0.5)/p.settings.xy,effect);
}
struct O { @location(0) colour:vec4<f32>,@location(1) depth:f32 }
@fragment fn fs(v:V)->O {
    let at=clamp(vec2<i32>(v.uv*p.settings.xy),vec2(0),vec2<i32>(p.settings.xy)-1);
    let colour=textureLoad(c,at,0);if colour.a<0.5 {discard;}
    return O(vec4(colour.rgb,colour.a*p.settings.z),v.depth);
}
"#;

const COMPOSITE: &str = r#"
@group(0) @binding(0) var cached:texture_2d<f32>;
@group(0) @binding(1) var cached_depth:texture_2d<f32>;
struct Boundary { rect:vec4<f32>,inverse_w:vec4<f32> }
@group(0) @binding(2) var<uniform> boundary:Boundary;
@fragment fn cached_fallback(v:V)->Fill {
    let size=vec2<i32>(textureDimensions(cached));
    let at=clamp(vec2<i32>((v.p.xy-boundary.rect.xy)/boundary.rect.zw*vec2<f32>(size)),vec2(0),size-1);
    var c=textureLoad(cached,at,0);var z=textureLoad(cached_depth,at,0).x;
    let ndc=(v.p.xy-v.viewport.xy)/v.viewport.zw*vec2(2.,-2.)+vec2(-1.,1.);
    let row=boundary.inverse_w;
    let depth=(1.-dot(row.xy,ndc*z)-row.w*z)/row.z/max(z,0.00001)*0.984375;
    if z<=0. || z!=z || depth>sealed_depth(v)+0.00001 {c=vec4(0.);z=1e7;}
    let alpha=c.a+v.colour.a*(1.-c.a);
    let rgb=(c.rgb*c.a+v.colour.rgb*v.colour.a*(1.-c.a))/max(alpha,0.00001);
    let tinted=mix(rgb,v.tint.rgb,v.tint.a);
    return Fill(vec4(tinted,alpha),sealed_depth(v),select(1e7,z,c.a>0.05));
}
"#;
