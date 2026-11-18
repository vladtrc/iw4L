use super::*;
use crate::frame::FrameWorld;

const MASK_SIGHT: u32 = 0x0280_d803;
const MASK_SIGHT_NEAR: u32 = 0x0280_d801;

type EyePose = ([f32; 3], [f32; 3]);

/// Valid only while the caller keeps the settled scene and actor poses fixed.
/// Never retained across script execution, movement, or animation updates.
#[derive(Default)]
pub(in super::super) struct EyeBatch(std::collections::BTreeMap<u64, Result<EyePose, String>>);

impl EyeBatch {
    fn eye(&mut self, world: &mut World, id: u64) -> Result<EyePose, String> {
        self.0
            .entry(id)
            .or_insert_with(|| eye_settled(world, id))
            .clone()
    }
}

pub(in super::super) const FIELDS: &[&str] = &[
    "fovcosine",
    "fovcosinebusy",
    "maxsightdistsqrd",
    "sightlatency",
    "ignoreclosefoliage",
];

#[derive(Clone, Debug)]
pub(crate) struct Sight {
    fov_cosine: f32,
    fov_cosine_busy: f32,
    max_distance_sq: f32,
    latency: i32,
    ignore_close_foliage: bool,
    pub has_threatening_enemy: bool,
}
impl Default for Sight {
    fn default() -> Self {
        Self {
            fov_cosine: 0.01,
            fov_cosine_busy: 0.574,
            max_distance_sq: 8192. * 8192.,
            latency: 100,
            ignore_close_foliage: false,
            has_threatening_enemy: false,
        }
    }
}
impl Sight {
    pub(in super::super) fn maximum_distance(&self) -> f32 {
        self.max_distance_sq.max(1.0).sqrt()
    }
    pub(in super::super) fn load(&self, field: &str) -> Option<Value> {
        Some(match field {
            "fovcosine" => Value::Float(self.fov_cosine),
            "fovcosinebusy" => Value::Float(self.fov_cosine_busy),
            "maxsightdistsqrd" => Value::Float(self.max_distance_sq),
            "sightlatency" => Value::Int(self.latency),
            "ignoreclosefoliage" => Value::Int(i32::from(self.ignore_close_foliage)),
            _ => return None,
        })
    }
    pub(in super::super) fn store(&mut self, field: &str, value: &Value) -> Result<bool, String> {
        if !FIELDS.contains(&field) {
            return Ok(false);
        }
        let args = std::slice::from_ref(value);
        match field {
            "sightlatency" => self.latency = super::super::super::args::int(args, 0)?,
            "ignoreclosefoliage" => {
                self.ignore_close_foliage = super::super::super::args::int(args, 0)? != 0
            }
            _ => {
                let number = float(args, 0)?;
                if !number.is_finite() {
                    return Err("actor sight parameter must be finite".into());
                }
                match field {
                    "fovcosine" => self.fov_cosine = number,
                    "fovcosinebusy" => self.fov_cosine_busy = number,
                    _ => self.max_distance_sq = number,
                }
            }
        }
        Ok(true)
    }
}

fn origin(world: &mut World, id: u64) -> Result<[f32; 3], String> {
    match super::super::super::players::entity_field(world, id, "origin") {
        Value::Vector(at) => Ok(at),
        _ => Err("sight entity has no origin".into()),
    }
}

pub(in super::super) fn eye(world: &mut World, id: u64) -> Result<([f32; 3], [f32; 3]), String> {
    if world.resource::<RoundScript>().player_client(id).is_none() {
        super::super::super::presence::settle_collision(world);
    }
    eye_settled(world, id)
}

pub(in super::super) fn eye_settled(
    world: &mut World,
    id: u64,
) -> Result<([f32; 3], [f32; 3]), String> {
    let client = world.resource::<RoundScript>().player_client(id);
    if let Some(client) = client {
        let ps = FrameWorld::from_world(world)
            .player(crate::ClientId(client))
            .copied()
            .ok_or("sight player has no physical state")?;
        let mut at = ps.origin;
        at[2] += ps.view_height_current;
        return Ok((at, math_iw4::angles_to_axis(ps.viewangles)[0]));
    }
    if let Some((at, axis)) = super::super::super::presence::tag_world_settled(world, id, "tag_eye")
    {
        let length = axis[0][0].hypot(axis[0][1]);
        return Ok((
            at,
            if length > 0. {
                [axis[0][0] / length, axis[0][1] / length, 0.]
            } else {
                [0.; 3]
            },
        ));
    }
    let mut at = origin(world, id)?;
    if let Some(actor) = world.resource::<RoundScript>().actors.get(&id) {
        at[2] += 64.;
        let look = actor.motion.look_angles;
        return Ok((at, math_iw4::angles_to_axis([look[0], look[1], 0.])[0]));
    }
    let number = world.resource::<RoundScript>().entities[&id].number;
    if let Some(mover) = FrameWorld::from_world(world).script_mover_by_number(number) {
        for (v, mid) in at.iter_mut().zip(mover.box_mid) {
            *v += mid;
        }
    }
    Ok((at, [0.; 3]))
}

fn vehicle_target(
    world: &mut World,
    target: u64,
    observer: [f32; 3],
    sentient: bool,
) -> Result<[f32; 3], String> {
    let tag = super::super::super::presence::tag_world_settled(world, target, "tag_eye");
    if !sentient && let Some((at, _)) = tag {
        return at
            .iter()
            .all(|v| v.is_finite())
            .then_some(at)
            .ok_or_else(|| "sight vehicle eye tag has non-finite coordinates".into());
    }
    let entity = &world.resource::<RoundScript>().entities[&target];
    let number = entity.number;
    let presence = entity.presence.ok_or("sight vehicle has no model owner")?;
    let frame = FrameWorld::from_world(world);
    let row = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))
        .ok_or("sight vehicle has no current model capability")?;
    let (origin, angles) = row
        .followed_pose
        .ok_or("sight vehicle has no settled pose")?;
    let bounds = row
        .body
        .map(|body| (body.bounds.midpoint, body.bounds.half_size))
        .or_else(|| {
            frame
                .script_mover_by_number(number)
                .filter(|mover| mover.box_half.iter().any(|&v| v > 0.0))
                .map(|mover| (mover.box_mid, mover.box_half))
        })
        .or_else(|| row.dobj.as_ref()?.capability.as_ref()?.bounds)
        .ok_or("sight vehicle has no authored bounds")?;
    let (midpoint, half) = bounds;
    if !origin
        .iter()
        .chain(angles.iter())
        .chain(midpoint.iter())
        .chain(half.iter())
        .all(|v| v.is_finite())
        || half.iter().any(|&v| v < 0.0)
    {
        return Err("sight vehicle has invalid bounds or pose".into());
    }
    let axis = math_iw4::angles_to_axis(angles);
    let center = tag.map_or_else(
        || {
            std::array::from_fn(|i| {
                origin[i] + (0..3).map(|j| axis[j][i] * midpoint[j]).sum::<f32>()
            })
        },
        |(at, _)| at,
    );
    if !observer.iter().chain(center.iter()).all(|v| v.is_finite()) {
        return Err("sight vehicle target has non-finite coordinates".into());
    }
    let point = face_target(observer, center, half, axis);
    if !point.iter().all(|v| v.is_finite()) {
        return Err("sight vehicle target cannot be represented".into());
    }
    perf::instant!("actor_vehicle_sight_point", "iw4l.sim", |ctx| {
        ctx.arg("target", i64::from(number));
        ctx.arg("x", f64::from(point[0]));
        ctx.arg("y", f64::from(point[1]));
        ctx.arg("z", f64::from(point[2]));
    });
    Ok(point)
}

fn face_target(from: [f32; 3], center: [f32; 3], half: [f32; 3], axis: [[f32; 3]; 3]) -> [f32; 3] {
    let delta: [f32; 3] = std::array::from_fn(|i| from[i] - center[i]);
    let local: [f32; 3] = std::array::from_fn(|j| (0..3).map(|i| axis[j][i] * delta[i]).sum());
    let ratios: [f32; 3] = std::array::from_fn(|i| {
        if local[i] == 0.0 {
            f32::INFINITY
        } else {
            half[i] / local[i].abs()
        }
    });
    let scale = ratios.into_iter().fold(f32::INFINITY, f32::min);
    if !scale.is_finite() || ratios.iter().filter(|&&v| v == scale).count() != 1 {
        return center;
    }
    std::array::from_fn(|i| from[i] + (center[i] - from[i]) * (1.0 - scale).abs())
}

fn fov(world: &mut World, id: u64, target: u64, now: i32) -> (f32, bool) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let enemy = runtime.object_field(id, "enemy");
    let favorite = runtime.object_field(id, "favoriteenemy");
    let actor = &runtime.actors[&id];
    let recent = actor.sentient_info.get(&target).is_some_and(|info| {
        info.last_known_position.is_some() && now.saturating_sub(info.last_known_time) <= 999
    });
    if actor.alert_level >= 3
        && recent
        && [enemy.clone(), favorite].contains(&Value::Object(target))
    {
        (0., true)
    } else if actor.sight.has_threatening_enemy && enemy != Value::Object(target) {
        (actor.sight.fov_cosine_busy, false)
    } else {
        (actor.sight.fov_cosine, false)
    }
}

fn in_range(
    from: [f32; 3],
    to: [f32; 3],
    forward: [f32; 3],
    cosine: f32,
    distance_sq: f32,
) -> bool {
    let delta: [f32; 3] = std::array::from_fn(|i| to[i] - from[i]);
    let length_sq = delta.iter().map(|v| v * v).sum::<f32>();
    if distance_sq != 0. && length_sq > distance_sq {
        return false;
    }
    let dot = (0..3).map(|i| forward[i] * delta[i]).sum::<f32>();
    cosine == 0. || (dot >= 0. && dot * dot >= length_sq * cosine * cosine)
}

fn update_cache(
    world: &mut World,
    id: u64,
    target: u64,
    visible: bool,
    now: i32,
) -> Result<(), String> {
    let at = origin(world, target)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let target_is_enemy = runtime.object_field(id, "enemy") == Value::Object(target);
    let info = runtime
        .actors
        .get_mut(&id)
        .unwrap()
        .sentient_info
        .entry(target)
        .or_default();
    let newly_seen = visible && !info.visible;
    info.visible = visible;
    info.visibility_updated_at = Some(now);
    if visible {
        info.last_visible_time = Some(now);
        info.last_known_time = now;
        info.last_known_position = Some(at);
        info.attack_time = 0;
    }
    if newly_seen && target_is_enemy {
        runtime
            .pending_notifies
            .push((Value::Object(id), "enemy_visible".into(), vec![]));
    }
    Ok(())
}

pub(in super::super) fn can_see(
    world: &mut World,
    id: u64,
    target: u64,
    latency: i32,
) -> Result<bool, String> {
    can_see_internal(world, id, target, latency, None)
}
pub(in super::super) fn can_see_batched(
    world: &mut World,
    id: u64,
    target: u64,
    latency: i32,
    eyes: &mut EyeBatch,
) -> Result<bool, String> {
    can_see_internal(world, id, target, latency, Some(eyes))
}
fn can_see_internal(
    world: &mut World,
    id: u64,
    target: u64,
    latency: i32,
    eyes: Option<&mut EyeBatch>,
) -> Result<bool, String> {
    let now = super::super::super::players::now_ms(world) as i32;
    let (is_sentient, reciprocal) = {
        let runtime = world.resource::<RoundScript>();
        if runtime.sentients.contains_key(&target)
            && let Some(info) = runtime.actors[&id].sentient_info.get(&target)
            && info
                .visibility_updated_at
                .is_some_and(|time| now <= time.saturating_add(latency))
        {
            return Ok(info.visible);
        }
        (
            runtime.sentients.contains_key(&target),
            runtime.actors.contains_key(&target),
        )
    };
    let vehicle = world.resource::<RoundScript>().entities[&target].kind
        == super::super::super::entities::EntityKind::Vehicle;
    if eyes.is_none() {
        super::super::super::presence::settle_collision(world);
    }
    let ((from, forward), (to, reverse_forward)) = match eyes {
        Some(eyes) => {
            let source = eyes.eye(world, id)?;
            let destination = if vehicle {
                (
                    vehicle_target(world, target, source.0, is_sentient)?,
                    [0.; 3],
                )
            } else {
                eyes.eye(world, target)?
            };
            (source, destination)
        }
        None => {
            let source = eye_settled(world, id)?;
            let destination = if vehicle {
                (
                    vehicle_target(world, target, source.0, is_sentient)?,
                    [0.; 3],
                )
            } else {
                eye_settled(world, target)?
            };
            (source, destination)
        }
    };
    let maximum = {
        let runtime = world.resource::<RoundScript>();
        let source_limit = runtime.actors[&id].sight.max_distance_sq;
        runtime.sentients.get(&target).map_or(source_limit, |s| {
            source_limit.min(s.max_visible_dist * s.max_visible_dist)
        })
    };
    let (cosine, _) = fov(world, id, target, now);
    let mut visible = in_range(from, to, forward, cosine, maximum);
    if visible {
        let ignore_close = world.resource::<RoundScript>().actors[&id]
            .sight
            .ignore_close_foliage;
        let distance = world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get("ai_foliageseethroughdist")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(128.);
        let clear = |world: &mut World, start, end, mask| {
            let hit = super::super::super::trace::capsule_settled(
                world,
                movement_iw4::GroundTraceInput {
                    start,
                    end,
                    mins: [0.; 3],
                    maxs: [0.; 3],
                    tracemask: mask,
                },
                &[id, target],
            );
            hit.fraction == 1. && hit.startsolid == 0 && hit.allsolid == 0
        };
        let delta: [f32; 3] = std::array::from_fn(|i| to[i] - from[i]);
        let length_sq = delta.iter().map(|v| v * v).sum::<f32>();
        if ignore_close {
            let length = length_sq.sqrt();
            let near = if length > 0. {
                std::array::from_fn(|i| from[i] + delta[i] * (distance / length))
            } else {
                from
            };
            visible =
                clear(world, near, to, MASK_SIGHT) && clear(world, from, near, MASK_SIGHT_NEAR);
        } else {
            visible = clear(
                world,
                from,
                to,
                if length_sq >= distance * distance {
                    MASK_SIGHT
                } else {
                    MASK_SIGHT_NEAR
                },
            );
        }
    }
    if is_sentient {
        update_cache(world, id, target, visible, now)?;
    }
    if visible && reciprocal {
        let (cosine, skip_check) = fov(world, target, id, now);
        let runtime = world.resource::<RoundScript>();
        let maximum = runtime.actors[&target]
            .sight
            .max_distance_sq
            .min(runtime.sentients[&id].max_visible_dist.powi(2));
        let visible = skip_check || in_range(to, from, reverse_forward, cosine, maximum);
        update_cache(world, target, id, visible, now)?;
    }
    Ok(visible)
}

pub(in super::super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "cansee", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let target = super::super::super::natives::engine::entity_id(world, arg(args, 0)?)?;
        let latency = if args.len() > 1 {
            super::super::super::args::int(args, 1)?
        } else {
            world.resource::<RoundScript>().actors[&id]
                .sight
                .latency
                .max(250)
        };
        Ok(Value::Int(i32::from(can_see(world, id, target, latency)?)))
    });
}
