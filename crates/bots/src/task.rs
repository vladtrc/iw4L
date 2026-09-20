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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionStage {
    #[default]
    Approach,
    Position,
    Interact,
    Hold,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decision {
    /// Hunt, investigate, fight, recover, objective utilities, in stable tie order.
    pub scores: [f32; 5],
    pub stage: ActionStage,
    pub objective: Option<crate::observation::ModeObjective>,
}
