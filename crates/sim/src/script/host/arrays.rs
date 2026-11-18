use super::args::kind;
use crate::script::{ArrayKey, RoundScript, Value};
use bevy_ecs::prelude::World;

pub fn array_values(world: &World, value: &Value) -> Result<Vec<Value>, String> {
    let Value::Array(id) = value else {
        return Err(format!("{} is not an array", kind(value)));
    };
    Ok(world
        .resource::<RoundScript>()
        .arrays
        .get(id)
        .ok_or("invalid array reference")?
        .values()
        .cloned()
        .collect())
}

pub fn new_array(world: &mut World, values: Vec<Value>) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(
        id,
        values
            .into_iter()
            .enumerate()
            .map(|(i, v)| (ArrayKey::Integer(i as i32), v))
            .collect(),
    );
    Ok(Value::Array(id))
}

pub(crate) fn iteration_key(
    world: &World,
    value: &Value,
    previous: Option<&Value>,
) -> Result<Value, String> {
    let Value::Array(id) = value else {
        return Err(format!("{} is not an array", kind(value)));
    };
    let runtime = world.resource::<RoundScript>();
    let array = runtime.arrays.get(id).ok_or("invalid array reference")?;
    let entry = if let Some(previous) = previous {
        let key = match previous {
            Value::Int(n) => ArrayKey::Integer(*n),
            Value::String(s) => ArrayKey::String(s.clone()),
            _ => return Err("array key must be an int or string".into()),
        };
        if !array.contains_key(&key) {
            return Err("array key does not exist".into());
        }
        array.range(..key).next_back()
    } else {
        array.last_key_value()
    };
    Ok(entry.map_or(Value::Undefined, |(key, _)| match key {
        ArrayKey::Integer(n) => Value::Int(*n),
        ArrayKey::String(s) => Value::String(s.clone()),
    }))
}
