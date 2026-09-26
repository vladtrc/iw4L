use super::iw4_natives::{atoi, string};
use super::*;
use bevy_ecs::prelude::World;

pub(super) fn arg(args: &[Value], index: usize) -> Result<&Value, String> {
    args.get(index)
        .ok_or_else(|| format!("parameter {} does not exist", index + 1))
}

pub(super) fn float(args: &[Value], index: usize) -> Result<f32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n as f32),
        Value::Float(n) => Ok(*n),
        other => Err(format!(
            "parameter {} is {}, not a float",
            index + 1,
            kind(other)
        )),
    }
}

pub(super) fn int(args: &[Value], index: usize) -> Result<i32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n),
        Value::Float(n) => Ok(*n as i32),
        other => Err(format!(
            "parameter {} is {}, not an int",
            index + 1,
            kind(other)
        )),
    }
}

pub(super) fn vector(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    match arg(args, index)? {
        Value::Vector(v) => Ok(*v),
        other => Err(format!(
            "parameter {} is {}, not a vector",
            index + 1,
            kind(other)
        )),
    }
}

pub(super) fn optional<T>(
    args: &[Value],
    index: usize,
    read: fn(&[Value], usize) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match args.get(index) {
        None | Some(Value::Undefined) => Ok(None),
        Some(_) => read(args, index).map(Some),
    }
}

pub(super) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "an int",
        Value::Float(_) => "a float",
        Value::String(_) => "a string",
        Value::LocalizedString(_) => "a localized string",
        Value::Vector(_) => "a vector",
        Value::Entity(_) => "an entity",
        Value::Object(_) => "an object",
        Value::Array(_) => "an array",
        Value::Function(_) | Value::Builtin(_) => "a function",
        Value::Animation { .. } => "an animation",
        Value::AnimationTree(_) => "an animtree",
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(super) fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    dot(d, d)
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length == 0.0 {
        v
    } else {
        scale(v, 1.0 / length)
    }
}

fn nearest_on_segment(a: [f32; 3], b: [f32; 3], point: [f32; 3]) -> [f32; 3] {
    let segment = sub(b, a);
    let length_sq = dot(segment, segment);
    if length_sq == 0.0 {
        return a;
    }
    let t = (dot(sub(point, a), segment) / length_sq).clamp(0.0, 1.0);
    add(a, scale(segment, t))
}

pub(super) fn random(world: &mut World) -> u32 {
    let mut runtime = world.resource_mut::<Runtime>();
    let mut x = runtime.rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    runtime.rng = x;
    x
}

fn random_unit(world: &mut World) -> f32 {
    (random(world) >> 8) as f32 / (1u32 << 24) as f32
}

pub(super) fn array_values(world: &World, value: &Value) -> Result<Vec<Value>, String> {
    let Value::Array(id) = value else {
        return Err(format!("{} is not an array", kind(value)));
    };
    Ok(world
        .resource::<Runtime>()
        .arrays
        .get(id)
        .ok_or("invalid array reference")?
        .values()
        .cloned()
        .collect())
}

pub(super) fn new_array(world: &mut World, values: Vec<Value>) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
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

fn table<'a>(tables: &'a BTreeMap<String, StringTable>, name: &str) -> Option<&'a StringTable> {
    tables.get(&entities::table_key(name))
}

fn table_search(table: &StringTable, column: usize, value: &str) -> Option<usize> {
    (0..table.rows).find(|&row| {
        table
            .cell(row, column)
            .is_some_and(|cell| cell.eq_ignore_ascii_case(value))
    })
}

fn table_lookup(world: &World, args: &[Value]) -> Result<String, String> {
    if args.len() != 4 {
        return Err("wrong number of parameters".into());
    }
    let tables = world.resource::<Runtime>().tables.clone();
    let name = string(args, 0)?;
    let column = int(args, 1)?;
    let value = string(args, 2)?;
    let result = int(args, 3)?;
    let Some(table) = table(&tables, &name) else {
        return Ok(String::new());
    };
    if column < 0 || result < 0 {
        return Ok(String::new());
    }
    Ok(table_search(table, column as usize, &value)
        .and_then(|row| table.cell(row, result as usize))
        .unwrap_or("")
        .to_owned())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    macro_rules! unary {
        ($($name:literal => $f:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let f: fn(f32) -> f32 = $f;
                Ok(Value::Float(f(float(args, 0)?)))
            });
        )*};
    }
    unary! {
        "abs" => f32::abs,
        "ceil" => f32::ceil,
        "floor" => f32::floor,
        "cos" => |d| d.to_radians().cos(),
        "sin" => |d| d.to_radians().sin(),
        "tan" => |d| d.to_radians().tan(),
        "angleclamp180" => |a| {
            let a = a - 360.0 * (a / 360.0).floor();
            if a > 180.0 { a - 360.0 } else { a }
        },
        "angleclamp" => |a| a - 360.0 * (a / 360.0).floor(),
    }
    macro_rules! inverse {
        ($($name:literal => $f:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let value = float(args, 0)?;
                if !(-1.0..=1.0).contains(&value) {
                    return Err(format!("{value} out of range"));
                }
                let f: fn(f32) -> f32 = $f;
                Ok(Value::Float(f(value).to_degrees()))
            });
        )*};
    }
    inverse! { "acos" => f32::acos, "asin" => f32::asin }
    registry.register(Function, "atan", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.atan().to_degrees()))
    });
    registry.register(Function, "sqrt", |_, _, args| {
        let value = float(args, 0)?;
        if value < 0.0 {
            return Err("negative sqrt".into());
        }
        Ok(Value::Float(value.sqrt()))
    });
    registry.register(Function, "int", |_, _, args| {
        Ok(Value::Int(match arg(args, 0)? {
            Value::Int(n) => *n,
            Value::Float(n) => *n as i32,
            Value::String(s) => atoi(s),
            other => return Err(format!("cannot cast {} to int", kind(other))),
        }))
    });
    registry.register(Function, "max", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.max(float(args, 1)?)))
    });
    registry.register(Function, "min", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.min(float(args, 1)?)))
    });
    registry.register(Function, "distance", |_, _, args| {
        Ok(Value::Float(
            distance_sq(vector(args, 0)?, vector(args, 1)?).sqrt(),
        ))
    });
    registry.register(Function, "distancesquared", |_, _, args| {
        Ok(Value::Float(distance_sq(
            vector(args, 0)?,
            vector(args, 1)?,
        )))
    });
    registry.register(Function, "distance2d", |_, _, args| {
        let (a, b) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Float(
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt(),
        ))
    });
    registry.register(Function, "length", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(dot(v, v).sqrt()))
    });
    registry.register(Function, "lengthsquared", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(dot(v, v)))
    });
    registry.register(Function, "vectordot", |_, _, args| {
        Ok(Value::Float(dot(vector(args, 0)?, vector(args, 1)?)))
    });
    registry.register(Function, "vectornormalize", |_, _, args| {
        Ok(Value::Vector(normalize(vector(args, 0)?)))
    });
    registry.register(Function, "vectortoangles", |_, _, args| {
        Ok(Value::Vector(math_iw4::vect_to_angles(vector(args, 0)?)))
    });
    registry.register(Function, "vectortoyaw", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(math_iw4::vec_to_yaw(v[0], v[1])))
    });
    macro_rules! axis {
        ($($name:literal => $pick:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let axes = math_iw4::angle_vectors(vector(args, 0)?);
                let pick: fn(([f32; 3], [f32; 3], [f32; 3])) -> [f32; 3] = $pick;
                Ok(Value::Vector(pick(axes)))
            });
        )*};
    }
    axis! {
        "anglestoforward" => |a| a.0,
        "anglestoright" => |a| a.1,
        "anglestoup" => |a| a.2,
    }
    registry.register(Function, "pointonsegmentnearesttopoint", |_, _, args| {
        Ok(Value::Vector(nearest_on_segment(
            vector(args, 0)?,
            vector(args, 1)?,
            vector(args, 2)?,
        )))
    });
    registry.register(Function, "vectorfromlinetopoint", |_, _, args| {
        let (a, b, point) = (vector(args, 0)?, vector(args, 1)?, vector(args, 2)?);
        let direction = normalize(sub(b, a));
        let along = dot(sub(point, a), direction);
        Ok(Value::Vector(sub(point, add(a, scale(direction, along)))))
    });
    registry.register(Function, "averagepoint", |world, _, args| {
        let points = array_values(world, arg(args, 0)?)?;
        let mut sum = [0.0; 3];
        for point in &points {
            let Value::Vector(v) = point else {
                return Err("averagepoint expects an array of vectors".into());
            };
            sum = add(sum, *v);
        }
        Ok(Value::Vector(if points.is_empty() {
            sum
        } else {
            scale(sum, 1.0 / points.len() as f32)
        }))
    });
    registry.register(Function, "averagenormal", |world, _, args| {
        let normals = array_values(world, arg(args, 0)?)?;
        let mut sum = [0.0; 3];
        for normal in &normals {
            let Value::Vector(v) = normal else {
                return Err("averagenormal expects an array of vectors".into());
            };
            sum = add(sum, *v);
        }
        Ok(Value::Vector(normalize(sum)))
    });
    registry.register(Function, "randomint", |world, _, args| {
        let max = int(args, 0)?;
        if max <= 0 {
            return Ok(Value::Int(0));
        }
        Ok(Value::Int((random(world) % max as u32) as i32))
    });
    registry.register(Function, "randomintrange", |world, _, args| {
        let (min, max) = (int(args, 0)?, int(args, 1)?);
        if max <= min {
            return Ok(Value::Int(min));
        }
        let span = (i64::from(max) - i64::from(min)) as u32;
        Ok(Value::Int(min.wrapping_add((random(world) % span) as i32)))
    });
    registry.register(Function, "randomfloat", |world, _, args| {
        let max = float(args, 0)?;
        Ok(Value::Float(random_unit(world) * max))
    });
    registry.register(Function, "randomfloatrange", |world, _, args| {
        let (min, max) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(min + random_unit(world) * (max - min)))
    });
    registry.register(Function, "getsubstr", |_, _, args| {
        let text = string(args, 0)?;
        let chars: Vec<char> = text.chars().collect();
        let start = int(args, 1)?.max(0) as usize;
        let end = match optional(args, 2, int)? {
            Some(end) => (end.max(0) as usize).min(chars.len()),
            None => chars.len(),
        };
        Ok(Value::String(
            chars
                .get(start.min(end)..end)
                .unwrap_or(&[])
                .iter()
                .collect::<String>()
                .into(),
        ))
    });
    registry.register(Function, "issubstr", |_, _, args| {
        Ok(Value::Int(
            string(args, 0)?.contains(string(args, 1)?.as_str()).into(),
        ))
    });
    registry.register(Function, "strtok", |world, _, args| {
        let text = string(args, 0)?;
        let delimiters = string(args, 1)?;
        let tokens = text
            .split(|c| delimiters.contains(c))
            .filter(|t| !t.is_empty())
            .map(|t| Value::String(t.into()))
            .collect();
        new_array(world, tokens)
    });
    registry.register(Function, "getarraykeys", |world, _, args| {
        let Value::Array(id) = arg(args, 0)? else {
            return Err("getarraykeys expects an array".into());
        };
        let keys: Vec<_> = world
            .resource::<Runtime>()
            .arrays
            .get(id)
            .ok_or("invalid array reference")?
            .keys()
            .rev()
            .map(|key| match key {
                ArrayKey::Integer(i) => Value::Int(*i),
                ArrayKey::String(s) => Value::String(s.clone()),
            })
            .collect();
        new_array(world, keys)
    });
    registry.register(Function, "tablelookup", |world, _, args| {
        Ok(Value::String(table_lookup(world, args)?.into()))
    });
    registry.register(Function, "tablelookupistring", |world, _, args| {
        Ok(Value::LocalizedString(table_lookup(world, args)?.into()))
    });
    registry.register(Function, "tablelookupbyrow", |world, _, args| {
        let tables = world.resource::<Runtime>().tables.clone();
        let name = string(args, 0)?;
        let (row, column) = (int(args, 1)?, int(args, 2)?);
        let value = table(&tables, &name)
            .filter(|_| row >= 0 && column >= 0)
            .and_then(|t| t.cell(row as usize, column as usize))
            .unwrap_or("");
        Ok(Value::string(value))
    });
    registry.register(Function, "tablelookuprownum", |world, _, args| {
        let tables = world.resource::<Runtime>().tables.clone();
        let name = string(args, 0)?;
        let column = int(args, 1)?;
        let value = string(args, 2)?;
        let row = table(&tables, &name)
            .filter(|_| column >= 0)
            .and_then(|t| table_search(t, column as usize, &value));
        Ok(Value::Int(row.map_or(-1, |r| r as i32)))
    });
    registry.register(Function, "getsystemtime", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getbuildnumber", |_, _, _| {
        Ok(Value::string(env!("CARGO_PKG_VERSION")))
    });
    registry.register(Function, "getbuildversion", |_, _, _| {
        Ok(Value::string(env!("CARGO_PKG_VERSION")))
    });
}
