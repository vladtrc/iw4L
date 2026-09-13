use bevy::prelude::*;

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeginKillcam {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnedPlayer {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbortKillcam {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamEnded {
    pub entity: Entity,
}
