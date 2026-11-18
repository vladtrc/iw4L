use crate::ClientId;
use crate::bullet_collision::{ColliderId, PlayerCollisionPose, TraceOutcome};
use crate::world_graph::*;
use entity_iw4::{
    GRAVITY_ACCELERATION as GRAVITY, Trajectory, evaluate_trajectory, evaluate_trajectory_delta,
};

/// Projectile collision response: the reflected velocity is scaled
/// between `parallel` (grazing) and `perpendicular` (head on). A merged map
/// sets its own through the `projectile_parallel` and `projectile_perpendicular`
/// dvars of its recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileFlightRules {
    pub parallel: f32,
    pub perpendicular: f32,
    /// Off a floor the projectile leaves at least this many degrees above
    /// the horizontal, its speed kept: it hops far instead of skimming
    /// (`projectile_lift`; 0 leaves the reflection alone).
    pub lift: f32,
    /// Off any wall the projectile heads, at the speed it bounced with, for
    /// the door that leads on out of its world (the `a` door of a portal
    /// there, the nearest): 1 on the low arc, 2 on the high one
    /// (`projectile_home`; 0 bounces as reflected, as does a door out of reach).
    pub home: f32,
    /// Optional target in simulation coordinates; uses a high arc.
    pub home_target: Option<[f32; 3]>,
    /// Maximum door-entry angle, using a staging bounce when needed (0 off).
    pub home_align: f32,
    pub gravity_scale: f32,
    pub speed_scale: f32,
}

impl Default for ProjectileFlightRules {
    fn default() -> Self {
        Self {
            parallel: 0.92,
            perpendicular: 0.82,
            lift: 0.0,
            home: 0.0,
            home_target: None,
            home_align: 0.0,
            gravity_scale: 1.0,
            speed_scale: 1.0,
        }
    }
}

impl ProjectileFlightRules {
    /// These rules with what the `projectile_*` dvars `dvar` reads set.
    pub fn with_dvars(self, dvar: impl Fn(&str) -> Option<f32>) -> Self {
        let finite = |name| dvar(name).filter(|value| value.is_finite());
        Self {
            parallel: finite("projectile_parallel").unwrap_or(self.parallel),
            perpendicular: finite("projectile_perpendicular").unwrap_or(self.perpendicular),
            lift: finite("projectile_lift").unwrap_or(self.lift),
            home: finite("projectile_home").unwrap_or(self.home),
            home_align: finite("projectile_home_align").unwrap_or(self.home_align),
            gravity_scale: finite("projectile_gravity_scale")
                .filter(|scale| *scale >= 0.0 && *scale <= 4.0)
                .unwrap_or(self.gravity_scale),
            speed_scale: finite("projectile_speed_scale")
                .filter(|scale| *scale >= 0.05 && *scale <= 4.0)
                .unwrap_or(self.speed_scale),
            home_target: match (
                finite("projectile_home_x"),
                finite("projectile_home_y"),
                finite("projectile_home_z"),
            ) {
                (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                _ => self.home_target,
            },
        }
    }

    fn position_at(&self, trajectory: &Trajectory, time: i32) -> [f32; 3] {
        let mut position = evaluate_trajectory(trajectory, time);
        let gravity = self.gravity_adjustment(trajectory);
        if gravity != 0.0 {
            let seconds = (time - trajectory.tr_time) as f32 / 1000.0;
            position[2] -= 0.5 * gravity * seconds * seconds;
        }
        position
    }

    fn velocity_at(&self, trajectory: &Trajectory, time: i32) -> [f32; 3] {
        let mut velocity = evaluate_trajectory_delta(trajectory, time);
        let gravity = self.gravity_adjustment(trajectory);
        if gravity != 0.0 {
            let seconds = (time - trajectory.tr_time) as f32 / 1000.0;
            velocity[2] -= gravity * seconds;
        }
        velocity
    }

    fn gravity_adjustment(&self, trajectory: &Trajectory) -> f32 {
        match trajectory.tr_type {
            entity_iw4::TR_GRAVITY => GRAVITY * (self.gravity_scale - 1.0),
            entity_iw4::TR_LINEAR if self.gravity_scale != 1.0 => GRAVITY * self.gravity_scale,
            _ => 0.0,
        }
    }
}

/// Trajectory state consumed by the shared flight step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileFlightBody {
    pub origin: [f32; 3],
    pub pos: Trajectory,
    pub travel: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProjectileFlightEvent {
    Bounce {
        origin: [f32; 3],
        normal: [f32; 3],
        surf_type: u8,
        speed_delta: f32,
    },
    Portal {
        portal: PortalId,
        from_a: bool,
        /// Where it entered, shared coordinates, the time and the velocity
        /// it entered with.
        at: [f32; 3],
        time: i32,
        velocity: [f32; 3],
    },
    Player {
        collider: ColliderId,
        origin: [f32; 3],
        normal: [f32; 3],
        /// Fraction of the entire requested tick, including earlier portals.
        fraction: f32,
    },
}

#[derive(Clone, Copy)]
pub struct ProjectileFlightScene<'a> {
    pub atlas: &'a Atlas,
    pub base: AtlasClip<'a>,
    pub players: &'a [PlayerCollisionPose],
    pub blocked: &'a [[bool; 2]],
}

/// Moves the body from `prev` to `time`: through doors, off walls, into a
/// player. A bounce holds the body at the wall until the end of the tick, as
/// stock grenades do. Four portal transfers also hold at the last exit for
/// the rest of the tick. A player hit ends the step.
pub fn projectile_step(
    scene: ProjectileFlightScene<'_>,
    owner: Option<ClientId>,
    rules: &ProjectileFlightRules,
    body: &mut ProjectileFlightBody,
    prev: i32,
    time: i32,
) -> Vec<ProjectileFlightEvent> {
    let ProjectileFlightScene {
        atlas,
        base,
        players,
        blocked,
    } = scene;
    let mut events = Vec::new();
    let mut from_time = prev.max(body.pos.tr_time);
    if rules.gravity_scale != 1.0
        && matches!(
            body.pos.tr_type,
            entity_iw4::TR_GRAVITY | entity_iw4::TR_LINEAR
        )
    {
        // Apply changed gravity to the current state, not its earlier history.
        body.pos.tr_delta = evaluate_trajectory_delta(&body.pos, from_time);
        body.pos.tr_base = body.origin;
        body.pos.tr_time = from_time;
    }
    // A tick crosses at most a few doors; the bound keeps a bad portal pair
    // from looping.
    for transfer in 0..4 {
        let start = body.origin;
        let end = rules.position_at(&body.pos, time);
        let crossing = atlas.crossing(start, end);
        let (_, outcome) = atlas.trace(base, players, owner, start, end);
        let barrier = atlas.unavailable_exit_hit(blocked, start, end);
        let outcome = atlas.clip_unavailable_exit(blocked, start, end, outcome);
        let hit_barrier = barrier.is_some_and(|barrier| barrier == outcome);
        let hit = match outcome {
            TraceOutcome::Hit {
                fraction,
                end,
                normal,
                collider,
            } => Some((fraction, end, normal, collider)),
            TraceOutcome::StartSolid { collider, end } => {
                let d = sub(start, end);
                let len = dot(d, d).sqrt();
                let normal = if len < 1e-6 {
                    [0.0, 0.0, 1.0]
                } else {
                    [d[0] / len, d[1] / len, d[2] / len]
                };
                collider.map(|collider| (0.0, end, normal, collider))
            }
            TraceOutcome::Miss { .. } | TraceOutcome::Invalid { .. } => None,
        };
        if let Some((portal_fraction, index, from_a)) = crossing
            && hit.is_none_or(|(fraction, ..)| portal_fraction < fraction)
        {
            let (entered, exit, yaw) = atlas.portals[index.index()].oriented(from_a);
            let at = lerp(start, end, portal_fraction);
            let at_time = from_time + ((time - from_time) as f32 * portal_fraction) as i32;
            let velocity = rules.velocity_at(&body.pos, at_time);
            body.travel += dot(sub(at, start), sub(at, start)).sqrt();
            let carried = carry_point(&entered, &exit, yaw, at);
            body.origin = carried;
            body.pos.tr_base = carried;
            body.pos.tr_time = at_time;
            body.pos.tr_delta = rotate_yaw(velocity, yaw);
            if transfer == 3 {
                body.pos.tr_time = time;
            }
            from_time = at_time;
            events.push(ProjectileFlightEvent::Portal {
                portal: index,
                from_a,
                at,
                time: at_time,
                velocity,
            });
            continue;
        }
        match hit {
            Some((fraction, at, normal, collider @ ColliderId::Player { .. })) => {
                body.travel += dot(sub(at, start), sub(at, start)).sqrt();
                body.origin = at;
                events.push(ProjectileFlightEvent::Player {
                    collider,
                    origin: at,
                    normal,
                    fraction: if time > prev {
                        ((from_time - prev) as f32 + fraction * (time - from_time) as f32)
                            / (time - prev) as f32
                    } else {
                        0.0
                    },
                });
            }
            Some((fraction, at, normal, collider)) => {
                let hit_time = from_time + ((time - from_time) as f32 * fraction) as i32;
                let incoming = rules.velocity_at(&body.pos, hit_time);
                let reflected = reflect(incoming, normal, rules);
                let gravity = if body.pos.tr_type == entity_iw4::TR_GRAVITY
                    || (body.pos.tr_type == entity_iw4::TR_LINEAR && rules.gravity_scale != 1.0)
                {
                    GRAVITY * rules.gravity_scale
                } else {
                    0.0
                };
                let outgoing =
                    homed(atlas, base, at, reflected, normal, rules, gravity).unwrap_or(reflected);
                body.travel += dot(sub(at, start), sub(at, start)).sqrt();
                let safe = if hit_barrier {
                    let gap = (4. * f32::EPSILON * at[0].abs()).max(0.02);
                    add(at, normal.map(|n| n * gap))
                } else {
                    at
                };
                body.origin = safe;
                body.pos.tr_base = safe;
                body.pos.tr_time = time;
                body.pos.tr_delta = outgoing;
                let surf_type = match collider {
                    ColliderId::World { surface_flags, .. }
                    | ColliderId::EntityDObjBone { surface_flags, .. }
                    | ColliderId::EntityLinkedBrush { surface_flags, .. } => {
                        trace_iw4::surface_type_from_flags(surface_flags)
                    }
                    ColliderId::Player { .. } => 0,
                    ColliderId::EntityBody { .. } => weapon_iw4::SURF_TYPE_FLESH as u8,
                };
                let change = sub(outgoing, incoming);
                events.push(ProjectileFlightEvent::Bounce {
                    origin: at,
                    normal,
                    surf_type,
                    speed_delta: dot(change, change).sqrt(),
                });
            }
            None => {
                body.travel += dot(sub(end, start), sub(end, start)).sqrt();
                body.origin = end;
                if rules.gravity_scale != 1.0 {
                    body.pos.tr_delta = rules.velocity_at(&body.pos, time);
                    body.pos.tr_base = end;
                    body.pos.tr_time = time;
                }
            }
        }
        break;
    }
    events
}

fn reflect(incoming: [f32; 3], normal: [f32; 3], rules: &ProjectileFlightRules) -> [f32; 3] {
    let speed = dot(incoming, incoming).sqrt();
    let nlen = dot(normal, normal).sqrt();
    if speed < 1e-6 || nlen < 1e-6 {
        return [0.0; 3];
    }
    let n = [normal[0] / nlen, normal[1] / nlen, normal[2] / nlen];
    let d = dot(incoming, n);
    let reflected = [
        incoming[0] - 2.0 * d * n[0],
        incoming[1] - 2.0 * d * n[1],
        incoming[2] - 2.0 * d * n[2],
    ];
    let incidence = (-d / speed).clamp(0.0, 1.0);
    let factor = rules.parallel + (rules.perpendicular - rules.parallel) * incidence;
    lifted(reflected, n, rules.lift).map(|v| v * factor)
}

fn ballistic_arc(
    at: [f32; 3],
    target: [f32; 3],
    speed: f32,
    high: bool,
    gravity: f32,
) -> Option<[f32; 3]> {
    if !at
        .into_iter()
        .chain(target)
        .chain([speed, gravity])
        .all(f32::is_finite)
        || speed < 1.0
        || gravity < 0.0
    {
        return None;
    }
    let d = std::array::from_fn::<_, 3, _>(|i| f64::from(target[i]) - f64::from(at[i]));
    let flat = d[0].hypot(d[1]);
    if flat < 1.0 {
        return None;
    }
    let speed = f64::from(speed);
    let gravity = f64::from(gravity);
    let out = if gravity == 0.0 {
        let length = flat.hypot(d[2]);
        d.map(|axis| axis * speed / length)
    } else {
        let v2 = speed * speed;
        let root = v2 * v2 - gravity * (gravity * flat * flat + 2.0 * d[2] * v2);
        if root < 0.0 {
            return None;
        }
        let root = root.sqrt();
        let tangent = if high {
            (v2 + root) / (gravity * flat)
        } else {
            (gravity * flat * flat + 2.0 * d[2] * v2) / (flat * (v2 + root))
        };
        let flat_speed = speed / tangent.hypot(1.0);
        [
            d[0] / flat * flat_speed,
            d[1] / flat * flat_speed,
            tangent * flat_speed,
        ]
    }
    .map(|axis| axis as f32);
    out.into_iter().all(f32::is_finite).then_some(out)
}

fn arrival_angle(
    at: [f32; 3],
    target: [f32; 3],
    out: [f32; 3],
    normal: [f32; 3],
    gravity: f32,
) -> Option<f32> {
    if !at
        .into_iter()
        .chain(target)
        .chain(out)
        .chain(normal)
        .chain([gravity])
        .all(f32::is_finite)
        || gravity < 0.0
    {
        return None;
    }
    let flat = f64::from(out[0]).hypot(f64::from(out[1]));
    if flat == 0.0 {
        return None;
    }
    let time = (f64::from(target[0]) - f64::from(at[0]))
        .hypot(f64::from(target[1]) - f64::from(at[1]))
        / flat;
    let incoming = [
        f64::from(out[0]),
        f64::from(out[1]),
        f64::from(out[2]) - f64::from(gravity) * time,
    ];
    let normal = normal.map(f64::from);
    let denominator = incoming[0].hypot(incoming[1]).hypot(incoming[2])
        * normal[0].hypot(normal[1]).hypot(normal[2]);
    if denominator == 0.0 {
        return None;
    }
    let cosine = incoming
        .into_iter()
        .zip(normal)
        .map(|(v, n)| v * n)
        .sum::<f64>()
        .abs()
        / denominator;
    let angle = cosine.clamp(0.0, 1.0).acos().to_degrees() as f32;
    angle.is_finite().then_some(angle)
}

fn homed(
    atlas: &Atlas,
    base: AtlasClip<'_>,
    at: [f32; 3],
    velocity: [f32; 3],
    normal: [f32; 3],
    rules: &ProjectileFlightRules,
    gravity: f32,
) -> Option<[f32; 3]> {
    if !rules.home.is_finite() || rules.home <= 0.0 || !rules.home_align.is_finite() {
        return None;
    }
    let world = world_at(at);
    let speed = dot(velocity, velocity).sqrt();
    let explicit = rules
        .home_target
        .filter(|target| world_at(*target) == world);
    let door = atlas
        .portals
        .iter()
        .map(|portal| portal.a)
        .filter(|door| door.world.index() == world)
        .min_by(|a, b| {
            let point =
                |d: &AtlasDoor| add(add(d.base, world_offset(world)), [0.0, 0.0, d.height * 0.5]);
            dot(sub(point(a), at), sub(point(a), at))
                .total_cmp(&dot(sub(point(b), at), sub(point(b), at)))
        });
    let target = explicit.or_else(|| {
        door.map(|door| {
            add(
                add(door.base, world_offset(world)),
                [0.0, 0.0, door.height * 0.5],
            )
        })
    })?;
    let clear_start = |out: [f32; 3]| {
        let start = add(at, normal.map(|v| v * 0.5));
        let end = add(
            at,
            [
                out[0] * 0.2,
                out[1] * 0.2,
                out[2] * 0.2 - 0.5 * gravity * 0.2 * 0.2,
            ],
        );
        matches!(
            atlas.trace(base, &[], None, start, end).1,
            TraceOutcome::Miss { .. }
        )
    };
    if world > 0 && explicit.is_none() && rules.home_align > 0.0 {
        let door = door?;
        if let Some(out) = ballistic_arc(at, target, speed, false, gravity)
            && arrival_angle(at, target, out, door.normal, gravity)
                .is_some_and(|angle| angle <= rules.home_align)
            && dot(out, normal) > 0.0
        {
            return Some(out);
        }
        // A direct line would graze the doorway. Lob to ground on its normal
        // axis first; a subsequent floor bounce can enter nearly head-on.
        let sign = if dot(sub(at, target), door.normal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let mut best: Option<(f32, [f32; 3])> = None;
        for side in [sign, -sign] {
            for distance in [512.0, 768.0, 1024.0, 1280.0, 1536.0, 1792.0] {
                let point = add(target, door.normal.map(|v| v * distance * side));
                let top = [point[0], point[1], target[2] + 512.0];
                let end = [point[0], point[1], target[2] - 2048.0];
                let TraceOutcome::Hit {
                    end: floor,
                    normal: floor_normal,
                    ..
                } = atlas.trace(base, &[], None, top, end).1
                else {
                    continue;
                };
                if floor_normal[2] < 0.7 || world_at(floor) != world {
                    continue;
                }
                let stage = add(floor, [0.0, 0.0, 0.25]);
                let distance_squared = dot(sub(stage, at), sub(stage, at));
                if distance_squared < 128.0 * 128.0 {
                    continue;
                }
                let stage_speed = (speed * speed + 2.0 * gravity * (at[2] - stage[2]))
                    .max(0.0)
                    .sqrt()
                    * rules.parallel.min(rules.perpendicular);
                let Some(final_out) = ballistic_arc(stage, target, stage_speed, false, gravity)
                else {
                    continue;
                };
                if !arrival_angle(stage, target, final_out, door.normal, gravity)
                    .is_some_and(|angle| angle <= rules.home_align * 0.9)
                {
                    continue;
                }
                let Some(out) = ballistic_arc(at, stage, speed, true, gravity) else {
                    continue;
                };
                if dot(out, normal) > 0.0
                    && clear_start(out)
                    && best
                        .is_none_or(|(distance, _)| distance_squared.total_cmp(&distance).is_lt())
                {
                    best = Some((distance_squared, out));
                }
            }
        }
        return best.map(|(_, out)| out);
    }
    let mut out = ballistic_arc(
        at,
        target,
        speed,
        explicit.is_some() || rules.home >= 2.0,
        gravity,
    )?;
    if explicit.is_some() && !clear_start(out) {
        out = ballistic_arc(at, target, speed, false, gravity)?;
    }
    (dot(out, normal) > 0.0).then_some(out)
}

/// `velocity` off a floor of normal `n` turned up to `lift` degrees above
/// the horizontal, its speed and heading kept.
fn lifted(velocity: [f32; 3], n: [f32; 3], lift: f32) -> [f32; 3] {
    let speed = dot(velocity, velocity).sqrt();
    let flat = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
    let (sin, cos) = lift.to_radians().sin_cos();
    if lift <= 0.0 || n[2] < 0.7 || flat < 1e-3 || velocity[2] >= speed * sin {
        return velocity;
    }
    let scale = speed * cos / flat;
    [velocity[0] * scale, velocity[1] * scale, speed * sin]
}
