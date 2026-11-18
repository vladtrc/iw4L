use super::*;

#[derive(Clone, Debug, Default)]
pub(crate) struct Traversal {
    active: Option<Active>,
    mode: TraverseMode,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TraverseMode {
    Gravity,
    #[default]
    NoGravity,
    NoClip,
}
#[derive(Clone, Debug)]
struct Active {
    pair: (u16, u16),
    thread: Option<u64>,
    goal: goals::Goal,
    entity: Option<u64>,
    previous_state: u8,
    previous_mode: Option<anim_mode::Mode>,
    entry: Option<Entry>,
}
#[derive(Clone, Debug)]
struct Entry {
    target: [f32; 3],
    yaw: f32,
    blend: arrival::EntryBlend,
}
impl Traversal {
    pub(super) fn active(&self) -> bool {
        self.active.is_some()
    }
    pub(super) fn pending(&self) -> bool {
        self.active() && !self.running()
    }
    pub(super) fn running(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.thread.is_some())
    }
    pub(super) fn pair(&self) -> Option<(u16, u16)> {
        self.active.as_ref().map(|a| a.pair)
    }
    pub(super) fn mode(&self) -> anim_mode::Mode {
        match self.mode {
            TraverseMode::Gravity => anim_mode::Mode::Gravity,
            TraverseMode::NoGravity => anim_mode::Mode::NoGravity,
            TraverseMode::NoClip => anim_mode::Mode::NoClip,
        }
    }
}

pub(super) fn can_plan(actor: &Actor) -> bool {
    !actor.arrival.active()
        && !actor.reacquire.active()
        && actor.badplace_escape.is_none()
        && actor.cover.movement_goal().is_none()
        && actor.scripted_control.is_none()
}

pub(super) fn authored_pair(world: &World, from: u16, to: u16) -> bool {
    let graph = &world.resource::<RoundScript>().path_nodes;
    graph.at(from).is_some_and(|node| {
        node.kind == 16 && node.animscript.as_ref().is_some_and(|s| !s.is_empty())
    }) && graph.at(to).is_some()
        && graph.links.get(usize::from(from)).is_some_and(|links| {
            links
                .iter()
                .any(|l| l.node == to && l.negotiation && l.disconnect_count == 0)
        })
}
fn pair(world: &World, id: u64) -> Option<(u16, u16)> {
    let r = world.resource::<RoundScript>();
    r.actors[&id]
        .traversal
        .pair()
        .or_else(|| r.actors[&id].navigation.negotiation(&r.path_nodes))
}

pub(super) fn clear(actor: &mut Actor) {
    if let Some(active) = actor.traversal.active.take() {
        if let Some(previous) = active.previous_mode
            && actor.alive
            && actor.scripted_control.is_none()
            && actor.script_anim_mode == anim_mode::Mode::None
            && (actor
                .anim_script_kind
                .is_none_or(|k| k == scripted::ControlKind::Traverse)
                || active.thread.is_none())
        {
            actor.script_anim_mode = previous;
        }
        actor.navigation = Default::default();
        actor.motion.stop();
        actor.motion.fall_velocity = 0.;
        actor.move_mode = 0;
        if let Some(state) = &mut actor.initial_state
            && state.state == 11
        {
            state.state = active.previous_state;
        }
    }
    actor.traversal.mode = TraverseMode::NoGravity;
}

pub(crate) fn advance(world: &mut World, _now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_traversal", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, a)| a.traversal.active().then_some(id))
        .collect();
    for id in ids {
        let r = world.resource::<RoundScript>();
        let a = &r.actors[&id];
        let active = a.traversal.active.as_ref().unwrap();
        let interrupted = !r.can_receive_call(&id)
            || !a.alive
            || a.delayed_death
            || a.scripted_control.is_some()
            || r.entities[&id].linked_to.is_some()
            || (active.thread.is_some()
                && a.anim_script_kind != Some(scripted::ControlKind::Traverse))
            || a.goals.script != active.goal
            || a.goals.entity != active.entity;
        let finished = active
            .thread
            .is_some_and(|serial| !crate::script::runtime::engine_thread_alive(world, serial));
        if interrupted || finished {
            if a.anim_script_kind == Some(scripted::ControlKind::Traverse) {
                scripted::kill_anim_script(world, id);
            }
            clear(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
        }
    }
    Ok(())
}

/// Called after ordinary route preparation, before move/stop script selection.
/// The preview ends at the special edge's start, so accepted approach movement
/// reaches this position rather than cutting a corner across the obstacle.
pub(super) fn try_start(world: &mut World, id: u64, now: i32) -> Result<bool, String> {
    if world.resource::<RoundScript>().actors[&id]
        .traversal
        .running()
    {
        return Ok(true);
    }
    let pending = world.resource::<RoundScript>().actors[&id]
        .traversal
        .active();
    let Some((from, to)) = pair(world, id) else {
        return Ok(false);
    };
    if !authored_pair(world, from, to) {
        if pending {
            clear(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
        }
        return Ok(false);
    }
    let (origin, angles, start, module, goal, entity) = {
        let mut r = world.resource_mut::<RoundScript>();
        let a = &r.actors[&id];
        if !a.alive
            || a.delayed_death
            || a.scripted_control.is_some()
            || anim_mode::effective(a) == anim_mode::Mode::PointRelative
            || a.arrival.active()
            || a.reacquire.active()
            || a.badplace_escape.is_some()
            || (!pending && anim_mode::effective(a).uses_root_motion())
            || r.entities[&id].linked_to.is_some()
        {
            return Ok(false);
        }
        let mut start = r.path_nodes.at(from).unwrap().clone();
        if let Some(entry) = r.actors[&id]
            .traversal
            .active
            .as_ref()
            .and_then(|s| s.entry.as_ref())
        {
            start.yaw = entry.yaw;
        }
        let Value::Vector(origin) = r.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let Value::Vector(angles) = r.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        let goal = r.actors[&id].goals.script.clone();
        let entity = r.actors[&id].goals.entity;
        let name = start
            .animscript
            .as_ref()
            .unwrap()
            .replace('\\', "/")
            .to_ascii_lowercase();
        (
            origin,
            angles,
            start,
            format!("animscripts/traverse/{name}::main"),
            goal,
            entity,
        )
    };
    // One unit of horizontal acceptance keeps the authored animation aligned;
    // the vertical allowance is the ordinary locomotion step height.
    if !pending
        && ((origin[0] - start.origin[0]).hypot(origin[1] - start.origin[1]) > 1.
            || (origin[2] - start.origin[2]).abs() > 18.)
    {
        return Ok(false);
    }
    let yaw_error = (start.yaw - angles[1] + 180.).rem_euclid(360.) - 180.;
    if !pending && yaw_error.abs() > 1. {
        let mut r = world.resource_mut::<RoundScript>();
        let a = r.actors.get_mut(&id).unwrap();
        a.move_mode = 0;
        a.orientation.clear_script();
        a.orientation.select_arrival(start.yaw);
        return Ok(true);
    }
    if !world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|p| p.names.contains_key(&module))
    {
        return Err(format!(
            "negotiation node {from} requires missing script {module}"
        ));
    }
    scripted::kill_anim_script(world, id);
    crate::script::runtime::deliver_external(world, i64::from(now));
    crate::script::runtime::run_current_threads(world, i64::from(now));
    scripted::finish_previous(world, id, i64::from(now))?;
    if !authored_pair(world, from, to) || pair(world, id) != Some((from, to)) {
        return Ok(false);
    }
    {
        let mut r = world.resource_mut::<RoundScript>();
        let moved = r.object_field(id, "origin") != Value::Vector(origin);
        let linked = r.entities[&id].linked_to.is_some();
        let a = r.actors.get_mut(&id).unwrap();
        if moved
            || linked
            || !a.alive
            || a.delayed_death
            || a.scripted_control.is_some()
            || anim_mode::effective(a) == anim_mode::Mode::PointRelative
            || a.goals.script != goal
            || a.goals.entity != entity
        {
            return Ok(false);
        }
        let pending = a.traversal.active.take();
        let previous_state = pending.as_ref().map_or_else(
            || a.initial_state.as_ref().map_or(1, |s| s.state),
            |s| s.previous_state,
        );
        let previous_mode = pending.as_ref().and_then(|s| s.previous_mode);
        let entry = pending.and_then(|s| s.entry).map(|mut e| {
            e.blend = arrival::EntryBlend::new(origin, e.target);
            e
        });
        a.traversal = Traversal {
            active: Some(Active {
                pair: (from, to),
                thread: None,
                goal: a.goals.script.clone(),
                entity: a.goals.entity,
                previous_state,
                previous_mode,
                entry,
            }),
            mode: TraverseMode::NoGravity,
        };
        a.motion.stop();
        a.motion.fall_velocity = 0.;
        a.move_mode = 0;
        a.orientation.clear_script();
        a.orientation.select_arrival(start.yaw);
        a.anim_script_kind = Some(scripted::ControlKind::Traverse);
        if let Some(state) = &mut a.initial_state {
            state.state = 11;
        }
    }
    let serial = crate::script::runtime::run_now_owned(
        world,
        &module,
        Value::Object(id),
        vec![],
        i64::from(now),
    )
    .map_err(|e| e.to_string())?;
    let mut r = world.resource_mut::<RoundScript>();
    let a = r.actors.get_mut(&id).unwrap();
    if let Some(active) = &mut a.traversal.active {
        active.thread = Some(serial);
        a.anim_script_thread = Some(serial);
    }
    Ok(true)
}

fn request_arrival(world: &mut World, id: u64, target: [f32; 3], yaw: f32) -> Result<(), String> {
    if !target.into_iter().chain([yaw]).all(f32::is_finite) {
        return Err("StartTraverseArrival placement must be finite".into());
    }
    let r = world.resource::<RoundScript>();
    let a = &r.actors[&id];
    if !r.can_receive_call(&id)
        || !a.alive
        || a.delayed_death
        || !a.physics_initialized
        || a.scripted_control.is_some()
        || r.entities[&id].linked_to.is_some()
        || anim_mode::effective(a) == anim_mode::Mode::PointRelative
    {
        return Ok(());
    }
    let Some((from, to)) = pair(world, id) else {
        return Ok(());
    };
    if !authored_pair(world, from, to) {
        return Ok(());
    }
    let name = r
        .path_nodes
        .at(from)
        .unwrap()
        .animscript
        .as_ref()
        .unwrap()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let module = format!("animscripts/traverse/{name}::main");
    if !r
        .program
        .as_ref()
        .is_some_and(|p| p.names.contains_key(&module))
    {
        return Err(format!(
            "negotiation node {from} requires missing script {module}"
        ));
    }
    let mut r = world.resource_mut::<RoundScript>();
    let Value::Vector(origin) = r.object_field(id, "origin") else {
        return Err("actor origin is not a vector".into());
    };
    let a = r.actors.get_mut(&id).unwrap();
    arrival::clear(a);
    reacquire::clear(a, false);
    bad_places::release(a);
    let entry = Entry {
        target,
        yaw: math_iw4::angle_normalize_360(yaw),
        blend: arrival::EntryBlend::new(origin, target),
    };
    if let Some(active) = &mut a.traversal.active {
        active.entry = Some(entry);
        active.previous_mode.get_or_insert(a.script_anim_mode);
    } else {
        a.traversal = Traversal {
            active: Some(Active {
                pair: (from, to),
                thread: None,
                goal: a.goals.script.clone(),
                entity: a.goals.entity,
                previous_state: a.initial_state.as_ref().map_or(1, |s| s.state),
                previous_mode: Some(a.script_anim_mode),
                entry: Some(entry),
            }),
            mode: TraverseMode::NoGravity,
        };
    }
    a.script_anim_mode = anim_mode::Mode::None;
    a.navigation = Default::default();
    a.motion.stop();
    a.motion.fall_velocity = 0.;
    a.move_mode = 0;
    a.orientation.clear_script();
    a.orientation.select_arrival(yaw);
    if let Some(state) = &mut a.initial_state {
        state.state = 11;
    }
    Ok(())
}
pub(super) fn motion(actor: &mut Actor, root: [f32; 3], dt: f32) -> [f32; 3] {
    actor
        .traversal
        .active
        .as_mut()
        .filter(|s| s.thread.is_some())
        .and_then(|s| s.entry.as_mut())
        .map_or(root, |e| e.blend.motion(root, dt))
}
pub(super) fn accepted(actor: &mut Actor, from: [f32; 3], to: [f32; 3]) {
    if let Some(entry) = actor
        .traversal
        .active
        .as_mut()
        .filter(|s| s.thread.is_some())
        .and_then(|s| s.entry.as_mut())
    {
        entry.blend.accepted(from, to);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "starttraversearrival",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            request_arrival(
                world,
                id,
                super::super::args::vector(args, 0)?,
                float(args, 1)?,
            )?;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "getnegotiationstartnode",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(pair(world, id).map_or(Value::Undefined, |(from, _)| {
                world.resource::<RoundScript>().path_nodes.value(from)
            }))
        },
    );
    registry.register(
        Namespace::Method,
        "getnegotiationendnode",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(pair(world, id).map_or(Value::Undefined, |(_, to)| {
                world.resource::<RoundScript>().path_nodes.value(to)
            }))
        },
    );
    registry.register(
        Namespace::Method,
        "traversemode",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let mode = match text(args, 0)? {
                "gravity" => TraverseMode::Gravity,
                "nogravity" => TraverseMode::NoGravity,
                "noclip" => TraverseMode::NoClip,
                name => {
                    return Err(format!(
                        "unknown traverseMode '{name}'; expected gravity, nogravity or noclip"
                    ));
                }
            };
            let mut r = world.resource_mut::<RoundScript>();
            let a = r.actors.get_mut(&id).unwrap();
            if a.traversal.mode != mode {
                a.motion.fall_velocity = 0.;
            }
            a.traversal.mode = mode;
            Ok(Value::Undefined)
        },
    );
}
