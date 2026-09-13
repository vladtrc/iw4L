use sim::Tick;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateClass {
    Authoritative,

    Predicted,

    PresentationOnly,

    DeclaredGap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorrectionRule {
    AdoptSnapshot,
    ReplayPendingInputs,
    PreserveOutsideSimulation,
    AppendMonotonicHistory,
    FollowDeclaredAdoptGap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateContractRow {
    pub state: &'static str,
    pub class: StateClass,
    pub correction: CorrectionRule,
}

pub const STATE_CONTRACT: &[StateContractRow] = &[
    StateContractRow {
        state: "resolved Snapshot and snapshot-restorable SimWorld state",
        class: StateClass::Authoritative,
        correction: CorrectionRule::AdoptSnapshot,
    },
    StateContractRow {
        state: "unacknowledged MoveHistory and predicted local state",
        class: StateClass::Predicted,
        correction: CorrectionRule::ReplayPendingInputs,
    },
    StateContractRow {
        state: "predictedError, interpolation, camera, audio, FX, and delivery cursors",
        class: StateClass::PresentationOnly,
        correction: CorrectionRule::PreserveOutsideSimulation,
    },
    StateContractRow {
        state: "FrameArchive of resolved authoritative snapshots",
        class: StateClass::Authoritative,
        correction: CorrectionRule::AppendMonotonicHistory,
    },
    StateContractRow {
        state: "SimWorld residue enumerated by sim::ADOPT_GAPS",
        class: StateClass::DeclaredGap,
        correction: CorrectionRule::FollowDeclaredAdoptGap,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityContractRow {
    pub subject: &'static str,
    pub identity: &'static str,
    pub lifetime: &'static str,
}

pub const IDENTITY_CONTRACT: &[IdentityContractRow] = &[
    IdentityContractRow {
        subject: "player life",
        identity: "(ClientId, LifeSequence)",
        lifetime: "accepted spawn through Died; respawn increments LifeSequence",
    },
    IdentityContractRow {
        subject: "dynamic entity and archived entity",
        identity: "EntityRef(number, generation)",
        lifetime: "G_Spawn allocation through free; reuse keeps number and increments generation",
    },
    IdentityContractRow {
        subject: "authoritative projectile",
        identity: "(ProjectileId, EntityRef)",
        lifetime: "launch through terminal event retention/free; predicted ProjectileId never escapes",
    },
    IdentityContractRow {
        subject: "presentation entity event",
        identity: "EventSequence",
        lifetime: "one authoritative emission; duplicate snapshots do not reopen delivery",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorrectionBoundary {
    pub snapshot_tick: Tick,

    pub replay_tick: Tick,
}

impl CorrectionBoundary {
    pub const fn after(snapshot_tick: Tick) -> Self {
        Self {
            snapshot_tick,
            replay_tick: Tick(snapshot_tick.0.wrapping_add(1)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotOrder {
    New,
    Duplicate,
    Stale,
}

pub const fn classify_snapshot(last: Option<Tick>, incoming: Tick) -> SnapshotOrder {
    match last {
        None => SnapshotOrder::New,
        Some(last) if incoming.0 > last.0 => SnapshotOrder::New,
        Some(last) if incoming.0 == last.0 => SnapshotOrder::Duplicate,
        Some(_) => SnapshotOrder::Stale,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideEffectReplayRule {
    Recompute,

    SuppressPredicted,

    DeduplicateAuthoritative,

    Resample,

    RefuseUnwired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideEffectContractRow {
    pub output: &'static str,
    pub rule: SideEffectReplayRule,
}

pub const SIDE_EFFECT_CONTRACT: &[SideEffectContractRow] = &[
    SideEffectContractRow {
        output: "gameplay mutation: projectile flight, damage, lifecycle, score",
        rule: SideEffectReplayRule::Recompute,
    },
    SideEffectContractRow {
        output: "entity events produced by a prediction-world step",
        rule: SideEffectReplayRule::SuppressPredicted,
    },
    SideEffectContractRow {
        output: "authoritative EntityEventRecord keyed by EventSequence",
        rule: SideEffectReplayRule::DeduplicateAuthoritative,
    },
    SideEffectContractRow {
        output: "reliable control journal: encoded but not consumed by the client",
        rule: SideEffectReplayRule::RefuseUnwired,
    },
    SideEffectContractRow {
        output: "camera, interpolation, loop audio, and other continuous presentation",
        rule: SideEffectReplayRule::Resample,
    },
];
