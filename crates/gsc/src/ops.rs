//! The VM's operators on script values, shared by the compiler's constant
//! folding and the runtime.

use crate::{Binary, Unary, Value};

pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::LocalizedString(_) => "localized string",
        Value::Vector(_) => "vector",
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::Function(_) => "function",
        Value::Builtin(_) => "builtin function",
        Value::Animation { .. } => "animation",
        Value::AnimationTree(_) => "animtree",
    }
}

pub fn mismatch(a: &Value, b: &Value) -> String {
    format!(
        "pair '{}' and '{}' has unmatching types",
        type_name(a),
        type_name(b)
    )
}

pub fn truth(value: &Value) -> Result<bool, String> {
    match value {
        Value::Int(n) => Ok(*n != 0),
        Value::Float(n) => Ok(*n != 0.0),
        _ => Err(format!("cast from {} to bool", type_name(value))),
    }
}

pub fn scalar(value: &Value) -> Option<f32> {
    match value {
        Value::Int(n) => Some(*n as f32),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

pub fn weaker(a: Value, b: Value) -> (Value, Value) {
    match (&a, &b) {
        (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
        (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
        (Value::Vector(_), _) if scalar(&b).is_some() => {
            let y = scalar(&b).unwrap();
            (a, Value::Vector([y; 3]))
        }
        (_, Value::Vector(_)) if scalar(&a).is_some() => {
            let x = scalar(&a).unwrap();
            (Value::Vector([x; 3]), b)
        }
        _ => (a, b),
    }
}

pub fn format_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let trim = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if !(-4..6).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", trim(mantissa), exponent.abs())
    } else {
        trim(&format!("{value:.*}", (5 - exponent) as usize))
    }
}

pub fn to_text(value: &Value) -> Option<String> {
    match value {
        Value::Int(n) => Some(n.to_string()),
        // format_g needs the exponent that NaN and infinity print without.
        Value::Float(n) if n.is_finite() => Some(format_g(f64::from(*n))),
        Value::Vector(v) if v.iter().all(|n| n.is_finite()) => Some(format!(
            "({}, {}, {})",
            format_g(f64::from(v[0])),
            format_g(f64::from(v[1])),
            format_g(f64::from(v[2]))
        )),
        _ => None,
    }
}

pub fn equality(a: Value, b: Value) -> Result<bool, String> {
    let (a, b) = weaker(a, b);
    Ok(match (&a, &b) {
        (Value::Array(_), _) | (_, Value::Array(_)) => {
            return Err("cannot compare arrays".into());
        }
        (Value::Undefined, Value::Undefined) => true,
        (Value::Object(_), Value::Object(_)) => a == b,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::LocalizedString(x), Value::LocalizedString(y)) => x == y,
        (Value::Vector(x), Value::Vector(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < 1e-6,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::Animation { .. }, Value::Animation { .. })
        | (Value::AnimationTree(_), Value::AnimationTree(_)) => a == b,
        _ => return Err(mismatch(&a, &b)),
    })
}

pub fn unary(op: Unary, value: Value) -> Result<Value, String> {
    match (op, &value) {
        (Unary::Not, _) => Ok(Value::Int(i32::from(!truth(&value)?))),
        (Unary::Complement, Value::Int(n)) => Ok(Value::Int(!n)),
        (Unary::Complement, _) => Err(format!("~ cannot be applied to {}", type_name(&value))),
    }
}

pub fn binary(op: Binary, a: Value, b: Value) -> Result<Value, String> {
    let integers = |a: &Value, b: &Value| match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok((*x, *y)),
        _ => Err(format!(
            "{op:?} requires int operands, found {} and {}",
            type_name(a),
            type_name(b)
        )),
    };
    let result = match op {
        Binary::Or => integers(&a, &b).map(|(x, y)| Value::Int(x | y))?,
        Binary::Xor => integers(&a, &b).map(|(x, y)| Value::Int(x ^ y))?,
        Binary::And => integers(&a, &b).map(|(x, y)| Value::Int(x & y))?,
        Binary::Shl => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shl((y & 31) as u32)))?
        }
        Binary::Shr => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shr((y & 31) as u32)))?
        }
        Binary::Mod => {
            let (x, y) = integers(&a, &b)?;
            Value::Int(x.checked_rem(y).unwrap_or(0))
        }
        Binary::Equal => Value::Int(i32::from(equality(a, b)?)),
        Binary::NotEqual => Value::Int(i32::from(!equality(a, b)?)),
        Binary::Case => match (&a, &b) {
            (Value::Int(_) | Value::String(_), _) => Value::Int(i32::from(a == b)),
            _ => return Err(format!("cannot switch on {}", type_name(&a))),
        },
        Binary::Add => {
            let (a, b) = match (&a, &b) {
                (Value::String(_), _) | (_, Value::String(_)) => {
                    let bytes = |value: &Value| match value {
                        Value::String(text) => Some(text.as_bytes().to_vec()),
                        other => to_text(other).map(String::into_bytes),
                    };
                    match (bytes(&a), bytes(&b)) {
                        (Some(mut x), Some(y)) => {
                            if x.len() + y.len() > 0x2000 {
                                return Err("string too long".into());
                            }
                            x.extend_from_slice(&y);
                            return Ok(Value::byte_string(&x));
                        }
                        _ => return Err(mismatch(&a, &b)),
                    }
                }
                (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
                (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
                _ => (a, b),
            };
            match (&a, &b) {
                (Value::Int(x), Value::Int(y)) => Value::Int(x.wrapping_add(*y)),
                (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                (Value::Vector(x), Value::Vector(y)) => {
                    Value::Vector([x[0] + y[0], x[1] + y[1], x[2] + y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Sub | Binary::Mul | Binary::Div => {
            let (a, b) = weaker(a, b);
            match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Sub) => Value::Int(x.wrapping_sub(*y)),
                (Value::Int(x), Value::Int(y), Binary::Mul) => Value::Int(x.wrapping_mul(*y)),
                // The stock scripts divide by a zero they never guard (a caps-per-minute
                // rate read before the first second is counted) and read the zero back.
                (Value::Int(_), Value::Int(0), _) | (_, Value::Float(0.0), Binary::Div) => {
                    Value::Float(0.0)
                }
                (Value::Int(x), Value::Int(y), _) => Value::Float(*x as f32 / *y as f32),
                (Value::Float(x), Value::Float(y), Binary::Sub) => Value::Float(x - y),
                (Value::Float(x), Value::Float(y), Binary::Mul) => Value::Float(x * y),
                (Value::Float(x), Value::Float(y), _) => Value::Float(x / y),
                (Value::Vector(x), Value::Vector(y), Binary::Sub) => {
                    Value::Vector([x[0] - y[0], x[1] - y[1], x[2] - y[2]])
                }
                (Value::Vector(x), Value::Vector(y), Binary::Mul) => {
                    Value::Vector([x[0] * y[0], x[1] * y[1], x[2] * y[2]])
                }
                (Value::Vector(_), Value::Vector(y), _) if y.contains(&0.0) => {
                    Value::Vector([0.0; 3])
                }
                (Value::Vector(x), Value::Vector(y), _) => {
                    Value::Vector([x[0] / y[0], x[1] / y[1], x[2] / y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Less | Binary::Greater | Binary::LessEqual | Binary::GreaterEqual => {
            let (a, b) = weaker(a, b);
            let ordered = match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Int(x), Value::Int(y), _) => x > y,
                (Value::Float(x), Value::Float(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Float(x), Value::Float(y), _) => x > y,
                _ => return Err(mismatch(&a, &b)),
            };
            let inverted = matches!(op, Binary::GreaterEqual | Binary::LessEqual);
            Value::Int(i32::from(ordered != inverted))
        }
    };
    match &result {
        Value::Float(n) if !n.is_finite() => Err("non-finite arithmetic result".into()),
        Value::Vector(v) if !v.iter().all(|n| n.is_finite()) => {
            Err("non-finite arithmetic result".into())
        }
        _ => Ok(result),
    }
}
