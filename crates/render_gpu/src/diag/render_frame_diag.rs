use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore};
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::pipelined_rendering::RenderExtractApp;
use bevy::render::renderer::{PendingCommandBuffers, RenderGraph, RenderGraphSystems};
use bevy::render::{Render, RenderApp, RenderSystems};

const RENDER_FRAME_LOG_EVERY: u32 = 64;

#[derive(Clone, Default)]
pub(crate) struct RenderFrameSample {
    /// The frame that was open when this render frame's extract began — the
    /// frame whose wall the render work ran inside. The main world reads the
    /// finished sample a frame or more later, so without this the stage
    /// timings land on whichever row happened to be open when they arrived.
    origin_frame: u64,
    /// When this render frame's extract began: the moment the state it
    /// presents was taken out of the main world, with the main world stalled
    /// so nothing moved between the two.
    origin_extracted_at: Option<Instant>,
    /// How old that state was when the render graph finished with it, and how
    /// many main frames had been opened since it was taken. Serialised
    /// rendering answers zero frames behind; a pipelined render world answers
    /// one or more, which is the cost the throughput result is paid for in.
    ///
    /// It ends where the graph does. Bevy presents after the `RenderGraph`
    /// schedule returns, so the present call is already outside this, and the
    /// compositor and the display are outside the process entirely: it is not
    /// input-to-photon and must never be read as it.
    pub(crate) presented_state_age_ms: Option<f32>,
    pub(crate) presented_frames_behind: Option<u32>,
    /// Which render frame this is. The main world reads the published sample
    /// whether or not a new one was published, and emitting the same
    /// measurement again would count one piece of work twice.
    sequence: u64,
    extract_ms: Option<f32>,
    thread_closed: bool,
    extract_commands_ms: Option<f32>,
    prepare_assets_ms: Option<f32>,
    prepare_meshes_ms: Option<f32>,
    create_views_ms: Option<f32>,
    specialize_ms: Option<f32>,
    prepare_views_ms: Option<f32>,
    queue_ms: Option<f32>,
    phase_sort_ms: Option<f32>,
    prepare_ms: Option<f32>,
    render_ms: Option<f32>,
    pub(crate) submit_prepare_ms: Option<f32>,

    pub(crate) colour_submit_ms: Option<f32>,

    pub(crate) submit_encode_ms: Option<f32>,

    pub(crate) submit_gather_ms: Option<f32>,
    pub(crate) pack_intern_hit_n: Option<u32>,
    pub(crate) pack_intern_miss_n: Option<u32>,
    pub(crate) pack_arena_share_n: Option<u32>,
    pub(crate) gpu_exec_reuse_n: Option<u32>,
    pub(crate) gpu_exec_unique_n: Option<u32>,
    pub(crate) pack_overlay_n: Option<u32>,
    pub(crate) pack_overlay_row_n: Option<u32>,
    pub(crate) pack_overlay_pixel_share_n: Option<u32>,
    pub(crate) pack_arena_vertex_n: Option<u32>,
    pub(crate) pack_arena_pixel_n: Option<u32>,
    pub(crate) pack_seed_n: Option<u32>,
    pub(crate) pack_walk_n: Option<u32>,
    pub(crate) tex_bind_hit_n: Option<u32>,
    pub(crate) tex_bind_miss_n: Option<u32>,

    pub(crate) pass_end_ms: Option<f32>,
    pub(crate) encoder_finish_ms: Option<f32>,
    pub(crate) submit_arena_ms: Option<f32>,
    pub(crate) submit_record_ms: Option<f32>,

    pub(crate) diag_ms: Option<f32>,

    pub(crate) diag_pass: Option<u32>,

    pub(crate) diag_draw_n: Option<u32>,

    pub(crate) diag_world_n: Option<u32>,

    pub(crate) diag_smodel_n: Option<u32>,

    pub(crate) diag_xmodel_n: Option<u32>,

    pub(crate) graph_render_ms: Option<f32>,

    /// The wall across bevy's whole submit set, not a `Queue::submit` body.
    pub(crate) graph_submit_ms: Option<f32>,
    pub(crate) graph_submit_pending_n: Option<u32>,

    pub(crate) graph_present_ms: Option<f32>,
    pub(crate) wgpu_floor_encode_ms: Option<f32>,
    pub(crate) wgpu_floor_finish_ms: Option<f32>,
    pub(crate) wgpu_floor_submit_ms: Option<f32>,
    pub(crate) wgpu_floor_encode_1bind_ms: Option<f32>,
    pub(crate) wgpu_floor_n: Option<u32>,
    pub(crate) wgpu_floor_binds: Option<u32>,

    pub(crate) markmesh_hits: Option<u32>,

    pub(crate) markmesh_prepared: Option<u32>,

    pub(crate) last_markmesh_refusal: Option<String>,

    pub(crate) last_markmesh_exec_skip: Option<String>,

    pub(crate) markmesh_missing_58: Option<u32>,
    pub(crate) last_mark_packed_custom: Option<u8>,
    pub(crate) last_mark_packed_scene_light: Option<u8>,
    pub(crate) last_mark_lmap_sampler: Option<u32>,
    pub(crate) glassmesh_hits: Option<u32>,
    pub(crate) glassmesh_prepared: Option<u32>,
    pub(crate) last_glassmesh_exec_skip: Option<String>,
    pub(crate) last_glassmesh_refusal: Option<String>,
    pub(crate) last_glass_packed_probe: Option<u8>,
    pub(crate) last_glass_probe_sampler: Option<u32>,
    pub(crate) gpu_ready: Option<u32>,

    pub(crate) set_bind_group_n: Option<u32>,

    pub(crate) gpu_prepared: Option<u32>,
    pub(crate) gpu_world_ready: Option<u32>,
    pub(crate) bsp_submitted_surfaces: [u32; 4],
    pub(crate) bsp_submit_refused_surfaces: [u32; 4],
    pub(crate) bsp_drawn_surfaces: [u32; 4],
    pub(crate) bsp_draw_refused_surfaces: [u32; 4],
    pub(crate) gpu_smodel_ready: Option<u32>,
    pub(crate) gpu_xmodel_ready: Option<u32>,
    pub(crate) end_depth_restore_n: Option<u32>,
    pub(crate) end_depth_range_type: Option<i32>,
    pub(crate) code_mesh_gpu_kind: Option<i32>,
    pub(crate) tess_stream_bind_n: Option<u32>,
    pub(crate) tess_stream_skip_n: Option<u32>,
    pub(crate) sun_shadow_gpu: Option<u32>,
    pub(crate) sun_shadow_gpu_miss: Option<u32>,
    pub(crate) sun_shadow_gpu_cause: Option<String>,
    pub(crate) sun_shadow_gpu_causes: Option<String>,
    pub(crate) spot_shadow_gpu: Option<u32>,
    pub(crate) spot_shadow_gpu_miss: Option<u32>,
    pub(crate) spot_shadow_gpu_cause: Option<String>,
    pub(crate) spot_shadow_slot_n: Option<u32>,
    pub(crate) sun_shadow_submit_ms: Option<f32>,
    pub(crate) sun_shadow_prepare_ms: Option<f32>,
    pub(crate) sun_shadow_patch_ms: Option<f32>,
    pub(crate) sun_shadow_arena_ms: Option<f32>,
    pub(crate) sun_shadow_record_ms: Option<f32>,
    pub(crate) sun_shadow_finish_ms: Option<f32>,
    pub(crate) sun_shadow_queue_ms: Option<f32>,
    pub(crate) sun_shadow_unnamed_ms: Option<f32>,
    pub(crate) sun_shadow_static_hit: Option<u32>,
    pub(crate) sun_shadow_world_ib_n: Option<u32>,
    pub(crate) sun_shadow_static_n: Option<u32>,
    pub(crate) sun_shadow_dynamic_n: Option<u32>,
    pub(crate) sun_shadow_wvp_intern_hit: Option<u32>,
    pub(crate) sun_shadow_wvp_intern_miss: Option<u32>,
    pub(crate) sun_shadow_wvp_intern_n: Option<u32>,
    pub(crate) sun_shadow_wvp_unique_base: Option<u32>,
    pub(crate) sun_shadow_wvp_unique_wvp: Option<u32>,
    pub(crate) sun_shadow_state_pipe_n: Option<u32>,
    pub(crate) sun_shadow_state_tess_n: Option<u32>,
    pub(crate) sun_shadow_state_bind_n: Option<u32>,
    pub(crate) sun_shadow_state_off_n: Option<u32>,
    pub(crate) sun_shadow_state_group_n: Option<u32>,
    pub(crate) sun_shadow_state_run_n: Option<u32>,
    pub(crate) sun_shadow_state_run_max: Option<u32>,
    pub(crate) sun_shadow_state_top10: Option<u32>,
    pub(crate) world_index_gaps: Option<u32>,
    pub(crate) world_run_indices_n: Option<u32>,
    pub(crate) world_material_runs: Option<u32>,
    pub(crate) world_material_runs_seq: Option<u32>,
    pub(crate) world_key_runs: Option<u32>,
    pub(crate) world_key_runs_seq: Option<u32>,
    pub(crate) world_mixed_breaks: Option<u32>,
    pub(crate) world_gathered: Option<u32>,

    pub(crate) world_ib_skip: Option<u32>,
    pub(crate) world_gpu_runs: Option<u32>,
    pub(crate) world_gpu_runs_seq: Option<u32>,
    pub(crate) world_sampler_runs_seq: Option<u32>,
    pub(crate) world_probe_runs_seq: Option<u32>,
    pub(crate) world_light_runs_seq: Option<u32>,
    pub(crate) smodel_reuse_n: Option<u32>,
    pub(crate) xmodel_reuse_n: Option<u32>,
    pub(crate) xmodel_material_runs: Option<u32>,
    pub(crate) smodel_index_gaps: Option<u32>,
    pub(crate) smodel_material_runs: Option<u32>,
    pub(crate) smodel_material_runs_seq: Option<u32>,
    pub(crate) smodel_material_run_max: Option<u32>,
    pub(crate) smodel_same_surface_n: Option<u32>,
    pub(crate) smodel_unique_surfaces: Option<u32>,
    pub(crate) smodel_hits: Option<u32>,
    pub(crate) smodel_lighting_runs: Option<u32>,
    pub(crate) smodel_lighting_run_max: Option<u32>,
    pub(crate) smodel_pretess_runs: Option<u32>,
    pub(crate) smodel_pretess_hits: Option<u32>,
    pub(crate) smodel_pretess_verts: Option<u32>,
    pub(crate) smodel_pretess_indices: Option<u32>,
    pub(crate) smodel_cached_lighting: Option<u32>,
    pub(crate) smodel_pretess_local: Option<u32>,
    pub(crate) smodel_pretess_length1: Option<u32>,
    pub(crate) smodel_pretess_skip: Option<u32>,
    pub(crate) submit_cause: Option<String>,
    pub(crate) submit_cause2: Option<String>,
    pub(crate) gpu_not_ready_n: Option<u32>,
    pub(crate) gpu_no_port_n: Option<u32>,
    pub(crate) pnr_smodel_mat: Option<String>,
    pub(crate) pnr_world_mat: Option<String>,
    pub(crate) pnr_smodel_ps: Option<String>,
    pub(crate) pnr_world_ps: Option<String>,
    pub(crate) pnr_smodel_key_n: Option<u32>,
    pub(crate) pnr_world_key_n: Option<u32>,
    pub(crate) pnr_port_n: Option<u32>,
    pub(crate) gpu_smodel_bind_mat: Option<String>,
    pub(crate) extract_ports_ms: Option<f32>,
    pub(crate) extract_tess_ms: Option<f32>,
    pub(crate) extract_products_ms: Option<f32>,
    pub(crate) extract_images_ms: Option<f32>,
    pub(crate) extract_diag_ms: Option<f32>,
    pub(crate) extract_world_skip: Option<u32>,
    pub(crate) extract_world_clone_bytes: Option<u64>,
    pub(crate) extract_xmodel_clone_bytes: Option<u64>,
    pub(crate) extract_xmodel_arc: Option<u32>,
    pub(crate) extract_fx_arc: Option<u32>,
    pub(crate) extract_fx_clone_bytes: Option<u64>,

    pub(crate) extract_products_arc: Option<u32>,

    pub(crate) extract_images_arc: Option<u32>,

    pub(crate) extract_products_bank_new: Option<u32>,
}

#[derive(Default)]
pub(crate) struct SharedRenderStages {
    working: RenderFrameSample,
    completed: Option<RenderFrameSample>,
    /// The sequence of the last sample whose counters were emitted.
    consumed: u64,
}

impl SharedRenderStages {
    fn begin_render_frame(&mut self) {
        let sequence = self.working.sequence + 1;
        let origin_frame = perf::frames::open_index();
        let floor = (
            self.working.wgpu_floor_encode_ms,
            self.working.wgpu_floor_finish_ms,
            self.working.wgpu_floor_submit_ms,
            self.working.wgpu_floor_encode_1bind_ms,
            self.working.wgpu_floor_n,
            self.working.wgpu_floor_binds,
        );
        self.working = RenderFrameSample::default();
        self.working.sequence = sequence;
        self.working.origin_frame = origin_frame;
        self.working.origin_extracted_at = Some(Instant::now());
        self.working.wgpu_floor_encode_ms = floor.0;
        self.working.wgpu_floor_finish_ms = floor.1;
        self.working.wgpu_floor_submit_ms = floor.2;
        self.working.wgpu_floor_encode_1bind_ms = floor.3;
        self.working.wgpu_floor_n = floor.4;
        self.working.wgpu_floor_binds = floor.5;
    }

    fn publish_render_frame(&mut self) {
        self.completed = Some(self.working.clone());
    }
}

impl std::ops::Deref for SharedRenderStages {
    type Target = RenderFrameSample;

    fn deref(&self) -> &Self::Target {
        &self.working
    }
}

impl std::ops::DerefMut for SharedRenderStages {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.working
    }
}

#[derive(Resource, Clone)]
pub struct SharedRenderStagesSlot(pub(crate) Arc<Mutex<SharedRenderStages>>);

impl Default for SharedRenderStagesSlot {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(SharedRenderStages::default())))
    }
}

impl SharedRenderStagesSlot {
    pub fn stamp_extract_colour(
        &self,
        ports_ms: f32,
        tess_ms: f32,
        world_skip: u32,
        world_clone_bytes: u64,
        xmodel_clone_bytes: u64,
        xmodel_arc: u32,
        fx_arc: u32,
        fx_clone_bytes: u64,
    ) {
        if let Ok(mut guard) = self.0.lock() {
            guard.extract_ports_ms = Some(ports_ms);
            guard.extract_tess_ms = Some(tess_ms);
            guard.extract_world_skip = Some(world_skip);
            guard.extract_world_clone_bytes = Some(world_clone_bytes);
            guard.extract_xmodel_clone_bytes = Some(xmodel_clone_bytes);
            guard.extract_xmodel_arc = Some(xmodel_arc);
            guard.extract_fx_arc = Some(fx_arc);
            guard.extract_fx_clone_bytes = Some(fx_clone_bytes);
        }
    }

    pub fn stamp_extract_products(&self, ms: f32, arc: u32, bank_new: u32) {
        if let Ok(mut guard) = self.0.lock() {
            guard.extract_products_ms = Some(ms);
            guard.extract_products_arc = Some(arc);
            guard.extract_products_bank_new = Some(bank_new);
        }
    }

    pub fn stamp_extract_images(&self, ms: f32, arc: u32) {
        if let Ok(mut guard) = self.0.lock() {
            guard.extract_images_ms = Some(ms);
            guard.extract_images_arc = Some(arc);
        }
    }

    pub fn stamp_extract_diag(&self, ms: f32) {
        if let Ok(mut guard) = self.0.lock() {
            guard.extract_diag_ms = Some(ms);
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct RenderFrameDiag {
    pub render_extract_ms: Option<f32>,

    pub render_extract_commands_ms: Option<f32>,

    pub render_prepare_assets_ms: Option<f32>,

    pub render_prepare_meshes_ms: Option<f32>,

    pub render_create_views_ms: Option<f32>,

    pub render_specialize_ms: Option<f32>,

    pub render_prepare_views_ms: Option<f32>,

    pub render_queue_ms: Option<f32>,

    pub render_phase_sort_ms: Option<f32>,

    pub render_prepare_ms: Option<f32>,

    pub render_render_ms: Option<f32>,

    pub present_mode: Option<String>,

    pub submit_prepare_ms: Option<f32>,

    pub colour_submit_ms: Option<f32>,

    pub submit_encode_ms: Option<f32>,

    pub submit_gather_ms: Option<f32>,

    pub pass_end_ms: Option<f32>,

    pub encoder_finish_ms: Option<f32>,

    pub submit_arena_ms: Option<f32>,

    pub submit_record_ms: Option<f32>,

    pub diag_ms: Option<f32>,

    pub diag_pass: Option<u32>,

    pub diag_draw_n: Option<u32>,

    pub diag_world_n: Option<u32>,

    pub diag_smodel_n: Option<u32>,

    pub diag_xmodel_n: Option<u32>,

    pub graph_render_ms: Option<f32>,

    pub graph_submit_ms: Option<f32>,
    pub graph_submit_pending_n: Option<u32>,

    pub graph_present_ms: Option<f32>,
    /// How old the drawn state was when the render graph finished with it, and
    /// how many main frames the main world had opened since it was extracted.
    pub presented_state_age_ms: Option<f32>,
    pub presented_frames_behind: Option<u32>,

    pub wgpu_floor_encode_ms: Option<f32>,
    pub wgpu_floor_finish_ms: Option<f32>,
    pub wgpu_floor_submit_ms: Option<f32>,
    pub wgpu_floor_encode_1bind_ms: Option<f32>,
    pub wgpu_floor_n: Option<u32>,
    pub wgpu_floor_binds: Option<u32>,

    pub pack_intern_hit_n: Option<u32>,

    pub pack_intern_miss_n: Option<u32>,

    pub pack_arena_share_n: Option<u32>,

    pub gpu_exec_reuse_n: Option<u32>,

    pub gpu_exec_unique_n: Option<u32>,

    pub pack_overlay_n: Option<u32>,
    pub pack_overlay_row_n: Option<u32>,
    pub pack_overlay_pixel_share_n: Option<u32>,
    pub pack_arena_vertex_n: Option<u32>,
    pub pack_arena_pixel_n: Option<u32>,

    pub pack_seed_n: Option<u32>,

    pub pack_walk_n: Option<u32>,

    pub tex_bind_hit_n: Option<u32>,

    pub tex_bind_miss_n: Option<u32>,

    pub markmesh_hits: Option<u32>,

    pub markmesh_prepared: Option<u32>,

    pub last_markmesh_refusal: Option<String>,

    pub last_markmesh_exec_skip: Option<String>,

    pub markmesh_missing_58: Option<u32>,

    pub last_mark_packed_custom: Option<u8>,

    pub last_mark_packed_scene_light: Option<u8>,

    pub last_mark_lmap_sampler: Option<u32>,

    pub glassmesh_hits: Option<u32>,

    pub glassmesh_prepared: Option<u32>,

    pub last_glassmesh_exec_skip: Option<String>,

    pub last_glassmesh_refusal: Option<String>,

    pub last_glass_packed_probe: Option<u8>,

    pub last_glass_probe_sampler: Option<u32>,

    pub gpu_ready: Option<u32>,

    pub set_bind_group_n: Option<u32>,

    pub gpu_prepared: Option<u32>,

    pub gpu_world_ready: Option<u32>,

    pub bsp_submitted_surfaces: [u32; 4],

    pub bsp_submit_refused_surfaces: [u32; 4],

    pub bsp_drawn_surfaces: [u32; 4],

    pub bsp_draw_refused_surfaces: [u32; 4],

    pub gpu_smodel_ready: Option<u32>,

    pub gpu_xmodel_ready: Option<u32>,

    pub end_depth_restore_n: Option<u32>,

    pub end_depth_range_type: Option<i32>,

    pub code_mesh_gpu_kind: Option<i32>,

    pub tess_stream_bind_n: Option<u32>,

    pub tess_stream_skip_n: Option<u32>,

    pub sun_shadow_gpu: Option<u32>,

    pub sun_shadow_gpu_miss: Option<u32>,

    pub sun_shadow_gpu_cause: Option<String>,

    pub sun_shadow_gpu_causes: Option<String>,

    pub spot_shadow_gpu: Option<u32>,
    pub spot_shadow_gpu_miss: Option<u32>,
    pub spot_shadow_gpu_cause: Option<String>,
    pub spot_shadow_slot_n: Option<u32>,

    pub sun_shadow_submit_ms: Option<f32>,

    pub sun_shadow_prepare_ms: Option<f32>,

    pub sun_shadow_patch_ms: Option<f32>,

    pub sun_shadow_arena_ms: Option<f32>,

    pub sun_shadow_record_ms: Option<f32>,

    pub sun_shadow_finish_ms: Option<f32>,

    pub sun_shadow_queue_ms: Option<f32>,

    pub sun_shadow_unnamed_ms: Option<f32>,

    pub sun_shadow_static_hit: Option<u32>,
    pub sun_shadow_world_ib_n: Option<u32>,

    pub sun_shadow_static_n: Option<u32>,

    pub sun_shadow_dynamic_n: Option<u32>,

    pub sun_shadow_wvp_intern_hit: Option<u32>,

    pub sun_shadow_wvp_intern_miss: Option<u32>,

    pub sun_shadow_wvp_intern_n: Option<u32>,

    pub sun_shadow_wvp_unique_base: Option<u32>,

    pub sun_shadow_wvp_unique_wvp: Option<u32>,

    pub sun_shadow_state_pipe_n: Option<u32>,

    pub sun_shadow_state_tess_n: Option<u32>,

    pub sun_shadow_state_bind_n: Option<u32>,

    pub sun_shadow_state_off_n: Option<u32>,

    pub sun_shadow_state_group_n: Option<u32>,

    pub sun_shadow_state_run_n: Option<u32>,

    pub sun_shadow_state_run_max: Option<u32>,

    pub sun_shadow_state_top10: Option<u32>,

    pub world_index_gaps: Option<u32>,

    pub world_run_indices_n: Option<u32>,

    pub world_material_runs: Option<u32>,

    pub world_material_runs_seq: Option<u32>,

    pub world_key_runs: Option<u32>,

    pub world_key_runs_seq: Option<u32>,

    pub world_mixed_breaks: Option<u32>,
    pub world_gathered: Option<u32>,

    pub world_ib_skip: Option<u32>,

    pub world_gpu_runs: Option<u32>,

    pub world_gpu_runs_seq: Option<u32>,

    pub world_sampler_runs_seq: Option<u32>,

    pub world_probe_runs_seq: Option<u32>,

    pub world_light_runs_seq: Option<u32>,

    pub smodel_reuse_n: Option<u32>,

    pub xmodel_reuse_n: Option<u32>,

    pub xmodel_material_runs: Option<u32>,

    pub smodel_index_gaps: Option<u32>,

    pub smodel_material_runs: Option<u32>,

    pub smodel_material_runs_seq: Option<u32>,

    pub smodel_material_run_max: Option<u32>,

    pub smodel_same_surface_n: Option<u32>,

    pub smodel_unique_surfaces: Option<u32>,

    pub smodel_hits: Option<u32>,

    pub smodel_lighting_runs: Option<u32>,

    pub smodel_lighting_run_max: Option<u32>,

    pub smodel_pretess_runs: Option<u32>,

    pub smodel_pretess_hits: Option<u32>,

    pub smodel_pretess_verts: Option<u32>,

    pub smodel_pretess_indices: Option<u32>,

    pub smodel_cached_lighting: Option<u32>,

    pub smodel_pretess_local: Option<u32>,

    pub smodel_pretess_length1: Option<u32>,

    pub smodel_pretess_skip: Option<u32>,

    pub submit_cause: Option<String>,

    pub submit_cause2: Option<String>,

    pub gpu_not_ready_n: Option<u32>,

    pub gpu_no_port_n: Option<u32>,

    pub pnr_smodel_mat: Option<String>,

    pub pnr_world_mat: Option<String>,

    pub pnr_smodel_ps: Option<String>,

    pub pnr_world_ps: Option<String>,

    pub pnr_smodel_key_n: Option<u32>,

    pub pnr_world_key_n: Option<u32>,

    pub pnr_port_n: Option<u32>,

    pub gpu_smodel_bind_mat: Option<String>,

    pub extract_ports_ms: Option<f32>,

    pub extract_tess_ms: Option<f32>,

    pub extract_products_ms: Option<f32>,

    pub extract_images_ms: Option<f32>,

    pub extract_diag_ms: Option<f32>,

    pub extract_world_skip: Option<u32>,

    pub extract_world_clone_bytes: Option<u64>,

    pub extract_xmodel_clone_bytes: Option<u64>,

    pub extract_xmodel_arc: Option<u32>,

    pub extract_fx_arc: Option<u32>,

    pub extract_fx_clone_bytes: Option<u64>,

    pub extract_products_arc: Option<u32>,

    pub extract_images_arc: Option<u32>,

    pub extract_products_bank_new: Option<u32>,

    pub gpu_frame_ms: Option<f32>,

    pub gpu_opaque_ms: Option<f32>,

    pub cpu_graph_ms: Option<f32>,

    pub cpu_opaque_ms: Option<f32>,

    pub cpu_present_ms: Option<f32>,

    pub cpu_graph_top: Option<String>,

    pub drawn_entities: Option<u32>,

    pub drawn_index_count: Option<u64>,
}

macro_rules! stage_timer {
    ($timer:ident, $start:ident, $end:ident, $field:ident) => {
        #[derive(Resource, Default)]
        struct $timer(Option<Instant>);

        fn $start(mut timer: ResMut<$timer>) {
            timer.0 = Some(Instant::now());
        }

        fn $end(timer: Res<$timer>, slot: Res<SharedRenderStagesSlot>) {
            let Some(started) = timer.0 else {
                return;
            };
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            if let Ok(mut guard) = slot.0.lock() {
                guard.$field = Some(ms);
            }
        }
    };
}

fn thread_stage_start() {
    perf::Span::RenderRenderThreadMs.begin();
}

fn thread_stage_end(slot: Res<SharedRenderStagesSlot>) {
    perf::Span::RenderRenderThreadMs.end();
    if let Ok(mut guard) = slot.0.lock() {
        guard.thread_closed = true;
    }
}

fn publish_render_frame(slot: Res<SharedRenderStagesSlot>) {
    if let Ok(mut guard) = slot.0.lock()
        && guard.thread_closed
    {
        guard.publish_render_frame();
    }
}
stage_timer!(
    ExtractCommandsStageTimer,
    extract_commands_stage_start,
    extract_commands_stage_end,
    extract_commands_ms
);
stage_timer!(
    PrepareAssetsStageTimer,
    prepare_assets_stage_start,
    prepare_assets_stage_end,
    prepare_assets_ms
);
stage_timer!(
    PrepareMeshesStageTimer,
    prepare_meshes_stage_start,
    prepare_meshes_stage_end,
    prepare_meshes_ms
);
stage_timer!(
    CreateViewsStageTimer,
    create_views_stage_start,
    create_views_stage_end,
    create_views_ms
);
stage_timer!(
    SpecializeStageTimer,
    specialize_stage_start,
    specialize_stage_end,
    specialize_ms
);
stage_timer!(
    PrepareViewsStageTimer,
    prepare_views_stage_start,
    prepare_views_stage_end,
    prepare_views_ms
);
stage_timer!(
    QueueStageTimer,
    queue_stage_start,
    queue_stage_end,
    queue_ms
);
stage_timer!(
    PhaseSortStageTimer,
    phase_sort_stage_start,
    phase_sort_stage_end,
    phase_sort_ms
);
stage_timer!(
    PrepareStageTimer,
    prepare_stage_start,
    prepare_stage_end,
    prepare_ms
);
fn render_stage_start() {
    perf::Span::RenderRenderRenderMs.begin();
}
fn render_stage_end() {
    perf::Span::RenderRenderRenderMs.end();
}
stage_timer!(
    GraphRenderStageTimer,
    graph_render_start,
    graph_render_end,
    graph_render_ms
);
#[derive(Resource, Default)]
struct GraphSubmitStageTimer(Option<Instant>);

/// What bevy's submit is about to be handed, taken where the graph's render
/// systems have all flushed and nothing has been submitted yet.
///
/// The interval this opens covers `submit_pending_command_buffers` — which
/// finishes every pending encoder and then calls `Queue::submit` — and
/// `handle_uncovered_swap_chains`. Both are bevy's, both are exclusive
/// systems, and neither can be timed from outside; the count is what says
/// whether a long interval had many encoders to finish or one submit that
/// blocked.
fn graph_submit_start(
    mut timer: ResMut<GraphSubmitStageTimer>,
    pending: Res<PendingCommandBuffers>,
    slot: Res<SharedRenderStagesSlot>,
) {
    timer.0 = Some(Instant::now());
    if let Ok(mut guard) = slot.0.lock() {
        guard.graph_submit_pending_n = Some(pending.len() as u32);
    }
}

fn graph_submit_end(timer: Res<GraphSubmitStageTimer>, slot: Res<SharedRenderStagesSlot>) {
    let Some(started) = timer.0 else {
        return;
    };
    let ms = started.elapsed().as_secs_f32() * 1000.0;
    if let Ok(mut guard) = slot.0.lock() {
        guard.graph_submit_ms = Some(ms);
    }
}

#[derive(Resource, Default)]
struct GraphPresentMark(Option<Instant>);

fn graph_present_mark(mut mark: ResMut<GraphPresentMark>, slot: Res<SharedRenderStagesSlot>) {
    let now = Instant::now();
    mark.0 = Some(now);
    // The last set of the graph, which is the latest point in the schedule
    // that still belongs to *this* image: bevy presents after the whole
    // `RenderGraph` schedule returns, from `render_system`, where no system of
    // ours runs. So this is close to the present and deliberately not it.
    let open = perf::frames::open_index();
    if let Ok(mut guard) = slot.0.lock() {
        let origin = guard.working.origin_frame;
        guard.working.presented_frames_behind =
            Some(u32::try_from(open.saturating_sub(origin)).unwrap_or(u32::MAX));
        if let Some(extracted) = guard.working.origin_extracted_at {
            guard.working.presented_state_age_ms =
                Some(now.duration_since(extracted).as_secs_f32() * 1000.0);
        }
    }
}

fn stamp_graph_present(mark: Res<GraphPresentMark>, slot: Res<SharedRenderStagesSlot>) {
    let Some(started) = mark.0 else {
        return;
    };
    let ms = started.elapsed().as_secs_f32() * 1000.0;
    if let Ok(mut guard) = slot.0.lock() {
        guard.graph_present_ms = Some(ms);
    }
}

fn main_opaque_gpu_path() -> DiagnosticPath {
    DiagnosticPath::from_components(["render", "main_opaque_pass_3d", "elapsed_gpu"])
}

pub fn register_render_frame_diag(app: &mut App) {
    app.add_plugins(RenderDiagnosticsPlugin);

    let slot = SharedRenderStagesSlot::default();
    app.insert_resource(slot.clone())
        .init_resource::<RenderFrameDiag>();

    wrap_pipelined_extract_wait(app);

    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        diag::warn!(
            World,
            "render frame diag: RenderApp missing — extract/queue/prepare columns stay NULL"
        );
        return;
    };

    render_app.insert_resource(slot.clone());
    render_app.init_resource::<ExtractCommandsStageTimer>();
    render_app.init_resource::<PrepareAssetsStageTimer>();
    render_app.init_resource::<PrepareMeshesStageTimer>();
    render_app.init_resource::<CreateViewsStageTimer>();
    render_app.init_resource::<SpecializeStageTimer>();
    render_app.init_resource::<PrepareViewsStageTimer>();
    render_app.init_resource::<QueueStageTimer>();
    render_app.init_resource::<PhaseSortStageTimer>();
    render_app.init_resource::<PrepareStageTimer>();
    render_app.init_resource::<GraphRenderStageTimer>();
    render_app.init_resource::<GraphSubmitStageTimer>();
    render_app.init_resource::<GraphPresentMark>();

    let mut default_extract = render_app.take_extract();
    let extract_slot = slot.clone();
    render_app.set_extract(move |main_world, render_world| {
        if let Ok(mut guard) = extract_slot.0.lock() {
            guard.begin_render_frame();
        }
        let started = Instant::now();
        if let Some(f) = default_extract.as_mut() {
            f(main_world, render_world);
        }
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        if let Ok(mut guard) = extract_slot.0.lock() {
            guard.extract_ms = Some(ms);
        }
    });

    render_app.add_systems(
        Render,
        (
            thread_stage_start.before(RenderSystems::ExtractCommands),
            thread_stage_end.after(RenderSystems::PostCleanup),
            publish_render_frame.after(thread_stage_end),
            extract_commands_stage_start.before(RenderSystems::ExtractCommands),
            extract_commands_stage_end
                .after(RenderSystems::ExtractCommands)
                .before(RenderSystems::PrepareAssets),
            prepare_assets_stage_start
                .after(RenderSystems::ExtractCommands)
                .before(RenderSystems::PrepareAssets),
            prepare_assets_stage_end
                .after(RenderSystems::PrepareAssets)
                .before(RenderSystems::PrepareMeshes),
            prepare_meshes_stage_start
                .after(RenderSystems::PrepareAssets)
                .before(RenderSystems::PrepareMeshes),
            prepare_meshes_stage_end
                .after(RenderSystems::PrepareMeshes)
                .before(RenderSystems::CreateViews),
            create_views_stage_start
                .after(RenderSystems::PrepareMeshes)
                .before(RenderSystems::CreateViews),
            create_views_stage_end
                .after(RenderSystems::CreateViews)
                .before(RenderSystems::Specialize),
            specialize_stage_start
                .after(RenderSystems::CreateViews)
                .before(RenderSystems::Specialize),
        ),
    );
    render_app.add_systems(
        Render,
        (
            specialize_stage_end
                .after(RenderSystems::Specialize)
                .before(RenderSystems::PrepareViews),
            prepare_views_stage_start
                .after(RenderSystems::Specialize)
                .before(RenderSystems::PrepareViews),
            prepare_views_stage_end
                .after(RenderSystems::PrepareViews)
                .before(RenderSystems::Queue),
            queue_stage_start
                .after(RenderSystems::PrepareViews)
                .before(RenderSystems::Queue),
            queue_stage_end
                .after(RenderSystems::Queue)
                .before(RenderSystems::PhaseSort),
            phase_sort_stage_start
                .after(RenderSystems::Queue)
                .before(RenderSystems::PhaseSort),
            phase_sort_stage_end
                .after(RenderSystems::PhaseSort)
                .before(RenderSystems::Prepare),
            prepare_stage_start
                .after(RenderSystems::PhaseSort)
                .before(RenderSystems::Prepare),
            prepare_stage_end
                .after(RenderSystems::Prepare)
                .before(RenderSystems::Render),
            render_stage_start
                .after(RenderSystems::Prepare)
                .before(RenderSystems::Render),
            render_stage_end
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
            stamp_graph_present
                .after(render_stage_end)
                .before(RenderSystems::Cleanup),
        ),
    );

    render_app.add_systems(
        RenderGraph,
        (
            graph_render_start
                .after(RenderGraphSystems::Begin)
                .before(RenderGraphSystems::Render),
            graph_render_end
                .after(RenderGraphSystems::Render)
                .before(RenderGraphSystems::Submit),
            graph_submit_start
                .after(graph_render_end)
                .before(RenderGraphSystems::Submit),
            graph_submit_end
                .after(RenderGraphSystems::Submit)
                .before(RenderGraphSystems::Finish),
            graph_present_mark.in_set(RenderGraphSystems::Finish),
        ),
    );
}

fn wrap_pipelined_extract_wait(app: &mut App) {
    let Some(extract_app) = app.get_sub_app_mut(RenderExtractApp) else {
        return;
    };
    let mut default_extract = extract_app.take_extract();
    extract_app.set_extract(move |main_world, sub_world| {
        let _wait = perf::Span::RenderRenderExtractWaitMs.enter();
        if let Some(f) = default_extract.as_mut() {
            f(main_world, sub_world);
        }
    });
}

pub(crate) const GPU_SPAN_COLOUR: &str = "iw4_colour";
pub(crate) const GPU_SPAN_EMISSIVE: &str = "iw4_emissive";
pub(crate) const GPU_SPAN_SUN: &str = "iw4_sun";
pub(crate) const GPU_SPAN_SPOT: &str = "iw4_spot";
pub(crate) const GPU_SPAN_FLOATZ: &str = "iw4_floatz";
pub(crate) const GPU_SPAN_POSTFX: &str = "iw4_postfx";

#[derive(Default)]
struct Iw4GpuSample {
    colour: Option<f32>,
    sun: Option<f32>,
    spot: Option<f32>,
    floatz: Option<f32>,
    postfx: Option<f32>,
}

impl Iw4GpuSample {
    fn add(&mut self, span: &str, ms: f64) {
        let slot = match span {
            GPU_SPAN_COLOUR | GPU_SPAN_EMISSIVE => &mut self.colour,
            GPU_SPAN_SUN => &mut self.sun,
            GPU_SPAN_SPOT => &mut self.spot,
            GPU_SPAN_FLOATZ => &mut self.floatz,
            GPU_SPAN_POSTFX => &mut self.postfx,
            _ => return,
        };
        *slot = Some(slot.unwrap_or(0.0) + ms as f32);
    }
}

struct GraphDiagSample {
    gpu_frame: Option<f32>,
    gpu_opaque: Option<f32>,
    gpu_iw4: Iw4GpuSample,
    cpu_graph: Option<f32>,
    cpu_opaque: Option<f32>,
    cpu_present: Option<f32>,
    cpu_top: Option<String>,
}

/// The newest delivered GPU batch summed, with the batch's own delivery
/// marker: the latest measurement timestamp in that batch. It marks a
/// delivery, not a source frame — equal durations are never compared, and a
/// span with no measurement in the batch stays missing rather than reused.
fn sample_graph_from_store(store: &DiagnosticsStore) -> (GraphDiagSample, Option<Instant>) {
    let mut gpu_sum = 0.0f64;
    let mut gpu_any = false;
    let mut gpu_opaque = store
        .get_measurement(&main_opaque_gpu_path())
        .map(|m| m.value as f32);
    let mut gpu_iw4 = Iw4GpuSample::default();
    let mut cpu_sum = 0.0f64;
    let mut cpu_any = false;
    let mut cpu_opaque = None;
    let mut cpu_present = 0.0f64;
    let mut present_any = false;
    let mut top_ms = 0.0f64;
    let mut top_name: Option<String> = None;

    let newest_gpu_time = store
        .iter()
        .filter(|diagnostic| diagnostic.path().components().last() == Some("elapsed_gpu"))
        .filter_map(|diagnostic| diagnostic.measurement().map(|m| m.time))
        .max();

    for diagnostic in store.iter() {
        let path = diagnostic.path();
        let components: Vec<_> = path.components().collect();
        let Some(kind) = components.last().copied() else {
            continue;
        };
        let Some(m) = diagnostic.measurement() else {
            continue;
        };
        match kind {
            "elapsed_gpu" => {
                if Some(m.time) != newest_gpu_time {
                    continue;
                }
                gpu_any = true;
                gpu_sum += m.value;
                if components.as_slice() == ["render", "main_opaque_pass_3d", "elapsed_gpu"] {
                    gpu_opaque = Some(m.value as f32);
                }
                if let Some(span) = components.len().checked_sub(2).map(|i| components[i]) {
                    gpu_iw4.add(span, m.value);
                }
            }
            "elapsed_cpu" => {
                cpu_any = true;
                cpu_sum += m.value;
                let node = components
                    .iter()
                    .skip(1)
                    .take(components.len().saturating_sub(2))
                    .copied()
                    .collect::<Vec<_>>()
                    .join("/");
                if node == "main_opaque_pass_3d" {
                    cpu_opaque = Some(m.value as f32);
                }
                if node.contains("present") {
                    present_any = true;
                    cpu_present += m.value;
                }
                if m.value >= top_ms {
                    top_ms = m.value;
                    top_name = Some(format!("{node}={:.2}", m.value));
                }
            }
            _ => {}
        }
    }

    (
        GraphDiagSample {
            gpu_frame: gpu_any.then_some(gpu_sum as f32),
            gpu_opaque,
            gpu_iw4,
            cpu_graph: cpu_any.then_some(cpu_sum as f32),
            cpu_opaque,
            cpu_present: present_any.then_some(cpu_present as f32),
            cpu_top: top_name,
        },
        newest_gpu_time,
    )
}

pub fn sample_render_frame_diag(
    slot: Res<SharedRenderStagesSlot>,
    mut diag: ResMut<RenderFrameDiag>,
    store: Option<Res<DiagnosticsStore>>,
    meshes: Res<Assets<Mesh>>,
    visible_meshes: Query<(&Mesh3d, &ViewVisibility)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut last_gpu_batch: Local<Option<Instant>>,
) {
    if let Ok(mut guard) = slot.0.lock() {
        // The published sample is read every main frame; its counters belong
        // to the render frame that produced it, once, and to the frame row
        // that render frame ran inside. The HUD fields below are the last
        // known value either way.
        let fresh = guard
            .completed
            .as_ref()
            .is_some_and(|sample| sample.sequence != guard.consumed);
        if let Some(sequence) = guard.completed.as_ref().map(|sample| sample.sequence) {
            guard.consumed = sequence;
        }
        if let Some(sample) = guard.completed.as_ref() {
            diag.render_extract_ms = sample.extract_ms;
            diag.render_extract_commands_ms = sample.extract_commands_ms;
            diag.render_prepare_assets_ms = sample.prepare_assets_ms;
            diag.render_prepare_meshes_ms = sample.prepare_meshes_ms;
            diag.render_create_views_ms = sample.create_views_ms;
            diag.render_specialize_ms = sample.specialize_ms;
            diag.render_prepare_views_ms = sample.prepare_views_ms;
            diag.render_queue_ms = sample.queue_ms;
            diag.render_phase_sort_ms = sample.phase_sort_ms;
            diag.render_prepare_ms = sample.prepare_ms;
            diag.render_render_ms = sample.render_ms;
            diag.submit_prepare_ms = sample.submit_prepare_ms;
            diag.colour_submit_ms = sample.colour_submit_ms;
            diag.submit_encode_ms = sample.submit_encode_ms;
            diag.submit_gather_ms = sample.submit_gather_ms;
            diag.pass_end_ms = sample.pass_end_ms;
            diag.encoder_finish_ms = sample.encoder_finish_ms;
            diag.submit_arena_ms = sample.submit_arena_ms;
            diag.submit_record_ms = sample.submit_record_ms;
            diag.diag_ms = sample.diag_ms;
            diag.diag_pass = sample.diag_pass;
            diag.diag_draw_n = sample.diag_draw_n;
            diag.diag_world_n = sample.diag_world_n;
            diag.diag_smodel_n = sample.diag_smodel_n;
            diag.diag_xmodel_n = sample.diag_xmodel_n;
            diag.graph_render_ms = sample.graph_render_ms;
            diag.graph_submit_ms = sample.graph_submit_ms;
            diag.graph_submit_pending_n = sample.graph_submit_pending_n;
            diag.graph_present_ms = sample.graph_present_ms;
            diag.presented_state_age_ms = sample.presented_state_age_ms;
            diag.presented_frames_behind = sample.presented_frames_behind;

            if fresh {
                for (counter, value) in [
                    (perf::Counter::RenderGraphRenderMs, sample.graph_render_ms),
                    (
                        perf::Counter::RenderGraphSubmitIntervalMs,
                        sample.graph_submit_ms,
                    ),
                    (perf::Counter::RenderGraphPresentMs, sample.graph_present_ms),
                    (
                        perf::Counter::RenderPresentedStateAgeMs,
                        sample.presented_state_age_ms,
                    ),
                    // The `PrepareViews` set as it ran: a set interval, not a
                    // body. Whatever the executor scheduled inside the set is
                    // in it, so it is not addable to anything and not
                    // exclusive CPU work.
                    (perf::Counter::RenderPrepareViewsMs, sample.prepare_views_ms),
                ] {
                    if let Some(value) = value {
                        counter.emit_at(f64::from(value), sample.origin_frame);
                    }
                }
                if let Some(pending) = sample.graph_submit_pending_n {
                    perf::Counter::RenderGraphSubmitPendingN
                        .emit_at(f64::from(pending), sample.origin_frame);
                }
                if let Some(behind) = sample.presented_frames_behind {
                    perf::Counter::RenderPresentedFramesBehind
                        .emit_at(f64::from(behind), sample.origin_frame);
                }
            }
            diag.wgpu_floor_encode_ms = sample.wgpu_floor_encode_ms;
            diag.wgpu_floor_finish_ms = sample.wgpu_floor_finish_ms;
            diag.wgpu_floor_submit_ms = sample.wgpu_floor_submit_ms;
            diag.wgpu_floor_encode_1bind_ms = sample.wgpu_floor_encode_1bind_ms;
            diag.wgpu_floor_n = sample.wgpu_floor_n;
            diag.wgpu_floor_binds = sample.wgpu_floor_binds;
            diag.pack_intern_hit_n = sample.pack_intern_hit_n;
            diag.pack_intern_miss_n = sample.pack_intern_miss_n;
            diag.pack_arena_share_n = sample.pack_arena_share_n;
            diag.gpu_exec_reuse_n = sample.gpu_exec_reuse_n;
            diag.gpu_exec_unique_n = sample.gpu_exec_unique_n;
            diag.pack_overlay_n = sample.pack_overlay_n;
            diag.pack_overlay_row_n = sample.pack_overlay_row_n;
            diag.pack_overlay_pixel_share_n = sample.pack_overlay_pixel_share_n;
            diag.pack_arena_vertex_n = sample.pack_arena_vertex_n;
            diag.pack_arena_pixel_n = sample.pack_arena_pixel_n;
            diag.pack_seed_n = sample.pack_seed_n;
            diag.pack_walk_n = sample.pack_walk_n;
            diag.tex_bind_hit_n = sample.tex_bind_hit_n;
            diag.tex_bind_miss_n = sample.tex_bind_miss_n;
            diag.markmesh_hits = sample.markmesh_hits;
            diag.markmesh_prepared = sample.markmesh_prepared;
            diag.last_markmesh_refusal = sample.last_markmesh_refusal.clone();
            diag.last_markmesh_exec_skip = sample.last_markmesh_exec_skip.clone();
            diag.markmesh_missing_58 = sample.markmesh_missing_58;
            diag.last_mark_packed_custom = sample.last_mark_packed_custom;
            diag.last_mark_packed_scene_light = sample.last_mark_packed_scene_light;
            diag.last_mark_lmap_sampler = sample.last_mark_lmap_sampler;
            diag.glassmesh_hits = sample.glassmesh_hits;
            diag.glassmesh_prepared = sample.glassmesh_prepared;
            diag.last_glassmesh_exec_skip = sample.last_glassmesh_exec_skip.clone();
            diag.last_glassmesh_refusal = sample.last_glassmesh_refusal.clone();
            diag.last_glass_packed_probe = sample.last_glass_packed_probe;
            diag.last_glass_probe_sampler = sample.last_glass_probe_sampler;
            diag.gpu_ready = sample.gpu_ready;
            diag.set_bind_group_n = sample.set_bind_group_n;
            diag.gpu_prepared = sample.gpu_prepared;
            diag.gpu_world_ready = sample.gpu_world_ready;
            diag.bsp_submitted_surfaces = sample.bsp_submitted_surfaces;
            diag.bsp_submit_refused_surfaces = sample.bsp_submit_refused_surfaces;
            diag.bsp_drawn_surfaces = sample.bsp_drawn_surfaces;
            diag.bsp_draw_refused_surfaces = sample.bsp_draw_refused_surfaces;
            diag.gpu_smodel_ready = sample.gpu_smodel_ready;
            diag.gpu_xmodel_ready = sample.gpu_xmodel_ready;
            diag.end_depth_restore_n = sample.end_depth_restore_n;
            diag.end_depth_range_type = sample.end_depth_range_type;
            diag.code_mesh_gpu_kind = sample.code_mesh_gpu_kind;
            diag.tess_stream_bind_n = sample.tess_stream_bind_n;
            diag.tess_stream_skip_n = sample.tess_stream_skip_n;
            diag.sun_shadow_gpu = sample.sun_shadow_gpu;
            diag.sun_shadow_gpu_miss = sample.sun_shadow_gpu_miss;
            diag.sun_shadow_gpu_cause = sample.sun_shadow_gpu_cause.clone();
            diag.sun_shadow_gpu_causes = sample.sun_shadow_gpu_causes.clone();
            diag.spot_shadow_gpu = sample.spot_shadow_gpu;
            diag.spot_shadow_gpu_miss = sample.spot_shadow_gpu_miss;
            diag.spot_shadow_gpu_cause = sample.spot_shadow_gpu_cause.clone();
            diag.spot_shadow_slot_n = sample.spot_shadow_slot_n;
            if fresh {
                for (counter, value) in [
                    (perf::Counter::SpotShadowGpu, sample.spot_shadow_gpu),
                    (
                        perf::Counter::SpotShadowGpuMiss,
                        sample.spot_shadow_gpu_miss,
                    ),
                    (perf::Counter::SpotShadowSlotN, sample.spot_shadow_slot_n),
                ] {
                    if let Some(value) = value {
                        counter.emit_at(f64::from(value), sample.origin_frame);
                    }
                }
            }
            diag.sun_shadow_submit_ms = sample.sun_shadow_submit_ms;
            diag.sun_shadow_prepare_ms = sample.sun_shadow_prepare_ms;
            diag.sun_shadow_patch_ms = sample.sun_shadow_patch_ms;
            diag.sun_shadow_arena_ms = sample.sun_shadow_arena_ms;
            diag.sun_shadow_record_ms = sample.sun_shadow_record_ms;
            diag.sun_shadow_finish_ms = sample.sun_shadow_finish_ms;
            diag.sun_shadow_queue_ms = sample.sun_shadow_queue_ms;
            diag.sun_shadow_unnamed_ms = sample.sun_shadow_unnamed_ms;
            diag.sun_shadow_static_hit = sample.sun_shadow_static_hit;
            diag.sun_shadow_world_ib_n = sample.sun_shadow_world_ib_n;
            diag.sun_shadow_static_n = sample.sun_shadow_static_n;
            diag.sun_shadow_dynamic_n = sample.sun_shadow_dynamic_n;
            diag.sun_shadow_wvp_intern_hit = sample.sun_shadow_wvp_intern_hit;
            diag.sun_shadow_wvp_intern_miss = sample.sun_shadow_wvp_intern_miss;
            diag.sun_shadow_wvp_intern_n = sample.sun_shadow_wvp_intern_n;
            diag.sun_shadow_wvp_unique_base = sample.sun_shadow_wvp_unique_base;
            diag.sun_shadow_wvp_unique_wvp = sample.sun_shadow_wvp_unique_wvp;
            diag.sun_shadow_state_pipe_n = sample.sun_shadow_state_pipe_n;
            diag.sun_shadow_state_tess_n = sample.sun_shadow_state_tess_n;
            diag.sun_shadow_state_bind_n = sample.sun_shadow_state_bind_n;
            diag.sun_shadow_state_off_n = sample.sun_shadow_state_off_n;
            diag.sun_shadow_state_group_n = sample.sun_shadow_state_group_n;
            diag.sun_shadow_state_run_n = sample.sun_shadow_state_run_n;
            diag.sun_shadow_state_run_max = sample.sun_shadow_state_run_max;
            diag.sun_shadow_state_top10 = sample.sun_shadow_state_top10;
            diag.world_index_gaps = sample.world_index_gaps;
            diag.world_run_indices_n = sample.world_run_indices_n;
            diag.world_material_runs = sample.world_material_runs;
            diag.world_material_runs_seq = sample.world_material_runs_seq;
            diag.world_key_runs = sample.world_key_runs;
            diag.world_key_runs_seq = sample.world_key_runs_seq;
            diag.world_mixed_breaks = sample.world_mixed_breaks;
            diag.world_gathered = sample.world_gathered;
            diag.world_ib_skip = sample.world_ib_skip;
            diag.world_gpu_runs = sample.world_gpu_runs;
            diag.world_gpu_runs_seq = sample.world_gpu_runs_seq;
            diag.world_sampler_runs_seq = sample.world_sampler_runs_seq;
            diag.world_probe_runs_seq = sample.world_probe_runs_seq;
            diag.world_light_runs_seq = sample.world_light_runs_seq;
            diag.smodel_reuse_n = sample.smodel_reuse_n;
            diag.xmodel_reuse_n = sample.xmodel_reuse_n;
            diag.xmodel_material_runs = sample.xmodel_material_runs;
            diag.smodel_index_gaps = sample.smodel_index_gaps;
            diag.smodel_material_runs = sample.smodel_material_runs;
            diag.smodel_material_runs_seq = sample.smodel_material_runs_seq;
            diag.smodel_material_run_max = sample.smodel_material_run_max;
            diag.smodel_same_surface_n = sample.smodel_same_surface_n;
            diag.smodel_unique_surfaces = sample.smodel_unique_surfaces;
            diag.smodel_hits = sample.smodel_hits;
            diag.smodel_lighting_runs = sample.smodel_lighting_runs;
            diag.smodel_lighting_run_max = sample.smodel_lighting_run_max;
            diag.smodel_pretess_runs = sample.smodel_pretess_runs;
            diag.smodel_pretess_hits = sample.smodel_pretess_hits;
            diag.smodel_pretess_verts = sample.smodel_pretess_verts;
            diag.smodel_pretess_indices = sample.smodel_pretess_indices;
            diag.smodel_cached_lighting = sample.smodel_cached_lighting;
            diag.smodel_pretess_local = sample.smodel_pretess_local;
            diag.smodel_pretess_length1 = sample.smodel_pretess_length1;
            diag.smodel_pretess_skip = sample.smodel_pretess_skip;
            diag.submit_cause = sample.submit_cause.clone();
            diag.submit_cause2 = sample.submit_cause2.clone();
            diag.gpu_not_ready_n = sample.gpu_not_ready_n;
            diag.gpu_no_port_n = sample.gpu_no_port_n;
            diag.pnr_smodel_mat = sample.pnr_smodel_mat.clone();
            diag.pnr_world_mat = sample.pnr_world_mat.clone();
            diag.pnr_smodel_ps = sample.pnr_smodel_ps.clone();
            diag.pnr_world_ps = sample.pnr_world_ps.clone();
            diag.pnr_smodel_key_n = sample.pnr_smodel_key_n;
            diag.pnr_world_key_n = sample.pnr_world_key_n;
            diag.pnr_port_n = sample.pnr_port_n;
            diag.gpu_smodel_bind_mat = sample.gpu_smodel_bind_mat.clone();
            diag.extract_ports_ms = sample.extract_ports_ms;
            diag.extract_tess_ms = sample.extract_tess_ms;
            diag.extract_products_ms = sample.extract_products_ms;
            diag.extract_images_ms = sample.extract_images_ms;
            diag.extract_diag_ms = sample.extract_diag_ms;
            diag.extract_world_skip = sample.extract_world_skip;
            diag.extract_world_clone_bytes = sample.extract_world_clone_bytes;
            diag.extract_xmodel_clone_bytes = sample.extract_xmodel_clone_bytes;
            diag.extract_xmodel_arc = sample.extract_xmodel_arc;
            diag.extract_fx_arc = sample.extract_fx_arc;
            diag.extract_fx_clone_bytes = sample.extract_fx_clone_bytes;
            diag.extract_products_arc = sample.extract_products_arc;
            diag.extract_images_arc = sample.extract_images_arc;
            diag.extract_products_bank_new = sample.extract_products_bank_new;
        }
    }

    if let Some(store) = store.as_ref() {
        let (sample, batch) = sample_graph_from_store(store);
        // The store holds the newest delivered batch, not this frame's work,
        // so without a delivery marker the same batch would publish again on
        // every main frame until the device resolves another. The batch
        // timestamp is the marker: a delivery identity, not a source-frame
        // id. Missing spans stay missing — they emit nothing either way.
        let fresh_batch = batch != *last_gpu_batch;
        if fresh_batch {
            *last_gpu_batch = batch;
            if let Some(ms) = sample.gpu_frame {
                perf::Counter::RenderGpuFrameMs.emit(f64::from(ms));
            }
            for (counter, value) in [
                (perf::Counter::RenderGpuColourMs, sample.gpu_iw4.colour),
                (perf::Counter::RenderGpuSunMs, sample.gpu_iw4.sun),
                (perf::Counter::RenderGpuSpotMs, sample.gpu_iw4.spot),
                (perf::Counter::RenderGpuFloatzMs, sample.gpu_iw4.floatz),
                (perf::Counter::RenderGpuPostfxMs, sample.gpu_iw4.postfx),
            ] {
                if let Some(ms) = value {
                    counter.emit(f64::from(ms));
                }
            }
        }
        diag.gpu_opaque_ms = sample.gpu_opaque;
        diag.cpu_graph_ms = sample.cpu_graph;
        diag.cpu_opaque_ms = sample.cpu_opaque;
        diag.cpu_present_ms = sample.cpu_present;
        diag.cpu_graph_top = sample.cpu_top;
    }

    let mut entities = 0u32;
    let mut index_count = 0u64;
    for (mesh3d, view_vis) in &visible_meshes {
        if !view_vis.get() {
            continue;
        }
        entities = entities.saturating_add(1);
        if let Some(mesh) = meshes.get(&mesh3d.0) {
            let n = mesh.indices().map(|i| i.len() as u64).unwrap_or(0);
            index_count = index_count.saturating_add(n);
        }
    }
    diag.drawn_entities = Some(entities);
    diag.drawn_index_count = Some(index_count);
    diag.present_mode = windows
        .iter()
        .next()
        .map(|w| format!("{:?}", w.present_mode));

    static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n % RENDER_FRAME_LOG_EVERY == 0 {
        diag::info!(
            World,
            "render frame diag: extract_ms={} extract_commands_ms={} prepare_assets_ms={} prepare_meshes_ms={} create_views_ms={} specialize_ms={} prepare_views_ms={} queue_ms={} phase_sort_ms={} prepare_ms={} render_ms={} present_mode={} submit_prepare_ms={} \
             ports_ms={} tess_ms={} products_ms={} images_ms={} diag_ms={} world_skip={} \
             world_clone_bytes={} xmodel_clone_bytes={} fx_clone_bytes={} \
             gpu_frame_ms={} gpu_opaque_ms={} drawn_entities={} drawn_index_count={}",
            fmt_opt_ms(diag.render_extract_ms),
            fmt_opt_ms(diag.render_extract_commands_ms),
            fmt_opt_ms(diag.render_prepare_assets_ms),
            fmt_opt_ms(diag.render_prepare_meshes_ms),
            fmt_opt_ms(diag.render_create_views_ms),
            fmt_opt_ms(diag.render_specialize_ms),
            fmt_opt_ms(diag.render_prepare_views_ms),
            fmt_opt_ms(diag.render_queue_ms),
            fmt_opt_ms(diag.render_phase_sort_ms),
            fmt_opt_ms(diag.render_prepare_ms),
            fmt_opt_ms(diag.render_render_ms),
            diag.present_mode.as_deref().unwrap_or("NULL"),
            fmt_opt_ms(diag.submit_prepare_ms),
            fmt_opt_ms(diag.extract_ports_ms),
            fmt_opt_ms(diag.extract_tess_ms),
            fmt_opt_ms(diag.extract_products_ms),
            fmt_opt_ms(diag.extract_images_ms),
            fmt_opt_ms(diag.extract_diag_ms),
            diag.extract_world_skip
                .map(|n| n.to_string())
                .unwrap_or_else(|| "NULL".into()),
            diag.extract_world_clone_bytes
                .map(|n| n.to_string())
                .unwrap_or_else(|| "NULL".into()),
            diag.extract_xmodel_clone_bytes
                .map(|n| n.to_string())
                .unwrap_or_else(|| "NULL".into()),
            diag.extract_fx_clone_bytes
                .map(|n| n.to_string())
                .unwrap_or_else(|| "NULL".into()),
            fmt_opt_ms(diag.gpu_frame_ms),
            fmt_opt_ms(diag.gpu_opaque_ms),
            entities,
            index_count
        );
    }
}

fn fmt_opt_ms(v: Option<f32>) -> String {
    match v {
        Some(ms) => format!("{ms:.3}"),
        None => "NULL".into(),
    }
}
