#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotifyKind {
    Connected,

    SpawnedPlayer,

    PrematchDone,

    LastAlive,

    RoundSwitch,

    RoundWin,

    GameWin,

    GameEnded,

    MatchEndingSoon,

    MatchEndingVerySoon,

    ShowingFinalKillcam,

    Disconnect,

    PlayLeaderDialogOnPlayer,
}

impl NotifyKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            NotifyKind::Connected => "connected",
            NotifyKind::SpawnedPlayer => "spawned_player",
            NotifyKind::PrematchDone => "prematch_done",
            NotifyKind::LastAlive => "last_alive",
            NotifyKind::RoundSwitch => "round_switch",
            NotifyKind::RoundWin => "round_win",
            NotifyKind::GameWin => "game_win",
            NotifyKind::GameEnded => "game_ended",
            NotifyKind::MatchEndingSoon => "match_ending_soon",
            NotifyKind::MatchEndingVerySoon => "match_ending_very_soon",
            NotifyKind::ShowingFinalKillcam => "showing_final_killcam",
            NotifyKind::Disconnect => "disconnect",
            NotifyKind::PlayLeaderDialogOnPlayer => "playLeaderDialogOnPlayer",
        }
    }

    pub const fn is_level_scoped(self) -> bool {
        matches!(
            self,
            NotifyKind::Connected
                | NotifyKind::PrematchDone
                | NotifyKind::LastAlive
                | NotifyKind::RoundSwitch
                | NotifyKind::RoundWin
                | NotifyKind::GameWin
                | NotifyKind::GameEnded
                | NotifyKind::MatchEndingSoon
                | NotifyKind::MatchEndingVerySoon
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndingReason {
    Time,

    Score,
}

impl MatchEndingReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            MatchEndingReason::Time => "time",
            MatchEndingReason::Score => "score",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundSwitch {
    Halftime,
    Overtime,
    Other,
}

impl RoundSwitch {
    pub const fn dialog_key(self) -> &'static str {
        match self {
            RoundSwitch::Halftime => "halftime",
            RoundSwitch::Overtime => "overtime",
            RoundSwitch::Other => "side_switch",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Winner {
    Team(crate::output::Team),
    Player(crate::output::ClientId),
    Undefined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notify {
    Connected(crate::output::ClientId),
    SpawnedPlayer,
    PrematchDone,
    LastAlive(crate::output::ClientId),
    RoundSwitch(RoundSwitch),
    RoundWin(Winner),
    GameWin(Winner),
    GameEnded,
    MatchEndingSoon(MatchEndingReason),
    MatchEndingVerySoon,
    ShowingFinalKillcam,
    Disconnect,
    PlayLeaderDialogOnPlayer,
}

impl Notify {
    pub const fn kind(self) -> NotifyKind {
        match self {
            Notify::Connected(_) => NotifyKind::Connected,
            Notify::SpawnedPlayer => NotifyKind::SpawnedPlayer,
            Notify::PrematchDone => NotifyKind::PrematchDone,
            Notify::LastAlive(_) => NotifyKind::LastAlive,
            Notify::RoundSwitch(_) => NotifyKind::RoundSwitch,
            Notify::RoundWin(_) => NotifyKind::RoundWin,
            Notify::GameWin(_) => NotifyKind::GameWin,
            Notify::GameEnded => NotifyKind::GameEnded,
            Notify::MatchEndingSoon(_) => NotifyKind::MatchEndingSoon,
            Notify::MatchEndingVerySoon => NotifyKind::MatchEndingVerySoon,
            Notify::ShowingFinalKillcam => NotifyKind::ShowingFinalKillcam,
            Notify::Disconnect => NotifyKind::Disconnect,
            Notify::PlayLeaderDialogOnPlayer => NotifyKind::PlayLeaderDialogOnPlayer,
        }
    }
}
