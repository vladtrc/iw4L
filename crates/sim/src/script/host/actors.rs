use super::{
    args::{arg, float},
    arrays::new_array,
    natives::engine::entity_id,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

mod aim;
pub(crate) mod anim_mode;
pub(crate) mod animation;
pub(crate) mod arrival;
pub(crate) mod bad_places;
mod combat;
pub(crate) mod corpses;
pub(crate) mod cover;
pub(crate) mod death;
mod feasibility;
pub(crate) mod gaze;
pub(crate) mod goals;
pub(crate) mod grenades;
mod melee;
pub(crate) mod navigation;
pub(crate) mod orientation;
pub(crate) mod perception;
pub(crate) mod physics;
pub(crate) mod prone;
pub(crate) mod reacquire;
pub(crate) mod scripted;
pub(crate) mod spatial;
pub(crate) mod startup;
pub(crate) mod suppression;
pub(crate) mod targeting;
pub(crate) mod teleport;
pub(crate) mod traversal;
pub(crate) mod turret;
pub(crate) mod weapon_drops;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Species {
    #[default]
    Human,
    Dog,
    Civilian,
}
impl Species {
    pub(super) fn humanoid(self) -> bool {
        matches!(self, Self::Human | Self::Civilian)
    }
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "human" => Ok(Self::Human),
            "dog" => Ok(Self::Dog),
            "civilian" => Ok(Self::Civilian),
            _ => Err(format!("unknown AI species '{name}'")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Dog => "dog",
            Self::Civilian => "civilian",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Actor {
    pub goals: goals::Goals,
    pub thermal_draw: bool,
    pub alert_level: u8,
    pub move_mode: u8,
    pub walk_distance: [f32; 2],
    pub combat_mode: String,
    pub doing_ambush: bool,
    pub no_attacker_accuracy_mod: bool,
    pub ignore_fixed_node_safety: bool,
    pub script_anim_mode: anim_mode::Mode,
    pub sight: perception::Sight,
    pub shot_check: Option<combat::ShotCheck>,
    pub sight_accuracy: Option<combat::SightAccuracy>,
    pub team_move_wait_until: i32,
    // Melee, Shoot and ShootBlank share the actor's per-tick attack budget.
    pub last_attack_ms: Option<i64>,
    pub grenade_throw: Option<grenades::Plan>,
    pub grenade_contact: grenades::Contact,
    pub grenade_response: Option<grenades::Response>,
    pub entity_target: Option<targeting::EntityTarget>,
    pub gaze_target: Option<u64>,
    pub reacquire: reacquire::Reacquire,
    pub cover: cover::Cover,
    pub arrival: arrival::Arrival,
    pub traversal: traversal::Traversal,
    pub badplace_escape: Option<goals::Goal>,
    pub damage: death::Damage,
    pub damage_shield: bool,
    pub prone_nodes: Option<prone::ProneNodes>,
    pub prone: prone::Prone,
    pub death: Option<death::Death>,
    pub navigation: navigation::Navigation,
    pub motion: physics::ActorPhysics,
    pub last_may_move_time: i32,
    pub orientation: orientation::Orientation,
    pub suppression: suppression::Suppression,
    pub turret: turret::Control,
    pub scripted_control: Option<scripted::ScriptedControl>,
    pub anim_script_thread: Option<u64>,
    pub anim_script_kind: Option<scripted::ControlKind>,
    pub previous_anim_script: Option<scripted::ControlKind>,
    pub pending_anim_end: Option<scripted::ControlKind>,
    pub custom_anim: Option<(u32, Option<u32>)>,
    pub aim_limits: [f32; 4],
    pub talk_to_species: u8,
    pub sentient_info: std::collections::BTreeMap<u64, perception::SentientInfo>,
    pub slot: u8,
    pub species: Species,
    pub alive: bool,
    pub physics_initialized: bool,
    pub delayed_death: bool,
    pub allowed_stances: u8,
    pub path_stances: Option<u8>,
    pub claimed_node: Option<u16>,
    pub at_claim_node: bool,
    pub engagement_min: [f32; 2],
    pub engagement_max: [f32; 2],
    pub inactive_human_tree: Option<super::animations::Animator>,
    pub inactive_script_tree: Option<super::animations::Animator>,
    pub script_initialized: bool,
    pub type_started: bool,
    pub tree_changed: bool,
    pub initial_state: Option<startup::InitialState>,
}

pub(crate) fn free_slot(runtime: &RoundScript) -> Result<u8, String> {
    (0..32)
        .find(|slot| runtime.actors.values().all(|actor| actor.slot != *slot))
        .ok_or_else(|| "no free actor slots".into())
}

pub(crate) fn allocate(runtime: &mut RoundScript, id: u64, slot: u8) -> Result<(), String> {
    super::sentients::allocate(runtime, id, 4, false)?;
    runtime.actors.insert(
        id,
        Actor {
            goals: Default::default(),
            thermal_draw: false,
            alert_level: 0,
            move_mode: 0,
            walk_distance: [256.0, 64.0],
            combat_mode: "cover".into(),
            doing_ambush: false,
            no_attacker_accuracy_mod: false,
            ignore_fixed_node_safety: false,
            script_anim_mode: Default::default(),
            sight: Default::default(),
            shot_check: None,
            sight_accuracy: None,
            team_move_wait_until: 0,
            last_attack_ms: None,
            grenade_throw: None,
            grenade_contact: Default::default(),
            grenade_response: None,
            entity_target: None,
            gaze_target: None,
            reacquire: Default::default(),
            cover: Default::default(),
            arrival: Default::default(),
            traversal: Default::default(),
            badplace_escape: None,
            damage: Default::default(),
            damage_shield: false,
            prone_nodes: None,
            prone: Default::default(),
            death: None,
            navigation: Default::default(),
            motion: Default::default(),
            last_may_move_time: 0,
            orientation: Default::default(),
            suppression: Default::default(),
            turret: Default::default(),
            scripted_control: None,
            anim_script_thread: None,
            anim_script_kind: None,
            previous_anim_script: None,
            pending_anim_end: None,
            custom_anim: None,
            aim_limits: [0.0; 4],
            talk_to_species: u8::MAX,
            sentient_info: Default::default(),
            slot,
            species: Species::Human,
            alive: false,
            physics_initialized: false,
            delayed_death: false,
            allowed_stances: 7,
            path_stances: None,
            claimed_node: None,
            at_claim_node: false,
            engagement_min: [0.0; 2],
            engagement_max: [0.0; 2],
            inactive_human_tree: None,
            inactive_script_tree: None,
            script_initialized: false,
            type_started: false,
            tree_changed: true,
            initial_state: None,
        },
    );
    super::animations::initialize_actor_tree(runtime, id);
    Ok(())
}

pub(super) fn actor_id(world: &World, receiver: &Value) -> Result<u64, String> {
    let id = entity_id(world, receiver)?;
    if !world.resource::<RoundScript>().actors.contains_key(&id) {
        return Err("entity is not an actor".into());
    }
    Ok(id)
}

fn text(args: &[Value], index: usize) -> Result<&str, String> {
    match arg(args, index)? {
        Value::String(name) => Ok(name),
        _ => Err(format!("parameter {} must be a string", index + 1)),
    }
}
fn filter(args: &[Value]) -> Result<(u8, Option<Species>), String> {
    let teams = if args.is_empty() {
        0x1e
    } else {
        super::spawners::team_flags(&args[..1])?
    };
    let species = if args.len() < 2 || text(args, 1)? == "all" {
        None
    } else {
        Some(Species::parse(text(args, 1)?)?)
    };
    Ok((teams, species))
}

fn population(
    runtime: &RoundScript,
    teams: u8,
    species: Option<Species>,
    exclude_dogs: bool,
) -> Vec<Value> {
    let mut owners: Vec<_> = runtime
        .actors
        .iter()
        .filter(|(id, actor)| {
            runtime.live(id)
                && actor.alive
                && !actor.delayed_death
                && species.is_none_or(|species| species == actor.species)
                && (!exclude_dogs || actor.species != Species::Dog)
                && runtime
                    .sentients
                    .get(id)
                    .is_some_and(|sentient| teams & (1 << sentient.team) != 0)
        })
        .map(|(id, actor)| (actor.slot, *id))
        .collect();
    owners.sort_by_key(|(slot, _)| *slot);
    owners
        .into_iter()
        .map(|(_, id)| Value::Object(id))
        .collect()
}

pub(crate) fn load_field(runtime: &RoundScript, id: u64, field: &str) -> Option<Value> {
    let actor = runtime.actors.get(&id)?;
    if let Some(value) = actor.sight.load(field) {
        return Some(value);
    }
    if let Some(value) = actor.damage.load(field) {
        return Some(value);
    }
    if let Some(value) = actor.suppression.load(field) {
        return Some(value);
    }
    if let Some(value) = goals::load(&actor.goals, field) {
        return Some(value);
    }
    if let Some(value) = actor.navigation.load(field) {
        return Some(value);
    }
    if let Some(index) = aim::FIELDS.iter().position(|name| *name == field) {
        return Some(Value::Float(actor.aim_limits[index]));
    }
    match field {
        "velocity" => Some(Value::Vector(actor.motion.velocity)),
        "teammovewaittime" => Some(Value::Int(actor.team_move_wait_until)),
        "grenade" => Some(grenades::contact_value(runtime, actor)),
        "damageshield" => Some(Value::Int(i32::from(actor.damage_shield))),
        "goingtoruntopos" => Some(Value::Int(actor.arrival.running_flag())),
        "node" => Some(
            actor
                .claimed_node
                .map_or(Value::Undefined, |node| runtime.path_nodes.value(node)),
        ),
        "relativedir" => Some(Value::Int(i32::from(actor.orientation.relative_dir))),
        "gunblockedbywall" => Some(Value::Int(i32::from(actor.orientation.gun_blocked))),
        "combatmode" => Some(Value::string(&actor.combat_mode)),
        "doingambush" => Some(Value::Int(i32::from(actor.doing_ambush))),
        "noattackeraccuracymod" => Some(Value::Int(i32::from(actor.no_attacker_accuracy_mod))),
        "ignoreforfixednodesafecheck" => {
            Some(Value::Int(i32::from(actor.ignore_fixed_node_safety)))
        }
        "stairsstate" => Some(Value::string(
            ["none", "up", "down"][usize::from(actor.motion.stairs_state)],
        )),
        "pathgoalpos" => Some(
            actor
                .navigation
                .end()
                .map_or(Value::Undefined, Value::Vector),
        ),
        "facemotion" => Some(Value::Int(i32::from(actor.orientation.face_motion()))),
        "movemode" => ["stop", "stop_soon", "walk", "run"]
            .get(usize::from(actor.move_mode))
            .map(|name| Value::string(name)),
        "walkdist" => Some(Value::Float(actor.walk_distance[0])),
        "walkdistfacingmotion" => Some(Value::Float(actor.walk_distance[1])),
        "script" => Some(
            actor
                .anim_script_kind
                .map_or(Value::Undefined, |kind| Value::string(kind.name())),
        ),
        "prevscript" => Some(
            actor
                .previous_anim_script
                .map_or(Value::Undefined, |kind| Value::string(kind.name())),
        ),
        "alertlevelint" => Some(Value::Int(i32::from(actor.alert_level))),
        "alertlevel" => Some(Value::string(
            ["asleep", "noncombat", "alert", "combat"][usize::from(actor.alert_level)],
        )),
        "desiredangle" => Some(Value::Float(actor.motion.desired_angles[1])),
        "turnrate" => Some(Value::Float(actor.orientation.turn_rate)),
        "type" => Some(Value::string(actor.species.name())),
        "delayeddeath" => Some(Value::Int(i32::from(actor.delayed_death))),
        _ => None,
    }
}
pub(crate) fn store_field(
    runtime: &mut RoundScript,
    id: u64,
    field: &str,
    value: &Value,
) -> Result<bool, String> {
    if field == "grenade" && runtime.actors.contains_key(&id) {
        return Err("actor grenade is read-only".into());
    }
    if field == "damageshield"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.damage_shield = super::args::int(std::slice::from_ref(value), 0)? != 0;
        return Ok(true);
    }
    if field == "goingtoruntopos" && runtime.actors.contains_key(&id) {
        let value = super::args::int(std::slice::from_ref(value), 0)?;
        arrival::set_running(runtime.actors.get_mut(&id).unwrap(), value)?;
        return Ok(true);
    }
    if let Some(actor) = runtime.actors.get_mut(&id)
        && actor.navigation.store(field, value)?
    {
        return Ok(true);
    }
    if let Some(actor) = runtime.actors.get_mut(&id)
        && actor.damage.store(field, value)?
    {
        return Ok(true);
    }

    if let Some(actor) = runtime.actors.get_mut(&id)
        && actor.sight.store(field, value)?
    {
        return Ok(true);
    }
    if runtime.actors.contains_key(&id)
        && [
            "node",
            "movemode",
            "velocity",
            "teammovewaittime",
            "stairsstate",
            "facemotion",
            "pathgoalpos",
            "doingambush",
            "desiredangle",
            "script",
            "prevscript",
            "relativedir",
            "gunblockedbywall",
        ]
        .contains(&field)
    {
        return Err(format!("actor {field} is read-only"));
    }
    if field == "alertlevelint"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        let level = super::args::int(std::slice::from_ref(value), 0)?;
        actor.alert_level = u8::try_from(level)
            .ok()
            .filter(|level| *level < 4)
            .ok_or("invalid alert level")?;
        return Ok(true);
    }
    if field == "noattackeraccuracymod"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.no_attacker_accuracy_mod = super::args::int(std::slice::from_ref(value), 0)? != 0;
        return Ok(true);
    }
    if field == "ignoreforfixednodesafecheck"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.ignore_fixed_node_safety = super::args::int(std::slice::from_ref(value), 0)? != 0;
        return Ok(true);
    }
    if field == "combatmode"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        let Value::String(name) = value else {
            return Err("combat mode must be a string".into());
        };
        if !["cover", "no_cover", "ambush", "ambush_nodes_only"].contains(&&**name) {
            return Err("invalid combat mode".into());
        }
        actor.combat_mode = name.to_string();
        actor.doing_ambush = matches!(&**name, "ambush" | "ambush_nodes_only");
        return Ok(true);
    }
    if let Some(index) = ["walkdist", "walkdistfacingmotion"]
        .iter()
        .position(|name| *name == field)
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.walk_distance[index] = float(std::slice::from_ref(value), 0)?;
        return Ok(true);
    }
    if field == "alertlevel"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        let Value::String(name) = value else {
            return Err("alert level must be a string".into());
        };
        actor.alert_level = ["asleep", "noncombat", "alert", "combat"]
            .iter()
            .position(|level| *level == &**name)
            .ok_or("invalid alert level")? as u8;
        return Ok(true);
    }
    if let Some(actor) = runtime.actors.get_mut(&id)
        && actor.suppression.store(field, value)?
    {
        return Ok(true);
    }
    if field == "turnrate"
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.orientation.turn_rate = float(std::slice::from_ref(value), 0)?;
        return Ok(true);
    }
    if let Some(actor) = runtime.actors.get_mut(&id)
        && goals::store(&mut actor.goals, field, value)?
    {
        return Ok(true);
    }
    if let Some(index) = aim::FIELDS.iter().position(|name| *name == field)
        && let Some(actor) = runtime.actors.get_mut(&id)
    {
        actor.aim_limits[index] = float(std::slice::from_ref(value), 0)?;
        return Ok(true);
    }
    if field == "delayeddeath" && runtime.actors.contains_key(&id) {
        return Err("actor delayeddeath is read-only".into());
    }
    if field != "type" || !runtime.actors.contains_key(&id) {
        return Ok(false);
    }
    let Value::String(name) = value else {
        return Err("actor type must be a species string".into());
    };
    let species = Species::parse(name)?;
    let previous = runtime.actors[&id].species;
    if previous.humanoid() != species.humanoid() {
        let active = runtime.animators.remove(&id);
        let actor = runtime.actors.get_mut(&id).unwrap();
        let restored = if previous.humanoid() {
            actor.inactive_human_tree = active;
            actor.inactive_script_tree.take()
        } else {
            actor.inactive_script_tree = active;
            actor.inactive_human_tree.take()
        };
        if let Some(restored) = restored {
            runtime.animators.insert(id, restored);
        }
        runtime.actors.get_mut(&id).unwrap().tree_changed = true;
    }
    runtime.actors.get_mut(&id).unwrap().species = species;
    super::animations::initialize_actor_tree(runtime, id);
    Ok(true)
}
pub(crate) fn check_array_write(runtime: &RoundScript, id: u64, field: &str) -> Result<(), String> {
    if ([
        "velocity",
        "teammovewaittime",
        "goingtoruntopos",
        "node",
        "type",
        "delayeddeath",
        "turnrate",
        "ignoreforfixednodesafecheck",
        "damageshield",
    ]
    .contains(&field)
        || death::FIELDS.contains(&field)
        || suppression::FIELDS.contains(&field)
        || aim::FIELDS.contains(&field)
        || goals::FIELDS.contains(&field)
        || perception::FIELDS.contains(&field)
        || navigation::FIELDS.contains(&field))
        && runtime.actors.contains_key(&id)
    {
        return Err("native actor field cannot be an array".into());
    }
    Ok(())
}

fn stance(name: &str) -> Result<u8, String> {
    match name {
        "stand" => Ok(1),
        "crouch" => Ok(2),
        "prone" => Ok(4),
        _ => Err(format!("unknown actor stance '{name}'")),
    }
}

fn set_engagement(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    minimum: bool,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let range = [float(args, 0)?, float(args, 1)?];
    if range.iter().any(|value| !value.is_finite()) {
        return Err("engagement distances must be finite".into());
    }
    if (minimum && range[0] < range[1]) || (!minimum && range[0] > range[1]) {
        return Err("engagement distance endpoints are in the wrong order".into());
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let actor = runtime.actors.get_mut(&id).unwrap();
    if minimum {
        actor.engagement_min = range;
    } else {
        actor.engagement_max = range;
    }
    Ok(Value::Undefined)
}

fn thermal_draw(world: &mut World, receiver: &Value, enabled: bool) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .thermal_draw = enabled;
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    gaze::register(registry);
    perception::register(registry);
    teleport::register(registry);
    scripted::register(registry);
    physics::register(registry);
    orientation::register(registry);
    suppression::register(registry);
    spatial::register(registry);
    aim::register(registry);
    combat::register(registry);
    melee::register(registry);
    grenades::register(registry);
    weapon_drops::register(registry);
    targeting::register(registry);
    reacquire::register(registry);
    cover::register(registry);
    turret::register(registry);
    arrival::register(registry);
    bad_places::register(registry);
    goals::register(registry);
    navigation::register(registry);
    traversal::register(registry);
    anim_mode::register(registry);
    corpses::register(registry);
    feasibility::register(registry);
    prone::register(registry);
    registry.register(Namespace::Method, "clearenemy", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let old = world
            .resource_mut::<RoundScript>()
            .object_field(id, "enemy");
        targeting::set_enemy(world, id, None);
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get_mut(&id).unwrap();
        if let Value::Object(target) = old
            && let Some(info) = actor.sentient_info.get_mut(&target)
        {
            info.last_known_time = 0;
            info.last_known_position = None;
        }
        // Sight history is independent of the discarded target location.
        actor.no_attacker_accuracy_mod = false;
        actor.shot_check = None;
        actor.sight_accuracy = None;
        actor.sight.has_threatening_enemy = false;
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "settalktospecies",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            // SP ActorCmd_SetTalkToSpecies reads parameter zero on every iteration.
            let mask = if args.is_empty() {
                0
            } else {
                match text(args, 0)? {
                    "all" => u8::MAX,
                    "human" => 1,
                    "dog" => 2,
                    "civilian" => 4,
                    _ => 0,
                }
            };
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .talk_to_species = mask;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "thermaldrawenable",
        |world, receiver, _| thermal_draw(world, receiver, true),
    );
    registry.register(
        Namespace::Method,
        "thermaldrawdisable",
        |world, receiver, _| thermal_draw(world, receiver, false),
    );
    registry.register(Namespace::Function, "getaicount", |world, _, args| {
        let (teams, species) = filter(args)?;
        Ok(Value::Int(
            population(world.resource::<RoundScript>(), teams, species, false).len() as i32,
        ))
    });
    registry.register(
        Namespace::Function,
        "getaispeciesarray",
        |world, _, args| {
            let (teams, species) = filter(args)?;
            new_array(
                world,
                population(world.resource::<RoundScript>(), teams, species, false),
            )
        },
    );
    registry.register(Namespace::Function, "getaiarray", |world, _, args| {
        let mut teams = super::spawners::team_flags(args)?;
        if teams == 0 {
            teams = 0x1e;
        }
        new_array(
            world,
            population(world.resource::<RoundScript>(), teams, None, true),
        )
    });
    registry.register(
        Namespace::Method,
        "setengagementmindist",
        |world, receiver, args| set_engagement(world, receiver, args, true),
    );
    registry.register(
        Namespace::Method,
        "setengagementmaxdist",
        |world, receiver, args| set_engagement(world, receiver, args, false),
    );
    registry.register(
        Namespace::Method,
        "allowedstances",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            if args.is_empty() {
                return Err("allowedstances requires at least one stance".into());
            }
            let mut mask = 0;
            for i in 0..args.len() {
                mask |= stance(text(args, i)?)?;
            }
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .allowed_stances = mask;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "isstanceallowed",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let requested = stance(text(args, 0)?)?;
            let runtime = world.resource::<RoundScript>();
            let actor = &runtime.actors[&id];
            let restriction = actor
                .path_stances
                .or_else(|| {
                    if actor.at_claim_node {
                        actor
                            .claimed_node
                            .and_then(|index| runtime.path_nodes.allowed_stances(index))
                    } else {
                        None
                    }
                })
                .unwrap_or(7);
            let mut mask = actor.allowed_stances & restriction;
            if mask == 0 {
                mask = actor.allowed_stances;
            }
            Ok(Value::Int(i32::from(mask & requested != 0)))
        },
    );
}
