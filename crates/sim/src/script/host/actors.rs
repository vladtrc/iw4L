use bevy_ecs::prelude::World;
use std::sync::Arc;

use super::arrays::new_array;
use super::entities::EntityKind;
use super::natives::engine::entity_id;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

mod awareness;
mod campaign;
mod events;
pub(crate) use events::projectile_impact;
mod grenades;
mod prone;
pub(crate) use campaign::{BadPlace, PlayerLos, ThreatBias};

const SPAWNER: i32 = 1;
pub(crate) const VEHICLE_SPAWNER: i32 = 2;

pub trait ActorNav: Send + Sync {
    fn route(&self, from: [f32; 3], to: [f32; 3], blocked: &[Bounds]) -> Option<Vec<[f32; 3]>>;
    fn traversal_at(&self, at: [f32; 3]) -> Option<(usize, usize)>;
}

pub type Bounds = ([f32; 3], [f32; 3]);

#[derive(Clone)]
pub(crate) struct NavHandle(pub Arc<dyn ActorNav>);

impl std::fmt::Debug for NavHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NavHandle")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GoalTarget {
    Pos([f32; 3]),
    Entity(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActorGoal {
    target: GoalTarget,
    route: Vec<[f32; 3]>,
    routed_to: Option<[f32; 3]>,
    settled: bool,
    stepping: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    Custom,
    Scripted { started: bool, ended: bool },
    Pain,
    Death,
    Arrival,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Orient {
    #[default]
    Default,
    Current,
    Motion,
    Angle(f32),
    Point([f32; 3]),
    Enemy,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActorBrain {
    script: Arc<str>,
    thread: Option<u64>,
    hold: Option<Hold>,
    pub(crate) anim_mode: Arc<str>,
    orient: Orient,
    enemy_seen_ms: i64,
    suppressed: Option<i64>,
    last_seen: Option<(u64, [f32; 3], i64)>,
    traverse: Option<(u64, u64)>,
    arrival: Option<([f32; 3], f32)>,
    denied: u8,
    grenade_target: Option<[f32; 3]>,
    entity_target: Option<u64>,
    fire_lines: Vec<(i64, [f32; 2], [f32; 2])>,
    move_blocked: Option<i64>,
    grenade_flight: Option<([f32; 3], Option<ActorGoal>)>,
    held_grenade: Option<(u32, i64)>,
    sight_accuracy: Option<f32>,
    likely_enemy: Option<(i64, Option<[f32; 3]>)>,
    prone_nodes: Option<[Value; 3]>,
    prone_scale: (f32, f32),
    prone: bool,
    unattackable: Vec<(u64, i64)>,
    run_to: Option<[f32; 3]>,
    reacquire_path: Option<Vec<[f32; 3]>>,
    known: Vec<(u64, i64, [f32; 3])>,
    push_player: bool,
}

const RUN_SPEED: f32 = 170.0;
const TURN_RATE: f32 = 360.0;
const ARRIVAL_ERROR: f32 = 8.0;
const NEAR_NODE: f32 = 32.0;
const REROUTE: f32 = 64.0;
const DEFAULT_GOAL_RADIUS: f32 = 16.0;
const DEFAULT_GOAL_HEIGHT: f32 = 80.0;

fn number(value: Value) -> Option<f32> {
    match value {
        Value::Int(n) => Some(n as f32),
        Value::Float(n) => Some(n),
        _ => None,
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub(crate) fn spawn_path_nodes(
    world: &mut World,
    nodes: &[asset_core::PathNode],
    entities: &[Vec<(String, String)>],
    keys: &std::collections::BTreeMap<String, super::entities::KeyType>,
) -> Result<(), String> {
    let at = |origin: [f32; 3]| [origin[0].round() as i32, origin[1].round() as i32];
    let mut extra: std::collections::HashMap<[i32; 2], Vec<(f32, &Vec<(String, String)>)>> =
        std::collections::HashMap::new();
    for pairs in entities {
        let field = |name: &str| pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name));
        let Some((_, origin)) = field("origin") else {
            continue;
        };
        if field("classname").is_some_and(|(_, c)| c.starts_with("node_")) {
            let origin = super::entities::vector(origin);
            extra
                .entry(at(origin))
                .or_default()
                .push((origin[2], pairs));
        }
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    for node in nodes {
        let id = runtime.new_object()?;
        runtime.set_object_field(id, "origin", Value::Vector(node.origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.0, node.yaw, 0.0]));
        runtime.set_object_field(id, "type", Value::string(node.kind));
        runtime.set_object_field(id, "radius", Value::Float(node.radius));
        runtime.set_object_field(id, "spawnflags", Value::Int(node.spawnflags));
        for (key, value) in &node.keys {
            runtime.set_object_field(id, key, Value::string(value));
        }
        let placed = extra.get(&at(node.origin)).and_then(|found| {
            found
                .iter()
                .min_by(|a, b| {
                    (a.0 - node.origin[2])
                        .abs()
                        .total_cmp(&(b.0 - node.origin[2]).abs())
                })
                .map(|(_, pairs)| *pairs)
        });
        for (key, value) in placed.into_iter().flat_map(|p| p.iter()) {
            let key = key.to_ascii_lowercase();
            if !matches!(
                key.as_str(),
                "classname" | "origin" | "angles" | "angle" | "spawnflags"
            ) && runtime.object_field(id, &key) == Value::Undefined
            {
                use super::entities::KeyType;
                use super::natives::iw4::{atof, atoi};
                let value = match keys.get(&key) {
                    Some(KeyType::Int) => Value::Int(atoi(value)),
                    Some(KeyType::Float) => Value::Float(atof(value) as f32),
                    Some(KeyType::Vector) => Value::Vector(super::entities::vector(value)),
                    Some(KeyType::String) | None => Value::string(value),
                };
                runtime.set_object_field(id, &key, value);
            }
        }
        runtime.campaign.nodes.push(id);
        runtime.campaign.node_origins.push(node.origin);
        runtime
            .campaign
            .node_links
            .push(node.links.iter().map(|(k, _)| *k).collect());
    }
    Ok(())
}

fn matching_nodes(world: &mut World, args: &[Value]) -> Result<Vec<Value>, String> {
    let value = Value::string(&super::args::string(args, 0)?);
    let key = super::args::string(args, 1)?.to_ascii_lowercase();
    let mut runtime = world.resource_mut::<RoundScript>();
    let nodes = runtime.campaign.nodes.clone();
    Ok(nodes
        .into_iter()
        .filter(|id| runtime.object_field(*id, &key) == value)
        .map(Value::Object)
        .collect())
}

const NODE_QUERY_MAX: usize = 256;
const NODE_QUERY_HEIGHT: f32 = 512.0;

fn nodes_in_radius(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    use super::args::{float, string, vector};
    let at = vector(args, 0)?;
    let radius = float(args, 1)?;
    let min = float(args, 2)?;
    let height = if args.len() > 3 {
        float(args, 3)?
    } else {
        NODE_QUERY_HEIGHT
    };
    let kind = if args.len() > 4 {
        Some(string(args, 4)?)
    } else {
        None
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let candidates: Vec<(u64, [f32; 3])> = runtime
        .campaign
        .nodes
        .iter()
        .copied()
        .zip(runtime.campaign.node_origins.iter().copied())
        .collect();
    let mut found = Vec::new();
    for (node, origin) in candidates {
        let planar = (origin[0] - at[0]).hypot(origin[1] - at[1]);
        if planar > radius || (origin[2] - at[2]).abs() > height || distance(at, origin) < min {
            continue;
        }
        if let Some(kind) = &kind {
            let Value::String(node_kind) = runtime.object_field(node, "type") else {
                continue;
            };
            let matches = if kind.eq_ignore_ascii_case("Cover") {
                cover_script(&node_kind).is_some()
            } else {
                node_kind.eq_ignore_ascii_case(kind)
            };
            if !matches {
                continue;
            }
        }
        found.push(Value::Object(node));
        if found.len() == NODE_QUERY_MAX {
            break;
        }
    }
    drop(runtime);
    new_array(world, found)
}

fn set_goal(
    world: &mut World,
    receiver: &Value,
    target: GoalTarget,
    at: [f32; 3],
    node: Option<u64>,
) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "goalpos", Value::Vector(at));
    runtime.set_object_field(id, "node", node.map_or(Value::Undefined, Value::Object));
    if let Some(brain) = runtime.campaign.actor_brains.get_mut(&id) {
        brain.arrival = None;
        brain.run_to = None;
    }
    runtime.campaign.actor_goals.insert(
        id,
        ActorGoal {
            target,
            route: Vec::new(),
            routed_to: None,
            settled: false,
            stepping: false,
        },
    );
    Ok(Value::Undefined)
}

fn origin_of(world: &mut World, value: &Value) -> Result<(u64, [f32; 3]), String> {
    let Value::Object(id) = value else {
        return Err(format!("{} has no origin", super::args::kind(value)));
    };
    match world
        .resource_mut::<RoundScript>()
        .object_field(*id, "origin")
    {
        Value::Vector(origin) => Ok((*id, origin)),
        _ => Err("goal has no origin".into()),
    }
}

fn health(runtime: &mut RoundScript, id: u64) -> i32 {
    match runtime.object_field(id, "health") {
        Value::Int(health) => health,
        Value::Float(health) => health as i32,
        _ => 0,
    }
}

fn live_actors(runtime: &RoundScript) -> Vec<u64> {
    runtime
        .entities
        .iter()
        .filter(|(id, e)| e.kind == EntityKind::Actor && runtime.script_is_defined(id))
        .map(|(id, _)| *id)
        .collect()
}

fn yaw_to(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

pub(crate) fn advance(world: &mut World) {
    events::raise_tick_events(world);
    suppress(world);
    grenades::advance(world);
    campaign::mark_player_los(world);
    look(world);
    campaign::player_attackers(world);
    run_scripts(world);
    move_actors(world);
    awareness::run_to_arrivals(world);
}

const SIGHT_PERIOD: u64 = 4;
const ENEMY_MEMORY_MS: i64 = 10_000;
const DEFAULT_SIGHT_DIST: f32 = 8192.0;
const DEFAULT_FOV_COS: f32 = 0.01;
const DEFAULT_FOV_COS_BUSY: f32 = 0.574;
const SIGHT_TRACES: usize = 3;

fn hostile(a: &Value, b: &Value) -> bool {
    let (Value::String(a), Value::String(b)) = (a, b) else {
        return false;
    };
    matches!((&**a, &**b), ("allies", "axis") | ("axis", "allies"))
}

fn truthy(value: &Value) -> bool {
    matches!(value, Value::Int(n) if *n != 0) || matches!(value, Value::Float(n) if *n != 0.0)
}

fn eye(world: &mut World, id: u64) -> Option<[f32; 3]> {
    match super::players::entity_field(world, id, "origin") {
        Value::Vector([x, y, z]) => Some([x, y, z + EYE_HEIGHT]),
        _ => None,
    }
}

fn clear_sight(world: &mut World, from: [f32; 3], to: [f32; 3]) -> bool {
    let trace = super::presence::settled(world).trace_world(
        from,
        to,
        [0.0; 3],
        [0.0; 3],
        crate::bullet_collision::MASK_SHOT,
    );
    trace.fraction >= 1.0 && trace.startsolid == 0
}

pub(crate) fn combatants(world: &mut World) -> Vec<(u64, Value)> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let mut out: Vec<u64> = runtime.players.values().map(|slot| slot.object).collect();
    out.extend(
        live_actors(&runtime)
            .into_iter()
            .filter(|id| health(&mut runtime, *id) > 0),
    );
    let mut found = Vec::new();
    for id in out {
        if !truthy(&runtime.object_field(id, "ignoreme")) {
            found.push((id, runtime.object_field(id, "team")));
        }
    }
    let sentients: Vec<(u64, Value)> = runtime
        .campaign
        .sentients
        .iter()
        .filter(|(id, _)| runtime.entities.contains_key(id))
        .map(|(id, team)| (*id, team.clone()))
        .collect();
    for (id, team) in sentients {
        if !truthy(&runtime.object_field(id, "ignoreme")) {
            found.push((id, team));
        }
    }
    found
}

fn look(world: &mut World) {
    let tick = u64::from(world.resource::<crate::step::StepRequest>().tick.0);
    let now = super::players::now_ms(world);
    let ids: Vec<u64> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        live_actors(&runtime)
            .into_iter()
            .filter(|id| (id + tick) % SIGHT_PERIOD == 0 && health(&mut runtime, *id) > 0)
            .collect()
    };
    if ids.is_empty() {
        return;
    }
    let all = combatants(world);
    for id in ids {
        let (team, current, ignore_all, sight_sq, fov, angles, group, threat, favorite) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let sight_sq = number(runtime.object_field(id, "maxsightdistsqrd"))
                .unwrap_or(DEFAULT_SIGHT_DIST * DEFAULT_SIGHT_DIST);
            let current = match runtime.object_field(id, "enemy") {
                Value::Object(enemy) => Some(enemy),
                _ => None,
            };
            let fov = if current.is_some() {
                number(runtime.object_field(id, "fovcosinebusy")).unwrap_or(DEFAULT_FOV_COS_BUSY)
            } else {
                number(runtime.object_field(id, "fovcosine")).unwrap_or(DEFAULT_FOV_COS)
            };
            let angles = match runtime.object_field(id, "angles") {
                Value::Vector(angles) => angles,
                _ => [0.0; 3],
            };
            let favorite = match runtime.object_field(id, "favoriteenemy") {
                Value::Object(enemy) => Some(enemy),
                _ => None,
            };
            (
                runtime.object_field(id, "team"),
                current,
                truthy(&runtime.object_field(id, "ignoreall")),
                sight_sq,
                fov,
                angles,
                campaign::group_of(&mut runtime, id),
                runtime.campaign.threat.clone(),
                favorite,
            )
        };
        let Some(from) = eye(world, id) else {
            continue;
        };
        let forward = [angles[1].to_radians().cos(), angles[1].to_radians().sin()];
        let memory = brain(world, id).known;
        let known = |e: u64| {
            memory
                .iter()
                .find(|k| k.0 == e && now - k.1 < ENEMY_MEMORY_MS)
                .map(|k| k.1)
        };
        let mut candidates: Vec<(f32, u64, [f32; 3], i32)> = Vec::new();
        if !ignore_all {
            for (other, other_team) in &all {
                if *other == id || !hostile(&team, other_team) {
                    continue;
                }
                let target = campaign::group_of(&mut world.resource_mut::<RoundScript>(), *other);
                if threat.ignores(&group, &target) {
                    continue;
                }
                let Some(at) = eye(world, *other) else {
                    continue;
                };
                let d = [at[0] - from[0], at[1] - from[1], at[2] - from[2]];
                let dist_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let planar = d[0].hypot(d[1]).max(f32::EPSILON);
                let in_view = (d[0] * forward[0] + d[1] * forward[1]) / planar >= fov;
                let followed = current == Some(*other) || known(*other).is_some();
                if dist_sq <= sight_sq && (in_view || followed) {
                    let bias = threat.bias(&group, &target) as i32;
                    candidates.push((dist_sq.sqrt(), *other, at, bias));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let visible: Vec<(f32, u64, [f32; 3], i32)> = candidates
            .iter()
            .enumerate()
            .filter(|(k, c)| *k < SIGHT_TRACES || current == Some(c.1))
            .map(|(_, c)| *c)
            .filter(|c| clear_sight(world, from, c.2))
            .collect();
        let remembered = brain(world, id).enemy_seen_ms;
        let mut scored = Vec::new();
        for c in &visible {
            scored.push((
                threat_score(world, id, c, true, current, remembered, now),
                *c,
                true,
            ));
        }
        for c in &candidates {
            if visible.iter().any(|v| v.1 == c.1) {
                continue;
            }
            let seen = match known(c.1) {
                Some(at) if current == Some(c.1) => Some(at.max(remembered)),
                Some(at) => Some(at),
                None if current == Some(c.1) => Some(remembered),
                None => None,
            };
            if let Some(seen) = seen.filter(|seen| now - seen < ENEMY_MEMORY_MS) {
                scored.push((
                    threat_score(world, id, c, false, current, seen, now),
                    *c,
                    false,
                ));
            }
        }
        let seen = scored
            .into_iter()
            .max_by(|a, b| {
                (Some(a.1.1) == favorite)
                    .cmp(&(Some(b.1.1) == favorite))
                    .then(a.0.cmp(&b.0))
            })
            .map(|(_, c, visible)| (c.1, c.2, visible));
        let mut brain = brain(world, id);
        brain.known.retain(|&(e, seen, _)| {
            now - seen < ENEMY_MEMORY_MS && !visible.iter().any(|c| c.1 == e)
        });
        brain.known.extend(
            visible
                .iter()
                .map(|c| (c.1, now, [c.2[0], c.2[1], c.2[2] - EYE_HEIGHT])),
        );
        if let Some(target) = brain.entity_target {
            if world.resource::<RoundScript>().live(&target) {
                store(world, id, brain);
                if current != Some(target) {
                    world.resource_mut::<RoundScript>().set_object_field(
                        id,
                        "enemy",
                        Value::Object(target),
                    );
                    crate::script::runtime::raise(world, Value::Object(id), "enemy", Vec::new());
                }
                continue;
            }
            brain.entity_target = None;
        }
        let still_live = current.is_some_and(|enemy| all.iter().any(|(other, _)| *other == enemy));
        let enemy = match seen {
            Some((other, at, true)) => {
                brain.enemy_seen_ms = now;
                brain.last_seen = Some((other, [at[0], at[1], at[2] - EYE_HEIGHT], now));
                world.resource_mut::<RoundScript>().set_object_field(
                    id,
                    "lastenemysightpos",
                    Value::Vector(at),
                );
                Some(other)
            }
            Some((other, _, false)) => Some(other),
            None if !ignore_all && still_live && now - brain.enemy_seen_ms < ENEMY_MEMORY_MS => {
                current
            }
            None => None,
        };
        store(world, id, brain);
        if enemy != current {
            world.resource_mut::<RoundScript>().set_object_field(
                id,
                "enemy",
                enemy.map_or(Value::Undefined, Value::Object),
            );
            crate::script::runtime::raise(world, Value::Object(id), "enemy", Vec::new());
        }
    }
}

fn threat_score(
    world: &mut World,
    id: u64,
    candidate: &(f32, u64, [f32; 3], i32),
    visible: bool,
    current: Option<u64>,
    remembered: i64,
    now: i64,
) -> i32 {
    let (dist, other, _, bias) = *candidate;
    let recent = now - remembered < ENEMY_MEMORY_MS;
    let duel = if visible || (current == Some(other) && recent) {
        let gap = threat_accuracy(world, other, id, dist) - threat_accuracy(world, id, other, dist);
        ((gap * 100.0).round() as i32).clamp(-500, 1000)
    } else {
        0
    };
    let wounded = world
        .resource::<RoundScript>()
        .player_client(other)
        .is_some()
        && match (
            super::players::entity_field(world, other, "health"),
            super::players::entity_field(world, other, "maxhealth"),
        ) {
            (Value::Int(health), Value::Int(max)) => (health as f32) < max as f32 * 0.8,
            _ => false,
        };
    let mut runtime = world.resource_mut::<RoundScript>();
    let own = match runtime.object_field(other, "threatbias") {
        Value::Int(n) => n,
        Value::Float(n) => n as i32,
        _ => 0,
    };
    let sight = if visible {
        1000
    } else if recent {
        500
    } else {
        0
    };
    let engaged = if current != Some(other) {
        0
    } else if visible || recent {
        if wounded { 1000 } else { 500 }
    } else if recent {
        200
    } else {
        100
    };
    let near = if dist < 2500.0 {
        ((2500.0 - dist).powi(2) * 0.0008).round() as i32
    } else {
        0
    };
    let given_up = runtime.campaign.actor_brains.get(&id).is_some_and(|b| {
        b.unattackable
            .iter()
            .any(|&(e, until)| e == other && now < until)
    });
    let attackers = live_actors(&runtime)
        .into_iter()
        .filter(|a| *a != id && runtime.object_field(*a, "enemy") == Value::Object(other))
        .count() as i32;
    own + bias + sight + engaged + duel + near + (attackers * -150).max(-1000)
        - if given_up { UNATTACKABLE_PENALTY } else { 0 }
}

const UNATTACKABLE_PENALTY: i32 = 10000;

const PLAYER_NEAR_ACCURACY: f32 = 0.5;
const PLAYER_NEAR_RANGE: f32 = 800.0;
const PLAYER_FAR_ACCURACY: f32 = 0.1;
const PLAYER_FAR_RANGE: f32 = 2000.0;

fn threat_accuracy(world: &mut World, shooter: u64, target: u64, dist: f32) -> f32 {
    let (shooter_client, vs_player) = {
        let runtime = world.resource::<RoundScript>();
        (
            runtime.player_client(shooter).is_some(),
            runtime.player_client(target).is_some(),
        )
    };
    if shooter_client {
        let t =
            ((dist - PLAYER_NEAR_RANGE) / (PLAYER_FAR_RANGE - PLAYER_NEAR_RANGE)).clamp(0.0, 1.0);
        return (PLAYER_NEAR_ACCURACY + (PLAYER_FAR_ACCURACY - PLAYER_NEAR_ACCURACY) * t) * 5.0;
    }
    let Value::String(name) = world
        .resource_mut::<RoundScript>()
        .object_field(shooter, "weapon")
    else {
        return 0.0;
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    let Ok(weapon) = crate::script_player::weapon_named(&frame, &name) else {
        return 0.0;
    };
    frame.weapon_ai_accuracy(weapon).map_or(1.0, |graphs| {
        let graph = if vs_player {
            graphs.vs_player
        } else {
            graphs.vs_ai
        };
        if graph.is_empty() {
            1.0
        } else {
            graph.accuracy(dist)
        }
    })
}

const SUPPRESS_RADIUS: f32 = 128.0;
const SUPPRESS_STEP: f32 = 0.15;
const SUPPRESS_DECAY: f32 = 0.01;
const SUPPRESSION_DURATION_MS: i32 = 5_000;
const SUPPRESS_NODE_DIST: f32 = 64.0;
const FRIENDLY_BLOCK_MS: i64 = 2_000;
const FRIENDLY_SUPPRESS_DIST: f32 = 128.0;
const FIRE_LINES: usize = 4;
const FIRE_LINE_OVERSHOOT: f32 = 30.0;

fn closest_on_segment(point: [f32; 3], from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let p = [point[0] - from[0], point[1] - from[1], point[2] - from[2]];
    let len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let t = if len_sq > 0.0 {
        ((p[0] * d[0] + p[1] * d[1] + p[2] * d[2]) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t]
}

fn segment_distance(point: [f32; 3], from: [f32; 3], to: [f32; 3]) -> f32 {
    distance(point, closest_on_segment(point, from, to))
}

const PLAYER_BLOCK_HALF_WIDTH: f32 = 15.0;
const PLAYER_BLOCK_HEIGHT: f32 = 70.0;

fn player_in_the_way(
    team: &Value,
    origin: [f32; 3],
    next: [f32; 3],
    left: f32,
    players: &[(Value, [f32; 3])],
) -> bool {
    players.iter().any(|(player_team, at)| {
        !hostile(team, player_team) && blocks_step(origin, next, left, *at)
    })
}

fn blocks_step(origin: [f32; 3], next: [f32; 3], left: f32, at: [f32; 3]) -> bool {
    let d = glam::Vec2::new(next[0] - origin[0], next[1] - origin[1]);
    let step = d.normalize_or_zero() * left.min(d.length());
    let to = [origin[0] + step.x, origin[1] + step.y, origin[2]];
    let reach = PLAYER_BLOCK_HALF_WIDTH + ACTOR_RADIUS;
    (at[0] - to[0]).abs() < reach
        && (at[1] - to[1]).abs() < reach
        && at[2] < to[2] + ACTOR_HEIGHT
        && to[2] < at[2] + PLAYER_BLOCK_HEIGHT
}

const DROP_PROBE: f32 = 72.0;
const MAX_DROP: f32 = 32.0;

fn may_move(world: &mut World, from: [f32; 3], to: [f32; 3], check_drop: bool) -> bool {
    let settled = super::presence::settled(world);
    let drop_at = |at: [f32; 3]| {
        let ground = settled.trace_world(
            [at[0], at[1], at[2] + DROP_PROBE],
            [at[0], at[1], at[2] - DROP_PROBE],
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_PLAYER_SOLID,
        );
        (ground.startsolid != 0, at[2] - ground.endpos[2])
    };
    let (solid, drop) = drop_at(to);
    if solid || (check_drop && drop > MAX_DROP) {
        return false;
    }
    if check_drop {
        let probes = (distance(from, to) / ACTOR_RADIUS).floor() as usize;
        for k in 1..probes {
            let t = k as f32 / probes as f32;
            let at = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
            if drop_at(at).1 > MAX_DROP {
                return false;
            }
        }
    }
    let lift = |[x, y, z]: [f32; 3]| [x, y, z + STEP_HEIGHT];
    let trace = settled.trace_world(
        lift(from),
        lift(to),
        [-ACTOR_RADIUS, -ACTOR_RADIUS, 0.0],
        [ACTOR_RADIUS, ACTOR_RADIUS, ACTOR_HEIGHT - STEP_HEIGHT],
        crate::bullet_collision::MASK_PLAYER_SOLID,
    );
    trace.fraction >= 1.0 && trace.startsolid == 0
}

fn crosses_fire_line(lines: &[(i64, [f32; 2], [f32; 2])], from: [f32; 3], to: [f32; 3]) -> bool {
    let start = glam::Vec2::new(from[0], from[1]);
    let dir = glam::Vec2::new(to[0], to[1]) - start;
    let end = start + dir + dir.normalize_or_zero() * FIRE_LINE_OVERSHOOT;
    lines.iter().any(|(_, a, b)| {
        let (a, b) = (glam::Vec2::from(*a), glam::Vec2::from(*b));
        let side = |p: glam::Vec2| (b - a).perp_dot(p - a);
        side(start) * side(end) < 0.0
    })
}

fn shooter(world: &mut World, number: i32) -> Option<u64> {
    match u32::try_from(number).map(|client| super::players::player_object(world, client)) {
        Ok(Value::Object(id)) => Some(id),
        _ => world
            .resource::<RoundScript>()
            .entities
            .iter()
            .find(|(_, entity)| entity.number == number)
            .map(|(id, _)| *id),
    }
}

fn suppress(world: &mut World) {
    let tick = world.resource::<crate::step::StepRequest>().tick.0;
    let shots: Vec<(i32, [f32; 3], [f32; 3])> = {
        let frame = crate::frame::FrameWorld::from_world(world);
        let mut shots: Vec<(i32, [f32; 3], [f32; 3])> = frame
            .entity_events()
            .iter()
            .filter(|record| {
                record.tick.0 + 1 == tick
                    && matches!(
                        entity_iw4::entity_event_action(record.event),
                        Ok(entity_iw4::EntityEventAction::BulletHit)
                    )
            })
            .map(|record| {
                (
                    record.payload.number,
                    record.payload.origin2,
                    record.payload.origin,
                )
            })
            .collect();
        shots.extend(
            frame
                .last_pellet_fx()
                .iter()
                .map(|record| (record.attacker, record.start, record.end)),
        );
        shots
    };
    let shots: Vec<(Option<u64>, Value, [f32; 3], [f32; 3])> = shots
        .into_iter()
        .map(|(number, from, to)| {
            let shooter = shooter(world, number);
            let team = shooter.map_or(Value::Undefined, |id| {
                world.resource_mut::<RoundScript>().object_field(id, "team")
            });
            (shooter, team, from, to)
        })
        .collect();
    let now = super::players::now_ms(world);
    let mut notifies = Vec::new();
    for id in live_actors(&world.resource::<RoundScript>()) {
        let mut runtime = world.resource_mut::<RoundScript>();
        let ignore = truthy(&runtime.object_field(id, "ignoresuppression"));
        let meter = number(runtime.object_field(id, "suppressionmeter")).unwrap_or(0.0);
        let mut next = if ignore { 0.0 } else { meter - SUPPRESS_DECAY };
        let mut suppressor = None;
        let mut fire_lines = Vec::new();
        let mut drop_route = false;
        if let Value::Vector(origin) = runtime.object_field(id, "origin") {
            let team = runtime.object_field(id, "team");
            let eye = [origin[0], origin[1], origin[2] + EYE_HEIGHT];
            let front = cover_front(&mut runtime, id, origin);
            for (shooter, _, from, to) in shots
                .iter()
                .filter(|(_, shooter_team, _, _)| hostile(&team, shooter_team))
            {
                if segment_distance(eye, *from, *to) >= SUPPRESS_RADIUS {
                    continue;
                }
                next += SUPPRESS_STEP;
                let attacker = shooter.map_or(Value::Undefined, Value::Object);
                let dist = segment_distance(origin, *from, *to);
                notifies.push((
                    id,
                    "bulletwhizby",
                    vec![attacker.clone(), Value::Float(dist)],
                ));
                if runtime
                    .campaign
                    .ai_listeners
                    .get(&id)
                    .is_some_and(|kinds| kinds.contains("bulletwhizby"))
                {
                    let args = vec![
                        Value::string("bulletwhizby"),
                        attacker.clone(),
                        Value::Vector(*to),
                    ];
                    notifies.push((id, "ai_event", args));
                }
                let toward =
                    glam::Vec2::new(from[0] - origin[0], from[1] - origin[1]).normalize_or_zero();
                if front.is_some_and(|front| front.dot(toward) >= 0.5) {
                    suppressor = Some(attacker);
                }
            }
            let next = runtime
                .campaign
                .actor_goals
                .get(&id)
                .and_then(|goal| goal.route.first().copied());
            for (shooter, _, from, to) in shots.iter().filter(|(shooter, shooter_team, _, _)| {
                *shooter != Some(id)
                    && matches!((&team, shooter_team), (Value::String(a), Value::String(b)) if a == b)
            }) {
                if segment_distance(eye, *from, *to) >= SUPPRESS_RADIUS {
                    continue;
                }
                let ai = shooter.filter(|shooter| {
                    runtime
                        .entities
                        .get(shooter)
                        .is_some_and(|entity| entity.kind == EntityKind::Actor)
                });
                match ai {
                    Some(ai) => {
                        let near = matches!(runtime.object_field(ai, "origin"),
                            Value::Vector(at) if distance(origin, at) <= FRIENDLY_SUPPRESS_DIST);
                        if ignore || next.is_some() || !near {
                            continue;
                        }
                    }
                    None => {
                        let attacker = shooter.map_or(Value::Undefined, Value::Object);
                        let dist = segment_distance(origin, *from, *to);
                        notifies.push((id, "bulletwhizby", vec![attacker, Value::Float(dist)]));
                    }
                }
                fire_lines.push((now, [from[0], from[1]], [to[0], to[1]]));
                if let Some(next) = next {
                    let closest = closest_on_segment(origin, *from, *to);
                    let ahead = (next[0] - origin[0]) * (closest[0] - origin[0])
                        + (next[1] - origin[1]) * (closest[1] - origin[1]);
                    drop_route |= ahead > 0.0;
                }
            }
        }
        let next = next.clamp(0.0, 1.0);
        if next != meter {
            runtime.set_object_field(id, "suppressionmeter", Value::Float(next));
        }
        let duration = number(runtime.object_field(id, "suppressionduration"))
            .map_or(SUPPRESSION_DURATION_MS as i64, |ms| ms as i64);
        if drop_route && let Some(goal) = runtime.campaign.actor_goals.get_mut(&id) {
            goal.route.clear();
            goal.routed_to = None;
        }
        drop(runtime);
        let mut brain = brain(world, id);
        let before = brain.clone();
        let was = brain.suppressed;
        brain
            .fire_lines
            .retain(|(at, _, _)| now - at < FRIENDLY_BLOCK_MS);
        for line in fire_lines {
            if brain.fire_lines.len() == FIRE_LINES {
                brain.fire_lines.remove(0);
            }
            brain.fire_lines.push(line);
        }
        if drop_route {
            brain.move_blocked = Some(now);
        } else if brain
            .move_blocked
            .is_some_and(|at| now - at >= FRIENDLY_BLOCK_MS)
        {
            brain.move_blocked = None;
        }
        match suppressor {
            Some(attacker) if !ignore => {
                if was.is_none() {
                    notifies.push((id, "suppression", vec![attacker]));
                }
                brain.suppressed = Some(now);
            }
            _ => {
                if was.is_some_and(|last| now - last >= duration) {
                    brain.suppressed = None;
                    notifies.push((id, "suppression_end", Vec::new()));
                }
            }
        }
        if brain != before {
            store(world, id, brain);
        }
    }
    for (id, name, args) in notifies {
        crate::script::runtime::raise(world, Value::Object(id), name, args);
    }
}

fn cover_front(runtime: &mut RoundScript, id: u64, origin: [f32; 3]) -> Option<glam::Vec2> {
    let Value::Object(node) = runtime.object_field(id, "node") else {
        return None;
    };
    let Value::String(kind) = runtime.object_field(node, "type") else {
        return None;
    };
    if !matches!(&*kind, "Cover Left" | "Cover Right") {
        return None;
    }
    let (Value::Vector(at), Value::Vector(angles)) = (
        runtime.object_field(node, "origin"),
        runtime.object_field(node, "angles"),
    ) else {
        return None;
    };
    if distance(origin, at) > SUPPRESS_NODE_DIST {
        return None;
    }
    let yaw = angles[1].to_radians();
    Some(glam::Vec2::new(yaw.cos(), yaw.sin()))
}

fn near_claim_node(world: &mut World, receiver: &Value, facing: bool) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let Value::Object(node) = runtime.object_field(id, "node") else {
        return Ok(Value::Int(0));
    };
    let (
        Value::Vector(origin),
        Value::Vector(at),
        Value::Vector(angles),
        Value::Vector(node_angles),
    ) = (
        runtime.object_field(id, "origin"),
        runtime.object_field(node, "origin"),
        runtime.object_field(id, "angles"),
        runtime.object_field(node, "angles"),
    )
    else {
        return Ok(Value::Int(0));
    };
    let aligned = !facing || math_iw4::angle_subtract(angles[1], node_angles[1]).abs() < 45.0;
    Ok(Value::Int((near(origin, at) && aligned).into()))
}

use crate::script_player::campaign::{CROUCH, FIRE, MELEE, PRONE, STAND};

fn has_grenades(world: &mut World, id: u64) -> bool {
    number(
        world
            .resource_mut::<RoundScript>()
            .object_field(id, "grenadeammo"),
    )
    .is_some_and(|ammo| ammo > 0.0)
}

const GRENADE_SAFE_RADIUS: f32 = 256.0;
const GRENADE_GRAVITY: f32 = 800.0;

fn grenade_throw(
    world: &mut World,
    id: u64,
    args: &[Value],
    target: [f32; 3],
) -> Result<Value, String> {
    let offset = match args.first() {
        Some(Value::Vector(offset)) => *offset,
        _ => [0.0, 0.0, 64.0],
    };
    let (origin, yaw) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = match runtime.object_field(id, "origin") {
            Value::Vector(at) => at,
            _ => return Ok(Value::Undefined),
        };
        let yaw = match runtime.object_field(id, "angles") {
            Value::Vector(angles) => angles[1].to_radians(),
            _ => 0.0,
        };
        (origin, yaw)
    };
    let from = [
        origin[0] + offset[0] * yaw.cos() - offset[1] * yaw.sin(),
        origin[1] + offset[0] * yaw.sin() + offset[1] * yaw.cos(),
        origin[2] + offset[2],
    ];
    let planar = (target[0] - from[0]).hypot(target[1] - from[1]);
    let t = (planar / 600.0).clamp(0.6, 2.0);
    let velocity = [
        (target[0] - from[0]) / t,
        (target[1] - from[1]) / t,
        (target[2] - from[2] + 0.5 * GRENADE_GRAVITY * t * t) / t,
    ];
    let half = t * 0.5;
    let apex = [
        from[0] + velocity[0] * half,
        from[1] + velocity[1] * half,
        from[2] + velocity[2] * half - 0.5 * GRENADE_GRAVITY * half * half,
    ];
    if !clear_sight(world, from, apex)
        || !clear_sight(world, apex, [target[0], target[1], target[2] + 16.0])
    {
        return Ok(Value::Undefined);
    }
    let mut brain = brain(world, id);
    brain.grenade_target = Some(target);
    store(world, id, brain);
    Ok(Value::Vector(velocity))
}

fn block_paths(world: &mut World, receiver: &Value, block: bool) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    let (lo, hi) = super::triggers::entity_bounds(world, id);
    let mut runtime = world.resource_mut::<RoundScript>();
    let at = match runtime.object_field(id, "origin") {
        Value::Vector(at) => at,
        _ => [0.0; 3],
    };
    let bounds = (
        std::array::from_fn(|k| at[k] + lo[k]),
        std::array::from_fn(|k| at[k] + hi[k]),
    );
    if block {
        runtime.campaign.blocked_paths.insert(id, bounds);
    } else {
        runtime.campaign.blocked_paths.remove(&id);
    }
    for goal in runtime.campaign.actor_goals.values_mut() {
        goal.routed_to = None;
    }
    Ok(Value::Undefined)
}

fn stance_bit(name: &str) -> u8 {
    match name {
        "stand" => STAND,
        "crouch" => CROUCH,
        "prone" => PRONE,
        _ => 0,
    }
}

fn stances_denied(world: &mut World, receiver: &Value) -> Result<u8, String> {
    let id = entity_id(world, receiver)?;
    let client = world.resource::<RoundScript>().player_client(id);
    Ok(match client {
        Some(client) => crate::frame::FrameWorld::from_world(world)
            .client_meta(crate::world::ClientId(client))
            .map_or(0, |meta| meta.controls.denied),
        None => brain(world, id).denied,
    })
}

fn set_stances_denied(world: &mut World, receiver: &Value, denied: u8) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    let client = world.resource::<RoundScript>().player_client(id);
    match client {
        Some(client) => {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            if frame.client_meta(crate::world::ClientId(client)).is_some() {
                frame
                    .client_meta_mut(crate::world::ClientId(client))
                    .controls
                    .denied = denied;
            }
        }
        None => {
            let mut brain = brain(world, id);
            brain.denied = denied;
            store(world, id, brain);
        }
    }
    Ok(Value::Undefined)
}

fn allow_stance(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    bit: u8,
) -> Result<Value, String> {
    let allow = truthy(super::args::arg(args, 0)?);
    let denied = stances_denied(world, receiver)?;
    let denied = if allow { denied & !bit } else { denied | bit };
    set_stances_denied(world, receiver, denied)
}

fn shoot(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let (name, enemy) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let name = match runtime.object_field(id, "weapon") {
            Value::String(name) => name.to_string(),
            _ => return Ok(Value::Undefined),
        };
        let enemy = match runtime.object_field(id, "enemy") {
            Value::Object(enemy) => Some(enemy),
            _ => None,
        };
        (name, enemy)
    };
    if name.is_empty() || name == "none" {
        return Ok(Value::Undefined);
    }
    let weapon =
        crate::script_player::weapon_named(&crate::frame::FrameWorld::from_world(world), &name)?;
    let scale = match args.first() {
        Some(value) => number(value.clone()).unwrap_or(1.0),
        None => 1.0,
    };
    let from = super::turrets::muzzle(world, id);
    let forward = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "angles")
    {
        Value::Vector(angles) => math_iw4::angle_vectors(angles).0,
        _ => [1.0, 0.0, 0.0],
    };
    let point = match (args.get(1), enemy) {
        (Some(Value::Vector(at)), _) => *at,
        (_, Some(enemy)) => match aim_at_enemy(world, id, enemy, weapon, scale, from, forward) {
            Some(point) => point,
            None => return Ok(Value::Undefined),
        },
        _ => std::array::from_fn(|k| from[k] + forward[k] * 1000.0),
    };
    let dir = glam::Vec3::from_array(std::array::from_fn(|k| point[k] - from[k]))
        .try_normalize()
        .map_or(forward, |d| d.to_array());
    super::turrets::fire_bullet(world, id, from, dir, weapon, None);
    let team = world.resource_mut::<RoundScript>().object_field(id, "team");
    if let Value::Vector(at) = world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        events::broadcast(
            world,
            "gunshot_teammate",
            Some(id),
            at,
            events::Hearers::Team(&team),
        );
    }
    Ok(Value::Undefined)
}

const MELEE_RANGE: f32 = 64.0;

fn melee(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    use crate::bullet_collision::{ColliderId, MASK_SHOT, TraceOutcome};
    let id = actor(world, receiver)?;
    let Some(from) = eye(world, id) else {
        return Ok(Value::Undefined);
    };
    let (weapon, enemy, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        (
            runtime.object_field(id, "weapon"),
            runtime.object_field(id, "enemy"),
            runtime.object_field(id, "angles"),
        )
    };
    let dir = match (args.first(), enemy) {
        (Some(Value::Vector(dir)), _) => Some(*dir),
        (_, Value::Object(enemy)) => {
            eye(world, enemy).map(|at| std::array::from_fn(|k| at[k] - from[k]))
        }
        _ => None,
    };
    let dir = dir
        .and_then(|d| glam::Vec3::from_array(d).try_normalize())
        .map(|d| d.to_array())
        .unwrap_or_else(|| match angles {
            Value::Vector(angles) => math_iw4::angle_vectors(angles).0,
            _ => [1.0, 0.0, 0.0],
        });
    let end = std::array::from_fn(|k| from[k] + dir[k] * MELEE_RANGE);
    let ignore = super::turrets::ignore_self(world, id);
    let TraceOutcome::Hit { collider, .. } =
        super::natives::engine::entity_trace(world, from, end, MASK_SHOT, ignore)
    else {
        return Ok(Value::Undefined);
    };
    let hit = super::natives::engine::collider_entity(world, collider);
    let frame = crate::frame::FrameWorld::from_world(world);
    let weapon = match weapon {
        Value::String(name) if !name.is_empty() && &*name != "none" => {
            crate::script_player::weapon_named(&frame, &name).ok()
        }
        _ => None,
    };
    let amount = weapon
        .and_then(|w| frame.combat_facts_for(w))
        .map_or(0, |facts| facts.melee_damage);
    let runtime = world.resource::<RoundScript>();
    let target = match (collider, &hit) {
        (ColliderId::Player { client, .. }, _) => Some(crate::script::HitTarget::Player(client)),
        (_, Value::Object(object)) => runtime
            .entities
            .get(object)
            .filter(|e| e.can_damage)
            .and_then(|e| e.presence)
            .map(crate::script::HitTarget::Entity),
        _ => None,
    };
    if let Some(target) = target
        && amount > 0
    {
        let inflictor = runtime.entities.get(&id).and_then(|e| e.presence);
        world
            .resource_mut::<RoundScript>()
            .hits
            .push(crate::script::ScriptHit {
                piece: None,
                target,
                amount,
                origin: from,
                attacker: None,
                inflictor,
                means: "MOD_MELEE",
                weapon: weapon.unwrap_or(0),
                flags: 0,
                hitloc: 0,
            });
    }
    Ok(hit)
}

fn aim_at_enemy(
    world: &mut World,
    id: u64,
    enemy: u64,
    weapon: u32,
    scale: f32,
    from: [f32; 3],
    forward: [f32; 3],
) -> Option<[f32; 3]> {
    let client = world.resource::<RoundScript>().player_client(enemy);
    let at = eye(world, enemy)?;
    let accuracy = final_accuracy(world, id, enemy, client, weapon, scale)
        * brain(world, id).sight_accuracy.unwrap_or(1.0);
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "finalaccuracy",
        Value::Float(accuracy),
    );
    let rand = |world: &mut World| super::natives::math::random_unit(world);
    let crand = |world: &mut World| super::natives::math::random_unit(world) * 2.0 - 1.0;
    let to = glam::Vec3::from_array(at) - glam::Vec3::from_array(from);
    let dir = to.try_normalize().unwrap_or(glam::Vec3::X);
    let side = glam::Vec3::new(dir.y, -dir.x, 0.0).normalize_or_zero();
    let miss = 1.0 - accuracy;
    if rand(world) < accuracy {
        let (lateral, vertical) = match client {
            Some(_) => {
                let feet = super::players::entity_field(world, enemy, "origin");
                let span = match feet {
                    Value::Vector(feet) => (feet[2] - (at[2] + 10.0)).max(-44.0),
                    _ => -44.0,
                };
                (crand(world) * 8.0, rand(world) * span + 10.0)
            }
            None => (
                crand(world) * miss * ACTOR_RADIUS,
                rand(world) * miss * -44.0,
            ),
        };
        let point = glam::Vec3::from_array(at) + side * lateral + glam::Vec3::Z * vertical;
        return Some(point.to_array());
    }
    let aim = glam::Vec3::from_array(forward);
    let sign = if aim.dot(dir) >= 0.984_808 {
        if rand(world) > 0.5 { 1.0 } else { -1.0 }
    } else if aim.dot(side) > 0.0 {
        1.0
    } else {
        -1.0
    };
    let (lateral, vertical) = match client {
        Some(_) => (8.0 + rand(world) * 8.0, 10.0 - rand(world) * 44.0),
        None => (
            rand(world) * 10.0 * miss + 36.735,
            crand(world) * -22.0 * miss,
        ),
    };
    let point = glam::Vec3::from_array(at) - dir * 16.0
        + side * (sign * lateral)
        + glam::Vec3::Z * vertical;
    Some(point.to_array())
}

fn final_accuracy(
    world: &mut World,
    id: u64,
    enemy: u64,
    client: Option<u32>,
    weapon: u32,
    scale: f32,
) -> f32 {
    let (accuracy, attacker_accuracy, from, to) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        (
            number(runtime.object_field(id, "accuracy")).unwrap_or(1.0),
            number(runtime.object_field(enemy, "attackeraccuracy")).unwrap_or(1.0),
            runtime.object_field(id, "origin"),
            runtime.object_field(enemy, "origin"),
        )
    };
    let to = match client {
        Some(_) => super::players::entity_field(world, enemy, "origin"),
        None => to,
    };
    let (Value::Vector(from), Value::Vector(to)) = (from, to) else {
        return 0.0;
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    let graph = frame.weapon_ai_accuracy(weapon).map_or(1.0, |graphs| {
        let graph = if client.is_some() {
            graphs.vs_player
        } else {
            graphs.vs_ai
        };
        if graph.is_empty() {
            1.0
        } else {
            graph.accuracy(distance(from, to))
        }
    });
    let (stance, motion) = match client {
        Some(client) => {
            let client = crate::ClientId(client);
            let stance = match crate::script_player::stance(&frame, client) {
                "prone" => 0.5,
                "crouch" => 0.75,
                _ => 1.0,
            };
            let velocity = frame.player(client).map_or([0.0; 3], |ps| ps.velocity);
            let dir = glam::Vec3::from_array(to) - glam::Vec3::from_array(from);
            let dir = dir.try_normalize().unwrap_or(glam::Vec3::X);
            let side = glam::Vec3::new(dir.y, -dir.x, 0.0);
            let speed = side.dot(glam::Vec3::from_array(velocity)).abs().min(250.0);
            (stance, (1.0 - speed / 250.0).max(0.3))
        }
        None => (1.0, 1.0),
    };
    (accuracy * attacker_accuracy * scale * graph * stance * motion).clamp(0.0, 1.0)
}

fn desired_script(world: &mut World, id: u64) -> String {
    if let Some(script) = super::turrets::campaign::gunner_script(world, id) {
        return script;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    if is_dog(&mut runtime, id) {
        if dog_bite_target(&mut runtime, id).is_some_and(|(at, reach)| {
            matches!(runtime.object_field(id, "origin"), Value::Vector(o) if planar(o, at) <= reach)
        }) {
            return "combat".into();
        }
        let moving = runtime
            .campaign
            .actor_goals
            .get(&id)
            .is_some_and(|goal| !goal.route.is_empty());
        return if moving { "move" } else { "stop" }.into();
    }
    if runtime
        .campaign
        .actor_goals
        .get(&id)
        .is_some_and(|goal| !goal.route.is_empty())
    {
        return "move".into();
    }
    let cover = match (
        runtime.object_field(id, "node"),
        runtime.object_field(id, "origin"),
    ) {
        (Value::Object(node), Value::Vector(origin)) => match (
            runtime.object_field(node, "origin"),
            runtime.object_field(node, "type"),
        ) {
            (Value::Vector(at), Value::String(kind)) if near(origin, at) => cover_script(&kind),
            _ => None,
        },
        _ => None,
    };
    let enemy =
        matches!(runtime.object_field(id, "enemy"), Value::Object(enemy) if runtime.live(&enemy));
    drop(runtime);
    if let Some(script) = cover
        && has_entry(world, &format!("animscripts/{script}::main"))
    {
        return script.into();
    }
    if enemy {
        return "combat".into();
    }
    "stop".into()
}

fn is_dog(runtime: &mut RoundScript, id: u64) -> bool {
    matches!(runtime.object_field(id, "type"), Value::String(kind) if &*kind == "dog")
}

fn script_entry(world: &mut World, id: u64, script: &str, function: &str) -> String {
    if is_dog(&mut world.resource_mut::<RoundScript>(), id) {
        format!("animscripts/dog/dog_{script}::{function}")
    } else {
        format!("animscripts/{script}::{function}")
    }
}

fn dog_bite_target(runtime: &mut RoundScript, id: u64) -> Option<([f32; 3], f32)> {
    let Value::Object(enemy) = runtime.object_field(id, "enemy") else {
        return None;
    };
    if !runtime.live(&enemy) {
        return None;
    }
    let dist = number(runtime.object_field(id, "meleeattackdist")).filter(|d| *d > 0.0)?;
    match runtime.object_field(enemy, "origin") {
        Value::Vector(at) => Some((at, dist + DOG_REACH_SLACK)),
        _ => None,
    }
}

fn dog_chase(
    runtime: &mut RoundScript,
    id: u64,
    origin: [f32; 3],
    goal_pos: [f32; 3],
    goal_radius: f32,
) -> Option<([f32; 3], f32)> {
    if !is_dog(runtime, id) {
        return None;
    }
    let (at, reach) = dog_bite_target(runtime, id)?;
    let fight = number(runtime.object_field(id, "pathenemyfightdist")).unwrap_or(0.0);
    (planar(origin, at) <= fight || planar(goal_pos, at) <= goal_radius).then_some((at, reach))
}

fn planar(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn cover_script(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Cover Stand" => "cover_stand",
        "Cover Crouch" | "Cover Crouch Window" => "cover_crouch",
        "Cover Prone" => "cover_prone",
        "Cover Left" | "Cover Wide Left" => "cover_left",
        "Cover Right" | "Cover Wide Right" => "cover_right",
        _ => return None,
    })
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() <= NEAR_NODE
        && (a[2] - b[2]).abs() <= DEFAULT_GOAL_HEIGHT
}

enum Start<'a> {
    Entry(&'a str),
    Function(u32),
}

fn switch(world: &mut World, id: u64, brain: &mut ActorBrain, script: &str, start: Start) {
    let old = std::mem::replace(&mut brain.script, script.into());
    if let Some(thread) = brain.thread.take() {
        crate::script::runtime::campaign::stop_thread(world, thread);
    }
    let receiver = Value::Object(id);
    if &*old == "scripted"
        && let Some(anim) = world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .and_then(|entity| entity.anim.as_mut())
    {
        anim.scripted = None;
    }
    if !old.is_empty() {
        crate::script::runtime::raise(world, receiver.clone(), "killanimscript", Vec::new());
        let end = script_entry(world, id, &old, "end_script");
        if has_entry(world, &end) {
            let now = super::players::now_ms(world);
            let _ = crate::script::runtime::run_now(world, &end, receiver.clone(), Vec::new(), now);
        }
    }
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let prev = if old.is_empty() { "none" } else { &old };
        runtime.set_object_field(id, "prevscript", Value::string(prev));
        runtime.set_object_field(id, "script", Value::string(script));
    }
    brain.thread = match start {
        Start::Entry(entry) if has_entry(world, entry) => {
            crate::script::runtime::campaign::start_thread(world, entry, receiver, Vec::new()).ok()
        }
        Start::Entry(_) => None,
        Start::Function(function) => {
            crate::script::runtime::campaign::start_function(world, function, receiver, Vec::new())
                .ok()
        }
    };
}

fn brain(world: &World, id: u64) -> ActorBrain {
    world
        .resource::<RoundScript>()
        .campaign
        .actor_brains
        .get(&id)
        .cloned()
        .unwrap_or_default()
}

fn store(world: &mut World, id: u64, brain: ActorBrain) {
    world
        .resource_mut::<RoundScript>()
        .campaign
        .actor_brains
        .insert(id, brain);
}

fn switch_to(world: &mut World, id: u64, script: &str, hold: Option<Hold>) {
    let mut brain = brain(world, id);
    let entry = script_entry(world, id, script, "main");
    switch(world, id, &mut brain, script, Start::Entry(&entry));
    brain.hold = hold;
    store(world, id, brain);
}

fn run_scripts(world: &mut World) {
    let library = world
        .resource::<RoundScript>()
        .campaign
        .anim_library
        .clone();
    let ids = live_actors(world.resource::<RoundScript>());
    world
        .resource_mut::<RoundScript>()
        .campaign
        .actor_brains
        .retain(|id, _| ids.contains(id));
    for id in ids {
        let mut brain = brain(world, id);
        if health(&mut world.resource_mut::<RoundScript>(), id) <= 0 {
            if brain.hold != Some(Hold::Death) {
                switch_to(world, id, "death", Some(Hold::Death));
                retire_oldest_corpse(world, id);
                campaign::mark_death_danger(world, id);
                if let Value::Vector(at) = world
                    .resource_mut::<RoundScript>()
                    .object_field(id, "origin")
                {
                    let team = world.resource_mut::<RoundScript>().object_field(id, "team");
                    events::broadcast(world, "death", Some(id), at, events::Hearers::Team(&team));
                }
            }
            continue;
        }
        let thread_done = brain
            .thread
            .is_none_or(|thread| !crate::script::runtime::campaign::running(world, thread));
        campaign::finish_traverse(world, id, &mut brain, thread_done);
        brain.hold = match brain.hold {
            Some(Hold::Scripted { started, ended }) => {
                let state = world
                    .resource_mut::<RoundScript>()
                    .entities
                    .get_mut(&id)
                    .and_then(|entity| entity.anim.as_mut())
                    .and_then(|anim| anim.scripted_ended(&library));
                match state {
                    Some(true) if ended => None,
                    Some(now_ended) => Some(Hold::Scripted {
                        started: true,
                        ended: now_ended,
                    }),
                    None if started || thread_done => None,
                    None => Some(Hold::Scripted { started, ended }),
                }
            }
            Some(Hold::Arrival) if thread_done => {
                if let Some(goal) = world
                    .resource_mut::<RoundScript>()
                    .campaign
                    .actor_goals
                    .get_mut(&id)
                {
                    goal.routed_to = None;
                }
                None
            }
            Some(Hold::Custom | Hold::Pain) if thread_done => None,
            Some(Hold::Death) => None,
            hold => hold,
        };
        if brain.hold.is_none() {
            let desired = campaign::traverse_script(world, &mut brain)
                .unwrap_or_else(|| desired_script(world, id));
            if *brain.script != *desired {
                let entry = if desired.starts_with("traverse/") {
                    format!("animscripts/{desired}::main")
                } else {
                    script_entry(world, id, &desired, "main")
                };
                switch(world, id, &mut brain, &desired, Start::Entry(&entry));
            }
        }
        store(world, id, brain);
    }
}

const DOG_REACH_SLACK: f32 = 15.0;

const MAX_CORPSES: usize = 5;

fn retire_oldest_corpse(world: &mut World, dead: u64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let corpses = &mut runtime.campaign.corpses;
    corpses.retain(|id| *id != dead);
    corpses.push_back(dead);
    let old: Vec<u64> = corpses
        .drain(..corpses.len().saturating_sub(MAX_CORPSES))
        .collect();
    for id in old {
        if runtime.entities.contains_key(&id) {
            runtime.request_delete(id);
        }
    }
}

fn approach_yaw(from: f32, to: f32, max: f32) -> f32 {
    let delta = math_iw4::angle_subtract(to, from).clamp(-max, max);
    math_iw4::angle_normalize_360(from + delta)
}

fn move_actors(world: &mut World) {
    let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
    let mut notes = Vec::new();
    let mut approaches = Vec::new();
    let mut arrivals = Vec::new();
    let (players, in_the_way): (
        std::collections::HashMap<u64, [f32; 3]>,
        Vec<(u64, [f32; 3])>,
    ) = {
        let slots: Vec<(u32, u64)> = world
            .resource::<RoundScript>()
            .players
            .iter()
            .map(|(client, slot)| (*client, slot.object))
            .collect();
        let frame = crate::frame::FrameWorld::from_world(world);
        let states: Vec<(u64, [f32; 3], [f32; 3])> = slots
            .into_iter()
            .filter_map(|(client, object)| {
                let ps = frame.player(crate::world::ClientId(client))?;
                Some((object, ps.origin, ps.velocity))
            })
            .collect();
        (
            states.iter().map(|&(object, at, _)| (object, at)).collect(),
            states
                .iter()
                .map(|&(object, at, v)| (object, std::array::from_fn(|k| at[k] + v[k] * 0.1)))
                .collect(),
        )
    };
    let now = super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let nav = runtime.campaign.actor_nav.clone();
    let in_the_way: Vec<(Value, [f32; 3])> = in_the_way
        .into_iter()
        .map(|(object, at)| (runtime.object_field(object, "team"), at))
        .collect();
    let blocked: Vec<Bounds> = runtime.campaign.blocked_paths.values().copied().collect();
    let ids = live_actors(&runtime);
    let gone: Vec<u64> = runtime
        .campaign
        .actor_goals
        .keys()
        .copied()
        .filter(|id| !runtime.live(id))
        .collect();
    for id in gone {
        runtime.campaign.actor_goals.remove(&id);
    }
    for id in ids {
        let Value::Vector(mut origin) = runtime.object_field(id, "origin") else {
            continue;
        };
        let mut angles = match runtime.object_field(id, "angles") {
            Value::Vector(angles) => angles,
            _ => [0.0; 3],
        };
        let brain = runtime
            .campaign
            .actor_brains
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let (delta, turn, has_tree, scripted) = match &runtime.entities[&id].anim {
            Some(anim) => (
                anim.root_delta.0,
                anim.root_delta.1,
                true,
                anim.scripted.is_some(),
            ),
            None => ([0.0; 3], 0.0, false, false),
        };
        if scripted {
            continue;
        }
        let code_moves = matches!(&*brain.anim_mode, "" | "none" | "normal");
        let mut walking = code_moves && (brain.script.is_empty() || &*brain.script == "move");
        let root_motion =
            !code_moves || brain.traverse.is_some() || brain.hold == Some(Hold::Arrival);
        let start = origin;
        let start_yaw = angles[1];
        let mut heading = None;
        let mut path_goal = None;
        let mut traverse = None;
        if let Some(target) = runtime
            .campaign
            .actor_goals
            .get(&id)
            .map(|goal| goal.target.clone())
        {
            let goal_pos = match target {
                GoalTarget::Pos(at) => Some(at),
                GoalTarget::Entity(e) => match runtime.object_field(e, "origin") {
                    Value::Vector(at) if runtime.live(&e) => Some(at),
                    _ => players.get(&e).copied(),
                },
            };
            let Some(goal_pos) = goal_pos else {
                runtime.campaign.actor_goals.remove(&id);
                continue;
            };
            let radius =
                number(runtime.object_field(id, "goalradius")).unwrap_or(DEFAULT_GOAL_RADIUS);
            let height =
                number(runtime.object_field(id, "goalheight")).unwrap_or(DEFAULT_GOAL_HEIGHT);
            let chase = dog_chase(&mut runtime, id, origin, goal_pos, radius);
            let (goal_pos, radius) = chase.unwrap_or((goal_pos, radius));
            let team = runtime.object_field(id, "team");
            let avoid = campaign::badplaces_for(&runtime, &team, now);
            let goal = runtime.campaign.actor_goals.get_mut(&id).unwrap();
            let planar =
                ((origin[0] - goal_pos[0]).powi(2) + (origin[1] - goal_pos[1]).powi(2)).sqrt();
            if planar <= radius.max(4.0)
                && (origin[2] - goal_pos[2]).abs() <= height
                && !(goal.stepping && !goal.route.is_empty())
            {
                goal.stepping = false;
                goal.route.clear();
                if chase.is_none() && !goal.settled {
                    goal.settled = true;
                    goal.routed_to = Some(goal_pos);
                    notes.push((id, "goal"));
                }
            } else {
                if goal
                    .routed_to
                    .is_none_or(|at| distance(at, goal_pos) > REROUTE)
                    && brain.move_blocked.is_none()
                    && let Some(nav) = &nav
                {
                    goal.settled = false;
                    goal.routed_to = Some(goal_pos);
                    let mut blocked = blocked.clone();
                    blocked.extend(avoid);
                    match nav.0.route(origin, goal_pos, &blocked) {
                        Some(route) => {
                            let from = route.iter().rev().nth(1).copied().unwrap_or(origin);
                            let (x, y) = (goal_pos[0] - from[0], goal_pos[1] - from[1]);
                            let len = x.hypot(y).max(f32::EPSILON);
                            approaches.push((id, [x / len, y / len, 0.0]));
                            goal.route = route;
                        }
                        None => {
                            goal.route.clear();
                            goal.settled = true;
                            notes.push((id, "bad_path"));
                        }
                    }
                }
                if let Some(&end) = goal.route.last() {
                    path_goal = Some(if goal.stepping { end } else { goal_pos });
                }
                if let Some(&next) = goal.route.first() {
                    heading = Some([next[0] - origin[0], next[1] - origin[1]]);
                }
                let planar_to_goal =
                    |at: [f32; 3]| (at[0] - goal_pos[0]).hypot(at[1] - goal_pos[1]);
                if walking
                    && let Some((at, yaw)) = brain.arrival
                    && planar_to_goal(origin) <= planar_to_goal(at) + ARRIVAL_ERROR
                {
                    goal.route.clear();
                    walking = false;
                    angles[1] = yaw;
                    arrivals.push(id);
                }
                if walking {
                    let mut left = if has_tree {
                        delta[0].hypot(delta[1])
                    } else {
                        RUN_SPEED * dt
                    };
                    if !brain.push_player
                        && let Some(&next) = goal.route.first()
                        && player_in_the_way(&team, origin, next, left, &in_the_way)
                    {
                        left = 0.0;
                    }
                    while left > 0.0
                        && let Some(&next) = goal.route.first()
                    {
                        let d = distance(origin, next);
                        if d > 0.0 {
                            heading = Some([next[0] - origin[0], next[1] - origin[1]]);
                        }
                        if d <= left {
                            origin = next;
                            goal.route.remove(0);
                            left -= d;
                            traverse = nav.as_ref().and_then(|nav| nav.0.traversal_at(origin));
                            if traverse.is_some() {
                                break;
                            }
                        } else {
                            let t = left / d;
                            origin = std::array::from_fn(|k| origin[k] + (next[k] - origin[k]) * t);
                            left = 0.0;
                        }
                    }
                }
            }
        }
        if let Some(link) = traverse {
            campaign::begin_traverse(&mut runtime, id, link);
        }
        if root_motion {
            let world_delta = super::script_anim::rotate([0.0, angles[1], 0.0], delta);
            origin = std::array::from_fn(|k| origin[k] + world_delta[k]);
        }
        let face = |at: [f32; 3]| yaw_to(origin, at);
        let motion = heading
            .filter(|_| walking)
            .map(|[x, y]| y.atan2(x).to_degrees());
        let enemy = match runtime.object_field(id, "enemy") {
            Value::Object(enemy) => match runtime.object_field(enemy, "origin") {
                Value::Vector(at) => Some(face(at)),
                _ => players.get(&enemy).map(|at| face(*at)),
            },
            _ => None,
        };
        let want = match brain.orient {
            Orient::Angle(yaw) => Some(yaw),
            Orient::Point(at) => Some(face(at)),
            Orient::Current => None,
            Orient::Motion | Orient::Default => motion,
            Orient::Enemy => enemy.or(motion),
        };
        let yaw = match want {
            Some(want) => approach_yaw(angles[1], want, TURN_RATE * dt),
            None if root_motion => math_iw4::angle_normalize_360(angles[1] + turn),
            None => angles[1],
        };
        let moved = origin != start || yaw != start_yaw;
        angles[1] = yaw;
        if let Some([x, y]) = heading {
            let len = x.hypot(y).max(f32::EPSILON);
            runtime.set_object_field(id, "lookaheaddir", Value::Vector([x / len, y / len, 0.0]));
        }
        runtime.set_object_field(
            id,
            "pathgoalpos",
            path_goal.map_or(Value::Undefined, Value::Vector),
        );
        runtime.set_object_field(
            id,
            "velocity",
            Value::Vector(std::array::from_fn(|k| (origin[k] - start[k]) / dt)),
        );
        if moved {
            runtime.set_object_field(id, "origin", Value::Vector(origin));
            runtime.set_object_field(id, "angles", Value::Vector(angles));
        }
    }
    for (id, note) in notes {
        crate::script::runtime::raise(world, Value::Object(id), note, Vec::new());
    }
    for (id, dir) in approaches {
        if truthy(
            &world
                .resource_mut::<RoundScript>()
                .object_field(id, "requestarrivalnotify"),
        ) {
            crate::script::runtime::raise(
                world,
                Value::Object(id),
                "cover_approach",
                vec![Value::Vector(dir)],
            );
        }
    }
    for id in arrivals {
        let mut brain = brain(world, id);
        brain.arrival = None;
        store(world, id, brain);
        switch_to(world, id, "cover_arrival", Some(Hold::Arrival));
    }
}

fn init_fields(world: &mut World, id: u64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let yaw = match runtime.object_field(id, "angles") {
        Value::Vector(angles) => angles[1],
        _ => 0.0,
    };
    let forward = [yaw.to_radians().cos(), yaw.to_radians().sin(), 0.0];
    let health = health(&mut runtime, id);
    let fields = [
        ("maxhealth", Value::Int(health)),
        ("script", Value::string("init")),
        ("prevscript", Value::string("none")),
        ("movemode", Value::string("run")),
        ("combatmode", Value::string("cover")),
        ("lookaheaddir", Value::Vector(forward)),
        ("lookaheaddist", Value::Float(0.0)),
        ("lookforward", Value::Vector(forward)),
        ("desiredangle", Value::Float(yaw)),
        ("facemotion", Value::Int(1)),
        ("stairsstate", Value::string("none")),
        ("groundtype", Value::string("dirt")),
        ("damageyaw", Value::Float(0.0)),
        ("damagelocation", Value::string("none")),
        ("damagedir", Value::Vector([0.0; 3])),
        ("damagemod", Value::string("MOD_UNKNOWN")),
        ("damageweapon", Value::string("none")),
        ("damagetaken", Value::Int(0)),
        ("relativedir", Value::Int(0)),
        ("suppressionmeter", Value::Float(0.0)),
        ("fovcosine", Value::Float(DEFAULT_FOV_COS)),
        ("fovcosinebusy", Value::Float(DEFAULT_FOV_COS_BUSY)),
        (
            "maxsightdistsqrd",
            Value::Float(DEFAULT_SIGHT_DIST * DEFAULT_SIGHT_DIST),
        ),
        ("sightlatency", Value::Int(100)),
        ("interval", Value::Float(80.0)),
        ("pacifistwait", Value::Float(20.0)),
        ("walkdist", Value::Float(256.0)),
        ("walkdistfacingmotion", Value::Float(64.0)),
        ("suppressionwait", Value::Int(2_000)),
        ("suppressionduration", Value::Int(SUPPRESSION_DURATION_MS)),
        ("grenadeawareness", Value::Float(0.33)),
        ("badplaceawareness", Value::Float(0.75)),
        ("delayeddeath", Value::Int(0)),
        ("diequietly", Value::Int(0)),
        ("forceragdollimmediate", Value::Int(0)),
        ("pacifist", Value::Int(0)),
        ("ignoreexplosionevents", Value::Int(0)),
        ("providecoveringfire", Value::Int(0)),
        ("damageshield", Value::Int(0)),
        ("nogrenadereturnthrow", Value::Int(0)),
        ("noattackeraccuracymod", Value::Int(0)),
        ("nododgemove", Value::Int(0)),
        ("dontavoidplayer", Value::Int(0)),
        ("ignoresuppression", Value::Int(0)),
        ("keepclaimednode", Value::Int(0)),
        ("keepclaimednodeifvalid", Value::Int(0)),
        ("fixednode", Value::Int(0)),
        ("alertlevel", Value::string("noncombat")),
        ("alertlevelint", Value::Int(0)),
        ("velocity", Value::Vector([0.0; 3])),
        ("lookaheadhitsstairs", Value::Int(0)),
        ("teammovewaittime", Value::Int(0)),
        ("gunblockedbywall", Value::Int(0)),
        ("doingambush", Value::Int(0)),
    ];
    for (name, value) in fields {
        if runtime.object_field(id, name) == Value::Undefined {
            runtime.set_object_field(id, name, value);
        }
    }
    default_aim_limits(&mut runtime, id);
}

fn default_aim_limits(runtime: &mut RoundScript, id: u64) {
    for (name, limit) in [
        ("rightaimlimit", 45.0),
        ("leftaimlimit", -45.0),
        ("upaimlimit", 45.0),
        ("downaimlimit", -45.0),
    ] {
        runtime.set_object_field(id, name, Value::Float(limit));
    }
}

const EYE_HEIGHT: f32 = 64.0;

fn pose_eye_drop(pose: &Value) -> f32 {
    match pose {
        Value::String(pose) if &**pose == "crouch" => EYE_HEIGHT - 40.0,
        Value::String(pose) if &**pose == "prone" => EYE_HEIGHT - 11.0,
        _ => 0.0,
    }
}
const ACTOR_RADIUS: f32 = 15.0;
const ACTOR_HEIGHT: f32 = 72.0;
const STEP_HEIGHT: f32 = 18.0;

fn highest_stance(world: &mut World, node: &Value) -> Result<&'static str, String> {
    let Value::Object(node) = node else {
        return Err("receiver is not a node".into());
    };
    let kind = world
        .resource_mut::<RoundScript>()
        .object_field(*node, "type");
    let Value::String(kind) = kind else {
        return Err("receiver is not a node".into());
    };
    Ok(match &*kind {
        "Cover Prone" | "Conceal Prone" => "prone",
        "Cover Crouch" | "Cover Crouch Window" | "Conceal Crouch" => "crouch",
        _ => "stand",
    })
}

fn hit_location(tag: &str) -> &'static str {
    let tag = tag.to_ascii_lowercase();
    let side = |right: &'static str, left: &'static str| {
        if tag.ends_with("_ri") { right } else { left }
    };
    match tag.as_str() {
        "j_helmet" => "helmet",
        "j_head" | "j_head_end" | "tag_eye" => "head",
        "j_neck" => "neck",
        t if t.starts_with("j_spine4")
            || t.starts_with("j_spineupper")
            || t.starts_with("j_clavicle") =>
        {
            "torso_upper"
        }
        t if t.starts_with("j_spine") || t == "j_mainroot" || t == "pelvis" => "torso_lower",
        t if t.starts_with("j_shoulder") || t.starts_with("j_bicep") => {
            side("right_arm_upper", "left_arm_upper")
        }
        t if t.starts_with("j_elbow") || t.starts_with("j_forearm") => {
            side("right_arm_lower", "left_arm_lower")
        }
        t if t.starts_with("j_wrist")
            || t.starts_with("j_thumb")
            || t.starts_with("j_index")
            || t.starts_with("j_mid")
            || t.starts_with("j_ring")
            || t.starts_with("j_pinky") =>
        {
            side("right_hand", "left_hand")
        }
        t if t.starts_with("j_hip") => side("right_leg_upper", "left_leg_upper"),
        t if t.starts_with("j_knee") => side("right_leg_lower", "left_leg_lower"),
        t if t.starts_with("j_ankle") || t.starts_with("j_ball") => side("right_foot", "left_foot"),
        "" => "none",
        _ => "torso_upper",
    }
}

pub(crate) fn damaged(
    world: &mut World,
    id: u64,
    hit: &super::entity_damage::EntityHit,
    attacker: &Value,
    weapon: &str,
    tag: &str,
) {
    let alive = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let yaw = match runtime.object_field(id, "angles") {
            Value::Vector(angles) => angles[1],
            _ => 0.0,
        };
        let from = hit.dir[1].atan2(hit.dir[0]).to_degrees() + 180.0;
        let fields = [
            ("damagetaken", Value::Int(hit.amount)),
            ("damagedir", Value::Vector(hit.dir)),
            ("damagelocation", Value::string(hit_location(tag))),
            (
                "damageyaw",
                Value::Float(math_iw4::angle_subtract(from, yaw)),
            ),
            ("damagemod", Value::string(hit.means)),
            ("damageweapon", Value::string(weapon)),
        ];
        for (name, value) in fields {
            runtime.set_object_field(id, name, value);
        }
        if *attacker != Value::Undefined {
            runtime.set_object_field(id, "lastattacker", attacker.clone());
        }
        health(&mut runtime, id) > 0
    };
    if alive
        && let Value::Vector(at) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
    {
        let team = world.resource_mut::<RoundScript>().object_field(id, "team");
        events::broadcast(world, "pain", Some(id), at, events::Hearers::Team(&team));
    }
    let pain_allowed = !matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(id, "allowpain"),
        Value::Int(0)
    );
    if alive && pain_allowed && brain(world, id).hold.is_none() {
        campaign::mark_pain_danger(world, id);
        switch_to(world, id, "pain", Some(Hold::Pain));
    }
}

fn has_entry(world: &World, name: &str) -> bool {
    world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|program| program.names.contains_key(name))
}

fn spawners(world: &mut World) -> Vec<u64> {
    map_actors(world, true)
}

fn map_actors(world: &mut World, spawner: bool) -> Vec<u64> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let ids: Vec<u64> = runtime
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Map && e.classname.starts_with("actor_"))
        .map(|(id, _)| *id)
        .collect();
    ids.into_iter()
        .filter(|id| {
            let flags = match runtime.object_field(*id, "spawnflags") {
                Value::Int(f) => f,
                _ => 0,
            };
            (flags & SPAWNER != 0) == spawner
        })
        .collect()
}

fn aitype(classname: &str) -> String {
    let aitype = classname.strip_prefix("actor_").unwrap_or(classname);
    format!("aitype/{}", aitype.to_ascii_lowercase())
}

pub(crate) fn start_deferred(world: &mut World) {
    let entries = std::mem::take(
        &mut world
            .resource_mut::<RoundScript>()
            .campaign
            .deferred_entries,
    );
    if entries.is_empty() {
        return;
    }
    world.resource_mut::<RoundScript>().loading = true;
    super::mission::restore_game(world);
    super::vehicles::campaign::init_map_vehicles(world);
    super::weapons::campaign::place_map_weapons(world);
    super::turrets::campaign::init_map_turrets(world);
    init_spawners(world);
    for entry in entries {
        if let Err(fault) = crate::script::runtime::start(world, &entry, Value::level(), Vec::new())
        {
            world.resource_mut::<RoundScript>().fault = Some(fault);
            return;
        }
    }
}

pub(crate) fn join_campaign(world: &mut World, player: u64) -> bool {
    let mut runtime = world.resource_mut::<RoundScript>();
    if runtime.campaign.deferred_entries.is_empty() {
        return false;
    }
    runtime.set_object_field(player, "team", Value::string("allies"));
    true
}

pub(crate) fn init_spawners(world: &mut World) {
    let now = super::players::now_ms(world);
    let mut types: Vec<String> = spawners(world)
        .into_iter()
        .chain(map_actors(world, false))
        .map(|id| aitype(&world.resource::<RoundScript>().entities[&id].classname))
        .collect();
    types.sort();
    types.dedup();
    for aitype in types {
        let entry = format!("{aitype}::precache");
        if has_entry(world, &entry) {
            let _ = crate::script::runtime::run_now(world, &entry, Value::level(), Vec::new(), now);
        }
    }
    spawn_placed_actors(world);
    for spawner in spawners(world) {
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.object_field(spawner, "count") == Value::Undefined {
            runtime.set_object_field(spawner, "count", Value::Int(1));
        }
        let classname = runtime.entities[&spawner].classname.clone();
        drop(runtime);
        let entry = format!("{}::spawner", aitype(&classname));
        if has_entry(world, &entry) {
            let _ = crate::script::runtime::run_now(
                world,
                &entry,
                Value::Object(spawner),
                Vec::new(),
                now,
            );
        }
    }
}

fn spawn_placed_actors(world: &mut World) {
    for placed in map_actors(world, false) {
        let targetname = world
            .resource_mut::<RoundScript>()
            .object_field(placed, "targetname");
        let Ok(id) = spawn_from(world, placed, true) else {
            continue;
        };
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "targetname", targetname);
        let init = script_entry(world, id, "init", "main");
        if has_entry(world, &init) {
            let now = super::players::now_ms(world);
            let _ =
                crate::script::runtime::run_now(world, &init, Value::Object(id), Vec::new(), now);
        }
        world.resource_mut::<RoundScript>().delete_entity(placed);
    }
}

fn spawn_from(world: &mut World, spawner: u64, actor: bool) -> Result<u64, String> {
    let (origin, angles, classname, fields) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = runtime.object_field(spawner, "origin");
        let angles = runtime.object_field(spawner, "angles");
        let classname = runtime.entities[&spawner].classname.clone();
        let fields = runtime.objects.get(&spawner).cloned().unwrap_or_default();
        (origin, angles, classname, fields)
    };
    let Value::Vector(origin) = origin else {
        return Err("spawner has no origin".into());
    };
    let id = spawn_script_model(world, origin)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let skip = ["classname", "targetname", "count"].map(|name| runtime.symbol(name));
    let body = runtime.objects.get_mut(&id).unwrap();
    for (field, value) in fields {
        if !skip.contains(&field) {
            body.insert(field, value);
        }
    }
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", angles);
    runtime.set_object_field(id, "weapon", Value::string("none"));
    if actor {
        let entity = runtime.entities.get_mut(&id).unwrap();
        entity.kind = EntityKind::Actor;
        entity.classname = classname.clone();
        entity.can_damage = true;
    }
    let main = format!("{}::main", aitype(&classname));
    let now = super::players::now_ms(world);
    crate::script::runtime::run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    if actor {
        init_fields(world, id);
    }
    Ok(id)
}

fn spawn_actor(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let spawner = entity_id(world, receiver)?;
    let count = match world
        .resource_mut::<RoundScript>()
        .object_field(spawner, "count")
    {
        Value::Int(n) => n,
        _ => 1,
    };
    if count <= 0 {
        return Ok(Value::Undefined);
    }
    world
        .resource_mut::<RoundScript>()
        .set_object_field(spawner, "count", Value::Int(count - 1));
    let id = spawn_from(world, spawner, true)?;
    let init = script_entry(world, id, "init", "main");
    if has_entry(world, &init) {
        let now = super::players::now_ms(world);
        crate::script::runtime::run_now(world, &init, Value::Object(id), Vec::new(), now)
            .map_err(|fault| format!("{init}: {fault:?}"))?;
    }
    let guy = Value::Object(id);
    crate::script::runtime::raise(world, Value::Object(spawner), "spawned", vec![guy.clone()]);
    Ok(guy)
}

fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    let id = entity_id(world, receiver)?;
    match world.resource::<RoundScript>().entities.get(&id) {
        Some(entity) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("receiver is not an actor".into()),
    }
}

fn actors(world: &mut World, teams: &[Value]) -> Vec<Value> {
    let any = teams.is_empty() || teams.iter().any(|t| *t == Value::string("all"));
    let mut runtime = world.resource_mut::<RoundScript>();
    let ids: Vec<u64> = runtime
        .entities
        .iter()
        .filter(|(id, e)| e.kind == EntityKind::Actor && runtime.script_is_defined(id))
        .map(|(id, _)| *id)
        .collect();
    ids.into_iter()
        .filter(|id| {
            let alive = matches!(runtime.object_field(*id, "health"), Value::Int(h) if h > 0);
            alive && (any || teams.contains(&runtime.object_field(*id, "team")))
        })
        .map(Value::Object)
        .collect()
}

fn spawn_script_model(world: &mut World, origin: [f32; 3]) -> Result<u64, String> {
    let presence = super::presence::spawn_presence(world, origin)?;
    let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
    let mut runtime = world.resource_mut::<RoundScript>();
    let id = runtime.create_entity(EntityKind::Spawned, "script_model")?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
    runtime.set_object_field(id, "spawnflags", Value::Int(0));
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.presence = Some(presence);
    if let Some(number) = number {
        entity.number = number;
    }
    Ok(id)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    awareness::register(registry);
    campaign::register(registry);
    events::register(registry);
    prone::register(registry);
    registry.register(Method, "setspawnerteam", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let team = super::args::string(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "team", Value::string(&team));
        Ok(Value::Undefined)
    });
    registry.register(Method, "iswaitingonsound", |world, receiver, _| {
        entity_id(world, receiver)?;
        let waiting = world
            .resource::<RoundScript>()
            .campaign
            .timed_notifies
            .iter()
            .any(|(_, entity, _)| entity == receiver);
        Ok(Value::Int(waiting.into()))
    });
    for name in ["playsound", "playsoundasmaster"] {
        registry.register(Method, name, |world, receiver, args| {
            const UNKNOWN_SOUND_MS: i64 = 2000;
            let id = entity_id(world, receiver)?;
            let alias = super::args::string(args, 0)?;
            if args.len() > 3 {
                return Err("expected a sound alias and at most a notify and a flag".into());
            }
            let origin = match world
                .resource_mut::<RoundScript>()
                .object_field(id, "origin")
            {
                Value::Vector(origin) => origin,
                _ => [0.0; 3],
            };
            super::natives::engine::play_sound_at(world, origin, &alias)?;
            if args.len() > 1 {
                let name = super::args::string(args, 1)?;
                let now = i64::from(crate::level_time_ms(
                    world.resource::<crate::step::StepRequest>().tick,
                ));
                let length = crate::frame::FrameWorld::from_world(world)
                    .script_sound_length_ms(&alias)
                    .map_or(UNKNOWN_SOUND_MS, i64::from);
                world
                    .resource_mut::<RoundScript>()
                    .campaign
                    .timed_notifies
                    .push((now + length, receiver.clone(), name.into()));
            }
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "dospawn", |world, receiver, _| {
        spawn_actor(world, receiver)
    });
    registry.register(Method, "stalingradspawn", |world, receiver, _| {
        spawn_actor(world, receiver)
    });
    registry.register(Method, "spawndrone", |world, receiver, _| {
        let spawner = entity_id(world, receiver)?;
        spawn_from(world, spawner, false).map(Value::Object)
    });
    registry.register(Function, "getspawnerarray", |world, _, _| {
        let ids = spawners(world).into_iter().map(Value::Object).collect();
        new_array(world, ids)
    });
    registry.register(Function, "getspawnerteamarray", |world, _, args| {
        let team = super::args::arg(args, 0)?.clone();
        let mut ids = spawners(world);
        let mut runtime = world.resource_mut::<RoundScript>();
        ids.retain(|id| runtime.object_field(*id, "team") == team);
        new_array(world, ids.into_iter().map(Value::Object).collect())
    });
    registry.register(Function, "getaiarray", |world, _, args| {
        let ids = actors(world, args);
        new_array(world, ids)
    });
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let mut ids = actors(world, &args[..args.len().min(1)]);
        let species = match args.get(1) {
            Some(Value::String(species)) => species.to_string(),
            _ => "all".into(),
        };
        if species != "all" {
            if species != "human" && species != "dog" {
                return Err(format!(
                    "unknown species '{species}' (should be human, dog, or all)"
                ));
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            ids.retain(|id| match id {
                Value::Object(id) => is_dog(&mut runtime, *id) == (species == "dog"),
                _ => false,
            });
        }
        new_array(world, ids)
    });
    registry.register(Function, "getaicount", |world, _, args| {
        Ok(Value::Int(actors(world, args).len() as i32))
    });
    registry.register(Function, "getcorpsearray", |world, _, _| {
        let runtime = world.resource::<RoundScript>();
        let ids = runtime
            .campaign
            .corpses
            .iter()
            .filter(|id| runtime.entities.contains_key(id))
            .map(|id| Value::Object(*id))
            .collect();
        new_array(world, ids)
    });
    registry.register(Function, "clearallcorpses", |world, _, _| {
        let mut runtime = world.resource_mut::<RoundScript>();
        let ids: Vec<u64> = runtime.campaign.corpses.drain(..).collect();
        for id in ids {
            if runtime.entities.contains_key(&id) {
                runtime.request_delete(id);
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "forceteleport", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let origin = super::args::vector(args, 0)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        if let Some(angles) = args.get(1).filter(|v| **v != Value::Undefined) {
            runtime.set_object_field(id, "angles", angles.clone());
        }
        Ok(Value::Undefined)
    });
    registry.register(
        Function,
        "getnode",
        |world, _, args| match matching_nodes(world, args)?.as_slice() {
            [] => Ok(Value::Undefined),
            [node] => Ok(node.clone()),
            _ => Err("getnode used with more than one node".into()),
        },
    );
    registry.register(Function, "getnodearray", |world, _, args| {
        let nodes = matching_nodes(world, args)?;
        new_array(world, nodes)
    });
    registry.register(Function, "getnodesinradius", nodes_in_radius);
    registry.register(Function, "getallnodes", |world, _, _| {
        let nodes = world
            .resource::<RoundScript>()
            .campaign
            .nodes
            .iter()
            .copied()
            .map(Value::Object)
            .collect();
        new_array(world, nodes)
    });
    registry.register(Method, "setgoalpos", |world, receiver, args| {
        let at = super::args::vector(args, 0)?;
        set_goal(world, receiver, GoalTarget::Pos(at), at, None)
    });
    registry.register(Method, "setgoalnode", |world, receiver, args| {
        let (node, at) = origin_of(world, super::args::arg(args, 0)?)?;
        set_goal(world, receiver, GoalTarget::Pos(at), at, Some(node))
    });
    registry.register(Method, "setgoalentity", |world, receiver, args| {
        let (target, at) = origin_of(world, super::args::arg(args, 0)?)?;
        set_goal(world, receiver, GoalTarget::Entity(target), at, None)
    });
    registry.register(Method, "animcustom", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let Some(Value::Function(function)) = args.first() else {
            return Err("animcustom expects a function".into());
        };
        let mut brain = brain(world, id);
        switch(world, id, &mut brain, "custom", Start::Function(*function));
        brain.hold = Some(Hold::Custom);
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    registry.register(Method, "animscripted", |world, receiver, args| {
        let Ok(id) = actor(world, receiver) else {
            return super::script_anim::scripted_native(world, receiver, args);
        };
        let init = script_entry(world, id, "scripted", "init");
        if !has_entry(world, &init) {
            return super::script_anim::scripted_native(world, receiver, args);
        }
        let now = super::players::now_ms(world);
        crate::script::runtime::run_now(world, &init, receiver.clone(), args.to_vec(), now)
            .map_err(|fault| format!("{init}: {fault:?}"))?;
        if let Some(anim) = world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .and_then(|entity| entity.anim.as_mut())
        {
            anim.scripted = None;
        }
        switch_to(
            world,
            id,
            "scripted",
            Some(Hold::Scripted {
                started: false,
                ended: false,
            }),
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "animmode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let mode = super::args::string(args, 0)?.to_ascii_lowercase();
        let mut brain = brain(world, id);
        brain.anim_mode = mode.into();
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    registry.register(Method, "orientmode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let mode = super::args::string(args, 0)?.to_ascii_lowercase();
        let orient = match mode.as_str() {
            "face angle" => Orient::Angle(
                number(super::args::arg(args, 1)?.clone()).ok_or("face angle expects a yaw")?,
            ),
            "face point" => Orient::Point(super::args::vector(args, 1)?),
            "face direction" => {
                let [x, y, _] = super::args::vector(args, 1)?;
                Orient::Angle(y.atan2(x).to_degrees())
            }
            "face current" => Orient::Current,
            "face motion" => Orient::Motion,
            "face enemy" | "face enemy or motion" => Orient::Enemy,
            _ => Orient::Default,
        };
        let mut brain = brain(world, id);
        brain.orient = orient;
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    registry.register(Method, "teleport", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let origin = super::args::vector(args, 0)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        if let Some(angles) = args.get(1).filter(|v| **v != Value::Undefined) {
            runtime.set_object_field(id, "angles", angles.clone());
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "nearnode", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let (_, at) = origin_of(world, super::args::arg(args, 0)?)?;
        let origin = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        {
            Value::Vector(origin) => origin,
            _ => return Ok(Value::Int(0)),
        };
        Ok(Value::Int(near(origin, at).into()))
    });
    registry.register(Method, "nearclaimnode", |world, receiver, _| {
        near_claim_node(world, receiver, false)
    });
    registry.register(Method, "nearclaimnodeandangle", |world, receiver, _| {
        near_claim_node(world, receiver, true)
    });
    registry.register(Method, "getmuzzleangle", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(world
            .resource_mut::<RoundScript>()
            .object_field(id, "angles"))
    });
    registry.register(Method, "getmotionangle", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let (Value::Vector(dir), Value::Vector(angles)) = (
            runtime.object_field(id, "lookaheaddir"),
            runtime.object_field(id, "angles"),
        ) else {
            return Ok(Value::Float(0.0));
        };
        let motion = dir[1].atan2(dir[0]).to_degrees();
        Ok(Value::Float(math_iw4::angle_subtract(motion, angles[1])))
    });
    registry.register(Method, "kill", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(client) = runtime.player_client(id) {
            runtime.hits.push(crate::script::ScriptHit {
                piece: None,
                target: crate::script::HitTarget::Player(crate::world::ClientId(client)),
                amount: 100_000,
                origin: [0.0; 3],
                attacker: None,
                inflictor: None,
                means: "MOD_SUICIDE",
                weapon: 0,
                flags: 0,
                hitloc: 0,
            });
            return Ok(Value::Undefined);
        }
        if health(&mut runtime, id) <= 0 {
            return Ok(Value::Undefined);
        }
        runtime.set_object_field(id, "health", Value::Int(0));
        drop(runtime);
        crate::script::runtime::raise(world, receiver.clone(), "death", vec![Value::Undefined]);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getnegotiationstartnode", |world, receiver, _| {
        campaign::negotiation_node(world, receiver, true)
    });
    registry.register(Method, "getnegotiationendnode", |world, receiver, _| {
        campaign::negotiation_node(world, receiver, false)
    });
    registry.register(Method, "gethighestnodestance", |world, receiver, _| {
        Ok(Value::string(highest_stance(world, receiver)?))
    });
    registry.register(Method, "doesnodeallowstance", |world, receiver, args| {
        let order = |stance: &str| {
            ["prone", "crouch", "stand"]
                .iter()
                .position(|s| *s == stance)
        };
        let wanted = super::args::string(args, 0)?;
        let allowed = order(&wanted) <= order(highest_stance(world, receiver)?);
        Ok(Value::Int(allowed.into()))
    });
    registry.register(Method, "getshootatpos", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let drop = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let pose = match runtime.object_field(id, "a") {
                Value::Object(a) => runtime.object_field(a, "pose"),
                _ => Value::Undefined,
            };
            pose_eye_drop(&pose)
        };
        Ok(eye(world, id).map_or(Value::Undefined, |[x, y, z]| {
            Value::Vector([x, y, z - drop])
        }))
    });
    registry.register(Method, "setentitytarget", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let target = entity_id(world, super::args::arg(args, 0)?)?;
        let mut brain = brain(world, id);
        brain.entity_target = Some(target);
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearentitytarget", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut brain = brain(world, id);
        brain.entity_target = None;
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    for name in ["setflashbanged", "traversemode"] {
        registry.register(Method, name, |world, receiver, _| {
            actor(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "shoot", shoot);
    registry.register(Method, "melee", melee);
    registry.register(Method, "pushplayer", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let on = truthy(super::args::arg(args, 0)?);
        let mut state = brain(world, id);
        state.push_player = on;
        store(world, id, state);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setdefaultaimlimits", |world, receiver, _| {
        let id = actor(world, receiver)?;
        default_aim_limits(&mut world.resource_mut::<RoundScript>(), id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "seerecently", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let target = entity_id(world, super::args::arg(args, 0)?)?;
        let window = number(super::args::arg(args, 1)?.clone()).unwrap_or(0.0);
        let enemy = world
            .resource_mut::<RoundScript>()
            .object_field(id, "enemy");
        if enemy == Value::Object(target) {
            let now = super::players::now_ms(world);
            let seen = brain(world, id).enemy_seen_ms;
            return Ok(Value::Int((now - seen <= (window * 1000.0) as i64).into()));
        }
        let (Some(from), Some(to)) = (eye(world, id), eye(world, target)) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(clear_sight(world, from, to).into()))
    });
    registry.register(Method, "lastknownpos", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let target = entity_id(world, super::args::arg(args, 0)?)?;
        let brain = brain(world, id);
        let remembered = brain
            .known
            .iter()
            .find(|k| k.0 == target)
            .map(|k| k.2)
            .or(brain.last_seen.filter(|s| s.0 == target).map(|s| s.1));
        Ok(remembered.map_or_else(
            || super::players::entity_field(world, target, "origin"),
            Value::Vector,
        ))
    });
    registry.register(Method, "allowstand", |world, receiver, args| {
        allow_stance(world, receiver, args, STAND)
    });
    registry.register(Method, "allowcrouch", |world, receiver, args| {
        allow_stance(world, receiver, args, CROUCH)
    });
    registry.register(Method, "allowprone", |world, receiver, args| {
        allow_stance(world, receiver, args, PRONE)
    });
    registry.register(Method, "allowfire", |world, receiver, args| {
        allow_stance(world, receiver, args, FIRE)
    });
    registry.register(Method, "allowmelee", |world, receiver, args| {
        allow_stance(world, receiver, args, MELEE)
    });
    registry.register(Method, "allowedstances", |world, receiver, args| {
        let mut allowed = 0;
        for index in 0..args.len() {
            allowed |= stance_bit(&super::args::string(args, index)?);
        }
        let stances = STAND | CROUCH | PRONE;
        let others = stances_denied(world, receiver)? & !stances;
        set_stances_denied(world, receiver, others | (stances & !allowed))
    });
    registry.register(Method, "isstanceallowed", |world, receiver, args| {
        let bit = stance_bit(&super::args::string(args, 0)?);
        let denied = stances_denied(world, receiver)?;
        Ok(Value::Int((denied & bit == 0).into()))
    });
    for namespace in [Method, Function] {
        registry.register(namespace, "disconnectpaths", |world, receiver, _| {
            block_paths(world, receiver, true)
        });
        registry.register(namespace, "connectpaths", |world, receiver, _| {
            block_paths(world, receiver, false)
        });
    }
    registry.register(Method, "shouldfacemotion", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let face = truthy(
            &world
                .resource_mut::<RoundScript>()
                .object_field(id, "facemotion"),
        );
        let free = matches!(brain(world, id).orient, Orient::Default | Orient::Motion);
        Ok(Value::Int((face && free).into()))
    });
    registry.register(Method, "checkgrenadethrow", |world, receiver, args| {
        let id = actor(world, receiver)?;
        if !has_grenades(world, id) {
            return Ok(Value::Undefined);
        }
        let Value::Object(enemy) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "enemy")
        else {
            return Ok(Value::Undefined);
        };
        let Value::Vector(at) = super::players::entity_field(world, enemy, "origin") else {
            return Ok(Value::Undefined);
        };
        let spread = number(super::args::arg(args, 1)?.clone()).unwrap_or(0.0);
        let angle = (id as f32 * 2.399 + super::players::now_ms(world) as f32 * 0.001).sin()
            * std::f32::consts::PI;
        let target = [
            at[0] + angle.cos() * spread * 0.5,
            at[1] + angle.sin() * spread * 0.5,
            at[2],
        ];
        grenade_throw(world, id, args, target)
    });
    registry.register(Method, "checkgrenadethrowpos", |world, receiver, args| {
        let id = actor(world, receiver)?;
        if !has_grenades(world, id) {
            return Ok(Value::Undefined);
        }
        let target = super::args::vector(args, 1)?;
        grenade_throw(world, id, args, target)
    });
    registry.register(Method, "throwgrenade", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut brain = brain(world, id);
        let Some(target) = brain.grenade_target.take() else {
            return Ok(Value::Undefined);
        };
        let held = brain.held_grenade.take();
        store(world, id, brain);
        let name = match held {
            Some((weapon, _)) => crate::script_player::weapon_name(
                &crate::frame::FrameWorld::from_world(world),
                weapon,
            )
            .to_string(),
            None => match world
                .resource_mut::<RoundScript>()
                .object_field(id, "grenadeweapon")
            {
                Value::String(name) => name.to_string(),
                _ => "fraggrenade".to_owned(),
            },
        };
        let from = eye(world, id).unwrap_or(target);
        let team = world.resource_mut::<RoundScript>().object_field(id, "team");
        let fuse = held.map(|(_, fuse)| fuse);
        super::weapons::campaign::schedule_grenade(world, &name, from, target, team, fuse)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if held.is_none()
            && let Some(ammo) = number(runtime.object_field(id, "grenadeammo")).filter(|n| *n > 0.0)
        {
            runtime.set_object_field(id, "grenadeammo", Value::Int(ammo as i32 - 1));
        }
        drop(runtime);
        crate::script::runtime::raise(world, Value::Object(id), "grenade_fire", Vec::new());
        Ok(Value::Undefined)
    });
    registry.register(Method, "pickupgrenade", |world, receiver, _| {
        let id = actor(world, receiver)?;
        grenades::pick_up(world, id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "isgrenadepossafe", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let pos = super::args::vector(args, 1)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let team = runtime.object_field(id, "team");
        let safe = live_actors(&runtime).into_iter().all(|other| {
            other == id
                || runtime.object_field(other, "team") != team
                || !matches!(runtime.object_field(other, "origin"),
                    Value::Vector(at) if distance(at, pos) < GRENADE_SAFE_RADIUS)
        });
        Ok(Value::Int(safe.into()))
    });
    registry.register(Method, "safeteleport", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let at = super::args::vector(args, 0)?;
        let angles = super::args::optional(args, 1, super::args::vector)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(at));
        if let Some(angles) = angles {
            runtime.set_object_field(id, "angles", Value::Vector(angles));
        }
        if let Some(goal) = runtime.campaign.actor_goals.get_mut(&id) {
            goal.routed_to = None;
            goal.route.clear();
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getentnum", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let number = match runtime.player_client(id) {
            Some(client) => client as i32,
            None => runtime.entities.get(&id).map_or(-1, |entity| entity.number),
        };
        Ok(Value::Int(number))
    });
    registry.register(Method, "startcoverarrival", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let at = super::args::vector(args, 0)?;
        let yaw = number(super::args::arg(args, 1)?.clone()).unwrap_or(0.0);
        let mut brain = brain(world, id);
        brain.arrival = Some((at, yaw));
        store(world, id, brain);
        Ok(Value::Undefined)
    });
    registry.register(Method, "issuppressed", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let suppressed = brain(world, id).suppressed.is_some();
        let mut runtime = world.resource_mut::<RoundScript>();
        let meter = number(runtime.object_field(id, "suppressionmeter")).unwrap_or(0.0);
        Ok(Value::Int((meter > 0.0 || suppressed).into()))
    });
    registry.register(Method, "lastknowntime", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let target = entity_id(world, super::args::arg(args, 0)?)?;
        Ok(Value::Int(match brain(world, id).last_seen {
            Some((seen, _, ms)) if seen == target => i32::try_from(ms).unwrap_or(i32::MAX),
            _ => 0,
        }))
    });
    registry.register(Method, "canattackenemynode", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let Value::Object(enemy) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "enemy")
        else {
            return Ok(Value::Int(0));
        };
        let node = world
            .resource_mut::<RoundScript>()
            .object_field(enemy, "node");
        let to = match &node {
            Value::Object(node) => match world
                .resource_mut::<RoundScript>()
                .object_field(*node, "origin")
            {
                Value::Vector([x, y, z]) => Some([x, y, z + EYE_HEIGHT]),
                _ => None,
            },
            _ => eye(world, enemy),
        };
        let Some(to) = to else {
            return Ok(Value::Int(0));
        };
        let from = super::turrets::muzzle(world, id);
        Ok(Value::Int(clear_sight(world, from, to).into()))
    });
    registry.register(Method, "maymovetopoint", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let to = super::args::vector(args, 0)?;
        let check_drop = super::args::optional(args, 1, super::args::int)?.is_none_or(|n| n != 0);
        let Value::Vector(from) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        else {
            return Ok(Value::Int(0));
        };
        let blocked = crosses_fire_line(&brain(world, id).fire_lines, from, to);
        Ok(Value::Int(
            (!blocked && may_move(world, from, to, check_drop)).into(),
        ))
    });
    registry.register(
        Method,
        "maymovefrompointtopoint",
        |world, receiver, args| {
            actor(world, receiver)?;
            let from = super::args::vector(args, 0)?;
            let to = super::args::vector(args, 1)?;
            let check_drop =
                super::args::optional(args, 2, super::args::int)?.is_none_or(|n| n != 0);
            Ok(Value::Int(may_move(world, from, to, check_drop).into()))
        },
    );
    registry.register(Method, "ismovesuppressed", |world, receiver, _| {
        let id = actor(world, receiver)?;
        Ok(Value::Int((!brain(world, id).fire_lines.is_empty()).into()))
    });
    registry.register(Method, "dropweapon", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let name = super::args::string(args, 0)?;
        let position = super::args::string(args, 1)?;
        let tag = match position.as_str() {
            "left" => "tag_weapon_left",
            "chest" => "tag_weapon_chest",
            "back" => "tag_stowed_back",
            _ => "tag_weapon_right",
        };
        let (origin, angles) = match super::presence::tag_world(world, id, tag) {
            Some((origin, axis)) => (origin, math_iw4::axis_to_angles(axis)),
            None => {
                let mut runtime = world.resource_mut::<RoundScript>();
                let origin = match runtime.object_field(id, "origin") {
                    Value::Vector([x, y, z]) => [x, y, z + 40.0],
                    _ => return Ok(Value::Undefined),
                };
                let angles = match runtime.object_field(id, "angles") {
                    Value::Vector(angles) => angles,
                    _ => [0.0; 3],
                };
                (origin, angles)
            }
        };
        match super::weapons::campaign::place_weapon(world, &name, origin, angles)? {
            Some(number) => {
                super::natives::player::new_item_entity(world, number, &format!("weapon_{name}"))
            }
            None => Ok(Value::Undefined),
        }
    });
    registry.register(Method, "cansee", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let target = entity_id(world, super::args::arg(args, 0)?)?;
        let (Some(from), Some(to)) = (eye(world, id), eye(world, target)) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(clear_sight(world, from, to).into()))
    });
    registry.register(Method, "canshootenemy", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let Value::Object(enemy) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "enemy")
        else {
            return Ok(Value::Int(0));
        };
        let from = super::turrets::muzzle(world, id);
        let Some(to) = eye(world, enemy) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(clear_sight(world, from, to).into()))
    });
    registry.register(Method, "canshoot", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let to = super::args::vector(args, 0)?;
        let from = super::turrets::muzzle(world, id);
        Ok(Value::Int(clear_sight(world, from, to).into()))
    });
    registry.register(Method, "getmuzzlepos", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Vector(super::turrets::muzzle(world, id)))
    });
    registry.register(Method, "clearenemy", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "enemy", Value::Undefined);
        Ok(Value::Undefined)
    });
    registry.register(Method, "isbadguy", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let team = world.resource_mut::<RoundScript>().object_field(id, "team");
        Ok(Value::Int((team == Value::string("axis")).into()))
    });
    fn make_sentient_target(world: &mut World, id: u64, team: String) -> Result<Value, String> {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime
            .entities
            .get(&id)
            .is_some_and(|e| e.kind == EntityKind::Actor);
        if actor || runtime.player_client(id).is_some() {
            return Err("makeentitysentient: not for an AI or a player".into());
        }
        runtime.campaign.sentients.insert(id, Value::string(&team));
        Ok(Value::Int(1))
    }
    fn free_sentient_target(world: &mut World, id: u64) -> Result<Value, String> {
        world
            .resource_mut::<RoundScript>()
            .campaign
            .sentients
            .remove(&id);
        Ok(Value::Undefined)
    }
    registry.register(Method, "makeentitysentient", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let team = super::args::string(args, 0)?;
        make_sentient_target(world, id, team)
    });
    registry.register(Function, "makeentitysentient", |world, _, args| {
        let id = entity_id(world, super::args::arg(args, 0)?)?;
        let team = super::args::string(args, 1)?;
        make_sentient_target(world, id, team)
    });
    registry.register(Method, "freeentitysentient", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        free_sentient_target(world, id)
    });
    registry.register(Function, "freeentitysentient", |world, _, args| {
        let id = entity_id(world, super::args::arg(args, 0)?)?;
        free_sentient_target(world, id)
    });
    registry.register(Function, "issentient", |world, _, args| {
        let sentient = match super::args::arg(args, 0)? {
            Value::Object(id) => {
                let runtime = world.resource::<RoundScript>();
                runtime.player_client(*id).is_some()
                    || runtime.campaign.sentients.contains_key(id)
                    || runtime
                        .entities
                        .get(id)
                        .is_some_and(|e| e.kind == EntityKind::Actor)
            }
            _ => false,
        };
        Ok(Value::Int(sentient.into()))
    });
    registry.register(Function, "isspawner", |world, _, args| {
        let Value::Object(id) = super::args::arg(args, 0)? else {
            return Ok(Value::Int(0));
        };
        let id = *id;
        let mut runtime = world.resource_mut::<RoundScript>();
        let Some(entity) = runtime
            .entities
            .get(&id)
            .filter(|e| e.kind == EntityKind::Map)
        else {
            return Ok(Value::Int(0));
        };
        let flag = if entity.classname.starts_with("actor_") {
            SPAWNER
        } else if super::entities::is_vehicle_class(&entity.classname) {
            VEHICLE_SPAWNER
        } else {
            return Ok(Value::Int(0));
        };
        let flags = match runtime.object_field(id, "spawnflags") {
            Value::Int(flags) => flags,
            _ => 0,
        };
        Ok(Value::Int((flags & flag != 0).into()))
    });
    registry.register(Function, "isai", |world, _, args| {
        let actor = world
            .resource::<RoundScript>()
            .entity(super::args::arg(args, 0)?)
            .is_some_and(|(_, e)| e.kind == EntityKind::Actor);
        Ok(Value::Int(actor.into()))
    });
}
