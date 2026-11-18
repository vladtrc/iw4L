use super::{
    ExactColourTargetViews, ExactTessBind, GpuSubmitRefusal, PreparedExactDraw, bsp_kind_index,
    indirect,
};
use crate::drawsurf::exact_pipeline::ExactPipelineRegistry;
use crate::drawsurf::scene_depth::SceneDepthTexture;
use bevy::render::render_phase::TrackedRenderPass;
use bevy::render::render_resource::{
    BindGroup, Buffer, CommandEncoder, IndexFormat, LoadOp, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::ViewTarget;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RecordCensus {
    pub(super) group0: u32,
    pub(super) group1: u32,

    pub(super) state: u32,

    pub(super) multi_draws: u32,
    pub(super) multi_draw_commands: u32,
}

impl RecordCensus {
    pub(super) fn total(self) -> u32 {
        self.group0.saturating_add(self.group1)
    }

    pub(super) fn add(&mut self, other: Self) {
        self.group0 = self.group0.saturating_add(other.group0);
        self.group1 = self.group1.saturating_add(other.group1);
        self.state = self.state.saturating_add(other.state);
        self.multi_draws = self.multi_draws.saturating_add(other.multi_draws);
        self.multi_draw_commands = self
            .multi_draw_commands
            .saturating_add(other.multi_draw_commands);
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct RecordMesh<'a> {
    pub(super) vertex: Option<&'a Buffer>,
    pub(super) index: Option<&'a Buffer>,
    pub(super) lighting: Option<&'a Buffer>,
}

#[derive(Clone, Copy)]
pub(super) struct RecordGeometry<'a> {
    pub(super) world: RecordMesh<'a>,
    pub(super) smodel: RecordMesh<'a>,
    pub(super) cached: RecordMesh<'a>,
    pub(super) skinned: RecordMesh<'a>,
    pub(super) xmodel: RecordMesh<'a>,
    pub(super) code: RecordMesh<'a>,
    pub(super) particles: RecordMesh<'a>,
    pub(super) marks: RecordMesh<'a>,
    pub(super) glass: RecordMesh<'a>,
    pub(super) world_layer: Option<&'a Buffer>,
    pub(super) world_effect: Option<&'a Buffer>,
    pub(super) world_index_count: u32,
    pub(super) world_layout_epoch: u64,
}

impl<'a> RecordGeometry<'a> {
    fn mesh(&self, tess: ExactTessBind) -> RecordMesh<'a> {
        match tess {
            ExactTessBind::World => self.world,
            ExactTessBind::Smodel => self.smodel,
            ExactTessBind::SmodelCached => self.cached,
            ExactTessBind::SmodelSkinned => self.skinned,
            ExactTessBind::XModel => self.xmodel,
            ExactTessBind::CodeMesh => self.code,
            ExactTessBind::ParticleCloud => self.particles,
            ExactTessBind::MarkMesh => self.marks,
            ExactTessBind::Glass => self.glass,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ColourPassInputs<'a> {
    pub(super) device: &'a RenderDevice,
    pub(super) depth: &'a SceneDepthTexture,
    pub(super) effect_depth: &'a bevy::render::render_resource::TextureView,
    pub(super) viewport: [f32; 4],
    pub(super) portal_level: Option<u8>,
    pub(super) exit_clip: Option<&'a BindGroup>,
    pub(super) scissor: Option<[u32; 4]>,
    pub(super) geometry: RecordGeometry<'a>,
    pub(super) indirect_args: Option<&'a Buffer>,
    pub(super) registry: &'a ExactPipelineRegistry,
    pub(super) constants: [Option<&'a BindGroup>; super::COLOUR_PREPARE_LANES],
    pub(super) focused_object_id: Option<u16>,
}

#[derive(Default)]
pub(super) struct ColourRecordResult {
    pub(super) indexed: u32,
    pub(super) drop_ms: f32,
    pub(super) bsp_drawn_surfaces: [u32; 4],
    pub(super) bsp_draw_refused_surfaces: [u32; 4],
    pub(super) focused_drawn: u32,
    pub(super) bindings: RecordCensus,
    pub(super) refused_draws: u32,
    pub(super) encode_not_ready: u32,
    pub(super) last_refusal: Option<GpuSubmitRefusal>,
}

impl ColourRecordResult {
    pub(super) fn add(&mut self, run: Self) {
        self.indexed = self.indexed.saturating_add(run.indexed);
        self.drop_ms += run.drop_ms;
        self.focused_drawn = self.focused_drawn.saturating_add(run.focused_drawn);
        self.bindings.add(run.bindings);
        self.refused_draws = self.refused_draws.saturating_add(run.refused_draws);
        self.encode_not_ready = self.encode_not_ready.saturating_add(run.encode_not_ready);
        self.last_refusal = run.last_refusal.or(self.last_refusal.take());
        for lane in 0..3 {
            self.bsp_drawn_surfaces[lane] =
                self.bsp_drawn_surfaces[lane].saturating_add(run.bsp_drawn_surfaces[lane]);
            self.bsp_draw_refused_surfaces[lane] = self.bsp_draw_refused_surfaces[lane]
                .saturating_add(run.bsp_draw_refused_surfaces[lane]);
        }
    }
}

pub(super) fn record_colour_pass<'a>(
    input: &ColourPassInputs<'_>,
    encoder: &mut CommandEncoder,
    target: &ViewTarget,
    textures_bind: [&BindGroup; 2],
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    label: &'static str,
) -> ColourRecordResult {
    let mut draws = draws.into_iter().peekable();
    if draws.peek().is_none() {
        return ColourRecordResult::default();
    }
    let views = ExactColourTargetViews::new(target);
    let mut result = ColourRecordResult::default();
    while let Some(first) = draws.peek() {
        let srgb_write = first.state.srgb_write_enable();
        let mut ops = target.get_color_attachment().ops;
        if srgb_write && matches!(ops.load, LoadOp::Clear(_)) {
            let (view, resolve_target) = views.attachment_views(false);
            let clear_attachments = [Some(RenderPassColorAttachment {
                view,
                resolve_target: resolve_target.map(|view| &**view),
                ops,
                depth_slice: None,
            })];
            let clear_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("iw4_exact_colour_raw_clear"),
                color_attachments: &clear_attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            drop(clear_pass);
            ops.load = LoadOp::Load;
        }
        let (view, resolve_target) = views.attachment_views(srgb_write);
        let attachment = RenderPassColorAttachment {
            view,
            resolve_target: resolve_target.map(|view| &**view),
            ops,
            depth_slice: None,
        };
        let run = record_colour_run(
            input,
            encoder,
            attachment,
            textures_bind[usize::from(srgb_write)],
            std::iter::from_fn(|| {
                draws.next_if(|draw| draw.state.srgb_write_enable() == srgb_write)
            }),
            label,
        );
        result.add(run);
    }
    result
}

fn record_colour_run<'a>(
    input: &ColourPassInputs<'_>,
    encoder: &mut CommandEncoder,
    attachment: RenderPassColorAttachment<'_>,
    textures_bind: &BindGroup,
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    label: &'static str,
) -> ColourRecordResult {
    let mut result = ColourRecordResult::default();
    let attachments = [
        Some(attachment),
        Some(RenderPassColorAttachment {
            view: input.effect_depth,
            resolve_target: None,
            ops: bevy::render::render_resource::Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
            depth_slice: None,
        }),
    ];
    let mut pass = TrackedRenderPass::new(
        input.device,
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(input.depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }),
    );
    if let Some([x, y, w, h]) = input.scissor {
        pass.set_scissor_rect(x, y, w, h);
    }
    let mut bound_exit_clip = None;
    if let Some(level) = input.portal_level {
        pass.set_stencil_reference(u32::from(level) << 6);
    }
    let mut bound_pipeline = None;
    let mut bound_tess = None;
    let mut bound_smc_off = None;
    let mut bound_depth = None;
    let mut bound_arena = None;
    let mut textures_bound = false;

    let indirect_args = input.indirect_args;
    let mut batch = indirect::IndirectBatch::default();

    macro_rules! issue_indirect_batch {
        () => {
            if let Some(done) = batch.take() {
                issue_indirect(&mut pass, indirect_args, done, &mut result.bindings);
            }
        };
    }
    let [vp_x, vp_y, vp_w, vp_h] = input.viewport;

    for draw in draws {
        let Some(gpu_pipeline) = input.registry.ready(draw.pipeline) else {
            result.refused_draws = result.refused_draws.saturating_add(1);
            result.encode_not_ready = result.encode_not_ready.saturating_add(1);
            result.last_refusal = Some(GpuSubmitRefusal::PipelineNotReady);
            if draw.bsp_counted
                && let Some(kind) = draw.bsp_kind
            {
                let lane = bsp_kind_index(kind);
                result.bsp_draw_refused_surfaces[lane] = result.bsp_draw_refused_surfaces[lane]
                    .saturating_add(u32::from(draw.bsp_surf_count));
            }
            continue;
        };
        if draw.count == 0 {
            continue;
        }
        let (Some(constant_base), Some(constants_bind)) = (
            draw.first_instance(),
            input
                .constants
                .get(usize::from(draw.arena_lane))
                .copied()
                .flatten(),
        ) else {
            result.refused_draws = result.refused_draws.saturating_add(1);
            result.last_refusal = Some(GpuSubmitRefusal::ConstantArenaMissing);
            continue;
        };
        let mesh = input.geometry.mesh(draw.tess);
        let clip = draw.slice_clip.as_deref().or(input.exit_clip);
        let clip_id = clip.map(|group| group.id());
        if bound_exit_clip != clip_id {
            issue_indirect_batch!();
            if let Some(group) = clip {
                pass.set_bind_group(2, group, &[]);
            }
            bound_exit_clip = clip_id;
        }
        let (vertex, index) = (mesh.vertex, mesh.index);
        let (Some(vertex), Some(index)) = (vertex, index) else {
            result.refused_draws = result.refused_draws.saturating_add(1);
            result.last_refusal = Some(GpuSubmitRefusal::UnsupportedTessKind);
            if draw.bsp_counted
                && let Some(kind) = draw.bsp_kind
            {
                let lane = bsp_kind_index(kind);
                result.bsp_draw_refused_surfaces[lane] = result.bsp_draw_refused_surfaces[lane]
                    .saturating_add(u32::from(draw.bsp_surf_count));
            }
            continue;
        };
        if bound_pipeline != Some(draw.pipeline)
            || bound_tess != Some(draw.tess)
            || (draw.tess == ExactTessBind::SmodelCached
                && bound_smc_off != Some(draw.smc_stream_off))
        {
            issue_indirect_batch!();
            pass.set_render_pipeline(gpu_pipeline);
            let vertex_slice = if draw.tess == ExactTessBind::SmodelCached {
                let end = draw
                    .smc_stream_off
                    .saturating_add(u64::from(lighting_iw4::SMC_BANK_VB_BYTES));
                vertex.slice(draw.smc_stream_off..end)
            } else {
                vertex.slice(..)
            };
            pass.set_vertex_buffer(0, vertex_slice);
            if let Some(lighting) = mesh.lighting {
                pass.set_vertex_buffer(1, lighting.slice(..));
            }
            if draw.tess == ExactTessBind::World
                && let Some(effect) = input.geometry.world_effect
            {
                pass.set_vertex_buffer(2, effect.slice(..));
            }
            result.bindings.state = result.bindings.state.saturating_add(2);
            if draw.tess == ExactTessBind::World
                && let Some(layer) = input.geometry.world_layer
            {
                pass.set_vertex_buffer(1, layer.slice(..));
                result.bindings.state = result.bindings.state.saturating_add(1);
            }
            let index_format = if matches!(
                draw.tess,
                ExactTessBind::SmodelCached | ExactTessBind::MarkMesh
            ) {
                IndexFormat::Uint16
            } else {
                IndexFormat::Uint32
            };
            pass.set_index_buffer(index.slice(..), index_format);
            result.bindings.state = result.bindings.state.saturating_add(1);
            bound_pipeline = Some(draw.pipeline);
            bound_tess = Some(draw.tess);
            bound_smc_off = Some(draw.smc_stream_off);
        }
        if vp_w > 0.0 && vp_h > 0.0 && bound_depth != Some((draw.depth_min, draw.depth_max)) {
            issue_indirect_batch!();
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, draw.depth_min, draw.depth_max);
            result.bindings.state = result.bindings.state.saturating_add(1);
            bound_depth = Some((draw.depth_min, draw.depth_max));
        }

        if bound_arena != Some(draw.arena_lane) {
            issue_indirect_batch!();
            pass.set_bind_group(0, constants_bind, &[]);
            bound_arena = Some(draw.arena_lane);
            result.bindings.group0 = result.bindings.group0.saturating_add(1);
        }
        if !textures_bound {
            pass.set_bind_group(1, textures_bind, &[]);
            textures_bound = true;
            result.bindings.group1 = result.bindings.group1.saturating_add(1);
        }
        if draw.tess == ExactTessBind::World {
            let logical = input.geometry.world_index_count;
            let end = draw.start.saturating_add(draw.count);
            if end > logical {
                result.refused_draws = result.refused_draws.saturating_add(1);
                result.last_refusal = Some(GpuSubmitRefusal::WorldPretessSpanBeyondLimit {
                    start: draw.start,
                    count: draw.count,
                    logical_len: logical,
                    epoch: input.geometry.world_layout_epoch,
                });
                continue;
            }
        }
        match (indirect_args, draw.indirect_arg) {
            (Some(_), Some(slot)) => {
                if let Some(done) = batch.push(slot) {
                    issue_indirect(&mut pass, indirect_args, done, &mut result.bindings);
                }
            }
            _ => pass.draw_indexed(
                draw.start..draw.start.saturating_add(draw.count),
                0,
                constant_base..constant_base.saturating_add(1),
            ),
        }
        result.indexed = result.indexed.saturating_add(1);
        if draw.owner_object_id == input.focused_object_id && input.focused_object_id.is_some() {
            result.focused_drawn = result.focused_drawn.saturating_add(1);
        }
        if draw.bsp_counted
            && let Some(kind) = draw.bsp_kind
        {
            let lane = bsp_kind_index(kind);
            result.bsp_drawn_surfaces[lane] =
                result.bsp_drawn_surfaces[lane].saturating_add(u32::from(draw.bsp_surf_count));
        }
    }
    issue_indirect_batch!();
    let drop_started = Instant::now();
    drop(pass);
    result.drop_ms = drop_started.elapsed().as_secs_f32() * 1000.0;
    result
}

fn issue_indirect<'a>(
    pass: &mut TrackedRenderPass<'a>,
    args: Option<&'a Buffer>,
    batch: indirect::IndirectBatch,
    record_n: &mut RecordCensus,
) {
    let Some(args) = args else {
        return;
    };
    pass.multi_draw_indexed_indirect(
        args,
        indirect::ExactIndirectDraws::byte_offset(batch.first()),
        batch.count(),
    );
    record_n.multi_draws = record_n.multi_draws.saturating_add(1);
    record_n.multi_draw_commands = record_n.multi_draw_commands.saturating_add(batch.count());
}
