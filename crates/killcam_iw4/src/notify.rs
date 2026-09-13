#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotifyKind {
    Disconnect,

    Spawned,

    SpawnedPlayer,

    GameEnded,

    BeginKillcam,

    KillcamEnded,

    AbortKillcam,

    DeathDelayFinished,

    ShowingFinalKillcam,

    RoundEndFinished,

    UseCopycat,
}

impl NotifyKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            NotifyKind::Disconnect => "disconnect",
            NotifyKind::Spawned => "spawned",
            NotifyKind::SpawnedPlayer => "spawned_player",
            NotifyKind::GameEnded => "game_ended",
            NotifyKind::BeginKillcam => "begin_killcam",
            NotifyKind::KillcamEnded => "killcam_ended",
            NotifyKind::AbortKillcam => "abort_killcam",
            NotifyKind::DeathDelayFinished => "death_delay_finished",
            NotifyKind::ShowingFinalKillcam => "showing_final_killcam",
            NotifyKind::RoundEndFinished => "round_end_finished",
            NotifyKind::UseCopycat => "use_copycat",
        }
    }

    pub const fn is_level_scoped(self) -> bool {
        matches!(self, NotifyKind::GameEnded | NotifyKind::RoundEndFinished)
    }
}
