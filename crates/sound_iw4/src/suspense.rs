use killcam_iw4::log::Log;
use killcam_iw4::task::Millis;

use crate::level::{Fleet, Level, play_sound_on_players};
use crate::notify::Notify;
use crate::output::Output;
use crate::recipes;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SuspenseStep {
    #[default]
    Waiting,

    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct SuspenseMusic {
    step: SuspenseStep,

    wake_at_ms: Millis,
}

impl SuspenseMusic {
    pub const fn start(now_ms: Millis, wait_ms: Millis) -> Self {
        Self {
            step: SuspenseStep::Waiting,
            wake_at_ms: now_ms + wait_ms,
        }
    }

    pub const fn step(self) -> SuspenseStep {
        self.step
    }

    pub const fn wake_at_ms(self) -> Millis {
        self.wake_at_ms
    }

    pub fn on_notify(&mut self, notify: Notify) {
        if matches!(notify, Notify::GameEnded | Notify::MatchEndingSoon(_)) {
            self.step = SuspenseStep::Done;
        }
    }

    pub fn advance<const N: usize>(
        &mut self,
        now_ms: Millis,
        level: &Level,
        fleet: &Fleet,
        track_index: usize,
        next_wait_ms: Millis,
        out: &mut Log<Output, N>,
    ) {
        if self.step != SuspenseStep::Waiting || now_ms < self.wake_at_ms {
            return;
        }
        if let Some(alias) = recipes::suspense_track(track_index) {
            play_sound_on_players(level, fleet.players, alias, None, &[], out);
        }
        self.wake_at_ms = now_ms + next_wait_ms;
    }
}

pub fn final_killcam_music() {}
