extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

const MENU_MS_TO_SEC: f32 = 0.001;

pub const MENU_TRANSITION_LERP: i32 = 1;

pub const MENU_TRANSITION_STRIDE: usize = 0x1c;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuTransition {
    pub transition_type: i32,

    pub target_field: i32,

    pub start_time: i32,

    pub start_val: f32,

    pub end_val: f32,

    pub time: f32,

    pub end_trigger_type: i32,
}

impl MenuTransition {
    #[must_use]
    pub fn active(&self) -> bool {
        self.transition_type != 0
    }

    #[must_use]
    pub fn eval(&self, now_ms: i32) -> f32 {
        if self.transition_type != MENU_TRANSITION_LERP {
            return 0.0;
        }
        if self.time <= 0.0 {
            return self.end_val;
        }
        let elapsed = (now_ms.wrapping_sub(self.start_time)) as f32 * MENU_MS_TO_SEC;
        if elapsed < 0.0 {
            return self.start_val;
        }
        if self.time < elapsed {
            return self.end_val;
        }
        self.start_val + elapsed * (self.end_val - self.start_val) / self.time
    }

    #[must_use]
    pub fn current_or(&self, now_ms: i32, idle: f32) -> f32 {
        if self.active() {
            self.eval(now_ms)
        } else {
            idle
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuLerpFromScript {
    pub scale: MenuTransition,
    pub alpha: MenuTransition,
    pub x: MenuTransition,
    pub y: MenuTransition,

    pub leftover: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuAnim {
    pub scale: f32,

    pub alpha: f32,

    pub offset: [f32; 2],
}

impl MenuAnim {
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        alpha: 1.0,
        offset: [0.0, 0.0],
    };
}

impl Default for MenuAnim {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl MenuLerpFromScript {
    #[must_use]
    pub fn anim(&self, now_ms: i32) -> MenuAnim {
        MenuAnim {
            scale: self.scale.current_or(now_ms, MenuAnim::IDENTITY.scale),
            alpha: self.alpha.current_or(now_ms, MenuAnim::IDENTITY.alpha),
            offset: [
                self.x.current_or(now_ms, 0.0),
                self.y.current_or(now_ms, 0.0),
            ],
        }
    }
}

#[must_use]
pub fn item_run_script_lerp(scripts: &[String], start_ms: i32) -> MenuLerpFromScript {
    let mut out = MenuLerpFromScript::default();
    for script in scripts {
        apply_script(&mut out, script, start_ms);
    }
    out
}

fn apply_script(out: &mut MenuLerpFromScript, script: &str, start_ms: i32) {
    let tokens = tokenize_menu_script(script);
    let mut i = 0usize;
    while i < tokens.len() {
        if tokens[i] == ";" {
            i += 1;
            continue;
        }
        let verb = tokens[i].as_str();
        i += 1;
        if !verb.eq_ignore_ascii_case("lerp") {
            let mut stmt = String::from(verb);
            while i < tokens.len() && tokens[i] != ";" {
                stmt.push(' ');
                stmt.push_str(&tokens[i]);
                i += 1;
            }
            out.leftover.push(stmt);
            continue;
        }
        match parse_lerp_args(&tokens, &mut i, start_ms) {
            Some((prop, slot)) => match prop {
                LerpProp::Scale => out.scale = slot,
                LerpProp::Alpha => out.alpha = slot,
                LerpProp::X => out.x = slot,
                LerpProp::Y => out.y = slot,
            },
            None => out.leftover.push(String::from("lerp")),
        }
    }
}

#[derive(Clone, Copy)]
enum LerpProp {
    Scale,
    Alpha,
    X,
    Y,
}

fn parse_lerp_args(
    tokens: &[String],
    i: &mut usize,
    start_ms: i32,
) -> Option<(LerpProp, MenuTransition)> {
    let prop = take(tokens, i)?;
    let kind = if prop.eq_ignore_ascii_case("scale") {
        LerpProp::Scale
    } else if prop.eq_ignore_ascii_case("alpha") {
        LerpProp::Alpha
    } else if prop.eq_ignore_ascii_case("x") {
        LerpProp::X
    } else if prop.eq_ignore_ascii_case("y") {
        LerpProp::Y
    } else {
        return None;
    };
    if !eq_kw(tokens, i, "from") {
        return None;
    }
    let from = parse_f32(take(tokens, i)?)?;
    if !eq_kw(tokens, i, "to") {
        return None;
    }
    let to = parse_f32(take(tokens, i)?)?;
    if !eq_kw(tokens, i, "over") {
        return None;
    }
    let over = parse_f32(take(tokens, i)?)?;
    Some((
        kind,
        MenuTransition {
            transition_type: MENU_TRANSITION_LERP,
            target_field: 0,
            start_time: start_ms,
            start_val: from,
            end_val: to,
            time: over,
            end_trigger_type: 0,
        },
    ))
}

fn take<'a>(tokens: &'a [String], i: &mut usize) -> Option<&'a str> {
    if *i < tokens.len() && tokens[*i] == ";" {
        return None;
    }
    let t = tokens.get(*i)?;
    *i += 1;
    Some(t.as_str())
}

fn eq_kw(tokens: &[String], i: &mut usize, kw: &str) -> bool {
    match take(tokens, i) {
        Some(t) => t.eq_ignore_ascii_case(kw),
        None => false,
    }
}

fn parse_f32(s: &str) -> Option<f32> {
    s.parse().ok()
}

fn tokenize_menu_script(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ';' {
            out.push(String::from(";"));
        } else if c == '"' {
            let mut buf = String::new();
            while let Some(ch) = chars.next() {
                if ch == '"' {
                    break;
                }
                if ch == '\\' {
                    if let Some(n) = chars.next() {
                        buf.push(n);
                    }
                } else {
                    buf.push(ch);
                }
            }
            out.push(buf);
        } else if c.is_whitespace() {
            continue;
        } else {
            let mut buf = String::from(c);
            while let Some(&n) = chars.peek() {
                if n.is_whitespace() || n == ';' || n == '"' {
                    break;
                }
                let Some(ch) = chars.next() else {
                    break;
                };
                buf.push(ch);
            }
            out.push(buf);
        }
    }
    out
}

#[must_use]
pub fn window_paint_scale_rect(x: f32, y: f32, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
    (
        x - w * (scale - 1.0) * 0.5,
        y - h * (scale - 1.0) * 0.5,
        w * scale,
        h * scale,
    )
}

#[must_use]
pub fn item_text_paint_scale(text_scale: f32, menu_scale: f32) -> f32 {
    text_scale * menu_scale
}
