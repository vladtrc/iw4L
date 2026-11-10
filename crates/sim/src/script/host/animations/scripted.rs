use super::super::args::vector;
use super::*;

#[derive(Clone, Debug)]
pub(super) struct ScriptedMotion {
    node: XAnimNodeId,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    origin_error: [f32; 3],
    angles_error: [f32; 3],
    correction: Option<([f32; 3], [f32; 3])>,
    started: bool,
    finished: bool,
}

fn placement(args: &[Value]) -> Result<([f32; 3], [[f32; 3]; 3]), String> {
    let origin = vector(args, 0)?;
    let angles = vector(args, 1)?;
    if origin.into_iter().chain(angles).any(|v| !v.is_finite()) {
        return Err("animation placement must be finite".into());
    }
    Ok((origin, math_iw4::angles_to_axis(angles)))
}

fn start(world: &mut World, receiver_value: &Value, args: &[Value]) -> Result<Value, String> {
    start_internal(world, receiver_value, args, true)
}

fn start_internal(
    world: &mut World,
    receiver_value: &Value,
    args: &[Value],
    handoff: bool,
) -> Result<Value, String> {
    if !(4..=6).contains(&args.len()) {
        return Err("animscripted expects four to six arguments".into());
    }
    let Value::String(notify) = arg(args, 0)? else {
        return Err("animation notification name must be a string".into());
    };
    let (origin, axis) = placement(&args[1..3])?;
    if let Some(mode) = args.get(4) {
        match mode {
            Value::String(name) if &**name == "normal" => (),
            Value::String(name) if &**name == "deathplant" => {
                return Err("animscripted death planting is not implemented".into());
            }
            _ => return Err("animscripted mode must be normal or deathplant".into()),
        }
    }
    let id = receiver(world, receiver_value)?;
    let (node, root, restart) = {
        let runtime = world.resource::<RoundScript>();
        let entity = &runtime.entities[&id];
        if entity.kind == EntityKind::Actor && !runtime.actors.contains_key(&id) {
            return Err("actor state requires an allocated actor".into());
        }
        if entity.kind != EntityKind::Actor
            && (&*entity.classname != "script_model" || entity.kind == EntityKind::Player)
        {
            return Err("entity does not support model animscripted".into());
        }
        if entity.linked_to.is_some() {
            return Err("animscripted relative linked placement is not implemented".into());
        }
        let anim = animator(world, id)?;
        let node = anim.node(arg(args, 3)?)?;
        let root = args
            .get(5)
            .map(|value| anim.node(value))
            .transpose()?
            .unwrap_or(XAnimNodeId(0));
        if !notify.is_empty() && anim.sync.group(node).is_none() {
            anim.leaf(node)?;
        }
        let looping = anim
            .runtime
            .leaf_clip(node)
            .map(|clip| clip.looping)
            .or_else(|| anim.sync.looping(node))
            .unwrap_or(false);
        (node, root, !looping)
    };
    if world.resource::<RoundScript>().entities[&id].kind == EntityKind::Actor {
        model(world.resource_mut::<RoundScript>().into_inner(), id)?;
        if !world.resource::<RoundScript>().actors[&id].alive {
            return Err("cannot animscripted a nonliving actor".into());
        }
        if handoff {
            return super::super::actors::scripted::enter(world, id, args);
        }
    }
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        model(&mut runtime, id)?;
        let anim = runtime.animators.get_mut(&id).unwrap();
        let descendants: Vec<_> = (0..anim.runtime.definition().nodes().len())
            .filter(|&index| {
                let mut parent = anim.runtime.definition().nodes()[index].parent;
                while let Some(at) = parent {
                    if at == root {
                        return true;
                    }
                    parent = anim.runtime.definition().nodes()[at.0 as usize].parent;
                }
                false
            })
            .collect();
        for index in descendants {
            clear_goal(anim, XAnimNodeId(index as u16), 0.2)?;
        }
    }
    let mut command = vec![
        args[3].clone(),
        Value::Float(1.0),
        Value::Float(0.2),
        Value::Float(1.0),
    ];
    let flagged = !notify.is_empty();
    if flagged {
        command.insert(0, args[0].clone());
    }
    set_anim(
        world,
        receiver_value,
        &command,
        true,
        restart,
        AnimCommand::Blend,
        flagged,
    )?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let Value::Vector(current_origin) = runtime.object_field(id, "origin") else {
        return Err("entity origin is not a vector".into());
    };
    let Value::Vector(current_angles) = runtime.object_field(id, "angles") else {
        return Err("entity angles are not a vector".into());
    };
    let anim = runtime.animators.get_mut(&id).unwrap();
    let (placed, angles) = anim
        .runtime
        .absolute_delta(node)
        .unwrap()
        .world_pose(origin, axis);
    anim.scripted = Some(ScriptedMotion {
        node,
        origin,
        axis,
        origin_error: std::array::from_fn(|i| current_origin[i] - placed[i]),
        angles_error: std::array::from_fn(|i| {
            math_iw4::angle_subtract(current_angles[i], angles[i])
        }),
        correction: None,
        started: false,
        finished: false,
    });
    Ok(Value::Undefined)
}

fn reduce_errors(motion: &mut ScriptedMotion, angle_rate: f32) {
    let length = math_iw4::vec3_length(motion.origin_error);
    let fraction = if length == 0.0 {
        0.0
    } else {
        (0.25 / length).min(1.0)
    };
    let position = motion.origin_error.map(|v| v * fraction);
    for (error, applied) in motion.origin_error.iter_mut().zip(position) {
        *error -= applied;
    }
    let angles = motion
        .angles_error
        .map(|v| v.clamp(-angle_rate * 0.05, angle_rate * 0.05));
    for (error, applied) in motion.angles_error.iter_mut().zip(angles) {
        *error -= applied;
    }
    motion.correction = Some((position, angles));
}

pub(super) fn begin_frame(world: &mut World) {
    for anim in world.resource_mut::<RoundScript>().animators.values_mut() {
        if let Some(motion) = &mut anim.scripted {
            motion.correction = None;
        }
    }
}

pub(super) fn apply(world: &mut World) -> Result<(), String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let owners: Vec<_> = runtime
        .animators
        .iter()
        .filter_map(|(&id, anim)| anim.scripted.as_ref().map(|_| id))
        .collect();
    let changed = !owners.is_empty();
    for id in owners {
        if runtime.entities[&id].linked_to.is_some() {
            return Err("animscripted relative linked placement is not implemented".into());
        }
        let rate = match runtime.object_field(id, "anglelerprate") {
            Value::Float(v) => v,
            Value::Int(v) => v as f32,
            _ => return Err("anglelerprate must be a number".into()),
        };
        if !rate.is_finite() || rate < 0.0 {
            return Err("anglelerprate must be finite and nonnegative".into());
        }
        let actor = runtime.actors.contains_key(&id);
        let anim = runtime.animators.get_mut(&id).unwrap();
        let motion = anim.scripted.as_mut().unwrap();
        if motion.finished || motion.node.0 == 0 {
            anim.scripted = None;
            continue;
        }
        if motion.correction.is_none() {
            reduce_errors(motion, rate);
        }
        let (mut origin, mut angles) = anim
            .runtime
            .absolute_delta(motion.node)
            .unwrap()
            .world_pose(motion.origin, motion.axis);
        if actor {
            motion.started = true;
        }
        let (position, correction) = motion.correction.unwrap();
        for i in 0..3 {
            origin[i] += position[i];
            angles[i] += correction[i];
            if correction[i] != 0.0 && motion.angles_error[i] != 0.0 {
                angles[i] = math_iw4::angle_normalize_360(angles[i]);
            }
        }
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
    drop(runtime);
    if changed {
        super::publish(world);
        super::super::presence::settle_collision(world);
    }
    Ok(())
}

pub(super) fn finish_frame(world: &mut World) -> Result<(), String> {
    for anim in world.resource_mut::<RoundScript>().animators.values_mut() {
        let Some(motion) = &mut anim.scripted else {
            continue;
        };
        if motion.started && anim.runtime.states()[motion.node.0 as usize].time == 1.0 {
            let node = motion.node;
            motion.finished = true;
            set_goal(anim, node, 1.0, 0.2, 1.0)?;
            complete_ancestors(anim, node, 0.2)?;
        } else {
            motion.started = true;
        }
    }
    Ok(())
}

fn start_pose(world: &mut World, args: &[Value]) -> Result<([f32; 3], [f32; 3]), String> {
    let (origin, axis) = placement(args)?;
    let pose = super::super::natives::player::animation_clip_or_branch(world, args, 2)?
        .and_then(|clip| clip.absolute_delta(0.0))
        .unwrap_or(xmodel_runtime::RootMotionPose::default());
    Ok(pose.world_pose(origin, axis))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "getstartorigin", |world, _, args| {
        Ok(Value::Vector(start_pose(world, args)?.0))
    });
    registry.register(Namespace::Function, "getstartangles", |world, _, args| {
        Ok(Value::Vector(start_pose(world, args)?.1))
    });
    registry.register(
        Namespace::Method,
        "startscriptedanim",
        |world, receiver, args| {
            super::super::actors::actor_id(world, receiver)?;
            start_internal(world, receiver, args, false)
        },
    );
    registry.register(Namespace::Method, "animscripted", start);
    registry.register(Namespace::Method, "animscriptedthirdperson", start);
    registry.register(Namespace::Method, "stopanimscripted", |world, value, _| {
        let id = receiver(world, value)?;
        let actor = {
            let mut runtime = world.resource_mut::<RoundScript>();
            if let Some(anim) = runtime.animators.get_mut(&id)
                && let Some(motion) = anim.scripted.take()
                && motion.node.0 != 0
                && !motion.finished
            {
                set_goal(anim, motion.node, 0.0, 0.2, 1.0)?;
                complete_ancestors(anim, motion.node, 0.2)?;
            }
            runtime.actors.contains_key(&id)
        };
        if actor {
            super::super::actors::scripted::pop(world, id);
        }
        Ok(Value::Undefined)
    });
}

pub(crate) fn actor_motion_active(world: &World, id: u64) -> bool {
    world
        .resource::<RoundScript>()
        .animators
        .get(&id)
        .is_some_and(|anim| {
            anim.scripted
                .as_ref()
                .is_some_and(|motion| !motion.finished && motion.node.0 != 0)
        })
}
pub(crate) fn stop_actor_motion(world: &mut World, id: u64) {
    if let Some(anim) = world.resource_mut::<RoundScript>().animators.get_mut(&id) {
        anim.scripted = None;
    }
}
