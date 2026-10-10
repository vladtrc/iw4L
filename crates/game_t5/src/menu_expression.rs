//! Black Ops menu expressions: postfix programs exactly as the zones store
//! them (`ExpressionStatement::rpn`). Operators `0..=23` are Black Ops'
//! `expressionOperatorType_e`; a comma folds call arguments into one list.
//!
//! Function indices are named only where Black Ops' own data proves them
//! (docs/fidelity/t5.md, "menu expression functions"); any other index is an
//! error, never a guess.

use menu_expr::{ExprError, GameQueries, MenuProgram, Operand};

const T5_NOOP: i32 = 0;
const T5_MUL: i32 = 2;
const T5_DIV: i32 = 3;
const T5_MOD: i32 = 4;
const T5_PLUS: i32 = 5;
const T5_MINUS: i32 = 6;
const T5_NEGATE: i32 = 7;
const T5_NOT: i32 = 8;
const T5_SMALLER: i32 = 9;
const T5_SMALLEREQ: i32 = 10;
const T5_GREATER: i32 = 11;
const T5_GREATEREQ: i32 = 12;
const T5_EQ: i32 = 13;
const T5_NOTEQ: i32 = 14;
const T5_LOGAND: i32 = 15;
const T5_LOGOR: i32 = 16;
const T5_COMMA: i32 = 18;
const T5_BITAND: i32 = 19;
const T5_BITOR: i32 = 20;
const T5_BITNEG: i32 = 21;
const T5_SHIFTLEFT: i32 = 22;
const T5_SHIFTRIGHT: i32 = 23;

const T5_FN_DVARINT: i32 = 30;
const T5_FN_DVARBOOL: i32 = 31;
const T5_FN_DVARSTRING: i32 = 33;
const T5_FN_UI_ACTIVE: i32 = 34;
const T5_FN_FLASHBANGED: i32 = 35;
const T5_FN_INKILLCAM: i32 = 38;
const T5_FN_ISDUALWIELD: i32 = 39;
const T5_FN_ISFUELWEAPON: i32 = 40;
const T5_FN_PLAYER: i32 = 41;
const T5_FN_ADSJAVELIN: i32 = 56;
const T5_FN_KEYBINDING: i32 = 81;

#[derive(Clone, Debug)]
enum T5Token {
    Const(Operand),
    Op(i32),
}

#[derive(Clone, Debug)]
enum Value {
    One(Operand),
    Args(Vec<Operand>),
}

impl Value {
    fn into_args(self) -> Vec<Operand> {
        match self {
            Self::One(v) => vec![v],
            Self::Args(v) => v,
        }
    }

    fn operand(self) -> Result<Operand, ExprError> {
        match self {
            Self::One(v) => Ok(v),
            Self::Args(_) => Err(ExprError::StrayOperands),
        }
    }
}

fn parse(tokens: &[&str]) -> Result<Vec<T5Token>, ExprError> {
    tokens
        .iter()
        .map(|token| {
            if let Some(n) = token.strip_prefix('#') {
                return n
                    .parse()
                    .map(T5Token::Op)
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(n) = token.strip_prefix("i:") {
                return n
                    .parse()
                    .map(|v| T5Token::Const(Operand::Int(v)))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(bits) = token.strip_prefix("f:") {
                return u32::from_str_radix(bits, 16)
                    .map(|b| T5Token::Const(Operand::Float(f32::from_bits(b))))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(hex) = token.strip_prefix("s:") {
                let mut bytes = Vec::with_capacity(hex.len() / 2);
                for pair in hex.as_bytes().chunks(2) {
                    let digits =
                        core::str::from_utf8(pair).map_err(|_| ExprError::TruncatedDump)?;
                    bytes.push(
                        u8::from_str_radix(digits, 16).map_err(|_| ExprError::TruncatedDump)?,
                    );
                }
                let text = bytes.iter().map(|&b| char::from(b)).collect::<String>();
                return Ok(T5Token::Const(Operand::Str(text)));
            }
            Err(ExprError::TruncatedDump)
        })
        .collect()
}

/// How Black Ops converts between operand types is not known: only operations
/// whose operands need no conversion are evaluated.
const COERCION: &str = game_api::unknown!(
    "t5.hud.expression_coercion",
    "how Black Ops converts between menu expression operand types",
    "Black Ops' operand conversion rules"
)
.id;

fn int(operand: Operand) -> Result<i32, ExprError> {
    match operand {
        Operand::Int(v) => Ok(v),
        _ => Err(ExprError::Unknown(COERCION)),
    }
}

/// A program's value as a condition.
fn truth(value: &Operand) -> Result<bool, ExprError> {
    match value {
        Operand::Int(v) => Ok(*v != 0),
        _ => Err(ExprError::Unknown(COERCION)),
    }
}

/// A program's value where a number is wanted.
fn number(value: &Operand) -> Result<f32, ExprError> {
    match value {
        Operand::Int(v) => Ok(*v as f32),
        Operand::Float(v) => Ok(*v),
        Operand::Str(_) => Err(ExprError::Unknown(COERCION)),
    }
}

/// A program's value where text is wanted.
fn text(value: &Operand) -> Result<String, ExprError> {
    match value {
        Operand::Str(v) => Ok(v.clone()),
        Operand::Int(v) => Ok(format!("{v}")),
        Operand::Float(_) => Err(ExprError::Unknown(COERCION)),
    }
}

fn binary(op: i32, a: Operand, b: Operand) -> Result<Operand, ExprError> {
    let flag = |v: bool| Operand::Int(i32::from(v));
    Ok(match (a, b) {
        (Operand::Int(a), Operand::Int(b)) => match op {
            T5_MUL => Operand::Int(a.wrapping_mul(b)),
            T5_MOD if b != 0 => Operand::Int(a.wrapping_rem(b)),
            T5_PLUS => Operand::Int(a.wrapping_add(b)),
            T5_MINUS => Operand::Int(a.wrapping_sub(b)),
            T5_SMALLER => flag(a < b),
            T5_SMALLEREQ => flag(a <= b),
            T5_GREATER => flag(a > b),
            T5_GREATEREQ => flag(a >= b),
            T5_EQ => flag(a == b),
            T5_NOTEQ => flag(a != b),
            T5_LOGAND => flag(a != 0 && b != 0),
            T5_LOGOR => flag(a != 0 || b != 0),
            _ => return Err(ExprError::Unknown(COERCION)),
        },
        (Operand::Float(a), Operand::Float(b)) => match op {
            T5_MUL => Operand::Float(a * b),
            T5_DIV if b != 0.0 => Operand::Float(a / b),
            T5_PLUS => Operand::Float(a + b),
            T5_MINUS => Operand::Float(a - b),
            T5_SMALLER => flag(a < b),
            T5_SMALLEREQ => flag(a <= b),
            T5_GREATER => flag(a > b),
            T5_GREATEREQ => flag(a >= b),
            _ => return Err(ExprError::Unknown(COERCION)),
        },
        (Operand::Str(a), Operand::Str(b)) => match op {
            T5_PLUS => Operand::Str(a + &b),
            T5_EQ => flag(a.eq_ignore_ascii_case(&b)),
            T5_NOTEQ => flag(!a.eq_ignore_ascii_case(&b)),
            _ => return Err(ExprError::Unknown(COERCION)),
        },
        _ => return Err(ExprError::Unknown(COERCION)),
    })
}

fn pop(stack: &mut Vec<Value>) -> Result<Value, ExprError> {
    stack.pop().ok_or(ExprError::StackUnderflow)
}

fn pop_operand(stack: &mut Vec<Value>) -> Result<Operand, ExprError> {
    pop(stack)?.operand()
}

fn arg_str(args: &[Operand]) -> Result<String, ExprError> {
    match args.first() {
        Some(Operand::Str(v)) => Ok(v.clone()),
        Some(_) => Err(ExprError::Unknown(COERCION)),
        None => Err(ExprError::StackUnderflow),
    }
}

fn call(op: i32, stack: &mut Vec<Value>, host: &dyn GameQueries) -> Result<Operand, ExprError> {
    let mut args = || pop(stack).map(Value::into_args);
    Ok(match op {
        T5_FN_DVARINT => Operand::Int(host.dvar_int(&arg_str(&args()?)?)?),
        T5_FN_DVARBOOL => Operand::Int(host.dvar_bool(&arg_str(&args()?)?)?),
        T5_FN_DVARSTRING => Operand::Str(host.dvar_string(&arg_str(&args()?)?)?),
        T5_FN_UI_ACTIVE => Operand::Int(host.ui_active()?),
        T5_FN_FLASHBANGED => Operand::Int(host.flashbanged()?),
        T5_FN_INKILLCAM => Operand::Int(host.in_killcam()?),
        T5_FN_ISDUALWIELD => Operand::Int(host.is_dual_wield()?),
        T5_FN_ISFUELWEAPON => Operand::Int(host.is_fuel_weapon()?),
        T5_FN_PLAYER => host.player_field(&arg_str(&args()?)?)?,
        T5_FN_ADSJAVELIN => Operand::Int(i32::from(host.ads_javelin()?)),
        T5_FN_KEYBINDING => host.key_binding(&arg_str(&args()?)?)?,
        other => return Err(ExprError::UnsupportedOp(other)),
    })
}

fn evaluate(tokens: &[T5Token], host: &dyn GameQueries) -> Result<Operand, ExprError> {
    let mut stack: Vec<Value> = Vec::new();
    for token in tokens {
        let op = match token {
            T5Token::Const(v) => {
                stack.push(Value::One(v.clone()));
                continue;
            }
            T5Token::Op(op) => *op,
        };
        let value = match op {
            T5_NOOP => continue,
            T5_COMMA => {
                let b = pop(&mut stack)?;
                let mut a = pop(&mut stack)?.into_args();
                a.extend(b.into_args());
                stack.push(Value::Args(a));
                continue;
            }
            T5_NEGATE => match pop_operand(&mut stack)? {
                Operand::Int(v) => Operand::Int(v.wrapping_neg()),
                Operand::Float(v) => Operand::Float(-v),
                Operand::Str(_) => return Err(ExprError::Host("negate a string")),
            },
            T5_NOT => Operand::Int(i32::from(int(pop_operand(&mut stack)?)? == 0)),
            T5_BITNEG => Operand::Int(!int(pop_operand(&mut stack)?)?),
            T5_MUL | T5_DIV | T5_MOD | T5_PLUS | T5_MINUS | T5_SMALLER..=T5_LOGOR => {
                let b = pop_operand(&mut stack)?;
                let a = pop_operand(&mut stack)?;
                binary(op, a, b)?
            }
            T5_BITAND | T5_BITOR | T5_SHIFTLEFT | T5_SHIFTRIGHT => {
                let b = int(pop_operand(&mut stack)?)?;
                let a = int(pop_operand(&mut stack)?)?;
                Operand::Int(match op {
                    T5_BITAND => a & b,
                    T5_BITOR => a | b,
                    T5_SHIFTLEFT => a.wrapping_shl(b as u32),
                    _ => a.wrapping_shr(b as u32),
                })
            }
            op if op >= 24 => call(op, &mut stack, host)?,
            other => return Err(ExprError::UnsupportedOp(other)),
        };
        stack.push(Value::One(value));
    }
    match stack.len() {
        0 => Err(ExprError::EmptyResult),
        1 => pop_operand(&mut stack),
        _ => Err(ExprError::StrayOperands),
    }
}

#[derive(Debug)]
struct Program(Vec<T5Token>);

impl MenuProgram for Program {
    fn evaluate(&self, queries: &dyn GameQueries) -> Result<Operand, ExprError> {
        evaluate(&self.0, queries)
    }
    fn truth(&self, value: &Operand) -> Result<bool, ExprError> {
        truth(value)
    }
    fn number(&self, value: &Operand) -> Result<f32, ExprError> {
        number(value)
    }
    fn text(&self, value: &Operand) -> Result<String, ExprError> {
        text(value)
    }
}

/// A Black Ops menu expression from its catalog (the tokens after `t5`).
pub fn parse_menu_expression(tokens: &[&str]) -> Result<Box<dyn MenuProgram>, ExprError> {
    Ok(Box::new(Program(parse(tokens)?)))
}
