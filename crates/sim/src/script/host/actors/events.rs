use bevy_ecs::prelude::World;
use std::sync::Arc;

use super::super::args::string;
use super::super::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

const KINDS: [(&str, (&str, f32), (&str, f32)); 12] = [
    (
        "footstep",
        ("ai_eventdistfootstep", 256.0),
        ("ai_busyeventdistfootstep", 256.0),
    ),
    (
        "footstep_walk",
        ("ai_eventdistfootstepwalk", 128.0),
        ("ai_busyeventdistfootstepwalk", 128.0),
    ),
    (
        "footstep_sprint",
        ("ai_eventdistfootstepsprint", 400.0),
        ("ai_busyeventdistfootstepsprint", 256.0),
    ),
    (
        "pain",
        ("ai_eventdistpain", 512.0),
        ("ai_busyeventdistpain", 256.0),
    ),
    (
        "death",
        ("ai_eventdistdeath", 1024.0),
        ("ai_busyeventdistdeath", 256.0),
    ),
    (
        "explode",
        ("ai_eventdistexplosion", 1024.0),
        ("ai_busyeventdistexplosion", 1024.0),
    ),
    (
        "grenade danger",
        ("ai_eventdistgrenadeping", 512.0),
        ("ai_busyeventdistgrenadeping", 256.0),
    ),
    (
        "gunshot",
        ("ai_eventdistgunshot", 2048.0),
        ("ai_busyeventdistgunshot", 400.0),
    ),
    (
        "gunshot_teammate",
        ("ai_eventdistgunshotteam", 2048.0),
        ("ai_busyeventdistgunshotteam", 256.0),
    ),
    (
        "silenced_shot",
        ("ai_eventdistsilencedshot", 128.0),
        ("ai_busyeventdistsilencedshot", 128.0),
    ),
    (
        "bulletwhizby",
        ("ai_eventdistbullet", 128.0),
        ("ai_busyeventdistbullet", 128.0),
    ),
    (
        "projectile_impact",
        ("ai_eventdistprojimpact", 256.0),
        ("ai_busyeventdistprojimpact", 256.0),
    ),
];

const MAX_LISTENERS: usize = 34;

#[derive(Clone, Copy)]
pub(super) enum Hearers<'a> {
    All,
    Others(&'a Value),
    Team(&'a Value),
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "addaieventlistener", |world, receiver, args| {
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        let kind = string(args, 0)?;
        if !KINDS.iter().any(|(name, _, _)| *name == kind) {
            return Err(format!("Unable to find AI event for [{kind}]"));
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let runtime = &mut *runtime;
        let listeners = &mut runtime.campaign.ai_listeners;
        listeners.retain(|id, _| runtime.entities.contains_key(id));
        if !listeners.contains_key(&id) && listeners.len() >= MAX_LISTENERS {
            return Err(format!("Max listeners exceeded; entity id: {id}"));
        }
        listeners.entry(id).or_default().insert(Arc::from(kind));
        Ok(Value::Undefined)
    });
}

pub(super) fn reach(world: &World, kind: &str, busy: bool) -> f32 {
    let Some((_, idle, busy_reach)) = KINDS.iter().find(|(name, _, _)| *name == kind) else {
        return 0.0;
    };
    let (dvar, default) = if busy { busy_reach } else { idle };
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(*dvar)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(*default)
}

pub(super) fn broadcast(
    world: &mut World,
    kind: &str,
    originator: Option<u64>,
    at: [f32; 3],
    hearers: Hearers,
) {
    deliver(world, kind, originator, at, hearers, |origin, reach| {
        super::distance(origin, at) <= reach
    });
}

const LINE_HALF_HEIGHT: f32 = 45.0;
const LINE_EYE_HEIGHT: f32 = 36.0;

pub(crate) fn projectile_impact(world: &mut World, owner: u64, at: [f32; 3]) {
    if world
        .resource::<RoundScript>()
        .campaign
        .ai_listeners
        .is_empty()
    {
        return;
    }
    let Value::Vector(from) = super::super::players::entity_field(world, owner, "origin") else {
        return;
    };
    let team = world
        .resource_mut::<RoundScript>()
        .object_field(owner, "team");
    let line = glam::Vec3::from_array(at) - glam::Vec3::from_array(from);
    deliver(
        world,
        "projectile_impact",
        Some(owner),
        at,
        Hearers::Others(&team),
        |origin, reach| {
            let along = line.dot(glam::Vec3::from_array(origin) - glam::Vec3::from_array(from));
            if along < 0.0 {
                return false;
            }
            let near = if along < line.length_squared() {
                glam::Vec3::from_array(from) + line * (along / line.length_squared())
            } else {
                glam::Vec3::from_array(at)
            };
            let across = glam::Vec2::new(origin[0] - near.x, origin[1] - near.y).length();
            across <= reach && (origin[2] + LINE_EYE_HEIGHT - near.z).abs() < LINE_HALF_HEIGHT
        },
    );
}

fn deliver(
    world: &mut World,
    kind: &str,
    originator: Option<u64>,
    at: [f32; 3],
    hearers: Hearers,
    near: impl Fn([f32; 3], f32) -> bool,
) {
    let (idle, busy) = (reach(world, kind, false), reach(world, kind, true));
    let receivers: Vec<(u64, bool, bool)> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let mut ids: Vec<u64> = super::live_actors(&runtime)
            .into_iter()
            .filter(|id| super::health(&mut runtime, *id) > 0)
            .collect();
        ids.extend(runtime.campaign.ai_listeners.keys().copied());
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .filter(|id| Some(*id) != originator)
            .map(|id| {
                let listens = runtime
                    .campaign
                    .ai_listeners
                    .get(&id)
                    .is_some_and(|kinds| kinds.contains(kind));
                let actor = runtime
                    .entities
                    .get(&id)
                    .is_some_and(|entity| entity.kind == EntityKind::Actor);
                (id, listens, actor)
            })
            .filter(|(_, listens, actor)| *listens || *actor)
            .collect()
    };
    for (id, listens, actor) in receivers {
        let (team, enemy) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let enemy = match runtime.object_field(id, "enemy") {
                Value::Object(enemy)
                    if runtime
                        .entities
                        .get(&id)
                        .is_some_and(|entity| entity.kind == EntityKind::Actor) =>
                {
                    Some(enemy)
                }
                _ => None,
            };
            (runtime.object_field(id, "team"), enemy)
        };
        let reach = if enemy.is_some_and(|enemy| Some(enemy) != originator) {
            busy
        } else {
            idle
        };
        let hears = matches!(team, Value::Undefined)
            || match hearers {
                Hearers::All => true,
                Hearers::Others(from) => team != *from,
                Hearers::Team(from) => team == *from,
            };
        let origin = super::super::players::entity_field(world, id, "origin");
        let Value::Vector(origin) = origin else {
            continue;
        };
        if hears && near(origin, reach) {
            if actor {
                react(world, id, kind, originator, at);
            }
            if listens {
                let args = vec![
                    Value::string(kind),
                    originator.map_or(Value::Undefined, Value::Object),
                    Value::Vector(at),
                ];
                crate::script::runtime::raise(world, Value::Object(id), "ai_event", args);
            }
        }
    }
}

const HEARD_SEEN_MS: i64 = 2_000;
const EXPLOSION_LEARN_DIST: f32 = 512.0;

fn react(world: &mut World, id: u64, kind: &str, originator: Option<u64>, at: [f32; 3]) {
    let field = |world: &mut World, object: u64, name: &str| {
        super::super::players::entity_field(world, object, name)
    };
    let (learn, alert) = match kind {
        "gunshot" | "silenced_shot" => (originator, true),
        "footstep" | "footstep_walk" | "footstep_sprint" => (originator, false),
        "gunshot_teammate" => (
            originator.and_then(|ally| match field(world, ally, "enemy") {
                Value::Object(enemy) => Some(enemy),
                _ => None,
            }),
            true,
        ),
        "pain" | "death" => (
            originator.and_then(|ally| match field(world, ally, "lastattacker") {
                Value::Object(attacker) => Some(attacker),
                _ => None,
            }),
            kind == "pain",
        ),
        "explode" => {
            if super::truthy(&field(world, id, "ignoreexplosionevents")) {
                return;
            }
            let near = match field(world, id, "origin") {
                Value::Vector(origin) => super::distance(origin, at) < EXPLOSION_LEARN_DIST,
                _ => false,
            };
            let args = vec![
                Value::Vector(at),
                originator.map_or(Value::Undefined, Value::Object),
            ];
            crate::script::runtime::raise(world, Value::Object(id), "explode", args);
            (originator.filter(|_| near), true)
        }
        _ => (None, false),
    };
    if alert {
        let mut runtime = world.resource_mut::<RoundScript>();
        if !matches!(runtime.object_field(id, "alertlevelint"), Value::Int(level) if level >= 2) {
            runtime.set_object_field(id, "alertlevel", Value::string("combat"));
            runtime.set_object_field(id, "alertlevelint", Value::Int(2));
        }
    }
    let Some(target) = learn else { return };
    if !world.resource::<RoundScript>().live(&target)
        || !super::hostile(&field(world, id, "team"), &field(world, target, "team"))
    {
        return;
    }
    let Value::Vector(position) = field(world, target, "origin") else {
        return;
    };
    let now = super::super::players::now_ms(world);
    let mut brain = super::brain(world, id);
    if brain
        .known
        .iter()
        .any(|(known, seen, _)| *known == target && now - seen < HEARD_SEEN_MS)
    {
        return;
    }
    brain.known.retain(|(known, _, _)| *known != target);
    brain.known.push((target, now, position));
    super::store(world, id, brain);
}

pub(super) fn raise_tick_events(world: &mut World) {
    {
        let runtime = world.resource::<RoundScript>();
        if runtime.campaign.ai_listeners.is_empty() && super::live_actors(&runtime).is_empty() {
            return;
        }
    }
    let tick = world.resource::<crate::step::StepRequest>().tick.0;
    let frame = crate::frame::FrameWorld::from_world(world);
    let events: Vec<(&'static str, i32, [f32; 3])> = frame
        .entity_events()
        .iter()
        .filter(|record| record.tick.0 + 1 == tick)
        .filter_map(|record| {
            let payload = &record.payload;
            match entity_iw4::entity_event_action(record.event).ok()? {
                entity_iw4::EntityEventAction::WeaponFire => Some((
                    if frame.weapon_silenced(payload.weapon) {
                        "silenced_shot"
                    } else {
                        "gunshot"
                    },
                    payload.number,
                    payload.origin,
                )),
                entity_iw4::EntityEventAction::Explosion => {
                    Some(("explode", payload.attacker_entity_num, payload.origin))
                }
                _ => None,
            }
        })
        .collect();
    let noise: Vec<(&'static str, i32, [f32; 3])> = frame
        .player_noise()
        .iter()
        .map(|(client, kind)| (*kind, client.0 as i32, [0.0; 3]))
        .collect();
    for (kind, number, at) in events.into_iter().chain(noise) {
        let originator = super::shooter(world, number);
        if kind == "explode" {
            broadcast(world, kind, originator, at, Hearers::All);
            continue;
        }
        let team = originator.map_or(Value::Undefined, |id| {
            world.resource_mut::<RoundScript>().object_field(id, "team")
        });
        let at = match originator {
            Some(id) => match super::super::players::entity_field(world, id, "origin") {
                Value::Vector(origin) => origin,
                _ => at,
            },
            None => at,
        };
        broadcast(world, kind, originator, at, Hearers::Others(&team));
    }
}
