use super::*;
use crate::frame::FrameWorld;
use math_iw4::angle_normalize_360 as normalize;

#[derive(Clone, Copy, Debug, Default)]
struct Orient {
    mode: u8,
    look: [f32; 2],
    body: f32,
}
impl Orient {
    fn angles(&mut self, pitch: f32, yaw: f32) {
        self.look = [normalize(pitch), normalize(yaw)];
        self.body = self.look[1];
    }
    fn vector(&mut self, v: [f32; 3]) {
        let yaw = v[1].atan2(v[0]).to_degrees();
        let pitch = (-v[2]).atan2(v[0].hypot(v[1])).to_degrees();
        self.angles(pitch, yaw);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Orientation {
    code: Orient,
    script: Orient,
    pub turn_rate: f32,
    face_motion: bool,
    pub(super) relative_dir: u8,
    pub(super) gun_blocked: bool,
}
impl Default for Orientation {
    fn default() -> Self {
        Self {
            code: Orient {
                mode: 1,
                ..Default::default()
            },
            script: Default::default(),
            turn_rate: 0.3,
            face_motion: false,
            relative_dir: 0,
            gun_blocked: false,
        }
    }
}
impl Orientation {
    pub(super) fn face_motion(&self) -> bool {
        self.face_motion
    }
    pub(super) fn select_move(&mut self) {
        self.code.mode = 4;
    }
    pub(super) fn select_arrival(&mut self, yaw: f32) {
        self.code.mode = 1;
        self.code.angles(0., yaw);
    }
    pub(super) fn select_stop(&mut self) {
        self.code.mode = 1;
    }
    pub(super) fn select_reacquire(&mut self, face_enemy: bool) {
        self.code.mode = if face_enemy { 3 } else { 4 };
    }
    pub(super) fn set_angles(&mut self, angles: [f32; 3]) {
        self.code.angles(angles[0], angles[1]);
        if self.script.mode == 1 {
            self.script.angles(angles[0], angles[1]);
        }
    }
    pub(super) fn clear_script(&mut self) {
        if self.script.mode == 0 {
            self.script = self.code;
        } else {
            self.code = self.script;
        }
        self.script.mode = 0;
    }
    fn active(&mut self) -> &mut Orient {
        if self.script.mode == 0 {
            &mut self.code
        } else {
            &mut self.script
        }
    }
    fn active_mode(&self) -> u8 {
        if self.script.mode == 0 {
            self.code.mode
        } else {
            self.script.mode
        }
    }
    fn update(&mut self, motion: &mut physics::ActorPhysics, yaw: f32) -> [f32; 3] {
        let target = *self.active();
        motion.desired_angles = [target.look[0], target.body];
        for (current, desired) in motion.look_angles.iter_mut().zip(target.look) {
            *current = normalize(*current + delta(desired, *current).clamp(-40., 40.));
        }
        let limit = self.turn_rate * 50.;
        let mut step = delta(target.body, yaw);
        if step > limit {
            step = limit;
        } else if step < -limit {
            step = -limit;
        }
        [0., normalize(yaw + step), 0.]
    }
}
fn delta(target: f32, current: f32) -> f32 {
    let turns = (target - current) * (1. / 360.);
    (turns - (turns + 0.5).floor()) * 360.
}

fn body_angle(world: &mut World, id: u64, yaw: f32) -> Result<(), String> {
    let angles = [0., normalize(yaw), 0.];
    let number = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.entities[&id].number
    };
    if world.resource::<RoundScript>().actors[&id].physics_initialized {
        let mut frame = FrameWorld::from_world(world);
        let mover = frame
            .script_mover_mut_by_number(number)
            .ok_or("actor presence is missing")?;
        entity_iw4::set_angle(&mut mover.state, angles);
    }
    Ok(())
}

fn orient_mode(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let mode = text(args, 0)?;
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = runtime.object_field(id, "origin");
        let actor = runtime.actors.get_mut(&id).unwrap();
        let orient = &mut actor.orientation;
        match mode {
            "face angle" => {
                orient.script.mode = 1;
                orient.script.angles(0., float(args, 1)?);
            }
            "face current" => {
                orient.script.body = actor.motion.desired_angles[1];
                orient.script.mode = 1;
            }
            "face direction" => {
                let v = super::super::args::vector(args, 1)?;
                if v[0] == 0. && v[1] == 0. {
                    return Err("face direction requires a nonzero XY vector".into());
                }
                orient.script.mode = 1;
                orient.script.vector(v);
            }
            "face point" => {
                let point = super::super::args::vector(args, 1)?;
                let Value::Vector(origin) = origin else {
                    return Err("actor origin is not a vector".into());
                };
                let v = std::array::from_fn(|i| point[i] - origin[i]);
                orient.script.mode = 1;
                if v[0] * v[0] + v[1] * v[1] >= 1. {
                    orient.script.vector(v);
                }
            }
            "face motion" => orient.script.mode = 2,
            "face enemy" => orient.script.mode = 3,
            "face enemy or motion" => orient.script.mode = 4,
            "face goal" => orient.script.mode = 5,
            "face default" => orient.clear_script(),
            _ => return Err(format!("unknown orientation mode '{mode}'")),
        }
    }
    if args.len() > 2 && super::super::args::int(args, 2)? > 0 {
        let yaw = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let orient = &mut runtime.actors.get_mut(&id).unwrap().orientation;
            orient.code.body = normalize(orient.script.body);
            orient.script.body
        };
        body_angle(world, id, yaw)?;
    }
    Ok(Value::Undefined)
}

fn should_face_motion(
    world: &mut World,
    id: u64,
    target: Option<[f32; 3]>,
) -> Result<bool, String> {
    if world.resource::<RoundScript>().actors[&id]
        .navigation
        .steering_point()
        .is_none()
    {
        return Ok(false);
    }
    let Some(target) = target else {
        return Ok(true);
    };
    let Value::Vector(origin) = super::super::players::entity_field(world, id, "origin") else {
        return Err("actor has no position for orientation".into());
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    // Scripts may narrow the distance at which an enemy takes precedence over
    // locomotion. Otherwise this runtime uses the actor's sight range.
    let distance = match runtime.object_field(id, "maxfaceenemydist") {
        Value::Float(n) => Some(n),
        Value::Int(n) => Some(n as f32),
        _ => None,
    }
    .unwrap_or(runtime.actors[&id].sight.maximum_distance());
    if !distance.is_finite() || distance < 0.0 {
        return Err("maxfaceenemydist must be finite and nonnegative".into());
    }
    Ok(
        glam::Vec3::from_array(origin).distance_squared(glam::Vec3::from_array(target))
            > distance * distance,
    )
}

pub(crate) fn advance(world: &mut World) -> Result<(), String> {
    let _span = perf::span_handle!("actor_orientation", "iw4l.sim").enter();
    let mut ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(_, a)| (a.alive || a.death.is_some()) && a.physics_initialized)
        .map(|(&id, a)| (a.slot, id))
        .collect();
    ids.sort_unstable();
    for (_, id) in ids {
        if super::turret::active(world.resource::<RoundScript>(), id).is_some() {
            continue;
        }
        if world.resource::<RoundScript>().entities[&id]
            .linked_to
            .as_ref()
            .is_some_and(|link| link.blend.is_some())
        {
            continue;
        }
        if super::anim_mode::effective(&world.resource::<RoundScript>().actors[&id])
            .uses_root_motion()
            && !world.resource::<RoundScript>().actors[&id]
                .traversal
                .pending()
        {
            let delta = super::anim_mode::delta(world, id)?.yaw();
            if delta != 0. {
                let yaw = {
                    let mut runtime = world.resource_mut::<RoundScript>();
                    let Value::Vector(angles) = runtime.object_field(id, "angles") else {
                        return Err("actor angles are not a vector".into());
                    };
                    let actor = runtime.actors.get_mut(&id).unwrap();
                    for orient in [&mut actor.orientation.code, &mut actor.orientation.script] {
                        orient.body = normalize(orient.body + delta);
                        orient.look[1] = normalize(orient.look[1] + delta);
                    }
                    actor.motion.desired_angles[1] =
                        normalize(actor.motion.desired_angles[1] + delta);
                    actor.motion.look_angles[1] = normalize(actor.motion.look_angles[1] + delta);
                    normalize(angles[1] + delta)
                };
                body_angle(world, id, yaw)?;
            }
        }
        let mode = world.resource::<RoundScript>().actors[&id]
            .orientation
            .active_mode();
        let target = if matches!(mode, 3 | 4) {
            targeting::facing_point(world, id)?
        } else {
            None
        };
        let face_motion = mode == 4 && should_face_motion(world, id, target)?;
        let yaw = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Value::Vector(angles) = runtime.object_field(id, "angles") else {
                return Err("actor angles are not a vector".into());
            };
            if scripted::owns_placement(&runtime.actors[&id]) {
                runtime
                    .actors
                    .get_mut(&id)
                    .unwrap()
                    .orientation
                    .code
                    .angles(angles[0], angles[1]);
                continue;
            }
            let claim = runtime.actors[&id].claimed_node;
            let Value::Vector(origin) = runtime.object_field(id, "origin") else {
                return Err("actor origin is not a vector".into());
            };
            let node_angle = claim.and_then(|index| runtime.path_nodes.actor_angle(index));
            let actor = runtime.actors.get_mut(&id).unwrap();
            let orient = &mut actor.orientation;
            match orient.active().mode {
                1 => orient.face_motion = false,
                2 => {
                    orient.face_motion = true;
                    if let Some(point) = actor.navigation.steering_point() {
                        let direction = std::array::from_fn(|i| point[i] - origin[i]);
                        if direction[0] * direction[0] + direction[1] * direction[1] > 0.001 {
                            orient.active().vector(direction);
                        }
                    }
                }
                4 if face_motion => {
                    orient.face_motion = true;
                    if let Some(point) = actor.navigation.steering_point() {
                        let direction = std::array::from_fn(|i| point[i] - origin[i]);
                        if direction[0] * direction[0] + direction[1] * direction[1] > 0.001 {
                            orient.active().vector(direction);
                        }
                    }
                }
                3 | 4 => {
                    orient.face_motion = false;
                    if let Some(point) = target.or_else(|| actor.navigation.steering_point()) {
                        let direction = std::array::from_fn(|i| point[i] - origin[i]);
                        if direction[0] * direction[0] + direction[1] * direction[1] > 0.001 {
                            orient.active().vector(direction);
                        }
                    }
                }
                5 => {
                    orient.face_motion = false;
                    if let Some(yaw) = node_angle {
                        orient.active().angles(0., yaw);
                    }
                }
                _ => {}
            }
            orient.update(&mut actor.motion, angles[1])[1]
        };
        body_angle(world, id, yaw)?;
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "orientmode", orient_mode);
    registry.register(
        Namespace::Method,
        "getanglestolikelyenemypath",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            reacquire::likely_angles(world, id)
                .map(|angles| angles.map_or(Value::Undefined, Value::Vector))
        },
    );
    registry.register(
        Namespace::Method,
        "shouldfacemotion",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            let target = targeting::facing_point(world, id)?;
            Ok(Value::Int(should_face_motion(world, id, target)?.into()))
        },
    );
    registry.register(Namespace::Method, "getmotionangle", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        let actor = &runtime.actors[&id];
        let velocity = actor.motion.velocity;
        let direction = if velocity[0] * velocity[0] + velocity[1] * velocity[1] > 1.0 {
            Some(velocity)
        } else {
            actor
                .navigation
                .point()
                .map(|point| std::array::from_fn(|i| point[i] - origin[i]))
        };
        let angle = if actor.orientation.face_motion {
            0.0
        } else {
            direction.map_or(0.0, |direction| {
                math_iw4::angle_subtract(direction[1].atan2(direction[0]).to_degrees(), angles[1])
            })
        };
        Ok(Value::Float(angle))
    });
}
