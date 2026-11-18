use super::*;

pub(crate) fn think(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_animation", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| {
            (actor.alive
                && actor.script_initialized
                && actor.scripted_control.is_none()
                && actor.turret.requested.is_none())
            .then_some(id)
        })
        .collect();
    for id in ids {
        let actor = &world.resource::<RoundScript>().actors[&id];
        if anim_mode::effective(actor) == anim_mode::Mode::PointRelative {
            continue;
        }
        if actor.traversal.active() {
            traversal::try_start(world, id, now)?;
            continue;
        }
        let running_to = actor.arrival.running_to();
        let transition = actor.arrival.transitioning();
        let at_goal = transition
            || (!running_to && actor.cover.holding())
            || goals::at_movement_goal(world, id)?;
        // Override routes are built with full support checks by the arrival
        // service. A blocked request must not fall back to a different path.
        if !running_to
            || world.resource::<RoundScript>().actors[&id]
                .navigation
                .point()
                .is_some()
        {
            navigation::prepare(world, id, at_goal)?;
        }
        if traversal::try_start(world, id, now)? {
            continue;
        }
        let mut moving = !at_goal
            && world.resource::<RoundScript>().actors[&id]
                .navigation
                .steering_point()
                .is_some();
        let mut kind =
            if let Some(kind) = grenades::behavior(&world.resource::<RoundScript>().actors[&id]) {
                kind
            } else if transition {
                scripted::ControlKind::CoverArrival
            } else if moving {
                scripted::ControlKind::Move
            } else if running_to {
                scripted::ControlKind::Stop
            } else {
                cover::stop_behavior(world, id)?
                    .or_else(|| combat::stationary_behavior(world, id))
                    .unwrap_or(scripted::ControlKind::Stop)
            };
        if matches!(
            kind,
            scripted::ControlKind::GrenadeReturn | scripted::ControlKind::GrenadeCower
        ) {
            moving = false;
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .navigation = Default::default();
        }
        let mode = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Value::Vector(origin) = runtime.object_field(id, "origin") else {
                return Err("actor origin is not a vector".into());
            };
            let actor = &runtime.actors[&id];
            let distance = glam::Vec3::from_array(origin)
                .distance(glam::Vec3::from_array(goals::movement_goal(actor).position));
            if !moving {
                0
            } else if distance > actor.walk_distance[1] {
                3
            } else {
                2
            }
        };
        let (changed, same) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let actor = runtime.actors.get_mut(&id).unwrap();
            let changed = actor.move_mode != mode;
            actor.move_mode = mode;
            (changed, actor.anim_script_kind == Some(kind))
        };
        if same {
            if changed {
                crate::script::runtime::raise(world, Value::Object(id), "movemode", vec![]);
            }
            continue;
        }
        scripted::kill_anim_script(world, id);
        crate::script::runtime::deliver_external(world, i64::from(now));
        crate::script::runtime::run_current_threads(world, i64::from(now));
        scripted::finish_previous(world, id, i64::from(now))?;
        let species = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Some(actor) = runtime.actors.get_mut(&id) else {
                continue;
            };
            if !actor.alive || actor.scripted_control.is_some() {
                continue;
            }
            // Delivering runto_arrived can start the arrival transition in a
            // waiting GSC thread. Re-read ownership after the prior callbacks.
            if actor.arrival.transitioning() {
                kind = scripted::ControlKind::CoverArrival;
                moving = false;
                actor.move_mode = 0;
            }
            if let Some(yaw) = grenades::facing(actor).or_else(|| actor.arrival.yaw()) {
                actor.orientation.select_arrival(yaw);
            } else if moving {
                if actor.reacquire.active() {
                    actor
                        .orientation
                        .select_reacquire(actor.reacquire.face_enemy());
                } else {
                    actor.orientation.select_move();
                }
            } else {
                actor.orientation.select_stop();
            }
            actor.orientation.clear_script();
            actor.anim_script_kind = Some(kind);
            actor.species
        };
        let module = match species {
            Species::Human => format!("animscripts/{}", kind.name()),
            Species::Dog => format!("animscripts/dog/dog_{}", kind.name()),
            Species::Civilian => format!("animscripts/civilian/civilian_{}", kind.name()),
        };
        let serial = crate::script::runtime::run_now_owned(
            world,
            &format!("{module}::main"),
            Value::Object(id),
            vec![],
            i64::from(now),
        )
        .map_err(|e| e.to_string())?;
        if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id) {
            actor.anim_script_thread = Some(serial);
            if kind == scripted::ControlKind::CoverArrival {
                arrival::started(actor, serial);
            }
        }
    }
    Ok(())
}
