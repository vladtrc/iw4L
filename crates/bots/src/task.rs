#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskKind {
    #[default]
    Hunt,
    Investigate,
    Fight,
    Recover,
    TouchObj,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchReason {
    Spawned,
    SawEnemy,
    LostSight,
    ObjectiveReachable,
    PathFailed,
    LowHealth,
    ClipEmpty,
    ProgressLost,
    Died,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    pub kind: TaskKind,
    pub reason: SwitchReason,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            kind: TaskKind::Hunt,
            reason: SwitchReason::Spawned,
        }
    }
}
