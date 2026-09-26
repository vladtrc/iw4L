use super::entities::{EntityKind, Motion, MotionPath, ScriptEntity};
use super::iw4_natives::{precache, string};
use super::natives_math::{arg, distance_sq, float, int, kind, new_array, optional, vector};
use super::runtime::raise;
use super::*;
use crate::bullet_collision::{MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS, PLAYER_MINS};
use bevy_ecs::prelude::World;

const GRAVITY: f32 = 800.0;
const ZERO: [f32; 3] = [0.0; 3];

fn runtime(world: &mut World) -> bevy_ecs::world::Mut<'_, Runtime> {
    world.resource_mut::<Runtime>()
}

fn now_ms(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS))
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn transpose(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|col| m[col][row]))
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "not an entity",
        other => kind(other),
    }
}

pub(super) fn entity_id(world: &World, value: &Value) -> Result<u64, String> {
    match world.resource::<Runtime>().entity(value) {
        Some((id, e)) if e.kind != EntityKind::HudElem => Ok(id),
        Some(_) => Err("hud element is not an entity".into()),
        None => Err(format!("{} is not an entity", describe(value))),
    }
}

fn with_entity(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut ScriptEntity),
) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    change(runtime(world).entities.get_mut(&id).unwrap());
    Ok(Value::Undefined)
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match super::players::entity_field(world, id, name) {
        Value::Vector(v) => v,
        _ => ZERO,
    }
}

fn origin_of(world: &mut World, value: &Value) -> Result<[f32; 3], String> {
    match value {
        Value::Object(id) if world.resource::<Runtime>().objects.contains_key(id) => {
            Ok(vector_field(world, *id, "origin"))
        }
        Value::Vector(v) => Ok(*v),
        other => Err(format!("{} has no origin", describe(other))),
    }
}

fn keyed_array(world: &mut World, pairs: Vec<(&str, Value)>) -> Result<Value, String> {
    let mut runtime = runtime(world);
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(
        id,
        pairs
            .into_iter()
            .map(|(k, v)| (ArrayKey::String(k.into()), v))
            .collect(),
    );
    Ok(Value::Array(id))
}

fn trace(
    world: &mut World,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> trace_iw4::Trace {
    crate::frame::FrameWorld::from_world(world).trace_world(start, end, mins, maxs, mask)
}

fn trace_passed(world: &mut World, start: [f32; 3], end: [f32; 3], mask: u32) -> bool {
    let t = trace(world, start, end, ZERO, ZERO, mask);
    t.fraction >= 1.0 && t.startsolid == 0
}

fn matching_entities(world: &mut World, args: &[Value]) -> Result<Vec<u64>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?.to_ascii_lowercase();
    let mut runtime = runtime(world);
    let field = runtime.symbol(&key);
    let runtime = &*runtime;
    Ok(runtime
        .entities
        .iter()
        .filter(|(_, e)| e.kind != EntityKind::HudElem)
        .map(|(id, _)| *id)
        .filter(|id| {
            matches!(
                runtime.objects.get(id).and_then(|f| f.get(&field)),
                Some(Value::String(s)) if **s == *value
            )
        })
        .collect())
}

fn classname_prefix(world: &World, ids: Vec<u64>, prefix: &str) -> Vec<u64> {
    let runtime = world.resource::<Runtime>();
    ids.into_iter()
        .filter(|id| runtime.entities[id].classname.starts_with(prefix))
        .collect()
}

fn single(ids: Vec<u64>, what: &str) -> Result<Value, String> {
    match ids.as_slice() {
        [] => Ok(Value::Undefined),
        [id] => Ok(Value::Object(*id)),
        _ => Err(format!("{what} used with more than one entity")),
    }
}

fn objects(world: &mut World, ids: Vec<u64>) -> Result<Value, String> {
    new_array(world, ids.into_iter().map(Value::Object).collect())
}

fn array_values(world: &World, value: &Value) -> Result<Vec<Value>, String> {
    let Value::Array(id) = value else {
        return Err(format!("{} is not an array", kind(value)));
    };
    Ok(world
        .resource::<Runtime>()
        .arrays
        .get(id)
        .ok_or("invalid array reference")?
        .values()
        .cloned()
        .collect())
}

fn seconds_ms(seconds: f32) -> Result<i64, String> {
    if !(seconds > 0.0) {
        return Err("total time must be positive".into());
    }
    Ok((seconds * 1000.0).round().max(1.0) as i64)
}

fn start_motion(
    world: &mut World,
    receiver: &Value,
    field: &'static str,
    path: MotionPath,
    duration_ms: i64,
    done: &'static str,
) -> Result<Value, String> {
    let start_ms = now_ms(world);
    with_entity(world, receiver, |e| {
        e.motion.retain(|m| m.field != field);
        e.motion.push(Motion {
            field,
            path,
            start_ms,
            duration_ms,
            done,
        });
    })
}

fn move_accel_check(args: &[Value], time: f32) -> Result<(), String> {
    let accel = optional(args, 2, float)?.unwrap_or(0.0);
    let decel = optional(args, 3, float)?.unwrap_or(0.0);
    if accel < 0.0 || decel < 0.0 {
        return Err("accel and decel time must not be negative".into());
    }
    if accel + decel > time {
        return Err("accel time plus decel time is greater than total time".into());
    }
    Ok(())
}

fn rotate_by(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let delta = float(args, 0)?;
    let time = float(args, 1)?;
    move_accel_check(args, time)?;
    let duration = seconds_ms(time)?;
    let id = entity_id(world, receiver)?;
    let from = vector_field(world, id, "angles");
    let mut to = from;
    to[axis] += delta;
    start_motion(
        world,
        receiver,
        "angles",
        MotionPath::Linear { from, to },
        duration,
        "rotatedone",
    )
}

fn add_angle(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let delta = float(args, 0)?;
    let id = entity_id(world, receiver)?;
    let mut angles = vector_field(world, id, "angles");
    angles[axis] += delta;
    runtime(world).set_object_field(id, "angles", Value::Vector(angles));
    Ok(Value::Undefined)
}

pub(super) fn advance_motions(world: &mut World, now: i64) {
    let mut finished = Vec::new();
    let mut runtime = runtime(world);
    let moving: Vec<u64> = runtime
        .entities
        .iter()
        .filter(|(_, e)| !e.motion.is_empty())
        .map(|(id, _)| *id)
        .collect();
    for id in moving {
        let motions = std::mem::take(&mut runtime.entities.get_mut(&id).unwrap().motion);
        let mut remaining = Vec::new();
        for motion in motions {
            let elapsed = (now - motion.start_ms).clamp(0, motion.duration_ms);
            let value = match motion.path {
                MotionPath::Linear { from, to } => {
                    lerp(from, to, elapsed as f32 / motion.duration_ms as f32)
                }
                MotionPath::Ballistic { from, velocity } => {
                    let s = elapsed as f32 / 1000.0;
                    let mut p = add(from, scale(velocity, s));
                    p[2] -= 0.5 * GRAVITY * s * s;
                    p
                }
            };
            runtime.set_object_field(id, motion.field, Value::Vector(value));
            if elapsed >= motion.duration_ms {
                finished.push((id, motion.done));
            } else {
                remaining.push(motion);
            }
        }
        runtime.entities.get_mut(&id).unwrap().motion = remaining;
    }
    let linked: Vec<(u64, entities::Link)> = runtime
        .entities
        .iter()
        .filter_map(|(id, e)| Some((*id, e.linked_to.clone()?)))
        .collect();
    for (id, link) in linked {
        if !runtime.objects.contains_key(&link.parent) {
            runtime.entities.get_mut(&id).unwrap().linked_to = None;
            continue;
        }
        let field = |runtime: &mut Runtime, name| match runtime.object_field(link.parent, name) {
            Value::Vector(v) => v,
            _ => ZERO,
        };
        let (base, base_angles) = (field(&mut runtime, "origin"), field(&mut runtime, "angles"));
        let axis = math_iw4::angles_to_axis(base_angles);
        let local = add(link.tag_offset.unwrap_or(ZERO), link.origin);
        let (child_axis, origin) =
            math_iw4::matrix_multiply43(math_iw4::angles_to_axis(link.angles), local, axis, base);
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(
            id,
            "angles",
            Value::Vector(math_iw4::axis_to_angles(child_axis)),
        );
    }
    let timers = std::mem::take(&mut runtime.timers);
    let (due, pending): (Vec<_>, Vec<_>) = timers.into_iter().partition(|(at, _, _)| *at <= now);
    runtime.timers = pending;
    drop(runtime);
    for (id, name) in finished {
        raise(world, Value::Object(id), name, Vec::new());
    }
    for (_, receiver, name) in due {
        raise(world, receiver, &name, Vec::new());
    }
}

fn fx_name(world: &World, id: i32) -> Result<String, String> {
    world
        .resource::<Runtime>()
        .precached
        .iter()
        .find(|((kind, _), index)| *kind == "fx" && **index == id)
        .map(|((_, name), _)| name.clone())
        .ok_or_else(|| format!("effect id {id} was not loaded"))
}

fn world_event(
    world: &mut World,
    kind: entity_iw4::EntityEventKind,
    index: u8,
    origin: [f32; 3],
    direction: [f32; 3],
) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    crate::frame::FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number: i32::from(trace_iw4::ENTITYNUM_WORLD),
            event_parm: i32::from(index),
            origin,
            direction,
            ..Default::default()
        },
    );
}

pub(super) fn sound_to(
    world: &mut World,
    audience: crate::EventAudience,
    alias: &str,
    number: i32,
    origin: [f32; 3],
) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let index = frame.sound_alias_index(alias);
    frame.push_entity_event(
        tick,
        audience,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number,
            event_parm: i32::from(index),
            origin,
            ..Default::default()
        },
    );
}

fn team_clients(world: &mut World, team: &str, except: Option<u32>) -> Vec<crate::ClientId> {
    let team = match team {
        "axis" => entity_iw4::TEAM_AXIS,
        "allies" => entity_iw4::TEAM_ALLIES,
        "spectator" => entity_iw4::TEAM_SPECTATOR,
        _ => return Vec::new(),
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| Some(id.0) != except)
        .filter(|id| {
            frame
                .client_meta(*id)
                .is_some_and(|m| m.client_state_team == team)
        })
        .collect()
}

pub(super) fn play_sound_at(world: &mut World, origin: [f32; 3], alias: &str) {
    let index = crate::frame::FrameWorld::from_world(world).sound_alias_index(alias);
    world_event(
        world,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        index,
        origin,
        ZERO,
    );
}

fn radius_damage(
    world: &mut World,
    inflictor: Option<crate::ScriptModelId>,
    args: &[Value],
) -> Result<Value, String> {
    let origin = vector(args, 0)?;
    let radius = float(args, 1)?;
    let max = float(args, 2)?;
    let min = float(args, 3)?;
    let attacker = match args.get(4) {
        Some(Value::Object(id)) => runtime(world).player_client(*id).map(crate::ClientId),
        _ => None,
    };
    let means = optional(args, 5, string)?
        .map(|name| super::entity_damage::means_named(&name))
        .transpose()?
        .unwrap_or("MOD_EXPLOSIVE");
    let weapon = match optional(args, 6, string)? {
        Some(name) if name != "none" => {
            crate::script_player::weapon_named(&crate::frame::FrameWorld::from_world(world), &name)?
        }
        _ => 0,
    };
    runtime(world)
        .blasts
        .push(super::entity_damage::ScriptBlast {
            origin,
            radius,
            max,
            min,
            attacker,
            inflictor,
            means,
            weapon,
        });
    Ok(Value::Undefined)
}

fn part(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    hidden: bool,
) -> Result<Value, String> {
    let tag: Arc<str> = string(args, 0)?.into();
    with_entity(world, receiver, |e| e.part_ops.push((tag, hidden)))
}

fn team_key(args: &[Value]) -> Result<String, String> {
    let team = string(args, 0)?;
    match team.as_str() {
        "allies" | "axis" | "free" | "spectator" | "none" | "neutral" => Ok(team),
        other => Err(format!("'{other}' is an illegal team string")),
    }
}

fn match_data_key(prefix: &str, keys: &[Value]) -> Result<String, String> {
    let mut path = String::from(prefix);
    for (i, key) in keys.iter().enumerate() {
        path.push('.');
        path.push_str(&match key {
            Value::Int(n) => n.to_string(),
            _ => string(keys, i)?,
        });
    }
    Ok(path)
}

fn set_match_data(world: &mut World, prefix: &str, args: &[Value]) -> Result<Value, String> {
    let (value, keys) = args.split_last().ok_or("wrong number of parameters")?;
    let key = match_data_key(prefix, keys)?;
    runtime(world).engine.match_data.insert(key, value.clone());
    Ok(Value::Undefined)
}

fn get_match_data(world: &mut World, prefix: &str, args: &[Value]) -> Result<Value, String> {
    let key = match_data_key(prefix, args)?;
    Ok(runtime(world)
        .engine
        .match_data
        .get(&key)
        .cloned()
        .unwrap_or(Value::Int(0)))
}

fn weapon_facts(
    world: &mut World,
    args: &[Value],
) -> Result<weapon_iw4::WeaponCombatFacts, String> {
    let name = string(args, 0)?;
    if name == "none" || name.is_empty() {
        return Ok(Default::default());
    }
    let frame = crate::frame::FrameWorld::from_world(world);
    let index = frame
        .weapon_index_by_script_name(&name)
        .ok_or_else(|| format!("unknown weapon '{name}'"))?;
    Ok(frame.weapon_combat_row(index).unwrap_or_default())
}

const WEAPON_TYPES: &[&str] = &["bullet", "grenade", "projectile", "riotshield"];
const WEAPON_CLASSES: &[&str] = &[
    "rifle",
    "sniper",
    "mg",
    "smg",
    "spread",
    "pistol",
    "grenade",
    "rocketlauncher",
    "turret",
    "throwingknife",
    "non-player",
    "item",
];
const INVENTORY_TYPES: &[&str] = &[
    "primary",
    "offhand",
    "item",
    "altmode",
    "exclusive",
    "scavenger",
];

fn enum_name(names: &[&str], index: i32) -> Value {
    Value::string(
        names
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or(names[0]),
    )
}

fn signal(world: &mut World, name: &str) -> Result<Value, String> {
    runtime(world).signals.push(name.into());
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "getent", |world, _, args| {
        single(matching_entities(world, args)?, "getent")
    });
    registry.register(Function, "getentarray", |world, _, args| {
        let ids = if args.is_empty() {
            let runtime = world.resource::<Runtime>();
            runtime
                .entities
                .iter()
                .filter(|(_, e)| e.kind != EntityKind::HudElem)
                .map(|(id, _)| *id)
                .collect()
        } else {
            matching_entities(world, args)?
        };
        objects(world, ids)
    });
    registry.register(Function, "getvehiclenode", |world, _, args| {
        let ids = matching_entities(world, args)?;
        single(
            classname_prefix(world, ids, "info_vehicle_node"),
            "getvehiclenode",
        )
    });
    registry.register(Function, "getvehiclenodearray", |world, _, args| {
        let ids = matching_entities(world, args)?;
        let ids = classname_prefix(world, ids, "info_vehicle_node");
        objects(world, ids)
    });
    registry.register(Function, "vehicle_getspawnerarray", |world, _, _| {
        new_array(world, Vec::new())
    });
    registry.register(Function, "isspawner", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getteamplayersalive", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(super::players::alive_on_team(world, &team)))
    });
    registry.register(Function, "isplayer", |world, _, args| {
        let value = arg(args, 0)?;
        let player = matches!(value, Value::Object(id)
            if world.resource::<Runtime>().player_client(*id).is_some());
        Ok(Value::Int(player.into()))
    });
    registry.register(Function, "isalive", |world, _, args| {
        let value = arg(args, 0)?.clone();
        let Ok(id) = entity_id(world, &value) else {
            return Ok(Value::Int(0));
        };
        let health = match super::players::entity_field(world, id, "health") {
            Value::Int(n) => n as f32,
            Value::Float(n) => n,
            _ => 0.0,
        };
        Ok(Value::Int((health > 0.0).into()))
    });
    registry.register(Function, "spawn", |world, _, args| {
        let classname = string(args, 0)?;
        let origin = vector(args, 1)?;
        let flags = optional(args, 2, int)?.unwrap_or(0);
        if !matches!(
            classname.as_str(),
            "script_origin"
                | "script_model"
                | "trigger_radius"
                | "info_notnull"
                | "info_notnull_big"
        ) {
            return Err(format!("unable to spawn \"{classname}\" entity"));
        }
        let cylinder = if classname == "trigger_radius" {
            Some((float(args, 3)?, float(args, 4)?))
        } else {
            None
        };
        let presence = if classname == "script_model" {
            Some(super::presence::spawn_presence(world, origin)?)
        } else {
            None
        };
        let mut runtime = runtime(world);
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(ZERO));
        runtime.set_object_field(id, "spawnflags", Value::Int(flags));
        let entity = runtime.entities.get_mut(&id).unwrap();
        entity.cylinder = cylinder;
        entity.presence = presence;
        Ok(Value::Object(id))
    });
    registry.register(Function, "sortbydistance", |world, _, args| {
        let values = array_values(world, arg(args, 0)?)?;
        let origin = vector(args, 1)?;
        let mut keyed = Vec::with_capacity(values.len());
        for value in values {
            keyed.push((distance_sq(origin_of(world, &value)?, origin), value));
        }
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        new_array(world, keyed.into_iter().map(|(_, v)| v).collect())
    });

    registry.register(Method, "delete", |world, receiver, args| {
        if !args.is_empty() {
            return Err("delete expects no arguments".into());
        }
        if let Value::Entity(entity) = receiver {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            frame
                .entity_kernel()
                .resolve(*entity)
                .map_err(|e| format!("invalid entity receiver: {e:?}"))?;
            if !frame.remove_script_mover_by_number(entity.number()) {
                return Err("delete receiver is not a script mover".into());
            }
            return Ok(Value::Undefined);
        }
        let id = entity_id(world, receiver)?;
        let item = match runtime(world).entities.get(&id).map(|e| &e.kind) {
            Some(entities::EntityKind::Item(number)) => Some(*number),
            _ => None,
        };
        if let Some(number) = item {
            crate::frame::FrameWorld::from_world(world).remove_dropped_item_by_number(number);
        }
        runtime(world).delete_entity(id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setorigin", |world, receiver, args| {
        let origin = vector(args, 0)?;
        if let Value::Entity(entity) = receiver {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            frame
                .entity_kernel()
                .resolve(*entity)
                .map_err(|e| format!("invalid entity receiver: {e:?}"))?;
            if !frame.set_script_mover_origin(entity.number(), origin) {
                return Err("setorigin receiver is not a script mover".into());
            }
            return Ok(Value::Undefined);
        }
        let id = entity_id(world, receiver)?;
        let mut runtime = runtime(world);
        runtime
            .entities
            .get_mut(&id)
            .unwrap()
            .motion
            .retain(|m| m.field != "origin");
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        Ok(Value::Undefined)
    });
    registry.register(Method, "getorigin", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Vector(vector_field(world, id, "origin")))
    });
    registry.register(Method, "gettagorigin", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let tag = string(args, 0)?;
        let origin = vector_field(world, id, "origin");
        let offset = super::presence::tag_offset(world, id, &tag).unwrap_or(ZERO);
        let axis = math_iw4::angles_to_axis(vector_field(world, id, "angles"));
        Ok(Value::Vector(std::array::from_fn(|i| {
            origin[i] + axis[0][i] * offset[0] + axis[1][i] * offset[1] + axis[2][i] * offset[2]
        })))
    });
    for name in ["geteye", "getpointinbounds"] {
        registry.register(Method, name, |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            Ok(Value::Vector(vector_field(world, id, "origin")))
        });
    }
    registry.register(Method, "gettagangles", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Vector(vector_field(world, id, "angles")))
    });
    registry.register(Method, "getvelocity", |world, receiver, _| {
        entity_id(world, receiver)?;
        Ok(Value::Vector(ZERO))
    });
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Int(world.resource::<Runtime>().entities[&id].number))
    });
    registry.register(Method, "setmodel", |world, receiver, args| {
        let model = string(args, 0)?;
        let id = entity_id(world, receiver)?;
        runtime(world).set_object_field(id, "model", Value::string(&model));
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "clonebrushmodeltoscriptmodel",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            let source = entity_id(world, arg(args, 0)?)?;
            let mut runtime = runtime(world);
            let brush = runtime.entities[&source]
                .brush
                .ok_or("CloneBrushmodelToScriptmodel source has no brush model")?;
            let origin = match runtime.object_field(id, "origin") {
                Value::Vector(v) => v,
                _ => ZERO,
            };
            let angles = match runtime.object_field(id, "angles") {
                Value::Vector(v) => v,
                _ => ZERO,
            };
            let entity = runtime.entities.get_mut(&id).unwrap();
            entity.brush = Some(brush);
            let presence = entity.presence;
            drop(runtime);
            if let Some(presence) = presence
                && let Some(row) =
                    crate::frame::FrameWorld::from_world(world).collision_owner_mut(presence)
            {
                row.linked_brushes = vec![crate::bullet_collision::LinkedBrushCollisionBrush {
                    cmodel_handle: brush,
                    origin,
                    angles,
                }];
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "hide", |world, receiver, _| {
        with_entity(world, receiver, |e| {
            e.hidden = true;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "show", |world, receiver, _| {
        with_entity(world, receiver, |e| {
            e.hidden = false;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "showtoplayer", |world, receiver, args| {
        let player = args.first().ok_or("showToPlayer: missing player")?;
        let client = runtime(world)
            .player_client_of(player)
            .filter(|client| *client < 64)
            .ok_or("showToPlayer: parameter 1 is not a player")?;
        with_entity(world, receiver, |e| e.shown_to |= 1 << client)
    });
    registry.register(Method, "solid", |world, receiver, _| {
        with_entity(world, receiver, |e| e.solid = true)
    });
    registry.register(Method, "notsolid", |world, receiver, _| {
        with_entity(world, receiver, |e| e.solid = false)
    });
    registry.register(Method, "setcontents", |world, receiver, args| {
        let contents = int(args, 0)?;
        let mut previous = 0;
        with_entity(world, receiver, |e| {
            previous = std::mem::replace(&mut e.contents, contents);
        })?;
        Ok(Value::Int(previous))
    });
    registry.register(Method, "setlightintensity", |world, receiver, args| {
        let value = float(args, 0)?;
        with_entity(world, receiver, |e| e.light = value)
    });
    registry.register(Method, "getlightintensity", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Float(
            world.resource::<Runtime>().entities[&id].light,
        ))
    });
    registry.register(Method, "linkto", |world, receiver, args| {
        let parent = entity_id(world, arg(args, 0)?)?;
        let id = entity_id(world, receiver)?;
        if parent == id {
            return Err("cannot link an entity to itself".into());
        }
        let tag = optional(args, 1, string)?
            .filter(|tag| !tag.is_empty() && !tag.eq_ignore_ascii_case("tag_origin"))
            .map(Arc::<str>::from);
        let (origin, angles) = match optional(args, 2, vector)? {
            Some(offset) => (offset, optional(args, 3, vector)?.unwrap_or(ZERO)),
            None => {
                let parent_axis = math_iw4::angles_to_axis(vector_field(world, parent, "angles"));
                let delta = sub(
                    vector_field(world, id, "origin"),
                    vector_field(world, parent, "origin"),
                );
                let child_axis = math_iw4::angles_to_axis(vector_field(world, id, "angles"));
                (
                    std::array::from_fn(|i| dot(delta, parent_axis[i])),
                    math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                        child_axis,
                        transpose(parent_axis),
                    )),
                )
            }
        };
        let tag_offset = tag.is_none().then_some(ZERO);
        runtime(world).entities.get_mut(&id).unwrap().linked_to = Some(entities::Link {
            parent,
            origin,
            angles,
            tag,
            tag_offset,
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "unlink", |world, receiver, _| {
        if let Some(client) = runtime(world).player_client_of(receiver) {
            let mut frame = crate::frame::FrameWorld::from_world(world);
            if frame.client_meta(crate::ClientId(client)).is_some() {
                frame
                    .client_meta_mut(crate::ClientId(client))
                    .controls
                    .linked = false;
            }
        }
        with_entity(world, receiver, |e| e.linked_to = None)
    });
    registry.register(Method, "moveto", |world, receiver, args| {
        let to = vector(args, 0)?;
        let time = float(args, 1)?;
        move_accel_check(args, time)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "origin");
        start_motion(
            world,
            receiver,
            "origin",
            MotionPath::Linear { from, to },
            duration,
            "movedone",
        )
    });
    registry.register(Method, "movegravity", |world, receiver, args| {
        let velocity = vector(args, 0)?;
        let duration = seconds_ms(float(args, 1)?)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "origin");
        start_motion(
            world,
            receiver,
            "origin",
            MotionPath::Ballistic { from, velocity },
            duration,
            "movedone",
        )
    });
    registry.register(Method, "rotateto", |world, receiver, args| {
        let to = vector(args, 0)?;
        let time = float(args, 1)?;
        move_accel_check(args, time)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "angles");
        let to = std::array::from_fn(|i| {
            let delta = (to[i] - from[i]) % 360.0;
            let delta = if delta > 180.0 {
                delta - 360.0
            } else if delta < -180.0 {
                delta + 360.0
            } else {
                delta
            };
            from[i] + delta
        });
        start_motion(
            world,
            receiver,
            "angles",
            MotionPath::Linear { from, to },
            duration,
            "rotatedone",
        )
    });
    registry.register(Method, "rotatepitch", |world, receiver, args| {
        rotate_by(world, receiver, args, 0)
    });
    registry.register(Method, "rotateyaw", |world, receiver, args| {
        rotate_by(world, receiver, args, 1)
    });
    registry.register(Method, "rotateroll", |world, receiver, args| {
        rotate_by(world, receiver, args, 2)
    });
    registry.register(Method, "rotatevelocity", |world, receiver, args| {
        let velocity = vector(args, 0)?;
        let time = float(args, 1)?;
        move_accel_check(args, time)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "angles");
        let to = add(from, scale(velocity, time));
        start_motion(
            world,
            receiver,
            "angles",
            MotionPath::Linear { from, to },
            duration,
            "rotatedone",
        )
    });
    registry.register(Method, "addpitch", |world, receiver, args| {
        add_angle(world, receiver, args, 0)
    });
    registry.register(Method, "addyaw", |world, receiver, args| {
        add_angle(world, receiver, args, 1)
    });
    registry.register(Method, "addroll", |world, receiver, args| {
        add_angle(world, receiver, args, 2)
    });
    registry.register(Method, "istouching", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let other = arg(args, 0)?.clone();
        let Ok(other) = entity_id(world, &other) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(
            super::triggers::is_touching(world, id, other).into(),
        ))
    });
    registry.register(Method, "placespawnpoint", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let start = vector_field(world, id, "origin");
        let up = add(start, [0.0, 0.0, 128.0]);
        let t = trace(
            world,
            start,
            up,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        let top = lerp(start, up, t.fraction);
        let down = add(top, [0.0, 0.0, -262_144.0]);
        let t = trace(
            world,
            top,
            down,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        let ground = lerp(top, down, t.fraction);
        runtime(world).set_object_field(id, "origin", Value::Vector(ground));
        Ok(Value::Undefined)
    });
    registry.register(Method, "attach", |world, receiver, args| {
        let model: Arc<str> = string(args, 0)?.into();
        let tag: Arc<str> = optional(args, 1, string)?.unwrap_or_default().into();
        let id = entity_id(world, receiver)?;
        let mut runtime = runtime(world);
        let entity = runtime.entities.get_mut(&id).unwrap();
        if entity
            .attachments
            .iter()
            .any(|(m, t)| *m == model && *t == tag)
        {
            return Err(format!("model '{model}' already attached to tag '{tag}'"));
        }
        entity.attachments.push((model, tag));
        Ok(Value::Undefined)
    });
    registry.register(Method, "detach", |world, receiver, args| {
        let model: Arc<str> = string(args, 0)?.into();
        let tag: Option<Arc<str>> = optional(args, 1, string)?.map(Into::into);
        let id = entity_id(world, receiver)?;
        let mut runtime = runtime(world);
        let entity = runtime.entities.get_mut(&id).unwrap();
        let index = entity
            .attachments
            .iter()
            .position(|(m, t)| *m == model && tag.as_ref().is_none_or(|tag| t == tag))
            .ok_or_else(|| format!("model '{model}' is not attached"))?;
        entity.attachments.remove(index);
        Ok(Value::Undefined)
    });
    registry.register(Method, "detachall", |world, receiver, _| {
        with_entity(world, receiver, |e| e.attachments.clear())
    });
    registry.register(Method, "getattachsize", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Int(
            world.resource::<Runtime>().entities[&id].attachments.len() as i32,
        ))
    });
    registry.register(Method, "getattachmodelname", |world, receiver, args| {
        let index = int(args, 0)?;
        let id = entity_id(world, receiver)?;
        let entity = &world.resource::<Runtime>().entities[&id];
        let (model, _) = entity
            .attachments
            .get(index.max(0) as usize)
            .ok_or("bad attachment index")?;
        Ok(Value::String(model.clone()))
    });
    registry.register(Method, "getattachtagname", |world, receiver, args| {
        let index = int(args, 0)?;
        let id = entity_id(world, receiver)?;
        let entity = &world.resource::<Runtime>().entities[&id];
        let (_, tag) = entity
            .attachments
            .get(index.max(0) as usize)
            .ok_or("bad attachment index")?;
        Ok(Value::String(tag.clone()))
    });
    registry.register(Function, "playfx", |world, _, args| {
        let name = fx_name(world, int(args, 0)?)?;
        let origin = vector(args, 1)?;
        let forward = optional(args, 2, vector)?.unwrap_or([0.0, 0.0, 1.0]);
        let index = crate::frame::FrameWorld::from_world(world).effect_name_index(&name);
        world_event(
            world,
            entity_iw4::EntityEventKind::PLAY_FX,
            index,
            origin,
            forward,
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "playfxontag", |world, _, args| {
        let name = fx_name(world, int(args, 0)?)?;
        let entity = arg(args, 1)?.clone();
        let tag = string(args, 2)?;
        let presence = runtime(world).presence_of(&entity);
        let fallback = (origin_of(world, &entity)?, [0.0, 0.0, 1.0]);
        let mut frame = crate::frame::FrameWorld::from_world(world);
        let (origin, forward) = presence
            .and_then(|id| {
                frame
                    .entity_collision_capabilities()
                    .iter()
                    .find(|row| row.owner.script_model() == Some(id))?
                    .dobj
                    .as_ref()?
                    .tag_world_pose(&tag)
            })
            .unwrap_or(fallback);
        let index = frame.effect_name_index(&name);
        world_event(
            world,
            entity_iw4::EntityEventKind::PLAY_FX,
            index,
            origin,
            forward,
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "playsoundatpos", |world, _, args| {
        let origin = vector(args, 0)?;
        let alias = string(args, 1)?;
        play_sound_at(world, origin, &alias);
        Ok(Value::Undefined)
    });
    registry.register(Method, "playloopsound", |world, receiver, args| {
        let alias: Arc<str> = string(args, 0)?.into();
        with_entity(world, receiver, |e| e.loop_sound = Some(alias))
    });
    registry.register(Method, "stoploopsound", |world, receiver, _| {
        with_entity(world, receiver, |e| e.loop_sound = None)
    });
    registry.register(Method, "playsound", |world, receiver, args| {
        let alias = string(args, 0)?;
        let origin = origin_of(world, receiver)?;
        let index = crate::frame::FrameWorld::from_world(world).sound_alias_index(&alias);
        world_event(
            world,
            entity_iw4::EntityEventKind::SOUND_ALIAS,
            index,
            origin,
            ZERO,
        );
        if let Some(name) = optional(args, 1, string)? {
            let at = now_ms(world) + i64::from(crate::MATCH_TICK_MS);
            runtime(world)
                .timers
                .push((at, receiver.clone(), name.into()));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "playsoundtoplayer", |world, receiver, args| {
        let alias = string(args, 0)?;
        let client = super::natives_player::player(world, arg(args, 1)?)?;
        let origin = origin_of(world, receiver)?;
        let number = i32::from(trace_iw4::ENTITYNUM_WORLD);
        sound_to(
            world,
            crate::EventAudience::Client(crate::ClientId(client)),
            &alias,
            number,
            origin,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "playsoundtoteam", |world, receiver, args| {
        let alias = string(args, 0)?;
        let team = string(args, 1)?;
        let except = match args.get(2) {
            Some(value) if !matches!(value, Value::Undefined) => {
                Some(super::natives_player::player(world, value)?)
            }
            _ => None,
        };
        let origin = origin_of(world, receiver)?;
        let number = i32::from(trace_iw4::ENTITYNUM_WORLD);
        let clients = team_clients(world, &team, except);
        if !clients.is_empty() {
            sound_to(
                world,
                crate::EventAudience::Clients(clients),
                &alias,
                number,
                origin,
            );
        }
        Ok(Value::Undefined)
    });
    macro_rules! entity_accepts {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                if !matches!(receiver, Value::Entity(_)) {
                    entity_id(world, receiver)?;
                }
                Ok(Value::Undefined)
            });
        )*};
    }
    registry.register(Method, "setcandamage", |world, receiver, args| {
        let on = int(args, 0)? != 0;
        with_entity(world, receiver, |e| e.can_damage = on)
    });
    registry.register(Method, "setcanradiusdamage", |world, receiver, args| {
        int(args, 0)?;
        entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "hidepart", |world, receiver, args| {
        part(world, receiver, args, true)
    });
    registry.register(Method, "showpart", |world, receiver, args| {
        part(world, receiver, args, false)
    });
    registry.register(Method, "scriptmodelplayanim", |world, receiver, args| {
        let clip = match arg(args, 0)? {
            Value::Animation { name, .. } => name.clone(),
            Value::String(name) => name.clone(),
            other => return Err(format!("{} is not an animation", kind(other))),
        };
        with_entity(world, receiver, |e| e.anim_op = Some(Some(clip)))
    });
    registry.register(Method, "scriptmodelclearanim", |world, receiver, _| {
        with_entity(world, receiver, |e| e.anim_op = Some(None))
    });
    entity_accepts![
        "setteamfortrigger",
        "releaseclaimedtrigger",
        "enablegrenadetouchdamage",
        "willneverchange",
        "laseron",
        "laseroff",
        "playsoundasmaster",
        "playrumbleonentity",
        "logstring",
    ];

    super::hud::register(registry);

    registry.register(Function, "bullettrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let t = trace(world, start, end, ZERO, ZERO, MASK_SHOT);
        let hit = t.fraction < 1.0;
        let surface = if hit {
            weapon_iw4::SURFACE_TYPE_NAMES
                .get(trace_iw4::surface_type_from_flags(t.surface_flags) as usize)
                .copied()
                .unwrap_or("default")
        } else {
            "none"
        };
        keyed_array(
            world,
            vec![
                ("fraction", Value::Float(t.fraction)),
                ("position", Value::Vector(lerp(start, end, t.fraction))),
                ("normal", Value::Vector(if hit { t.normal } else { ZERO })),
                ("surfacetype", Value::string(surface)),
            ],
        )
    });
    registry.register(Function, "bullettracepassed", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Int(
            trace_passed(world, start, end, MASK_SHOT).into(),
        ))
    });
    registry.register(Function, "physicstrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let t = trace(world, start, end, ZERO, ZERO, MASK_PLAYER_SOLID);
        Ok(Value::Vector(lerp(start, end, t.fraction)))
    });
    registry.register(Function, "playerphysicstrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let t = trace(
            world,
            start,
            end,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        Ok(Value::Vector(lerp(start, end, t.fraction)))
    });
    registry.register(Function, "spawnsighttrace", |world, _, args| {
        let (start, end) = (vector(args, 1)?, vector(args, 2)?);
        Ok(Value::Int(
            trace_passed(world, start, end, MASK_SHOT).into(),
        ))
    });
    registry.register(Function, "canspawn", |world, _, args| {
        let origin = vector(args, 0)?;
        let t = trace(
            world,
            origin,
            origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        Ok(Value::Int((t.startsolid == 0 && t.allsolid == 0).into()))
    });
    registry.register(Function, "positionwouldtelefrag", |_, _, args| {
        vector(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "sightconetrace", |world, _, args| {
        let origin = vector(args, 0)?;
        let target = origin_of(world, arg(args, 1)?)?;
        let passed = trace_passed(world, origin, target, MASK_SHOT);
        Ok(Value::Float(if passed { 1.0 } else { 0.0 }))
    });
    registry.register(Method, "damageconetrace", |world, receiver, args| {
        let origin = vector(args, 0)?;
        let target = origin_of(world, receiver)?;
        let passed = trace_passed(world, origin, target, MASK_SHOT);
        Ok(Value::Float(if passed { 1.0 } else { 0.0 }))
    });

    registry.register(Function, "setteamscore", |world, _, args| {
        let team = team_key(args)?;
        let score = int(args, 1)?;
        runtime(world).engine.team_scores.insert(team, score);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamscore", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(
            runtime(world)
                .engine
                .team_scores
                .get(&team)
                .copied()
                .unwrap_or(0),
        ))
    });
    registry.register(Function, "setteamradar", |world, _, args| {
        let team = team_key(args)?;
        let on = int(args, 1)?;
        runtime(world).engine.team_radar.insert(team, on);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamradar", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(
            runtime(world)
                .engine
                .team_radar
                .get(&team)
                .copied()
                .unwrap_or(0),
        ))
    });
    registry.register(Function, "blockteamradar", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.team_radar_blocked.insert(team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "unblockteamradar", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.team_radar_blocked.remove(&team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setgameendtime", |world, _, args| {
        let time = int(args, 0)?;
        runtime(world).engine.game_end_time = time;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setmapcenter", |world, _, args| {
        let center = vector(args, 0)?;
        runtime(world).engine.map_center = center;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setwinningteam", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.winning_team = Some(team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getnorthyaw", |world, _, _| {
        let yaw = runtime(world)
            .engine
            .worldspawn
            .get("northyaw")
            .map_or(0.0, |v| iw4_natives::atof(v) as f32);
        Ok(Value::Float(yaw))
    });
    for name in ["logprint", "logstring"] {
        registry.register(Function, name, |_, _, args| {
            diag::info!(Sim, "gsc log: {}", string(args, 0)?.trim_end());
            Ok(Value::Undefined)
        });
    }
    registry.register(Function, "exitlevel", |world, _, _| {
        runtime(world).exit_level = true;
        signal(world, EXIT_LEVEL)
    });
    registry.register(Function, "map_restart", |world, _, _| {
        signal(world, MAP_RESTART)
    });
    registry.register(Function, "setmatchdata", |world, _, args| {
        set_match_data(world, "match", args)
    });
    registry.register(Function, "getmatchdata", |world, _, args| {
        get_match_data(world, "match", args)
    });
    registry.register(Function, "setclientmatchdata", |world, _, args| {
        set_match_data(world, "client", args)
    });
    registry.register(Function, "getclientmatchdata", |world, _, args| {
        get_match_data(world, "client", args)
    });
    registry.register(Function, "soundexists", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Int(1))
    });

    for name in ["precacheturret", "precachevehicle", "precachefxteamthermal"] {
        registry.register(Function, name, |world, _, args| {
            precache(world, "asset", string(args, 0)?).map(Value::Int)
        });
    }
    fn fx_entity(world: &mut World, args: &[Value], origin_at: usize) -> Result<Value, String> {
        int(args, 0)?;
        let origin = vector(args, origin_at)?;
        let mut runtime = runtime(world);
        let id = runtime.create_entity(EntityKind::Spawned, "script_model")?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        Ok(Value::Object(id))
    }
    registry.register(Function, "spawnfx", |world, _, args| {
        fx_entity(world, args, 1)
    });
    registry.register(Function, "playloopedfx", |world, _, args| {
        fx_entity(world, args, 2)
    });
    registry.register(Method, "usetriggerrequirelookat", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        runtime(world).require_look_at.insert(id);
        Ok(Value::Undefined)
    });
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                runtime(world).presented.insert($name, args.to_vec());
                Ok(Value::Undefined)
            });
        )*};
    }
    presented![
        "obituary",
        "earthquake",
        "playfxontagforclients",
        "stopfxontag",
        "triggerfx",
        "playrumbleonposition",
        "setslowmotion",
        "setac130ambience",
        "physicsexplosionsphere",
        "setminimap",
        "setclientnamemode",
    ];
    macro_rules! platform {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, _| Ok(Value::Undefined));
        )*};
    }
    platform![
        "endlobby",
        "endparty",
        "sendranks",
        "setplayerteamrank",
        "updateskill",
        "sendmatchdata",
        "sendclientmatchdata",
        "setmatchdatadef",
        "setclientmatchdatadef",
    ];

    registry.register(Function, "weaponclass", |world, _, args| {
        Ok(enum_name(
            WEAPON_CLASSES,
            weapon_facts(world, args)?.weap_class,
        ))
    });
    registry.register(Function, "weapontype", |world, _, args| {
        Ok(enum_name(
            WEAPON_TYPES,
            weapon_facts(world, args)?.weap_type,
        ))
    });
    registry.register(Function, "weaponinventorytype", |world, _, args| {
        Ok(enum_name(
            INVENTORY_TYPES,
            weapon_facts(world, args)?.inventory_type,
        ))
    });
    registry.register(Function, "weaponclipsize", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.clip_size))
    });
    registry.register(Function, "weaponmaxammo", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.max_ammo))
    });
    registry.register(Function, "weaponfiretime", |world, _, args| {
        Ok(Value::Float(
            weapon_facts(world, args)?.fire_time_ms as f32 / 1000.0,
        ))
    });
    registry.register(Function, "weaponinheritsperks", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.inherits_perks.into()))
    });
    registry.register(Function, "weaponaltweaponname", |world, _, args| {
        let alternate = weapon_facts(world, args)?.alternate_weapon;
        if alternate == 0 {
            return Ok(Value::string("none"));
        }
        let frame = crate::frame::FrameWorld::from_world(world);
        Ok(Value::string(frame.weapon_script_name(alternate)))
    });

    macro_rules! refused {
        ($namespace:ident: $($name:literal),* => $message:literal) => {$(
            registry.register($namespace, $name, |_, _, _| Err($message.into()));
        )*};
    }
    refused!(Function: "getanimlength", "animhasnotetrack", "getnotetracktimes"
        => "animation data is not loaded in the simulation");
    refused!(Function: "getweaponmodel", "getweaponhidetags"
        => "weapon models are not loaded in the simulation");
    refused!(Function: "spawnturret" => "turrets are not simulated");
    registry.register(Function, "radiusdamage", |world, _, args| {
        radius_damage(world, None, args)
    });
    registry.register(Method, "radiusdamage", |world, receiver, args| {
        let inflictor = runtime(world).presence_of(receiver);
        radius_damage(world, inflictor, args)
    });
    refused!(Function: "glassradiusdamage", "missile_createattractorent"
        => "script-driven damage is not simulated");
    refused!(Function: "kick" => "no client with that number");
    refused!(Method: "missile_setflightmodedirect", "missile_settargetent",
        "missile_settargetpos"
        => "receiver is not a missile");
    refused!(Method: "getcorpseanim", "startragdoll", "isragdoll" => "receiver is not a corpse");
    refused!(Method: "itemweaponsetammo" => "receiver is not a weapon item");
    refused!(Method: "setmode", "maketurretinoperable", "maketurretsolid", "setturretminimapvisible",
        "setturretmodechangewait", "setturretteam", "setsentrycarried", "setsentryowner",
        "getturrettarget", "shootturret", "settargetentity", "cleartargetentity"
        => "receiver is not a turret");
    refused!(Method: "attachpath", "startpath", "vehicle_dospawn", "vehicleturretcontroloff",
        "vehicleturretcontrolon", "vehicle_canturrettargetpoint", "fireweapon",
        "setturrettargetent", "setdefaultdroppitch"
        => "receiver is not a vehicle");
    refused!(Method: "allowjump", "allowspectateteam", "anyammoforweaponmodes", "attachshieldmodel",
        "attackbuttonpressed", "beginlocationselection", "buttonpressed", "cameralinkto",
        "cameraunlink", "canplayerplacesentry", "clearperks", "clientclaimtrigger",
        "clientreleasetrigger", "cloneplayer", "closeingamemenu", "closemenu", "closepopupmenu",
        "controlslinkto", "controlsunlink", "detachshieldmodel", "disableoffhandweapons",
        "disableusability", "disableweapons", "disableweaponswitch", "dropitem", "dropscavengerbag",
        "enableoffhandweapons", "enableusability", "enableweapons", "enableweaponswitch",
        "endlocationselection", "finishplayerdamage", "forceusehintoff", "forceusehinton",
        "fragbuttonpressed", "freezecontrols", "getcurrentprimaryweapon", "getcurrentweapon",
        "getguid", "getoffhandsecondaryclass", "getplayerangles", "getplayerdata", "getrestedtime",
        "getspectatingplayer", "getstance", "getthirdpersoncrosshairoffset", "getweaponammoclip",
        "getweaponammostock", "getweaponslistall", "getweaponslistexclusives",
        "getweaponslistitems", "getweaponslistoffhands", "getweaponslistprimaries", "getxuid",
        "givemaxammo", "givestartammo", "giveweapon", "hasperk", "hasweapon",
        "isfiringturret", "ishost", "isitemunlocked", "ismantling", "isonground",
        "isonladder", "isusingonlinedataoffline", "isusingturret", "kc_regweaponforfxremoval",
        "laststandrevive", "meleebuttonpressed", "moveshieldmodel", "notifyonplayercommand",
        "openmenu", "openpopupmenu", "pingplayer", "player_recoilscaleoff", "player_recoilscaleon",
        "playerads", "playerforcedeathanim", "playerhide", "playerlinkedoffsetenable",
        "playerlinkto", "playerlinkweaponviewtodelta", "playlocalsound", "predictstreampos",
        "radarjamoff", "radarjamon", "remotecamerasoundscapeoff", "resetspreadoverride", "sayall",
        "sayteam", "secondaryoffhandbuttonpressed", "setactionslot", "setblurforplayer",
        "setcarddisplayslot", "setcardicon", "setcardnameplate", "setcardtitle", "setclientdvar",
        "setclientdvars", "setdepthoffield", "setempjammed", "setmovespeedscale", "setnormalhealth",
        "setoffhandprimaryclass", "setoffhandsecondaryclass", "setperk", "setplayerangles",
        "setplayerdata", "setrank", "setrearviewrenderenabled", "setspawnweapon",
        "setspectatedefaults", "setspreadoverride", "setstance", "setviewmodel",
        "setweaponammoclip", "setweaponammostock", "setweaponhudiconoverride", "shellshock",
        "showhudsplash", "spawn", "startac130", "stopac130", "stoplocalsound", "stoprumble",
        "stopshellshock", "stunplayer", "suicide", "switchtoweapon", "takeallweapons",
        "takeweapon", "thermalvisionfofoverlayoff", "thermalvisionfofoverlayon",
        "thermalvisionoff", "thermalvisionon", "unsetperk", "updatedmscores", "updatescores",
        "usebuttonpressed", "viewkick", "visionsetmissilecamforplayer", "visionsetnakedforplayer",
        "visionsetthermalforplayer", "weaponlockfinalize", "weaponlockfree",
        "weaponlocknoclearance", "weaponlockstart", "weaponlocktargettooclose",
        "worldpointinreticle_circle"
        => "receiver is not a player");
}

pub const EXIT_LEVEL: &str = "engine:exitlevel";
pub const MAP_RESTART: &str = "engine:map_restart";
