use super::{Fault, RoundScript, Value, entry, find_thread, kill, spawn_thread};
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) fn start_thread(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, Fault> {
    let (program, function, location) = entry(world, name)?;
    spawn_thread(world, &program, function, receiver, args).map_err(|m| Fault::at(&location, m))
}

pub(crate) fn start_function(
    world: &mut World,
    function: u32,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, String> {
    let program = world
        .resource::<RoundScript>()
        .program
        .clone()
        .ok_or("no loaded GSC program")?;
    spawn_thread(world, &program, function as usize, receiver, args)
}

pub(crate) fn running(world: &mut World, serial: u64) -> bool {
    find_thread(world, serial).is_some()
}

pub(crate) fn stop_thread(world: &mut World, serial: u64) {
    if let Some(entity) = find_thread(world, serial) {
        kill(world, entity, serial);
    }
}

pub(super) fn advance(world: &mut World, now: i64) {
    super::super::host::script_anim::advance(world);
    super::super::host::actors::advance(world);
    super::super::host::mission::advance(world);
    super::super::host::weapons::campaign::advance_magic_grenades(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut runtime.campaign.timed_notifies)
        .into_iter()
        .partition(|(at, _, _)| *at <= now);
    runtime.campaign.timed_notifies = later;
    runtime.pending_notifies.extend(
        due.into_iter()
            .map(|(_, receiver, name)| (receiver, name, Vec::new())),
    );
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CampaignState {
    pub(crate) deferred_entries: Vec<String>,
    pub(crate) videos: Arc<BTreeMap<String, i32>>,
    pub(crate) turret_ai_scripts: Arc<BTreeMap<String, String>>,
    pub(crate) active: bool,
    pub(crate) blocked_paths: BTreeMap<u64, crate::script::Bounds>,
    pub(crate) badplaces: Vec<crate::script::host::actors::BadPlace>,
    pub(crate) corpses: std::collections::VecDeque<u64>,
    pub(crate) drivers: BTreeMap<u32, u64>,
    pub(crate) ai_listeners: BTreeMap<u64, std::collections::BTreeSet<Arc<str>>>,
    pub(crate) ownerless_missiles: std::collections::BTreeSet<crate::ProjectileId>,
    pub(crate) magic_grenades: Vec<super::super::host::weapons::campaign::MagicGrenade>,
    pub(crate) turret_gunners: BTreeMap<u64, (u64, i64)>,
    pub(crate) turret_users: BTreeMap<u64, (u32, i64)>,
    pub(crate) turret_firing: BTreeMap<u64, i64>,
    pub(crate) turret_seen: BTreeMap<u64, (u64, [f32; 3], i64)>,
    pub(crate) link_blends: BTreeMap<u32, ([f32; 3], [f32; 3], i64, i64)>,
    pub(crate) invulnerable: std::collections::BTreeSet<u64>,
    pub(crate) health_shield_off: std::collections::BTreeSet<u64>,
    pub(crate) health_shield: BTreeMap<u64, i64>,
    pub(crate) death_shield: std::collections::BTreeSet<u64>,
    pub(crate) anim_library: Arc<super::super::host::script_anim::AnimLibrary>,
    pub(crate) anim_notes_seen: std::collections::BTreeSet<(Arc<str>, Arc<str>)>,
    pub(crate) timed_notifies: Vec<(i64, Value, Arc<str>)>,
    pub(crate) nodes: Vec<u64>,
    pub(crate) node_origins: Vec<[f32; 3]>,
    pub(crate) node_links: Vec<Vec<u16>>,
    pub(crate) sentients: BTreeMap<u64, Value>,
    pub(crate) targets: Vec<(u64, [f32; 3])>,
    pub(crate) player_los: BTreeMap<u32, super::super::host::actors::PlayerLos>,
    pub(crate) node_los: BTreeMap<u64, i64>,
    pub(crate) node_danger: BTreeMap<(u64, String), i64>,
    pub(crate) actor_goals: BTreeMap<u64, super::super::host::actors::ActorGoal>,
    pub(crate) actor_brains: BTreeMap<u64, super::super::host::actors::ActorBrain>,
    pub(crate) actor_nav: Option<super::super::host::actors::NavHandle>,
    pub(crate) threat: super::super::host::actors::ThreatBias,
    pub(crate) level_change: Option<super::super::host::mission::LevelChange>,
    pub(crate) save: Option<super::super::host::mission::SaveRequest>,
    pub(crate) resume_tick: Option<crate::Tick>,
    pub(crate) loaded_ms: i64,
}

impl CampaignState {
    pub(crate) fn roots(&self, pending: &mut Vec<Value>) {
        pending.extend(self.nodes.iter().copied().map(Value::Object));
        pending.extend(
            self.timed_notifies
                .iter()
                .map(|(_, receiver, _)| receiver.clone()),
        );
    }
}
