//! Native aperture rendering. Colour-format changes and scene/depth resolves
//! retain the same depth/stencil attachment across render-pass boundaries.
use super::encode::{ColourPassInputs, ColourRecordResult, record_colour_pass};
use super::{ExactColourTargetViews, PreparedExactDraw};

use bevy::render::diagnostic::{DiagnosticsRecorder, RecordDiagnostics};
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::view::ViewTarget;
use render_frame::portal_stencil::{PORTAL_LEVEL_MASK, PortalStencilTransition};
use std::sync::Arc;
mod fallback;
mod occlusion;
pub(crate) use fallback::PortalFallbackGpu;
pub(crate) use occlusion::PortalQueryGpu;

pub fn exit_clip_mode(device: &RenderDevice) -> d3d9_sm3::ExitClipMode {
    static FORCE_DISCARD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let discard =
        *FORCE_DISCARD.get_or_init(|| std::env::var_os("IW4L_PORTAL_EXIT_DISCARD").is_some());
    if !discard && device.features().contains(WgpuFeatures::CLIP_DISTANCES) {
        d3d9_sm3::ExitClipMode::ClipDistance
    } else {
        d3d9_sm3::ExitClipMode::FragmentDiscard
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExitClass {
    Inside,
    Outside,
    Intersect,
}

pub(super) fn classify_sphere(
    plane: [f64; 4],
    sphere: Option<([f32; 3], f32)>,
    pose: bevy::math::Mat4,
) -> ExitClass {
    let Some((center, radius)) = sphere else {
        return ExitClass::Intersect;
    };
    let pose = pose.as_dmat4();
    let center = pose.transform_point3(bevy::math::DVec3::from_array(center.map(f64::from)));
    // Frobenius norm bounds any scale/shear, so a bound cannot be made too
    // small by a placement transform. Unknown/non-finite bounds stay clipped.
    let radius = f64::from(radius)
        * (pose.x_axis.truncate().length_squared()
            + pose.y_axis.truncate().length_squared()
            + pose.z_axis.truncate().length_squared())
        .sqrt();
    let p = bevy::math::DVec4::from_array(plane);
    let distance = p.dot(center.extend(1.0));
    let extent = radius * p.truncate().length() + 0.05;
    if !distance.is_finite() || !extent.is_finite() || radius < 0.0 {
        ExitClass::Intersect
    } else if distance > extent {
        ExitClass::Inside
    } else if distance < -extent {
        ExitClass::Outside
    } else {
        ExitClass::Intersect
    }
}

pub(super) fn exit_class(
    extracted: super::ExtractedColourRefs<'_>,
    tag: u8,
    kind: &render_frame::RetainedDrawKind,
) -> ExitClass {
    let Some(plane) = tag
        .checked_sub(1)
        .and_then(|view| extracted.frame.exec_frame.world_cameras.get(view as usize))
        .and_then(Option::as_ref)
        .and_then(|c| c.exit_plane_world)
    else {
        return ExitClass::Inside;
    };
    let geometry = &extracted.world.static_geometry;
    match *kind {
        render_frame::RetainedDrawKind::World {
            surf,
            run,
            run_off,
            world_from_local,
            ..
        } => {
            let mut common = None;
            for offset in 0..usize::from(run.max(1)) {
                let surf = if run > 1 {
                    extracted
                        .frame
                        .frame_products
                        .0
                        .world_run_surfs
                        .get(run_off as usize + offset)
                        .copied()
                } else {
                    Some(surf)
                };
                let class = classify_sphere(
                    plane,
                    surf.and_then(|s| geometry.world_bounds.get(s as usize))
                        .copied()
                        .flatten(),
                    world_from_local,
                );
                if class == ExitClass::Intersect || common.is_some_and(|c| c != class) {
                    return ExitClass::Intersect;
                }
                common = Some(class);
            }
            common.unwrap_or(ExitClass::Intersect)
        }
        render_frame::RetainedDrawKind::Smodel {
            surface,
            owner_world_from_local,
            ..
        } => classify_sphere(
            plane,
            geometry
                .smodel_bounds
                .get(surface as usize)
                .copied()
                .flatten(),
            owner_world_from_local,
        ),
        // Animated/effect geometry without a conservative placement bound must
        // retain per-fragment clipping, never be culled from its origin alone.
        _ => ExitClass::Intersect,
    }
}

pub(super) fn exit_plane_layout(device: &RenderDevice) -> BindGroupLayout {
    device.create_bind_group_layout(
        Some("portal exit plane"),
        &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(112),
            },
            count: None,
        }],
    )
}

pub(super) fn clip_planes_group(device: &RenderDevice, planes: [[f32; 4]; 7]) -> BindGroup {
    let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("portal clip planes"),
        contents: bytemuck::cast_slice(&planes),
        usage: BufferUsages::UNIFORM,
    });
    device.create_bind_group(
        Some("portal clip planes"),
        &exit_plane_layout(device),
        &[BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    )
}

pub(super) fn entity_slice_planes(
    frame: &render_frame::MaterialExecFrame,
    tag: u8,
    slice: [f64; 4],
    aperture: Option<render_frame::PortalEntityAperture>,
) -> Option<[[f32; 4]; 7]> {
    let view = usize::from(tag.checked_sub(1)?);
    // The root deliberately has no WorldCamera: it reads the frame's native
    // camera. A missing child camera must not silently use that root matrix.
    let (clip, exit, exit_world, eye) =
        if let Some(camera) = frame.world_cameras.get(view).and_then(Option::as_ref) {
            (
                camera.clip_from_world,
                camera.exit_plane_clip,
                camera.exit_plane_world,
                camera.view_origin.to_array(),
            )
        } else if frame.portal.as_ref()?.camera_world == view {
            (
                frame.clip_from_world?,
                None,
                None,
                frame.view_origin.to_array(),
            )
        } else {
            return None;
        };
    let hidden = slice_hidden_in_view(slice, exit_world, eye);
    Some(entity_clip_uniform(clip, exit, slice, aperture, hidden))
}

pub(super) fn simple_clip_planes(planes: [[f32; 4]; 2]) -> [[f32; 4]; 7] {
    let mut out = [[0., 0., 0., 1.]; 7];
    out[0] = planes[0];
    out[1] = planes[1];
    out[6] = [0.; 4];
    out
}

pub(super) fn entity_clip_uniform(
    clip: bevy::math::Mat4,
    exit: Option<[f32; 4]>,
    slice: [f64; 4],
    aperture: Option<render_frame::PortalEntityAperture>,
    hidden: bool,
) -> [[f32; 4]; 7] {
    let matrix = clip.as_dmat4().inverse().transpose();
    let project = |plane| {
        (matrix * bevy::math::DVec4::from_array(plane))
            .as_vec4()
            .to_array()
    };
    let mut out = simple_clip_planes([exit.unwrap_or([0., 0., 0., 1.]), project(slice)]);
    if let Some(aperture) = aperture {
        for (slot, plane) in out[2..6].iter_mut().zip(aperture.planes) {
            *slot = project(plane);
        }
        out[6] = [
            if aperture.is_clone { 2. } else { 1. },
            f32::from(hidden),
            0.,
            0.,
        ];
    } else if hidden {
        out[1] = [0., 0., 0., -1.];
    }
    out
}

// Exit and traveller slices overlap slightly to avoid seams. Opposite halves
// of the same exit must not leave that overlap as a visible strip. A cut seen
// from its discarded side belongs to the next portal's child scene, except
// when the current exit explicitly opens that same retained half-space.
fn slice_hidden_in_view(slice: [f64; 4], exit: Option<[f64; 4]>, eye: [f32; 3]) -> bool {
    let slice = bevy::math::DVec4::from_array(slice);
    let matching = exit.is_some_and(|exit| {
        let exit = bevy::math::DVec4::from_array(exit);
        (exit.truncate() - slice.truncate()).length() < 1e-5 && (exit.w - slice.w).abs() < 0.05
    });
    let opposing = exit.is_some_and(|exit| {
        let exit = bevy::math::DVec4::from_array(exit);
        (exit.truncate() + slice.truncate()).length() < 1e-5 && (exit.w + slice.w).abs() < 0.05
    });
    opposing
        || (!matching
            && slice.dot(bevy::math::DVec3::from_array(eye.map(f64::from)).extend(1.)) < 0.)
}

const MASK_SHADER: &str = r#"
struct V {
    @builtin(position) p:vec4<f32>, @location(0) colour:vec4<f32>,
    @interpolate(flat) @location(1) plane:vec4<f32>,
    @interpolate(flat) @location(2) viewport:vec4<f32>,
    @interpolate(flat) @location(3) tint:vec4<f32>,
    @interpolate(flat) @location(4) blocked:f32,
}
@vertex fn vs(@location(0) p:vec4<f32>, @location(1) colour:vec4<f32>,
    @location(2) plane:vec4<f32>, @location(3) viewport:vec4<f32>, @location(4) tint:vec4<f32>, @location(5) blocked:f32) -> V {
    return V(p,colour,plane,viewport,tint,blocked);
}
struct Fill { @location(0) colour:vec4<f32>, @builtin(frag_depth) depth:f32, @location(1) effect_depth:f32 }
fn sealed_depth(v:V) -> f32 {
    let xy=vec2((v.p.x-v.viewport.x)/v.viewport.z*2.0-1.0,
        1.0-(v.p.y-v.viewport.y)/v.viewport.w*2.0);
    var depth=v.p.z;
    if abs(v.plane.z)>0.000001 { depth=-(dot(v.plane.xy,xy)+v.plane.w)/v.plane.z; }
    return clamp(depth,0.0,0.984375);
}
@fragment fn seal(v:V) -> Fill { return Fill(v.colour,sealed_depth(v),0.0); }
@fragment fn terminal(v:V) -> Fill {
    return Fill(mix(v.colour,vec4(v.tint.rgb,1.0),v.tint.a),sealed_depth(v),
        select(1.0e7,1.0/max(v.p.w,1.0e-7),v.blocked>0.5));
}
@fragment fn fill(v:V) -> Fill { return Fill(v.colour,0.0,select(1.0e7,1.0/max(v.p.w,1.0e-7),v.blocked>0.5)); }
@fragment fn tint(v:V) -> Fill { return Fill(v.tint,v.p.z,0.0); }
"#;

pub(crate) struct PortalMaskPipelines {
    query: RenderPipeline,
    terminal: RenderPipeline,
    mark: [RenderPipeline; 3],
    fill: RenderPipeline,
    restore: RenderPipeline,
    unmark: [RenderPipeline; 3],
    tint: RenderPipeline,
}

impl PortalMaskPipelines {
    pub(crate) fn new(device: &RenderDevice, format: TextureFormat, samples: u32) -> Self {
        let shader = device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("native portal aperture"),
            source: ShaderSource::Wgsl(MASK_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("native portal aperture"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let make = |label, write_mask, op, depth_compare, depth_write, mode| {
            let face = StencilFaceState {
                compare: CompareFunction::Equal,
                pass_op: op,
                fail_op: StencilOperation::Keep,
                depth_fail_op: StencilOperation::Keep,
            };
            device.create_render_pipeline(&RawRenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: RawVertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[Some(RawVertexBufferLayout {
                        array_stride: 84,
                        step_mode: VertexStepMode::Vertex,
                        attributes: &[
                            VertexAttribute {
                                format: VertexFormat::Float32,
                                offset: 80,
                                shader_location: 5,
                            },
                            VertexAttribute {
                                format: VertexFormat::Float32x4,
                                offset: 64,
                                shader_location: 4,
                            },
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
                        ],
                    })],
                    compilation_options: Default::default(),
                },
                fragment: Some(RawFragmentState {
                    module: &shader,
                    entry_point: Some(match mode {
                        1 => "fill",
                        2 => "tint",
                        3 => "terminal",
                        _ => "seal",
                    }),
                    targets: &[
                        Some(ColorTargetState {
                            format,
                            blend: (mode == 2).then_some(BlendState::ALPHA_BLENDING),
                            write_mask: if mode != 0 {
                                ColorWrites::ALL
                            } else {
                                ColorWrites::empty()
                            },
                        }),
                        Some(ColorTargetState {
                            format: super::floatz::EFFECT_DEPTH_FORMAT,
                            blend: None,
                            write_mask: if mode == 1 || mode == 3 {
                                ColorWrites::RED
                            } else {
                                ColorWrites::empty()
                            },
                        }),
                    ],
                    compilation_options: Default::default(),
                }),
                primitive: PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(DepthStencilState {
                    format: super::SCENE_DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(depth_compare),
                    stencil: StencilState {
                        front: face,
                        back: face,
                        read_mask: PORTAL_LEVEL_MASK,
                        write_mask,
                    },
                    bias: Default::default(),
                }),
                multisample: MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            terminal: make(
                "portal terminal fallback",
                render_frame::portal_stencil::MATERIAL_STENCIL_MASK,
                StencilOperation::Zero,
                CompareFunction::GreaterEqual,
                true,
                3,
            ),
            query: make(
                "portal visibility query",
                0,
                StencilOperation::Keep,
                CompareFunction::GreaterEqual,
                false,
                0,
            ),
            mark: std::array::from_fn(|level| {
                make(
                    "portal mark",
                    PortalStencilTransition::between(level as u8, level as u8 + 1)
                        .unwrap()
                        .invert_mask,
                    StencilOperation::Invert,
                    CompareFunction::GreaterEqual,
                    false,
                    0,
                )
            }),
            fill: make(
                "portal fog and depth fill",
                0x3f,
                StencilOperation::Zero,
                CompareFunction::Always,
                true,
                1,
            ),
            restore: make(
                "portal parent depth seal",
                render_frame::portal_stencil::MATERIAL_STENCIL_MASK,
                StencilOperation::Zero,
                CompareFunction::Always,
                true,
                0,
            ),
            unmark: std::array::from_fn(|level| {
                make(
                    "portal unmark",
                    PortalStencilTransition::between(level as u8 + 1, level as u8)
                        .unwrap()
                        .invert_mask,
                    StencilOperation::Invert,
                    CompareFunction::Always,
                    false,
                    0,
                )
            }),
            tint: make(
                "portal level tint",
                0,
                StencilOperation::Keep,
                CompareFunction::Always,
                false,
                2,
            ),
        }
    }
}

struct Aperture {
    parent: usize,
    child: usize,
    start: u32,
    count: u32,
    tint_start: u32,
    rect: [u32; 4],
    fallback: bool,
    occluded: bool,
    query_index: u32,
    key: Vec<u32>,
    plane: [f32; 4],
    blocked: bool,
}

struct ApertureFrame {
    vertices: Buffer,
    apertures: Vec<Aperture>,
    pipelines: Arc<PortalMaskPipelines>,
    tint: bool,
}

impl ApertureFrame {
    fn query(
        &self,
        input: &ColourPassInputs<'_>,
        encoder: &mut CommandEncoder,
        target: &ViewTarget,
        apertures: &[&Aperture],
        level: u8,
        queries: &occlusion::QueryBatch,
    ) {
        if apertures.is_empty() {
            return;
        }
        let views = ExactColourTargetViews::new(target);
        let (view, resolve) = views.attachment_views(false);
        let attachments = [
            Some(RenderPassColorAttachment {
                view,
                resolve_target: resolve.map(|v| &**v),
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            }),
            Some(RenderPassColorAttachment {
                view: input.effect_depth,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            }),
        ];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("portal visibility probes"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(input.depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: Some(&queries.queries),
            multiview_mask: None,
        });
        let [x, y, w, h] = input.viewport;
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_pipeline(&self.pipelines.query);
        pass.set_stencil_reference(u32::from(level) << 6);
        pass.set_vertex_buffer(0, *self.vertices.slice(..));
        for aperture in apertures {
            let [x, y, w, h] = aperture.rect;
            pass.set_scissor_rect(x, y, w, h);
            pass.begin_occlusion_query(aperture.query_index);
            pass.draw(aperture.start..aperture.start + aperture.count, 0..1);
            pass.end_occlusion_query();
        }
    }

    fn new(
        input: &ColourPassInputs<'_>,
        target: &ViewTarget,
        frame: &render_frame::MaterialExecFrame,
        samples: u32,
    ) -> Option<Self> {
        let view = frame.portal.as_ref()?;
        let plan = view.plan.as_ref()?;
        if plan.settings.only.is_some() {
            return None;
        }
        let [vx, vy, vw, vh] = input.viewport;
        let mut vertices = Vec::<[f32; 21]>::new();
        let mut apertures = Vec::new();
        let scene_max = 1.0 - render_backend::DEPTH_RANGE_BAND;
        for aperture in &plan.apertures {
            let projection = aperture.projection;
            let blocked = f32::from(
                aperture.fallback == Some(render_frame::portal_plan::PortalFallback::Blocked),
            );
            let mut plane = projection.plane;
            plane[2] /= scene_max;
            let mut tint = match aperture.depth {
                1 => [0.1, 1.0, 0.25, 0.22],
                2 => [0.1, 0.35, 1.0, 0.22],
                _ => [1.0, 0.15, 0.8, 0.22],
            };
            if !plan.settings.tint {
                tint[3] = 0.;
            }
            let mut fog =
                if aperture.fallback == Some(render_frame::portal_plan::PortalFallback::Blocked) {
                    // A solid closed panel, never a fog-filled view of the exit.
                    [0.16, 0.13, 0.10, 1.]
                } else {
                    fog_for_view(frame, view, aperture.child)
                };
            if aperture.fallback.is_some() && blocked == 0.0 {
                let opening = view.openings[aperture.opening];
                let delta = frame.view_origin - bevy::math::Vec3::from_array(opening.base);
                let across = bevy::math::Vec3::from_array(opening.across);
                let up = bevy::math::Vec3::from_array(opening.up);
                let nearest = across
                    * delta
                        .dot(across)
                        .clamp(-opening.width * 0.5, opening.width * 0.5)
                    + up * delta.dot(up).clamp(0., opening.height);
                fog[3] = (1. - (-(delta - nearest).length() / 768.).exp()).clamp(0.1, 0.85);
            }
            let start = vertices.len() as u32;
            let indices: &[usize] = if projection.corners[..4] == projection.corners[4..] {
                &[0, 1, 2, 0, 2, 3]
            } else {
                &[
                    0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6,
                    7, 2, 7, 3, 3, 7, 4, 3, 4, 0,
                ]
            };
            for &index in indices {
                let mut p = projection.corners[index];
                p[2] *= scene_max;
                vertices.push([
                    p[0], p[1], p[2], p[3], fog[0], fog[1], fog[2], 1.0, plane[0], plane[1],
                    plane[2], plane[3], vx, vy, vw, vh, tint[0], tint[1], tint[2], tint[3],
                    blocked,
                ]);
            }
            let tint_start = vertices.len() as u32;
            for p in [
                [-1.0, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ] {
                vertices.push([
                    p[0], p[1], 0.0, 1.0, fog[0], fog[1], fog[2], 1.0, plane[0], plane[1],
                    plane[2], plane[3], vx, vy, vw, vh, tint[0], tint[1], tint[2], tint[3],
                    blocked,
                ]);
            }
            apertures.push(Aperture {
                parent: aperture.parent,
                child: aperture.child,
                start,
                count: indices.len() as u32,
                tint_start,
                rect: projection.rect,
                fallback: aperture.fallback.is_some(),
                occluded: aperture.fallback
                    == Some(render_frame::portal_plan::PortalFallback::Occluded),
                query_index: apertures.len() as u32,
                key: view.query_keys[aperture.opening].clone(),
                plane: aperture.projection.plane,
                blocked: blocked > 0.,
            });
        }
        let vertices = input.device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("portal aperture vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        Some(Self {
            vertices,
            apertures,
            tint: plan.settings.tint,
            pipelines: input.registry.portal_masks(
                input.device,
                target.main_texture_format(),
                samples,
            ),
        })
    }

    fn mark(
        &self,
        input: &ColourPassInputs<'_>,
        encoder: &mut CommandEncoder,
        target: &ViewTarget,
        aperture: &Aperture,
        level: u8,
        enter: bool,
    ) {
        let views = ExactColourTargetViews::new(target);
        let (view, resolve) = views.attachment_views(false);
        let attachments = [
            Some(RenderPassColorAttachment {
                view,
                resolve_target: resolve.map(|v| &**v),
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            }),
            Some(RenderPassColorAttachment {
                view: input.effect_depth,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            }),
        ];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(if enter {
                "portal mark/fill"
            } else {
                "portal restore/unmark"
            }),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(input.depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y, w, h] = input.viewport;
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        let [x, y, w, h] = aperture.rect;
        pass.set_scissor_rect(x, y, w, h);
        pass.set_vertex_buffer(0, *self.vertices.slice(..));
        let range = aperture.start..aperture.start + aperture.count;
        if enter && level == 3 {
            // The deepest accepted view already uses level 3. Paint only the
            // remaining boundary; no child exists and no level 4 is needed.
            pass.set_stencil_reference(u32::from(level) << 6);
            pass.set_pipeline(&self.pipelines.terminal);
            pass.draw(range, 0..1);
            return;
        }
        if enter {
            let transition = PortalStencilTransition::between(level, level + 1).unwrap();
            pass.set_stencil_reference(transition.reference);
            pass.set_pipeline(&self.pipelines.mark[level as usize]);
            pass.draw(range.clone(), 0..1);
            pass.set_stencil_reference(u32::from(level + 1) << 6);
            pass.set_pipeline(&self.pipelines.fill);
            pass.draw(range, 0..1);
        } else {
            if self.tint {
                pass.set_stencil_reference(u32::from(level + 1) << 6);
                pass.set_pipeline(&self.pipelines.tint);
                pass.draw(aperture.tint_start..aperture.tint_start + 6, 0..1);
            }
            let transition = PortalStencilTransition::between(level + 1, level).unwrap();
            pass.set_stencil_reference(transition.reference);
            pass.set_pipeline(&self.pipelines.restore);
            pass.draw(range.clone(), 0..1);
            pass.set_pipeline(&self.pipelines.unmark[level as usize]);
            pass.draw(range, 0..1);
        }
    }
}

fn fog_for_view(
    frame: &render_frame::MaterialExecFrame,
    view: &render_frame::PortalView,
    child: usize,
) -> [f32; 4] {
    view.worlds
        .get(child)
        .and_then(|world| frame.world_fog_colors.get(*world))
        .copied()
        .unwrap_or([0.35, 0.4, 0.5, 1.0])
}

pub(super) struct PortalScene<'a, 'input> {
    pub(super) input: &'a ColourPassInputs<'input>,
    pub(super) target: &'a ViewTarget,
    pub(super) binds: [&'a BindGroup; 4],
    pub(super) draws: &'a [PreparedExactDraw],
    pub(super) samples: u32,
    pub(super) diagnostics: Option<&'a DiagnosticsRecorder>,
}

impl PortalScene<'_, '_> {
    pub(super) fn record(
        self,
        encoder: &mut CommandEncoder,
        frame: &render_frame::MaterialExecFrame,
        resolve: &mut impl FnMut(&mut CommandEncoder) -> bool,
    ) -> ColourRecordResult {
        let input = self.input;
        let target = self.target;
        let samples = self.samples;
        let masks = ApertureFrame::new(input, target, frame, samples);
        let mut history = input
            .registry
            .portal_fallback
            .lock()
            .expect("portal image history");
        history.begin(frame.portal.as_ref().and_then(|v| {
            Some(fallback::Camera {
                generation: v.occlusion.as_ref()?.generation,
                frame: v.occlusion.as_ref()?.frame,
                time: frame.float_time,
                clip: frame.clip_from_world?,
                eye: frame.view_origin,
                viewport: input.viewport,
                tint: v.plan.as_ref()?.settings.tint,
            })
        }));
        let keys = frame
            .portal
            .as_ref()
            .and_then(|v| {
                v.plan.as_ref().map(|p| {
                    p.apertures
                        .iter()
                        .map(|a| v.query_keys[a.opening].clone())
                        .collect()
                })
            })
            .unwrap_or(Vec::new());
        let queries = input
            .registry
            .portal_queries
            .lock()
            .expect("portal query ring")
            .begin(
                input.device,
                frame.portal.as_ref().and_then(|v| v.occlusion.as_ref()),
                keys,
            );
        let exit_groups: Vec<_> = frame
            .world_cameras
            .iter()
            .map(|camera| {
                let plane = camera.as_ref()?.exit_plane_clip?;
                let buffer = input.device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("portal exit plane"),
                    contents: bytemuck::cast_slice(&simple_clip_planes([
                        plane,
                        [0.0, 0.0, 0.0, 1.0],
                    ])),
                    usage: BufferUsages::UNIFORM,
                });
                Some(input.device.create_bind_group(
                    Some("portal exit plane"),
                    &exit_plane_layout(input.device),
                    &[BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                ))
            })
            .collect();
        if frame.portal.is_some() {
            static LOG: std::sync::Once = std::sync::Once::new();
            LOG.call_once(|| {
                diag::info!(
                    World,
                    "portal exit clipping: {:?}",
                    exit_clip_mode(input.device)
                )
            });
        }
        let root = frame
            .portal
            .as_ref()
            .map(|p| p.plan.as_ref().map_or(p.camera_world, |plan| plan.root));
        let result = PortalViewRecorder {
            scene: &self,
            encoder,
            masks: masks.as_ref(),
            queries: queries.as_ref(),
            history: &mut history,
            exit_groups: &exit_groups,
            root,
            camera_overlay: root == frame.portal.as_ref().map(|p| p.camera_world),
            resolve,
        }
        .render_view(input, root, 0);
        if let Some(queries) = queries {
            input
                .registry
                .portal_queries
                .lock()
                .expect("portal query ring")
                .finish(encoder, queries);
        } else {
            perf::counter!("portal_query_submitted", "iw4l.render", 0.0);
        }
        history.finish();
        result
    }
}

struct PortalViewRecorder<'r, 'a, 'input, F> {
    scene: &'r PortalScene<'a, 'input>,
    encoder: &'r mut CommandEncoder,
    masks: Option<&'r ApertureFrame>,
    queries: Option<&'r occlusion::QueryBatch>,
    history: &'r mut PortalFallbackGpu,
    exit_groups: &'r [Option<BindGroup>],
    root: Option<usize>,
    camera_overlay: bool,
    resolve: &'r mut F,
}

impl<F: FnMut(&mut CommandEncoder) -> bool> PortalViewRecorder<'_, '_, '_, F> {
    fn render_view(
        &mut self,
        input: &ColourPassInputs<'_>,
        view: Option<usize>,
        level: u8,
    ) -> ColourRecordResult {
        let target = self.scene.target;
        let binds = self.scene.binds;
        let draws = self.scene.draws;
        let samples = self.scene.samples;
        let diagnostics = self.scene.diagnostics;
        let masks = self.masks;
        let queries = self.queries;
        let exit_groups = self.exit_groups;
        let root = self.root;
        let camera_overlay = self.camera_overlay;
        let mut input = *input;
        input.portal_level = view.map(|_| level);
        input.exit_clip = view
            .and_then(|view| exit_groups.get(view))
            .and_then(Option::as_ref);
        let accepts = |draw: &&PreparedExactDraw| match view {
            Some(view) => {
                !viewmodel_overlay(draw)
                    && (draw.portal_tag as usize == view + 1
                        || (camera_overlay && Some(view) == root && draw.portal_tag == 0))
            }
            None => true,
        };
        let tag = view.unwrap_or(0);
        let opaque_span = diagnostics.time_span(self.encoder, format!("portal_v{tag}_opaque"));
        let mut result = record_colour_pass(
            &input,
            self.encoder,
            target,
            [binds[0], binds[1]],
            draws
                .iter()
                .filter(accepts)
                .filter(|d| !d.after_scene_resolve && (view.is_none() || !blended(d))),
            "portal view opaque",
        );
        opaque_span.end(self.encoder);
        if let (Some(masks), Some(view)) = (masks, view) {
            let children: Vec<_> = masks
                .apertures
                .iter()
                .filter(|a| a.parent == view)
                .collect();
            if let Some(queries) = queries {
                masks.query(&input, self.encoder, target, &children, level, queries);
            }
            for aperture in children {
                if aperture.occluded {
                    continue;
                }
                if aperture.fallback && !aperture.blocked {
                    let span = diagnostics.time_span(
                        self.encoder,
                        format!("portal_v{}_cached_fallback", aperture.child),
                    );
                    self.history
                        .fallback(&input, self.encoder, self.scene, masks, aperture, level);
                    span.end(self.encoder);
                    continue;
                }
                if level == 3 {
                    assert!(
                        aperture.fallback,
                        "only a boundary may follow portal depth 3"
                    );
                    masks.mark(&input, self.encoder, target, aperture, level, true);
                    continue;
                }
                assert!(level < 3, "portal plan exceeds stencil level capacity");
                let mark_span =
                    diagnostics.time_span(self.encoder, format!("portal_v{}_mark", aperture.child));
                masks.mark(&input, self.encoder, target, aperture, level, true);
                mark_span.end(self.encoder);
                if !aperture.fallback {
                    let mut child_input = input;
                    child_input.scissor = Some(aperture.rect);
                    result.add(self.render_view(&child_input, Some(aperture.child), level + 1));
                    let span = diagnostics.time_span(
                        self.encoder,
                        format!("portal_v{}_cache_capture", aperture.child),
                    );
                    self.history
                        .capture(&input, self.encoder, target, aperture, samples);
                    span.end(self.encoder);
                }
                let restore_span = diagnostics
                    .time_span(self.encoder, format!("portal_v{}_restore", aperture.child));
                masks.mark(&input, self.encoder, target, aperture, level, false);
                restore_span.end(self.encoder);
            }
        }
        // Blended parent surfaces must see the completed children. Otherwise
        // aperture fill erases their contribution in the portal region. They use
        // the unresolved tables; only scene/FloatZ readers need the resolve below.
        if view.is_some()
            && draws
                .iter()
                .filter(accepts)
                .any(|d| !d.after_scene_resolve && blended(d))
        {
            let span = diagnostics.time_span(self.encoder, format!("portal_v{tag}_translucent"));
            result.add(record_colour_pass(
                &input,
                self.encoder,
                target,
                [binds[0], binds[1]],
                draws
                    .iter()
                    .filter(accepts)
                    .filter(|d| !d.after_scene_resolve && blended(d)),
                "portal view translucent",
            ));
            span.end(self.encoder);
        }
        let mut resolve_view = |encoder: &mut CommandEncoder| {
            let span = diagnostics.time_span(encoder, format!("portal_v{tag}_resolve"));
            let ready = (self.resolve)(encoder);
            span.end(encoder);
            if !ready {
                diag::warn!(
                    World,
                    "portal late draw resolve refused: view={tag} level={level}"
                );
            }
            ready
        };
        if draws.iter().filter(accepts).any(|d| d.after_scene_resolve) && resolve_view(self.encoder)
        {
            let span = diagnostics.time_span(self.encoder, format!("portal_v{tag}_late"));
            let emissive_span = diagnostics.time_span(
                self.encoder,
                crate::diag::render_frame_diag::GPU_SPAN_EMISSIVE,
            );
            result.add(record_colour_pass(
                &input,
                self.encoder,
                target,
                [binds[2], binds[3]],
                draws
                    .iter()
                    .filter(accepts)
                    .filter(|d| d.after_scene_resolve),
                "portal view late",
            ));
            emissive_span.end(self.encoder);
            span.end(self.encoder);
        }
        if camera_overlay && view.is_some() && view == root {
            // Viewmodels belong to the physical camera, never to an unfolded view.
            // Draw them after sealing portals, including when the aperture cuts near.
            result.add(record_colour_pass(
                &input,
                self.encoder,
                target,
                [binds[0], binds[1]],
                draws
                    .iter()
                    .filter(|d| viewmodel_overlay(d) && !d.after_scene_resolve),
                "portal camera overlay",
            ));
            if draws
                .iter()
                .any(|d| viewmodel_overlay(d) && d.after_scene_resolve)
                && resolve_view(self.encoder)
            {
                result.add(record_colour_pass(
                    &input,
                    self.encoder,
                    target,
                    [binds[2], binds[3]],
                    draws
                        .iter()
                        .filter(|d| viewmodel_overlay(d) && d.after_scene_resolve),
                    "portal camera overlay late",
                ));
            }
        }

        result
    }
}

fn blended(draw: &PreparedExactDraw) -> bool {
    draw.state
        .apply_change_state_0_host(false)
        .blend
        .blend_state()
        .is_some()
}

fn viewmodel_overlay(draw: &PreparedExactDraw) -> bool {
    draw.depth_min >= 1.0 - render_backend::DEPTH_RANGE_BAND
}

/// Combine the level test with the material states used by exact colour passes.
/// A reference of zero for the material bits matches the existing renderer.
pub(super) fn material_stencil(bits: u32) -> StencilState {
    let mut state = crate::drawsurf::scene_depth::stencil_state(bits);
    let material = bits & 0x40 != 0;
    let equal = material
        && (state.front.compare == CompareFunction::Equal
            || state.back.compare == CompareFunction::Equal);
    for face in [&mut state.front, &mut state.back] {
        if !matches!(
            face.compare,
            CompareFunction::Always | CompareFunction::Equal | CompareFunction::Never
        ) || face.fail_op != StencilOperation::Keep
        {
            diag::warn_once!(
                World,
                "portal material stencil composition requires review: bits={bits:#x} face={face:?}"
            );
        }
        face.compare = if face.compare == CompareFunction::Never && material {
            CompareFunction::Never
        } else {
            CompareFunction::Equal
        };
        // A material failure must never mutate pixels in an ancestor region.
        face.fail_op = StencilOperation::Keep;
    }
    state.read_mask = if equal { 0xff } else { PORTAL_LEVEL_MASK };
    state.write_mask = if material { 0x3f } else { 0 };
    state
}
