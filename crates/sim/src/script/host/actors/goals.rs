use super::super::args::{int, vector};
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Goal {
    pub position: [f32; 3],
    pub radius: f32,
    pub height: f32,
    pub node: Option<u16>,
    pub volume: Option<u64>,
}
impl Default for Goal {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            radius: 4.0,
            height: 80.0,
            node: None,
            volume: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Goals {
    pub script: Goal,
    pub code: Goal,
    pub entity: Option<u64>,
    pub entity_update_interval: i32,
    pub last_entity_update: i32,
    pub keep_claimed: bool,
    pub keep_claimed_if_valid: bool,
    pub arrival_time: i32,
    pub arrival_duration: i32,
}
impl Default for Goals {
    fn default() -> Self {
        Self {
            script: Goal::default(),
            code: Goal::default(),
            entity: None,
            entity_update_interval: 2500,
            last_entity_update: 0,
            keep_claimed: false,
            keep_claimed_if_valid: false,
            arrival_time: 0,
            arrival_duration: 0,
        }
    }
}
impl Goals {
    pub(super) fn initialize(&mut self, origin: [f32; 3]) {
        *self = Self::default();
        self.script.position = origin;
        self.code.position = origin;
    }
    fn clear_arrival(&mut self) {
        self.arrival_time = 0;
        self.arrival_duration = 0;
    }
    fn clear_keep_claimed(&mut self) {
        self.keep_claimed = false;
        self.keep_claimed_if_valid = false;
        self.clear_arrival();
    }
    pub(crate) fn trace_values(&self, values: &mut Vec<Value>) {
        values.extend(
            [self.entity, self.script.volume, self.code.volume]
                .into_iter()
                .flatten()
                .map(Value::Object),
        );
    }
    pub(crate) fn remove_entity(&mut self, id: u64) {
        if self.entity == Some(id) {
            self.entity = None;
        }
        if self.script.volume == Some(id) {
            self.script.volume = None;
        }
        if self.code.volume == Some(id) {
            self.code.volume = None;
        }
    }
}

pub(super) const FIELDS: [&str; 5] = [
    "goalradius",
    "goalheight",
    "goalpos",
    "keepclaimednode",
    "keepclaimednodeifvalid",
];
pub(super) fn load(goals: &Goals, field: &str) -> Option<Value> {
    Some(match field {
        "goalradius" => Value::Float(goals.script.radius),
        "goalheight" => Value::Float(goals.script.height),
        "goalpos" => Value::Vector(goals.code.position),
        "keepclaimednode" => Value::Int(i32::from(goals.keep_claimed)),
        "keepclaimednodeifvalid" => Value::Int(i32::from(goals.keep_claimed_if_valid)),
        _ => return None,
    })
}
pub(super) fn store(goals: &mut Goals, field: &str, value: &Value) -> Result<bool, String> {
    match field {
        "goalpos" => return Err("actor goalpos is read-only".into()),
        "goalradius" | "goalheight" => {
            let value = float(std::slice::from_ref(value), 0)?;
            if !value.is_finite() {
                return Err("goal dimensions must be finite".into());
            }
            if field == "goalradius" {
                if value < 0.0 {
                    return Err("goalradius must be nonnegative".into());
                }
                goals.script.radius = value.max(4.0);
            } else {
                goals.script.height = value.max(80.0);
            }
        }
        "keepclaimednode" => goals.keep_claimed = int(std::slice::from_ref(value), 0)? != 0,
        "keepclaimednodeifvalid" => {
            goals.keep_claimed_if_valid = int(std::slice::from_ref(value), 0)? != 0
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn finite_vector(args: &[Value]) -> Result<[f32; 3], String> {
    let position = vector(args, 0)?;
    if position.into_iter().all(f32::is_finite) {
        Ok(position)
    } else {
        Err("goal position must be finite".into())
    }
}
fn set_position(
    world: &mut World,
    id: u64,
    position: [f32; 3],
    node: Option<u16>,
) -> Result<Value, String> {
    let volume = world.resource::<RoundScript>().actors[&id]
        .goals
        .script
        .volume;
    let outside = volume
        .map(|volume| super::super::triggers::contact_point(world, volume, position))
        .transpose()?
        .is_some_and(|inside| !inside);
    let mut runtime = world.resource_mut::<RoundScript>();
    let goals = &mut runtime.actors.get_mut(&id).unwrap().goals;
    goals.entity = None;
    if goals.script.position != position || goals.script.node != node {
        goals.keep_claimed = false;
        goals.clear_arrival();
        goals.script.position = position;
        goals.script.node = node;
    }
    if outside {
        goals.script.volume = None;
    }
    Ok(Value::Undefined)
}

pub(crate) fn update_one(world: &mut World, id: u64, now: i32) -> Result<(), String> {
    let goals = &world.resource::<RoundScript>().actors[&id].goals;
    let mut code = goals.script.clone();
    code.radius = code.radius.max(4.0);
    code.height = code.height.max(80.0);
    let update = goals.entity.filter(|_| {
        goals
            .last_entity_update
            .wrapping_add(goals.entity_update_interval)
            < now
    });
    if goals.entity.is_some() {
        code.node = None;
        code.volume = None;
        code.position = goals.code.position;
    }
    if let Some(entity) = update {
        let Value::Vector(position) = super::super::players::entity_field(world, entity, "origin")
        else {
            return Err("goal entity has no physical origin".into());
        };
        if !position.into_iter().all(f32::is_finite) {
            return Err("goal entity origin must be finite".into());
        }
        code.position = position;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let goals = &mut runtime.actors.get_mut(&id).unwrap().goals;
    if update.is_some() {
        goals.last_entity_update = now;
    }
    goals.code = code;
    Ok(())
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(_, actor)| actor.alive && actor.physics_initialized)
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        update_one(world, id, now)?;
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "getgoalvolume", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        Ok(world.resource::<RoundScript>().actors[&id]
            .goals
            .script
            .volume
            .map_or(Value::Undefined, Value::Object))
    });
    registry.register(
        Namespace::Method,
        "cleargoalvolume",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .goals
                .script
                .volume = None;
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Method, "setgoalpos", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        set_position(world, id, finite_vector(args)?, None)
    });
    registry.register(Namespace::Method, "setgoalnode", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let (node, position) = {
            let runtime = world.resource::<RoundScript>();
            let value = arg(args, 0)?;
            let index = runtime.path_nodes.index(value)?;
            let node = runtime.path_nodes.node(value)?;
            if node.links.is_empty() && node.spawnflags & 1 == 0 {
                diag::warn!(Sim, "SetGoalNode: unlinked node at {:?}", node.origin);
            }
            if !node.origin.into_iter().all(f32::is_finite) {
                return Err("goal node origin must be finite".into());
            }
            (index, node.origin)
        };
        set_position(world, id, position, Some(node))
    });
    registry.register(
        Namespace::Method,
        "setgoalentity",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let entity = entity_id(world, arg(args, 0)?)?;
            let interval = if args.len() > 1 {
                Some(int(args, 1)?)
            } else {
                None
            };
            let mut runtime = world.resource_mut::<RoundScript>();
            let goals = &mut runtime.actors.get_mut(&id).unwrap().goals;
            goals.clear_keep_claimed();
            goals.entity = Some(entity);
            goals.script.node = None;
            goals.script.volume = None;
            if let Some(interval) = interval {
                goals.entity_update_interval = interval;
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "setgoalvolume",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let (target, position) = {
                let goals = &world.resource::<RoundScript>().actors[&id].goals;
                (goals.entity, goals.script.position)
            };
            if target.is_some() {
                return Err("SetGoalVolume cannot constrain an entity goal".into());
            }
            let volume = entity_id(world, arg(args, 0)?)?;
            if !super::super::triggers::contact_point(world, volume, position)? {
                return Err("goal position is outside the goal volume".into());
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            if matches!(runtime.object_field(id, "fixednode"), Value::Int(value) if value != 0) {
                diag::warn!(Sim, "SetGoalVolume: actor has fixednode enabled");
            }
            let goals = &mut runtime.actors.get_mut(&id).unwrap().goals;
            goals.clear_keep_claimed();
            goals.script.volume = Some(volume);
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Method, "isingoal", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        if args.len() != 1 {
            return Err("IsInGoal requires one position".into());
        }
        let position = finite_vector(args)?;
        let goal = world.resource::<RoundScript>().actors[&id]
            .goals
            .code
            .clone();
        let delta: [f32; 3] = std::array::from_fn(|i| position[i] - goal.position[i]);
        let near = delta[2] * delta[2] <= goal.height * goal.height
            && delta[0] * delta[0] + delta[1] * delta[1] <= goal.radius * goal.radius;
        let inside = near
            && match goal.volume {
                Some(volume) => super::super::triggers::contact_point(world, volume, position)?,
                None => true,
            };
        Ok(Value::Int(i32::from(inside)))
    });
}
