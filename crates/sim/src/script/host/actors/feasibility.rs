use super::super::args::{int, vector};
use super::*;
use trace_iw4::Trace;

type V3 = [f32; 3];

#[derive(Clone, Copy)]
enum Obstacle {
    Geometry,
    Sentient { may_cross: bool },
}

fn trace(
    world: &mut World,
    id: u64,
    start: V3,
    end: V3,
    bounds: crate::EntityBodyBounds,
    mask: u32,
) -> (Trace, Obstacle) {
    let (mins, maxs) = bounds.abs_bounds([0.; 3]);
    let hit = super::super::trace::capsule(
        world,
        movement_iw4::GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask: mask,
        },
        &[id],
    );
    let runtime = world.resource::<RoundScript>();
    let object = (hit.hit_type == trace_iw4::HITTYPE_ENTITY)
        .then(|| {
            runtime
                .players
                .get(&u32::from(hit.hit_id))
                .map(|slot| slot.object)
                .or_else(|| {
                    runtime
                        .entities
                        .iter()
                        .find(|(_, entity)| entity.number == i32::from(hit.hit_id))
                        .map(|(&object, _)| object)
                })
        })
        .flatten();
    let obstacle = match object.filter(|object| runtime.sentients.contains_key(object)) {
        None => Obstacle::Geometry,
        Some(object) => Obstacle::Sentient {
            // Actor_SetAnimScript's moveMode byte is nonzero for code movement.
            // AnimSpecific (including custom/scripted) sets that byte to zero.
            may_cross: runtime.actors.get(&object).is_some_and(|actor| {
                actor.anim_script_kind == Some(scripted::ControlKind::Move)
                    || actor.motion.pile_up_actor == Some(id)
            }),
        },
    };
    (hit, obstacle)
}

// Path_CheckDrop (821c3dd8) and Path_PredictionTrace (821c3f30).
fn check_drop(
    point: &mut V3,
    mask: u32,
    trace: &mut impl FnMut(V3, V3, crate::EntityBodyBounds, u32) -> (Trace, Obstacle),
) -> bool {
    let top = [point[0], point[1], point[2] + 18.];
    let bottom = [point[0], point[1], top[2] - 72.];
    let (hit, _) = trace(top, bottom, spatial::ACTOR_BOUNDS, mask);
    *point = std::array::from_fn(|i| top[i] + (bottom[i] - top[i]) * hit.fraction);
    hit.fraction != 1.
        && ((hit.surface_flags & 0x200 != 0 && hit.normal[2] >= 0.) || hit.normal[2] > 0.7)
}

fn prediction(
    mut start: V3,
    end: V3,
    mut mask: u32,
    allow_allsolid: bool,
    trace: &mut impl FnMut(V3, V3, crate::EntityBodyBounds, u32) -> (Trace, Obstacle),
) -> bool {
    let mut bounds = spatial::ACTOR_BOUNDS;
    bounds.midpoint[2] += 5.;
    bounds.half_size[2] -= 5.;
    let mut short_stair = false;
    loop {
        let horizontal_end = [end[0], end[1], start[2]];
        let (hit, obstacle) = trace(start, horizontal_end, bounds, mask);
        if hit.allsolid != 0 && !allow_allsolid {
            return false;
        }
        let mut point = std::array::from_fn(|i| {
            start[i] + (horizontal_end[i] - start[i]) * (hit.fraction * 0.99)
        });
        if hit.startsolid == 0 && hit.fraction == 1. {
            return end[2] - point[2] <= 72. && check_drop(&mut point, mask, trace);
        }
        let stairs = hit.surface_flags & 0x200 != 0 && hit.normal[2] >= 0.;
        match obstacle {
            Obstacle::Geometry => {
                let distance_sq = (point[0] - start[0]).powi(2) + (point[1] - start[1]).powi(2);
                if distance_sq >= 225. {
                    short_stair = false;
                } else if !stairs || short_stair {
                    return false;
                } else {
                    short_stair = true;
                }
            }
            Obstacle::Sentient { may_cross: false } => return false,
            Obstacle::Sentient { may_cross: true } => mask &= !0x4000,
        }
        if !check_drop(&mut point, mask, trace) {
            return false;
        }
        // Native advances from a lower floor directly, otherwise raises the sweep
        // above its previous height by one step (30 on an authored stair surface).
        if point[2] >= start[2] {
            point[2] = start[2] + if stairs { 30. } else { 10. };
        }
        start = point;
    }
}

fn trace_check(
    start: V3,
    end: V3,
    mask: u32,
    check_drop_height: bool,
    allow_allsolid: bool,
    trace: &mut impl FnMut(V3, V3, crate::EntityBodyBounds, u32) -> (Trace, Obstacle),
) -> bool {
    let top = [end[0], end[1], end[2] + 72.];
    let bottom = [end[0], end[1], end[2] - 72.];
    let (ground, _) = trace(
        top,
        bottom,
        crate::EntityBodyBounds {
            midpoint: [0.; 3],
            half_size: [0.; 3],
            contents: 0,
        },
        mask | 0x2000,
    );
    if ground.startsolid != 0 {
        return false;
    }
    let ground_z = top[2] + (bottom[2] - top[2]) * ground.fraction;
    if check_drop_height && start[2] - ground_z > 32. {
        return false;
    }
    prediction(start, end, mask, allow_allsolid, trace)
}

fn may_move(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    from_point: bool,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let offset = usize::from(from_point);
    let end = vector(args, offset)?;
    let supplied_start = from_point.then(|| vector(args, 0)).transpose()?;
    let check_drop_height = args.len() <= offset + 1 || int(args, offset + 1)? != 0;
    let ignore_actors = args.len() > offset + 2 && int(args, offset + 2)? != 0;
    let (start, mask) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.entities[&id].linked_to.is_some() {
            return Ok(Value::Int(0));
        }
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let actor = &runtime.actors[&id];
        if !from_point && actor.suppression.blocks_move(origin, end) {
            return Ok(Value::Int(0));
        }
        let mask = if ignore_actors {
            actor.motion.trace_mask & !0x02004000
        } else {
            actor.motion.trace_mask | 0x02004000
        };
        (supplied_start.unwrap_or(origin), mask)
    };
    if start.into_iter().chain(end).any(|v| !v.is_finite()) {
        return Err("movement query points must be finite".into());
    }
    let result = trace_check(
        start,
        end,
        mask,
        check_drop_height,
        !from_point,
        &mut |a, b, bounds, mask| trace(world, id, a, b, bounds, mask),
    );
    if result {
        let now = world
            .get_resource::<crate::step::StepRequest>()
            .map_or(0, |r| crate::level_time_ms(r.tick));
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .last_may_move_time = now;
    }
    Ok(Value::Int(i32::from(result)))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "maymovetopoint",
        |world, receiver, args| may_move(world, receiver, args, false),
    );
    registry.register(
        Namespace::Method,
        "maymovefrompointtopoint",
        |world, receiver, args| may_move(world, receiver, args, true),
    );
}
