pub mod host;
mod runtime;
pub(crate) mod vm;

pub(crate) use bevy_ecs::prelude::Resource;
pub(crate) use std::collections::{BTreeMap, VecDeque};
pub(crate) use std::sync::Arc;

pub(crate) use runtime::Runtime;

pub use gsc::IR_VERSION;
pub use gsc::{Builtin, Catalog, Namespace, Owner};
pub(crate) use gsc::{Callee, Global, Op};
pub use gsc::{Fault, Location};
pub use host::actor_anims::ActorAnimTree;
pub use host::actor_nav::{ActorPaths, NavNode, NavNodeKind};
pub(crate) use host::controls::{
    action_slot_command, command_buttons, player_commands, select_location,
};
pub use host::entities::{
    KeyType, LevelData, StringTable, parse_entity_string, parse_radiant_keys,
};
pub(crate) use host::entity_damage::{
    EntityHit, HitTarget, ScriptBlast, ScriptHit, damage_entity, radius_targets,
};
pub(crate) use host::mechanics::Mechanics;
pub(crate) use host::mechanics::advance_mechanics;
pub use host::natives::engine::{EXIT_LEVEL, MAP_RESTART};
pub(crate) use host::natives::iw4::set_dvar;
pub(crate) use host::players::{
    answer_join, answer_menu, apply_disconnects, apply_player_links, choose_class,
    choose_default_class, disconnect_player, flashbang, force_death, give_killstreak, is_t5,
    note_team_answer, personal_class, player_damage, script_seats, set_profile, sync_players,
};
pub(crate) use host::presence::sync_presence;
pub use host::registry::{Native, NativeRegistry};
pub(crate) use host::restart::restart_level;
pub(crate) use host::weapons::{publish_projectile_launches, sync_engine_events};

/// Advances actor animation one authority tick, and raises timed notifies,
/// before scripts run.
pub(crate) fn advance_actors(world: &mut bevy_ecs::prelude::World) {
    if !world
        .resource::<crate::step::StepRequest>()
        .reason
        .advances_authority_world()
    {
        return;
    }
    let seconds = crate::MATCH_TICK_MS as f32 / 1000.0;
    host::actor_anims::advance(world, seconds);
    host::actor_nav::locomote(world, seconds);
    host::actor_brain::think(world);
    host::natives::engine::deliver_timed_notifies(world);
}
pub(crate) use gsc::ArrayKey;
pub use gsc::{FileSources, SourceOrigin, SourceResolver, decode_source, normalize_module};
pub use gsc::{ModuleIdentity, Program, Realm, Site};
pub use gsc::{ScriptString, Value};
pub(crate) use runtime::{
    advance_scheduler, copy_state, healthy, install, preflight, reset, start, take_signals,
};
pub(crate) use vm::state::{Frame, Thread, ThreadState, Waiter, WaiterKind};

pub(crate) use host::entity_damage::{
    destructible_attacker, destructible_callback, destructible_debris, destructible_effect,
    set_destructible_model,
};
