use bevy::prelude::*;
use frame::WorldGeneration;

use super::spawn::{WorldSpawnJob, WorldSpawnPhase};
use std::collections::HashSet;
use std::sync::Arc;

use crate::assemble::drawsurf::{FpvDrawPlan, MaterialGeneration};

pub(crate) const GPU_QUIET_FRAMES: u32 = 2;

pub(crate) fn overlay_is_quiet(ready: bool, quiet: u32) -> bool {
    ready && quiet >= GPU_QUIET_FRAMES
}

#[derive(Default)]
pub struct WorldGpuWait {
    started: Option<std::time::Instant>,
    quiet: u32,
    stage: Option<assets::LoadStage>,
    images_stage: Option<assets::LoadStage>,
    pipelines_stage: Option<assets::LoadStage>,
}

impl WorldGpuWait {
    pub fn arm(&mut self, loading: Option<&assets::LoadingScreen>) {
        self.started = Some(std::time::Instant::now());
        self.quiet = 0;
        if let Some(progress) = loading.map(|screen| &screen.progress) {
            self.images_stage = Some(progress.stage("preparing GPU textures"));
            self.pipelines_stage = Some(progress.stage("compiling render pipelines"));
            let stage = progress.stage("waiting for complete render frames");
            stage.total(u64::from(GPU_QUIET_FRAMES));
            self.stage = Some(stage);
        }
    }

    pub(crate) fn quiet(&self) -> u32 {
        self.quiet
    }

    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.started
            .map(|at| at.elapsed())
            .unwrap_or(std::time::Duration::ZERO)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OverlayWarmup {
    pub generation: WorldGeneration,
    pub initialized: bool,
    pub total: u32,
    pub ready: u32,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct WorldGpuReady {
    pub spawn: WorldGeneration,
    pub images: bool,
    pub pipelines: bool,
    pub waiting_n: u32,
    pub pipeline_n: u32,
    pub warmup: OverlayWarmup,

    pub working_hits: u32,
    pub working_not_ready: u32,
    pub image_ready_n: u32,
    pub image_need_n: u32,
}

impl WorldGpuReady {
    fn live_for(&self, spawn: WorldGeneration) -> bool {
        self.spawn.0.is_some() && self.spawn == spawn && self.images && self.pipelines
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct GpuSubmitDemand {
    pub world_generation: WorldGeneration,

    pub warm_pipelines: bool,

    pub overlay_gpu_wait: bool,

    pub wants_residency: bool,
    pub pipeline_world_materials: Arc<HashSet<u16>>,
    pub pipeline_smodel_materials: Arc<HashSet<u16>>,
    pub pipeline_demand_revision: u64,
}

/// Producer of the working pipeline demand set. Spawn snapshots are adopted by
/// pointer identity; live FPV ids are inserted only when the set actually grows.
#[derive(Resource, Clone, Debug, Default)]
pub struct PipelineDemandTracker {
    spawn_world: Arc<HashSet<u16>>,
    spawn_smodel: Arc<HashSet<u16>>,
    working_world: Arc<HashSet<u16>>,
    revision: u64,
}

impl PipelineDemandTracker {
    fn sync_spawn(&mut self, world: Arc<HashSet<u16>>, smodel: Arc<HashSet<u16>>) {
        if Arc::ptr_eq(&self.spawn_world, &world) && Arc::ptr_eq(&self.spawn_smodel, &smodel) {
            return;
        }
        self.spawn_world = world.clone();
        self.spawn_smodel = smodel;
        self.working_world = world;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn absorb_fpv(&mut self, fpv: &FpvDrawPlan, runtime: &MaterialGeneration) {
        let mut extra = Vec::new();
        for material in fpv.materials() {
            let Some(ordinal) = material.material_sorted_index else {
                continue;
            };
            let Some(row) = runtime.catalog.material_for_sorted_ordinal(ordinal) else {
                continue;
            };
            let id = row.asset_id.0;
            if !self.working_world.contains(&id) {
                extra.push(id);
            }
        }
        if extra.is_empty() {
            return;
        }
        let mut set = (*self.working_world).clone();
        set.extend(extra);
        self.working_world = Arc::new(set);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn snapshot(&self) -> (Arc<HashSet<u16>>, Arc<HashSet<u16>>, u64) {
        (
            self.working_world.clone(),
            self.spawn_smodel.clone(),
            self.revision,
        )
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct GpuLoadProgress {
    pub generation: WorldGeneration,
    pub waiting_n: u32,
    pub pipeline_n: u32,
    pub warmup: OverlayWarmup,

    pub working_hits: u32,
    pub working_not_ready: u32,

    pub pending_shaders: Vec<bevy::asset::AssetId<bevy::shader::Shader>>,

    pub residency: Option<GpuImageResidency>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GpuImageResidency {
    pub need: u32,
    pub ready: u32,
}

pub fn spawn_gpu_extract_live(
    gpu_spawn: WorldGeneration,
    job_spawn: Option<WorldGeneration>,
    overlay_gpu_wait: bool,
    world_gen: WorldGeneration,
) -> bool {
    world_gen.0.is_some()
        && gpu_spawn == world_gen
        && job_spawn == Some(world_gen)
        && overlay_gpu_wait
}

pub(crate) fn publish_gpu_submit_demand(
    spawn: Option<Res<WorldSpawnJob>>,
    generation: Option<Res<WorldGeneration>>,
    ready: Res<WorldGpuReady>,
    mut tracker: ResMut<PipelineDemandTracker>,
    mut demand: ResMut<GpuSubmitDemand>,
) {
    let warm_pipelines = spawn.as_ref().is_some_and(|job| {
        matches!(
            job.phase,
            WorldSpawnPhase::Images
                | WorldSpawnPhase::Plan
                | WorldSpawnPhase::WorldTess
                | WorldSpawnPhase::Gpu
        )
    });
    let world_generation = generation.map(|g| *g).unwrap_or_default();
    let overlay_gpu_wait = spawn
        .as_ref()
        .is_some_and(|job| job.phase == WorldSpawnPhase::Gpu);
    let spawn_world = spawn
        .as_ref()
        .map(|job| job.images.pipeline_world_materials.clone())
        .unwrap_or_default();
    let spawn_smodel = spawn
        .as_ref()
        .map(|job| job.images.pipeline_smodel_materials.clone())
        .unwrap_or_default();
    tracker.sync_spawn(spawn_world, spawn_smodel);
    let (pipeline_world_materials, pipeline_smodel_materials, pipeline_demand_revision) =
        tracker.snapshot();
    *demand = GpuSubmitDemand {
        world_generation,
        warm_pipelines,
        overlay_gpu_wait,
        wants_residency: spawn_gpu_extract_live(
            ready.spawn,
            spawn.as_ref().map(|job| job.spawn),
            overlay_gpu_wait,
            world_generation,
        ),
        pipeline_world_materials,
        pipeline_smodel_materials,
        pipeline_demand_revision,
    };
}

pub(crate) fn poll(
    wait: &mut WorldGpuWait,
    spawn: WorldGeneration,
    gpu: Option<&WorldGpuReady>,
    gap_ms: f32,
) -> bool {
    if let Some(gpu) = gpu {
        if let Some(stage) = &wait.images_stage {
            stage.total(u64::from(gpu.image_need_n));
            stage.set_done(u64::from(gpu.image_ready_n));
        }
        if gpu.images {
            wait.images_stage = None;
        }
        if let Some(stage) = &wait.pipelines_stage {
            stage.total(u64::from(gpu.warmup.total));
            stage.set_done(u64::from(gpu.warmup.ready));
        }
        if gpu.pipelines {
            wait.pipelines_stage = None;
        }
    }
    let ready = gpu.is_some_and(|gpu| gpu.live_for(spawn));
    if ready {
        wait.quiet = wait.quiet.saturating_add(1);
    } else {
        wait.quiet = 0;
    }
    if let Some(stage) = &wait.stage {
        stage.set_done(u64::from(wait.quiet.min(GPU_QUIET_FRAMES)));
    }
    diag::info!(
        World,
        "world spawn slice: phase=gpu quiet={}/{GPU_QUIET_FRAMES} images={} pipelines={} hits={} not_ready={} waiting={} total={} warm={}/{} img={}/{} gap={gap_ms:.1}ms",
        wait.quiet,
        gpu.map(|g| i32::from(g.images)).unwrap_or(-1),
        gpu.map(|g| i32::from(g.pipelines)).unwrap_or(-1),
        gpu.map(|g| g.working_hits as i32).unwrap_or(-1),
        gpu.map(|g| g.working_not_ready as i32).unwrap_or(-1),
        gpu.map(|g| g.waiting_n as i32).unwrap_or(-1),
        gpu.map(|g| g.pipeline_n as i32).unwrap_or(-1),
        gpu.map(|g| g.warmup.ready as i32).unwrap_or(-1),
        gpu.map(|g| g.warmup.total as i32).unwrap_or(-1),
        gpu.map(|g| g.image_ready_n as i32).unwrap_or(-1),
        gpu.map(|g| g.image_need_n as i32).unwrap_or(-1),
    );
    if !overlay_is_quiet(ready, wait.quiet) {
        return false;
    }
    wait.stage = None;
    wait.images_stage = None;
    wait.pipelines_stage = None;
    true
}

pub(crate) fn register_resources(app: &mut App) {
    app.init_resource::<WorldGpuReady>()
        .init_resource::<GpuSubmitDemand>()
        .init_resource::<PipelineDemandTracker>()
        .init_resource::<GpuLoadProgress>()
        .add_systems(
            Update,
            (
                consume_gpu_load_progress.before(super::spawn::spawn_world),
                publish_gpu_submit_demand.after(super::spawn::spawn_world),
            ),
        );
}

pub(crate) fn extract_images_ready(
    mut main_world: ResMut<bevy::render::MainWorld>,
    gpu_images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
) {
    use bevy::asset::RenderAssetUsages;
    let wants_residency = main_world
        .get_resource::<GpuSubmitDemand>()
        .is_some_and(|demand| demand.wants_residency);
    if !wants_residency {
        if let Some(mut progress) = main_world.get_resource_mut::<GpuLoadProgress>() {
            progress.residency = None;
        }
        return;
    }
    let residency = {
        let images = main_world.resource::<Assets<Image>>();
        let mut counts = GpuImageResidency::default();
        for (id, image) in images.iter() {
            if !image.asset_usage.contains(RenderAssetUsages::RENDER_WORLD) {
                continue;
            }
            counts.need = counts.need.saturating_add(1);
            if gpu_images.get(id).is_some() {
                counts.ready = counts.ready.saturating_add(1);
            }
        }
        counts
    };
    if let Some(mut progress) = main_world.get_resource_mut::<GpuLoadProgress>() {
        progress.residency = Some(residency);
    }
}

pub(crate) fn consume_gpu_load_progress(
    progress: Res<GpuLoadProgress>,
    generation: Option<Res<WorldGeneration>>,
    demand: Res<GpuSubmitDemand>,
    mut job: Option<ResMut<WorldSpawnJob>>,
    mut ready: ResMut<WorldGpuReady>,
) {
    if let Some(job) = job.as_mut() {
        job.request_pending_shaders(&progress.pending_shaders);
    }
    let world_gen = generation.map(|g| *g).unwrap_or(WorldGeneration(None));
    ready.waiting_n = progress.waiting_n;
    ready.pipeline_n = progress.pipeline_n;
    ready.warmup = if progress.generation == world_gen {
        progress.warmup
    } else {
        OverlayWarmup::default()
    };
    ready.working_hits = progress.working_hits;
    ready.working_not_ready = progress.working_not_ready;
    match progress.residency {
        Some(counts) => {
            ready.image_need_n = counts.need;
            ready.image_ready_n = counts.ready;
            ready.images = counts.need > 0 && counts.ready == counts.need;
        }
        None => ready.images = false,
    }
    let job_phase_live = spawn_gpu_extract_live(
        ready.spawn,
        job.as_ref().map(|job| job.spawn),
        demand.overlay_gpu_wait,
        world_gen,
    );
    ready.pipelines = job_phase_live
        && progress.warmup.initialized
        && progress.generation == world_gen
        && progress.warmup.ready == progress.warmup.total
        && progress.waiting_n == 0
        && progress.working_hits > 0
        && progress.working_not_ready == 0;
}
