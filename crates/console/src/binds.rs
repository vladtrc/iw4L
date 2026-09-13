use input_iw4::{command_id_lookup, command_name};

use std::collections::HashMap;

use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::prelude::Resource;

pub const DEFAULT_CONTROLS: &str = include_str!("../assets/default_controls.cfg");

pub const BINDABLE_KEYS: &[&str] = &[
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "space",
    "tab",
    "shift",
    "ctrl",
    "alt",
    "enter",
    "backspace",
    "escape",
    "uparrow",
    "downarrow",
    "leftarrow",
    "rightarrow",
    "semicolon",
    "quote",
    "comma",
    "period",
    "slash",
    "minus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "mouse1",
    "mouse2",
    "mouse3",
    "mouse4",
    "mouse5",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindButton {
    Key(KeyCode),
    Mouse(MouseButton),
}

pub struct BindInputs<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
}

impl<'a> BindInputs<'a> {
    pub fn new(keys: &'a ButtonInput<KeyCode>, mouse: &'a ButtonInput<MouseButton>) -> Self {
        Self { keys, mouse }
    }

    pub fn pressed(&self, button: BindButton) -> bool {
        match button {
            BindButton::Key(key) => self.keys.pressed(key),
            BindButton::Mouse(btn) => self.mouse.pressed(btn),
        }
    }

    pub fn just_pressed(&self, button: BindButton) -> bool {
        match button {
            BindButton::Key(key) => self.keys.just_pressed(key),
            BindButton::Mouse(btn) => self.mouse.just_pressed(btn),
        }
    }

    pub fn just_released(&self, button: BindButton) -> bool {
        match button {
            BindButton::Key(key) => self.keys.just_released(key),
            BindButton::Mouse(btn) => self.mouse.just_released(btn),
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct KeyBinds {
    map: HashMap<BindButton, u32>,
}

impl KeyBinds {
    pub fn apply_defaults(&mut self) {
        self.map.clear();
        let _ = self.apply_script(DEFAULT_CONTROLS);
    }

    pub fn apply_script(&mut self, script: &str) -> Vec<String> {
        self.apply_script_inner(script, true)
    }

    pub(crate) fn apply_config_script(&mut self, script: &str) -> Vec<String> {
        self.apply_script_inner(script, false)
    }

    fn apply_script_inner(&mut self, script: &str, echo_success: bool) -> Vec<String> {
        let mut output = Vec::new();
        for raw in script.split([';', '\n']) {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let Some(command) = crate::ConsoleCommand::parse(line) else {
                continue;
            };
            match command.name.as_str() {
                "bind" => match self.cmd_bind(&command.args) {
                    Ok(Some(msg)) if echo_success => output.push(msg),
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(msg) => output.push(msg),
                },
                "unbind" => match self.cmd_unbind(&command.args) {
                    Ok(Some(msg)) if echo_success => output.push(msg),
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(msg) => output.push(msg),
                },
                "unbindall" => {
                    self.map.clear();
                    if echo_success {
                        output.push("unbindall".into());
                    }
                }
                other => output.push(format!("unknown bind-script command `{other}`")),
            }
        }
        output
    }

    pub fn set(&mut self, button: BindButton, id: u32) {
        self.map.insert(button, id);
    }

    pub fn clear_button(&mut self, button: BindButton) -> bool {
        self.map.remove(&button).is_some()
    }

    pub fn clear_command(&mut self, id: u32) -> bool {
        let before = self.map.len();
        self.map.retain(|_, bound| *bound != id);
        self.map.len() != before
    }

    pub fn clear_all(&mut self) {
        self.map.clear();
    }

    pub fn get(&self, button: BindButton) -> Option<u32> {
        self.map.get(&button).copied()
    }

    pub fn binding_name(&self, button: BindButton) -> Option<&'static str> {
        self.get(button).and_then(command_name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (BindButton, u32)> + '_ {
        self.map.iter().map(|(b, id)| (*b, *id))
    }

    pub fn list_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .map
            .iter()
            .filter_map(|(button, id)| {
                command_name(*id).map(|name| format!("bind {} {name}", display_button(*button)))
            })
            .collect();
        lines.sort();
        lines.dedup();
        lines
    }

    fn cmd_bind(&mut self, args: &[String]) -> Result<Option<String>, String> {
        match args {
            [] => Ok(None),
            [key] => {
                let buttons =
                    parse_button_name(key).ok_or_else(|| format!("unknown key `{key}`"))?;
                let names: Vec<&str> = buttons
                    .iter()
                    .filter_map(|button| self.binding_name(*button))
                    .collect();
                if names.is_empty() {
                    Ok(Some(format!("`{key}` is unbound")))
                } else {
                    Ok(Some(format!("bind {key} {}", names[0])))
                }
            }
            [key, action @ ..] => {
                let action = action.join(" ");
                let id = command_id_lookup(&action)
                    .ok_or_else(|| format!("unknown command `{action}`"))?;
                let buttons =
                    parse_button_name(key).ok_or_else(|| format!("unknown key `{key}`"))?;
                for button in buttons {
                    self.set(button, id);
                }
                let name = command_name(id).unwrap_or(action.as_str());
                Ok(Some(format!("bind {key} {name}")))
            }
        }
    }

    fn cmd_unbind(&mut self, args: &[String]) -> Result<Option<String>, String> {
        match args {
            [key] => {
                let buttons =
                    parse_button_name(key).ok_or_else(|| format!("unknown key `{key}`"))?;
                let mut any = false;
                for button in buttons {
                    any |= self.clear_button(button);
                }
                if any {
                    Ok(Some(format!("unbind {key}")))
                } else {
                    Ok(Some(format!("`{key}` is unbound")))
                }
            }
            _ => Err("usage: unbind <key>".into()),
        }
    }
}

pub fn host_keynum(button: BindButton) -> usize {
    match button {
        BindButton::Mouse(MouseButton::Left) => 180,
        BindButton::Mouse(MouseButton::Right) => 181,
        BindButton::Mouse(MouseButton::Middle) => 182,
        BindButton::Mouse(MouseButton::Back) => 183,
        BindButton::Mouse(MouseButton::Forward) => 184,
        BindButton::Mouse(_) => 185,
        BindButton::Key(key) => keycode_keynum(key),
    }
}

fn keycode_keynum(key: KeyCode) -> usize {
    match key {
        KeyCode::KeyA => 1,
        KeyCode::KeyB => 2,
        KeyCode::KeyC => 3,
        KeyCode::KeyD => 4,
        KeyCode::KeyE => 5,
        KeyCode::KeyF => 6,
        KeyCode::KeyG => 7,
        KeyCode::KeyH => 8,
        KeyCode::KeyI => 9,
        KeyCode::KeyJ => 10,
        KeyCode::KeyK => 11,
        KeyCode::KeyL => 12,
        KeyCode::KeyM => 13,
        KeyCode::KeyN => 14,
        KeyCode::KeyO => 15,
        KeyCode::KeyP => 16,
        KeyCode::KeyQ => 17,
        KeyCode::KeyR => 18,
        KeyCode::KeyS => 19,
        KeyCode::KeyT => 20,
        KeyCode::KeyU => 21,
        KeyCode::KeyV => 22,
        KeyCode::KeyW => 23,
        KeyCode::KeyX => 24,
        KeyCode::KeyY => 25,
        KeyCode::KeyZ => 26,
        KeyCode::Digit0 => 27,
        KeyCode::Digit1 => 28,
        KeyCode::Digit2 => 29,
        KeyCode::Digit3 => 30,
        KeyCode::Digit4 => 31,
        KeyCode::Digit5 => 32,
        KeyCode::Digit6 => 33,
        KeyCode::Digit7 => 34,
        KeyCode::Digit8 => 35,
        KeyCode::Digit9 => 36,
        KeyCode::Space => 37,
        KeyCode::Tab => 38,
        KeyCode::ShiftLeft => 39,
        KeyCode::ShiftRight => 40,
        KeyCode::ControlLeft => 41,
        KeyCode::ControlRight => 42,
        KeyCode::AltLeft => 43,
        KeyCode::AltRight => 44,
        KeyCode::Enter => 45,
        KeyCode::Backspace => 46,
        KeyCode::Escape => 47,
        KeyCode::ArrowUp => 48,
        KeyCode::ArrowDown => 49,
        KeyCode::ArrowLeft => 50,
        KeyCode::ArrowRight => 51,
        KeyCode::Semicolon => 52,
        KeyCode::Quote => 53,
        KeyCode::Comma => 54,
        KeyCode::Period => 55,
        KeyCode::Slash => 56,
        KeyCode::Minus => 57,
        KeyCode::Equal => 58,
        KeyCode::BracketLeft => 59,
        KeyCode::BracketRight => 60,
        KeyCode::Backslash => 61,
        _ => 62,
    }
}

pub fn parse_button_name(name: &str) -> Option<Vec<BindButton>> {
    let name = name.trim().to_ascii_lowercase();
    let button = match name.as_str() {
        "a" => BindButton::Key(KeyCode::KeyA),
        "b" => BindButton::Key(KeyCode::KeyB),
        "c" => BindButton::Key(KeyCode::KeyC),
        "d" => BindButton::Key(KeyCode::KeyD),
        "e" => BindButton::Key(KeyCode::KeyE),
        "f" => BindButton::Key(KeyCode::KeyF),
        "g" => BindButton::Key(KeyCode::KeyG),
        "h" => BindButton::Key(KeyCode::KeyH),
        "i" => BindButton::Key(KeyCode::KeyI),
        "j" => BindButton::Key(KeyCode::KeyJ),
        "k" => BindButton::Key(KeyCode::KeyK),
        "l" => BindButton::Key(KeyCode::KeyL),
        "m" => BindButton::Key(KeyCode::KeyM),
        "n" => BindButton::Key(KeyCode::KeyN),
        "o" => BindButton::Key(KeyCode::KeyO),
        "p" => BindButton::Key(KeyCode::KeyP),
        "q" => BindButton::Key(KeyCode::KeyQ),
        "r" => BindButton::Key(KeyCode::KeyR),
        "s" => BindButton::Key(KeyCode::KeyS),
        "t" => BindButton::Key(KeyCode::KeyT),
        "u" => BindButton::Key(KeyCode::KeyU),
        "v" => BindButton::Key(KeyCode::KeyV),
        "w" => BindButton::Key(KeyCode::KeyW),
        "x" => BindButton::Key(KeyCode::KeyX),
        "y" => BindButton::Key(KeyCode::KeyY),
        "z" => BindButton::Key(KeyCode::KeyZ),
        "0" => BindButton::Key(KeyCode::Digit0),
        "1" => BindButton::Key(KeyCode::Digit1),
        "2" => BindButton::Key(KeyCode::Digit2),
        "3" => BindButton::Key(KeyCode::Digit3),
        "4" => BindButton::Key(KeyCode::Digit4),
        "5" => BindButton::Key(KeyCode::Digit5),
        "6" => BindButton::Key(KeyCode::Digit6),
        "7" => BindButton::Key(KeyCode::Digit7),
        "8" => BindButton::Key(KeyCode::Digit8),
        "9" => BindButton::Key(KeyCode::Digit9),
        "space" => BindButton::Key(KeyCode::Space),
        "tab" => BindButton::Key(KeyCode::Tab),
        "enter" | "return" => BindButton::Key(KeyCode::Enter),
        "backspace" => BindButton::Key(KeyCode::Backspace),
        "escape" | "esc" => BindButton::Key(KeyCode::Escape),
        "uparrow" | "up" => BindButton::Key(KeyCode::ArrowUp),
        "downarrow" | "down" => BindButton::Key(KeyCode::ArrowDown),
        "leftarrow" | "left" => BindButton::Key(KeyCode::ArrowLeft),
        "rightarrow" | "right" => BindButton::Key(KeyCode::ArrowRight),
        "semicolon" => BindButton::Key(KeyCode::Semicolon),
        "quote" => BindButton::Key(KeyCode::Quote),
        "comma" => BindButton::Key(KeyCode::Comma),
        "period" => BindButton::Key(KeyCode::Period),
        "slash" => BindButton::Key(KeyCode::Slash),
        "minus" => BindButton::Key(KeyCode::Minus),
        "equal" | "equals" => BindButton::Key(KeyCode::Equal),
        "bracketleft" | "[" => BindButton::Key(KeyCode::BracketLeft),
        "bracketright" | "]" => BindButton::Key(KeyCode::BracketRight),
        "backslash" => BindButton::Key(KeyCode::Backslash),
        "shift" | "shiftleft" | "lshift" => {
            return Some(vec![
                BindButton::Key(KeyCode::ShiftLeft),
                BindButton::Key(KeyCode::ShiftRight),
            ]);
        }
        "shiftright" | "rshift" => BindButton::Key(KeyCode::ShiftRight),
        "ctrl" | "control" | "ctrlleft" | "lctrl" => {
            return Some(vec![
                BindButton::Key(KeyCode::ControlLeft),
                BindButton::Key(KeyCode::ControlRight),
            ]);
        }
        "ctrlright" | "rctrl" => BindButton::Key(KeyCode::ControlRight),
        "alt" | "altleft" | "lalt" => {
            return Some(vec![
                BindButton::Key(KeyCode::AltLeft),
                BindButton::Key(KeyCode::AltRight),
            ]);
        }
        "altright" | "ralt" => BindButton::Key(KeyCode::AltRight),
        "mouse1" | "mouseleft" | "lmb" => BindButton::Mouse(MouseButton::Left),
        "mouse2" | "mouseright" | "rmb" => BindButton::Mouse(MouseButton::Right),
        "mouse3" | "mousemiddle" | "mmb" => BindButton::Mouse(MouseButton::Middle),
        "mouse4" => BindButton::Mouse(MouseButton::Back),
        "mouse5" => BindButton::Mouse(MouseButton::Forward),
        _ => return None,
    };
    Some(vec![button])
}

pub fn parse_key_name(name: &str) -> Option<Vec<BindButton>> {
    parse_button_name(name)
}

pub fn display_button(button: BindButton) -> String {
    match button {
        BindButton::Key(key) => display_key(key),
        BindButton::Mouse(MouseButton::Left) => "MOUSE1".into(),
        BindButton::Mouse(MouseButton::Right) => "MOUSE2".into(),
        BindButton::Mouse(MouseButton::Middle) => "MOUSE3".into(),
        BindButton::Mouse(MouseButton::Back) => "MOUSE4".into(),
        BindButton::Mouse(MouseButton::Forward) => "MOUSE5".into(),
        BindButton::Mouse(other) => format!("{other:?}"),
    }
}

fn display_key(key: KeyCode) -> String {
    match key {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Space => "SPACE",
        KeyCode::Tab => "TAB",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "SHIFT",
        KeyCode::ControlLeft | KeyCode::ControlRight => "CTRL",
        KeyCode::AltLeft | KeyCode::AltRight => "ALT",
        KeyCode::Enter => "ENTER",
        KeyCode::Backspace => "BACKSPACE",
        KeyCode::Escape => "ESCAPE",
        KeyCode::ArrowUp => "UPARROW",
        KeyCode::ArrowDown => "DOWNARROW",
        KeyCode::ArrowLeft => "LEFTARROW",
        KeyCode::ArrowRight => "RIGHTARROW",
        KeyCode::Semicolon => "SEMICOLON",
        KeyCode::Quote => "QUOTE",
        KeyCode::Comma => "COMMA",
        KeyCode::Period => "PERIOD",
        KeyCode::Slash => "SLASH",
        KeyCode::Minus => "MINUS",
        KeyCode::Equal => "EQUAL",
        KeyCode::BracketLeft => "BRACKETLEFT",
        KeyCode::BracketRight => "BRACKETRIGHT",
        KeyCode::Backslash => "BACKSLASH",
        other => return format!("{other:?}"),
    }
    .to_owned()
}
