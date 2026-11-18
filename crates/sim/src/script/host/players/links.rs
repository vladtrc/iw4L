use super::*;
use crate::script::host::args::{float, optional, string};
use crate::script::{Namespace, NativeRegistry};

type Axis = [[f32; 3]; 3];
const IDENTITY: Axis = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Curve {
    start_ms: i64,
    duration_ms: i64,
    accel: f32,
    decel: f32,
}

impl Curve {
    fn new(now: i64, seconds: f32, accel: f32, decel: f32) -> Result<Self, String> {
        if ![seconds, accel, decel].into_iter().all(f32::is_finite)
            || seconds < 0.0
            || accel < 0.0
            || decel < 0.0
            || accel + decel > seconds
            || seconds > i32::MAX as f32 / 1000.0
        {
            return Err(
                "link times must be finite, nonnegative, and ramps must fit the duration".into(),
            );
        }
        Ok(Self {
            start_ms: now,
            duration_ms: if seconds == 0.0 {
                0
            } else {
                ((seconds * 1000.0).round() as i64).max(1)
            },
            accel: if seconds == 0.0 { 0.0 } else { accel / seconds },
            decel: if seconds == 0.0 { 0.0 } else { decel / seconds },
        })
    }
    pub(super) fn finished(&self, now: i64) -> bool {
        now >= self.start_ms + self.duration_ms
    }
    pub(super) fn fraction(&self, now: i64) -> f32 {
        if self.finished(now) {
            return 1.0;
        }
        let t = ((now - self.start_ms) as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        let (a, d) = (self.accel, self.decel);
        // Integrate smoothstep velocity on each ramp. This gives continuous
        // acceleration at the cruise boundaries using our own easing model.
        let area = if a > 0.0 && t < a {
            let r = t / a;
            a * (r.powi(3) - 0.5 * r.powi(4))
        } else if d > 0.0 && t > 1.0 - d {
            let r = (t - (1.0 - d)) / d;
            1.0 - d - a * 0.5 + d * (r - r.powi(3) + 0.5 * r.powi(4))
        } else {
            t - a * 0.5
        };
        (area / (1.0 - (a + d) * 0.5)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Blend {
    pub curve: Curve,
    pub start_world_origin: [f32; 3],
    pub from_view: Axis,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClampTransition {
    pub curve: Curve,
    pub from: [f32; 4],
    pub to: [f32; 4],
}
impl ClampTransition {
    pub(super) fn sample(&self, now: i64) -> [f32; 4] {
        let t = self.curve.fraction(now);
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * t)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroundReference {
    pub entity: Option<u64>,
    pub axis: Axis,
}
impl Default for GroundReference {
    fn default() -> Self {
        Self {
            entity: None,
            axis: IDENTITY,
        }
    }
}

pub(crate) fn relative(axis: Axis, frame: Axis) -> Axis {
    math_iw4::matrix_multiply(axis, super::transpose(frame))
}
pub(crate) fn interpolate(from: Axis, to: Axis, fraction: f32) -> Axis {
    let a = glam::Quat::from_mat3(&glam::Mat3::from_cols_array_2d(&from)).normalize();
    let b = glam::Quat::from_mat3(&glam::Mat3::from_cols_array_2d(&to)).normalize();
    glam::Mat3::from_quat(a.slerp(b, fraction.clamp(0.0, 1.0))).to_cols_array_2d()
}

pub(crate) fn validate_parent(world: &World, child: u64, parent: u64) -> Result<(), String> {
    let runtime = world.resource::<RoundScript>();
    let mut at = Some(parent);
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = at {
        if id == child || !visited.insert(id) {
            return Err("entity link would create a parent cycle".into());
        }
        at = runtime
            .entities
            .get(&id)
            .and_then(|entity| entity.linked_to.as_ref().map(|link| link.parent))
            .or_else(|| {
                runtime.player_client(id).and_then(|client| {
                    runtime
                        .players
                        .get(&client)?
                        .link
                        .as_ref()
                        .map(|link| link.parent)
                })
            });
    }
    Ok(())
}

fn clock(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| i64::from(crate::level_time_ms(request.tick)))
}

pub(crate) fn write_view(world: &mut World, client: u32, view: [f32; 3]) {
    let mut frame = FrameWorld::from_world(world);
    if let Some(ps) = frame.player_mut(ClientId(client)) {
        for (i, angle) in view.into_iter().enumerate() {
            ps.delta_angles[i] += super::angle_delta(angle, ps.viewangles[i]);
        }
        ps.viewangles = view;
    }
}

pub(crate) fn refresh_references(world: &mut World) {
    let references: Vec<_> = world
        .resource::<RoundScript>()
        .players
        .iter()
        .map(|(&client, slot)| (client, slot.ground_reference.clone()))
        .collect();
    for (client, reference) in references {
        let Some(entity) = reference
            .entity
            .filter(|id| world.resource::<RoundScript>().live(id))
        else {
            continue;
        };
        let Value::Vector(angles) = entity_field(world, entity, "angles") else {
            continue;
        };
        let axis = math_iw4::angles_to_axis(angles);
        if axis == reference.axis {
            continue;
        }
        let delta = math_iw4::matrix_multiply(super::transpose(reference.axis), axis);
        if let Some(view) = FrameWorld::from_world(world)
            .player(ClientId(client))
            .map(|ps| ps.viewangles)
        {
            let view = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                math_iw4::angles_to_axis(view),
                delta,
            ));
            write_view(world, client, view);
        }
        if let Some(slot) = world.resource_mut::<RoundScript>().players.get_mut(&client) {
            slot.ground_reference.axis = axis;
            if let Some(link) = slot.link.as_mut() {
                link.link_axis = math_iw4::matrix_multiply(link.link_axis, delta);
            }
        }
    }
}

pub(super) fn effective_clamp(link: &PlayerLink, now: i64) -> Option<[f32; 4]> {
    link.clamp_transition
        .as_ref()
        .map(|transition| transition.sample(now))
        .or(link.clamp)
}

fn set_offset(world: &mut World, receiver: &Value, enabled: bool) -> Result<Value, String> {
    let client = super::super::natives::player::player(world, receiver)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let link = runtime
        .players
        .get_mut(&client)
        .and_then(|slot| slot.link.as_mut())
        .ok_or("player must be linked to change its local offset")?;
    link.use_offset = enabled;
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "playerlinkedoffsetenable",
        |world, receiver, _| set_offset(world, receiver, true),
    );
    registry.register(
        Namespace::Method,
        "playerlinkedoffsetdisable",
        |world, receiver, _| set_offset(world, receiver, false),
    );
    registry.register(
        Namespace::Method,
        "playerlinktoblend",
        |world, receiver, args| {
            let client = super::super::natives::player::player(world, receiver)?;
            let child = world
                .resource::<RoundScript>()
                .players
                .get(&client)
                .ok_or("player has disconnected")?
                .object;
            let parent =
                super::super::natives::engine::entity_id(world, super::super::args::arg(args, 0)?)?;
            validate_parent(world, child, parent)?;
            let tag = optional(args, 1, string)?
                .map(|name| name.to_ascii_lowercase())
                .filter(|name| !name.is_empty());
            let curve = Curve::new(
                clock(world),
                optional(args, 2, float)?.unwrap_or(1.0),
                optional(args, 3, float)?.unwrap_or(0.0),
                optional(args, 4, float)?.unwrap_or(0.0),
            )?;
            super::link_parent_pose(world, parent, tag.as_deref())?;
            super::apply_player_links(world);
            let (_, axis) = super::link_parent_pose(world, parent, tag.as_deref())?;
            let ps = FrameWorld::from_world(world)
                .player(ClientId(client))
                .copied()
                .ok_or("player has no physical state")?;
            let stance_mask = playerstate_iw4::pm_flags::SCRIPT_NO_STAND
                | playerstate_iw4::pm_flags::SCRIPT_NO_CROUCH;
            if ps.pm_flags & stance_mask == stance_mask {
                return Err("cannot link a player with standing and crouching disabled".into());
            }
            let frame = world.resource::<RoundScript>().players[&client]
                .ground_reference
                .axis;
            let from_view = relative(math_iw4::angles_to_axis(ps.viewangles), frame);
            super::link_player(
                world,
                client,
                PlayerLink {
                    parent,
                    tag: tag.map(Arc::from),
                    origin: [0.0; 3],
                    angles: [0.0; 3],
                    use_offset: false,
                    view: LinkView::Delta,
                    clamp: None,
                    parent_axis: relative(axis, frame),
                    view_fraction: 1.0,
                    use_tag_angles: true,
                    link_axis: axis,
                    restore_view: None,
                    blend: Some(Blend {
                        curve,
                        start_world_origin: ps.origin,
                        from_view,
                    }),
                    clamp_transition: None,
                },
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "lerpviewangleclamp",
        |world, receiver, args| {
            if args.len() != 7 {
                return Err(
                    "lerpviewangleclamp expects duration, two ramps and four angular limits".into(),
                );
            }
            let client = super::super::natives::player::player(world, receiver)?;
            let now = clock(world);
            let curve = Curve::new(now, float(args, 0)?, float(args, 1)?, float(args, 2)?)?;
            let mut limits = [0.0; 4];
            for (i, limit) in limits.iter_mut().enumerate() {
                let value = float(args, i + 3)?;
                if !value.is_finite() {
                    return Err("view angle limits must be finite".into());
                }
                *limit = value.clamp(0.0, 180.0);
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            let link = runtime
                .players
                .get_mut(&client)
                .and_then(|slot| slot.link.as_mut())
                .ok_or("player must be linked to change its view clamp")?;
            if link
                .blend
                .as_ref()
                .is_some_and(|blend| !blend.curve.finished(now))
            {
                return Err("cannot change view clamp during a player link blend".into());
            }
            let from = effective_clamp(link, now).unwrap_or([-180.0, 180.0, -180.0, 180.0]);
            link.clamp_transition = Some(ClampTransition {
                curve,
                from,
                to: [-limits[2], limits[3], -limits[0], limits[1]],
            });
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "playersetgroundreferenceent",
        |world, receiver, args| {
            let client = super::super::natives::player::player(world, receiver)?;
            let entity = match super::super::args::arg(args, 0)? {
                Value::Undefined => None,
                value => Some(super::super::natives::engine::entity_id(world, value)?),
            };
            let axis = match entity {
                None => IDENTITY,
                Some(entity) => match entity_field(world, entity, "angles") {
                    Value::Vector(angles) if angles.into_iter().all(f32::is_finite) => {
                        math_iw4::angles_to_axis(angles)
                    }
                    _ => return Err("ground reference has no finite orientation".into()),
                },
            };
            {
                let mut runtime = world.resource_mut::<RoundScript>();
                let slot = runtime
                    .players
                    .get_mut(&client)
                    .ok_or("player has disconnected")?;
                let rebase =
                    math_iw4::matrix_multiply(slot.ground_reference.axis, super::transpose(axis));
                if let Some(link) = slot.link.as_mut() {
                    link.parent_axis = math_iw4::matrix_multiply(link.parent_axis, rebase);
                    if let Some(blend) = link.blend.as_mut() {
                        blend.from_view = math_iw4::matrix_multiply(blend.from_view, rebase);
                    }
                }
                slot.ground_reference = GroundReference { entity, axis };
            }
            if entity.is_none()
                && let Some(mut view) = FrameWorld::from_world(world)
                    .player(ClientId(client))
                    .map(|ps| ps.viewangles)
            {
                view[2] = 0.0;
                write_view(world, client, view);
            }
            Ok(Value::Undefined)
        },
    );
}
