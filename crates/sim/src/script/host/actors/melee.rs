use super::*;
use crate::bullet_collision::{ColliderId, MASK_SHOT, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::host::entity_damage::{ObjectHit, apply_object_hit};
use crate::script::host::natives::engine::{collider_entity, entity_trace, trace_ignore};

// Scripted NPC melee reaches 64 world units from the eye (PC and SP symbols).
const REACH_UNITS: f32 = 64.0;
// Surface flags are authored collision data, shared with the weapon trace path.
const SURFACE_NO_IMPACT: u32 = 0x10;

fn surface_flags(collider: ColliderId) -> u32 {
    match collider {
        ColliderId::World { surface_flags, .. }
        | ColliderId::EntityDObjBone { surface_flags, .. }
        | ColliderId::EntityLinkedBrush { surface_flags, .. } => surface_flags,
        _ => 0,
    }
}

fn direction(
    world: &mut World,
    id: u64,
    from: [f32; 3],
    explicit: Option<[f32; 3]>,
) -> Result<[f32; 3], String> {
    let target = {
        let runtime = world.resource::<RoundScript>();
        runtime.sentients[&id]
            .target
            .filter(|target| runtime.sentients.contains_key(target))
    };
    let aim = if let Some(target) = target {
        let to = perception::eye(world, target)?.0;
        let mut delta = std::array::from_fn::<_, 3, _>(|i| f64::from(to[i]) - f64::from(from[i]));
        if let Some(explicit) = explicit {
            let horizontal = f64::from(explicit[0]).hypot(f64::from(explicit[1]));
            if horizontal == 0.0 {
                return Err("melee direction toward an enemy needs a horizontal component".into());
            }
            let distance = delta[0].hypot(delta[1]);
            delta[0] = f64::from(explicit[0]) / horizontal * distance;
            delta[1] = f64::from(explicit[1]) / horizontal * distance;
        }
        delta
    } else {
        let forward = explicit.unwrap_or_else(|| {
            let look = world.resource::<RoundScript>().actors[&id]
                .motion
                .look_angles;
            super::super::presence::tag_world(world, id, "tag_eye").map_or_else(
                || math_iw4::angles_to_axis([look[0], look[1], 0.0])[0],
                |(_, axis)| axis[0],
            )
        });
        forward.map(f64::from)
    };
    let length = aim.iter().map(|n| n * n).sum::<f64>().sqrt();
    if !length.is_finite() || length == 0.0 {
        return Err("melee requires a finite nonzero direction".into());
    }
    Ok(aim.map(|n| (n / length) as f32))
}

fn impact(
    world: &mut World,
    id: u64,
    weapon: u32,
    point: [f32; 3],
    direction: [f32; 3],
    collider: ColliderId,
    target: Option<u64>,
) {
    let runtime = world.resource::<RoundScript>();
    let number = runtime.entities[&id].number;
    let flesh = target.is_some_and(|target| runtime.sentients.contains_key(&target));
    let other_entity_num = target.map_or(i32::from(trace_iw4::ENTITYNUM_WORLD), |target| {
        runtime.entities[&target].number
    });
    let surface_flags = surface_flags(collider);
    let tick = world.resource::<crate::step::StepRequest>().tick;
    FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        if flesh {
            entity_iw4::EntityEventKind::MELEE_HIT
        } else {
            entity_iw4::EntityEventKind::MELEE_MISS
        },
        crate::EntityEventPayload {
            number,
            attacker_entity_num: number,
            other_entity_num,
            weapon,
            origin: point,
            direction,
            surface_flags,
            surf_type: trace_iw4::surface_type_from_flags(surface_flags) as u8,
            ..Default::default()
        },
    );
}

fn melee(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let explicit = if args.is_empty() {
        None
    } else {
        Some(super::super::args::vector(args, 0)?)
    };
    if explicit.is_some_and(|direction| !direction.into_iter().all(f32::is_finite)) {
        return Err("melee direction must be finite".into());
    }
    let now = super::super::players::now_ms(world);
    if world.resource::<RoundScript>().actors[&id].last_attack_ms == Some(now) {
        return Ok(Value::Undefined);
    }
    let weapon = combat::weapon(world, id).ok_or("actor has no registered melee weapon")?;
    let facts = FrameWorld::from_world(world)
        .weapon_combat_row(weapon)
        .ok_or("actor melee weapon has no damage facts")?;
    if facts.melee_damage < 0 {
        return Err("actor melee damage cannot be negative".into());
    }
    if world.resource::<RoundScript>().entities[&id]
        .presence
        .is_none()
    {
        return Err("actor has no physical melee presence".into());
    }
    let (from, _) = perception::eye(world, id)?;
    if !from.into_iter().all(f32::is_finite) {
        return Err("actor melee eye position must be finite".into());
    }
    let aim = direction(world, id, from, explicit)?;
    let end = std::array::from_fn(|i| from[i] + aim[i] * REACH_UNITS);
    if !FrameWorld::from_world(world).collision_complete() {
        return Err("melee collision data is unavailable".into());
    }
    let hit = entity_trace(
        world,
        from,
        end,
        MASK_SHOT,
        trace_ignore(world, Some(receiver)),
    );
    let (point, collider) = match hit {
        TraceOutcome::Hit { end, collider, .. } => (end, Some(collider)),
        TraceOutcome::StartSolid { end, collider } => (end, collider),
        TraceOutcome::Miss { end } => (end, None),
        TraceOutcome::Invalid { reason } => return Err(format!("melee trace failed: {reason:?}")),
    };
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .last_attack_ms = Some(now);
    super::super::triggers::damage_line_from(
        world,
        from,
        point,
        facts.melee_damage,
        Value::Object(id),
        "MOD_MELEE",
    );
    let Some(collider) = collider else {
        return Ok(Value::Undefined);
    };
    if surface_flags(collider) & SURFACE_NO_IMPACT != 0 {
        return Ok(Value::Undefined);
    }
    let target = match collider_entity(world, collider) {
        Value::Object(target) => Some(target),
        _ => None,
    };
    if target.is_some_and(|target| {
        let runtime = world.resource::<RoundScript>();
        runtime
            .sentients
            .get(&target)
            .is_some_and(|sentient| sentient.team == runtime.sentients[&id].team)
    }) {
        return Ok(Value::Undefined);
    }
    impact(world, id, weapon, point, aim, collider, target);
    if let ColliderId::World { glass_encoded, .. } = collider
        && let Some(piece) = glass_encoded.checked_sub(1)
    {
        let mut frame = FrameWorld::from_world(world);
        let mut random = *frame.stuck_holdrand_mut();
        frame.world_objects_mut().apply_glass_hit(
            u32::from(piece),
            u32::from(crate::world_objects::GLASS_MELEE_DAMAGE),
            now as i32,
            crate::world_objects::GlassHit {
                position: point,
                direction: aim,
            },
            &mut || crate::item::random_unit(&mut random),
        );
        *frame.stuck_holdrand_mut() = random;
    }
    let Some(target) =
        target.filter(|target| world.resource::<RoundScript>().entities[target].can_damage)
    else {
        return Ok(Value::Undefined);
    };
    let (bone, hitloc) = match collider {
        ColliderId::Player { hitloc, .. } => (None, hitloc),
        ColliderId::EntityDObjBone {
            bone,
            part_classification,
            ..
        } => (Some(usize::from(bone)), part_classification),
        _ => (None, 0),
    };
    apply_object_hit(
        world,
        target,
        &ObjectHit {
            amount: facts.melee_damage,
            attacker: Value::Object(id),
            inflictor: Value::Object(id),
            means: "MOD_MELEE",
            weapon,
            point,
            dir: aim,
            bone,
            hitloc,
            flags: 0,
        },
    );
    Ok(Value::Object(target))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "melee", melee);
}
