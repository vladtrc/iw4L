use super::*;
use glam::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PathMode {
    #[default]
    Constrained,
    Follow,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Body {
    wrecked: bool,
    crashing_disabled: bool,
    conveyor: Vec3,
    previous_relative_velocity: Vec3,
}

pub(super) struct Motion {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub velocity: [f32; 3],
    pub notes: Vec<(&'static str, Vec<Value>)>,
}

fn physics(heli: &Heli) -> Result<asset_core::VehiclePhysics, String> {
    heli.physics
        .ok_or("This vehicle is not a physics vehicle.")?;
    heli.motion
        .and_then(|motion| motion.physics)
        .ok_or("vehicle physical inputs are unavailable".into())
}

fn finite(value: f32) -> Result<f32, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err("vehicle physics requires finite inputs".into())
    }
}

fn vec(args: &[Value], at: usize) -> Result<Vec3, String> {
    let v = Vec3::from_array(vector(args, at)?);
    if v.is_finite() {
        Ok(v)
    } else {
        Err("vehicle physics requires finite vectors".into())
    }
}

fn bounds(world: &mut World, id: u64) -> Result<(Vec3, Vec3), String> {
    super::super::presence::settle_collision(world);
    let model = world
        .resource_mut::<RoundScript>()
        .object_field(id, "model");
    if let Value::String(name) = &model {
        let retained = crate::frame::FrameWorld::from_world(world)
            .model_capability(name)
            .flatten();
        if let Some((mid, half)) = retained.and_then(|model| model.physics_bounds.or(model.bounds))
        {
            let mid = Vec3::from_array(mid);
            let half = Vec3::from_array(half);
            if mid.is_finite() && half.is_finite() && half.min_element() > 0.0 {
                return Ok((mid, half));
            }
        }
    }
    let (mins, maxs) = super::super::triggers::entity_bounds(world, id);
    let mins = Vec3::from_array(mins);
    let maxs = Vec3::from_array(maxs);
    if !mins.is_finite() || !maxs.is_finite() || (maxs - mins).min_element() <= 0.0 {
        return Err(format!(
            "physical vehicle model {model:?} requires model collision bounds"
        ));
    }
    Ok(((mins + maxs) * 0.5, (maxs - mins) * 0.5))
}

pub(super) fn world_to_body(velocity: [f32; 3], angles: [f32; 3]) -> [f32; 3] {
    let velocity = Vec3::from_array(velocity);
    math_iw4::angles_to_axis(angles).map(|axis| velocity.dot(Vec3::from_array(axis)))
}

pub(super) fn local_velocity(heli: &Heli, angles: [f32; 3]) -> [f32; 3] {
    let conveyor = heli.physics.map_or(Vec3::ZERO, |body| body.conveyor);
    world_to_body(
        (Vec3::from_array(heli.velocity) - conveyor).to_array(),
        angles,
    )
}

fn reported_speed(heli: &Heli, angles: [f32; 3]) -> f32 {
    let speed = local_velocity(heli, angles)[0].abs();
    // Script speed reporting has a 0.1 mph deadband; transverse/current motion is separate.
    if speed < 0.1 * MPH { 0.0 } else { speed }
}

fn mass(facts: asset_core::VehiclePhysics, body: Body) -> f32 {
    facts.mass
        * if body.wrecked {
            facts.wrecked_mass_scale
        } else {
            1.0
        }
}

fn wreck(heli: &mut Heli) {
    let body = heli.physics.as_mut().unwrap();
    body.wrecked = true;
    body.crashing_disabled = false;
    heli.goal = None;
    heli.path_running = false;
    heli.arrived = false;
}

fn impulse_motion(
    velocity: Vec3,
    angular: [f32; 3],
    angles: [f32; 3],
    half: Vec3,
    arm: Vec3,
    push: Vec3,
    mass: f32,
) -> Result<(Vec3, [f32; 3]), String> {
    let velocity = velocity + push / mass;
    let torque = arm.cross(push);
    let axis = math_iw4::angles_to_axis(angles).map(Vec3::from_array);
    // Independent box inertia from the actual model bounds, in the same mass/length units.
    let inertia = Vec3::new(
        half.y * half.y + half.z * half.z,
        half.x * half.x + half.z * half.z,
        half.x * half.x + half.y * half.y,
    ) * (mass / 3.0);
    let local = Vec3::new(
        torque.dot(axis[0]),
        torque.dot(axis[1]),
        torque.dot(axis[2]),
    ) / inertia;
    let angular = Vec3::from_array(angular)
        + Vec3::new(local.y, local.z, local.x) * (180.0 / std::f32::consts::PI);
    if !velocity.is_finite() || !angular.is_finite() {
        return Err("vehicle impulse exceeds finite motion range".into());
    }
    Ok((velocity, angular.to_array()))
}

fn impulse(
    heli: &mut Heli,
    angles: [f32; 3],
    half: Vec3,
    arm: Vec3,
    push: Vec3,
) -> Result<(), String> {
    let facts = physics(heli)?;
    let (velocity, angular) = impulse_motion(
        Vec3::from_array(heli.velocity),
        heli.angle_vel,
        angles,
        half,
        arm,
        push,
        mass(facts, heli.physics.unwrap()),
    )?;
    heli.velocity = velocity.to_array();
    heli.angle_vel = angular;
    heli.speed = reported_speed(heli, angles);
    Ok(())
}

fn crash_push(velocity: Vec3, facts: asset_core::VehiclePhysics, body: Body) -> Vec3 {
    let direction = velocity.with_z(0.0).normalize_or_zero() + Vec3::Z;
    direction * velocity.length() * mass(facts, body) * facts.collision_launch_scale
}

fn set_crashing(world: &mut World, receiver: &Value, disabled: bool) -> Result<Value, String> {
    let h = heli(world, receiver)?;
    physics(h)?;
    let body = h.physics.as_mut().unwrap();
    if disabled && body.wrecked {
        return Err("This vehicle at is already wrecked.".into());
    }
    body.crashing_disabled = disabled;
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "vehphys_disablecrashing", |world, receiver, _| {
        set_crashing(world, receiver, true)
    });
    registry.register(Method, "vehphys_enablecrashing", |world, receiver, _| {
        set_crashing(world, receiver, false)
    });
    registry.register(Method, "vehphys_setspeed", |world, receiver, args| {
        let speed = finite(float(args, 0)? * MPH)?;
        if speed < 0.0 {
            return Err("Cannot set negative speed on vehicle".into());
        }
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        let angles = vec_field(&mut world.resource_mut::<RoundScript>(), id, "angles");
        let forward = Vec3::from_array(math_iw4::angle_vectors(angles).0);
        let h = heli(world, receiver)?;
        physics(h)?;
        let v = Vec3::from_array(h.velocity);
        h.velocity = (v + forward * (speed - v.dot(forward))).to_array();
        h.speed = reported_speed(h, angles);
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "vehphys_setconveyorbelt",
        |world, receiver, args| {
            let yaw = finite(float(args, 0)?)?.to_radians();
            let speed = finite(float(args, 1)? * MPH)?;
            if speed < 0.0 {
                return Err("Cannot set negative speed on vehicle".into());
            }
            let h = heli(world, receiver)?;
            physics(h)?;
            let body = h.physics.as_mut().unwrap();
            // Retain the previous relative motion so the next tick observes the change in flow.
            let relative = Vec3::from_array(h.velocity) - body.conveyor;
            body.conveyor = Vec3::new(yaw.cos() * speed, yaw.sin() * speed, 0.0);
            body.previous_relative_velocity = relative;
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "vehphys_crash", |world, receiver, args| {
        let kick = if args.is_empty() {
            false
        } else {
            int(args, 0)? != 0
        };
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        physics(heli(world, receiver)?)?;
        let (_, half) = bounds(world, id)?;
        let angles = vec_field(&mut world.resource_mut::<RoundScript>(), id, "angles");
        let h = heli(world, receiver)?;
        wreck(h);
        if kick {
            let facts = physics(h)?;
            let v = Vec3::from_array(h.velocity);
            let push = crash_push(v, facts, h.physics.unwrap());
            impulse(
                h,
                angles,
                half,
                Vec3::Z * half.z,
                push * facts.impulse_scale,
            )?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "vehphys_launch", |world, receiver, args| {
        let push = vec(args, 0)? * MPH;
        if !push.is_finite() || !push.length().is_finite() {
            return Err("vehicle impulse exceeds finite range".into());
        }
        let id = super::super::natives::engine::entity_id(world, receiver)?;
        physics(heli(world, receiver)?)?;
        let (center, half) = bounds(world, id)?;
        let (origin, angles) = {
            let mut r = world.resource_mut::<RoundScript>();
            (
                vec_field(&mut r, id, "origin"),
                vec_field(&mut r, id, "angles"),
            )
        };
        let axis = math_iw4::angles_to_axis(angles).map(Vec3::from_array);
        let com =
            Vec3::from_array(origin) + axis[0] * center.x + axis[1] * center.y + axis[2] * center.z;
        let at_point = args.len() > 2;
        let arm = if at_point {
            vec(args, 2)? - com
        } else {
            let radius = finite(if args.len() < 2 { 0.0 } else { float(args, 1)? })?;
            if radius == 0.0 {
                Vec3::ZERO
            } else {
                let unit = Vec3::from_array(std::array::from_fn(|_| {
                    let n = super::super::natives::math::random(world) >> 8;
                    n as f32 / ((1u32 << 24) - 1) as f32 * 2.0 - 1.0
                }))
                .normalize_or_zero();
                unit * radius
            }
        };
        let h = heli(world, receiver)?;
        if at_point {
            let facts = physics(h)?;
            let scale = if h.physics.unwrap().wrecked {
                facts.wrecked_mass_scale
            } else {
                1.0
            };
            impulse(h, angles, half, arm, push * scale)?;
        } else {
            wreck(h);
            let facts = physics(h)?;
            let axis = math_iw4::angles_to_axis(angles).map(Vec3::from_array);
            let radians = Vec3::from_array(h.angle_vel) * (std::f32::consts::PI / 180.0);
            let omega = axis[0] * radians.z + axis[1] * radians.x + axis[2] * radians.y;
            let point_velocity = Vec3::from_array(h.velocity) + omega.cross(arm);
            let direction = push.normalize_or_zero();
            let deficit = (push.length() - point_velocity.dot(direction)).max(0.0);
            impulse(
                h,
                angles,
                half,
                arm,
                direction * deficit * facts.impulse_scale,
            )?;
        }
        Ok(Value::Undefined)
    });
}

pub(in crate::script::host) fn load_field(
    runtime: &RoundScript,
    id: u64,
    name: &str,
) -> Option<Value> {
    if name != "veh_pathtype" {
        return None;
    }
    let h = runtime.vehicles.get(&id)?;
    Some({
        Value::string(match h.path_mode {
            PathMode::Constrained => "constrained",
            PathMode::Follow => "follow",
        })
    })
}

pub(in crate::script::host) fn store_field(
    runtime: &mut RoundScript,
    id: u64,
    name: &str,
    value: &Value,
) -> Result<bool, String> {
    if name != "veh_pathtype" {
        return Ok(false);
    }
    let Some(h) = runtime.vehicles.get_mut(&id) else {
        return Ok(false);
    };
    let mode = match value {
        Value::String(s) if &**s == "constrained" => PathMode::Constrained,
        Value::String(s) if &**s == "follow" => {
            physics(h)?;
            PathMode::Follow
        }
        _ => return Err("illegal vehicle path type: expected constrained or follow".into()),
    };
    h.path_mode = mode;
    Ok(true)
}

pub(in crate::script::host) fn check_array_write(
    runtime: &RoundScript,
    id: u64,
    name: &str,
) -> Result<(), String> {
    if name == "veh_pathtype" && runtime.vehicles.contains_key(&id) {
        Err("native vehicle path type cannot be an array".into())
    } else {
        Ok(())
    }
}

pub(super) fn teleport(h: &mut Heli) {
    if let Some(body) = &mut h.physics {
        body.previous_relative_velocity = -body.conveyor;
        h.angle_vel = [0.0; 3];
    }
}

pub(super) fn advance(world: &mut World, id: u64) -> Result<Motion, String> {
    let _span = perf::span_handle!("vehicle_physics", "iw4l.sim").enter();
    let (center, half) = bounds(world, id)?;
    let (origin, angles, presence, look_at) = {
        let mut r = world.resource_mut::<RoundScript>();
        let target = r.vehicles[&id]
            .look_at
            .filter(|target| r.entities.contains_key(target));
        let look_at = target.map(|target| vec_field(&mut r, target, "origin"));
        (
            vec_field(&mut r, id, "origin"),
            vec_field(&mut r, id, "angles"),
            r.entities[&id].presence,
            look_at,
        )
    };
    let h = &world.resource::<RoundScript>().vehicles[&id];
    let facts = physics(h)?;
    let mut body = h.physics.unwrap();
    let mut velocity = Vec3::from_array(h.velocity);
    let mut angular = h.angle_vel;
    let goal = h
        .goal
        .filter(|_| !body.wrecked && !(h.arrived && h.stop_at_goal));
    let stopping = h.stop_at_goal;
    let mut heading = h.heading;
    let mut next_angles = angles;
    let exclude = presence.map(crate::AuthorityModelOwner::ScriptModel);
    let axis = math_iw4::angles_to_axis(angles).map(Vec3::from_array);
    let offset = axis[0] * center.x + axis[1] * center.y + axis[2] * center.z;
    let hull = axis[0].abs() * half.x + axis[1].abs() * half.y + axis[2].abs() * half.z;
    let mut notes = Vec::new();
    let mut at = Vec3::from_array(origin) + offset;
    if let Some(goal) = goal {
        let delta = Vec3::from_array(goal) - Vec3::from_array(origin);
        let current_speed = (velocity - body.conveyor)
            .dot(delta.normalize_or_zero())
            .max(0.0);
        let wanted = if stopping {
            h.max_speed.min((2.0 * h.decel * delta.length()).sqrt())
        } else {
            h.max_speed
        };
        let speed = if wanted > current_speed {
            (current_speed + h.accel * TICK_S).min(wanted)
        } else {
            (current_speed - h.decel * TICK_S).max(wanted)
        };
        let desired = if h.path_mode == PathMode::Constrained {
            delta.normalize_or_zero()
        } else {
            let yaw = math_iw4::vect_to_angles(delta.to_array())[1];
            update_angle(
                &mut next_angles[1],
                &mut angular[1],
                yaw,
                [h.yaw_speed, h.yaw_accel, h.yaw_decel],
                h.yaw_overshoot,
            );
            Vec3::from_array(math_iw4::angle_vectors([0.0, next_angles[1], 0.0]).0)
        };
        heading = desired.to_array();
        let wanted = desired * speed + body.conveyor;
        velocity += (wanted - velocity).clamp_length_max(h.accel.max(h.decel) * TICK_S);
    }
    if goal.is_none() && !body.wrecked {
        let relative = (velocity - body.conveyor).with_z(0.0);
        velocity -= relative.clamp_length_max(h.decel * TICK_S);
    }
    let yaw = h
        .target_yaw
        .or(h.goal_yaw.filter(|_| h.arrived))
        .or_else(|| {
            look_at
                .map(|at| math_iw4::vect_to_angles([at[0] - origin[0], at[1] - origin[1], 0.0])[1])
        });
    if !body.wrecked {
        if let Some(yaw) = yaw {
            update_angle(
                &mut next_angles[1],
                &mut angular[1],
                yaw,
                [h.yaw_speed, h.yaw_accel, h.yaw_decel],
                h.yaw_overshoot,
            );
        } else if h.path_mode == PathMode::Constrained && goal.is_some() {
            let yaw = math_iw4::vect_to_angles(heading)[1];
            update_angle(
                &mut next_angles[1],
                &mut angular[1],
                yaw,
                [h.yaw_speed, h.yaw_accel, h.yaw_decel],
                h.yaw_overshoot,
            );
        }
    }
    let boat = h.motion.is_some_and(|motion| motion.kind == 3);
    let gravity = 800.0; // Existing IW4L authority gravity, world units/s².
    if boat {
        let from = at + Vec3::Z * hull.z;
        let to = at - Vec3::Z * hull.z;
        let water = crate::frame::FrameWorld::from_world(world).trace_static_world(
            from.to_array(),
            to.to_array(),
            [0.0; 3],
            [0.0; 3],
            0x20,
        );
        if water.fraction < 1.0 {
            // Independent buoyancy spring about the actual water surface and chassis center.
            let surface = from.z + (to.z - from.z) * water.fraction;
            let stiffness = gravity / half.z;
            let lift = (gravity + (surface - at.z) * stiffness
                - 2.0 * stiffness.sqrt() * velocity.z)
                .max(0.0);
            velocity.z += lift * TICK_S;
            let drag = ((body.conveyor - velocity).with_z(0.0) * stiffness.sqrt() * TICK_S)
                .clamp_length_max((body.conveyor - velocity).with_z(0.0).length());
            velocity += drag;
        }
    }
    velocity.z -= gravity * TICK_S;
    velocity = velocity.clamp_length_max(facts.max_velocity);
    let mut left = TICK_S;
    let mut grounded = false;
    for _ in 0..4 {
        if left <= f32::EPSILON {
            break;
        }
        let end = at + velocity * left;
        let trace = crate::frame::FrameWorld::from_world(world).trace_world_box_except(
            movement_iw4::GroundTraceInput {
                start: at.to_array(),
                end: end.to_array(),
                mins: (-hull).to_array(),
                maxs: hull.to_array(),
                tracemask: crate::bullet_collision::MASK_PHYS_WORLD,
            },
            exclude,
        );
        at += (end - at) * trace.fraction;
        if trace.fraction >= 1.0 {
            break;
        }
        let normal = Vec3::from_array(trace.normal);
        let mut incoming = velocity.dot(normal);
        grounded |= normal.z >= 0.7;
        let momentum = (-incoming).max(0.0) * mass(facts, body);
        if normal.z.abs() < 0.7
            && momentum >= facts.collision_momentum
            && !body.wrecked
            && !body.crashing_disabled
        {
            body.wrecked = true;
            let push = crash_push(velocity, facts, body) * facts.impulse_scale;
            (velocity, angular) = impulse_motion(
                velocity,
                angular,
                angles,
                half,
                Vec3::Z * half.z,
                push,
                mass(facts, body),
            )?;
            incoming = velocity.dot(normal);
            notes.push((
                "veh_collision",
                vec![
                    Value::Vector((velocity / MPH).to_array()),
                    Value::Vector(normal.to_array()),
                ],
            ));
        }
        if trace.allsolid != 0 || incoming >= 0.0 {
            velocity = Vec3::ZERO;
            break;
        }
        velocity -= normal * incoming * (1.0 + facts.bounce);
        let friction = if body.wrecked {
            facts.wrecked_friction
        } else {
            facts.friction
        };
        let relative = velocity - body.conveyor;
        let tangent = relative - normal * relative.dot(normal);
        velocity -= tangent.clamp_length_max(friction * gravity * left);
        left *= 1.0 - trace.fraction;
    }
    if body.wrecked {
        for i in 0..3 {
            next_angles[i] = math_iw4::angle_subtract(next_angles[i] + angular[i] * TICK_S, 0.0);
        }
        if grounded {
            let damping = (1.0 - facts.wrecked_friction * TICK_S).max(0.0);
            angular = angular.map(|v| v * damping);
        }
    } else {
        for i in [0, 2] {
            update_angle(
                &mut next_angles[i],
                &mut angular[i],
                0.0,
                [45.0, 45.0, 18.0],
                0.0,
            );
        }
    }
    let relative = velocity - body.conveyor;
    let acceleration = (relative - body.previous_relative_velocity) / TICK_S;
    if acceleration.length() >= facts.jolt_threshold && acceleration.length_squared() > 0.0 {
        let local = Vec3::new(
            acceleration.dot(axis[0]),
            acceleration.dot(axis[1]),
            acceleration.dot(axis[2]),
        );
        notes.push(("veh_jolt", vec![Value::Vector((local / MPH).to_array())]));
    }
    body.previous_relative_velocity = relative;
    let next_axis = math_iw4::angles_to_axis(next_angles).map(Vec3::from_array);
    let mut next_offset =
        next_axis[0] * center.x + next_axis[1] * center.y + next_axis[2] * center.z;
    if next_angles != angles {
        let rotated_hull =
            next_axis[0].abs() * half.x + next_axis[1].abs() * half.y + next_axis[2].abs() * half.z;
        let overlap = crate::frame::FrameWorld::from_world(world).trace_world_box_except(
            movement_iw4::GroundTraceInput {
                start: at.to_array(),
                end: at.to_array(),
                mins: (-rotated_hull).to_array(),
                maxs: rotated_hull.to_array(),
                tracemask: crate::bullet_collision::MASK_PHYS_WORLD,
            },
            exclude,
        );
        if overlap.startsolid != 0 {
            next_angles = angles;
            next_offset = offset;
            angular = angular.map(|v| -v * facts.bounce);
        }
    }
    let next = at - next_offset;
    if !next.is_finite() || !velocity.is_finite() || !next_angles.into_iter().all(f32::is_finite) {
        return Err("vehicle physics produced non-finite motion".into());
    }
    let h = world
        .resource_mut::<RoundScript>()
        .into_inner()
        .vehicles
        .get_mut(&id)
        .unwrap();
    if body.wrecked {
        wreck(h);
    }
    h.physics = Some(body);
    h.velocity = velocity.to_array();
    h.angle_vel = angular;
    h.speed = reported_speed(h, next_angles);
    h.heading = heading;
    if let Some(goal) = goal.filter(|_| !body.wrecked) {
        let distance = next.distance(Vec3::from_array(goal));
        if h.near_goal > 0.0 && !h.near_notified && distance <= h.near_goal {
            h.near_notified = true;
            notes.push(("near_goal", Vec::new()));
        }
        if !h.arrived && distance <= ARRIVED {
            h.arrived = true;
            notes.push(("goal", Vec::new()));
        }
    }
    Ok(Motion {
        origin: next.to_array(),
        angles: next_angles,
        velocity: velocity.to_array(),
        notes,
    })
}
