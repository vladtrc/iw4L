use super::*;
use crate::frame::FrameWorld;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Checked,
    Force,
    Safe,
}

fn teleport(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    mode: Mode,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let origin = super::super::args::vector(args, 0)?;
    let angles = if args.len() > 1 {
        Some(super::super::args::vector(args, 1)?)
    } else {
        None
    };
    if origin
        .into_iter()
        .chain(angles.into_iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err("teleport placement must be finite".into());
    }
    let (number, distance, scripted, linked) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let entity = &runtime.entities[&id];
        let linked = entity.linked_to.is_some();
        if !runtime.actors[&id].physics_initialized {
            return Err("actor teleport requires physical presence".into());
        }
        let number = entity.number;
        let Value::Vector(old) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let distance = (0..3).map(|i| (old[i] - origin[i]).powi(2)).sum::<f32>();
        let loading = runtime.loading;
        if !linked && !loading && mode != Mode::Force {
            if mode == Mode::Safe {
                let maximum = if args.len() > 2 {
                    float(args, 2)?
                } else {
                    100.
                };
                if distance > maximum * maximum {
                    return Ok(Value::Int(0));
                }
            }
            if distance > 100. {
                return Err(format!(
                    "actor teleport visibility checks are not implemented (distance={})",
                    distance.sqrt()
                ));
            }
            if runtime.actors[&id].navigation.point().is_some()
                && !runtime.actors[&id].traversal.active()
            {
                return Err(
                    "actor teleport during an active path requires path lookahead state".into(),
                );
            }
        }
        let actor = runtime.actors.get_mut(&id).unwrap();
        actor.motion.stop();
        if !linked && (loading || distance > 100.) {
            actor.navigation = Default::default();
        }
        if let Some(angles) = angles {
            actor.motion.set_angles(angles);
            actor.orientation.set_angles(angles);
        }
        if !linked && distance > 100.0 {
            actor.path_stances = None;
        }
        let scripted = actor.scripted_control.is_some() || actor.traversal.active();
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        if let Some(angles) = angles {
            runtime.set_object_field(id, "angles", Value::Vector(angles));
        }
        (number, distance, scripted, linked)
    };
    let mut frame = FrameWorld::from_world(world);
    let mover = frame
        .script_mover_mut_by_number(number)
        .ok_or("actor presence is missing")?;
    entity_iw4::set_origin(&mut mover.state, origin);
    if let Some(angles) = angles {
        entity_iw4::set_angle(&mut mover.state, angles);
    }
    if linked || distance > 16384.0 {
        mover.state.e_flags ^= 2;
    }
    super::super::presence::settle_collision(world);
    if !linked && mode == Mode::Force && distance > 100.0 && !scripted {
        super::scripted::kill_anim_script(world, id);
    }
    Ok(Value::Int(1))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "forceteleport",
        |world, receiver, args| teleport(world, receiver, args, Mode::Force),
    );
    registry.register(Namespace::Method, "teleport", |world, receiver, args| {
        teleport(world, receiver, args, Mode::Checked)
    });
    registry.register(
        Namespace::Method,
        "safeteleport",
        |world, receiver, args| teleport(world, receiver, args, Mode::Safe),
    );
}
