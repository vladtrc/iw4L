#![allow(dead_code, unused_macros, unused_imports)]
use bevy_ecs::prelude::World;

use crate::script::host::players::entity_field;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

enum Stub {
    Undefined,
    Zero,
    ZeroVector,
    Dirt,
    Origin,
}

fn pending(
    world: &mut World,
    name: &'static str,
    default: Stub,
    receiver: &Value,
) -> Result<Value, String> {
    let calls = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let calls = runtime.unsupported.entry(name).or_default();
        *calls = calls.saturating_add(1);
        *calls
    };
    if calls == 1 {
        diag::info!(Sim, "gsc: campaign builtin {name} not built yet; stubbed");
    }
    Ok(match default {
        Stub::Undefined => Value::Undefined,
        Stub::Zero => Value::Int(0),
        Stub::ZeroVector => Value::Vector([0.0; 3]),
        Stub::Dirt => Value::string("dirt"),
        Stub::Origin => match receiver {
            Value::Object(id) => entity_field(world, *id, "origin"),
            _ => Value::Undefined,
        },
    })
}

macro_rules! stubs {
    ($registry:ident; $($namespace:ident $name:literal => $default:expr;)*) => {
        $($registry.register(Namespace::$namespace, $name, |world, receiver, _| {
            pending(world, $name, $default, receiver)
        });)*
    };
}

pub(crate) fn register(_registry: &mut NativeRegistry) {}
