use super::encode::{RecordCensus, RecordMesh};
use super::{ExactTessBind, PreparedExactDraw};
use crate::drawsurf::backend::{D3dScissorRect, SHADOWMAP_CLEAR_Z};
use crate::drawsurf::exact_pipeline::ExactPipelineRegistry;
use crate::drawsurf::shadowmap_sun_gpu::scissor_xywh;
use bevy::prelude::LinearRgba;
use bevy::render::render_phase::TrackedRenderPass;
use bevy::render::render_resource::{
    BindGroup, Buffer, CommandEncoder, IndexFormat, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};
use bevy::render::renderer::RenderDevice;
use render_frame::SunShadowViewport;

#[derive(Clone, Copy)]
pub(super) struct ShadowGeometry<'a> {
    pub(super) world: RecordMesh<'a>,
    pub(super) smodel: RecordMesh<'a>,
    pub(super) cached: RecordMesh<'a>,
    pub(super) skinned: RecordMesh<'a>,
    pub(super) xmodel: RecordMesh<'a>,
    pub(super) smodel_index_epochs: &'a [Buffer],
    pub(super) xmodel_index_epochs: &'a [Buffer],
    pub(super) world_layer: Option<&'a Buffer>,
    pub(super) world_effect: Option<&'a Buffer>,
}

impl<'a> ShadowGeometry<'a> {
    fn mesh(&self, tess: ExactTessBind, epoch: u32) -> Option<RecordMesh<'a>> {
        let mut mesh = match tess {
            ExactTessBind::World => self.world,
            ExactTessBind::Smodel => self.smodel,
            ExactTessBind::SmodelCached => self.cached,
            ExactTessBind::SmodelSkinned => self.skinned,
            ExactTessBind::XModel => self.xmodel,
            _ => return None,
        };
        if tess == ExactTessBind::Smodel {
            mesh.index = self.smodel_index_epochs.get(epoch as usize);
        } else if tess == ExactTessBind::XModel {
            mesh.index = self.xmodel_index_epochs.get(epoch as usize);
        }
        Some(mesh)
    }
}

pub(super) struct ShadowPassInputs<'a> {
    pub(super) device: &'a RenderDevice,
    pub(super) registry: &'a ExactPipelineRegistry,
    pub(super) geometry: ShadowGeometry<'a>,
    pub(super) constants_bind: Option<&'a BindGroup>,
    pub(super) textures_bind: &'a BindGroup,
    pub(super) color_view: &'a TextureView,
    pub(super) depth_view: &'a TextureView,
    pub(super) scissor: D3dScissorRect,
    pub(super) viewport: SunShadowViewport,
    pub(super) cleared: bool,
    pub(super) tile_clear: Option<&'a bevy::render::render_resource::RenderPipeline>,
}

#[derive(Default)]
pub(super) struct ShadowRecordResult {
    pub(super) indexed: u32,
    pub(super) bindings: RecordCensus,
}

pub(super) fn record_shadowmap_draws<'a>(
    encoder: &mut CommandEncoder,
    inputs: ShadowPassInputs<'_>,
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    label: &'static str,
) -> ShadowRecordResult {
    let ShadowPassInputs {
        device,
        registry,
        geometry,
        constants_bind,
        textures_bind,
        color_view,
        depth_view,
        scissor,
        viewport,
        cleared,
        tile_clear,
    } = inputs;
    let mut result = ShadowRecordResult::default();
    let (sx, sy, sw, sh) = scissor_xywh(scissor);
    let vp = viewport;
    let attachments = [Some(RenderPassColorAttachment {
        view: color_view,
        resolve_target: None,
        ops: Operations {
            load: if cleared {
                LoadOp::Clear(LinearRgba::WHITE.into())
            } else {
                LoadOp::Load
            },
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    let mut pass = TrackedRenderPass::new(
        device,
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(Operations {
                    load: if cleared {
                        LoadOp::Clear(SHADOWMAP_CLEAR_Z)
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: if cleared {
                        LoadOp::Clear(0)
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }),
    );
    if sw > 0 && sh > 0 {
        pass.set_scissor_rect(sx, sy, sw, sh);
    }
    if let Some(pipeline) = tile_clear {
        pass.set_viewport(
            vp.x as f32,
            vp.y as f32,
            vp.width as f32,
            vp.height as f32,
            0.,
            1.,
        );
        pass.set_stencil_reference(0);
        pass.set_render_pipeline(pipeline);
        pass.draw(0..3, 0..1);
    }
    let mut record_n = RecordCensus {
        state: u32::from(sw > 0 && sh > 0),
        ..RecordCensus::default()
    };
    let mut bound_pipeline = None;
    let mut bound_tess = None;
    let mut bound_epoch = None;
    let mut bound_depth = None;
    let mut constants_bound = false;
    let mut bound_slice = None;
    let mut indexed = 0u32;
    for draw in draws {
        let Some(gpu_pipeline) = registry.ready(draw.pipeline) else {
            diag::warn_once!(World, "shadow pipeline not ready");
            continue;
        };
        let (Some(constant_base), Some(constants_bind)) = (draw.constant_base, constants_bind)
        else {
            diag::warn_once!(World, "shadow constant arena missing");
            continue;
        };
        let Some(mesh) = geometry.mesh(draw.tess, draw.ring_epoch) else {
            diag::warn_once!(World, "shadow tess kind unsupported");
            continue;
        };
        let (vertex, index) = (mesh.vertex, mesh.index);
        let (Some(vertex), Some(index)) = (vertex, index) else {
            diag::warn_once!(World, "shadow tess buffer missing");
            continue;
        };
        if bound_pipeline != Some(draw.pipeline)
            || bound_tess != Some(draw.tess)
            || bound_epoch != Some(draw.ring_epoch)
        {
            pass.set_render_pipeline(gpu_pipeline);
            pass.set_vertex_buffer(0, vertex.slice(..));
            if let Some(lighting) = mesh.lighting {
                pass.set_vertex_buffer(1, lighting.slice(..));
            }
            if draw.tess == ExactTessBind::World
                && let Some(effect) = geometry.world_effect
            {
                pass.set_vertex_buffer(2, effect.slice(..));
            }
            record_n.state = record_n.state.saturating_add(2);
            if draw.tess == ExactTessBind::World
                && let Some(layer) = geometry.world_layer
            {
                pass.set_vertex_buffer(1, layer.slice(..));
                record_n.state = record_n.state.saturating_add(1);
            }
            pass.set_index_buffer(index.slice(..), IndexFormat::Uint32);
            record_n.state = record_n.state.saturating_add(1);
            bound_pipeline = Some(draw.pipeline);
            bound_tess = Some(draw.tess);
            bound_epoch = Some(draw.ring_epoch);
        }

        if bound_depth != Some((draw.depth_min, draw.depth_max)) {
            pass.set_viewport(
                vp.x as f32,
                vp.y as f32,
                vp.width as f32,
                vp.height as f32,
                draw.depth_min,
                draw.depth_max,
            );
            bound_depth = Some((draw.depth_min, draw.depth_max));
            record_n.state = record_n.state.saturating_add(1);
        }
        let slice = draw.slice_clip.as_ref().map(|group| group.id());
        if bound_slice != slice {
            if let Some(group) = draw.slice_clip.as_deref() {
                pass.set_bind_group(2, group, &[]);
            }
            bound_slice = slice;
        }
        if !constants_bound {
            pass.set_bind_group(0, constants_bind, &[]);
            pass.set_bind_group(1, textures_bind, &[]);
            constants_bound = true;
            record_n.group0 = record_n.group0.saturating_add(1);
            record_n.group1 = record_n.group1.saturating_add(1);
        }
        pass.draw_indexed(
            draw.start..draw.start.saturating_add(draw.count),
            0,
            constant_base..constant_base.saturating_add(1),
        );
        indexed = indexed.saturating_add(1);
    }
    drop(pass);
    result.indexed = indexed;
    result.bindings = record_n;
    result
}

pub(super) fn clear_empty_shadowmap_sun_partition_zero(
    encoder: &mut CommandEncoder,
    color_view: &bevy::render::render_resource::TextureView,
    depth_view: &bevy::render::render_resource::TextureView,
) {
    let attachments = [Some(RenderPassColorAttachment {
        view: color_view,
        resolve_target: None,
        ops: Operations {
            load: LoadOp::Clear(LinearRgba::WHITE.into()),
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    let pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("iw4_shadowmap_sun_partition_zero_empty"),
        color_attachments: &attachments,
        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(Operations {
                load: LoadOp::Clear(SHADOWMAP_CLEAR_Z),
                store: StoreOp::Store,
            }),
            stencil_ops: Some(Operations {
                load: LoadOp::Clear(0),
                store: StoreOp::Store,
            }),
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    drop(pass);
}
