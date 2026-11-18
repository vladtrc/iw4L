use super::super::{anim_mode, goals, navigation, scripted};
use super::*;

// The packaged return script picks up a grenade 32 units in front of the feet.
const PICKUP_STANDOFF: f32 = 32.0;
const ESCAPE_DIRECTIONS: usize = 8;

#[derive(Clone, Debug)]
pub(crate) struct Response {
    grenade: crate::ProjectileId,
    phase: Phase,
    goal: Option<goals::Goal>,
    previous_mode: anim_mode::Mode,
    facing: Option<f32>,
}
#[derive(Clone, Debug)]
enum Phase {
    Acquire,
    Throw { thread: Option<u64> },
    Escape,
    Cower,
}

pub(crate) fn movement_goal(actor: &Actor) -> Option<&goals::Goal> {
    actor.grenade_response.as_ref()?.goal.as_ref()
}

pub(crate) fn behavior(actor: &Actor) -> Option<scripted::ControlKind> {
    Some(match actor.grenade_response.as_ref()?.phase {
        Phase::Throw { .. } => scripted::ControlKind::GrenadeReturn,
        Phase::Cower => scripted::ControlKind::GrenadeCower,
        _ => scripted::ControlKind::Move,
    })
}

pub(crate) fn facing(actor: &Actor) -> Option<f32> {
    actor.grenade_response.as_ref()?.facing
}

pub(crate) fn cancel(world: &mut World, actor: u64) -> Result<(), String> {
    let response = world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&actor)
        .and_then(|actor| actor.grenade_response.take());
    let Some(response) = response else {
        return Ok(());
    };
    held::release(world, actor)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let state = runtime.actors.get_mut(&actor).unwrap();
    if matches!(
        state.anim_script_kind,
        Some(scripted::ControlKind::GrenadeReturn | scripted::ControlKind::GrenadeCower)
    ) {
        state.script_anim_mode = response.previous_mode;
    }
    state.navigation = Default::default();
    state.motion.stop();
    state.move_mode = 0;
    Ok(())
}

fn return_time(world: &World, actor: u64) -> Option<i32> {
    let runtime = world.resource::<RoundScript>();
    let animator = runtime.animators.get(&actor)?;
    animator
        .data
        .names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.starts_with("grenade_return_"))
        .filter_map(|(index, _)| {
            animator
                .runtime
                .leaf_clip(xmodel_runtime::XAnimNodeId(index as u16))
        })
        .filter_map(|clip| {
            clip.notifies
                .iter()
                .filter(|note| note.name == "grenade_throw")
                .map(|note| (clip.duration() * note.time * 1000.0).ceil() as i32)
                .max()
        })
        .max()
}

fn script_available(world: &World, name: &str) -> bool {
    world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|program| program.names.contains_key(name))
}

fn goal(position: [f32; 3]) -> goals::Goal {
    goals::Goal {
        position,
        radius: 4.0,
        height: super::super::spatial::ACTOR_BOUNDS.half_size[2] * 2.,
        ..Default::default()
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    glam::Vec3::from_array(a).distance(glam::Vec3::from_array(b))
}

fn escape(
    world: &mut World,
    actor: u64,
    landing: [f32; 3],
    radius: f32,
) -> Result<(Phase, Option<goals::Goal>), String> {
    let at = position(world, actor)?;
    let away = glam::Vec2::new(at[0] - landing[0], at[1] - landing[1]);
    let heading = away.y.atan2(away.x);
    let clearance = radius + super::super::spatial::ACTOR_BOUNDS.half_size[0];
    let mut candidates: Vec<_> = (0..ESCAPE_DIRECTIONS)
        .map(|index| {
            let angle = heading + std::f32::consts::TAU * index as f32 / ESCAPE_DIRECTIONS as f32;
            [
                landing[0] + angle.cos() * clearance,
                landing[1] + angle.sin() * clearance,
                at[2],
            ]
        })
        .collect();
    candidates.sort_by(|a, b| distance(at, *a).total_cmp(&distance(at, *b)));
    for point in candidates {
        if navigation::direct_supported(world, actor, at, point, true) {
            return Ok((Phase::Escape, Some(goal(point))));
        }
    }
    // No supported escape corridor: the real authored cower behavior owns pose.
    Ok((Phase::Cower, None))
}

fn enter(
    world: &mut World,
    actor: u64,
    grenade: crate::ProjectileId,
    phase: Phase,
    goal: Option<goals::Goal>,
) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let state = runtime.actors.get_mut(&actor).unwrap();
    let previous_mode = state
        .grenade_response
        .as_ref()
        .map_or(state.script_anim_mode, |response| response.previous_mode);
    super::super::cover::clear(state);
    super::super::reacquire::clear(state, false);
    state.grenade_throw = None;
    state.navigation = Default::default();
    state.grenade_response = Some(Response {
        grenade,
        phase,
        goal,
        previous_mode,
        facing: None,
    });
}

fn throwing(world: &mut World, actor: u64, response: &Response) -> Result<bool, String> {
    let Phase::Throw { thread } = response.phase else {
        return Ok(false);
    };
    let state = &world.resource::<RoundScript>().actors[&actor];
    let current = (state.anim_script_kind == Some(scripted::ControlKind::GrenadeReturn))
        .then_some(state.anim_script_thread)
        .flatten();
    if let Some(thread) = thread.or(current) {
        if !crate::script::runtime::engine_thread_alive(world, thread) {
            cancel(world, actor)?;
        } else {
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&actor)
                .unwrap()
                .grenade_response
                .as_mut()
                .unwrap()
                .phase = Phase::Throw {
                thread: Some(thread),
            };
        }
    }
    Ok(true)
}

pub(crate) fn think(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_grenades", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .keys()
        .copied()
        .collect();
    for actor in ids {
        let (eligible, contact, response) = {
            let runtime = world.resource::<RoundScript>();
            let state = &runtime.actors[&actor];
            (
                state.alive
                    && state.script_initialized
                    && state.physics_initialized
                    && state.species == super::super::Species::Human
                    && state.scripted_control.is_none()
                    && state.turret.requested.is_none()
                    && !state.traversal.active()
                    && !state.arrival.active()
                    && anim_mode::effective(state) != anim_mode::Mode::PointRelative,
                state.grenade_contact.threat,
                state.grenade_response.clone(),
            )
        };
        if !eligible {
            cancel(world, actor)?;
            continue;
        }
        if let Some(response) = &response
            && throwing(world, actor, response)?
        {
            continue;
        }
        let Some(projectile) = contact.and_then(|id| held::find(world, id)) else {
            cancel(world, actor)?;
            continue;
        };
        let facts = facts(world, projectile.weapon)?;
        let duration = projectile
            .detonate_at_ms
            .map_or(0, |end| end.saturating_sub(now).max(0));
        let Some(landing) = crate::equipment::predict_projectile(
            &FrameWorld::from_world(world),
            projectile,
            now,
            duration,
        ) else {
            cancel(world, actor)?;
            continue;
        };
        let at = position(world, actor)?;
        let content = FrameWorld::from_world(world).content();
        let Some(landing) = content.simulation_pose_in(landing, at) else {
            cancel(world, actor)?;
            continue;
        };
        let landing = landing.w_axis.truncate().to_array();
        let local_projectile = content
            .simulation_pose_in(projectile.origin, at)
            .map(|pose| pose.w_axis.truncate().to_array());
        let radius = facts.explosion_radius.max(0) as f32 + SAFETY_MARGIN_UNITS as f32;
        if distance(at, landing) > radius {
            cancel(world, actor)?;
            continue;
        }
        let already_reserved = world
            .resource::<RoundScript>()
            .actors
            .iter()
            .any(|(&id, state)| {
                id != actor
                    && state.grenade_response.as_ref().is_some_and(|state| {
                        state.grenade == projectile.id
                            && matches!(state.phase, Phase::Acquire | Phase::Throw { .. })
                    })
            });
        let animation_ms = return_time(world, actor);
        let can_return = local_projectile.is_some()
            && !already_reserved
            && facts.stickiness == 0
            && animation_ms.is_some_and(|time| duration > time + crate::MATCH_TICK_MS as i32)
            && script_available(world, "animscripts/grenade_return_throw::main");
        if can_return {
            let direction =
                glam::Vec2::new(landing[0] - at[0], landing[1] - at[1]).normalize_or_zero();
            let pickup = [
                landing[0] - direction.x * PICKUP_STANDOFF,
                landing[1] - direction.y * PICKUP_STANDOFF,
                at[2],
            ];
            let from = held::hand(world, actor)
                .ok()
                .map(|(hand, _)| std::array::from_fn(|i| hand[i] + pickup[i] - at[i]));
            if let Some(from) = from
                && navigation::direct_supported(world, actor, at, pickup, true)
                && held::return_plan(world, actor, projectile, from)?.is_some()
            {
                let reach = PICKUP_STANDOFF + super::super::spatial::ACTOR_BOUNDS.half_size[0];
                if let Some(projectile_origin) = local_projectile
                    && distance(at, projectile_origin) <= reach
                {
                    let from = held::hand(world, actor)?.0;
                    if let Some(plan) = held::return_plan(world, actor, projectile, from)? {
                        enter(
                            world,
                            actor,
                            projectile.id,
                            Phase::Throw { thread: None },
                            None,
                        );
                        let mut runtime = world.resource_mut::<RoundScript>();
                        let state = runtime.actors.get_mut(&actor).unwrap();
                        state.grenade_response.as_mut().unwrap().facing = Some(
                            (projectile_origin[1] - at[1])
                                .atan2(projectile_origin[0] - at[0])
                                .to_degrees(),
                        );
                        state.grenade_throw = Some(plan);
                        state.motion.stop();
                        state.orientation.clear_script();
                        continue;
                    }
                } else {
                    let needs_update = response.as_ref().is_none_or(|old| {
                        old.grenade != projectile.id
                            || !matches!(old.phase, Phase::Acquire)
                            || old
                                .goal
                                .as_ref()
                                .is_none_or(|goal| distance(goal.position, pickup) > goal.radius)
                    });
                    if needs_update {
                        enter(
                            world,
                            actor,
                            projectile.id,
                            Phase::Acquire,
                            Some(goal(pickup)),
                        );
                    }
                    continue;
                }
            }
        }
        if response.as_ref().is_some_and(|old| {
            old.grenade == projectile.id
                && matches!(old.phase, Phase::Escape)
                && old
                    .goal
                    .as_ref()
                    .is_some_and(|goal| distance(goal.position, landing) > radius)
        }) {
            continue;
        }
        let (phase, goal) = escape(world, actor, landing, radius)?;
        if matches!(phase, Phase::Cower)
            && !script_available(world, "animscripts/grenade_cower::main")
        {
            return Err("grenade danger has no supported escape or authored cower script".into());
        }
        enter(world, actor, projectile.id, phase, goal);
    }
    Ok(())
}
