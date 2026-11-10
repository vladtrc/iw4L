use super::{
    args::{arg, float},
    arrays::new_array,
    natives::engine::entity_id,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

mod aim;
pub(crate) mod goals;
pub(crate) mod orientation;
pub(crate) mod perception;
pub(crate) mod physics;
pub(crate) mod scripted;
pub(crate) mod spatial;
pub(crate) mod startup;
pub(crate) mod suppression;
pub(crate) mod teleport;

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
    pub motion: physics::ActorPhysics,
    pub orientation: orientation::Orientation,
    pub suppression: suppression::Suppression,
    pub scripted_control: Option<scripted::ScriptedControl>,
    pub anim_script_thread: Option<u64>,
    pub anim_script_kind: Option<scripted::ControlKind>,
    pub previous_anim_script: Option<scripted::ControlKind>,
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
            motion: Default::default(),
            orientation: Default::default(),
            suppression: Default::default(),
            scripted_control: None,
            anim_script_thread: None,
            anim_script_kind: None,
            previous_anim_script: None,
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
    if let Some(value) = actor.suppression.load(field) {
        return Some(value);
    }
    if let Some(value) = goals::load(&actor.goals, field) {
        return Some(value);
    }
    if let Some(index) = aim::FIELDS.iter().position(|name| *name == field) {
        return Some(Value::Float(actor.aim_limits[index]));
    }
    match field {
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
    if (["type", "delayeddeath", "turnrate"].contains(&field)
        || suppression::FIELDS.contains(&field)
        || aim::FIELDS.contains(&field)
        || goals::FIELDS.contains(&field))
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
    teleport::register(registry);
    scripted::register(registry);
    physics::register(registry);
    orientation::register(registry);
    suppression::register(registry);
    spatial::register(registry);
    aim::register(registry);
    goals::register(registry);
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
