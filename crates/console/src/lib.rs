pub mod binds;
mod class_dispatch;
mod debug_cg_gun;
mod debug_cl_yawspeed;
mod debug_distortion;
mod debug_dof;
mod debug_draw_method;
mod debug_fog;
mod debug_fx;
mod debug_fx_marks;
mod debug_glow;
mod debug_lod;
mod debug_move;
mod debug_script_mover;
mod debug_sm;
mod debug_smc;
mod debug_view_proj;
mod debug_vision;
mod diagnostics;
pub mod editor;
mod feature_dispatch;
pub mod input;
pub mod plugin;
pub mod registry;
pub mod suggest;
mod user_settings;
mod weapon_dispatch;

pub use binds::{
    BINDABLE_KEYS, BindButton, BindInputs, DEFAULT_CONTROLS, KeyBinds, display_button, host_keynum,
    parse_button_name, parse_key_name,
};
pub use class_dispatch::class_completions;
pub use editor::ConsoleEditor;
pub use feature_dispatch::register_feature_commands;
pub use plugin::{
    ConsoleCommandQueue, ConsoleDispatch, ConsoleDispatchSet, ConsoleFont, ConsolePlugin,
    ConsoleSettings, ConsoleState,
};
pub use registry::{ArgCompleter, CommandSpec, ConsoleRegistry, StaticCompleter};
pub use weapon_dispatch::{attach_completions, weapon_completions};

use input_iw4::{command_id_lookup, command_name};

use std::collections::BTreeSet;

use bevy::prelude::{Message, Resource};

#[derive(Resource, Debug, Default, Clone)]
pub struct ConsoleInputState {
    held: BTreeSet<u32>,
    timed: Vec<(u32, f32)>,

    pending_mouse: Option<(f32, f32)>,

    mouse_rate: Option<(f32, f32)>,
}

pub const PRESS_SECONDS: f32 = 0.15;

pub const PRESS_TICK_DT_MAX: f32 = 0.05;

fn plus_command_id(name: &str) -> Option<u32> {
    let id = command_id_lookup(name)?;
    if id < input_iw4::HOLD_PAIR_LIMIT && id % 2 == 0 {
        Some(id - 1)
    } else {
        Some(id)
    }
}

impl ConsoleInputState {
    pub fn hold(&mut self, input: &str) -> bool {
        let Some(id) = plus_command_id(input) else {
            return false;
        };
        self.timed.retain(|(held, _)| *held != id);
        self.held.insert(id)
    }

    pub fn press(&mut self, input: &str, seconds: f32) -> bool {
        let Some(id) = plus_command_id(input) else {
            return false;
        };
        self.held.insert(id);
        match self.timed.iter_mut().find(|(held, _)| *held == id) {
            Some(slot) => slot.1 = slot.1.max(seconds),
            None => self.timed.push((id, seconds)),
        }
        true
    }

    pub fn tick(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, PRESS_TICK_DT_MAX);
        for (_, remaining) in self.timed.iter_mut() {
            *remaining -= dt;
        }
        for (id, _) in self.timed.iter().filter(|(_, left)| *left <= 0.0) {
            self.held.remove(id);
        }
        self.timed.retain(|(_, left)| *left > 0.0);
    }

    pub fn release(&mut self, input: &str) -> bool {
        let Some(id) = plus_command_id(input) else {
            return false;
        };
        self.timed.retain(|(held, _)| *held != id);
        self.held.remove(&id)
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.timed.clear();
        self.pending_mouse = None;
        self.mouse_rate = None;
    }

    pub fn queue_mouse(&mut self, dx: f32, dy: f32) {
        match &mut self.pending_mouse {
            Some((x, y)) => {
                *x += dx;
                *y += dy;
            }
            None => self.pending_mouse = Some((dx, dy)),
        }
    }

    pub fn take_mouse(&mut self) -> (f32, f32) {
        self.pending_mouse.take().unwrap_or((0.0, 0.0))
    }

    pub fn mouse_rate(&self) -> Option<(f32, f32)> {
        self.mouse_rate
    }

    pub fn set_mouse_rate(&mut self, dx: f32, dy: f32) {
        self.mouse_rate = if dx == 0.0 && dy == 0.0 {
            None
        } else {
            Some((dx, dy))
        };
    }

    pub fn held(&self, input: &str) -> bool {
        plus_command_id(input).is_some_and(|id| self.held.contains(&id))
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.held.iter().copied()
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.held.iter().filter_map(|id| command_name(*id))
    }
}

pub(crate) fn is_bind_command(word: &str) -> bool {
    command_id_lookup(word).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq, Message)]
pub struct ConsoleCommand {
    pub name: String,
    pub args: Vec<String>,
    pub raw: String,

    pub background: bool,

    pub interactive: bool,
}

impl ConsoleCommand {
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim().strip_prefix('/').unwrap_or(line.trim()).trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut words: Vec<String> = trimmed.split_whitespace().map(str::to_owned).collect();

        let background = matches!(words.last().map(String::as_str), Some("&"));
        if background {
            words.pop();
        }

        let bang_token = matches!(words.last().map(String::as_str), Some("!"));
        if bang_token {
            words.pop();
        }
        let raw = words.join(" ");
        if raw.is_empty() {
            return None;
        }
        let mut words = words.into_iter();

        let mut name = words.next()?;
        let bang_name = name.ends_with('!') && name.len() > 1;
        if bang_name {
            name.pop();
        }
        Some(Self {
            name,
            args: words.collect(),
            raw,
            background,
            interactive: bang_token || bang_name,
        })
    }

    pub fn parse_script(line: &str) -> Vec<Self> {
        line.split(';').filter_map(Self::parse).collect()
    }
}

pub type SubmittedCommand = ConsoleCommand;

#[derive(Resource, Debug, Default)]
pub struct ConsoleQueue {
    pending: Vec<ConsoleCommand>,
}

impl ConsoleQueue {
    pub fn push_line(&mut self, line: &str) {
        self.pending.extend(ConsoleCommand::parse_script(line));
    }

    pub fn drain(&mut self) -> Vec<ConsoleCommand> {
        core::mem::take(&mut self.pending)
    }
}

pub fn startup_commands() -> Vec<String> {
    startup_commands_from(std::env::args(), std::env::var("IW4L_CMDS").ok())
}

pub fn strip_cmds_flag(args: impl Iterator<Item = String>) -> Vec<String> {
    let mut skip_value = false;
    args.filter(|arg| {
        let drop = skip_value || arg == "--cmds" || arg.starts_with("--cmds=");
        skip_value = arg == "--cmds";
        !drop
    })
    .collect()
}

fn startup_commands_from(args: impl Iterator<Item = String>, env: Option<String>) -> Vec<String> {
    let mut scripts = Vec::new();
    let mut args = args.skip_while(|a| a != "--cmds" && !a.starts_with("--cmds="));
    match args.next() {
        Some(flag) if flag == "--cmds" => scripts.extend(args.next()),
        Some(inline) => scripts.push(inline["--cmds=".len()..].to_owned()),
        None => {}
    }
    scripts.extend(env);
    scripts
        .iter()
        .flat_map(|script| script.split(';'))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Resource, Debug, Default, Clone)]
pub struct ConsoleLine(pub String);
