#![no_std]
#![forbid(unsafe_code)]

pub mod camtime;
pub mod cancel;
pub mod cleanup;
pub mod final_killcam;
pub mod focus;
pub mod log;
pub mod notify;
pub mod round_end;
pub mod task;
pub mod timeline;
pub mod watch;

pub use camtime::{
    ARCHIVE_TRIM_EPSILON, CamtimeBranch, CamtimeInput, DEFAULT_POSTDELAY_SECONDS,
    MIN_MAXTIME_SECONDS, MaxtimeTrim, Recalc, SERVER_FRAME_MS, SERVER_FRAME_SECONDS, Window,
    WindowPlan, camtime, plan_window, postdelay, recalc_after_first_frame,
};
pub use cancel::{CancelTick, DOUBLE_TAP_LIMIT_SECONDS, DoubleTap, SkipEdge};
pub use cleanup::{CLEANUP_NOTIFY, CleanupEntry, CleanupStep, killcam_cleanup_steps};
pub use final_killcam::{
    FINAL_KILLCAM_MAXTIME_SECONDS, FinalKillcamConfig, FinalKillcamLog, FinalKillcamOutput,
    FinalKillcamSequence, FinalKillcamStart,
};
pub use focus::{
    Entity, FIRST_RECHECK_DELAY_SECONDS, FocusDelay, FocusRule, Inflictor, NO_KILLCAM_ENTITY,
    archive_clock_ms, focus_first_check, focus_second_check, get_killcam_entity,
    killcam_entity_index,
};
pub use notify::NotifyKind;
pub use round_end::{
    POST_ROUND_TIME_MS, ROUND_END_DELAY_MS, RoundEndWaitConfig, RoundEndWaitLog,
    RoundEndWaitOutput, RoundEndWaitSequence,
};
pub use task::{
    EndOn, Event, Millis, ResumeError, Scheduler, SpawnError, Task, TaskId, Wake, Woke,
};
pub use timeline::{
    DeathConfig, DeathLog, DeathOutput, DeathSequence, NoKillcam, ONE_SECOND_MS, QUARTER_SECOND_MS,
    StartKillcam,
};
pub use watch::{NothingToShow, end_killcam_if_nothing_to_show};
