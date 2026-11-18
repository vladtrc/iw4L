use super::super::args::{int, optional, vector};
use super::super::natives::engine::{collider_entity, entity_trace, trace_ignore};
use super::*;
use crate::bullet_collision::{MASK_SHOT, TraceOutcome};
use crate::frame::FrameWorld;

#[derive(Clone, Debug)]
pub(crate) struct ShotCheck {
    target: u64,
    time: i64,
    result: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SightAccuracy {
    target: u64,
    visible_fraction: f32,
}

fn update_player_sight_accuracy(world: &mut World, id: u64) -> Result<(), String> {
    let target = targeting::enemy(world, id).filter(|target| {
        world
            .resource::<RoundScript>()
            .player_client(*target)
            .is_some()
    });
    let sampled = if let Some(target) = target {
        let (eye, _) = perception::eye(world, target)?;
        let client = world
            .resource::<RoundScript>()
            .player_client(target)
            .unwrap();
        let origin = FrameWorld::from_world(world)
            .player(crate::ClientId(client))
            .ok_or("sight player has no physical state")?
            .origin;
        let from = muzzle(world, id).map_or(perception::eye(world, id)?.0, |pose| pose.0);
        // Sample upper, middle and lower body. Volley uncertainty grows as
        // visible target area shrinks; collision still resolves every bullet.
        let mut visible = 0;
        for fraction in [0.25, 0.5, 1.0] {
            let point = std::array::from_fn(|i| origin[i] + (eye[i] - origin[i]) * fraction);
            let ignore = trace_ignore(world, Some(&Value::Object(id)));
            let exposed = match entity_trace(world, from, point, MASK_SHOT, ignore) {
                TraceOutcome::Miss { .. } => true,
                TraceOutcome::Hit { collider, .. } => {
                    collider_entity(world, collider) == Value::Object(target)
                }
                TraceOutcome::StartSolid { .. } => false,
                TraceOutcome::Invalid { reason } => {
                    return Err(format!("player sight trace unavailable: {reason:?}"));
                }
            };
            visible += usize::from(exposed);
        }
        Some(SightAccuracy {
            target,
            visible_fraction: visible as f32 / 3.0,
        })
    } else {
        None
    };
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .sight_accuracy = sampled;
    Ok(())
}
fn sight_spread(world: &World, id: u64, spread: f32) -> f32 {
    let runtime = world.resource::<RoundScript>();
    let Some(sample) = runtime.actors[&id]
        .sight_accuracy
        .as_ref()
        .filter(|sample| runtime.sentients[&id].target == Some(sample.target))
    else {
        return spread;
    };
    // Preserve cone area per visible body fraction. A fully hidden target
    // produces maximum uncertainty rather than a divide-by-zero or a fake hit.
    (spread.to_radians().tan() / sample.visible_fraction.max(f32::EPSILON).sqrt())
        .atan()
        .to_degrees()
        .min(90.0 * (1.0 - f32::EPSILON))
}

// A standing actor can fight without holding a cover node. Movement and
// scripted arrival owners keep control; authored combat scripts decide how to fire.
pub(super) fn stationary_behavior(world: &mut World, id: u64) -> Option<scripted::ControlKind> {
    let actor = &world.resource::<RoundScript>().actors[&id];
    if actor.species != Species::Human
        || actor.arrival.active()
        || actor.reacquire.active()
        || actor.badplace_escape.is_some()
    {
        return None;
    }
    let target = targeting::enemy(world, id)?;
    let runtime = world.resource::<RoundScript>();
    if !runtime.sentients.contains_key(&target)
        || runtime
            .actors
            .get(&target)
            .is_some_and(|actor| !actor.alive)
        || runtime.player_client(target).is_some_and(|client| {
            crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .is_none_or(|player| player.pm_type >= 2)
        })
    {
        return None;
    }
    Some(scripted::ControlKind::Combat)
}

pub(super) fn muzzle(world: &mut World, id: u64) -> Option<([f32; 3], [[f32; 3]; 3])> {
    let silenced = weapon(world, id)
        .and_then(|weapon| {
            FrameWorld::from_world(world)
                .combat_facts_for(weapon)
                .map(|facts| facts.silenced)
        })
        .unwrap_or(false);
    if silenced
        && let Some(pose) = super::super::presence::tag_world(world, id, "tag_flash_silenced")
    {
        return Some(pose);
    }
    super::super::presence::tag_world(world, id, "tag_flash")
}

pub(super) fn weapon(world: &mut World, id: u64) -> Option<u32> {
    let Value::String(name) = world
        .resource_mut::<RoundScript>()
        .object_field(id, "weapon")
    else {
        return None;
    };
    crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)
        .ok()
        .filter(|weapon| *weapon != 0)
}

fn point(world: &mut World, id: u64) -> Result<[f32; 3], String> {
    super::targeting::point(world, id)
}

fn clear_shot(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3]) -> Result<bool, String> {
    if !from.into_iter().chain(to).all(f32::is_finite) {
        return Err("shoot check positions must be finite".into());
    }
    let now = super::super::players::now_ms(world) as i32;
    if let Some(fog) = world.resource::<RoundScript>().engine.fog {
        let fog = fog.sample(now);
        // Our optical model stops target acquisition at one percent
        // transmittance. Partially opaque fog never creates a hard cutoff.
        const OPAQUE_TRANSMITTANCE: f32 = 0.01;
        if fog.max_opacity >= 1.0 - OPAQUE_TRANSMITTANCE && fog.density() > 0.0 {
            let distance = (0..3)
                .map(|i| (to[i] - from[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            let opaque_distance = fog.start_dist - OPAQUE_TRANSMITTANCE.ln() / fog.density();
            if distance > opaque_distance {
                return Ok(false);
            }
        }
    }
    let ignore = trace_ignore(world, Some(&Value::Object(id)));
    match entity_trace(world, from, to, MASK_SHOT, ignore) {
        TraceOutcome::Miss { .. } => Ok(true),
        TraceOutcome::Hit { collider, .. } => {
            let Value::Object(hit) = collider_entity(world, collider) else {
                return Ok(false);
            };
            let mut runtime = world.resource_mut::<RoundScript>();
            if runtime.object_field(id, "enemy") == Value::Object(hit) {
                return Ok(true);
            }
            Ok(runtime
                .sentients
                .get(&hit)
                .zip(runtime.sentients.get(&id))
                .is_some_and(|(hit, me)| hit.team != me.team))
        }
        TraceOutcome::StartSolid { .. } => Ok(false),
        TraceOutcome::Invalid { reason } => Err(format!("shoot trace is unavailable: {reason:?}")),
    }
}

fn node_sight_points(node: &asset_core::PathNode, eye_height: f32) -> Vec<[f32; 3]> {
    let mut eye = node.origin;
    eye[2] += eye_height;
    let mut points = vec![eye];
    // Authored peek permissions are compatibility data. Our observation poses
    // use this runtime's body height/diameter rather than retail offset tables.
    if node.spawnflags & 0x200 != 0 {
        let mut over = eye;
        over[2] += spatial::ACTOR_BOUNDS.half_size[2];
        points.push(over);
    }
    let (sin, cos) = node.yaw.to_radians().sin_cos();
    let diameter = spatial::ACTOR_BOUNDS.half_size[0] * 2.;
    for (flag, side) in [(0x400, 1.), (0x800, -1.)] {
        if node.spawnflags & flag != 0 {
            points.push([
                eye[0] - sin * diameter * side,
                eye[1] + cos * diameter * side,
                eye[2],
            ]);
        }
    }
    points
}
fn can_attack_enemy_node(world: &mut World, id: u64) -> Result<bool, String> {
    let Some(target) = targeting::enemy(world, id)
        .filter(|t| world.resource::<RoundScript>().sentients.contains_key(t))
    else {
        return Ok(false);
    };
    if !FrameWorld::from_world(world).collision_complete() {
        return Err("node visibility requires retained collision geometry".into());
    }
    let Some(from) = super::super::sentients::nearest::query(world, id)? else {
        return Ok(false);
    };
    let Some(to) = super::super::sentients::nearest::query(world, target)? else {
        return Ok(false);
    };
    if from == to {
        return Ok(true);
    }
    let (from_node, to_node) = {
        let nodes = &world.resource::<RoundScript>().path_nodes;
        (
            nodes.at(from).ok_or("invalid source path node")?.clone(),
            nodes.at(to).ok_or("invalid enemy path node")?.clone(),
        )
    };
    let source_eye = perception::eye(world, id)?.0;
    let target_eye = perception::eye(world, target)?.0;
    let Value::Vector(source_origin) = super::super::players::entity_field(world, id, "origin")
    else {
        return Err("actor has no origin".into());
    };
    let Value::Vector(target_origin) = super::super::players::entity_field(world, target, "origin")
    else {
        return Err("enemy has no origin".into());
    };
    let from = node_sight_points(&from_node, source_eye[2] - source_origin[2]);
    let to = node_sight_points(&to_node, target_eye[2] - target_origin[2]);
    for &start in &from {
        for &end in &to {
            let hit = super::super::trace::capsule(
                world,
                movement_iw4::GroundTraceInput {
                    start,
                    end,
                    mins: [0.; 3],
                    maxs: [0.; 3],
                    tracemask: 0x820015,
                },
                &[id, target],
            );
            if hit.startsolid == 0 && hit.allsolid == 0 && hit.fraction == 1. {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "updateplayersightaccuracy",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            update_player_sight_accuracy(world, id)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Namespace::Method,
        "canattackenemynode",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(Value::Int(can_attack_enemy_node(world, id)?.into()))
        },
    );
    registry.register(Namespace::Method, "shoot", shoot);
    registry.register(Namespace::Method, "shootblank", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let now = super::super::players::now_ms(world);
        if world.resource::<RoundScript>().actors[&id].last_attack_ms == Some(now) {
            return Ok(Value::Undefined);
        }
        let weapon = weapon(world, id).ok_or("actor has no registered weapon")?;
        let facts = FrameWorld::from_world(world)
            .combat_facts_for(weapon)
            .ok_or("actor weapon has no executable firing facts")?;
        if facts.weap_type != weapon_iw4::WEAPTYPE_BULLET {
            return Err("shootblank requires a bullet weapon".into());
        }
        let (from, axis) = muzzle(world, id).ok_or("actor weapon has no muzzle pose")?;
        finish_shot(world, id, weapon, from, axis[0], now);
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Method, "getmuzzlepos", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let at = if let Some((at, _)) = muzzle(world, id) {
            at
        } else {
            super::perception::eye(world, id)?.0
        };
        Ok(Value::Vector(at))
    });
    registry.register(
        Namespace::Method,
        "getmuzzlesideoffsetpos",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            let Some((mut at, _)) = muzzle(world, id) else {
                return Ok(Value::Vector(super::perception::eye(world, id)?.0));
            };
            let Value::Vector(origin) = super::super::players::entity_field(world, id, "origin")
            else {
                return Err("actor has no origin".into());
            };
            let Value::Vector(angles) = super::super::players::entity_field(world, id, "angles")
            else {
                return Err("actor has no orientation".into());
            };
            let (_, right, _) = math_iw4::angle_vectors(angles);
            let side = right[0] * (at[0] - origin[0]) + right[1] * (at[1] - origin[1]);
            for i in 0..2 {
                at[i] = origin[i] + side * right[i];
            }
            Ok(Value::Vector(at))
        },
    );
    registry.register(Namespace::Method, "canshoot", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let to = vector(args, 0)?;
        let offset = optional(args, 1, vector)?.unwrap_or([0.; 3]);
        if !to.into_iter().chain(offset).all(f32::is_finite) {
            return Err("shoot check positions must be finite".into());
        }
        if weapon(world, id).is_none() {
            return Ok(Value::Int(0));
        }
        let Some((from, _)) = muzzle(world, id) else {
            return Ok(Value::Int(0));
        };
        let from = std::array::from_fn(|i| from[i] + offset[i]);
        Ok(Value::Int(clear_shot(world, id, from, to)?.into()))
    });
    registry.register(
        Namespace::Method,
        "canshootenemy",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let interval = i64::from(optional(args, 0, int)?.unwrap_or(250));
            let now = super::super::players::now_ms(world);
            let target = {
                let mut runtime = world.resource_mut::<RoundScript>();
                match runtime.object_field(id, "enemy") {
                    Value::Object(target) if runtime.live(&target) => target,
                    _ => return Err("canshootenemy requires a current enemy".into()),
                }
            };
            if let Some(cache) = &world.resource::<RoundScript>().actors[&id].shot_check
                && cache.target == target
                && now >= cache.time
                && now - cache.time <= interval
            {
                return Ok(Value::Int(cache.result.into()));
            }
            let Some((from, _)) = muzzle(world, id) else {
                return Ok(Value::Int(0));
            };
            let to = point(world, target)?;
            let result = clear_shot(world, id, from, to)?;
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .shot_check = Some(ShotCheck {
                target,
                time: now,
                result,
            });
            Ok(Value::Int(result.into()))
        },
    );
}

fn crowded_spread(world: &World, id: u64, spread: f32) -> f32 {
    let runtime = world.resource::<RoundScript>();
    if runtime.actors[&id].no_attacker_accuracy_mod {
        return spread;
    }
    let Some(target) = runtime.sentients[&id]
        .target
        .filter(|target| runtime.player_client(*target).is_some())
    else {
        return spread;
    };
    let attackers = super::targeting::attacker_count(runtime, target).max(1) as f32;
    // Share the incoming-hit budget: tangent-plane cone area grows with
    // the number of attackers, so its radius grows with sqrt(count).
    let radius = spread.to_radians().tan() * attackers.sqrt();
    radius.atan().to_degrees().min(90.0 * (1.0 - f32::EPSILON))
}

fn shoot_parameters(args: &[Value]) -> Result<(f32, Option<[f32; 3]>), String> {
    let (accuracy, explicit) = match args.len() {
        0 => (1.0, None),
        1 => (float(args, 0)?, None),
        _ => (1.0, Some(vector(args, 1)?)),
    };
    if !accuracy.is_finite() || accuracy < 0.0 {
        return Err("shoot accuracy multiplier must be finite and nonnegative".into());
    }
    if explicit.is_some_and(|point| !point.into_iter().all(f32::is_finite)) {
        return Err("shoot target must be finite".into());
    }
    Ok((accuracy, explicit))
}

fn shoot(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let (accuracy, explicit) = shoot_parameters(args)?;
    let now = super::super::players::now_ms(world);
    if world.resource::<RoundScript>().actors[&id].last_attack_ms == Some(now) {
        return Ok(Value::Undefined);
    }
    let weapon = weapon(world, id).ok_or("actor has no registered weapon")?;
    let facts = FrameWorld::from_world(world)
        .combat_facts_for(weapon)
        .ok_or("actor weapon has no executable firing facts")?;
    let kind = weapon_iw4::fire_weapon_kind(facts.weap_type, facts.weap_class)
        .ok_or("actor weapon has no firing implementation")?;
    if kind == weapon_iw4::FireWeaponKind::Flame {
        return Err("actor flame weapon firing is not implemented".into());
    }
    let (from, axis) = muzzle(world, id).ok_or("actor weapon has no muzzle pose")?;
    let target = match explicit {
        Some(point) => Some(point),
        None => {
            let enemy = world
                .resource_mut::<RoundScript>()
                .object_field(id, "enemy");
            match enemy {
                Value::Object(enemy) if world.resource::<RoundScript>().live(&enemy) => {
                    Some(point(world, enemy)?)
                }
                _ => None,
            }
        }
    };
    let aim = target
        .map_or(glam::Vec3::from_array(axis[0]), |to| {
            glam::Vec3::from_array(to) - glam::Vec3::from_array(from)
        })
        .try_normalize()
        .ok_or("shoot requires a nonzero direction")?;
    let spread = facts.hip_spread_stand_min.max(0.0) * accuracy;
    if !spread.is_finite() || spread >= 90.0 {
        return Err("shoot spread must be finite and below 90 degrees".into());
    }
    let spread = if kind == weapon_iw4::FireWeaponKind::Bullet {
        crowded_spread(world, id, sight_spread(world, id, spread))
    } else {
        spread
    };
    let presence = {
        let runtime = world.resource::<RoundScript>();
        let entity = &runtime.entities[&id];
        entity.presence.ok_or("actor has no physical presence")?
    };
    let pellets = if kind == weapon_iw4::FireWeaponKind::Bullet {
        facts.pellet_count()
    } else {
        1
    };
    if pellets <= 0 {
        return Err("actor weapon must fire at least one pellet".into());
    }
    for _ in 0..pellets {
        let direction = super::super::natives::math::sample_shot_direction(world, aim, spread)?;
        if kind == weapon_iw4::FireWeaponKind::Bullet {
            super::super::turrets::fire_bullet(
                world,
                id,
                from,
                direction.to_array(),
                weapon,
                None,
                Some(presence),
            )?;
        } else {
            super::super::weapons::launch_actor(world, id, weapon, from, direction.to_array())?;
        }
    }
    finish_shot(world, id, weapon, from, aim.to_array(), now);
    let team = world.resource::<RoundScript>().sentients[&id].team;
    super::super::ai_events::point(
        world,
        super::super::ai_events::Event::GunshotTeammate,
        Some(id),
        from,
        1u32 << team,
    );
    super::super::ai_events::point(
        world,
        if facts.silenced {
            super::super::ai_events::Event::SilencedShot
        } else {
            super::super::ai_events::Event::Gunshot
        },
        Some(id),
        from,
        !(1u32 << team),
    );
    Ok(Value::Undefined)
}

fn finish_shot(
    world: &mut World,
    id: u64,
    weapon: u32,
    from: [f32; 3],
    direction: [f32; 3],
    now: i64,
) {
    let number = {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.actors.get_mut(&id).unwrap().last_attack_ms = Some(now);
        runtime.entities[&id].number
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::FIRE_WEAPON,
        crate::EntityEventPayload {
            number,
            attacker_entity_num: number,
            weapon,
            origin: from,
            direction,
            ..Default::default()
        },
    );
}
