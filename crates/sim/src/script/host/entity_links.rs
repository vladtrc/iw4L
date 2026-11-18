use super::args::{arg, int, optional, string};
use super::entities::{EntityKind, Link};
use super::natives::engine::entity_id;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

type Axis = [[f32; 3]; 3];
const IDENTITY: Axis = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
const POSITION_RESPONSE_SECONDS: f32 = 0.08;
const ORIENTATION_RESPONSE_SECONDS: f32 = 0.04;

#[derive(Clone, Debug, PartialEq)]
pub struct BlendToTag {
    start_ms: i64,
    origin: [f32; 3],
    axis: Axis,
    yaw_only: bool,
    collision_slide: bool,
}

fn now(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(crate::level_time_ms(r.tick)))
}

impl BlendToTag {
    fn pose(&self, now: i64) -> (Axis, [f32; 3]) {
        let seconds = (now - self.start_ms).max(0) as f32 * 0.001;
        // Zero-velocity critically damped springs in the parent's frame.
        // Position settles over a few hundred ms; orientation catches up
        // sooner so a seated actor faces the attachment during the join.
        let residual = |response_seconds: f32| {
            let t = seconds / response_seconds;
            (1.0 + t) * (-t).exp()
        };
        let position = residual(POSITION_RESPONSE_SECONDS);
        let rotation = residual(ORIENTATION_RESPONSE_SECONDS);
        (
            super::players::links::interpolate(self.axis, IDENTITY, 1.0 - rotation),
            self.origin.map(|v| v * position),
        )
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "enablelinkto", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let entity = &runtime.entities[&id];
        if entity.link_enabled {
            return Err("entity already has linkTo enabled".into());
        }
        if !matches!(entity.kind, EntityKind::Map | EntityKind::Spawned)
            || world
                .resource::<super::mechanics::Mechanics>()
                .has_physics_body(id)
        {
            return Err(format!(
                "entity '{}' does not support enableLinkTo",
                entity.classname
            ));
        }
        world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .unwrap()
            .link_enabled = true;
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "linktoblendtotag",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            let parent = entity_id(world, arg(args, 0)?)?;
            if world.resource::<RoundScript>().player_client(id).is_some() {
                return Err("linktoblendtotag requires a non-player entity".into());
            }
            require_enabled(world, id)?;
            super::players::links::validate_parent(world, id, parent)?;
            let tag = optional(args, 1, string)?
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| !s.is_empty());
            let yaw_only = optional(args, 2, int)?.unwrap_or(1) != 0;
            let collision_slide = optional(args, 3, int)?.unwrap_or(0) != 0;
            let (base, parent_axis) =
                super::players::link_parent_pose(world, parent, tag.as_deref())?;
            let (origin, angles) = {
                let mut runtime = world.resource_mut::<RoundScript>();
                (
                    runtime.object_field(id, "origin"),
                    runtime.object_field(id, "angles"),
                )
            };
            let (Value::Vector(origin), Value::Vector(angles)) = (origin, angles) else {
                return Err("linked entity requires a position and orientation".into());
            };
            if !origin.into_iter().chain(angles).all(f32::is_finite) {
                return Err("linked entity pose must be finite".into());
            }
            let axis =
                super::players::links::relative(math_iw4::angles_to_axis(angles), parent_axis);
            let delta: [f32; 3] = std::array::from_fn(|i| origin[i] - base[i]);
            let local = std::array::from_fn(|i| (0..3).map(|j| delta[j] * parent_axis[i][j]).sum());
            let blend = BlendToTag {
                start_ms: now(world),
                origin: local,
                axis,
                yaw_only,
                collision_slide,
            };
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.entities.get_mut(&id).unwrap().linked_to = Some(Link {
                parent,
                origin: local,
                angles: math_iw4::axis_to_angles(axis),
                tag: tag.map(Arc::from),
                scripted_parent: None,
                blend: Some(blend),
            });
            if let Some(actor) = runtime.actors.get_mut(&id) {
                actor.navigation = Default::default();
                actor.motion.velocity = [0.; 3];
                actor.motion.wish_delta = [0.; 3];
            }
            Ok(Value::Undefined)
        },
    );
}

pub(crate) fn enabled_at_spawn(kind: &EntityKind, classname: &str) -> bool {
    matches!(
        kind,
        EntityKind::Actor
            | EntityKind::ActorCorpse { .. }
            | EntityKind::Drone
            | EntityKind::Vehicle
            | EntityKind::Missile(_)
            | EntityKind::Corpse { .. }
    ) || matches!(
        classname,
        "script_model"
            | "script_brushmodel"
            | "script_origin"
            | "light"
            | "misc_turret"
            | "misc_mg42"
    )
}

pub(crate) fn require_enabled(world: &World, id: u64) -> Result<(), String> {
    let entity = &world.resource::<RoundScript>().entities[&id];
    if entity.link_enabled {
        Ok(())
    } else {
        Err(format!(
            "entity '{}' requires EnableLinkTo before linking",
            entity.classname
        ))
    }
}

fn ordered(links: BTreeMap<u64, Link>) -> Vec<(u64, Link)> {
    let mut children: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut ready = BTreeSet::new();
    for (&id, link) in &links {
        if links.contains_key(&link.parent) {
            children.entry(link.parent).or_default().push(id);
        } else {
            ready.insert(id);
        }
    }
    let mut result = Vec::with_capacity(links.len());
    while let Some(id) = ready.pop_first() {
        result.push((id, links[&id].clone()));
        if let Some(children) = children.remove(&id) {
            ready.extend(children);
        }
    }
    result
}

pub(crate) fn advance(world: &mut World) {
    let links = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter_map(|(&id, e)| Some((id, e.linked_to.clone()?)))
        .collect();
    let now = now(world);
    let mut updated = BTreeSet::new();
    for (id, link) in ordered(links) {
        if !world.resource::<RoundScript>().live(&link.parent) {
            world
                .resource_mut::<RoundScript>()
                .entities
                .get_mut(&id)
                .unwrap()
                .linked_to = None;
            continue;
        }
        if link.tag.is_some() && updated.contains(&link.parent) {
            super::presence::settle_collision(world);
        }
        let Some((base, parent_axis)) = super::presence::link_parent_pose(world, &link) else {
            world
                .resource_mut::<RoundScript>()
                .entities
                .get_mut(&id)
                .unwrap()
                .linked_to = None;
            continue;
        };
        let (local_axis, local_origin) = link.blend.as_ref().map_or_else(
            || (math_iw4::angles_to_axis(link.angles), link.origin),
            |blend| blend.pose(now),
        );
        let (axis, target) =
            math_iw4::matrix_multiply43(local_axis, local_origin, parent_axis, base);
        let origin = if let Some(blend) = &link.blend {
            constrained_origin(world, id, target, blend.collision_slide)
        } else {
            target
        };
        let mut angles = math_iw4::axis_to_angles(axis);
        if link.blend.as_ref().is_some_and(|blend| blend.yaw_only)
            && let Value::Vector(current) = world
                .resource_mut::<RoundScript>()
                .object_field(id, "angles")
        {
            angles[0] = current[0];
            angles[2] = current[2];
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        if let Some(actor) = runtime.actors.get_mut(&id) {
            actor.navigation = Default::default();
            actor.motion.velocity = [0.; 3];
            actor.motion.wish_delta = [0.; 3];
        }
        updated.insert(id);
    }
}

fn constrained_origin(world: &mut World, id: u64, target: [f32; 3], slide: bool) -> [f32; 3] {
    let (start, owner, actor) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Vector(start) = runtime.object_field(id, "origin") else {
            return target;
        };
        (
            start,
            runtime.entities[&id]
                .presence
                .map(crate::AuthorityModelOwner::ScriptModel),
            runtime.actors.contains_key(&id),
        )
    };
    // Non-actor movers have no character capsule. Their normal linkage is
    // kinematic; collision fallback is meaningful only for actor movement.
    if !actor {
        return target;
    }
    let bounds = super::actors::spatial::ACTOR_BOUNDS;
    let (mins, maxs) = bounds.abs_bounds([0.; 3]);
    let mut trace = |start, end, mask, _| {
        let frame = FrameWorld::from_world(world);
        let input = movement_iw4::GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask: mask,
        };
        let mut hit = frame.trace_world_hull_except(input, owner);
        let content = frame.content();
        frame.visit_players(|client, ps| {
            let body = crate::entity_body::EntityBodyPose {
                bounds: crate::EntityBodyBounds {
                    midpoint: [0., 0., 35.],
                    half_size: [15., 15., 35.],
                    contents: 0x0200_0000,
                },
                origin: ps.origin,
                number: client.0 as u16,
            };
            hit = body.clip(&content, hit, input);
        });
        hit
    };
    let hit = trace(start, target, super::actors::physics::LINK_MASK, false);
    if hit.startsolid == 0 && hit.allsolid == 0 && hit.fraction >= 1. {
        return target;
    }
    if slide {
        super::actors::physics::slide_link(start, target, &mut trace)
    } else {
        start
    }
}
