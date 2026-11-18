use super::*;

// Landing within half a humanoid hull height is near an authored foot target.
const LANDING_TOLERANCE: f64 = super::super::spatial::ACTOR_BOUNDS.half_size[2] as f64;
// Bounded numerical corrections keep contact discontinuities from consuming
// an unbounded script tick; failed convergence yields no throw solution.
const CORRECTION_PASSES: usize = 3;
// Bound native-query work to 100 simulation steps. Flight feasibility is
// independent of a script fuse, which may expire before the grenade lands.
const MAX_FLIGHT_STEPS: usize = 100;

fn preview(
    world: &mut World,
    owner: crate::ProjectileOwner,
    weapon: u32,
    origin: [f32; 3],
    velocity: [f32; 3],
    duration: i32,
    bounce: bool,
) -> Option<[f32; 3]> {
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let motion = motion(origin, velocity, now);
    let frame = FrameWorld::from_world(world);
    // A read-only hypothetical flight. No ID/entity is allocated or inserted.
    let projectile = crate::ProjectileState {
        id: crate::ProjectileId(0),
        owner,
        weapon,
        origin,
        velocity,
        pos: motion.pos,
        apos: motion.apos,
        entnum: playerstate_iw4::ENTITYNUM_NONE,
        launch_time: now,
        spawn_time_ms: now,
        detonate_at_ms: Some(now.saturating_add(duration)),
        cleanup_at_ms: now.saturating_add(duration),
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
        grounded: false,
        guide: Default::default(),
        stable_missile: true,
        attached_to: None,
        cause: None,
    };
    crate::equipment::predict_trajectory(&frame, projectile, now, duration, bounce)
}

pub(super) struct Options<'a> {
    pub methods: &'a [Method],
    pub bounce: bool,
}

pub(super) fn solve(
    world: &mut World,
    owner: crate::ProjectileOwner,
    weapon: u32,
    origin: [f32; 3],
    target: Target,
    options: Options<'_>,
) -> Result<Option<[f32; 3]>, String> {
    let Options { methods, bounce } = options;
    let content = FrameWorld::from_world(world).content();
    let Some(pose) = content.simulation_pose_in(target.position, origin) else {
        return Ok(None);
    };
    let target = Target {
        position: pose.w_axis.truncate().to_array(),
        velocity: pose.transform_vector3(target.velocity.into()).to_array(),
    };
    if origin == target.position && target.velocity == [0.0; 3] {
        return Ok(None);
    }
    let equipment = facts(world, weapon)?;
    if !FrameWorld::from_world(world).collision_complete() {
        return Err("grenade collision data is unavailable".into());
    }
    super::super::super::presence::settle_collision(world);
    let maximum_speed = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ai_maxgrenadethrowspeed")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_else(|| {
            f64::from(equipment.projectile_speed).hypot(f64::from(equipment.projectile_speed_up))
        });
    if !maximum_speed.is_finite() || maximum_speed < 0.0 {
        return Err("grenade maximum speed must be finite and nonnegative".into());
    }
    let step = crate::MATCH_TICK_MS as i32;
    let horizon = step * MAX_FLIGHT_STEPS as i32;
    let gravity = f64::from(entity_iw4::GRAVITY_ACCELERATION);
    let mut candidates = Vec::new();
    for duration in (step..=horizon).step_by(step as usize) {
        let seconds = f64::from(duration) / 1000.0;
        let at = std::array::from_fn::<_, 3, _>(|i| {
            f64::from(target.position[i]) + f64::from(target.velocity[i]) * seconds
        });
        let velocity = std::array::from_fn::<_, 3, _>(|i| {
            (at[i] - f64::from(origin[i])) / seconds
                + if i == 2 { gravity * seconds * 0.5 } else { 0.0 }
        });
        let energy = velocity.iter().map(|n| n * n).sum::<f64>();
        if energy.is_finite() {
            candidates.push((duration, velocity, at, energy));
        }
    }
    for method in methods {
        let mut ordered: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                *method == Method::Unlimited || candidate.3 <= maximum_speed * maximum_speed
            })
            .collect();
        ordered.sort_by(|a, b| match method {
            Method::Energy => a.3.total_cmp(&b.3),
            Method::Slow => b.0.cmp(&a.0),
            _ => a.0.cmp(&b.0),
        });
        for (duration, initial, target, _) in ordered {
            let mut velocity = *initial;
            let seconds = f64::from(*duration) / 1000.0;
            let mut previous_error = f64::INFINITY;
            for _ in 0..if bounce { CORRECTION_PASSES } else { 1 } {
                if *method != Method::Unlimited
                    && velocity.iter().map(|n| n * n).sum::<f64>() > maximum_speed * maximum_speed
                {
                    break;
                }
                let velocity32 = velocity.map(|n| n as f32);
                if !velocity32.into_iter().all(f32::is_finite) {
                    break;
                }
                let Some(landed) =
                    preview(world, owner, weapon, origin, velocity32, *duration, bounce)
                else {
                    break;
                };
                let Some(landed) = content.simulation_pose_in(landed, origin) else {
                    break;
                };
                let landed = landed.w_axis.truncate().to_array();
                let error = std::array::from_fn::<_, 3, _>(|i| target[i] - f64::from(landed[i]));
                let distance = error.iter().map(|n| n * n).sum::<f64>().sqrt();
                if distance <= LANDING_TOLERANCE {
                    return Ok(Some(velocity32));
                }
                if distance >= previous_error {
                    break;
                }
                previous_error = distance;
                for i in 0..3 {
                    velocity[i] += error[i] / seconds;
                }
            }
        }
    }
    Ok(None)
}
