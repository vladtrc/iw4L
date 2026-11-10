use super::super::args::string;
use crate::script::{NativeRegistry, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(
        crate::script::Namespace::Function,
        "getcommandfromkey",
        |world, _, args| {
            let key = string(args, 0)?;
            let bindings = world
                .get_resource::<crate::LocalKeyBindings>()
                .ok_or("local key bindings unavailable")?;
            Ok(Value::string(bindings.command_for_key(&key)))
        },
    );
    registry.register(
        crate::script::Namespace::Function,
        "getkeybinding",
        |world, _, args| {
            let command = string(args, 0)?;
            let bindings = world
                .get_resource::<crate::LocalKeyBindings>()
                .ok_or("local key bindings unavailable")?;
            let keys = bindings.keys_for_command(&command);
            let first = keys.first().map_or_else(
                || "KEY_UNBOUND".into(),
                |&key| crate::local_input::key_label(key),
            );
            let second = keys
                .get(1)
                .map_or_else(String::new, |&key| crate::local_input::key_label(key));
            let count = keys.len() as i32;
            let array = super::super::arrays::new_array(world, vec![])?;
            let Value::Array(id) = array else {
                unreachable!()
            };
            world
                .resource_mut::<crate::script::RoundScript>()
                .arrays
                .get_mut(&id)
                .unwrap()
                .extend([
                    (
                        crate::script::ArrayKey::String("key1".into()),
                        Value::LocalizedString(first.into()),
                    ),
                    (
                        crate::script::ArrayKey::String("key2".into()),
                        Value::LocalizedString(second.into()),
                    ),
                    (
                        crate::script::ArrayKey::String("count".into()),
                        Value::Int(count),
                    ),
                ]);
            Ok(array)
        },
    );
}
