use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::prelude::Resource;

use crate::progress::LoadProgress;

#[derive(Resource)]
pub struct LoadingScreen {
    pub progress: LoadProgress,
    pub(crate) title: String,
    pub(crate) mode_label: String,
    pub(crate) complete: bool,
    failure: Option<String>,

    pub(crate) spawned_at: Instant,

    pub(crate) elapsed: Duration,
    pub(crate) complete_at: Option<Instant>,

    pub(crate) since_complete: Duration,
    pub(crate) preview_ready: bool,
}

impl LoadingScreen {
    pub fn new(
        progress: LoadProgress,
        title: impl Into<String>,
        mode_label: impl Into<String>,
    ) -> Self {
        Self {
            progress,
            title: title.into(),
            mode_label: mode_label.into(),
            complete: false,
            failure: None,
            spawned_at: Instant::now(),
            elapsed: Duration::ZERO,
            complete_at: None,
            since_complete: Duration::ZERO,
            preview_ready: false,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn mode_label(&self) -> &str {
        &self.mode_label
    }

    /// The overlay's own bookkeeping only. What the load does when it
    /// completes belongs to [`crate::MapLoadProcess`], which is still there
    /// when no overlay is.
    pub fn finish(&mut self) {
        if !self.complete && self.failure.is_none() {
            self.complete = true;
            self.complete_at = Some(Instant::now());
            self.since_complete = Duration::ZERO;
        }
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.failure = Some(reason.into());
        self.complete = false;
        self.complete_at = None;
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn mark_preview_ready(&mut self) {
        self.preview_ready = true;
    }

    pub fn preview_ready(&self) -> bool {
        self.preview_ready
    }

    pub fn tick_elapsed(&mut self, _delta: Duration) {
        self.elapsed = self.spawned_at.elapsed();
        if let Some(at) = self.complete_at {
            self.since_complete = at.elapsed();
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn since_complete(&self) -> Duration {
        self.since_complete
    }
}

#[derive(Resource, Clone, Debug)]
pub struct LoadingPreviewSource {
    pub path: PathBuf,
    pub map_name: String,

    pub request_id: u64,
}
