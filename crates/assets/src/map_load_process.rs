//! The load itself, as something that outlives the screen drawn over it.
//!
//! One request, one process. The overlay reads it; the overlay coming down or
//! never existing does not change what the load does, what it waits for or what
//! it reports. A new request takes a new process with its own [`LoadProgress`],
//! so a task still finishing the map before it writes into state nobody reads
//! rather than into the rows of the map that replaced it.

use std::time::Instant;

use bevy::prelude::Resource;

use crate::progress::{LoadProgress, StageHandle, StageId};

#[derive(Resource)]
pub struct MapLoadProcess {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub progress: LoadProgress,
    requested_at: Instant,
    /// Opened once local preparation is done and the session has still not let
    /// this client continue, so the table can name what is holding the load.
    admission: Option<StageHandle>,
    complete: bool,
}

impl MapLoadProcess {
    pub fn new(
        request_id: u64,
        load_key: frame::LocalLoadKey,
        zone: impl Into<String>,
        progress: LoadProgress,
    ) -> Self {
        let requested_at = progress.requested_at();
        Self {
            request_id,
            load_key,
            zone: zone.into(),
            progress,
            requested_at,
            admission: None,
            complete: false,
        }
    }

    pub fn requested_at(&self) -> Instant {
        self.requested_at
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Local preparation is done and the session has not admitted this client
    /// yet. The wait is its own stage, not a gap between two other rows.
    pub fn await_admission(&mut self) {
        if self.complete || self.admission.is_some() {
            return;
        }
        self.admission = Some(self.progress.begin(StageId::Admission, None));
    }

    /// The client is in and the world is installed. The heap maintenance that
    /// follows belongs to the load, not to whichever surface was watching.
    pub fn finish(&mut self) {
        if self.complete {
            return;
        }
        self.complete = true;
        if let Some(stage) = self.admission.take() {
            stage.done();
        } else {
            self.progress.record_skipped(StageId::Admission);
        }
        let installed_ms = self.requested_at.elapsed().as_secs_f32() * 1000.0;
        let trim_ms = diag::release_freed_heap().as_secs_f32() * 1000.0;
        let with_trim_ms = self.requested_at.elapsed().as_secs_f32() * 1000.0;
        let rss = crate::process_resident_bytes()
            .map(|bytes| format!(" rss_mib={}", bytes >> 20))
            .unwrap_or_default();
        let heap = diag::process_live_heap_bytes()
            .map(|bytes| format!(" heap_mib={}", bytes >> 20))
            .unwrap_or_default();
        diag::info!(
            World,
            "load complete: {installed_ms:.0}ms overlay-to-world-installed (+trim {with_trim_ms:.0}ms) for `{}`{rss}{heap} trim={trim_ms:.1}ms",
            self.zone
        );
    }

    /// The session refused this client. The wait ends as a failure; whatever the
    /// load already recorded stays readable.
    pub fn fail(&mut self) {
        if self.complete {
            return;
        }
        self.complete = true;
        if let Some(stage) = self.admission.take() {
            stage.fail();
        }
    }

    pub fn abandon(&mut self) {
        if let Some(stage) = self.admission.take() {
            stage.cancel();
        }
    }
}
