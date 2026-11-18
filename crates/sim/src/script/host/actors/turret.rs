use super::super::{entities::Link, turrets};
use super::*;
use glam::Vec3;

pub(crate) mod pose;

#[derive(Clone, Debug, Default)]
pub(crate) struct Control {
    pub requested: Option<u64>,
    pub(super) grid: Option<std::sync::Arc<pose::AimGrid>>,
    pub(super) placement: Option<pose::Placement>,
}

pub(crate) fn active(runtime: &RoundScript, id: u64) -> Option<u64> {
    let object = runtime.actors.get(&id)?.turret.requested?;
    (runtime.engine.turrets.get(&object)?.active_user == Some(id)).then_some(object)
}

fn position(world: &mut World, id: u64) -> Result<Vec3, String> {
    match super::super::players::entity_field(world, id, "origin") {
        Value::Vector(origin) => Ok(Vec3::from_array(origin)),
        _ => Err("turret user or turret has no origin".into()),
    }
}

fn can_use(world: &mut World, id: u64, object: u64) -> Result<bool, String> {
    let runtime = world.resource::<RoundScript>();
    let actor = &runtime.actors[&id];
    let turret = &runtime.engine.turrets[&object];
    if active(runtime, id) == Some(object) {
        return Ok(true);
    }
    if !actor.alive || actor.delayed_death || turret.active_user.is_some() || !turret.operable {
        return Ok(false);
    }
    if super::super::players::links::validate_parent(world, id, object).is_err() {
        return Ok(false);
    }
    let current = active(runtime, id);
    if let Some(current) = current {
        let actor = position(world, id)?;
        if actor
            .truncate()
            .distance_squared(position(world, current)?.truncate())
            <= actor
                .truncate()
                .distance_squared(position(world, object)?.truncate())
        {
            return Ok(false);
        }
    }
    turrets::actor_can_engage(world, id, object)
}

fn attach(
    world: &mut World,
    id: u64,
    parent: u64,
    tag: Option<crate::script::Arc<str>>,
) -> Result<(), String> {
    super::super::players::links::validate_parent(world, id, parent)?;
    let (base, axis) = super::super::players::link_parent_pose(world, parent, tag.as_deref())?;
    let origin = position(world, id)?;
    let Value::Vector(angles) = super::super::players::entity_field(world, id, "angles") else {
        return Err("turret user has no angles".into());
    };
    let delta = origin - Vec3::from_array(base);
    let local = axis.map(|direction| delta.dot(Vec3::from_array(direction)));
    let rotation = super::super::players::links::relative(math_iw4::angles_to_axis(angles), axis);
    world
        .resource_mut::<RoundScript>()
        .entities
        .get_mut(&id)
        .unwrap()
        .linked_to = Some(Link {
        parent,
        tag,
        origin: local,
        angles: math_iw4::axis_to_angles(rotation),
        scripted_parent: None,
        blend: None,
    });
    Ok(())
}

fn release(world: &mut World, id: u64, object: u64) -> Result<(), String> {
    let parent = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Some(turret) = runtime.engine.turrets.get_mut(&object) else {
            return Ok(());
        };
        if turret.active_user != Some(id) {
            return Ok(());
        }
        turret.active_user = None;
        runtime
            .entities
            .get(&object)
            .and_then(|entity| entity.linked_to.clone())
    };
    let linked = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .and_then(|entity| entity.linked_to.as_ref())
        .is_some_and(|link| link.parent == object);
    if linked {
        if let Some(parent) = parent {
            attach(world, id, parent.parent, parent.tag)?;
        } else {
            world
                .resource_mut::<RoundScript>()
                .entities
                .get_mut(&id)
                .unwrap()
                .linked_to = None;
        }
    }
    crate::script::runtime::raise(world, Value::Object(object), "turretstatechange", vec![]);
    Ok(())
}

pub(crate) fn stop(world: &mut World, id: u64) -> Result<(), String> {
    // Release by user as well as request: a failed/superseded request must not
    // leave the previous turret busy forever.
    let owned: Vec<_> = world
        .resource::<RoundScript>()
        .engine
        .turrets
        .iter()
        .filter_map(|(&object, turret)| (turret.active_user == Some(id)).then_some(object))
        .collect();
    for object in owned {
        release(world, id, object)?;
    }
    if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id) {
        actor.turret.requested = None;
        actor.turret.placement = None;
    }
    if world
        .resource::<RoundScript>()
        .actors
        .get(&id)
        .is_some_and(|actor| actor.anim_script_kind == Some(scripted::ControlKind::Turret))
    {
        scripted::kill_anim_script(world, id);
    }
    Ok(())
}

pub(crate) fn dissociate(runtime: &mut RoundScript, id: u64) {
    for (&object, turret) in &mut runtime.engine.turrets {
        if turret.active_user == Some(id) {
            turret.active_user = None;
            runtime.pending_notifies.push((
                Value::Object(object),
                "turretstatechange".into(),
                vec![],
            ));
        }
        if turret.owner == Some(id) {
            turret.owner = None;
        }
    }
    for (&actor_id, actor) in &mut runtime.actors {
        if actor.turret.requested == Some(id) {
            actor.turret.requested = None;
            actor.turret.placement = None;
            if let Some(entity) = runtime.entities.get_mut(&actor_id)
                && entity
                    .linked_to
                    .as_ref()
                    .is_some_and(|link| link.parent == id)
            {
                entity.linked_to = None;
            }
            if actor.anim_script_kind == Some(scripted::ControlKind::Turret) {
                actor.previous_anim_script = actor.anim_script_kind.take();
                actor.pending_anim_end = actor.previous_anim_script;
                actor.anim_script_thread = None;
                runtime.pending_notifies.push((
                    Value::Object(actor_id),
                    "killanimscript".into(),
                    vec![],
                ));
            }
        }
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    pose::register(registry);
    registry.register(
        Namespace::Method,
        "canuseturret",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let object = turrets::turret_of(world, arg(args, 0)?)?;
            Ok(Value::Int(can_use(world, id, object)?.into()))
        },
    );
    registry.register(Namespace::Method, "useturret", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let object = turrets::turret_of(world, arg(args, 0)?)?;
        let allowed = can_use(world, id, object)?;
        let previous = world.resource::<RoundScript>().actors[&id].turret.requested;
        if previous != allowed.then_some(object) {
            stop(world, id)?;
        }
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .turret
            .requested = allowed.then_some(object);
        Ok(Value::Int(allowed.into()))
    });
    registry.register(Namespace::Method, "getturret", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        Ok(active(world.resource::<RoundScript>(), id).map_or(Value::Undefined, Value::Object))
    });
    registry.register(Namespace::Method, "stopuseturret", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        stop(world, id)?;
        Ok(Value::Undefined)
    });
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| actor.turret.requested.map(|_| (actor.slot, id)))
        .collect();
    let mut ids = ids;
    ids.sort_unstable();
    for (_, id) in ids {
        let runtime = world.resource::<RoundScript>();
        let Some(actor) = runtime.actors.get(&id) else {
            continue;
        };
        let Some(object) = actor.turret.requested else {
            continue;
        };
        if !actor.alive
            || actor.delayed_death
            || actor.scripted_control.is_some()
            || !runtime.engine.turrets.contains_key(&object)
            || !runtime.entities.contains_key(&object)
        {
            stop(world, id)?;
            continue;
        }
        if !actor.script_initialized {
            continue;
        }
        let mounted = active(runtime, id).is_some();
        let linked = runtime.entities[&id]
            .linked_to
            .as_ref()
            .is_some_and(|link| link.parent == object);
        let ignore_goals = runtime.engine.turrets[&object].ignore_goals;
        goals::update_one(world, id, now)?;
        let goal = world.resource::<RoundScript>().actors[&id]
            .goals
            .code
            .clone();
        let at = position(world, object)?.to_array();
        if (mounted && !linked) || (!ignore_goals && !goals::contains(world, &goal, at)?) {
            stop(world, id)?;
            continue;
        }
        if mounted {
            if turrets::actor_should_detach(world, id, object)? {
                stop(world, id)?;
            }
            continue;
        }
        if !can_use(world, id, object)? {
            stop(world, id)?;
            continue;
        }
        let script = {
            let weapon = world.resource::<RoundScript>().engine.turrets[&object].weapon;
            crate::frame::FrameWorld::from_world(world)
                .content()
                .turret_ai_script(weapon)
                .ok_or_else(|| format!("turret weapon {weapon} has no AI animation script"))?
                .to_owned()
        };
        let callback = format!("animscripts/{script}::main");
        if world
            .resource::<RoundScript>()
            .program
            .as_ref()
            .is_none_or(|program| !program.names.contains_key(&callback))
        {
            return Err(format!(
                "turret AI animation callback '{callback}' was not loaded"
            ));
        }
        // Deliver the old animation's endon before starting the weapon callback;
        // its child threads can outlive main and must not be restarted each tick.
        scripted::kill_anim_script(world, id);
        crate::script::runtime::deliver_external(world, i64::from(now));
        crate::script::runtime::run_current_threads(world, i64::from(now));
        scripted::finish_previous(world, id, i64::from(now))?;
        let runtime = world.resource::<RoundScript>();
        if runtime.actors.get(&id).is_none_or(|actor| {
            !actor.alive
                || actor.turret.requested != Some(object)
                || actor.scripted_control.is_some()
        }) || !runtime.entities.contains_key(&object)
            || !runtime.engine.turrets.contains_key(&object)
        {
            continue;
        }
        if !can_use(world, id, object)? {
            stop(world, id)?;
            continue;
        }
        attach(world, id, object, None)?;
        grenades::cancel(world, id)?;
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            let turret = runtime.engine.turrets.get_mut(&object).unwrap();
            turret.active_user = Some(id);
            let actor = runtime.actors.get_mut(&id).unwrap();
            cover::clear(actor);
            arrival::clear(actor);
            reacquire::clear(actor, false);
            actor.navigation = Default::default();
            actor.motion.velocity = [0.; 3];
            actor.motion.wish_delta = [0.; 3];
            actor.move_mode = 0;
            actor.anim_script_kind = Some(scripted::ControlKind::Turret);
        }
        crate::script::runtime::raise(world, Value::Object(object), "turretstatechange", vec![]);
        let serial = crate::script::runtime::run_now_owned(
            world,
            &callback,
            Value::Object(id),
            vec![],
            i64::from(now),
        )
        .map_err(|error| error.to_string())?;
        if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id)
            && actor.anim_script_kind == Some(scripted::ControlKind::Turret)
        {
            actor.anim_script_thread = Some(serial);
        }
    }
    Ok(())
}
