use bevy::prelude::{Res, Resource};

#[derive(Resource, Default)]
pub(in crate::drawsurf) struct ExactColourSubmitCensus {
    pub(super) preparation: SubmitPreparationCensus,
    pub(super) frame: ExactColourFrameCensus,
    pub(in crate::drawsurf) submitted_keys: Vec<u64>,
    pub(in crate::drawsurf) world_exec_ready_keys: Vec<u64>,
}

impl ExactColourSubmitCensus {
    pub(super) fn begin_prepare(&mut self, recording: bool) {
        self.submitted_keys.clear();
        if recording {
            self.frame = ExactColourFrameCensus::default();
        }
    }

    pub(super) fn publish_coverage(&mut self, submitted: &[u64], world_ready: &[u64]) {
        self.submitted_keys.clear();
        self.submitted_keys.extend_from_slice(submitted);
        self.world_exec_ready_keys.clear();
        self.world_exec_ready_keys.extend_from_slice(world_ready);
    }
}

#[derive(Default)]
pub(super) struct SubmitPreparationCensus {
    pub(super) ready_draws: u32,
    pub(super) refused_draws: u32,
    pub(super) log_frame: u32,
    pub(super) gpu_ready: Option<u32>,
    pub(super) set_bind_group0_n: Option<u32>,
    pub(super) set_bind_group1_n: Option<u32>,
    pub(super) material_runs_n: Option<u32>,
    pub(super) pass_setups_n: Option<u32>,
    pub(super) obj_binds_n: Option<u32>,
    pub(super) shell_hits_n: Option<u32>,
    pub(super) shell_misses_n: Option<u32>,
    pub(super) overlay_const_writes_n: Option<u32>,
    pub(super) overlay_need_known_n: Option<u32>,
    pub(super) code_mesh_gpu_kind: Option<i32>,
    pub(super) sun_shadow_prepare_ms: Option<f32>,
    pub(super) sun_shadow_patch_ms: Option<f32>,
    pub(super) sun_shadow_arena_ms: Option<f32>,
    pub(super) sun_shadow_record_ms: Option<f32>,
    pub(super) sun_shadow_finish_ms: Option<f32>,
    pub(super) sun_shadow_queue_ms: Option<f32>,
    pub(super) sun_shadow_unnamed_ms: Option<f32>,
    pub(super) sun_shadow_static_hit: Option<u32>,
    pub(super) sun_shadow_static_n: Option<u32>,
    pub(super) sun_shadow_dynamic_n: Option<u32>,
    pub(super) sun_shadow_wvp_intern_hit: Option<u32>,
    pub(super) sun_shadow_wvp_intern_miss: Option<u32>,
    pub(super) sun_shadow_wvp_intern_n: Option<u32>,
    pub(super) sun_shadow_wvp_unique_base: Option<u32>,
    pub(super) sun_shadow_wvp_unique_wvp: Option<u32>,
    pub(super) sun_shadow_state_pipe_n: Option<u32>,
    pub(super) sun_shadow_state_tess_n: Option<u32>,
    pub(super) sun_shadow_state_bind_n: Option<u32>,
    pub(super) sun_shadow_state_off_n: Option<u32>,
    pub(super) sun_shadow_state_group_n: Option<u32>,
    pub(super) sun_shadow_state_run_n: Option<u32>,
    pub(super) sun_shadow_state_run_max: Option<u32>,
    pub(super) sun_shadow_state_top10: Option<u32>,
}

#[derive(Default)]
pub(super) struct ExactColourFrameCensus {
    pub(super) bsp_submitted_surfaces: [u32; 4],
    pub(super) bsp_submit_refused_surfaces: [u32; 4],
    pub(super) bsp_drawn_surfaces: [u32; 4],
    pub(super) bsp_draw_refused_surfaces: [u32; 4],
    pub(super) submit_prepare_ms: Option<f32>,
    pub(super) colour_submit_ms: Option<f32>,
    pub(super) submit_encode_ms: Option<f32>,
    pub(super) submit_gather_ms: Option<f32>,
    pub(super) pass_end_ms: Option<f32>,
    pub(super) encoder_finish_ms: Option<f32>,
    pub(super) submit_arena_ms: Option<f32>,
    pub(super) submit_record_ms: Option<f32>,
    pub(super) pack_intern_hit_n: Option<u32>,
    pub(super) pack_intern_miss_n: Option<u32>,
    pub(super) pack_arena_share_n: Option<u32>,
    pub(super) gpu_exec_reuse_n: Option<u32>,
    pub(super) gpu_exec_unique_n: Option<u32>,
    pub(super) pack_overlay_n: Option<u32>,
    pub(super) pack_overlay_row_n: Option<u32>,
    pub(super) pack_overlay_pixel_share_n: Option<u32>,
    pub(super) pack_arena_vertex_n: Option<u32>,
    pub(super) pack_arena_pixel_n: Option<u32>,
    pub(super) pack_seed_n: Option<u32>,
    pub(super) pack_walk_n: Option<u32>,
    pub(super) tex_bind_hit_n: Option<u32>,
    pub(super) tex_bind_miss_n: Option<u32>,
    pub(super) markmesh_hits: Option<u32>,
    pub(super) markmesh_prepared: Option<u32>,
    pub(super) last_markmesh_refusal: Option<String>,
    pub(super) last_markmesh_exec_skip: Option<String>,
    pub(super) markmesh_missing_58: Option<u32>,
    pub(super) last_mark_packed_custom: Option<u8>,
    pub(super) last_mark_packed_scene_light: Option<u8>,
    pub(super) last_mark_lmap_sampler: Option<u32>,
    pub(super) glassmesh_hits: Option<u32>,
    pub(super) glassmesh_prepared: Option<u32>,
    pub(super) last_glassmesh_exec_skip: Option<String>,
    pub(super) last_glassmesh_refusal: Option<String>,
    pub(super) last_glass_packed_probe: Option<u8>,
    pub(super) last_glass_probe_sampler: Option<u32>,
    pub(super) set_bind_group_n: Option<u32>,
    pub(super) set_state_n: Option<u32>,
    pub(super) multi_draw_n: Option<u32>,
    pub(super) multi_draw_commands_n: Option<u32>,
    pub(super) gpu_prepared: Option<u32>,
    pub(super) gpu_world_ready: Option<u32>,
    pub(super) gpu_smodel_ready: Option<u32>,
    pub(super) gpu_xmodel_ready: Option<u32>,
    pub(super) end_depth_restore_n: Option<u32>,
    pub(super) end_depth_range_type: Option<i32>,
    pub(super) sun_shadow_gpu: Option<u32>,
    pub(super) sun_shadow_gpu_miss: Option<u32>,
    pub(super) sun_shadow_gpu_cause: Option<String>,
    pub(super) sun_shadow_gpu_causes: Option<String>,
    pub(super) spot_shadow_gpu: Option<u32>,
    pub(super) spot_shadow_gpu_miss: Option<u32>,
    pub(super) spot_shadow_gpu_cause: Option<String>,
    pub(super) spot_shadow_slot_n: Option<u32>,
    pub(super) sun_shadow_submit_ms: Option<f32>,
    pub(super) sun_shadow_world_ib_n: Option<u32>,
    pub(super) world_index_gaps: Option<u32>,
    pub(super) world_run_indices_n: Option<u32>,
    pub(super) world_material_runs: Option<u32>,
    pub(super) world_material_runs_seq: Option<u32>,
    pub(super) world_key_runs: Option<u32>,
    pub(super) world_key_runs_seq: Option<u32>,
    pub(super) world_mixed_breaks: Option<u32>,
    pub(super) world_gathered: Option<u32>,
    pub(super) world_ib_skip: Option<u32>,
    pub(super) world_gpu_runs: Option<u32>,
    pub(super) world_gpu_runs_seq: Option<u32>,
    pub(super) world_sampler_runs_seq: Option<u32>,
    pub(super) world_probe_runs_seq: Option<u32>,
    pub(super) world_light_runs_seq: Option<u32>,
    pub(super) smodel_reuse_n: Option<u32>,
    pub(super) xmodel_reuse_n: Option<u32>,
    pub(super) xmodel_material_runs: Option<u32>,
    pub(super) smodel_index_gaps: Option<u32>,
    pub(super) smodel_material_runs: Option<u32>,
    pub(super) smodel_material_runs_seq: Option<u32>,
    pub(super) smodel_material_run_max: Option<u32>,
    pub(super) smodel_same_surface_n: Option<u32>,
    pub(super) smodel_unique_surfaces: Option<u32>,
    pub(super) smodel_hits: Option<u32>,
    pub(super) smodel_lighting_runs: Option<u32>,
    pub(super) smodel_lighting_run_max: Option<u32>,
    pub(super) smodel_pretess_runs: Option<u32>,
    pub(super) smodel_pretess_hits: Option<u32>,
    pub(super) smodel_pretess_verts: Option<u32>,
    pub(super) smodel_pretess_indices: Option<u32>,
    pub(super) smodel_cached_lighting: Option<u32>,
    pub(super) smodel_pretess_local: Option<u32>,
    pub(super) smodel_pretess_length1: Option<u32>,
    pub(super) smodel_pretess_skip: Option<u32>,
    pub(super) submit_cause: Option<String>,
    pub(super) submit_cause2: Option<String>,
    pub(super) gpu_not_ready_n: Option<u32>,
    pub(super) gpu_no_port_n: Option<u32>,
    pub(super) pnr_smodel_mat: Option<String>,
    pub(super) pnr_world_mat: Option<String>,
    pub(super) pnr_smodel_ps: Option<String>,
    pub(super) pnr_world_ps: Option<String>,
    pub(super) pnr_smodel_key_n: Option<u32>,
    pub(super) pnr_world_key_n: Option<u32>,
    pub(super) pnr_port_n: Option<u32>,
    pub(super) gpu_smodel_bind_mat: Option<String>,
}

pub(super) fn copy_submit_prepare_ms(
    census: Res<ExactColourSubmitCensus>,
    slot: Option<Res<crate::diag::render_frame_diag::SharedRenderStagesSlot>>,
) {
    if !perf::recording() {
        return;
    }

    for (counter, value) in [
        (
            perf::counter!("dip_colour", "iw4l.render", handle),
            census.preparation.gpu_ready,
        ),
        (
            perf::counter!("dip_sun", "iw4l.render", handle),
            census.frame.sun_shadow_gpu,
        ),
        (
            perf::counter!("bind0", "iw4l.render", handle),
            census.preparation.set_bind_group0_n,
        ),
        (
            perf::counter!("bind1", "iw4l.render", handle),
            census.preparation.set_bind_group1_n,
        ),
        (
            perf::counter!("cmd_state", "iw4l.render", handle),
            census.frame.set_state_n,
        ),
        (
            perf::counter!("overlay_const_writes", "iw4l.render", handle),
            census.preparation.overlay_const_writes_n,
        ),
        (
            perf::counter!("multi_draw", "iw4l.render", handle),
            census.frame.multi_draw_n,
        ),
        (
            perf::counter!("multi_draw_cmds", "iw4l.render", handle),
            census.frame.multi_draw_commands_n,
        ),
    ] {
        if let Some(value) = value {
            counter.emit(f64::from(value));
        }
    }
    for (counter, value) in [
        (
            perf::counter!("submit_sun", "iw4l.render", handle),
            census.frame.sun_shadow_submit_ms,
        ),
        (
            perf::counter!("submit_gather", "iw4l.render", handle),
            census.frame.submit_gather_ms,
        ),
        (
            perf::counter!("submit_prepare", "iw4l.render", handle),
            census.frame.submit_prepare_ms,
        ),
        (
            perf::counter!("submit_arena", "iw4l.render", handle),
            census.frame.submit_arena_ms,
        ),
        (
            perf::counter!("submit_record", "iw4l.render", handle),
            census.frame.submit_record_ms,
        ),
    ] {
        if let Some(value) = value {
            counter.emit(f64::from(value));
        }
    }
    let Some(slot) = slot else {
        return;
    };
    if let Ok(mut guard) = slot.0.lock() {
        guard.submit_prepare_ms = census.frame.submit_prepare_ms;
        guard.colour_submit_ms = census.frame.colour_submit_ms;
        guard.submit_encode_ms = census.frame.submit_encode_ms;
        guard.submit_gather_ms = census.frame.submit_gather_ms;
        guard.pass_end_ms = census.frame.pass_end_ms;
        guard.encoder_finish_ms = census.frame.encoder_finish_ms;
        guard.submit_arena_ms = census.frame.submit_arena_ms;
        guard.submit_record_ms = census.frame.submit_record_ms;
        guard.pack_intern_hit_n = census.frame.pack_intern_hit_n;
        guard.pack_intern_miss_n = census.frame.pack_intern_miss_n;
        guard.pack_arena_share_n = census.frame.pack_arena_share_n;
        guard.gpu_exec_reuse_n = census.frame.gpu_exec_reuse_n;
        guard.gpu_exec_unique_n = census.frame.gpu_exec_unique_n;
        guard.pack_overlay_n = census.frame.pack_overlay_n;
        guard.pack_overlay_row_n = census.frame.pack_overlay_row_n;
        guard.pack_overlay_pixel_share_n = census.frame.pack_overlay_pixel_share_n;
        guard.pack_arena_vertex_n = census.frame.pack_arena_vertex_n;
        guard.pack_arena_pixel_n = census.frame.pack_arena_pixel_n;
        guard.pack_seed_n = census.frame.pack_seed_n;
        guard.pack_walk_n = census.frame.pack_walk_n;
        guard.tex_bind_hit_n = census.frame.tex_bind_hit_n;
        guard.tex_bind_miss_n = census.frame.tex_bind_miss_n;
        guard.markmesh_hits = census.frame.markmesh_hits;
        guard.markmesh_prepared = census.frame.markmesh_prepared;
        guard.last_markmesh_refusal = census.frame.last_markmesh_refusal.clone();
        guard.last_markmesh_exec_skip = census.frame.last_markmesh_exec_skip.clone();
        guard.markmesh_missing_58 = census.frame.markmesh_missing_58;
        guard.last_mark_packed_custom = census.frame.last_mark_packed_custom;
        guard.last_mark_packed_scene_light = census.frame.last_mark_packed_scene_light;
        guard.last_mark_lmap_sampler = census.frame.last_mark_lmap_sampler;
        guard.glassmesh_hits = census.frame.glassmesh_hits;
        guard.glassmesh_prepared = census.frame.glassmesh_prepared;
        guard.last_glassmesh_exec_skip = census.frame.last_glassmesh_exec_skip.clone();
        guard.last_glassmesh_refusal = census.frame.last_glassmesh_refusal.clone();
        guard.last_glass_packed_probe = census.frame.last_glass_packed_probe;
        guard.last_glass_probe_sampler = census.frame.last_glass_probe_sampler;
        guard.gpu_ready = census.preparation.gpu_ready;
        guard.set_bind_group_n = census.frame.set_bind_group_n;
        guard.gpu_prepared = census.frame.gpu_prepared;
        guard.gpu_world_ready = census.frame.gpu_world_ready;
        guard.bsp_submitted_surfaces = census.frame.bsp_submitted_surfaces;
        guard.bsp_submit_refused_surfaces = census.frame.bsp_submit_refused_surfaces;
        guard.bsp_drawn_surfaces = census.frame.bsp_drawn_surfaces;
        guard.bsp_draw_refused_surfaces = census.frame.bsp_draw_refused_surfaces;
        guard.gpu_smodel_ready = census.frame.gpu_smodel_ready;
        guard.gpu_xmodel_ready = census.frame.gpu_xmodel_ready;
        guard.end_depth_restore_n = census.frame.end_depth_restore_n;
        guard.end_depth_range_type = census.frame.end_depth_range_type;
        guard.code_mesh_gpu_kind = census.preparation.code_mesh_gpu_kind;
        guard.sun_shadow_gpu = census.frame.sun_shadow_gpu;
        guard.sun_shadow_gpu_miss = census.frame.sun_shadow_gpu_miss;
        guard.sun_shadow_gpu_cause = census.frame.sun_shadow_gpu_cause.clone();
        guard.sun_shadow_gpu_causes = census.frame.sun_shadow_gpu_causes.clone();
        guard.spot_shadow_gpu = census.frame.spot_shadow_gpu;
        guard.spot_shadow_gpu_miss = census.frame.spot_shadow_gpu_miss;
        guard.spot_shadow_gpu_cause = census.frame.spot_shadow_gpu_cause.clone();
        guard.spot_shadow_slot_n = census.frame.spot_shadow_slot_n;
        guard.sun_shadow_submit_ms = census.frame.sun_shadow_submit_ms;
        guard.sun_shadow_prepare_ms = census.preparation.sun_shadow_prepare_ms;
        guard.sun_shadow_patch_ms = census.preparation.sun_shadow_patch_ms;
        guard.sun_shadow_arena_ms = census.preparation.sun_shadow_arena_ms;
        guard.sun_shadow_record_ms = census.preparation.sun_shadow_record_ms;
        guard.sun_shadow_finish_ms = census.preparation.sun_shadow_finish_ms;
        guard.sun_shadow_queue_ms = census.preparation.sun_shadow_queue_ms;
        guard.sun_shadow_unnamed_ms = census.preparation.sun_shadow_unnamed_ms;
        guard.sun_shadow_static_hit = census.preparation.sun_shadow_static_hit;
        guard.sun_shadow_world_ib_n = census.frame.sun_shadow_world_ib_n;
        guard.sun_shadow_static_n = census.preparation.sun_shadow_static_n;
        guard.sun_shadow_dynamic_n = census.preparation.sun_shadow_dynamic_n;
        guard.sun_shadow_wvp_intern_hit = census.preparation.sun_shadow_wvp_intern_hit;
        guard.sun_shadow_wvp_intern_miss = census.preparation.sun_shadow_wvp_intern_miss;
        guard.sun_shadow_wvp_intern_n = census.preparation.sun_shadow_wvp_intern_n;
        guard.sun_shadow_wvp_unique_base = census.preparation.sun_shadow_wvp_unique_base;
        guard.sun_shadow_wvp_unique_wvp = census.preparation.sun_shadow_wvp_unique_wvp;
        guard.sun_shadow_state_pipe_n = census.preparation.sun_shadow_state_pipe_n;
        guard.sun_shadow_state_tess_n = census.preparation.sun_shadow_state_tess_n;
        guard.sun_shadow_state_bind_n = census.preparation.sun_shadow_state_bind_n;
        guard.sun_shadow_state_off_n = census.preparation.sun_shadow_state_off_n;
        guard.sun_shadow_state_group_n = census.preparation.sun_shadow_state_group_n;
        guard.sun_shadow_state_run_n = census.preparation.sun_shadow_state_run_n;
        guard.sun_shadow_state_run_max = census.preparation.sun_shadow_state_run_max;
        guard.sun_shadow_state_top10 = census.preparation.sun_shadow_state_top10;
        guard.world_index_gaps = census.frame.world_index_gaps;
        guard.world_run_indices_n = census.frame.world_run_indices_n;
        guard.world_material_runs = census.frame.world_material_runs;
        guard.world_material_runs_seq = census.frame.world_material_runs_seq;
        guard.world_key_runs = census.frame.world_key_runs;
        guard.world_key_runs_seq = census.frame.world_key_runs_seq;
        guard.world_mixed_breaks = census.frame.world_mixed_breaks;
        guard.world_gathered = census.frame.world_gathered;
        guard.world_ib_skip = census.frame.world_ib_skip;
        guard.world_gpu_runs = census.frame.world_gpu_runs;
        guard.world_gpu_runs_seq = census.frame.world_gpu_runs_seq;
        guard.world_sampler_runs_seq = census.frame.world_sampler_runs_seq;
        guard.world_probe_runs_seq = census.frame.world_probe_runs_seq;
        guard.world_light_runs_seq = census.frame.world_light_runs_seq;
        guard.smodel_reuse_n = census.frame.smodel_reuse_n;
        guard.xmodel_reuse_n = census.frame.xmodel_reuse_n;
        guard.xmodel_material_runs = census.frame.xmodel_material_runs;
        guard.smodel_index_gaps = census.frame.smodel_index_gaps;
        guard.smodel_material_runs = census.frame.smodel_material_runs;
        guard.smodel_material_runs_seq = census.frame.smodel_material_runs_seq;
        guard.smodel_material_run_max = census.frame.smodel_material_run_max;
        guard.smodel_same_surface_n = census.frame.smodel_same_surface_n;
        guard.smodel_unique_surfaces = census.frame.smodel_unique_surfaces;
        guard.smodel_hits = census.frame.smodel_hits;
        guard.smodel_lighting_runs = census.frame.smodel_lighting_runs;
        guard.smodel_lighting_run_max = census.frame.smodel_lighting_run_max;
        guard.smodel_pretess_runs = census.frame.smodel_pretess_runs;
        guard.smodel_pretess_hits = census.frame.smodel_pretess_hits;
        guard.smodel_pretess_verts = census.frame.smodel_pretess_verts;
        guard.smodel_pretess_indices = census.frame.smodel_pretess_indices;
        guard.smodel_cached_lighting = census.frame.smodel_cached_lighting;
        guard.smodel_pretess_local = census.frame.smodel_pretess_local;
        guard.smodel_pretess_length1 = census.frame.smodel_pretess_length1;
        guard.smodel_pretess_skip = census.frame.smodel_pretess_skip;
        guard.submit_cause = census.frame.submit_cause.clone();
        guard.submit_cause2 = census.frame.submit_cause2.clone();
        guard.gpu_not_ready_n = census.frame.gpu_not_ready_n;
        guard.gpu_no_port_n = census.frame.gpu_no_port_n;
        guard.pnr_smodel_mat = census.frame.pnr_smodel_mat.clone();
        guard.pnr_world_mat = census.frame.pnr_world_mat.clone();
        guard.pnr_smodel_ps = census.frame.pnr_smodel_ps.clone();
        guard.pnr_world_ps = census.frame.pnr_world_ps.clone();
        guard.pnr_smodel_key_n = census.frame.pnr_smodel_key_n;
        guard.pnr_world_key_n = census.frame.pnr_world_key_n;
        guard.pnr_port_n = census.frame.pnr_port_n;
        guard.gpu_smodel_bind_mat = census.frame.gpu_smodel_bind_mat.clone();
    }
}
