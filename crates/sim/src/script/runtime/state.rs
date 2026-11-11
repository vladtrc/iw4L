use bevy_ecs::prelude::Resource;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::script::host;
use crate::script::{ArrayKey, Fault, Native, Program, StringTable, Value};

#[derive(Resource, Clone, Debug)]
pub(crate) struct RoundScript {
    pub(crate) program: Option<Arc<Program>>,
    pub(crate) natives: Vec<Native>,
    pub(crate) next_serial: u64,
    pub fault: Option<Fault>,
    pub(crate) fault_reported: bool,
    pub(crate) errors: BTreeMap<(String, String), u64>,
    /// Deleted entities scripts may still hold. They keep their fields until
    /// the end of the frame that deleted them, then read as undefined.
    pub(crate) dead: std::collections::BTreeSet<u64>,
    pub(crate) dying: Vec<u64>,
    pub(crate) last_tick: Option<crate::Tick>,
    pub(crate) started: bool,
    pub(crate) objects: BTreeMap<u64, BTreeMap<u32, Value>>,
    pub(crate) next_object: u64,
    pub(crate) arrays: BTreeMap<u64, BTreeMap<ArrayKey, Value>>,
    pub(crate) dynamic_symbols: BTreeMap<Arc<str>, u32>,
    pub(crate) buckets: BTreeMap<i64, VecDeque<u64>>,
    pub(crate) spawned: Vec<u64>,
    pub(crate) waiters: Vec<crate::script::Waiter>,
    pub(crate) loading: bool,
    pub(crate) presented: BTreeMap<&'static str, Vec<Value>>,
    pub(crate) unsupported: BTreeMap<&'static str, u64>,
    pub(crate) budget: usize,
    pub(crate) suspended: Vec<u64>,
    pub(crate) suspended_frames: usize,
    /// Endons that fired on a suspended thread; applied when its child yields.
    pub(crate) pending_unwinds: Vec<(u64, usize)>,
    pub(crate) parents: BTreeMap<u64, u64>,
    pub(crate) orphans: Vec<u64>,
    pub(crate) child_spawn: bool,
    pub(crate) entities: BTreeMap<u64, host::entities::ScriptEntity>,
    pub(crate) hud_slots: BTreeMap<u64, usize>,
    pub(crate) next_entity_number: i32,
    pub(crate) tables: Arc<BTreeMap<String, StringTable>>,
    pub(crate) rng: u32,
    pub(crate) pending_notifies: Vec<(Value, Arc<str>, Vec<Value>)>,
    pub(crate) signals: Vec<Arc<str>>,
    pub(crate) engine: host::entities::EngineState,
    pub(crate) players: BTreeMap<u32, host::players::PlayerSlot>,
    pub(crate) menu_answers: BTreeMap<u32, VecDeque<host::players::MenuAnswer>>,
    pub(crate) team_ranks: BTreeMap<u32, (i32, i32)>,
    pub(crate) ranks_sent: bool,
    pub(crate) selected_classes: BTreeMap<u32, u32>,
    pub(crate) thrown_insertions: BTreeMap<u32, (Arc<str>, u64)>,
    pub(crate) insertion_spots: Vec<([f32; 3], Arc<str>)>,
    pub(crate) disconnects: std::collections::BTreeSet<u32>,
    pub(crate) kicks: BTreeMap<u32, String>,
    pub(crate) joined: std::collections::BTreeSet<u32>,
    pub(crate) current_hit: Option<crate::script_player::Hit>,
    pub(crate) deaths: VecDeque<(u32, &'static str, Vec<Value>)>,
    pub(crate) exit_level: bool,
    pub(crate) shown: BTreeMap<u64, host::presence::Shown>,
    pub(crate) retired_presence: Vec<(crate::ScriptModelId, bool)>,
    pub(crate) new_models: Vec<u64>,
    pub(crate) blasts: Vec<host::entity_damage::ScriptBlast>,
    pub(crate) hits: Vec<host::entity_damage::ScriptHit>,
    pub(crate) use_held: std::collections::BTreeSet<u32>,
    pub(crate) fired_once: std::collections::BTreeSet<u64>,
    pub(crate) require_look_at: std::collections::BTreeSet<u64>,
    pub(crate) server_info: std::collections::BTreeSet<String>,
    pub(crate) server_info_defaults: BTreeMap<String, String>,
    pub(crate) missiles: BTreeMap<crate::ProjectileId, u64>,
    pub(crate) grenade_touches: Vec<host::triggers::GrenadeTouch>,
    pub(crate) lingering: Vec<(i64, u64)>,
    retired_items: Vec<u64>,
    finishing: std::collections::BTreeSet<u64>,
    finishing_order: Vec<u64>,
    pub(crate) vehicles: BTreeMap<u64, host::vehicles::Heli>,
    pub(crate) planes: BTreeMap<u64, host::vehicles::Plane>,
    pub(crate) use_selected: BTreeMap<u32, u64>,
    pub(crate) t5: host::natives::t5::T5State,
    pub(crate) restart: Option<Arc<host::restart::RestartPlan>>,
    pub(crate) campaign: super::campaign::CampaignState,
    pub(crate) finished: bool,
    pub(crate) pending_restart: Option<bool>,
    pub(crate) restored_pers: BTreeMap<u32, host::restart::Detached>,
}

impl RoundScript {
    pub(crate) fn new(_: &MatchScript) -> Self {
        Self {
            program: Default::default(),
            natives: Default::default(),
            next_serial: Default::default(),
            fault: Default::default(),
            fault_reported: Default::default(),
            errors: Default::default(),
            dead: Default::default(),
            dying: Default::default(),
            last_tick: Default::default(),
            started: Default::default(),
            objects: Default::default(),
            next_object: Default::default(),
            arrays: Default::default(),
            dynamic_symbols: Default::default(),
            buckets: Default::default(),
            spawned: Default::default(),
            waiters: Default::default(),
            loading: Default::default(),
            presented: Default::default(),
            unsupported: Default::default(),
            budget: Default::default(),
            suspended: Default::default(),
            suspended_frames: Default::default(),
            pending_unwinds: Default::default(),
            entities: Default::default(),
            hud_slots: Default::default(),
            next_entity_number: Default::default(),
            tables: Default::default(),
            rng: Default::default(),
            pending_notifies: Default::default(),
            signals: Default::default(),
            engine: Default::default(),
            players: Default::default(),
            menu_answers: Default::default(),
            team_ranks: Default::default(),
            ranks_sent: Default::default(),
            selected_classes: Default::default(),
            thrown_insertions: Default::default(),
            insertion_spots: Default::default(),
            disconnects: Default::default(),
            kicks: Default::default(),
            joined: Default::default(),
            current_hit: Default::default(),
            deaths: Default::default(),
            exit_level: Default::default(),
            shown: Default::default(),
            retired_presence: Default::default(),
            blasts: Default::default(),
            hits: Default::default(),
            use_held: Default::default(),
            fired_once: Default::default(),
            require_look_at: Default::default(),
            server_info: Default::default(),
            server_info_defaults: Default::default(),
            missiles: Default::default(),
            grenade_touches: Default::default(),
            lingering: Default::default(),
            retired_items: Default::default(),
            finishing: Default::default(),
            finishing_order: Default::default(),
            vehicles: Default::default(),
            planes: Default::default(),
            use_selected: Default::default(),
            t5: Default::default(),
            restart: Default::default(),
            finished: Default::default(),
            pending_restart: Default::default(),
            parents: Default::default(),
            orphans: Default::default(),
            child_spawn: Default::default(),
            new_models: Default::default(),
            campaign: Default::default(),
            restored_pers: Default::default(),
        }
    }

    pub(crate) fn program_fingerprint(&self) -> Option<[u8; 32]> {
        self.program.as_ref().map(|program| program.fingerprint())
    }

    pub(crate) fn live(&self, id: &u64) -> bool {
        self.objects.contains_key(id) && !self.dead.contains(id)
    }

    pub(crate) fn script_is_defined(&self, id: &u64) -> bool {
        self.live(id) && !self.finishing.contains(id)
    }

    pub(crate) fn can_receive_call(&self, id: &u64) -> bool {
        self.script_is_defined(id) && !self.retired_items.contains(id)
    }

    pub(crate) fn request_delete(&mut self, id: u64) -> bool {
        if !self.live(&id) || !self.finishing.insert(id) {
            return false;
        }
        self.finishing_order.push(id);
        true
    }

    pub(crate) fn retire_item_payload(&mut self, number: i32) {
        self.retired_items
            .extend(self.entities.iter().filter_map(|(id, entity)| {
                matches!(entity.kind, host::entities::EntityKind::Item(at) if at == number)
                    .then_some(*id)
            }));
    }

    pub(crate) fn take_retired_items(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.retired_items)
    }

    pub(crate) fn take_finishing(&mut self) -> Vec<u64> {
        self.finishing.clear();
        std::mem::take(&mut self.finishing_order)
    }

    pub(crate) fn request_round_restart(&mut self, persist: bool) -> Result<(), String> {
        if self.program.is_none() || !self.started || self.fault.is_some() {
            return Err("no healthy started match scripts".into());
        }
        if self.finished {
            return Err("match end already requested".into());
        }
        self.finished = true;
        self.pending_restart = Some(persist);
        self.signals.push(crate::script::MAP_RESTART.into());
        Ok(())
    }

    pub(crate) fn symbol(&mut self, name: &str) -> u32 {
        let program = self.program.as_ref().unwrap();
        if let Some(&id) = program.symbol_ids.get(name) {
            return id;
        }
        let next = (program.symbols.len() + self.dynamic_symbols.len()) as u32;
        *self.dynamic_symbols.entry(name.into()).or_insert(next)
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct MatchScript {
    pub(crate) precached: BTreeMap<(&'static str, String), i32>,
    pub(crate) dvars: BTreeMap<String, String>,
    pub(crate) local_presentation_dvars: bool,
    pub(crate) local_presentation_client: Option<crate::ClientId>,
    pub(crate) pending_local_dvars: Vec<(crate::TargetBoxDvar, String)>,
    pub(crate) personal_classes: BTreeMap<(u32, u32), crate::ClassDef>,
    pub(crate) weapon_bridge: BTreeMap<u32, Vec<(u32, u32)>>,
    pub(crate) next_spawned_presence: u32,
    pub(crate) match_data: host::match_data::MatchData,
    pub(crate) sp_profile: BTreeMap<String, Value>,
}
