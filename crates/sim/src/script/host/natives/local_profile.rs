use super::super::args::{arg, int, kind, string};
use super::player::player;
use crate::LocalPlayerProfile;
use crate::script::{Namespace::Method, NativeRegistry, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    for name in ["updategamerprofile", "updategamerprofileall"] {
        registry.register(crate::script::Namespace::Function, name, |world, _, _| {
            world.resource_mut::<LocalPlayerProfile>().request_save();
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "getplayerintelisfound", |world, receiver, args| {
        if player(world, receiver)? != 0 {
            return Err("local player profile unavailable".into());
        }
        let number = int(args, 0)?;
        Ok(Value::Int(i32::from(
            world
                .resource::<LocalPlayerProfile>()
                .intel_is_found(number),
        )))
    });
    registry.register(Method, "setplayerintelfound", |world, receiver, args| {
        if player(world, receiver)? != 0 {
            return Err("local player profile unavailable".into());
        }
        let number = int(args, 0)?;
        world
            .resource_mut::<LocalPlayerProfile>()
            .set_intel_found(number);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getplayersetting", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name = string(args, 0)?;
        if name.to_ascii_lowercase().starts_with("gameskill") {
            let key = format!("g_gameskill_player_{}", client + 1);
            return world
                .resource::<crate::script::MatchScript>()
                .dvars
                .get(&key)
                .map(|value| Value::String(value.as_str().into()))
                .ok_or_else(|| format!("missing player setting dvar {key}"));
        }
        Err(format!("unknown player setting {name}"))
    });
    registry.register(
        Method,
        "getlocalplayerprofiledata",
        |world, receiver, args| {
            if player(world, receiver)? != 0 {
                return Ok(Value::Undefined);
            }
            let name = string(args, 0)?;
            world
                .resource::<LocalPlayerProfile>()
                .get_int(&name)
                .map(Value::Int)
                .ok_or_else(|| format!("unknown local player profile field {name}"))
        },
    );
    registry.register(
        Method,
        "setlocalplayerprofiledata",
        |world, receiver, args| {
            if player(world, receiver)? != 0 {
                return Ok(Value::Undefined);
            }
            if args.len() < 2 {
                return Err("setlocalplayerprofiledata expects at least two arguments".into());
            }
            let name = string(args, 0)?;
            if world
                .resource::<LocalPlayerProfile>()
                .get_int(&name)
                .is_none()
            {
                return Err(format!("unknown local player profile field {name}"));
            }
            let value = match arg(args, 1)? {
                Value::Int(value) => *value,
                other => return Err(format!("parameter 2 is {}, not an int", kind(other))),
            };
            world
                .resource_mut::<LocalPlayerProfile>()
                .set_int(&name, value);
            Ok(Value::Undefined)
        },
    );
}
