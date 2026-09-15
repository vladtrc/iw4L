//! What the run reached, and when. One `Last` system watches the resources the
//! load transaction publishes and stamps the first frame each became true; the
//! report turns those stamps into the waterfall.

use std::path::{Path, PathBuf};
use std::time::Instant;

use assets::{LoadLaneTiming, LoadProgress, LoadingScreen, MatchLoadBusy, MatchLoadRequest};
use audio::MapAmbientBooted;
use bevy::prelude::*;
use render_frontend::prepare::scene::cull::DpvsFrameStats;
use render_frontend::prepare::scene::world::WorldScene;
use session::MatchInstalled;

/// Consecutive frames with a working colour set before the picture counts as
/// rendered: one frame can hit while a pipeline is still compiling.
const READY_FRAMES: u32 = 2;

pub(crate) struct Milestones {
    pub(crate) zone: String,
    pub(crate) screenshot: PathBuf,
    pub(crate) progress: LoadProgress,
    pub(crate) t0: Instant,
    pub(crate) request: Option<Instant>,
    pub(crate) installed: Option<Instant>,
    pub(crate) spawned: Option<Instant>,
    pub(crate) overlay_down: Option<Instant>,
    pub(crate) ingame: Option<Instant>,
    pub(crate) rendered_spawn: Option<Instant>,
    pub(crate) ambient: Option<Instant>,
    pub(crate) shot: Option<Instant>,
    pub(crate) lanes: Vec<LoadLaneTiming>,
    pub(crate) batches_at_shot: Option<u32>,
    pub(crate) g0_at_shot: Option<u32>,
    overlay_seen: bool,
    ready_frames: u32,
    last_batches: Option<u32>,
    last_g0: Option<u32>,
    pub(crate) last_spawned: bool,
}

impl Milestones {
    pub(crate) fn new(zone: String, screenshot: PathBuf, progress: LoadProgress) -> Self {
        Self {
            zone,
            screenshot,
            progress,
            t0: Instant::now(),
            request: None,
            installed: None,
            spawned: None,
            overlay_down: None,
            ingame: None,
            rendered_spawn: None,
            ambient: None,
            shot: None,
            lanes: Vec::new(),
            batches_at_shot: None,
            g0_at_shot: None,
            overlay_seen: false,
            ready_frames: 0,
            last_batches: None,
            last_g0: None,
            last_spawned: false,
        }
    }

    /// The first moment the map was drawn and nothing was left in front of it.
    /// It is where the load report stops and the frame report starts.
    ///
    /// The drawn frame is what decides; the overlay only pushes the moment
    /// later when it outlives that frame. It cannot be required: a run that
    /// never raised an overlay at all would otherwise file every frame it
    /// drew as load cost.
    pub(crate) fn playable(&self) -> Option<Instant> {
        let rendered = self.rendered_spawn?;
        Some(match self.overlay_down {
            Some(overlay) => rendered.max(overlay),
            None => rendered,
        })
    }

    /// Lanes as of now. Called once the walk is done and again at exit, so a
    /// stage that closed late is still in the report.
    pub(crate) fn take_lane_snapshot(&mut self) {
        let lanes = self.progress.lane_timings();
        if lanes.len() >= self.lanes.len() {
            self.lanes = lanes;
        }
    }

    pub(crate) fn screenshot_bytes(&self) -> Option<u64> {
        std::fs::metadata(&self.screenshot)
            .ok()
            .map(|meta| meta.len())
            .filter(|len| *len > 0)
    }
}

static MILESTONES: std::sync::Mutex<Option<Milestones>> = std::sync::Mutex::new(None);

pub(crate) fn install(state: Milestones) {
    *MILESTONES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(state);
}

pub(crate) fn with<T>(read: impl FnOnce(&mut Milestones) -> T) -> Option<T> {
    let mut guard = MILESTONES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    guard.as_mut().map(read)
}

pub(crate) fn screenshot_path(artifacts: &Path, zone: &str) -> PathBuf {
    artifacts
        .join("screenshots")
        .join("bench")
        .join(format!("{}.png", sanitize(zone)))
}

fn sanitize(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    for ch in zone.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_start_matches('.');
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn poll(
    request: Option<Res<MatchLoadRequest>>,
    busy: Option<Res<MatchLoadBusy>>,
    mut installed: MessageReader<MatchInstalled>,
    scene: Option<Res<WorldScene>>,
    loading: Option<Res<LoadingScreen>>,
    screen: Option<Res<ui::AppScreen>>,
    ambient: Option<Res<MapAmbientBooted>>,
    stats: Option<Res<DpvsFrameStats>>,
    working: Option<Res<render::ColourWorkingSet>>,
) {
    let now = Instant::now();
    let saw_request = request.is_some() || busy.is_some_and(|busy| busy.0);
    let saw_installed = installed.read().next().is_some();
    let spawned_now = scene.as_deref().is_some_and(|scene| scene.spawned);
    let overlay_now = loading.is_some();
    let ingame_now = screen.is_some_and(|screen| matches!(*screen, ui::AppScreen::InGame));
    let ambient_now = ambient.is_some_and(|ambient| ambient.0);
    let census = stats
        .as_deref()
        .map(|stats| (stats.submitted_batches, stats.g0_world_surfs.len() as u32));
    let ready_now = working
        .as_ref()
        .is_some_and(|set| set.hits > 0 && set.pipeline_not_ready == 0);

    with(|bench| {
        if bench.request.is_none() && saw_request {
            bench.request = Some(now);
        }
        if bench.installed.is_none() && saw_installed {
            bench.installed = Some(now);
            bench.take_lane_snapshot();
        }
        if spawned_now {
            bench.last_spawned = true;
            if bench.spawned.is_none() {
                bench.spawned = Some(now);
                bench.take_lane_snapshot();
            }
        }
        if overlay_now {
            bench.overlay_seen = true;
        } else if bench.overlay_down.is_none() && bench.overlay_seen {
            bench.overlay_down = Some(now);
        }
        if bench.ingame.is_none() && ingame_now {
            bench.ingame = Some(now);
        } else if ingame_now && bench.rendered_spawn.is_none() {
            bench.ready_frames = if ready_now { bench.ready_frames + 1 } else { 0 };
            if bench.ready_frames >= READY_FRAMES {
                bench.rendered_spawn = Some(now);
            }
        }
        if bench.ambient.is_none() && ambient_now {
            bench.ambient = Some(now);
        }
        if let Some((batches, g0)) = census {
            bench.last_batches = Some(batches);
            bench.last_g0 = Some(g0);
        }
        if bench.shot.is_none() && bench.screenshot_bytes().is_some() {
            bench.shot = Some(now);
            bench.batches_at_shot = bench.last_batches;
            bench.g0_at_shot = bench.last_g0;
        }
        // Everything recorded from the first playable frame on is gameplay, and
        // belongs to the frame report rather than to the cost of the load.
        if bench.playable().is_some() {
            perf::stats::mark_live();
        }
    });
}
