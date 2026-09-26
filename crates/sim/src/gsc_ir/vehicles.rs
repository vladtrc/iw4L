use super::entities::EntityKind;
use super::iw4_natives::string;
use super::natives_math::{arg, float, int, optional, vector};
use super::runtime::{raise, run_now};
use super::*;
use bevy_ecs::prelude::World;
use natives::Namespace::{Function, Method};

pub(super) const DAMAGE: &str = "maps/mp/gametypes/_callbacksetup::codecallback_vehicledamage";

const MPH: f32 = 17.6; // miles per hour, not units per second
const TICK_S: f32 = crate::MATCH_TICK_MS as f32 / 1000.0;
const ARRIVED: f32 = 4.0;

#[derive(Clone, Debug)]
pub(crate) struct Heli {
    goal: Option<[f32; 3]>,
    stop_at_goal: bool,
    arrived: bool,
    near_goal: f32,
    near_notified: bool,
    speed: f32,
    max_speed: f32,
    accel: f32,
    decel: f32,
    heading: [f32; 3],
    yaw_speed: f32,
    target_yaw: Option<f32>,
    goal_yaw: Option<f32>,
    look_at: Option<u64>,
    max_pitch: f32,
    max_roll: f32,
}

impl Default for Heli {
    fn default() -> Self {
        Self {
            goal: None,
            stop_at_goal: true,
            arrived: false,
            near_goal: 0.0,
            near_notified: false,
            speed: 0.0,
            max_speed: 0.0,
            accel: 0.0,
            decel: 0.0,
            heading: [1.0, 0.0, 0.0],
            yaw_speed: 90.0,
            target_yaw: None,
            goal_yaw: None,
            look_at: None,
            max_pitch: 20.0,
            max_roll: 30.0,
        }
    }
}

fn heli<'a>(world: &'a mut World, receiver: &Value) -> Result<&'a mut Heli, String> {
    let Value::Object(id) = receiver else {
        return Err("receiver is not a vehicle".into());
    };
    world
        .resource_mut::<Runtime>()
        .into_inner()
        .vehicles
        .get_mut(id)
        .ok_or_else(|| "receiver is not a vehicle".into())
}

fn vec_field(runtime: &mut Runtime, object: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(object, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn spawn_vehicle(
    world: &mut World,
    classname: &str,
    origin: [f32; 3],
    angles: [f32; 3],
    model: &str,
    flight: Option<Heli>,
) -> Result<Value, String> {
    let presence = super::presence::spawn_presence(world, origin)?;
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.create_entity(EntityKind::Vehicle, classname)?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector(angles));
    runtime.set_object_field(id, "model", Value::string(model));
    runtime.entities.get_mut(&id).unwrap().presence = Some(presence);
    if let Some(mut flight) = flight {
        flight.heading = math_iw4::angle_vectors(angles).0;
        runtime.set_object_field(id, "veh_speed", Value::Float(0.0));
        runtime.vehicles.insert(id, flight);
    }
    Ok(Value::Object(id))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "spawnhelicopter", |world, _, args| {
        world
            .resource::<Runtime>()
            .player_client_of(arg(args, 0)?)
            .ok_or("spawnHelicopter owner is not a player")?;
        let (origin, angles) = (vector(args, 1)?, vector(args, 2)?);
        string(args, 3)?;
        let model = string(args, 4)?;
        spawn_vehicle(
            world,
            "script_vehicle",
            origin,
            angles,
            &model,
            Some(Heli::default()),
        )
    });
    registry.register(Function, "spawnplane", |world, _, args| {
        world
            .resource::<Runtime>()
            .player_client_of(arg(args, 0)?)
            .ok_or("spawnPlane owner is not a player")?;
        let classname = string(args, 1)?;
        let origin = vector(args, 2)?;
        spawn_vehicle(world, &classname, origin, [0.0; 3], "", None)
    });
    registry.register(Method, "setvehgoalpos", |world, receiver, args| {
        let goal = vector(args, 0)?;
        let stop = optional(args, 1, int)?.unwrap_or(0) != 0;
        let heli = heli(world, receiver)?;
        heli.goal = Some(goal);
        heli.stop_at_goal = stop;
        heli.arrived = false;
        heli.near_notified = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "vehicle_setspeed", |world, receiver, args| {
        let speed = float(args, 0)?.max(0.0) * MPH;
        let accel = optional(args, 1, float)?.unwrap_or(speed / MPH).max(1.0) * MPH;
        let decel = optional(args, 2, float)?.map_or(accel, |d| d.max(1.0) * MPH);
        let heli = heli(world, receiver)?;
        heli.max_speed = speed;
        heli.accel = accel;
        heli.decel = decel;
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "vehicle_setspeedimmediate",
        |world, receiver, args| {
            let speed = float(args, 0)?.max(0.0) * MPH;
            let heli = heli(world, receiver)?;
            heli.max_speed = speed;
            heli.speed = speed;
            heli.accel = heli.accel.max(speed);
            heli.decel = heli.decel.max(speed);
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "vehicle_getspeed", |world, receiver, _| {
        Ok(Value::Float(heli(world, receiver)?.speed / MPH))
    });
    registry.register(Method, "setyawspeed", |world, receiver, args| {
        let speed = float(args, 0)?.max(0.0);
        heli(world, receiver)?.yaw_speed = speed;
        Ok(Value::Undefined)
    });
    registry.register(Method, "settargetyaw", |world, receiver, args| {
        let yaw = float(args, 0)?;
        heli(world, receiver)?.target_yaw = Some(yaw);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleartargetyaw", |world, receiver, _| {
        heli(world, receiver)?.target_yaw = None;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setgoalyaw", |world, receiver, args| {
        let yaw = float(args, 0)?;
        heli(world, receiver)?.goal_yaw = Some(yaw);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleargoalyaw", |world, receiver, _| {
        heli(world, receiver)?.goal_yaw = None;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setmaxpitchroll", |world, receiver, args| {
        let (pitch, roll) = (float(args, 0)?.abs(), float(args, 1)?.abs());
        let heli = heli(world, receiver)?;
        heli.max_pitch = pitch;
        heli.max_roll = roll;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setneargoalnotifydist", |world, receiver, args| {
        let dist = float(args, 0)?.max(0.0);
        heli(world, receiver)?.near_goal = dist;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setlookatent", |world, receiver, args| {
        let target = super::natives_engine::entity_id(world, arg(args, 0)?)?;
        heli(world, receiver)?.look_at = Some(target);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearlookatent", |world, receiver, _| {
        heli(world, receiver)?.look_at = None;
        Ok(Value::Undefined)
    });
    for name in [
        "sethoverparams",
        "setturningability",
        "setdamagestage",
        "setvehweapon",
    ] {
        registry.register(Method, name, |world, receiver, _| {
            heli(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "vehicle_finishdamage", |world, receiver, args| {
        let Some((object, _)) = world.resource::<Runtime>().entity(receiver) else {
            return Err("receiver is not a vehicle".into());
        };
        let attacker = arg(args, 1)?.clone();
        let amount = int(args, 2)?;
        let flags = optional(args, 3, int)?.unwrap_or(0);
        let means = string(args, 4)?;
        let weapon = string(args, 5)?;
        let point = vector(args, 6)?;
        let dir = vector(args, 7)?;
        let part = optional(args, 11, string)?.unwrap_or_default();
        let mut runtime = world.resource_mut::<Runtime>();
        let before = match runtime.object_field(object, "health") {
            Value::Int(health) => health,
            Value::Float(health) => health as i32,
            _ => 0,
        };
        let after = before.saturating_sub(amount);
        runtime.set_object_field(object, "health", Value::Int(after));
        let model = runtime.object_field(object, "model");
        drop(runtime);
        raise(
            world,
            receiver.clone(),
            "damage",
            vec![
                Value::Int(amount),
                attacker.clone(),
                Value::Vector(dir),
                Value::Vector(point),
                Value::string(&means),
                model,
                Value::string(""),
                Value::string(&part),
                Value::Int(flags),
                Value::string(&weapon),
            ],
        );
        if before > 0 && after <= 0 {
            raise(world, receiver.clone(), "death", vec![attacker]);
        }
        Ok(Value::Undefined)
    });
}

pub(super) fn damage(
    world: &mut World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
    attacker: Value,
    weapon: &str,
    part: &str,
) {
    let args = vec![
        Value::Undefined,
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::string(weapon),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string("none"),
        Value::Int(0),
        Value::Int(0),
        Value::string(part),
    ];
    let now = super::players::now_ms(world);
    let _ = run_now(world, DAMAGE, Value::Object(object), args, now);
}

fn approach_angle(from: f32, to: f32, step: f32) -> f32 {
    let delta = math_iw4::angle_subtract(to, from);
    if delta.abs() <= step {
        to
    } else {
        from + step.copysign(delta)
    }
}

pub(super) fn advance(world: &mut World) {
    let ids: Vec<u64> = world
        .resource::<Runtime>()
        .vehicles
        .keys()
        .copied()
        .collect();
    for id in ids {
        let mut runtime = world.resource_mut::<Runtime>();
        if !runtime.entities.contains_key(&id) {
            runtime.vehicles.remove(&id);
            continue;
        }
        let origin = vec_field(&mut runtime, id, "origin");
        let angles = vec_field(&mut runtime, id, "angles");
        let look_at = runtime.vehicles[&id]
            .look_at
            .filter(|target| runtime.entities.contains_key(target));
        let look_at = look_at.map(|target| vec_field(&mut runtime, target, "origin"));
        let heli = runtime.vehicles.get_mut(&id).unwrap();
        let before = heli.speed;
        let mut notes = Vec::new();
        let mut next;
        match heli.goal.filter(|_| !(heli.arrived && heli.stop_at_goal)) {
            Some(goal) => {
                let to: [f32; 3] = std::array::from_fn(|i| goal[i] - origin[i]);
                let dist = to.iter().map(|v| v * v).sum::<f32>().sqrt();
                let wanted = if heli.stop_at_goal {
                    heli.max_speed.min((2.0 * heli.decel * dist).sqrt())
                } else {
                    heli.max_speed
                };
                heli.speed = if wanted > heli.speed {
                    (heli.speed + heli.accel * TICK_S).min(wanted)
                } else {
                    (heli.speed - heli.decel * TICK_S).max(wanted)
                };
                if dist > f32::EPSILON {
                    heli.heading = to.map(|v| v / dist);
                }
                let step = (heli.speed * TICK_S).min(dist);
                next = std::array::from_fn(|i| origin[i] + heli.heading[i] * step);
                let left = dist - step;
                if heli.near_goal > 0.0 && !heli.near_notified && left <= heli.near_goal {
                    heli.near_notified = true;
                    notes.push("near_goal");
                }
                if !heli.arrived && left <= ARRIVED.max(heli.speed * TICK_S) {
                    heli.arrived = true;
                    if heli.stop_at_goal {
                        next = goal;
                        heli.speed = 0.0;
                    }
                    notes.push("goal");
                }
            }
            None => {
                heli.speed = (heli.speed - heli.decel * TICK_S).max(0.0);
                next = std::array::from_fn(|i| origin[i] + heli.heading[i] * heli.speed * TICK_S);
            }
        }
        let desired_yaw = heli
            .target_yaw
            .or(heli.arrived.then_some(heli.goal_yaw).flatten())
            .or(look_at
                .map(|at| math_iw4::vect_to_angles([at[0] - next[0], at[1] - next[1], 0.0])[1]))
            .or(
                (heli.speed > 1.0 && heli.heading[..2].iter().any(|v| v.abs() > 0.01))
                    .then(|| math_iw4::vect_to_angles(heli.heading)[1]),
            )
            .unwrap_or(angles[1]);
        let yaw = approach_angle(angles[1], desired_yaw, heli.yaw_speed * TICK_S);
        let turn = math_iw4::angle_subtract(yaw, angles[1]) / TICK_S;
        let accel = (heli.speed - before) / TICK_S;
        let pitch = (accel / MPH)
            .clamp(-heli.max_pitch, heli.max_pitch)
            .clamp(-25.0, 25.0);
        let roll = (turn * 0.25)
            .clamp(-heli.max_roll, heli.max_roll)
            .clamp(-35.0, 35.0);
        let speed = heli.speed;
        runtime.set_object_field(id, "origin", Value::Vector(next));
        runtime.set_object_field(id, "angles", Value::Vector([pitch, yaw, roll]));
        runtime.set_object_field(id, "veh_speed", Value::Float(speed / MPH));
        drop(runtime);
        for note in notes {
            raise(world, Value::Object(id), note, Vec::new());
        }
    }
}
