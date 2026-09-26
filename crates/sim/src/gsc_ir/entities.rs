use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}
impl StringTable {
    pub(super) fn cell(&self, row: usize, column: usize) -> Option<&str> {
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
    pub entities: Vec<Vec<(String, String)>>,
    pub tables: BTreeMap<String, StringTable>,
}

pub(super) fn table_key(name: &str) -> String {
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
    pub worldspawn: BTreeMap<String, String>,
    pub world: Option<u64>,
    pub team_scores: BTreeMap<String, i32>,
    pub team_radar: BTreeMap<String, i32>,
    pub team_radar_blocked: std::collections::BTreeSet<String>,
    pub match_data: BTreeMap<String, Value>,
    pub game_end_time: i32,
    pub map_center: [f32; 3],
    pub winning_team: Option<String>,
    pub objectives: BTreeMap<u8, super::objectives::ScriptObjective>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EntityKind {
    Map,
    Spawned,
    HudElem,
    Player,
    Corpse { slot: u8, anim: Option<Arc<str>> },
    Item(i32),
    Missile(crate::ProjectileId),
    Vehicle,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Link {
    pub parent: u64,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub tag: Option<Arc<str>>,
    pub tag_offset: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Usable {
    pub cursor: i32,
    pub hint: i32,
    pub enabled: bool,
    pub barred: std::collections::BTreeSet<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Physics {
    pub velocity: [f32; 3],
    pub ticks: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScriptEntity {
    pub number: i32,
    pub kind: EntityKind,
    pub classname: Arc<str>,
    pub cylinder: Option<(f32, f32)>,
    pub brush: Option<u32>,
    pub trigger_model: Option<u32>,
    pub hidden: bool,
    pub shown_to: u64,
    pub solid: bool,
    pub linked_to: Option<Link>,
    pub motion: Vec<Motion>,
    pub light: f32,
    pub attachments: Vec<(Arc<str>, Arc<str>)>,
    pub contents: i32,
    pub audience: HudAudience,
    pub presence: Option<crate::ScriptModelId>,
    pub can_damage: bool,
    pub part_ops: Vec<(Arc<str>, bool)>,
    pub anim_op: Option<Option<Arc<str>>>,
    pub loop_sound: Option<Arc<str>>,
    pub usable: Option<Usable>,
    pub physics: Option<Physics>,
}

pub(crate) const SPAWNED_PRESENCE_BASE: u32 = 0x4000_0000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HudAudience {
    All,
    Team(Arc<str>),
    Client(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Motion {
    pub field: &'static str,
    pub path: MotionPath,
    pub start_ms: i64,
    pub duration_ms: i64,
    pub done: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MotionPath {
    Linear { from: [f32; 3], to: [f32; 3] },
    Ballistic { from: [f32; 3], velocity: [f32; 3] },
}

fn vector(text: &str) -> [f32; 3] {
    let mut parts = text
        .split_ascii_whitespace()
        .map(|p| iw4_natives::atof(p) as f32);
    std::array::from_fn(|_| parts.next().unwrap_or(0.0))
}

impl Runtime {
    pub(super) fn object_field(&mut self, id: u64, name: &str) -> Value {
        let field = self.symbol(name);
        self.objects
            .get(&id)
            .and_then(|fields| fields.get(&field))
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    pub(super) fn set_object_field(&mut self, id: u64, name: &str, value: Value) {
        let field = self.symbol(name);
        if let Some(fields) = self.objects.get_mut(&id) {
            if value == Value::Undefined {
                fields.remove(&field);
            } else {
                fields.insert(field, value);
            }
        }
    }

    pub(super) fn new_object(&mut self) -> Result<u64, String> {
        let id = self.next_object;
        self.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
        self.objects.insert(id, BTreeMap::new());
        Ok(id)
    }

    pub(super) fn create_entity(
        &mut self,
        kind: EntityKind,
        classname: &str,
    ) -> Result<u64, String> {
        let number = if kind == EntityKind::HudElem {
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
        } else {
            if self
                .entities
                .values()
                .filter(|e| e.kind != EntityKind::HudElem)
                .count()
                >= MAX_SCRIPT_ENTITIES
            {
                return Err("G_Spawn: no free entities".into());
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
                hidden: false,
                shown_to: 0,
                solid: true,
                linked_to: None,
                motion: Vec::new(),
                light: 1.0,
                attachments: Vec::new(),
                contents: 0,
                audience: HudAudience::All,
                presence: None,
                can_damage: false,
                part_ops: Vec::new(),
                anim_op: None,
                loop_sound: None,
                usable: None,
                physics: None,
            },
        );
        if kind != EntityKind::HudElem {
            self.set_object_field(id, "classname", Value::string(classname));
            let code = if classname.starts_with("script_vehicle") {
                "script_vehicle"
            } else {
                classname
            };
            self.set_object_field(id, "code_classname", Value::string(code));
            self.set_object_field(id, "origin", Value::Vector([0.0; 3]));
            self.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        }
        Ok(id)
    }

    pub(super) fn create_player(&mut self, client: u32) -> Result<u64, String> {
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
                hidden: false,
                shown_to: 0,
                solid: true,
                linked_to: None,
                motion: Vec::new(),
                light: 1.0,
                attachments: Vec::new(),
                contents: 0,
                audience: HudAudience::All,
                presence: None,
                can_damage: true,
                part_ops: Vec::new(),
                anim_op: None,
                loop_sound: None,
                usable: None,
                physics: None,
            },
        );
        self.set_object_field(id, "classname", Value::string("player"));
        self.set_object_field(id, "code_classname", Value::string("player"));
        Ok(id)
    }

    pub(super) fn player_client_of(&self, value: &Value) -> Option<u32> {
        match value {
            Value::Object(id) => self.player_client(*id),
            _ => None,
        }
    }

    pub(super) fn player_client(&self, id: u64) -> Option<u32> {
        self.entities
            .get(&id)
            .filter(|e| e.kind == EntityKind::Player)
            .map(|e| e.number as u32)
    }

    pub(super) fn delete_entity(&mut self, id: u64) {
        if let Some(entity) = self.entities.remove(&id)
            && let Some(presence) = entity.presence
        {
            self.retired_presence.push((
                presence,
                matches!(entity.kind, EntityKind::Spawned | EntityKind::Vehicle),
            ));
        }
        self.shown.remove(&id);
        self.objects.remove(&id);
    }

    pub(super) fn entity(&self, value: &Value) -> Option<(u64, &ScriptEntity)> {
        match value {
            Value::Object(id) => self.entities.get(id).map(|e| (*id, e)),
            _ => None,
        }
    }

    pub(super) fn spawn_map_entities(
        &mut self,
        entities: &[Vec<(String, String)>],
    ) -> Result<(), String> {
        let structs = match self.object_field(0, "struct") {
            Value::Array(id) => Some(id),
            _ => None,
        };
        let t5 = self
            .program
            .as_ref()
            .is_some_and(|p| p.realm() == super::Realm::T5);
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
                let next = self.next_entity_number;
                let world = self.create_entity(EntityKind::Map, classname)?;
                self.next_entity_number = next;
                if let Some(entity) = self.entities.get_mut(&world) {
                    entity.number = i32::from(trace_iw4::ENTITYNUM_WORLD);
                }
                self.set_object_field(world, "health", Value::Int(0));
                self.engine.world = Some(world);
                continue;
            }
            // Path nodes belong to the path system, not the entity list.
            if classname.is_empty() || classname.starts_with("node_") {
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
                self.create_entity(EntityKind::Map, classname)?
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
                            Value::Vector([0.0, iw4_natives::atof(value) as f32, 0.0]),
                        );
                        continue;
                    }
                    "spawnflags" | "count" | "health" | "dmg" | "maxhealth" => {
                        Value::Int(iw4_natives::atoi(value))
                    }
                    // T5 compares exploder numbers as ints.
                    "script_exploder" if t5 => Value::Int(iw4_natives::atoi(value)),
                    "speed" | "radius" | "height" => {
                        let number = iw4_natives::atof(value) as f32;
                        match key.as_str() {
                            "radius" => radius = Some(number),
                            "height" => height = Some(number),
                            _ => {}
                        }
                        Value::Float(number)
                    }
                    "classname" if classname == "script_struct" => continue,
                    _ => Value::string(value),
                };
                self.set_object_field(id, &key, value);
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

pub(super) const MAX_SCRIPT_ENTITIES: usize = 2048 - 64;
pub(super) const MAX_HUD_ELEMS: usize = 1024;
