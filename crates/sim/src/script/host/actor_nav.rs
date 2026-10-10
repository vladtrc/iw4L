use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use bevy_ecs::prelude::World;

use super::args::{arg, float, string, vector};
use super::entities::EntityKind;
use crate::bullet_collision::MASK_PLAYER_SOLID;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};

/// A path node as actors use it.
#[derive(Clone, Debug)]
pub struct NavNode {
    pub kind: NavNodeKind,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub targetname: String,
    pub target: String,
    pub animscript: String,
    /// Linked node, link length, and whether crossing it is a traversal.
    pub links: Vec<(u16, f32, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavNodeKind {
    Path,
    NegotiationBegin,
    NegotiationEnd,
    Other,
}

impl NavNodeKind {
    fn script_type(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::NegotiationBegin => "Begin",
            Self::NegotiationEnd => "End",
            Self::Other => "Other",
        }
    }
}

/// The map's authored path network.
#[derive(Debug, Default)]
pub struct ActorPaths {
    nodes: Vec<NavNode>,
}

/// Sight lines run between eyes this high above the feet.
const EYE_HEIGHT: f32 = 60.0;
/// How far an actor's melee reaches, and how far off its facing.
const MELEE_REACH: f32 = 96.0;
const MELEE_ARC: f32 = 60.0;
const DEFAULT_MELEE_DAMAGE: i32 = 60;

/// How many of the closest nodes are traced for a straight walk.
const REACH_CANDIDATES: usize = 12;

/// Nodes farther above or below a position than this do not serve it.
const NODE_STEP_HEIGHT: f32 = 72.0;

impl ActorPaths {
    pub fn new(nodes: Vec<NavNode>) -> Self {
        Self { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The closest node with nothing solid between it and `at`, or the
    /// closest node at all when none of the nearest few is in reach.
    fn nearest(
        &self,
        at: [f32; 3],
        reach: &mut dyn FnMut([f32; 3], [f32; 3]) -> bool,
    ) -> Option<u16> {
        let mut candidates: Vec<(f32, u16)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| (node.origin[2] - at[2]).abs() <= NODE_STEP_HEIGHT)
            .map(|(index, node)| (distance_sq(node.origin, at), index as u16))
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates
            .iter()
            .take(REACH_CANDIDATES)
            .find(|(_, node)| reach(at, self.nodes[*node as usize].origin))
            .or(candidates.first())
            .map(|(_, node)| *node)
    }

    /// The node sequence from the node serving `from` to the one serving `to`.
    fn plan(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        reach: &mut dyn FnMut([f32; 3], [f32; 3]) -> bool,
    ) -> Option<Vec<u16>> {
        let start = self.nearest(from, reach)?;
        let goal = self.nearest(to, reach)?;
        let goal_origin = self.nodes[goal as usize].origin;
        let mut best: BTreeMap<u16, (f32, Option<u16>)> = BTreeMap::new();
        best.insert(start, (0.0, None));
        let mut open = BinaryHeap::new();
        open.push(Frontier {
            estimate: distance_sq(self.nodes[start as usize].origin, goal_origin).sqrt(),
            node: start,
        });
        while let Some(Frontier { node, .. }) = open.pop() {
            if node == goal {
                let mut path = vec![goal];
                let mut cursor = goal;
                while let Some(&(_, Some(previous))) = best.get(&cursor) {
                    path.push(previous);
                    cursor = previous;
                }
                path.reverse();
                // Already past the first node: head for the second.
                if let [first, second, ..] = path[..] {
                    let (a, b) = (
                        self.nodes[first as usize].origin,
                        self.nodes[second as usize].origin,
                    );
                    if distance_sq(from, b) < distance_sq(a, b) && reach(from, b) {
                        path.remove(0);
                    }
                }
                return Some(path);
            }
            let cost = best[&node].0;
            for &(next, length, _) in &self.nodes[node as usize].links {
                let reached = cost + length.max(1.0);
                if best.get(&next).is_some_and(|(known, _)| *known <= reached) {
                    continue;
                }
                best.insert(next, (reached, Some(node)));
                open.push(Frontier {
                    estimate: reached
                        + distance_sq(self.nodes[next as usize].origin, goal_origin).sqrt(),
                    node: next,
                });
            }
        }
        None
    }
}

#[derive(PartialEq)]
struct Frontier {
    estimate: f32,
    node: u16,
}

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// How an actor's animation moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AnimMode {
    /// The engine walks the actor along its path at the animation's speed.
    #[default]
    Normal,
    /// The animation moves the actor; gravity keeps it on the ground.
    Gravity,
    /// The animation moves the actor horizontally; the ground sets its height.
    ZOnlyPhysics,
    /// The animation moves the actor freely.
    NoGravity,
    /// Only the animation's turn applies.
    AngleDeltas,
    None,
}

/// What an actor faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Orient {
    #[default]
    Motion,
    Enemy,
    Angle(f32),
    Point([f32; 3]),
    Current,
}

#[derive(Clone, Debug)]
pub(crate) enum Goal {
    Position([f32; 3]),
    Entity(u64),
}

/// An actor's goal, path and movement modes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ActorMove {
    goal: Option<Goal>,
    /// Where the path was planned to; an entity goal replans when it moves.
    planned_to: Option<[f32; 3]>,
    waypoints: Vec<[f32; 3]>,
    /// The path node each waypoint stands on; the goal point has none.
    waypoint_nodes: Vec<Option<u16>>,
    /// The traversal the actor is playing between two negotiation nodes.
    pub(crate) traversal: Option<Traversal>,
    pub(crate) anim_mode: AnimMode,
    pub(crate) orient: Orient,
    /// Stances a script allows; none recorded means all.
    stances: Option<Vec<String>>,
    /// Direction of travel over the last tick, in degrees.
    heading: Option<f32>,
    /// A scripted animation placing the actor, once started.
    scripted: Option<Scripted>,
    /// `AnimScripted` was called; the scripted animscript starts it.
    scripted_pending: bool,
}

/// A negotiation link being crossed by its begin node's animscript.
#[derive(Clone, Debug)]
pub(crate) struct Traversal {
    pub(crate) begin: u16,
    pub(crate) end: u16,
    pub(crate) script: String,
    pub(crate) thread: Option<u64>,
}

/// A scripted animation aligned to a point: the actor stands where the
/// animation's root motion puts it relative to `origin` and `yaw`.
#[derive(Clone, Debug)]
struct Scripted {
    origin: [f32; 3],
    yaw: f32,
    node: u16,
}

impl ActorMove {
    pub(crate) fn has_path(&self) -> bool {
        !self.waypoints.is_empty()
    }

    fn clear_path(&mut self) {
        self.waypoints.clear();
        self.waypoint_nodes.clear();
    }

    fn drop_waypoint(&mut self) {
        self.waypoints.remove(0);
        if !self.waypoint_nodes.is_empty() {
            self.waypoint_nodes.remove(0);
        }
    }

    /// Ends a finished traversal; the path resumes past its end node.
    pub(crate) fn finish_traversal(&mut self) {
        let Some(traversal) = self.traversal.take() else {
            return;
        };
        if self.waypoint_nodes.first() == Some(&Some(traversal.end)) {
            self.drop_waypoint();
        }
    }

    /// A scripted animation was asked for and has not started yet.
    pub(crate) fn scripted_waiting(&self) -> bool {
        self.scripted_pending && self.scripted.is_none()
    }

    pub(crate) fn is_scripted(&self) -> bool {
        self.scripted.is_some() || self.scripted_pending
    }
}

/// A goal set this close to the current one keeps the current path.
const SAME_GOAL_DISTANCE: f32 = 16.0;
/// A goal entity that moves farther than this is chased with a new path.
const REPLAN_DISTANCE: f32 = 64.0;
/// A waypoint closer than this is passed.
const WAYPOINT_REACHED: f32 = 16.0;
/// When a script names no radius.
const DEFAULT_GOAL_RADIUS: f32 = 32.0;
/// Degrees an actor turns per second toward what it faces.
const TURN_RATE: f32 = 360.0;
/// The actor's collision box for ground traces.
const ACTOR_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const ACTOR_MAXS: [f32; 3] = [15.0, 15.0, 18.0];
const GROUND_PROBE_STARTS: [f32; 3] = [48.0, 18.0, 1.0];
const GROUND_PROBE_DOWN: f32 = 64.0;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getnode", |world, _, args| {
        let found = find_nodes(world, args)?;
        Ok(found.into_iter().next().unwrap_or(Value::Undefined))
    });
    registry.register(Function, "getnodearray", |world, _, args| {
        let found = find_nodes(world, args)?;
        super::arrays::new_array(world, found)
    });
    registry.register(Method, "setgoalpos", |world, receiver, args| {
        let goal = vector(args, 0)?;
        set_goal(world, receiver, Goal::Position(goal))
    });
    registry.register(Method, "setgoalnode", |world, receiver, args| {
        let node = arg(args, 0)?.clone();
        let Value::Object(id) = node else {
            return Err("expects a path node".into());
        };
        let origin = match world.resource_mut::<Runtime>().object_field(id, "origin") {
            Value::Vector(origin) => origin,
            _ => return Err("path node has no origin".into()),
        };
        set_goal(world, receiver, Goal::Position(origin))
    });
    registry.register(Method, "setgoalentity", |world, receiver, args| {
        let target = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        set_goal(world, receiver, Goal::Entity(target))
    });
    registry.register(Method, "animscripted", |world, receiver, args| {
        let Ok(id) = actor_id(world, receiver) else {
            return model_anim_scripted(world, receiver, args);
        };
        let init = format!(
            "animscripts/{}scripted::init",
            super::actor_brain::prefix(world, id)
        );
        let now = super::players::now_ms(world);
        crate::script::runtime::run_now(world, &init, receiver.clone(), args.to_vec(), now)
            .map_err(|fault| format!("{init}: {fault:?}"))?;
        with_move(world, receiver, |movement| {
            movement.scripted = None;
            movement.scripted_pending = true;
        })
    });
    registry.register(Method, "startscriptedanim", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let notify = string(args, 0)?;
        let origin = vector(args, 1)?;
        let angles = vector(args, 2)?;
        let anim = arg(args, 3)?.clone();
        let root = args.get(5).filter(|root| **root != Value::Undefined);
        let rate = match args.get(6) {
            Some(Value::Float(rate)) => *rate,
            Some(Value::Int(rate)) => *rate as f32,
            _ => 1.0,
        };
        let goal_time = match args.get(7) {
            Some(Value::Float(time)) => *time,
            Some(Value::Int(time)) => *time as f32,
            _ => 0.2,
        };
        let node =
            super::actor_anims::play_scripted(world, id, &notify, &anim, root, rate, goal_time)?;
        with_move(world, receiver, |movement| {
            movement.scripted = Some(Scripted {
                origin,
                yaw: angles[1],
                node,
            });
            movement.scripted_pending = false;
            movement.clear_path();
        })
    });
    registry.register(Method, "stopanimscripted", |world, receiver, _| {
        with_move(world, receiver, |movement| {
            movement.scripted = None;
            movement.scripted_pending = false;
        })
    });
    registry.register(Method, "forceteleport", force_teleport);
    registry.register(Method, "teleport", force_teleport);
    registry.register(Method, "calcpathlength", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let to = vector(args, 0)?;
        let (from, _) = pose(&mut world.resource_mut::<Runtime>(), id);
        let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
            return Ok(Value::Float(distance_sq(from, to).sqrt()));
        };
        let mut reach = |a: [f32; 3], b: [f32; 3]| walkable(world, a, b);
        let Some(nodes) = paths.plan(from, to, &mut reach) else {
            return Ok(Value::Undefined);
        };
        let mut length = 0.0;
        let mut at = from;
        for point in nodes
            .iter()
            .map(|&node| paths.nodes[node as usize].origin)
            .chain(std::iter::once(to))
        {
            length += distance_sq(at, point).sqrt();
            at = point;
        }
        Ok(Value::Float(length))
    });
    for (name, end) in [
        ("getnegotiationstartnode", false),
        ("getnegotiationendnode", true),
    ] {
        let native: crate::script::Native = if end {
            |world, receiver, _| negotiation_node(world, receiver, true)
        } else {
            |world, receiver, _| negotiation_node(world, receiver, false)
        };
        registry.register(Method, name, native);
    }
    // Traversals move by their animation alone until the animscript ends.
    registry.register(Method, "traversemode", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "maymovetopoint", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let to = vector(args, 0)?;
        let (from, _) = pose(&mut world.resource_mut::<Runtime>(), id);
        Ok(Value::Int(walkable(world, from, to).into()))
    });
    registry.register(Method, "maymovefrompointtopoint", |world, _, args| {
        let (from, to) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Int(walkable(world, from, to).into()))
    });
    registry.register(Method, "isingoal", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let point = vector(args, 0)?;
        let radius = goal_radius(world, id);
        let inside =
            goal_position(world, id).is_none_or(|goal| horizontal_distance(goal, point) <= radius);
        Ok(Value::Int(inside.into()))
    });
    registry.register(Method, "cansee", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let target = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        let (origin, _) = pose(&mut world.resource_mut::<Runtime>(), id);
        let Some(at) = as_vector(super::players::entity_field(world, target, "origin")) else {
            return Ok(Value::Int(0));
        };
        let trace = FrameWorld::from_world(world).trace_clip(
            [origin[0], origin[1], origin[2] + EYE_HEIGHT],
            [at[0], at[1], at[2] + EYE_HEIGHT],
            [0.0; 3],
            [0.0; 3],
            MASK_PLAYER_SOLID,
        );
        Ok(Value::Int(i32::from(
            trace.startsolid == 0 && trace.fraction >= 1.0,
        )))
    });
    registry.register(Method, "melee", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (origin, angles) = pose(&mut world.resource_mut::<Runtime>(), id);
        let enemy = match world.resource_mut::<Runtime>().object_field(id, "enemy") {
            Value::Object(enemy) => enemy,
            _ => return Ok(Value::Undefined),
        };
        let Some(client) = world.resource::<Runtime>().player_client(enemy) else {
            return Ok(Value::Undefined);
        };
        let Some(at) = as_vector(super::players::entity_field(world, enemy, "origin")) else {
            return Ok(Value::Undefined);
        };
        let facing = math_iw4::angle_subtract(yaw_to(origin, at), angles[1]).abs();
        if horizontal_distance(origin, at) > MELEE_REACH
            || (origin[2] - at[2]).abs() > NODE_STEP_HEIGHT
            || facing > MELEE_ARC
        {
            return Ok(Value::Undefined);
        }
        let amount = match world
            .resource_mut::<Runtime>()
            .object_field(id, "meleedamage")
        {
            Value::Int(n) => n,
            Value::Float(f) => f as i32,
            _ => DEFAULT_MELEE_DAMAGE,
        };
        let inflictor = world.resource::<Runtime>().presence_of(receiver);
        world
            .resource_mut::<Runtime>()
            .hits
            .push(crate::script::ScriptHit {
                piece: None,
                target: crate::script::HitTarget::Player(crate::ClientId(client)),
                amount,
                origin,
                attacker: None,
                inflictor,
                means: "MOD_MELEE",
                weapon: 0,
                flags: 0,
                hitloc: 0,
            });
        Ok(Value::Object(enemy))
    });
    registry.register(Method, "allowedstances", |world, receiver, args| {
        let stances = (0..args.len())
            .map(|index| string(args, index))
            .collect::<Result<Vec<_>, _>>()?;
        with_move(world, receiver, |movement| movement.stances = Some(stances))
    });
    registry.register(Method, "isstanceallowed", |world, receiver, args| {
        let stance = string(args, 0)?;
        let id = actor_id(world, receiver)?;
        let allowed = world
            .resource::<Runtime>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.stances.as_ref())
            .is_none_or(|stances| stances.iter().any(|s| *s == stance));
        Ok(Value::Int(allowed.into()))
    });
    registry.register(Method, "getmotionangle", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (_, angles) = pose(&mut world.resource_mut::<Runtime>(), id);
        let heading = world
            .resource::<Runtime>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.heading);
        Ok(Value::Float(heading.map_or(0.0, |heading| {
            math_iw4::angle_subtract(heading, angles[1])
        })))
    });
    registry.register(Method, "getclosestenemysqdist", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (origin, _) = pose(&mut world.resource_mut::<Runtime>(), id);
        Ok(Value::Float(
            enemy_position(world, id).map_or(f32::MAX, |enemy| distance_sq(origin, enemy)),
        ))
    });
    registry.register(Method, "animmode", |world, receiver, args| {
        let mode = match string(args, 0)?.as_str() {
            "normal" => AnimMode::Normal,
            "gravity" => AnimMode::Gravity,
            "zonly_physics" => AnimMode::ZOnlyPhysics,
            "nogravity" | "noclip" => AnimMode::NoGravity,
            "angle deltas" => AnimMode::AngleDeltas,
            "none" => AnimMode::None,
            other => return Err(format!("unknown animmode '{other}'")),
        };
        with_move(world, receiver, |movement| movement.anim_mode = mode)
    });
    registry.register(Method, "orientmode", |world, receiver, args| {
        let orient = match string(args, 0)?.as_str() {
            "face motion" | "face default" => Orient::Motion,
            "face enemy" | "face enemy or motion" => Orient::Enemy,
            "face angle" => Orient::Angle(float(args, 1)?),
            "face point" => Orient::Point(vector(args, 1)?),
            "face current" => Orient::Current,
            other => return Err(format!("unknown orientmode '{other}'")),
        };
        with_move(world, receiver, |movement| movement.orient = orient)
    });
}

fn negotiation_node(world: &mut World, receiver: &Value, end: bool) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let Some(traversal) = world
        .resource::<Runtime>()
        .actor_moves
        .get(&id)
        .and_then(|movement| movement.traversal.clone())
    else {
        return Ok(Value::Undefined);
    };
    let paths = FrameWorld::from_world(world)
        .actor_paths()
        .ok_or("the map has no path network")?;
    node_object(
        world,
        &paths,
        if end { traversal.end } else { traversal.begin },
    )
}

fn actor_id(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<Runtime>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("receiver is not an actor".into()),
    }
}

fn with_move(
    world: &mut World,
    receiver: &Value,
    apply: impl FnOnce(&mut ActorMove),
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    apply(
        world
            .resource_mut::<Runtime>()
            .actor_moves
            .entry(id)
            .or_default(),
    );
    Ok(Value::Undefined)
}

fn force_teleport(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let origin = vector(args, 0)?;
    let angles = match args.get(1) {
        Some(Value::Vector(angles)) => Some(*angles),
        _ => None,
    };
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    if let Some(angles) = angles {
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
    let movement = runtime.actor_moves.entry(id).or_default();
    movement.clear_path();
    movement.planned_to = None;
    Ok(Value::Undefined)
}

fn set_goal(world: &mut World, receiver: &Value, goal: Goal) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let mut runtime = world.resource_mut::<Runtime>();
    let movement = runtime.actor_moves.entry(id).or_default();
    // Scripts re-issue the same goal every frame; only a goal that moved
    // replans, as an entity goal does once it strays from the plan.
    let same = match (&movement.goal, &goal) {
        (Some(Goal::Position(old)), Goal::Position(new)) => {
            distance_sq(*old, *new) < SAME_GOAL_DISTANCE * SAME_GOAL_DISTANCE
        }
        (Some(Goal::Entity(old)), Goal::Entity(new)) => old == new,
        _ => false,
    };
    movement.goal = Some(goal);
    if !same {
        movement.planned_to = None;
        movement.clear_path();
    }
    Ok(Value::Undefined)
}

/// `GetNode(value, key)` and `GetNodeArray(value, key)`: nodes whose key matches.
fn find_nodes(world: &mut World, args: &[Value]) -> Result<Vec<Value>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?;
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return Ok(Vec::new());
    };
    let matches: Vec<u16> = (0..paths.nodes.len())
        .filter(|&index| {
            let node = &paths.nodes[index];
            match key.as_str() {
                "targetname" => node.targetname == value,
                "target" => node.target == value,
                "animscript" => node.animscript == value,
                _ => false,
            }
        })
        .map(|index| index as u16)
        .collect();
    matches
        .into_iter()
        .map(|index| node_object(world, &paths, index))
        .collect()
}

/// The script object standing for a path node, made on first use.
fn node_object(world: &mut World, paths: &ActorPaths, index: u16) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    if let Some(&id) = runtime.path_node_objects.get(&index)
        && runtime.live(&id)
    {
        return Ok(Value::Object(id));
    }
    let node = &paths.nodes[index as usize];
    let id = runtime.new_object()?;
    runtime.set_object_field(id, "origin", Value::Vector(node.origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0, node.yaw, 0.0]));
    runtime.set_object_field(id, "type", Value::string(node.kind.script_type()));
    for (name, text) in [
        ("targetname", &node.targetname),
        ("target", &node.target),
        ("animscript", &node.animscript),
    ] {
        if !text.is_empty() {
            runtime.set_object_field(id, name, Value::string(text));
        }
    }
    runtime.path_node_objects.insert(index, id);
    Ok(Value::Object(id))
}

fn pose(runtime: &mut Runtime, id: u64) -> ([f32; 3], [f32; 3]) {
    let vector = |runtime: &mut Runtime, name| match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    (vector(runtime, "origin"), vector(runtime, "angles"))
}

/// Plans paths for new goals, walks each actor by its animation and reports
/// `goal` and `bad_path`.
/// `AnimScripted(notify, origin, angles, anim, ...)` on a script model: it
/// plays the clip once from that pose and reports `end` under `notify` when
/// the clip is done.
fn model_anim_scripted(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = super::natives::engine::entity_id(world, receiver)?;
    let notify = string(args, 0)?;
    let (origin, angles) = (vector(args, 1)?, vector(args, 2)?);
    let clip = match arg(args, 3)? {
        Value::Animation { name, .. } => name.clone(),
        other => return Err(format!("{other:?} is not an animation")),
    };
    let seconds = crate::frame::FrameWorld::from_world(world)
        .anim_clip_named(&clip)
        .map_or(0.0, |clip| {
            f32::from(clip.numframes.max(1)) / clip.framerate.max(1.0)
        });
    let now = i64::from(super::players::now_ms(world));
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector(angles));
    if let Some(entity) = runtime.entities.get_mut(&id) {
        entity.anim_op = Some(Some(clip));
    }
    runtime.timed_notifies.push((
        now + (seconds * 1000.0) as i64,
        receiver.clone(),
        notify.into(),
        vec![Value::string("end")],
    ));
    Ok(Value::Undefined)
}

pub(crate) fn locomote(world: &mut World, seconds: f32) {
    let actors: Vec<u64> = world
        .resource::<Runtime>()
        .actor_moves
        .keys()
        .copied()
        .collect();
    let paths = FrameWorld::from_world(world).actor_paths();
    for actor in actors {
        if !world.resource::<Runtime>().entities.contains_key(&actor) {
            world.resource_mut::<Runtime>().actor_moves.remove(&actor);
            continue;
        }
        let (mut origin, mut angles) = pose(&mut world.resource_mut::<Runtime>(), actor);
        let goal_radius = match world
            .resource_mut::<Runtime>()
            .object_field(actor, "goalradius")
        {
            Value::Int(n) => n as f32,
            Value::Float(f) => f,
            _ => DEFAULT_GOAL_RADIUS,
        };
        let goal_at = goal_position(world, actor);
        let mut movement = world.resource::<Runtime>().actor_moves[&actor].clone();
        let mut events: Vec<&'static str> = Vec::new();

        if let Some(goal_at) = goal_at {
            let stale = movement.planned_to.is_none_or(|planned| {
                distance_sq(planned, goal_at) > REPLAN_DISTANCE * REPLAN_DISTANCE
            });
            if stale {
                movement.planned_to = Some(goal_at);
                movement.clear_path();
                if horizontal_distance(origin, goal_at) > goal_radius {
                    let mut reach = |from: [f32; 3], to: [f32; 3]| walkable(world, from, to);
                    let planned = paths
                        .as_deref()
                        .and_then(|paths| paths.plan(origin, goal_at, &mut reach));
                    match planned {
                        Some(nodes) => {
                            let paths = paths.as_deref().unwrap();
                            movement.waypoints = nodes
                                .iter()
                                .map(|&node| paths.nodes[node as usize].origin)
                                .collect();
                            movement.waypoint_nodes =
                                nodes.iter().map(|&node| Some(node)).collect();
                            movement.waypoints.push(goal_at);
                            movement.waypoint_nodes.push(None);
                        }
                        None => events.push("bad_path"),
                    }
                }
            }
        }

        if let Some(scripted) = movement.scripted.clone() {
            match super::actor_anims::node_clip(world, actor, scripted.node) {
                Some((clip, time)) => {
                    let local = clip.abs_delta_trans(time);
                    let (sin, cos) = scripted.yaw.to_radians().sin_cos();
                    origin = [
                        scripted.origin[0] + local[0] * cos - local[1] * sin,
                        scripted.origin[1] + local[0] * sin + local[1] * cos,
                        scripted.origin[2] + local[2],
                    ];
                    angles[1] = scripted.yaw + clip.abs_delta_yaw(time);
                    if !clip.looping && time >= 1.0 {
                        movement.scripted = None;
                    }
                }
                None => movement.scripted = None,
            }
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(actor, "origin", Value::Vector(origin));
            runtime.set_object_field(actor, "angles", Value::Vector(angles));
            runtime.actor_moves.insert(actor, movement);
            continue;
        }
        let delta = super::actor_anims::root_delta(world, actor);
        let (local, turned) = delta.unwrap_or(([0.0; 3], 0.0));
        let (sin, cos) = angles[1].to_radians().sin_cos();
        let world_delta = [
            local[0] * cos - local[1] * sin,
            local[0] * sin + local[1] * cos,
            local[2],
        ];
        let mut heading = None;
        match movement.anim_mode {
            _ if movement.traversal.is_some() => {
                for axis in 0..3 {
                    origin[axis] += world_delta[axis];
                }
                angles[1] += turned;
            }
            AnimMode::Normal => {
                while movement
                    .waypoints
                    .first()
                    .is_some_and(|&next| horizontal_distance(origin, next) < WAYPOINT_REACHED)
                    && movement.waypoints.len() > 1
                {
                    if let Some(traversal) = paths
                        .as_deref()
                        .and_then(|paths| traversal_at(paths, &movement.waypoint_nodes))
                    {
                        movement.drop_waypoint();
                        movement.traversal = Some(traversal);
                        break;
                    }
                    movement.drop_waypoint();
                }
                if let Some(&next) = movement.waypoints.first() {
                    let step = (world_delta[0].powi(2) + world_delta[1].powi(2)).sqrt();
                    let to = [next[0] - origin[0], next[1] - origin[1]];
                    let length = (to[0] * to[0] + to[1] * to[1]).sqrt();
                    if length > 0.001 {
                        let advance = step.min(length);
                        origin[0] += to[0] / length * advance;
                        origin[1] += to[1] / length * advance;
                        heading = Some(to[1].atan2(to[0]).to_degrees());
                    }
                    origin = ground(world, origin);
                }
            }
            AnimMode::Gravity | AnimMode::ZOnlyPhysics => {
                origin[0] += world_delta[0];
                origin[1] += world_delta[1];
                origin = ground(world, origin);
                angles[1] += turned;
            }
            AnimMode::NoGravity => {
                for axis in 0..3 {
                    origin[axis] += world_delta[axis];
                }
                angles[1] += turned;
            }
            AnimMode::AngleDeltas => angles[1] += turned,
            AnimMode::None => {}
        }

        movement.heading = heading;
        let facing = match &movement.orient {
            Orient::Motion => heading,
            Orient::Angle(yaw) => Some(*yaw),
            Orient::Point(point) => Some(yaw_to(origin, *point)),
            Orient::Enemy => enemy_position(world, actor)
                .map(|enemy| yaw_to(origin, enemy))
                .or(heading),
            Orient::Current => None,
        };
        if let Some(wanted) = facing {
            let turn = math_iw4::angle_subtract(wanted, angles[1]);
            let limit = TURN_RATE * seconds;
            angles[1] += turn.clamp(-limit, limit);
        }

        if let Some(goal_at) = goal_at
            && horizontal_distance(origin, goal_at) <= goal_radius
            && movement.planned_to.is_some()
            && (movement.waypoints.len() <= 1)
        {
            if movement.has_path() || !matches!(movement.goal, Some(Goal::Entity(_))) {
                events.push("goal");
            }
            movement.clear_path();
            if matches!(movement.goal, Some(Goal::Position(_))) {
                movement.goal = None;
            }
        }

        {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(actor, "origin", Value::Vector(origin));
            runtime.set_object_field(actor, "angles", Value::Vector(angles));
            runtime.actor_moves.insert(actor, movement);
        }
        for event in events {
            raise(world, Value::Object(actor), event, Vec::new());
        }
    }
}

/// The traversal starting at the path's first node, when the link to the
/// second is a negotiation link.
fn traversal_at(paths: &ActorPaths, nodes: &[Option<u16>]) -> Option<Traversal> {
    let (Some(Some(begin)), Some(Some(end))) = (nodes.first(), nodes.get(1)) else {
        return None;
    };
    let node = &paths.nodes[*begin as usize];
    let negotiation = node
        .links
        .iter()
        .any(|&(to, _, negotiation)| to == *end && negotiation);
    (node.kind == NavNodeKind::NegotiationBegin && negotiation && !node.animscript.is_empty()).then(
        || Traversal {
            begin: *begin,
            end: *end,
            script: node.animscript.clone(),
            thread: None,
        },
    )
}

fn goal_radius(world: &mut World, actor: u64) -> f32 {
    match world
        .resource_mut::<Runtime>()
        .object_field(actor, "goalradius")
    {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => DEFAULT_GOAL_RADIUS,
    }
}

fn goal_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let goal = world
        .resource::<Runtime>()
        .actor_moves
        .get(&actor)?
        .goal
        .clone()?;
    match goal {
        Goal::Position(at) => Some(at),
        Goal::Entity(id) => as_vector(super::players::entity_field(world, id, "origin")),
    }
}

fn enemy_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let enemy = match world
        .resource_mut::<Runtime>()
        .object_field(actor, "favoriteenemy")
    {
        Value::Object(id) => id,
        _ => return None,
    };
    as_vector(super::players::entity_field(world, enemy, "origin"))
}

/// The floor under an actor at `at`, or `at` when there is none in reach.
/// The probe starts high to climb steps and lower when it starts in solid,
/// as when the actor straddles a wall it just climbed.
fn ground(world: &mut World, at: [f32; 3]) -> [f32; 3] {
    let frame = FrameWorld::from_world(world);
    let end = [at[0], at[1], at[2] - GROUND_PROBE_DOWN];
    for up in GROUND_PROBE_STARTS {
        let start = [at[0], at[1], at[2] + up];
        let trace = frame.trace_clip(start, end, ACTOR_MINS, ACTOR_MAXS, MASK_PLAYER_SOLID);
        if trace.startsolid != 0 {
            continue;
        }
        if trace.fraction >= 1.0 {
            return at;
        }
        return [
            at[0],
            at[1],
            start[2] + (end[2] - start[2]) * trace.fraction,
        ];
    }
    at
}

/// Nothing solid lies between two points at knee height.
fn walkable(world: &mut World, from: [f32; 3], to: [f32; 3]) -> bool {
    let lift = |p: [f32; 3]| [p[0], p[1], p[2] + 18.0];
    let trace = FrameWorld::from_world(world).trace_clip(
        lift(from),
        lift(to),
        [-8.0, -8.0, 0.0],
        [8.0, 8.0, 18.0],
        MASK_PLAYER_SOLID,
    );
    trace.startsolid == 0 && trace.fraction >= 1.0
}

fn yaw_to(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

fn horizontal_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Keeps the engine's `enemy` field on the favorite enemy while it lives and
/// the actor does not ignore everyone.
pub(crate) fn choose_enemy(world: &mut World, actor: u64) {
    let mut runtime = world.resource_mut::<Runtime>();
    let ignoring = matches!(runtime.object_field(actor, "ignoreall"), Value::Int(n) if n != 0);
    let favorite = match runtime.object_field(actor, "favoriteenemy") {
        Value::Object(id) if !ignoring && runtime.live(&id) => Some(id),
        _ => None,
    };
    drop(runtime);
    let enemy = favorite.filter(
        |&id| matches!(super::players::entity_field(world, id, "health"), Value::Int(n) if n > 0),
    );
    world.resource_mut::<Runtime>().set_object_field(
        actor,
        "enemy",
        enemy.map_or(Value::Undefined, Value::Object),
    );
}

pub(crate) fn melee_range_enemy(world: &mut World, actor: u64, range: f32) -> bool {
    let Some(enemy) = enemy_position(world, actor) else {
        return false;
    };
    let (origin, _) = pose(&mut world.resource_mut::<Runtime>(), actor);
    horizontal_distance(origin, enemy) <= range && (origin[2] - enemy[2]).abs() <= NODE_STEP_HEIGHT
}

fn as_vector(value: Value) -> Option<[f32; 3]> {
    match value {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}
