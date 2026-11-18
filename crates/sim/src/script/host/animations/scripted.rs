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
    relative: bool,
    previous_mode: Option<super::super::actors::anim_mode::Mode>,
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
    start_internal(world, receiver_value, args, true, false)?;
    // ScrCmd_animscriptedInternal captures the inverse parent frame after starting
    // the script. StartScriptedAnim reuses that frame when the actor hands off.
    let id = receiver(world, receiver_value)?;
    let link = world.resource::<RoundScript>().entities[&id]
        .linked_to
        .clone();
    if let Some(link) = link {
        let pose = super::super::presence::link_parent_pose(world, &link)
            .ok_or("animscripted parent attachment has no pose")?;
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .unwrap()
            .linked_to
            .as_mut()
            .unwrap()
            .scripted_parent = Some(pose);
    }
    Ok(Value::Undefined)
}

fn start_relative(
    world: &mut World,
    receiver_value: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = receiver(world, receiver_value)?;
    let presence = world.resource::<RoundScript>().entities[&id].presence;
    let has_model = super::super::presence::settled(world)
        .entity_collision_capabilities()
        .iter()
        .any(|row| row.owner.script_model() == presence && row.dobj.is_some());
    if !has_model {
        return Err("animrelative requires a server model pose".into());
    }
    let link = world.resource::<RoundScript>().entities[&id]
        .linked_to
        .clone();
    let parent = link
        .as_ref()
        .map(|link| {
            super::super::presence::link_parent_pose(world, link)
                .ok_or("animrelative parent attachment has no pose")
        })
        .transpose()?;
    start_internal(world, receiver_value, args, false, true)?;
    if let Some(parent) = parent {
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .unwrap()
            .linked_to
            .as_mut()
            .unwrap()
            .scripted_parent = Some(parent);
    }
    Ok(Value::Undefined)
}

fn start_internal(
    world: &mut World,
    receiver_value: &Value,
    args: &[Value],
    handoff: bool,
    relative: bool,
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
        // Scripted placement is also used by campaign vehicles (Favela's van).
        // The model and animation tree are the capability boundary; classname
        // alone does not determine whether an entity can play the animation.
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
        if !relative {
            restore_mode(&mut runtime, id);
        }
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
    let previous_mode = if relative {
        runtime.actors.get(&id).map(|a| a.script_anim_mode)
    } else {
        None
    };
    let previous_mode = runtime
        .animators
        .get(&id)
        .and_then(|a| a.scripted.as_ref())
        .filter(|m| m.relative)
        .and_then(|m| m.previous_mode)
        .or(previous_mode);
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
        relative,
        previous_mode,
    });
    if relative && let Some(actor) = runtime.actors.get_mut(&id) {
        actor.script_anim_mode = super::super::actors::anim_mode::Mode::PointRelative;
        actor.motion.stop();
        actor.navigation = Default::default();
        actor.move_mode = 0;
    }
    Ok(Value::Undefined)
}

fn reduce_errors(motion: &mut ScriptedMotion, angle_rate: f32) {
    if motion.relative {
        // Blend placement error in our own frame-time model, retaining the
        // remaining error rather than jumping to the reference frame at start.
        const BLEND_SECONDS: f32 = 0.15;
        let seconds = crate::MATCH_TICK_MS as f32 * 0.001;
        motion.origin_error = motion
            .origin_error
            .map(|v| v * (-seconds / BLEND_SECONDS).exp());
        motion.angles_error = motion
            .angles_error
            .map(|v| v - v.clamp(-angle_rate * seconds, angle_rate * seconds));
        motion.correction = Some((motion.origin_error, motion.angles_error));
        return;
    }
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
    let completed: Vec<_> = world
        .resource::<RoundScript>()
        .animators
        .iter()
        .filter_map(|(&id, a)| {
            a.scripted
                .as_ref()
                .is_some_and(|m| m.relative && (m.finished || m.node.0 == 0))
                .then_some(id)
        })
        .collect();
    for id in completed {
        stop_actor_motion(world, id);
    }
    for anim in world.resource_mut::<RoundScript>().animators.values_mut() {
        if let Some(motion) = &mut anim.scripted {
            motion.correction = None;
        }
    }
}

type Pose = ([f32; 3], [[f32; 3]; 3]);

fn relative_pose(pose: Pose, parent: Pose) -> ([[f32; 3]; 3], [f32; 3]) {
    let inverse = std::array::from_fn(|i| std::array::from_fn(|j| parent.1[j][i]));
    let translation =
        std::array::from_fn(|i| -(0..3).map(|j| parent.0[j] * parent.1[i][j]).sum::<f32>());
    math_iw4::matrix_multiply43(pose.1, pose.0, inverse, translation)
}

fn move_with_parent(origin: [f32; 3], axis: [[f32; 3]; 3], anchor: Pose, parent: Pose) -> Pose {
    let (local_axis, local_origin) = relative_pose((origin, axis), anchor);
    let (axis, origin) = math_iw4::matrix_multiply43(local_axis, local_origin, parent.1, parent.0);
    (origin, axis)
}

pub(super) fn apply(world: &mut World) -> Result<(), String> {
    let owners: Vec<_> = world
        .resource::<RoundScript>()
        .animators
        .iter()
        .filter_map(|(&id, anim)| anim.scripted.as_ref().map(|_| id))
        .collect();
    let changed = !owners.is_empty();
    for id in owners {
        let link = world.resource::<RoundScript>().entities[&id]
            .linked_to
            .clone();
        if link.as_ref().is_some_and(|link| link.blend.is_some()) {
            let mut runtime = world.resource_mut::<RoundScript>();
            let anim = runtime.animators.get_mut(&id).unwrap();
            let motion = anim.scripted.as_mut().unwrap();
            if motion.finished || motion.node.0 == 0 {
                anim.scripted = None;
            } else {
                // The attachment owns placement while the clip still runs
                // and delivers its normal completion notifications.
                motion.started = true;
            }
            continue;
        }
        let parent = link
            .as_ref()
            .map(|link| {
                super::super::presence::link_parent_pose(world, link)
                    .ok_or("animscripted parent attachment has no pose")
            })
            .transpose()?;
        let mut runtime = world.resource_mut::<RoundScript>();
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
            if !motion.relative {
                anim.scripted = None;
            }
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
        if let (Some(link), Some(parent)) = (link, parent) {
            let anchor = link
                .scripted_parent
                .ok_or("linked animation has no captured parent frame")?;
            let pose = move_with_parent(origin, math_iw4::angles_to_axis(angles), anchor, parent);
            origin = pose.0;
            angles = math_iw4::axis_to_angles(pose.1);
            // G_CalcTagAxis(ent, 0) retains the resulting relative attachment for
            // ordinary link updates after this animation has finished.
            let (local_axis, local_origin) = relative_pose(pose, parent);
            let link = runtime
                .entities
                .get_mut(&id)
                .unwrap()
                .linked_to
                .as_mut()
                .unwrap();
            link.origin = local_origin;
            link.angles = math_iw4::axis_to_angles(local_axis);
        }
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
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
            start_internal(world, receiver, args, false, false)
        },
    );
    registry.register(Namespace::Method, "animrelative", start_relative);
    registry.register(Namespace::Method, "animscripted", start);
    registry.register(Namespace::Method, "animscriptedthirdperson", start);
    registry.register(Namespace::Method, "stopanimscripted", |world, value, _| {
        let id = receiver(world, value)?;
        let relative = world
            .resource::<RoundScript>()
            .animators
            .get(&id)
            .and_then(|a| a.scripted.as_ref())
            .is_some_and(|m| m.relative);
        if relative {
            let mut runtime = world.resource_mut::<RoundScript>();
            restore_mode(&mut runtime, id);
        }
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
        if actor && !relative {
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
fn restore_mode(runtime: &mut RoundScript, id: u64) {
    let previous = runtime
        .animators
        .get(&id)
        .and_then(|a| a.scripted.as_ref())
        .filter(|m| m.relative)
        .and_then(|m| m.previous_mode);
    if let (Some(previous), Some(actor)) = (previous, runtime.actors.get_mut(&id))
        && actor.script_anim_mode == super::super::actors::anim_mode::Mode::PointRelative
    {
        actor.script_anim_mode = previous;
        actor.motion.stop();
    }
}
pub(crate) fn stop_actor_motion(world: &mut World, id: u64) {
    clear_motion(&mut world.resource_mut::<RoundScript>(), id);
}

pub(super) fn clear_motion(runtime: &mut RoundScript, id: u64) {
    restore_mode(runtime, id);
    if let Some(anim) = runtime.animators.get_mut(&id) {
        anim.scripted = None;
    }
}
