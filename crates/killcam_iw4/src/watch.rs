use crate::camtime::SERVER_FRAME_SECONDS;
use crate::notify::NotifyKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NothingToShow {
    KeepWatching,

    Abort,
}

pub fn end_killcam_if_nothing_to_show(archivetime: f32) -> NothingToShow {
    if archivetime <= 0.0 {
        NothingToShow::Abort
    } else {
        NothingToShow::KeepWatching
    }
}

pub const SAMPLE_PERIOD_SECONDS: f32 = SERVER_FRAME_SECONDS;

pub const ABORT_NOTIFY: NotifyKind = NotifyKind::AbortKillcam;
