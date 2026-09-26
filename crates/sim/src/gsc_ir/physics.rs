use super::iw4_natives::string;
use super::natives_math::{arg, vector};
use super::runtime::raise;
use super::*;
use crate::bullet_collision::MASK_PLAYER_SOLID;
use crate::frame::FrameWorld;
use crate::world::ClientId;
use bevy_ecs::prelude::World;
use natives::Namespace::Method;

const GRAVITY: f32 = 800.0;
const TICK_S: f32 = crate::MATCH_TICK_MS as f32 / 1000.0;
const SETTLE_TICKS: u32 = 200;
const BODY_MINS: [f32; 3] = [-12.0, -12.0, 0.0];
const BODY_MAXS: [f32; 3] = [12.0, 12.0, 24.0];
const USE_RADIUS: f32 = 128.0;
const HINTS: [&str; 5] = [
    "HINT_NONE",
    "HINT_NOICON",
    "HINT_ACTIVATE",
    "HINT_HEALTH",
    "HINT_FRIENDLY",
];

fn object_of(world: &World, receiver: &Value) -> Result<u64, String> {
    super::natives_engine::entity_id(world, receiver)
}

fn usable<'a>(
    world: &'a mut World,
    receiver: &Value,
) -> Result<&'a mut super::entities::Usable, String> {
    let object = object_of(world, receiver)?;
    let runtime = world.resource_mut::<Runtime>().into_inner();
    Ok(runtime
        .entities
        .get_mut(&object)
        .unwrap()
        .usable
        .get_or_insert_with(|| super::entities::Usable {
            cursor: 1,
            hint: -1,
            enabled: false,
            barred: Default::default(),
        }))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    for name in ["physicslaunchserver", "physicslaunchclient"] {
        registry.register(Method, name, |world, receiver, args| {
            let object = object_of(world, receiver)?;
            vector(args, 0)?;
            let force = vector(args, 1)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.linked_to = None;
            entity.motion.clear();
            entity.physics = Some(super::entities::Physics {
                velocity: force,
                ticks: 0,
            });
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "makeusable", |world, receiver, _| {
        usable(world, receiver)?.enabled = true;
        Ok(Value::Undefined)
    });
    registry.register(Method, "makeunusable", |world, receiver, _| {
        usable(world, receiver)?.enabled = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setcursorhint", |world, receiver, args| {
        let name = string(args, 0)?;
        let cursor = HINTS
            .iter()
            .position(|hint| hint.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("{name} is not a valid hint type"))?;
        usable(world, receiver)?.cursor = cursor as i32;
        Ok(Value::Undefined)
    });
    registry.register(Method, "sethintstring", |world, receiver, args| {
        let hint = super::hud::string_index(world, arg(args, 0)?)?;
        usable(world, receiver)?.hint = hint;
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableplayeruse", |world, receiver, args| {
        let client = super::natives_player::player(world, arg(args, 0)?)?;
        usable(world, receiver)?.barred.insert(client);
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableplayeruse", |world, receiver, args| {
        let client = super::natives_player::player(world, arg(args, 0)?)?;
        usable(world, receiver)?.barred.remove(&client);
        Ok(Value::Undefined)
    });
}

pub(super) fn advance(world: &mut World) {
    let bodies: Vec<(u64, super::entities::Physics)> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter_map(|(object, e)| Some((*object, e.physics?)))
        .collect();
    for (object, mut body) in bodies {
        let (origin, angles) = {
            let mut runtime = world.resource_mut::<Runtime>();
            match (
                runtime.object_field(object, "origin"),
                runtime.object_field(object, "angles"),
            ) {
                (Value::Vector(o), Value::Vector(a)) => (o, a),
                (Value::Vector(o), _) => (o, [0.0; 3]),
                _ => continue,
            }
        };
        body.velocity[2] -= GRAVITY * TICK_S;
        let end: [f32; 3] = std::array::from_fn(|i| origin[i] + body.velocity[i] * TICK_S);
        let trace = FrameWorld::from_world(world).trace_static_world(
            origin,
            end,
            BODY_MINS,
            BODY_MAXS,
            MASK_PLAYER_SOLID,
        );
        let fraction = if trace.startsolid != 0 {
            0.0
        } else {
            trace.fraction
        };
        let at: [f32; 3] = std::array::from_fn(|i| origin[i] + (end[i] - origin[i]) * fraction);
        body.ticks += 1;
        let rested = fraction < 1.0 || body.ticks >= SETTLE_TICKS;
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.set_object_field(object, "origin", Value::Vector(at));
        if rested {
            runtime.set_object_field(object, "angles", Value::Vector([0.0, angles[1], 0.0]));
        }
        runtime.entities.get_mut(&object).unwrap().physics = (!rested).then_some(body);
        drop(runtime);
        if rested {
            raise(world, Value::Object(object), "physics_finished", Vec::new());
        }
    }
}

pub(super) fn select_usables(world: &mut World) {
    let usables: Vec<(u64, [f32; 3], super::entities::Usable)> = {
        let mut runtime = world.resource_mut::<Runtime>();
        let candidates: Vec<(u64, super::entities::Usable)> = runtime
            .entities
            .iter()
            .filter(|(_, e)| !e.hidden)
            .filter_map(|(object, e)| Some((*object, e.usable.clone().filter(|u| u.enabled)?)))
            .collect();
        candidates
            .into_iter()
            .filter_map(
                |(object, usable)| match runtime.object_field(object, "origin") {
                    Value::Vector(at) => Some((object, at, usable)),
                    _ => None,
                },
            )
            .collect()
    };
    let clients: Vec<u32> = world
        .resource::<Runtime>()
        .players
        .keys()
        .copied()
        .collect();
    let mut selected = std::collections::BTreeMap::new();
    let mut frame = FrameWorld::from_world(world);
    for client in clients {
        let id = ClientId(client);
        if !frame.client_meta(id).is_some_and(|m| {
            m.lifecycle == crate::ClientLifecycle::Alive && !m.controls.usability_disabled
        }) {
            continue;
        }
        let Some(ps) = frame.player(id).copied() else {
            continue;
        };
        let eye = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ];
        let nearest = usables
            .iter()
            .filter(|(_, _, usable)| !usable.barred.contains(&client))
            .map(|(object, at, usable)| {
                let d2: f32 = (0..3).map(|i| (at[i] - eye[i]).powi(2)).sum();
                (d2, *object, usable)
            })
            .filter(|(d2, _, _)| *d2 <= USE_RADIUS * USE_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, object, usable)) = nearest else {
            continue;
        };
        selected.insert(client, object);
        if let Some(ps) = frame.player_mut(id)
            && ps.cursor_hint == 0
        {
            ps.cursor_hint = usable.cursor;
            ps.cursor_hint_string = usable.hint;
        }
    }
    world.resource_mut::<Runtime>().use_selected = selected;
}
