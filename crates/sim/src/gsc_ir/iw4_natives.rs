use super::runtime::to_text;
use super::*;
use bevy_ecs::prelude::World;

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "getdvar", |world, _, args| {
        Ok(Value::String(dvar(world, args)?.into()))
    });
    registry.register(Function, "getdvarint", |world, _, args| {
        Ok(Value::Int(atoi(&dvar(world, args)?)))
    });
    registry.register(Function, "getdvarfloat", |world, _, args| {
        Ok(Value::Float(atof(&dvar(world, args)?) as f32))
    });
    registry.register(Function, "getdvarvector", |world, _, args| {
        let text = dvar(world, args)?;
        let mut parts = text.split_ascii_whitespace().map(|p| atof(p) as f32);
        Ok(Value::Vector(std::array::from_fn(|_| {
            parts.next().unwrap_or(0.0)
        })))
    });
    registry.register(Function, "setdvar", |world, _, args| {
        let name = dvar_name(args)?;
        let value = dvar_value(args)?;
        runtime(world).dvars.insert(name, value);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setdvarifuninitialized", |world, _, args| {
        if args.len() != 2 {
            return Err("wrong number of parameters".into());
        }
        let name = dvar_name(args)?;
        let value = dvar_value(args)?;
        let mut runtime = runtime(world);
        let slot = runtime.dvars.entry(name).or_default();
        if slot.is_empty() {
            *slot = value;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "isstring", |_, _, args| {
        let value = args.first().ok_or("parameter 1 does not exist")?;
        Ok(Value::Int(matches!(value, Value::String(_)).into()))
    });
    registry.register(Function, "issplitscreen", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "tolower", |_, _, args| {
        Ok(Value::String(string(args, 0)?.to_ascii_lowercase().into()))
    });
    registry.register(Function, "precachestring", |world, _, args| {
        let Some(Value::LocalizedString(text)) = args.first() else {
            return Err("parameter 1 is not a localized string".into());
        };
        if !text.is_empty() {
            precache(world, "string", text.to_string())?;
        }
        Ok(Value::Undefined)
    });
    macro_rules! precache_by_name {
        ($($name:literal => $kind:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                precache(world, $kind, string(args, 0)?).map(|_| Value::Undefined)
            });
        )*};
    }
    precache_by_name! {
        "precachemodel" => "model",
        "precacheshader" => "shader",
        "precacheitem" => "item",
        "precacheshellshock" => "shellshock",
        "precachemenu" => "menu",
        "precacherumble" => "rumble",
        "precachestatusicon" => "statusicon",
        "precacheheadicon" => "headicon",
        "precacheminimapicon" => "minimapicon",
        "precachempanim" => "mpanim",
        "precacheleaderboards" => "leaderboards",
        "precachelocationselector" => "locationselector",
    }
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                runtime(world).presented.insert($name, args.to_vec());
                Ok(Value::Undefined)
            });
        )*};
    }
    presented![
        "setexpfog",
        "visionsetnaked",
        "visionsetnight",
        "visionsetmissilecam",
        "visionsetthermal",
        "visionsetpain",
        "setthermalbodymaterial",
        "ambientplay",
        "ambientstop",
    ];
    registry.register(Function, "getmapcustom", |world, _, args| {
        let key = string(args, 0)?;
        Ok(Value::string(
            crate::frame::FrameWorld::from_world(world).map_custom(&key),
        ))
    });
    registry.register(Function, "loadfx", |world, _, args| {
        let name = string(args, 0)?;
        if name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("fx/")) {
            return Err("effect name should start after the 'fx' folder.".into());
        }
        precache(world, "fx", name).map(Value::Int)
    });
    registry.register(Function, "makedvarserverinfo", |world, _, args| {
        let name = string(args, 0)?.to_ascii_lowercase();
        let value = if args.len() > 1 {
            dvar_value(args)?
        } else {
            String::new()
        };
        let mut runtime = runtime(world);
        runtime.server_info.insert(name.clone());
        runtime.dvars.entry(name).or_insert(value);
        Ok(Value::Undefined)
    });
}

pub(super) fn precache(world: &mut World, kind: &'static str, name: String) -> Result<i32, String> {
    let mut runtime = runtime(world);
    if let Some(&index) = runtime.precached.get(&(kind, name.clone())) {
        return Ok(index);
    }
    if !runtime.loading {
        return Err(format!(
            "{kind} must be precached before any wait statements in the gametype or level script"
        ));
    }
    let index = 1 + runtime.precached.keys().filter(|(k, _)| *k == kind).count() as i32;
    runtime.precached.insert((kind, name), index);
    Ok(index)
}

pub(crate) fn set_dvar(world: &mut World, name: &str, value: &str) {
    runtime(world)
        .dvars
        .insert(name.to_ascii_lowercase(), value.to_owned());
}

fn runtime(world: &mut World) -> bevy_ecs::world::Mut<'_, Runtime> {
    world.resource_mut::<Runtime>()
}

pub(super) fn string(args: &[Value], index: usize) -> Result<String, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("parameter {} does not exist", index + 1))?;
    match value {
        Value::String(text) => Ok(text.to_string()),
        other => to_text(other)
            .ok_or_else(|| format!("parameter {} cannot be cast to string", index + 1)),
    }
}

/// The dvar's text, or the optional second argument when the dvar does not exist.
fn dvar(world: &mut World, args: &[Value]) -> Result<String, String> {
    let fallback = match args.len() {
        1 => String::new(),
        2 => string(args, 1)?,
        _ => return Err("wrong number of parameters".into()),
    };
    let name = string(args, 0)?.to_ascii_lowercase();
    Ok(runtime(world).dvars.get(&name).cloned().unwrap_or(fallback))
}

fn dvar_name(args: &[Value]) -> Result<String, String> {
    let name = string(args, 0)?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!("Dvar {name} has an invalid dvar name"));
    }
    Ok(name.to_ascii_lowercase())
}

fn dvar_value(args: &[Value]) -> Result<String, String> {
    if let Some(Value::LocalizedString(reference)) = args.get(1) {
        return Ok(reference.to_string());
    }
    string(args, 1)
}

/// C `atoi`: optional whitespace and sign, then digits; wraps like the MSVC CRT.
pub(super) fn atoi(text: &str) -> i32 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// C `atof`: the longest decimal prefix, 0 when there is none.
pub(super) fn atof(text: &str) -> f64 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let bytes = text.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    let mantissa = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if !text[mantissa..end].bytes().any(|b| b.is_ascii_digit()) {
        return 0.0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1 + usize::from(matches!(bytes.get(end + 1), Some(b'-' | b'+')));
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                exponent += 1;
            }
            end = exponent;
        }
    }
    text[..end].parse().unwrap_or(0.0)
}
