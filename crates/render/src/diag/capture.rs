use std::collections::VecDeque;
use std::path::PathBuf;

use assets::LoadingScreen;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use frame::{AppScreen, HasWorld};

use render_frontend::prepare::scene::cull::DpvsFrameStats;

pub const CAPTURE_SETTLE_FRAMES: u32 = 30;

pub const CAPTURE_WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(180);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRequest {
    pub path: PathBuf,

    pub exit_after_capture: bool,
}

impl CaptureRequest {
    pub fn from_env() -> Option<CaptureRequest> {
        let path = std::env::var_os("IW4L_SCREENSHOT")?;
        Some(CaptureRequest {
            path: PathBuf::from(path),
            exit_after_capture: false,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureFrameFacts {
    pub screen: AppScreen,

    pub loading_overlay: bool,

    pub has_world: bool,

    pub submitted_batches: u32,

    pub g0_world: u32,
}

impl CaptureFrameFacts {
    pub fn content_up(self) -> bool {
        self.obstruction().is_none()
    }

    pub fn world_submitted(self) -> bool {
        self.submitted_batches > 0 || self.g0_world > 0
    }

    fn obstruction(self) -> Option<CaptureWait> {
        if self.screen == AppScreen::Loading {
            return Some(CaptureWait::MatchLoading);
        }
        if self.loading_overlay {
            return Some(CaptureWait::LoadingOverlayUp);
        }

        if self.has_world && !self.world_submitted() {
            return Some(CaptureWait::WorldNotSubmitted);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureWait {
    MatchLoading,

    LoadingOverlayUp,

    WorldNotSubmitted,

    Settling { frames_left: u32 },
}

impl CaptureWait {
    pub fn label(self) -> String {
        match self {
            CaptureWait::MatchLoading => "match still loading".to_owned(),
            CaptureWait::LoadingOverlayUp => "loading overlay still drawn".to_owned(),
            CaptureWait::WorldNotSubmitted => "world handed over, no batch submitted".to_owned(),
            CaptureWait::Settling { frames_left } => {
                format!("settling ({frames_left} frames left)")
            }
        }
    }
}

pub fn capture_wait(facts: CaptureFrameFacts, settled_frames: u32) -> Option<CaptureWait> {
    facts.obstruction().or_else(|| {
        (settled_frames < CAPTURE_SETTLE_FRAMES).then(|| CaptureWait::Settling {
            frames_left: CAPTURE_SETTLE_FRAMES - settled_frames,
        })
    })
}

#[derive(Clone, Debug)]
struct PendingCapture {
    request: CaptureRequest,
    waited_frames: u32,
    queued_at: std::time::Instant,

    reported: Option<std::mem::Discriminant<CaptureWait>>,
}

impl PendingCapture {
    fn should_report(&mut self, wait: CaptureWait) -> bool {
        let kind = std::mem::discriminant(&wait);
        let fresh = self.reported != Some(kind);
        self.reported = Some(kind);
        fresh
    }
}

#[derive(Resource, Default)]
pub struct CaptureQueue {
    pending: VecDeque<PendingCapture>,

    settled_frames: u32,

    writing: usize,

    write_completed: bool,

    exit_when_drained: bool,
}

impl CaptureQueue {
    pub fn push(&mut self, request: CaptureRequest) {
        if request.exit_after_capture {
            self.exit_when_drained = true;
        }
        self.pending.push_back(PendingCapture {
            request,
            waited_frames: 0,
            queued_at: std::time::Instant::now(),
            reported: None,
        });
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_paths(&self) -> Vec<PathBuf> {
        self.pending
            .iter()
            .map(|p| p.request.path.clone())
            .collect()
    }

    pub fn exit_after_drained(&mut self) -> bool {
        if self.pending.is_empty() && self.writing == 0 {
            return false;
        }
        self.exit_when_drained = true;
        true
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn capture_frame(
    mut commands: Commands,
    mut queue: ResMut<CaptureQueue>,
    screen: Option<Res<AppScreen>>,
    loading: Option<Res<LoadingScreen>>,
    has_world: Option<Res<HasWorld>>,
    stats: Option<Res<DpvsFrameStats>>,
    working: Option<Res<render_gpu::ColourWorkingSet>>,
    mut exit: MessageWriter<AppExit>,
) {
    let submitted_batches = stats.as_ref().map(|s| s.submitted_batches).unwrap_or(0);
    let g0_world = stats
        .as_ref()
        .map(|s| s.g0_world_surfs.len() as u32)
        .unwrap_or(0);
    let facts = CaptureFrameFacts {
        screen: screen.map(|s| *s).unwrap_or_default(),
        loading_overlay: loading.is_some(),
        has_world: has_world.map(|w| w.0).unwrap_or(false),
        submitted_batches,
        g0_world,
    };

    let pipelines_ready =
        !facts.has_world || working.is_some_and(|set| set.hits > 0 && set.pipeline_not_ready == 0);
    queue.settled_frames = if facts.content_up() && pipelines_ready {
        queue.settled_frames.saturating_add(1)
    } else {
        0
    };

    take_ready_capture(&mut commands, &mut queue, facts);

    if queue.write_completed {
        queue.write_completed = false;
    } else if queue.exit_when_drained && queue.pending.is_empty() && queue.writing == 0 {
        exit.write(AppExit::Success);
    }
}

fn take_ready_capture(commands: &mut Commands, queue: &mut CaptureQueue, facts: CaptureFrameFacts) {
    let settled_frames = queue.settled_frames;
    let Some(front) = queue.pending.front_mut() else {
        return;
    };
    front.waited_frames = front.waited_frames.saturating_add(1);
    let waited = front.waited_frames;

    match capture_wait(facts, settled_frames) {
        Some(wait) => {
            if front.queued_at.elapsed() >= CAPTURE_WAIT_LIMIT {
                let failed = queue.pending.pop_front().expect("front exists");
                diag::error!(
                    Launch,
                    "screenshot: gave up on {} after {} frames — {}",
                    failed.request.path.display(),
                    waited,
                    wait.label()
                );
                return;
            }

            if front.should_report(wait) {
                diag::info!(
                    Launch,
                    "screenshot: holding {} — {}",
                    front.request.path.display(),
                    wait.label()
                );
            }
        }
        None => {
            let ready = queue.pending.pop_front().expect("front exists");
            let path = ready.request.path.clone();
            diag::info!(
                Launch,
                "screenshot: writing {} (screen {:?}, world batches {}, g0_world {}, settled {} frames, \
                 queued {} frames ago)",
                path.display(),
                facts.screen,
                facts.submitted_batches,
                facts.g0_world,
                settled_frames,
                waited
            );
            queue.writing += 1;
            let mut save = save_to_disk(path);
            commands.spawn(Screenshot::primary_window()).observe(
                move |captured: On<ScreenshotCaptured>, mut queue: ResMut<CaptureQueue>| {
                    save(captured);
                    queue.writing -= 1;
                    queue.write_completed = true;
                },
            );
        }
    }
}

pub(crate) fn report_unwritten_captures(
    queue: Res<CaptureQueue>,
    mut exit: MessageReader<AppExit>,
) {
    if exit.read().next().is_none() {
        return;
    }
    for path in queue.pending_paths() {
        diag::error!(
            Launch,
            "screenshot: {} was never written — process exiting with the request queued",
            path.display()
        );
    }
}
