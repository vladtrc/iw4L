mod controller;
mod intent;
mod memory;
mod motor;
mod nav;
mod observation;
mod plugin;
mod query;
mod roster;
mod sensor;
mod support;
mod task;
pub mod unique_loadout;

#[cfg(test)]
mod tests;

pub use controller::HostController;
pub use intent::{BotIntent, MotorReport, MoveMode, PathOutcome};
pub use nav::{
    NAV_HULL, NAV_SCHEMA, NavGraph, PathError, bake, bake_seeded, brush_bounds, find_path,
    playable_bounds, roam_node,
};
pub use observation::{
    BotEvent, BotObservation, Contact, DEFAULT_OBJECTIVE_RADIUS, KnowledgeSource, ModeObjective,
    SelfState, Stance, WeaponClass,
};
pub use plugin::BotsPlugin;
pub use query::{
    Budgeted, HullTrace, ObstacleKind, SightSample, TraceBudget, WalkSample, WorldQuery,
};
pub use roster::{
    BotAddQueue, BotClassPool, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpRequest,
    BotTpTarget, BotTpWhere, MAX_HOST_BOTS,
};
pub use sensor::observe;
pub use support::{MapModeTell, Traversal, V1_TELLS};
pub use task::{SwitchReason, Task, TaskKind};
