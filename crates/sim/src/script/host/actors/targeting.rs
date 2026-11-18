use super::super::args::optional;
use super::*;

pub(crate) const ALERT_LEVEL: u8 = 2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EntityTarget {
    pub entity: u64,
    pub priority: f32,
}

pub(super) fn point(world: &mut World, target: u64) -> Result<[f32; 3], String> {
    if world
        .resource::<RoundScript>()
        .sentients
        .contains_key(&target)
    {
        return perception::eye(world, target).map(|(point, _)| point);
    }
    if let Some((point, _)) = super::super::presence::tag_world(world, target, "tag_eye") {
        return Ok(point);
    }
    let Value::Vector(origin) = super::super::players::entity_field(world, target, "origin") else {
        return Err("actor target has no position".into());
    };
    let Value::Vector(angles) = super::super::players::entity_field(world, target, "angles") else {
        return Err("actor target has no orientation".into());
    };
    let (mins, maxs) = super::super::triggers::entity_bounds(world, target);
    let axis = math_iw4::angles_to_axis(angles);
    Ok(std::array::from_fn(|i| {
        origin[i]
            + (0..3)
                .map(|j| axis[j][i] * (mins[j] + maxs[j]) * 0.5)
                .sum::<f32>()
    }))
}

pub(super) fn enemy(world: &mut World, id: u64) -> Option<u64> {
    let mut runtime = world.resource_mut::<RoundScript>();
    match runtime.object_field(id, "enemy") {
        Value::Object(target) if runtime.can_receive_call(&target) => Some(target),
        _ => None,
    }
}

// Derive the count from live target edges, so deletion and repeated selection
// cannot drift an increment/decrement counter. This is distinct from the
// script-writable client attackercount byte used by campaign damage.
pub(crate) fn attacker_count(runtime: &RoundScript, target: u64) -> usize {
    if !runtime.sentients.contains_key(&target) || !runtime.can_receive_call(&target) {
        return 0;
    }
    runtime
        .sentients
        .iter()
        .filter(|(source, sentient)| {
            sentient.target == Some(target)
                && runtime.can_receive_call(source)
                && runtime
                    .actors
                    .get(source)
                    .is_none_or(|actor| actor.alive && !actor.delayed_death)
        })
        .count()
}

const NEW_ENEMY_DELAY_MS: i32 = 1000;

fn transition(world: &mut World, id: u64, now: i32) {
    let (target, previous, due) = {
        let runtime = world.resource::<RoundScript>();
        let state = &runtime.sentients[&id];
        (state.target, state.announced_target, state.enemy_notify_at)
    };
    let changed = target != previous;
    if due.is_some_and(|deadline| changed || now >= deadline) {
        if let Some(previous) = previous {
            super::super::ai_events::new_enemy(world, id, previous, now);
        }
        world
            .resource_mut::<RoundScript>()
            .sentients
            .get_mut(&id)
            .unwrap()
            .enemy_notify_at = None;
    }
    if !changed {
        return;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let deadline = target
        .filter(|target| runtime.sentients.contains_key(target))
        .map(|_| now.saturating_add(NEW_ENEMY_DELAY_MS));
    let state = runtime.sentients.get_mut(&id).unwrap();
    state.announced_target = target;
    state.enemy_notify_at = deadline;
}

pub(super) fn set_enemy(world: &mut World, id: u64, target: Option<u64>) {
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "enemy",
        target.map_or(Value::Undefined, Value::Object),
    );
    let now = super::super::players::now_ms(world) as i32;
    transition(world, id, now);
}

pub(crate) fn dissociate(runtime: &mut RoundScript, owner: u64) {
    let mut changed = Vec::new();
    for (&id, state) in &mut runtime.sentients {
        if id == owner || state.target == Some(owner) {
            if state.target.take().is_some() && id != owner {
                changed.push(id);
            }
            state.announced_target = None;
            state.enemy_notify_at = None;
        } else if state.announced_target == Some(owner) {
            state.announced_target = None;
            state.enemy_notify_at = None;
        }
    }
    for (&id, actor) in &mut runtime.actors {
        if id == owner
            || actor
                .entity_target
                .is_some_and(|target| target.entity == owner)
        {
            actor.entity_target = None;
        }
        if id == owner || changed.contains(&id) {
            if super::reacquire::clear(actor, false) && id != owner {
                runtime
                    .pending_notifies
                    .push((Value::Object(id), "movemode".into(), vec![]));
            }
            actor.shot_check = None;
            actor.no_attacker_accuracy_mod = false;
            actor.sight.has_threatening_enemy = false;
        }
    }
    for id in changed {
        if runtime.can_receive_call(&id) {
            runtime
                .pending_notifies
                .push((Value::Object(id), "enemy".into(), vec![]));
        }
    }
}

fn flag_unattackable(world: &mut World, id: u64) {
    let Some(target) =
        enemy(world, id).filter(|t| world.resource::<RoundScript>().sentients.contains_key(t))
    else {
        return;
    };
    let now = super::super::players::now_ms(world) as i32;
    let mut r = world.resource_mut::<RoundScript>();
    let actor = r.actors.get_mut(&id).unwrap();
    if !actor.initial_state.as_ref().is_some_and(|s| s.state == 1) {
        return;
    }
    const RETRY_DELAY_MS: i32 = 5000;
    actor.sentient_info.entry(target).or_default().attack_time = now.saturating_add(RETRY_DELAY_MS);
    actor.initial_state.as_mut().unwrap().substate = 101;
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "flagenemyunattackable",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            flag_unattackable(world, id);
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "setentitytarget",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let target = entity_id(world, arg(args, 0)?)?;
            if world
                .resource::<RoundScript>()
                .sentients
                .contains_key(&target)
            {
                return Err("setentitytarget requires a non-sentient entity".into());
            }
            let priority = optional(args, 1, float)?.unwrap_or(1.0);
            if !priority.is_finite() || priority <= 0.0 || priority > 1.0 {
                return Err("entity target priority must be in (0, 1]".into());
            }
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .entity_target = Some(EntityTarget {
                entity: target,
                priority,
            });
            if priority == 1.0 {
                set_enemy(world, id, Some(target));
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "clearentitytarget",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            let previous = world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .entity_target
                .take();
            if previous.is_some_and(|previous| {
                world
                    .resource_mut::<RoundScript>()
                    .object_field(id, "enemy")
                    == Value::Object(previous.entity)
            }) {
                set_enemy(world, id, None);
            }
            Ok(Value::Undefined)
        },
    );
}

// A memory confidence decays linearly over the SP known-enemy window. Threat
// scores use this runtime's continuous distance model, not the retail tables.
const ENEMY_MEMORY_MS: i32 = 10_000;

pub(super) fn facing_point(world: &mut World, id: u64) -> Result<Option<[f32; 3]>, String> {
    let Some(target) = enemy(world, id) else {
        return Ok(None);
    };
    let runtime = world.resource::<RoundScript>();
    if runtime.sentients.contains_key(&target) {
        let now = super::super::players::now_ms(world) as i32;
        if let Some(info) = runtime.actors[&id].sentient_info.get(&target)
            && !info.visible
        {
            return Ok(info
                .last_known_position
                .filter(|_| now.saturating_sub(info.last_known_time) < ENEMY_MEMORY_MS));
        }
    }
    point(world, target).map(Some)
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_targeting", "iw4l.sim").enter();
    // Selection only changes perception/target memory and queues notifications.
    // It executes no GSC or pose writes, so all sight queries share this scene.
    super::super::presence::settle_collision(world);
    let mut actors: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(id, actor)| {
            actor.alive
                && actor.physics_initialized
                && world.resource::<RoundScript>().can_receive_call(id)
        })
        .map(|(&id, actor)| (actor.slot, id))
        .collect();
    actors.sort_unstable();
    {
        let mut eyes = perception::EyeBatch::default();
        for (_, id) in actors {
            select_settled(world, id, now, &mut eyes)?;
        }
    }
    let sources: Vec<_> = world
        .resource::<RoundScript>()
        .sentients
        .keys()
        .copied()
        .filter(|id| world.resource::<RoundScript>().can_receive_call(id))
        .collect();
    for id in sources {
        transition(world, id, now);
    }
    Ok(())
}

fn select_settled(
    world: &mut World,
    id: u64,
    now: i32,
    eyes: &mut perception::EyeBatch,
) -> Result<(), String> {
    let scripted = world.resource::<RoundScript>().actors[&id].entity_target;
    let scripted = scripted.filter(|target| {
        world
            .resource::<RoundScript>()
            .can_receive_call(&target.entity)
    });
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .entity_target = scripted;
    let ignore_all = world.resource::<RoundScript>().sentients[&id].ignore_all != 0;
    if ignore_all {
        set_enemy(world, id, None);
        return Ok(());
    }
    if let Some(target) = scripted.filter(|target| target.priority == 1.0) {
        set_enemy(world, id, Some(target.entity));
        return Ok(());
    }
    let (team, group, sight_range, candidates) = {
        let runtime = world.resource::<RoundScript>();
        let sentient = &runtime.sentients[&id];
        (
            sentient.team,
            sentient.threat_group,
            runtime.actors[&id].sight.maximum_distance(),
            runtime.sentients.keys().copied().collect::<Vec<_>>(),
        )
    };
    let Value::Vector(origin) = super::super::players::entity_field(world, id, "origin") else {
        return Err("actor has no position for threat selection".into());
    };
    let current = enemy(world, id);
    let mut best = scripted.map(|target| (target.entity, target.priority, true));
    let mut threatening = false;
    for target in candidates {
        let runtime = world.resource::<RoundScript>();
        let target_state = &runtime.sentients[&target];
        let hostile = matches!(team, 1..=3)
            && matches!(target_state.team, 1..=3)
            && team != target_state.team;
        if target == id
            || !hostile
            || !runtime.can_receive_call(&target)
            || runtime
                .actors
                .get(&target)
                .is_some_and(|actor| !actor.alive || actor.delayed_death)
        {
            continue;
        }
        // Bias rows belong to the candidate, columns to the observing actor.
        let bias = runtime.threat_groups.bias(target_state.threat_group, group);
        if bias == super::super::sentients::IGNORE_GROUP_BIAS {
            continue;
        }
        let health = super::super::players::entity_field(world, target, "health");
        if !matches!(health, Value::Int(n) if n > 0)
            && !matches!(health, Value::Float(n) if n > 0.0)
        {
            continue;
        }
        if world.resource::<RoundScript>().sentients[&target].ignore_me != 0 {
            continue;
        }
        let attackable = world.resource::<RoundScript>().actors[&id]
            .sentient_info
            .get(&target)
            .is_none_or(|info| info.attack_time <= now);
        let visible = perception::can_see_batched(world, id, target, 0, eyes)?;
        let Some(info) = world.resource::<RoundScript>().actors[&id]
            .sentient_info
            .get(&target)
            .cloned()
        else {
            continue;
        };
        let Some(position) = info.last_known_position else {
            continue;
        };
        let age = now.saturating_sub(info.last_known_time).max(0);
        if !visible && age >= ENEMY_MEMORY_MS {
            continue;
        }
        let distance = glam::Vec3::from_array(position).distance(glam::Vec3::from_array(origin));
        // At the sight range, distance urgency is one. One bias point adds
        // that same urgency; the rational normalization keeps scores below 1.
        let urgency = (sight_range / distance.max(1.0) + bias as f32).max(0.0);
        let confidence = if visible {
            1.0
        } else {
            1.0 - age as f32 / ENEMY_MEMORY_MS as f32
        };
        let score = (urgency / (1.0 + urgency) * confidence).min(1.0 - f32::EPSILON);
        if score <= 0.0 {
            continue;
        }
        if best.is_none_or(|(other, strength, available)| {
            attackable && !available
                || attackable == available
                    && (score > strength
                        || score == strength && current == Some(target) && current != Some(other))
        }) {
            best = Some((target, score, attackable));
            threatening = visible && attackable;
        }
    }
    set_enemy(world, id, best.map(|(target, _, _)| target));
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .sight
        .has_threatening_enemy = threatening;
    Ok(())
}
