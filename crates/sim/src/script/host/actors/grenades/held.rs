use super::*;

// Use the model's authored hand pose rather than an actor-relative guess.
const HAND_TAG: &str = "tag_inhand";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Contact {
    pub(super) threat: Option<crate::ProjectileId>,
    pub(super) held: Option<crate::ProjectileId>,
}

pub(crate) fn contact_value(runtime: &RoundScript, actor: &Actor) -> Value {
    actor
        .grenade_contact
        .threat
        .and_then(|id| runtime.missiles.get(&id))
        .copied()
        .map_or(Value::Undefined, Value::Object)
}

pub(super) fn find(world: &mut World, id: crate::ProjectileId) -> Option<crate::ProjectileState> {
    crate::frame::collect_projectiles(world)
        .into_iter()
        .find(|p| p.id == id && p.live)
}

pub(super) fn projectile(world: &mut World, actor: u64) -> Option<crate::ProjectileState> {
    let id = world.resource::<RoundScript>().actors[&actor]
        .grenade_contact
        .held?;
    find(world, id)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f64 {
    (0..3)
        .map(|i| (f64::from(a[i]) - f64::from(b[i])).powi(2))
        .sum()
}

// Our threat selection uses the actual predicted landing and blast radius.
// It is independent of script event-listener registration.
pub(crate) fn observe(world: &mut World, grenade: &crate::ProjectileState, now: i32) {
    if world
        .resource::<RoundScript>()
        .actors
        .values()
        .any(|actor| actor.grenade_contact.held == Some(grenade.id))
    {
        return;
    }
    let Some(facts) = FrameWorld::from_world(world).equipment_facts_for(grenade.weapon) else {
        return;
    };
    if facts.weap_type != weapon_iw4::WEAPTYPE_GRENADE || !grenade.live {
        return;
    }
    let duration = grenade
        .detonate_at_ms
        .map_or(0, |time| time.saturating_sub(now).max(0));
    let Some(landing) = crate::equipment::predict_projectile(
        &FrameWorld::from_world(world),
        *grenade,
        now,
        duration,
    ) else {
        return;
    };
    let radius = f64::from(facts.explosion_radius.max(0)) + SAFETY_MARGIN_UNITS;
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(_, actor)| actor.alive && actor.grenade_contact.held.is_none())
        .map(|(&id, _)| id)
        .collect();
    let content = FrameWorld::from_world(world).content();
    for id in ids {
        let Ok(at) = position(world, id) else {
            continue;
        };
        let local_landing = content.simulation_pose_in(landing, at);
        if local_landing
            .is_none_or(|pose| distance(at, pose.w_axis.truncate().to_array()) > radius * radius)
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            let contact = &mut runtime.actors.get_mut(&id).unwrap().grenade_contact;
            if contact.threat == Some(grenade.id) {
                contact.threat = None;
            }
            continue;
        }
        let (number, old, awareness, has_enemy) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let awareness = match runtime.object_field(id, "grenadeawareness") {
                Value::Float(n) => n,
                Value::Int(n) => n as f32,
                _ => 1.0,
            };
            (
                runtime.entities[&id].number,
                runtime.actors[&id].grenade_contact.threat,
                awareness,
                runtime.sentients[&id].target.is_some(),
            )
        };
        if grenade.owner.number() == number || !awareness.is_finite() || awareness <= 0.0 {
            continue;
        }
        if has_enemy
            && awareness < 1.0
            && crate::item::random_unit(&mut world.resource_mut::<RoundScript>().rng) > awareness
        {
            continue;
        }
        if let Some(previous) = old
            .filter(|old| *old != grenade.id)
            .and_then(|old| find(world, old))
            && let Some(previous) = content.simulation_pose_in(previous.origin, at)
            && let Some(current) = content.simulation_pose_in(grenade.origin, at)
            && distance(at, previous.w_axis.truncate().to_array())
                <= distance(at, current.w_axis.truncate().to_array())
        {
            continue;
        }
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .grenade_contact
            .threat = Some(grenade.id);
        let source = super::super::super::entity_damage::attacker_value(
            world,
            grenade.owner.client(),
            grenade.owner.entity(),
        );
        let notification = {
            let runtime = world.resource::<RoundScript>();
            let source_team = if let Value::Object(source) = source {
                runtime.sentients.get(&source).map(|state| state.team)
            } else {
                None
            };
            source_team
                .filter(|team| {
                    matches!(team, 1..=3)
                        && matches!(runtime.sentients[&id].team, 1..=3)
                        && *team != runtime.sentients[&id].team
                })
                .and_then(|_| runtime.missiles.get(&grenade.id).copied())
        };
        if let Some(object) = notification {
            crate::script::runtime::raise(
                world,
                Value::Object(id),
                "grenadedanger",
                vec![Value::Object(object)],
            );
        }
    }
}

pub(super) fn hand(world: &mut World, actor: u64) -> Result<([f32; 3], [f32; 3]), String> {
    super::super::super::presence::tag_world(world, actor, HAND_TAG)
        .map(|(position, axis)| (position, math_iw4::axis_to_angles(axis)))
        .ok_or_else(|| format!("grenade return requires authored {HAND_TAG} pose"))
}

fn attach(
    world: &mut World,
    actor: u64,
    projectile: crate::ProjectileState,
    now: i32,
) -> Result<(), String> {
    let (at, angles) = hand(world, actor)?;
    let origin = position(world, actor)?;
    let Value::Vector(actor_angles) = players::entity_field(world, actor, "angles") else {
        return Err("grenade holder angles are unavailable".into());
    };
    let axis = math_iw4::angles_to_axis(actor_angles);
    let offset = axis.map(|row| (0..3).map(|i| row[i] * (at[i] - origin[i])).sum());
    let target = super::super::super::guidance::target(world, &Value::Object(actor), offset)?;
    let mut frame = FrameWorld::from_world(world);
    let row = frame
        .projectile_mut_by_number(projectile.entnum)
        .filter(|p| p.id == projectile.id)
        .ok_or("grenade is no longer live")?;
    row.origin = at;
    row.velocity = [0.; 3];
    row.pos = entity_iw4::Trajectory {
        tr_type: entity_iw4::TR_STATIONARY,
        tr_time: now,
        tr_base: at,
        ..Default::default()
    };
    row.apos = entity_iw4::Trajectory {
        tr_type: entity_iw4::TR_STATIONARY,
        tr_time: now,
        tr_base: angles,
        ..Default::default()
    };
    row.attached_to = Some(target);
    row.grounded = false;
    row.stuck_pane = None;
    Ok(())
}

pub(super) fn pickup(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    let actor = actor_id(world, receiver)?;
    let Some(id) = world.resource::<RoundScript>().actors[&actor]
        .grenade_contact
        .threat
    else {
        return Ok(Value::Undefined);
    };
    let Some(projectile) = find(world, id) else {
        return Ok(Value::Undefined);
    };
    if world
        .resource::<RoundScript>()
        .actors
        .iter()
        .any(|(&other, state)| other != actor && state.grenade_contact.held == Some(id))
    {
        return Err("grenade is already held by another actor".into());
    }
    let facts = facts(world, projectile.weapon)?;
    if facts.stickiness != 0 {
        return Err("attached explosives cannot be returned".into());
    }
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    if world.resource::<RoundScript>().actors[&actor]
        .grenade_throw
        .is_none()
    {
        let (origin, _) = hand(world, actor)?;
        let plan = return_plan(world, actor, projectile, origin)?
            .ok_or("no physical grenade return trajectory")?;
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&actor)
            .unwrap()
            .grenade_throw = Some(plan);
    }
    attach(world, actor, projectile, now)?;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&actor)
        .unwrap()
        .grenade_contact
        .held = Some(id);
    if let Some(object) = world.resource::<RoundScript>().missiles.get(&id).copied() {
        world.resource_mut::<RoundScript>().set_object_field(
            object,
            "activator",
            Value::Object(actor),
        );
    }
    Ok(Value::Undefined)
}

pub(super) fn rethrow(
    world: &mut World,
    actor: u64,
    projectile: crate::ProjectileState,
    origin: [f32; 3],
    velocity: [f32; 3],
) -> Result<(), String> {
    let owner = owner(world, Some(actor))?;
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let mut frame = FrameWorld::from_world(world);
    let row = frame
        .projectile_mut_by_number(projectile.entnum)
        .filter(|p| p.id == projectile.id && p.live)
        .ok_or("held grenade is no longer live")?;
    row.origin = origin;
    row.velocity = velocity;
    row.pos = motion(origin, velocity, now).pos;
    row.owner = owner;
    row.attached_to = None;
    row.grounded = false;
    row.guide = Default::default();
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.actors.get_mut(&actor).unwrap().grenade_contact = Default::default();
    if let Some(object) = runtime.missiles.get(&projectile.id).copied() {
        runtime.entities.get_mut(&object).unwrap().missile_owner = Some(actor);
        runtime.set_object_field(object, "activator", Value::Undefined);
    }
    Ok(())
}

pub(crate) fn release(world: &mut World, actor: u64) -> Result<(), String> {
    if let Some(projectile) = projectile(world, actor) {
        // Drop at the last real hand pose. Preserve the live missile and timer.
        rethrow(world, actor, projectile, projectile.origin, [0.; 3])?;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let state = runtime.actors.get_mut(&actor).unwrap();
    state.grenade_contact = Default::default();
    state.grenade_throw = None;
    Ok(())
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .keys()
        .copied()
        .collect();
    for id in ids {
        let state = world.resource::<RoundScript>().actors[&id].grenade_contact;
        if let Some(held) = state.held {
            if let Some(projectile) = find(world, held) {
                if !world.resource::<RoundScript>().actors[&id].alive {
                    release(world, id)?;
                } else {
                    attach(world, id, projectile, now)?;
                }
            } else {
                let mut runtime = world.resource_mut::<RoundScript>();
                runtime.actors.get_mut(&id).unwrap().grenade_contact = Default::default();
                runtime.actors.get_mut(&id).unwrap().grenade_throw = None;
            }
        } else if let Some(threat) = state.threat
            && find(world, threat).is_none()
        {
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .grenade_contact
                .threat = None;
        }
    }
    Ok(())
}

pub(super) fn return_plan(
    world: &mut World,
    actor: u64,
    projectile: crate::ProjectileState,
    origin: [f32; 3],
) -> Result<Option<Plan>, String> {
    let facts = facts(world, projectile.weapon)?;
    let radius = f64::from(facts.explosion_radius.max(0)) + SAFETY_MARGIN_UNITS;
    let mut targets = Vec::new();
    if let Some(target) = enemy_target(world, actor)? {
        targets.push(target);
    }
    let at = position(world, actor)?;
    let heading = (origin[1] - at[1]).atan2(origin[0] - at[0]);
    // Even angular sampling bounds the query without a direction/physics table.
    const DIRECTION_SAMPLES: usize = 8;
    for index in 0..DIRECTION_SAMPLES {
        let angle = heading + std::f32::consts::TAU * index as f32 / DIRECTION_SAMPLES as f32;
        targets.push(Target {
            position: [
                origin[0] + angle.cos() * radius as f32 * 2.,
                origin[1] + angle.sin() * radius as f32 * 2.,
                at[2],
            ],
            velocity: [0.; 3],
        });
    }
    let team = world.resource::<RoundScript>().sentients[&actor].team;
    let friends: Vec<_> = world
        .resource::<RoundScript>()
        .sentients
        .iter()
        .filter(|(_, state)| state.team == team)
        .map(|(&id, _)| id)
        .collect();
    let content = FrameWorld::from_world(world).content();
    for target in targets {
        let mut safe = true;
        for friend in &friends {
            if let Some(pose) =
                content.simulation_pose_in(position(world, *friend)?, target.position)
                && distance(pose.w_axis.truncate().to_array(), target.position) <= radius * radius
            {
                safe = false;
                break;
            }
        }
        if !safe {
            continue;
        }
        let methods = vec![Method::Energy, Method::Fast, Method::Slow];
        if let Some(velocity) = trajectory::solve(
            world,
            owner(world, Some(actor))?,
            projectile.weapon,
            origin,
            target,
            trajectory::Options {
                methods: &methods,
                bounce: true,
            },
        )? {
            return Ok(Some(Plan {
                origin,
                velocity,
                offset: [0.; 3],
                methods,
                bounce: true,
                track: false,
                target,
            }));
        }
    }
    Ok(None)
}
