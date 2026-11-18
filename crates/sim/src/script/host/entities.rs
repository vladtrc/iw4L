use crate::script::{Arc, ArrayKey, BTreeMap, RoundScript, Value};

pub(crate) fn is_ragdoll(world: &mut bevy_ecs::prelude::World, object: u64) -> bool {
    let number = world.resource::<RoundScript>().entities[&object].number;
    let frame = crate::frame::FrameWorld::from_world(world);
    let trajectory = frame
        .script_mover_by_number(number)
        .map(|mover| mover.state.tr_type)
        .or_else(|| {
            frame
                .dropped_item_by_number(number)
                .map(|item| item.state.tr_type)
        })
        .or_else(|| {
            frame
                .projectile_by_number(number)
                .map(|projectile| projectile.pos.tr_type)
        })
        .or_else(|| {
            frame
                .corpses()
                .slots
                .iter()
                .find(|corpse| corpse.occupied && corpse.entnum == number)
                .map(|corpse| corpse.tr_type)
        });
    trajectory.is_some_and(|kind| (10..=12).contains(&kind))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}
impl StringTable {
    pub fn cell(&self, row: usize, column: usize) -> Option<&str> {
        if row >= self.rows || column >= self.columns {
            return None;
        }
        self.cells
            .get(row * self.columns + column)
            .map(String::as_str)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LevelData {
    pub anim_trees: Arc<BTreeMap<String, super::animations::ScriptAnimTree>>,
    pub profile: asset_core::RuntimeProfile,
    pub campaign_player: Option<crate::ClientId>,
    pub path_nodes: std::sync::Arc<[asset_core::PathNode]>,
    pub path_visibility: Arc<[u8]>,
    pub absent_effects: std::collections::BTreeSet<String>,
    pub entities: Vec<Vec<(String, String)>>,
    pub tables: BTreeMap<String, StringTable>,
    pub keys: BTreeMap<String, KeyType>,
    pub schemas: BTreeMap<String, Arc<structured_data_iw4::DefinitionSet>>,
    pub player_data_defaults: Option<crate::PlayerDataDefaults>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    Int,
    Float,
    String,
    Vector,
}

pub fn parse_radiant_keys(text: &str) -> Result<BTreeMap<String, KeyType>, String> {
    let mut keys = BTreeMap::new();
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("//") {
            rest = after.find('\n').map_or("", |end| &after[end..]);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.find("*/").map_or("", |end| &after[end + 2..]);
            plain.push(' ');
        } else {
            let c = rest.chars().next().unwrap();
            plain.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    let mut words = plain.split_whitespace();
    while let Some(kind) = words.next() {
        let kind = match kind {
            "int" => KeyType::Int,
            "float" => KeyType::Float,
            "string" => KeyType::String,
            "vector" => KeyType::Vector,
            other => return Err(format!("unknown key type {other}")),
        };
        let key = words.next().ok_or("missing key name")?.to_ascii_lowercase();
        if keys.insert(key.clone(), kind).is_some() {
            return Err(format!("duplicate key {key}"));
        }
    }
    Ok(keys)
}

pub fn table_key(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

pub fn parse_entity_string(text: &str) -> Vec<Vec<(String, String)>> {
    let mut entities = Vec::new();
    let mut current: Option<Vec<(String, String)>> = None;
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                current = Some(Vec::new());
                tokens.clear();
            }
            '}' => {
                if let Some(pairs) = current.take() {
                    entities.push(pairs);
                }
                tokens.clear();
            }
            '"' => {
                let mut token = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    token.push(c);
                }
                tokens.push(token);
                if tokens.len() == 2 {
                    let value = tokens.pop().unwrap();
                    let key = tokens.pop().unwrap();
                    if let Some(pairs) = current.as_mut() {
                        pairs.push((key, value));
                    }
                }
            }
            _ => {}
        }
    }
    entities
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EngineState {
    pub cinematic: Option<crate::ScriptCinematic>,
    pub worldspawn: BTreeMap<String, String>,
    pub grenade_hints: Vec<[f32; 3]>,
    pub vehicle_spawn_times: BTreeMap<u64, i32>,
    pub world: Option<u64>,
    pub team_scores: BTreeMap<String, i32>,
    pub team_radar: BTreeMap<String, i32>,
    pub team_radar_blocked: std::collections::BTreeSet<String>,
    pub game_end_time: i32,
    pub slow_motion: Option<crate::ScriptSlowMotion>,
    pub ambient: Option<crate::ScriptAmbient>,
    pub ac130_ambient: Option<crate::ScriptAmbient>,
    pub players_ignore_radius_damage: bool,
    pub map_center: [f32; 3],
    pub winning_team: Option<String>,
    pub objectives: BTreeMap<u8, super::objectives::ScriptObjective>,
    pub minimap: Option<super::controls::MiniMap>,
    pub attractors: [Option<super::guidance::Attractor>; super::guidance::ATTRACTOR_SLOTS],
    pub turrets: BTreeMap<u64, super::turrets::Turret>,
    pub effects: BTreeMap<u64, PersistentFx>,
    pub fog: Option<crate::ScriptFog>,
    pub earthquakes: Vec<crate::ScriptEarthquake>,
    pub next_earthquake: u32,
    pub naked_vision: Option<crate::VisionChange>,
    pub thermal_vision: Option<crate::VisionChange>,
    pub thermal_body_material: String,
    pub missile_vision: Option<crate::VisionChange>,
    pub night_vision: Option<crate::VisionChange>,
    pub night_vision_assets: bool,
    pub half_res_particles: bool,
    pub pain_vision: Option<crate::VisionChange>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PersistentFx {
    pub name: String,
    pub origin: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub start_ms: Option<i32>,
    pub repeat_ms: i32,
    pub cull_distance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntityKind {
    World,
    Map,
    Spawned,
    Drone,
    Actor,
    ActorCorpse {
        slot: u8,
    },
    HudElem,
    Player,
    Corpse {
        slot: u8,
        anim: Option<Arc<str>>,
        entnum: i32,
        generation: u32,
    },
    Item(i32),
    Missile(crate::ProjectileId),
    Vehicle,
    VehicleSpawner,
    VehicleNode,
    Fx,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub parent: u64,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub tag: Option<Arc<str>>,
    pub scripted_parent: Option<([f32; 3], [[f32; 3]; 3])>,
    pub blend: Option<super::entity_links::BlendToTag>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Usable {
    pub cursor: i32,
    pub hint: i32,
    pub enabled: bool,
    pub barred: std::collections::BTreeSet<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptEntity {
    pub number: i32,
    pub kind: EntityKind,
    pub classname: Arc<str>,
    pub cylinder: Option<(f32, f32)>,
    pub brush: Option<u32>,
    pub trigger_model: Option<u32>,
    pub eq_disabled: bool,
    pub hero_only_lighting: bool,
    pub less_frequent_lighting: bool,
    pub aim_assist: bool,
    pub stable_missiles: bool,
    pub look_at_name: Option<Arc<str>>,
    pub look_at_description: Option<Arc<str>>,
    pub hidden: bool,
    pub shown_to: u64,
    pub hidden_from: u64,
    pub interpolation_toggle: bool,
    pub solid: bool,
    pub link_enabled: bool,
    pub linked_to: Option<Link>,
    pub light: f32,
    pub attachments: Vec<(Arc<str>, Arc<str>)>,
    pub contents: i32,
    pub(crate) audience: HudAudience,
    pub presence: Option<crate::ScriptModelId>,
    pub missile_owner: Option<u64>,
    pub can_damage: bool,
    pub can_radius_damage: bool,
    pub anim_op: Option<Option<Arc<str>>>,
    pub loop_sound: Option<Arc<str>>,
    pub(crate) sound_wait: Option<super::audio::waits::SoundWait>,
    pub(crate) usable: Option<Usable>,
    pub(crate) trigger_policy: super::triggers::TriggerPolicy,
}

impl ScriptEntity {
    pub(crate) fn accepts_damage(&self, flags: i32) -> bool {
        self.can_damage
            && (flags & crate::script_player::IDFLAGS_RADIUS == 0 || self.can_radius_damage)
    }
}

pub(crate) const SPAWNED_PRESENCE_BASE: u32 = 0x4000_0000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HudAudience {
    All,
    Team(Arc<str>),
    Client(u32),
}

fn vector(text: &str) -> [f32; 3] {
    let mut parts = text
        .split_ascii_whitespace()
        .map(|p| super::natives::iw4::atof(p) as f32);
    std::array::from_fn(|_| parts.next().unwrap_or(0.0))
}

impl RoundScript {
    pub(crate) fn object_field(&mut self, id: u64, name: &str) -> Value {
        if name == "enemy"
            && let Some(sentient) = self.sentients.get(&id)
        {
            return sentient.target.map_or(Value::Undefined, Value::Object);
        }
        let field = self.symbol(name);
        self.objects
            .get(&id)
            .and_then(|fields| fields.get(&field))
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    pub fn set_object_field(&mut self, id: u64, name: &str, value: Value) {
        if name == "enemy"
            && let Some(sentient) = self.sentients.get_mut(&id)
        {
            let target = match value {
                Value::Object(target) => Some(target),
                _ => None,
            };
            if sentient.target != target {
                sentient.target = target;
                if let Some(actor) = self.actors.get_mut(&id) {
                    if super::actors::reacquire::clear(actor, false) {
                        self.pending_notifies
                            .push((Value::Object(id), "movemode".into(), vec![]));
                    }
                    actor.shot_check = None;
                    actor.sight_accuracy = None;
                    actor.no_attacker_accuracy_mod = false;
                    actor.sight.has_threatening_enemy = false;
                    if target.is_some() {
                        actor.alert_level =
                            actor.alert_level.max(super::actors::targeting::ALERT_LEVEL);
                    }
                }
                self.pending_notifies
                    .push((Value::Object(id), "enemy".into(), vec![]));
            }
            return;
        }
        let field = self.symbol(name);
        if name == "origin"
            && self.objects.get(&id).and_then(|fields| fields.get(&field)) != Some(&value)
            && let Some(sentient) = self.sentients.get_mut(&id)
        {
            sentient.nearest_node_valid = false;
        }
        if let Some(fields) = self.objects.get_mut(&id) {
            if value == Value::Undefined {
                fields.remove(&field);
            } else {
                fields.insert(field, value);
            }
        }
    }

    pub(crate) fn new_object(&mut self) -> Result<u64, String> {
        let id = self.next_object;
        self.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
        self.objects.insert(id, BTreeMap::new());
        Ok(id)
    }

    pub(crate) fn create_entity(
        &mut self,
        kind: EntityKind,
        classname: &str,
    ) -> Result<u64, String> {
        let actor_slot = if kind == EntityKind::Actor {
            Some(super::actors::free_slot(self)?)
        } else {
            None
        };
        let number = if kind == EntityKind::World {
            i32::from(trace_iw4::ENTITYNUM_WORLD)
        } else if kind == EntityKind::HudElem {
            if self
                .entities
                .values()
                .filter(|e| e.kind == EntityKind::HudElem)
                .count()
                >= MAX_HUD_ELEMS
            {
                return Err("exceeded maximum number of script hud elements".into());
            }
            -1
        } else if kind == EntityKind::Fx {
            if self
                .entities
                .values()
                .filter(|entity| entity.kind == EntityKind::Fx)
                .count()
                >= 768
            {
                return Err("too many FX entities (768)".into());
            }
            -1
        } else if kind == EntityKind::VehicleNode {
            -1
        } else {
            let limit = if self.profile == asset_core::RuntimeProfile::Campaign {
                usize::from(trace_iw4::ENTITYNUM_WORLD)
            } else {
                MAX_SCRIPT_ENTITIES
            };
            let used = self
                .entities
                .values()
                .filter(|e| {
                    !matches!(
                        e.kind,
                        EntityKind::World
                            | EntityKind::HudElem
                            | EntityKind::VehicleNode
                            | EntityKind::Fx
                    )
                })
                .count();
            if used >= limit {
                perf::instant!("script_entity_capacity_refused", "iw4l.sim", |ctx| {
                    ctx.arg("classname", classname);
                    ctx.arg("used", used as i64);
                    ctx.arg("limit", limit as i64);
                    ctx.arg("next_number", i64::from(self.next_entity_number));
                });
                return Err("no free entities".into());
            }
            let number = self.next_entity_number;
            self.next_entity_number += 1;
            number
        };
        let id = self.new_object()?;
        self.entities.insert(
            id,
            ScriptEntity {
                number,
                kind: kind.clone(),
                classname: classname.into(),
                cylinder: None,
                brush: None,
                trigger_model: None,
                eq_disabled: false,
                hero_only_lighting: false,
                less_frequent_lighting: false,
                aim_assist: false,
                stable_missiles: false,
                look_at_name: None,
                look_at_description: None,
                hidden: false,
                shown_to: 0,
                hidden_from: 0,
                interpolation_toggle: false,
                solid: true,
                link_enabled: super::entity_links::enabled_at_spawn(&kind, classname),
                linked_to: None,
                light: 1.0,
                attachments: Vec::new(),
                contents: 0,
                audience: HudAudience::All,
                presence: None,
                missile_owner: None,
                can_damage: classname == "trigger_damage",
                can_radius_damage: true,
                anim_op: None,
                loop_sound: None,
                sound_wait: None,
                usable: None,
                trigger_policy: Default::default(),
            },
        );
        if kind != EntityKind::HudElem {
            self.set_object_field(id, "classname", Value::string(classname));
            let code = if classname.starts_with("trigger_multiple_") {
                "trigger_multiple"
            } else if classname.starts_with("trigger_use_") {
                "trigger_use"
            } else if classname.starts_with("script_vehicle")
                && classname != "script_vehicle_collmap"
            {
                "script_vehicle"
            } else {
                classname
            };
            self.set_object_field(id, "code_classname", Value::string(code));
            if self.profile == asset_core::RuntimeProfile::Campaign {
                self.set_object_field(id, "spawnflags", Value::Int(0));
            }
            self.set_object_field(id, "anglelerprate", Value::Float(540.0));
            self.set_object_field(id, "origin", Value::Vector([0.0; 3]));
            self.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        }
        if let Some(slot) = actor_slot
            && let Err(error) = super::actors::allocate(self, id, slot)
        {
            self.entities.remove(&id);
            self.objects.remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    pub(crate) fn create_player(&mut self, client: u32) -> Result<u64, String> {
        let id = self.new_object()?;
        self.entities.insert(
            id,
            ScriptEntity {
                number: client as i32,
                kind: EntityKind::Player,
                classname: "player".into(),
                cylinder: None,
                brush: None,
                trigger_model: None,
                eq_disabled: false,
                hero_only_lighting: false,
                less_frequent_lighting: false,
                aim_assist: false,
                stable_missiles: false,
                look_at_name: None,
                look_at_description: None,
                hidden: false,
                shown_to: 0,
                hidden_from: 0,
                interpolation_toggle: false,
                solid: true,
                link_enabled: false,
                linked_to: None,
                light: 1.0,
                attachments: Vec::new(),
                contents: 0,
                audience: HudAudience::All,
                presence: None,
                missile_owner: None,
                can_damage: true,
                can_radius_damage: true,
                anim_op: None,
                loop_sound: None,
                sound_wait: None,
                usable: None,
                trigger_policy: Default::default(),
            },
        );
        self.set_object_field(id, "classname", Value::string("player"));
        self.set_object_field(id, "code_classname", Value::string("player"));
        Ok(id)
    }

    pub fn player_client_of(&self, value: &Value) -> Option<u32> {
        match value {
            Value::Object(id) => self.player_client(*id),
            _ => None,
        }
    }

    pub(crate) fn player_client(&self, id: u64) -> Option<u32> {
        self.entities
            .get(&id)
            .filter(|e| e.kind == EntityKind::Player)
            .map(|e| e.number as u32)
    }

    pub(crate) fn delete_entity(&mut self, id: u64) {
        super::actors::turret::dissociate(self, id);
        super::actors::targeting::dissociate(self, id);
        self.actor_corpses.remove(id);
        self.ai_event_listeners.remove_entity(id);
        super::actors::suppression::dissociate(self, id);
        self.path_nodes.remove_blocker(id);
        self.actors.remove(&id);
        for actor in self.actors.values_mut() {
            actor.goals.remove_entity(id);
            if actor.gaze_target == Some(id) {
                actor.gaze_target = None;
            }
        }
        self.spawners.remove(&id);
        self.sentients.remove(&id);
        for actor in self.actors.values_mut() {
            actor.sentient_info.remove(&id);
        }
        self.animators.remove(&id);
        if let Some(client) = self.player_client(id) {
            self.release_trigger_claims(client);
        }
        self.use_selected.retain(|_, selected| *selected != id);
        if let Some(entity) = self.entities.remove(&id)
            && let Some(presence) = entity.presence
        {
            self.retired_presence.push((
                presence,
                matches!(
                    entity.kind,
                    EntityKind::Spawned
                        | EntityKind::Drone
                        | EntityKind::Actor
                        | EntityKind::ActorCorpse { .. }
                        | EntityKind::Vehicle
                        | EntityKind::Missile(_)
                ),
            ));
        }
        self.shown.remove(&id);
        self.missiles.retain(|_, object| *object != id);
        self.vehicles.remove(&id);
        self.engine.turrets.remove(&id);
        self.engine.vehicle_spawn_times.remove(&id);
        self.planes.remove(&id);
        self.fired_once.remove(&id);
        self.require_look_at.remove(&id);
        self.lingering.retain(|(_, object)| *object != id);
        for slot in &mut self.engine.attractors {
            if slot.is_some_and(|a| a.anchor == super::guidance::Anchor::Entity(id)) {
                *slot = None;
            }
        }
        // A deleted entity keeps its fields for the rest of the
        // frame; scripts read them right after `delete()` (UAV bookkeeping).
        if self.objects.contains_key(&id) && self.dead.insert(id) {
            self.dying.push(id);
        }
    }

    pub fn entity(&self, value: &Value) -> Option<(u64, &ScriptEntity)> {
        match value {
            Value::Object(id) => self.entities.get(id).map(|e| (*id, e)),
            _ => None,
        }
    }

    pub(crate) fn spawn_map_entities(
        &mut self,
        entities: &[Vec<(String, String)>],
        keys: &BTreeMap<String, KeyType>,
    ) -> Result<(), String> {
        let structs = match self.object_field(0, "struct") {
            Value::Array(id) => Some(id),
            _ => None,
        };
        let policy = crate::script::families::policy_for_round(self);
        for (ordinal, pairs) in entities.iter().enumerate() {
            let classname = pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("classname"))
                .map_or("", |(_, v)| v.as_str());
            if classname == "worldspawn" {
                self.engine.worldspawn = pairs
                    .iter()
                    .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                    .collect();
                let world = if self.profile == asset_core::RuntimeProfile::Campaign {
                    self.create_entity(EntityKind::World, classname)?
                } else {
                    let next = self.next_entity_number;
                    let world = self.create_entity(EntityKind::Map, classname)?;
                    self.next_entity_number = next;
                    self.entities.get_mut(&world).unwrap().number =
                        i32::from(trace_iw4::ENTITYNUM_WORLD);
                    world
                };
                self.set_object_field(world, "health", Value::Int(0));
                self.engine.world = Some(world);
                continue;
            }
            if classname.is_empty() || classname.starts_with("node_") {
                continue;
            }
            if classname == "info_grenade_hint" {
                if self.engine.grenade_hints.len() == 512 {
                    return Err("MAX_GRENADE_HINTS (512) exceeded".into());
                }
                let origin = pairs
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("origin"))
                    .map_or([0.0; 3], |(_, value)| vector(value));
                self.engine.grenade_hints.push(origin);
                continue;
            }
            let id = if classname == "script_struct" {
                let Some(array) = structs else {
                    continue;
                };
                let id = self.new_object()?;
                let values = self.arrays.get_mut(&array).ok_or("level.struct is gone")?;
                let index = values.len() as i32;
                values.insert(ArrayKey::Integer(index), Value::Object(id));
                id
            } else {
                let kind = if classname.starts_with("info_vehicle_node") {
                    EntityKind::VehicleNode
                } else {
                    EntityKind::Map
                };
                self.create_entity(kind, classname)?
            };
            let mut radius = None;
            let mut height = None;
            let mut cmodel = None;
            let mut trigger_model = None;
            for (key, value) in pairs {
                let key = key.to_ascii_lowercase();
                if key == "model" {
                    cmodel = value.strip_prefix('*').and_then(|n| n.parse::<u32>().ok());
                    trigger_model = value.strip_prefix('?').and_then(|n| n.parse::<u32>().ok());
                }
                let value = match key.as_str() {
                    "origin" | "angles" => Value::Vector(vector(value)),
                    "angle" => {
                        self.set_object_field(
                            id,
                            "angles",
                            Value::Vector([0.0, super::natives::iw4::atof(value) as f32, 0.0]),
                        );
                        continue;
                    }
                    "spawnflags" | "count" | "health" | "dmg" | "maxhealth" | "threshold"
                    | "accumulate" => Value::Int(super::natives::iw4::atoi(value)),
                    _ if policy.integer_entity_fields.contains(&key.as_str()) => {
                        Value::Int(super::natives::iw4::atoi(value))
                    }
                    "speed" | "radius" | "height" | "wait" => {
                        let number = super::natives::iw4::atof(value) as f32;
                        match key.as_str() {
                            "radius" => radius = Some(number),
                            "height" => height = Some(number),
                            _ => {}
                        }
                        Value::Float(number)
                    }
                    "classname" if classname == "script_struct" => continue,
                    _ => match keys.get(&key) {
                        Some(KeyType::Int) => Value::Int(super::natives::iw4::atoi(value)),
                        Some(KeyType::Float) => {
                            Value::Float(super::natives::iw4::atof(value) as f32)
                        }
                        Some(KeyType::Vector) => Value::Vector(vector(value)),
                        Some(KeyType::String) | None => Value::string(value),
                    },
                };
                self.set_object_field(id, &key, value);
            }
            if self.profile == asset_core::RuntimeProfile::Campaign
                && classname.starts_with("script_vehicle")
            {
                if classname == "script_vehicle_collmap" {
                    let entity = self.entities.get_mut(&id).unwrap();
                    entity.solid = false;
                    entity.contents = 0;
                    entity.hidden = true;
                } else if matches!(self.object_field(id, "spawnflags"), Value::Int(flags) if flags & 2 != 0)
                {
                    let entity = self.entities.get_mut(&id).unwrap();
                    entity.kind = EntityKind::VehicleSpawner;
                    entity.solid = false;
                    entity.contents = 0;
                    entity.hidden = true;
                }
            }
            if self.profile == asset_core::RuntimeProfile::Campaign
                && super::spawners::aitype_module(classname).is_some()
            {
                super::spawners::install_map_actor(self, id, pairs)?;
            }
            if let Some(entity) = self.entities.get_mut(&id) {
                if classname == "trigger_radius" {
                    entity.cylinder = Some((radius.unwrap_or(0.0), height.unwrap_or(0.0)));
                } else {
                    entity.brush = cmodel;
                    entity.trigger_model = trigger_model;
                }
                if matches!(classname, "script_model" | "script_brushmodel") {
                    entity.presence = u32::try_from(ordinal)
                        .ok()
                        .map(crate::ScriptModelId::from_authored_source_ordinal);
                }
            }
        }
        Ok(())
    }
}

pub(crate) const MAX_SCRIPT_ENTITIES: usize = 2048 - 64;
pub(crate) const MAX_HUD_ELEMS: usize = 1024;

pub(crate) fn load_native_field(runtime: &RoundScript, id: u64, field: &str) -> Option<Value> {
    super::vehicles::physics::load_field(runtime, id, field)
        .or_else(|| super::actors::load_field(runtime, id, field))
        .or_else(|| super::sentients::load_field(runtime, id, field))
}
pub(crate) fn store_native_field(
    runtime: &mut RoundScript,
    id: u64,
    field: &str,
    value: &Value,
) -> Result<bool, String> {
    if super::vehicles::physics::store_field(runtime, id, field, value)? {
        return Ok(true);
    }
    if super::actors::store_field(runtime, id, field, value)? {
        return Ok(true);
    }
    super::sentients::store_field(runtime, id, field, value)
}
pub(crate) fn check_native_array_write(
    runtime: &RoundScript,
    id: u64,
    field: &str,
) -> Result<(), String> {
    super::vehicles::physics::check_array_write(runtime, id, field)?;
    if field == "laststand" && runtime.player_client(id).is_some() {
        return Err("native player field laststand cannot be an array".into());
    }
    super::actors::check_array_write(runtime, id, field)?;
    super::sentients::check_array_write(runtime, id, field)
}
