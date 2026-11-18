use std::collections::BTreeMap;

use crate::EntityCollisionCapabilities;
use crate::script::{ArrayKey, RoundScript, Value};

fn field<'a>(runtime: &'a RoundScript, object: u64, name: &str) -> Option<&'a Value> {
    let symbol = runtime
        .program
        .as_ref()?
        .symbol_ids
        .get(name)
        .or_else(|| runtime.dynamic_symbols.get(name))?;
    runtime.objects.get(&object)?.get(symbol)
}

fn body_state(runtime: &RoundScript, object: u64) -> Option<&BTreeMap<ArrayKey, Value>> {
    let Value::Array(parts) = field(runtime, object, "destructible_parts")? else {
        return None;
    };
    let Value::Object(body) = runtime.arrays.get(parts)?.get(&ArrayKey::Integer(0))? else {
        return None;
    };
    let Value::Array(values) = field(runtime, *body, "v")? else {
        return None;
    };
    runtime.arrays.get(values).map(|values| &**values)
}

fn integer(values: &BTreeMap<ArrayKey, Value>, key: &str) -> Option<i64> {
    match values.get(&ArrayKey::String(key.into()))? {
        Value::Int(value) => Some(i64::from(*value)),
        Value::Float(value) if value.is_finite() => Some(*value as i64),
        _ => None,
    }
}

pub(crate) fn entities(runtime: &RoundScript, models: &[EntityCollisionCapabilities]) {
    for (object, entity) in &runtime.entities {
        let Some(id) = entity.presence else {
            continue;
        };
        let Some(body) = body_state(runtime, *object) else {
            continue;
        };
        let state = integer(body, "currentState");
        let clip = models
            .iter()
            .find(|row| row.owner.script_model() == Some(id))
            .and_then(|row| row.dobj.as_ref())
            .and_then(|dobj| dobj.semantic_state.tree.as_ref())
            .and_then(|tree| {
                tree.nodes
                    .iter()
                    .filter(|node| node.state.weight > 0.0)
                    .find_map(|node| node.clip.as_deref())
            });
        perf::instant!("truck", "iw4l.sim", |ctx| {
            let (script_model_id, health, death_clip): (u32, Option<i64>, Option<&str>) =
                (id.to_wire(), integer(body, "health"), clip);
            ctx.arg("script_model_id", i64::from(script_model_id));
            ctx.arg("state", state);
            ctx.arg("health", health);
            ctx.arg("death_clip", death_clip);
        });
    }
}

pub(crate) fn sight(
    world: &mut bevy_ecs::prelude::World,
    start: [f32; 3],
    end: [f32; 3],
    mask: u32,
    ignored: &[u64],
) -> trace_iw4::Trace {
    capsule(
        world,
        movement_iw4::GroundTraceInput {
            start,
            end,
            mins: [0.; 3],
            maxs: [0.; 3],
            tracemask: mask,
        },
        ignored,
    )
}

pub(crate) fn capsule(
    world: &mut bevy_ecs::prelude::World,
    input: movement_iw4::GroundTraceInput,
    ignored: &[u64],
) -> trace_iw4::Trace {
    super::presence::settle_collision(world);
    capsule_settled(world, input, ignored)
}

/// A query in a read-only batch whose caller has already settled collision.
/// No script, animation or entity writes may occur between settling and this call.
pub(crate) fn capsule_settled(
    world: &mut bevy_ecs::prelude::World,
    input: movement_iw4::GroundTraceInput,
    ignored: &[u64],
) -> trace_iw4::Trace {
    let (owners, clients) = {
        let runtime = world.resource::<RoundScript>();
        (
            ignored
                .iter()
                .filter_map(|id| runtime.entities.get(id)?.presence)
                .map(crate::AuthorityModelOwner::ScriptModel)
                .collect::<Vec<_>>(),
            ignored
                .iter()
                .filter_map(|id| runtime.player_client(*id))
                .collect::<Vec<_>>(),
        )
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    let mut hit = frame.trace_hull_between(input, &owners);
    let content = frame.content();
    frame.visit_players(|client, player| {
        if clients.contains(&client.0) || player.pm_type >= 2 {
            return;
        }
        let height = if player.pm_flags & 1 != 0 {
            30.
        } else if player.pm_flags & 2 != 0 {
            50.
        } else {
            70.
        };
        let body = crate::entity_body::EntityBodyPose {
            bounds: crate::EntityBodyBounds {
                midpoint: [0., 0., height * 0.5],
                half_size: [15., 15., height * 0.5],
                contents: 0x02000000,
            },
            origin: player.origin,
            number: client.0 as u16,
        };
        hit = body.clip(&content, hit, input);
    });
    hit
}
