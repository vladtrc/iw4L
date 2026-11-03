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
    runtime.arrays.get(values)
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
