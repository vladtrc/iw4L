extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub const OP_NOOP: i32 = 0x00;
pub const OP_RIGHTPAREN: i32 = 0x01;
pub const OP_MULTIPLY: i32 = 0x02;
pub const OP_DIVIDE: i32 = 0x03;
pub const OP_MODULUS: i32 = 0x04;
pub const OP_ADD: i32 = 0x05;
pub const OP_SUBTRACT: i32 = 0x06;
pub const OP_NOT: i32 = 0x07;
pub const OP_LESSTHAN: i32 = 0x08;
pub const OP_LESSTHANEQUALTO: i32 = 0x09;
pub const OP_GREATERTHAN: i32 = 0x0A;
pub const OP_GREATERTHANEQUALTO: i32 = 0x0B;
pub const OP_EQUALS: i32 = 0x0C;
pub const OP_NOTEQUAL: i32 = 0x0D;
pub const OP_AND: i32 = 0x0E;
pub const OP_OR: i32 = 0x0F;
pub const OP_LEFTPAREN: i32 = 0x10;
pub const OP_COMMA: i32 = 0x11;
pub const OP_FIRSTFUNCTIONCALL: i32 = 0x17;
pub const OP_STATICDVARINT: i32 = 0x17;

pub const OP_STATICDVARBOOL: i32 = 0x18;

pub const OP_STATICDVARSTRING: i32 = 0x1A;
pub const OP_INT: i32 = 0x1B;

pub const OP_MIN: i32 = 0x20;

pub const OP_MAX: i32 = 0x21;

pub const OP_KEYBINDING: i32 = 0x5d;

pub const OP_ACTIONSLOTUSABLE: i32 = 0x5e;
pub const OP_MILLISECONDS: i32 = 0x22;

pub const OP_DVARINT: i32 = 0x23;

pub const OP_DVARBOOL: i32 = 0x24;

pub const OP_DVARFLOAT: i32 = 0x25;

pub const OP_UIACTIVE: i32 = 0x28;

pub const OP_FLASHBANGED: i32 = 0x29;

pub const OP_MISSILECAM: i32 = 0x2B;

pub const OP_SCOPED: i32 = 0x2C;

pub const OP_SCOREBOARD_VISIBLE: i32 = 0x2E;

pub const OP_INKILLCAM: i32 = 0x2F;

pub const OP_MENUISOPEN: i32 = 0x39;
pub const OP_PLAYERFIELD: i32 = 0x31;
pub const OP_GETPERK: i32 = 0x32;

pub const OP_SELECTINGLOCATION: i32 = 0x33;
pub const OP_TEAMFIELD: i32 = 0x35;
pub const OP_OTHERTEAMFIELD: i32 = 0x36;

pub const OP_ADSJAVELIN: i32 = 0x40;

pub const OP_TABLELOOKUP: i32 = 0x4A;

pub const OP_TABLELOOKUPBYROW: i32 = 0x4B;

pub const OP_LOCALIZESTRING: i32 = 0x4D;

pub const OP_LOCALVARINT: i32 = 0x4E;
pub const OP_LOCALVARSTRING: i32 = 0x51;
pub const OP_TIMELEFT: i32 = 0x52;
pub const OP_SECONDSASCOUNTDOWN: i32 = 0x53;
pub const OP_GAMETYPENAME: i32 = 0x55;
pub const OP_SCORE: i32 = 0x58;

pub const OP_SPECTATINGCLIENT: i32 = 0x5A;

pub const OP_SPECTATINGFREE: i32 = 0x5B;

pub const OP_IS_ITEM_UNLOCKED: i32 = 0x70;

pub const OP_WEAPONNAME: i32 = 0x75;

pub const OP_EMPJAMMED: i32 = 0x8A;

pub const OP_GETSPLASHTEXT: i32 = 0x8F;

pub const OP_GETSPLASHDESCRIPTION: i32 = 0x90;

pub const OP_GETSPLASHMATERIAL: i32 = 0x91;

pub const OP_SPLASHHASICON: i32 = 0x92;

pub const OP_SPLASHROWNUM: i32 = 0x93;

pub const OP_GETPLAYERCARDINFO: i32 = 0xA5;

const PRECEDENCE: [i32; 81] = [
    2147483647, 0, 11, 11, 11, 13, 13, 9, 15, 15, 15, 15, 16, 16, 25, 25, 99, 80, 17, 18, 9, 14,
    14, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
];

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Int(i32),
    Float(f32),
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprError {
    TruncatedDump,
    FunctionBodyMissing,
    UnsupportedOp(i32),
    StackUnderflow,
    StrayOperands,
    EmptyResult,
    Host(&'static str),
}

pub trait ExprHost {
    fn localize_string(&self, _args: &[Operand]) -> Result<String, ExprError> {
        Err(ExprError::UnsupportedOp(OP_LOCALIZESTRING))
    }
    fn milliseconds(&self) -> i32;
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError>;

    fn static_dvar_string(&self, index: i32) -> Result<String, ExprError> {
        let _ = index;
        Err(ExprError::UnsupportedOp(OP_STATICDVARSTRING))
    }
    fn team_field(&self, field: &str) -> Result<Operand, ExprError>;
    fn player_field(&self, field: &str) -> Result<Operand, ExprError>;
    fn other_team_field(&self, field: &str) -> Result<Operand, ExprError>;
    fn local_var_string(&self, name: &str) -> Result<Operand, ExprError>;

    fn local_var_int(&self, name: &str) -> Result<i32, ExprError> {
        Ok(source_int(&self.local_var_string(name)?))
    }
    fn time_left(&self) -> Result<i32, ExprError>;
    fn score_at_rank(&self, rank: i32) -> Result<i32, ExprError>;
    fn gametype_name(&self) -> Result<Operand, ExprError>;

    fn splash_text(&self, slot: i32) -> Result<Operand, ExprError> {
        let _ = slot;
        Err(ExprError::UnsupportedOp(OP_GETSPLASHTEXT))
    }
    fn splash_description(&self, slot: i32) -> Result<Operand, ExprError> {
        let _ = slot;
        Err(ExprError::UnsupportedOp(OP_GETSPLASHDESCRIPTION))
    }
    fn splash_material(&self, slot: i32) -> Result<Operand, ExprError> {
        let _ = slot;
        Err(ExprError::UnsupportedOp(OP_GETSPLASHMATERIAL))
    }
    fn splash_has_icon(&self, slot: i32) -> Result<Operand, ExprError> {
        let _ = slot;
        Err(ExprError::UnsupportedOp(OP_SPLASHHASICON))
    }
    fn splash_row_num(&self, slot: i32) -> Result<Operand, ExprError> {
        let _ = slot;
        Err(ExprError::UnsupportedOp(OP_SPLASHROWNUM))
    }

    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        let _ = name;
        Err(ExprError::UnsupportedOp(OP_DVARINT))
    }

    fn dvar_bool(&self, name: &str) -> Result<i32, ExprError> {
        self.dvar_int(name)
    }

    fn dvar_float(&self, name: &str) -> Result<f32, ExprError> {
        let _ = name;
        Err(ExprError::UnsupportedOp(OP_DVARFLOAT))
    }

    fn table_lookup(
        &self,
        table: &str,
        col0: i32,
        key: &str,
        result_col: i32,
    ) -> Result<Operand, ExprError> {
        let _ = (table, col0, key, result_col);
        Err(ExprError::UnsupportedOp(OP_TABLELOOKUP))
    }

    fn table_lookup_by_row(&self, table: &str, row: i32, col: i32) -> Result<Operand, ExprError> {
        let _ = (table, row, col);
        Err(ExprError::UnsupportedOp(OP_TABLELOOKUPBYROW))
    }

    fn player_card_info(&self, field: i32, lookup: i32, slot: i32) -> Result<Operand, ExprError> {
        let _ = (field, lookup, slot);
        Err(ExprError::UnsupportedOp(OP_GETPLAYERCARDINFO))
    }

    fn get_perk(&self, name: &str) -> Result<Operand, ExprError> {
        let _ = name;
        Err(ExprError::UnsupportedOp(OP_GETPERK))
    }

    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn key_binding(&self, _command: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("keybinding"))
    }
    fn action_slot_usable(&self, _slot: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("actionslotusable"))
    }
    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn scoreboard_visible(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn menu_is_open(&self, name: &str) -> Result<i32, ExprError> {
        let _ = name;
        Ok(0)
    }

    fn flashbanged(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("flashbanged"))
    }

    fn missilecam(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn scoped(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn selecting_location(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn adsjavelin(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn spectating_client(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn spectating_free(&self) -> Result<i32, ExprError> {
        Ok(0)
    }

    fn is_item_unlocked(&self, item: &str) -> Result<i32, ExprError> {
        let _ = item;
        Err(ExprError::Host("isitemunlocked"))
    }

    fn weapon_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("weaponname"))
    }

    fn emp_jammed(&self) -> Result<i32, ExprError> {
        Ok(0)
    }
}

#[derive(Clone, Debug)]
enum Entry {
    Op(i32),
    Operand(Operand),
    Nested(Vec<Entry>),
}

fn precedence(op: i32) -> i32 {
    if (0..PRECEDENCE.len() as i32).contains(&op) {
        PRECEDENCE[op as usize]
    } else {
        5
    }
}

fn is_associative(op: i32) -> bool {
    op < OP_DIVIDE || (op > OP_MODULUS && op != OP_SUBTRACT)
}

fn pairs_with_right_paren(op: i32) -> bool {
    op >= OP_FIRSTFUNCTIONCALL || op == OP_LEFTPAREN
}

#[derive(Clone, Debug)]
pub struct Statement {
    entries: Vec<Entry>,

    empty: bool,
}

impl Statement {
    pub fn parse(dump: &str) -> Result<Self, ExprError> {
        let empty = dump.split_whitespace().next().is_none();
        Ok(Self {
            entries: parse_dump(dump)?,
            empty,
        })
    }

    pub fn is_true(&self, host: &impl ExprHost) -> Result<bool, ExprError> {
        if self.empty {
            return Ok(true);
        }
        Ok(source_int(&self.evaluate(host)?) != 0)
    }

    pub fn evaluate(&self, host: &impl ExprHost) -> Result<Operand, ExprError> {
        eval_entries(&self.entries, host)
    }

    pub fn evaluate_float(&self, host: &impl ExprHost) -> Result<f32, ExprError> {
        Ok(source_float(&self.evaluate(host)?))
    }

    pub fn evaluate_string(&self, host: &impl ExprHost) -> Result<String, ExprError> {
        Ok(source_str(&self.evaluate(host)?))
    }
}

pub fn is_expression_true(dump: &str, host: &impl ExprHost) -> Result<bool, ExprError> {
    Statement::parse(dump)?.is_true(host)
}

pub fn evaluate(dump: &str, host: &impl ExprHost) -> Result<Operand, ExprError> {
    Statement::parse(dump)?.evaluate(host)
}

pub fn evaluate_float(dump: &str, host: &impl ExprHost) -> Result<f32, ExprError> {
    Ok(source_float(&evaluate(dump, host)?))
}

pub fn evaluate_string(dump: &str, host: &impl ExprHost) -> Result<String, ExprError> {
    Ok(source_str(&evaluate(dump, host)?))
}

fn parse_dump(dump: &str) -> Result<Vec<Entry>, ExprError> {
    let toks: Vec<&str> = dump.split_whitespace().collect();
    parse_seq(&toks, 0).map(|(e, _)| e)
}

fn parse_seq(toks: &[&str], mut i: usize) -> Result<(Vec<Entry>, usize), ExprError> {
    let mut out = Vec::new();
    while i < toks.len() {
        match toks[i] {
            "}" => return Ok((out, i)),
            "{" => {
                i += 1;
                let (inner, j) = parse_seq(toks, i)?;
                if j >= toks.len() || toks[j] != "}" {
                    return Err(ExprError::TruncatedDump);
                }
                out.push(Entry::Nested(inner));
                i = j + 1;
            }
            t if t.starts_with("s:") => {
                let hex = &t[2..];
                if hex.len() % 2 != 0 {
                    return Err(ExprError::TruncatedDump);
                }
                let mut bytes = Vec::new();
                for pair in hex.as_bytes().chunks_exact(2) {
                    let digits =
                        core::str::from_utf8(pair).map_err(|_| ExprError::TruncatedDump)?;
                    bytes.push(
                        u8::from_str_radix(digits, 16).map_err(|_| ExprError::TruncatedDump)?,
                    );
                }
                let string = String::from_utf8(bytes).map_err(|_| ExprError::TruncatedDump)?;
                out.push(Entry::Operand(Operand::Str(string)));
                i += 1;
            }
            "fnmiss" => return Err(ExprError::FunctionBodyMissing),
            "op" => {
                i += 1;
                let n = toks.get(i).ok_or(ExprError::TruncatedDump)?;
                let op = n.parse().map_err(|_| ExprError::TruncatedDump)?;
                out.push(Entry::Op(op));
                i += 1;
            }
            "d" => {
                i += 3;
            }
            t if t.starts_with("f:") => {
                let bits =
                    u32::from_str_radix(&t[2..], 16).map_err(|_| ExprError::TruncatedDump)?;
                out.push(Entry::Operand(Operand::Float(f32::from_bits(bits))));
                i += 1;
            }
            t => {
                if let Ok(n) = t.parse::<i32>() {
                    out.push(Entry::Operand(Operand::Int(n)));
                } else {
                    out.push(Entry::Operand(Operand::Str(String::from(t))));
                }
                i += 1;
            }
        }
    }
    Ok((out, i))
}

fn eval_entries(entries: &[Entry], host: &impl ExprHost) -> Result<Operand, ExprError> {
    let mut op_stack: Vec<(i32, usize)> = Vec::new();
    let mut data: Vec<Operand> = Vec::new();
    for entry in entries {
        match entry {
            Entry::Operand(v) => {
                if data.len() >= 60 {
                    return Err(ExprError::StrayOperands);
                }
                data.push(v.clone());
            }
            Entry::Nested(inner) => {
                if inner.is_empty() {
                    return Err(ExprError::EmptyResult);
                }
                data.push(eval_entries(inner, host)?);
            }
            Entry::Op(op) => {
                if *op != OP_LEFTPAREN {
                    run_higher(*op, host, &mut op_stack, &mut data)?;
                }
                if op_stack.len() >= 60 {
                    return Err(ExprError::StrayOperands);
                }
                op_stack.push((*op, data.len()));
            }
        }
    }
    while !op_stack.is_empty() {
        run_op(host, &mut op_stack, &mut data)?;
    }
    if data.len() != 1 {
        return if data.is_empty() {
            Err(ExprError::EmptyResult)
        } else {
            Err(ExprError::StrayOperands)
        };
    }
    Ok(data.pop().unwrap_or(Operand::Int(0)))
}

fn run_higher(
    incoming: i32,
    host: &impl ExprHost,
    op_stack: &mut Vec<(i32, usize)>,
    data: &mut Vec<Operand>,
) -> Result<(), ExprError> {
    while !op_stack.is_empty() {
        let top = op_stack.last().map(|entry| entry.0).unwrap_or(OP_NOOP);
        let keep = (precedence(top) >= precedence(incoming)
            || (precedence(top) == 5 && incoming != OP_RIGHTPAREN))
            && (is_associative(incoming) || top != incoming);
        if keep {
            break;
        }
        run_op(host, op_stack, data)?;
    }
    Ok(())
}

fn pop_op(op_stack: &mut Vec<(i32, usize)>) -> Result<(i32, usize), ExprError> {
    op_stack.pop().ok_or(ExprError::StackUnderflow)
}

fn pop_data(data: &mut Vec<Operand>) -> Result<Operand, ExprError> {
    data.pop().ok_or(ExprError::StackUnderflow)
}

fn pop_splash_slot(data: &mut Vec<Operand>) -> i32 {
    match data.pop() {
        Some(v) => source_int(&v),
        None => 0,
    }
}

fn run_op(
    host: &impl ExprHost,
    op_stack: &mut Vec<(i32, usize)>,
    data: &mut Vec<Operand>,
) -> Result<(), ExprError> {
    let (op, operand_base) = pop_op(op_stack)?;
    match op {
        OP_LOCALIZESTRING => {
            if operand_base > data.len() {
                return Err(ExprError::StackUnderflow);
            }
            let args = data.split_off(operand_base);
            if args.len() > 10 {
                return Err(ExprError::StrayOperands);
            }
            data.push(Operand::Str(host.localize_string(&args)?));
            Ok(())
        }
        OP_NOOP | OP_COMMA => Ok(()),
        OP_LEFTPAREN => Ok(()),
        OP_RIGHTPAREN => {
            loop {
                if op_stack.is_empty() {
                    break;
                }
                let top = op_stack.last().map(|entry| entry.0).unwrap_or(OP_NOOP);
                run_op(host, op_stack, data)?;
                if pairs_with_right_paren(top) {
                    break;
                }
            }
            Ok(())
        }
        OP_NOT => {
            let v = pop_data(data)?;
            data.push(Operand::Int(i32::from(source_int(&v) == 0)));
            Ok(())
        }
        OP_MULTIPLY
        | OP_DIVIDE
        | OP_MODULUS
        | OP_ADD
        | OP_SUBTRACT
        | OP_LESSTHAN
        | OP_LESSTHANEQUALTO
        | OP_GREATERTHAN
        | OP_GREATERTHANEQUALTO
        | OP_EQUALS
        | OP_NOTEQUAL
        | OP_AND
        | OP_OR => {
            let b = pop_data(data)?;
            let a = pop_data(data)?;
            data.push(logic_op(op, a, b)?);
            Ok(())
        }
        OP_MILLISECONDS => {
            data.push(Operand::Int(host.milliseconds()));
            Ok(())
        }
        OP_KEYBINDING => {
            let command = source_str(&data.pop().ok_or(ExprError::StackUnderflow)?);
            data.push(host.key_binding(&command)?);
            Ok(())
        }
        OP_ACTIONSLOTUSABLE => {
            let slot = source_int(&data.pop().ok_or(ExprError::StackUnderflow)?);
            data.push(Operand::Int(host.action_slot_usable(slot)?));
            Ok(())
        }
        OP_UIACTIVE => {
            data.push(Operand::Int(host.ui_active()?));
            Ok(())
        }
        OP_FLASHBANGED => {
            data.push(Operand::Int(host.flashbanged()?));
            Ok(())
        }
        OP_MISSILECAM => {
            data.push(Operand::Int(host.missilecam()?));
            Ok(())
        }
        OP_SCOPED => {
            data.push(Operand::Int(host.scoped()?));
            Ok(())
        }
        OP_SCOREBOARD_VISIBLE => {
            data.push(Operand::Int(host.scoreboard_visible()?));
            Ok(())
        }
        OP_INKILLCAM => {
            data.push(Operand::Int(host.in_killcam()?));
            Ok(())
        }
        OP_SELECTINGLOCATION => {
            data.push(Operand::Int(host.selecting_location()?));
            Ok(())
        }
        OP_ADSJAVELIN => {
            data.push(Operand::Int(host.adsjavelin()?));
            Ok(())
        }
        OP_SPECTATINGCLIENT => {
            data.push(Operand::Int(host.spectating_client()?));
            Ok(())
        }
        OP_SPECTATINGFREE => {
            data.push(Operand::Int(host.spectating_free()?));
            Ok(())
        }
        OP_IS_ITEM_UNLOCKED => {
            let item = source_str(&pop_data(data)?);
            data.push(Operand::Int(host.is_item_unlocked(&item)?));
            Ok(())
        }
        OP_WEAPONNAME => {
            data.push(host.weapon_name()?);
            Ok(())
        }
        OP_EMPJAMMED => {
            data.push(Operand::Int(host.emp_jammed()?));
            Ok(())
        }
        OP_MENUISOPEN => {
            let name = source_str(&pop_data(data)?);
            data.push(Operand::Int(host.menu_is_open(&name)?));
            Ok(())
        }
        OP_DVARINT | OP_DVARBOOL => {
            let name = source_str(&pop_data(data)?);
            data.push(Operand::Int(if op == OP_DVARBOOL {
                host.dvar_bool(&name)?
            } else {
                host.dvar_int(&name)?
            }));
            Ok(())
        }
        OP_DVARFLOAT => {
            let name = source_str(&pop_data(data)?);
            data.push(Operand::Float(host.dvar_float(&name)?));
            Ok(())
        }
        OP_STATICDVARINT | OP_STATICDVARBOOL => {
            let idx = source_int(&pop_data(data)?);
            data.push(Operand::Int(host.static_dvar_int(idx)?));
            Ok(())
        }
        OP_STATICDVARSTRING => {
            let idx = source_int(&pop_data(data)?);
            data.push(Operand::Str(host.static_dvar_string(idx)?));
            Ok(())
        }
        OP_INT => {
            let v = pop_data(data)?;
            data.push(Operand::Int(source_int(&v)));
            Ok(())
        }
        OP_TEAMFIELD => {
            let name = source_str(&pop_data(data)?);
            data.push(host.team_field(&name)?);
            Ok(())
        }
        OP_PLAYERFIELD => {
            let name = source_str(&pop_data(data)?);
            data.push(host.player_field(&name)?);
            Ok(())
        }
        OP_OTHERTEAMFIELD => {
            let name = source_str(&pop_data(data)?);
            data.push(host.other_team_field(&name)?);
            Ok(())
        }
        OP_LOCALVARSTRING => {
            let name = source_str(&pop_data(data)?);
            data.push(host.local_var_string(&name)?);
            Ok(())
        }
        OP_LOCALVARINT => {
            let name = source_str(&pop_data(data)?);
            data.push(Operand::Int(host.local_var_int(&name)?));
            Ok(())
        }
        OP_TIMELEFT => {
            data.push(Operand::Int(host.time_left()?));
            Ok(())
        }
        OP_SECONDSASCOUNTDOWN => {
            let seconds = source_int(&pop_data(data)?);
            data.push(Operand::Str(crate::seconds_to_countdown_display(seconds)));
            Ok(())
        }
        OP_GAMETYPENAME => {
            data.push(host.gametype_name()?);
            Ok(())
        }
        OP_SCORE => {
            let rank = source_int(&pop_data(data)?);
            data.push(Operand::Int(host.score_at_rank(rank)?));
            Ok(())
        }
        OP_GETSPLASHTEXT => {
            let slot = pop_splash_slot(data);
            data.push(host.splash_text(slot)?);
            Ok(())
        }
        OP_GETSPLASHDESCRIPTION => {
            let slot = pop_splash_slot(data);
            data.push(host.splash_description(slot)?);
            Ok(())
        }
        OP_GETSPLASHMATERIAL => {
            let slot = pop_splash_slot(data);
            data.push(host.splash_material(slot)?);
            Ok(())
        }
        OP_SPLASHHASICON => {
            let slot = pop_splash_slot(data);
            data.push(host.splash_has_icon(slot)?);
            Ok(())
        }
        OP_SPLASHROWNUM => {
            let slot = pop_splash_slot(data);
            data.push(host.splash_row_num(slot)?);
            Ok(())
        }
        OP_MIN | OP_MAX => {
            let b = pop_data(data)?;
            let a = pop_data(data)?;
            let fa = source_float(&a);
            let fb = source_float(&b);
            let r = if op == OP_MIN { fa.min(fb) } else { fa.max(fb) };
            if matches!((&a, &b), (Operand::Int(_), Operand::Int(_))) {
                data.push(Operand::Int(r as i32));
            } else {
                data.push(Operand::Float(r));
            }
            Ok(())
        }
        OP_TABLELOOKUP => {
            let result_col = source_int(&pop_data(data)?);
            let key = source_str(&pop_data(data)?);
            let col0 = source_int(&pop_data(data)?);
            let table = source_str(&pop_data(data)?);
            data.push(host.table_lookup(&table, col0, &key, result_col)?);
            Ok(())
        }
        OP_TABLELOOKUPBYROW => {
            let col = source_int(&pop_data(data)?);
            let row = source_int(&pop_data(data)?);
            let table = source_str(&pop_data(data)?);
            data.push(host.table_lookup_by_row(&table, row, col)?);
            Ok(())
        }
        OP_GETPLAYERCARDINFO => {
            let slot = source_int(&pop_data(data)?);
            let lookup = source_int(&pop_data(data)?);
            let field = source_int(&pop_data(data)?);
            data.push(host.player_card_info(field, lookup, slot)?);
            Ok(())
        }
        OP_GETPERK => {
            let name = source_str(&pop_data(data)?);
            data.push(host.get_perk(&name)?);
            Ok(())
        }
        other => Err(ExprError::UnsupportedOp(other)),
    }
}

fn logic_op(op: i32, a: Operand, b: Operand) -> Result<Operand, ExprError> {
    if matches!(op, OP_EQUALS | OP_NOTEQUAL) {
        if let (Operand::Str(x), Operand::Str(y)) = (&a, &b) {
            let eq = x.eq_ignore_ascii_case(y);
            return Ok(Operand::Int(i32::from(if op == OP_EQUALS {
                eq
            } else {
                !eq
            })));
        }
    }
    let ia = source_int(&a);
    let ib = source_int(&b);
    let fa = source_float(&a);
    let fb = source_float(&b);
    let r = match op {
        OP_MULTIPLY => {
            if matches!((&a, &b), (Operand::Int(_), Operand::Int(_))) {
                return Ok(Operand::Int(ia.wrapping_mul(ib)));
            }
            return Ok(Operand::Float(fa * fb));
        }
        OP_DIVIDE => {
            if fb == 0.0 {
                return Ok(Operand::Int(0));
            }
            return Ok(Operand::Float(fa / fb));
        }
        OP_MODULUS => Operand::Int(if ib == 0 { 0 } else { ia % ib }),
        OP_ADD => {
            if matches!((&a, &b), (Operand::Str(_), _) | (_, Operand::Str(_))) {
                let mut s = source_str(&a);
                s.push_str(&source_str(&b));
                return Ok(Operand::Str(s));
            }
            if matches!((&a, &b), (Operand::Int(_), Operand::Int(_))) {
                return Ok(Operand::Int(ia.wrapping_add(ib)));
            }
            return Ok(Operand::Float(fa + fb));
        }
        OP_SUBTRACT => {
            if matches!((&a, &b), (Operand::Int(_), Operand::Int(_))) {
                return Ok(Operand::Int(ia.wrapping_sub(ib)));
            }
            return Ok(Operand::Float(fa - fb));
        }
        OP_LESSTHAN => Operand::Int(i32::from(fa < fb)),
        OP_LESSTHANEQUALTO => Operand::Int(i32::from(fa <= fb)),
        OP_GREATERTHAN => Operand::Int(i32::from(fa > fb)),
        OP_GREATERTHANEQUALTO => Operand::Int(i32::from(fa >= fb)),
        OP_EQUALS => Operand::Int(i32::from(ia == ib)),
        OP_NOTEQUAL => Operand::Int(i32::from(ia != ib)),
        OP_AND => Operand::Int(i32::from(ia != 0 && ib != 0)),
        OP_OR => Operand::Int(i32::from(ia != 0 || ib != 0)),
        other => return Err(ExprError::UnsupportedOp(other)),
    };
    Ok(r)
}

fn source_int(v: &Operand) -> i32 {
    match v {
        Operand::Int(n) => *n,
        Operand::Float(f) => *f as i32,
        Operand::Str(s) => s.parse().unwrap_or(0),
    }
}

fn source_float(v: &Operand) -> f32 {
    match v {
        Operand::Int(n) => *n as f32,
        Operand::Float(f) => *f,
        Operand::Str(s) => s.parse().unwrap_or(0.0),
    }
}

fn source_str(v: &Operand) -> String {
    match v {
        Operand::Str(s) => s.clone(),
        Operand::Int(n) => {
            let mut tmp = [0u8; 16];
            let mut i = tmp.len();
            let mut x = n.unsigned_abs();
            if x == 0 {
                return String::from("0");
            }
            while x > 0 {
                i -= 1;
                tmp[i] = b'0' + (x % 10) as u8;
                x /= 10;
            }
            if *n < 0 {
                i -= 1;
                tmp[i] = b'-';
            }
            String::from(core::str::from_utf8(&tmp[i..]).unwrap_or("0"))
        }
        Operand::Float(f) => {
            if *f == 0.0 {
                String::from("0")
            } else {
                String::from("f")
            }
        }
    }
}
