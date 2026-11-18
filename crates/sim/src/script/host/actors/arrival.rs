use super::*;
use glam::Vec2;

#[derive(Clone, Debug, Default)]
pub(crate) struct Arrival {
    run_to: Option<RunTo>,
    last_run_to: Option<goals::Goal>,
    run_to_flag: i32,
    transition: Option<Transition>,
}
#[derive(Clone, Debug, PartialEq)]
struct Context {
    goal: goals::Goal,
    entity: Option<u64>,
    code_position: [f32; 3],
}
impl Context {
    fn of(actor: &Actor) -> Self {
        Self {
            goal: actor.goals.script.clone(),
            entity: actor.goals.entity,
            code_position: actor.goals.code.position,
        }
    }
}
#[derive(Clone, Debug)]
struct RunTo {
    goal: goals::Goal,
    context: Context,
}
#[derive(Clone, Debug)]
struct Transition {
    blend: EntryBlend,
    yaw: f32,
    context: Context,
    previous_mode: anim_mode::Mode,
    thread: Option<u64>,
}
impl Arrival {
    pub(super) fn active(&self) -> bool {
        self.run_to.is_some() || self.transition.is_some()
    }
    pub(super) fn transitioning(&self) -> bool {
        self.transition.is_some()
    }
    pub(super) fn running_flag(&self) -> i32 {
        self.run_to_flag
    }
    pub(super) fn running_to(&self) -> bool {
        self.run_to.is_some()
    }
    pub(super) fn movement_goal(&self) -> Option<&goals::Goal> {
        self.run_to.as_ref().map(|run| &run.goal)
    }
    pub(super) fn yaw(&self) -> Option<f32> {
        self.transition.as_ref().map(|state| state.yaw)
    }
}
pub(super) fn set_running(actor: &mut Actor, value: i32) -> Result<(), String> {
    if value == 0 {
        if actor.arrival.run_to.take().is_some() {
            actor.navigation = Default::default();
            actor.move_mode = 0;
        }
        actor.arrival.run_to_flag = 0;
    } else if actor.arrival.run_to.is_some() {
        actor.arrival.run_to_flag = value;
    } else {
        if actor.arrival.transitioning() {
            return Err("cannot reactivate a run-to request during cover arrival".into());
        }
        let goal = actor
            .arrival
            .last_run_to
            .clone()
            .ok_or("goingtoruntopos requires a previous SetRunToPos destination")?;
        actor.arrival.run_to = Some(RunTo {
            goal,
            context: Context::of(actor),
        });
        actor.arrival.run_to_flag = value;
        actor.navigation = Default::default();
    }
    Ok(())
}
fn origin(world: &mut World, id: u64) -> Result<[f32; 3], String> {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(at) if at.into_iter().all(f32::is_finite) => Ok(at),
        _ => Err("arrival requires a finite actor origin".into()),
    }
}
fn available(world: &World, id: u64) -> bool {
    let r = world.resource::<RoundScript>();
    let a = &r.actors[&id];
    r.can_receive_call(&id)
        && a.alive
        && !a.delayed_death
        && a.physics_initialized
        && a.scripted_control.is_none()
        && anim_mode::effective(a) != anim_mode::Mode::PointRelative
        && r.entities[&id].linked_to.is_none()
}
pub(super) fn clear(actor: &mut Actor) {
    actor.arrival.run_to_flag = 0;
    if let Some(transition) = actor.arrival.transition.take() {
        // Do not restore locomotion policy over a new scripted/death owner.
        if actor.alive
            && actor.scripted_control.is_none()
            && (actor.anim_script_kind == Some(scripted::ControlKind::CoverArrival)
                || transition.thread.is_none())
            && actor.script_anim_mode == anim_mode::Mode::None
        {
            actor.script_anim_mode = transition.previous_mode;
        }
        if let Some(state) = actor
            .initial_state
            .as_mut()
            .filter(|state| state.state == 5)
        {
            state.state = 1;
            state.substate = 100;
        }
    }
    if actor.arrival.run_to.take().is_some() {
        actor.navigation = Default::default();
        actor.move_mode = 0;
    }
}
fn run_to(world: &mut World, id: u64, at: [f32; 3]) -> Result<(), String> {
    if !at.into_iter().all(f32::is_finite) {
        return Err("SetRunToPos position must be finite".into());
    }
    let now = super::super::players::now_ms(world) as i32;
    goals::update_one(world, id, now)?;
    let changed = world.resource::<RoundScript>().actors[&id]
        .arrival
        .run_to
        .as_ref()
        .is_none_or(|run| run.goal.position != at);
    if world.resource::<RoundScript>().actors[&id]
        .arrival
        .run_to
        .as_ref()
        .is_some_and(|run| {
            run.goal.position == at
                && run.context == Context::of(&world.resource::<RoundScript>().actors[&id])
        })
    {
        return Ok(());
    }
    let from = origin(world, id)?;
    let goal = goals::Goal {
        position: at,
        height: spatial::ACTOR_BOUNDS.half_size[2],
        ..Default::default()
    };
    let route = navigation::supported_plan(world, id, from, &goal, false);
    {
        let mut r = world.resource_mut::<RoundScript>();
        let a = r.actors.get_mut(&id).unwrap();
        clear(a);
        reacquire::clear(a, false);
        a.arrival.last_run_to = Some(goal.clone());
        a.arrival.run_to_flag = 1;
        a.arrival.run_to = Some(RunTo {
            goal,
            context: Context::of(a),
        });
        a.navigation = Default::default();
        if let Some(route) = route {
            a.navigation = route;
        }
    }
    if changed {
        crate::script::runtime::raise(world, Value::Object(id), "goal_changed", vec![]);
    }
    Ok(())
}
fn start(world: &mut World, id: u64, at: [f32; 3], yaw: f32) -> Result<(), String> {
    if !at.into_iter().chain([yaw]).all(f32::is_finite) {
        return Err("StartCoverArrival placement must be finite".into());
    }
    if !available(world, id) {
        return Ok(());
    }
    let actor = &world.resource::<RoundScript>().actors[&id];
    if actor
        .initial_state
        .as_ref()
        .is_some_and(|state| !matches!(state.state, 1 | 2 | 5))
    {
        return Ok(());
    }
    if actor.species != Species::Human {
        return Err("cover arrival requires a supported human arrival animscript".into());
    }
    let supported = world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|p| p.names.contains_key("animscripts/cover_arrival::main"));
    if !supported {
        return Err("animscripts/cover_arrival::main is not loaded".into());
    }
    goals::update_one(world, id, super::super::players::now_ms(world) as i32)?;
    let from = origin(world, id)?;
    let mut r = world.resource_mut::<RoundScript>();
    let a = r.actors.get_mut(&id).unwrap();
    clear(a);
    reacquire::clear(a, false);
    let previous_mode = a.script_anim_mode;
    a.arrival.transition = Some(Transition {
        blend: EntryBlend::new(from, at),
        yaw: math_iw4::angle_normalize_360(yaw),
        context: Context::of(a),
        previous_mode,
        thread: None,
    });
    a.navigation = Default::default();
    a.move_mode = 0;
    a.script_anim_mode = anim_mode::Mode::None;
    if let Some(state) = a.initial_state.as_mut() {
        state.state = 5;
    }
    Ok(())
}
pub(super) fn started(actor: &mut Actor, serial: u64) {
    if let Some(state) = actor.arrival.transition.as_mut() {
        state.thread = Some(serial);
    }
}
// Physical entry correction shared by cover and explicitly requested traversal.
// Only accepted movement consumes error; blocked motion cannot bank a launch.
#[derive(Clone, Debug)]
pub(super) struct EntryBlend {
    error: Vec2,
    correction_requested: Vec2,
    motion_requested: Vec2,
}
impl EntryBlend {
    pub(super) fn new(from: [f32; 3], to: [f32; 3]) -> Self {
        Self {
            error: Vec2::new(to[0] - from[0], to[1] - from[1]),
            correction_requested: Vec2::ZERO,
            motion_requested: Vec2::ZERO,
        }
    }
    pub(super) fn motion(&mut self, root: [f32; 3], dt: f32) -> [f32; 3] {
        const ENTRY_BLEND_SECONDS: f32 = 0.15;
        let correction = self.error * (1. - (-dt / ENTRY_BLEND_SECONDS).exp());
        let requested = Vec2::new(root[0], root[1]) + correction;
        self.correction_requested = correction;
        self.motion_requested = requested;
        [requested.x, requested.y, root[2]]
    }
    pub(super) fn accepted(&mut self, from: [f32; 3], to: [f32; 3]) {
        let actual = Vec2::new(to[0] - from[0], to[1] - from[1]);
        let requested = self.motion_requested;
        let fraction = if requested.length_squared() > f32::EPSILON {
            (actual.dot(requested) / requested.length_squared()).clamp(0., 1.)
        } else {
            0.
        };
        self.error -= self.correction_requested * fraction;
    }
}
pub(super) fn motion(actor: &mut Actor, root: [f32; 3], dt: f32) -> [f32; 3] {
    actor
        .arrival
        .transition
        .as_mut()
        .filter(|s| s.thread.is_some())
        .map_or(root, |s| s.blend.motion(root, dt))
}
pub(super) fn accepted(actor: &mut Actor, from: [f32; 3], to: [f32; 3]) {
    if let Some(state) = actor
        .arrival
        .transition
        .as_mut()
        .filter(|s| s.thread.is_some())
    {
        state.blend.accepted(from, to);
    }
}
pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_arrival", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, a)| a.arrival.active().then_some(id))
        .collect();
    let mut arrived = false;
    for id in ids {
        goals::update_one(world, id, now)?;
        let (context, transition, run) = {
            let a = &world.resource::<RoundScript>().actors[&id];
            (
                Context::of(a),
                a.arrival.transition.clone(),
                a.arrival.run_to.clone(),
            )
        };
        let actor = &world.resource::<RoundScript>().actors[&id];
        let retiring = !world.resource::<RoundScript>().can_receive_call(&id)
            || actor.death.is_some()
            || actor.delayed_death;
        if retiring
            || (transition.is_some() && !available(world, id))
            || transition.as_ref().is_some_and(|t| t.context != context)
            || run.as_ref().is_some_and(|r| r.context != context)
        {
            clear(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
            continue;
        }
        if let Some(transition) = transition {
            if let Some(serial) = transition.thread {
                let current = world.resource::<RoundScript>().actors[&id].anim_script_kind
                    == Some(scripted::ControlKind::CoverArrival);
                if !current || !crate::script::runtime::engine_thread_alive(world, serial) {
                    clear(
                        world
                            .resource_mut::<RoundScript>()
                            .actors
                            .get_mut(&id)
                            .unwrap(),
                    );
                }
            }
            continue;
        }
        if let Some(run) = run {
            // SetRunToPos records intent even during initialization, linking
            // or scripted control. Locomotion waits for its normal owner.
            if !available(world, id) {
                continue;
            }
            let at = origin(world, id)?;
            if goals::contains(world, &run.goal, at)? {
                clear(
                    world
                        .resource_mut::<RoundScript>()
                        .actors
                        .get_mut(&id)
                        .unwrap(),
                );
                crate::script::runtime::raise(world, Value::Object(id), "runto_arrived", vec![]);
                arrived = true;
            } else {
                let path = world.resource::<RoundScript>().actors[&id]
                    .navigation
                    .clone();
                if !navigation::route_supported(world, id, &path, at, false) {
                    let route = navigation::supported_plan(world, id, at, &run.goal, false);
                    let mut runtime = world.resource_mut::<RoundScript>();
                    let actor = runtime.actors.get_mut(&id).unwrap();
                    actor.navigation = Default::default();
                    if let Some(route) = route {
                        actor.navigation = route;
                    }
                }
            }
        }
    }
    if arrived {
        // A waiting approach thread may immediately request the arrival
        // animscript. Deliver before cover/path selection releases its claim
        // or restarts locomotion for the old destination.
        crate::script::runtime::deliver_external(world, i64::from(now));
        crate::script::runtime::run_current_threads(world, i64::from(now));
    }
    Ok(())
}
fn exit_follows_path(world: &mut World, id: u64, exit: [f32; 3]) -> Result<bool, String> {
    if !exit.into_iter().all(f32::is_finite) {
        return Err("cover exit position must be finite".into());
    }
    let a = &world.resource::<RoundScript>().actors[&id];
    if !available(world, id) || a.cover.previous_node().is_none() || a.navigation.point().is_none()
    {
        return Ok(false);
    }
    let path = a.navigation.clone();
    let from = origin(world, id)?;
    let delta = Vec2::new(exit[0] - from[0], exit[1] - from[1]);
    let lookahead = delta.length() + spatial::ACTOR_BOUNDS.half_size[0] * 2.;
    let mut join = None;
    for &point in path.remaining_points() {
        let toward = Vec2::new(point[0] - from[0], point[1] - from[1]);
        if toward.length_squared() <= f32::EPSILON {
            continue;
        }
        if delta.dot(toward) < 0. {
            return Ok(false);
        }
        join = Some(point);
        if toward.length() >= lookahead {
            break;
        }
    }
    let Some(join) = join else {
        return Ok(false);
    };
    // A lateral cover exit may temporarily increase distance to the goal.
    // Its route projection must not reverse, and both physical joins must be
    // supported; requiring Euclidean progress would reject valid side exits.
    Ok(navigation::direct_supported(world, id, from, exit, false)
        && navigation::direct_supported(world, id, exit, join, false))
}
pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "setruntopos", |w, r, args| {
        let id = actor_id(w, r)?;
        run_to(w, id, super::super::args::vector(args, 0)?)?;
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Method, "startcoverarrival", |w, r, args| {
        let id = actor_id(w, r)?;
        start(w, id, super::super::args::vector(args, 0)?, float(args, 1)?)?;
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "checkcoverexitposwithpath",
        |w, r, args| {
            let id = actor_id(w, r)?;
            exit_follows_path(w, id, super::super::args::vector(args, 0)?)
                .map(|ok| Value::Int(ok.into()))
        },
    );
}
