use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndingReason {
    Time,

    Score,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchEndingSoon {
    pub reason: MatchEndingReason,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchEndingVerySoon;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEnded;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrematchDone;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameWinner {
    Allies,

    Axis,

    Player(u32),

    Undefined,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameWin {
    pub winner: GameWinner,
}

/// Only fires when the round that just ended was not the last one.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundWin {
    pub winner: GameWinner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundSwitchKind {
    Halftime,

    Overtime,

    Other,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundSwitch {
    pub kind: RoundSwitchKind,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawningIntermission;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitLevelCalled;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnedPlayerNotify {
    pub client: u32,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassDestroyed {
    pub piece: u32,
}

pub fn register_script_notify(app: &mut App) {
    app.add_message::<MatchEndingSoon>()
        .add_message::<MatchEndingVerySoon>()
        .add_message::<GameEnded>()
        .add_message::<PrematchDone>()
        .add_message::<GameWin>()
        .add_message::<RoundWin>()
        .add_message::<RoundSwitch>()
        .add_message::<SpawningIntermission>()
        .add_message::<ExitLevelCalled>()
        .add_message::<SpawnedPlayerNotify>()
        .add_message::<GlassDestroyed>();
}
