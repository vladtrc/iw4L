use bevy_ecs::prelude::Resource;
use std::collections::BTreeMap;

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalKeyBindings {
    commands: BTreeMap<u8, String>,
}
impl LocalKeyBindings {
    pub fn from_commands(commands: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut result = Self::default();
        for (name, command) in commands {
            if let Some(number) = key_number(&name) {
                result.commands.entry(number).or_insert(command);
            }
        }
        result
    }
    pub fn command_for_key(&self, name: &str) -> &str {
        key_number(name)
            .and_then(|number| self.commands.get(&number))
            .map_or("", String::as_str)
    }
    pub(crate) fn keys_for_command(&self, command: &str) -> Vec<u8> {
        self.commands
            .iter()
            .filter_map(|(&number, bound)| bound.eq_ignore_ascii_case(command).then_some(number))
            .take(2)
            .collect()
    }
}

fn key_number(name: &str) -> Option<u8> {
    let bytes = name.as_bytes();
    match bytes {
        [byte @ 1..=127] => Some(*byte),
        [b'0', b'x', hi, lo] => {
            let nibble = |b: u8| match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                _ => 0,
            };
            Some(nibble(*hi) * 16 + nibble(*lo))
        }
        _ => KEY_NAMES
            .iter()
            .find_map(|&(key, number, _)| key.eq_ignore_ascii_case(name).then_some(number)),
    }
}

pub(crate) fn key_label(number: u8) -> String {
    if (33..=126).contains(&number) && number != b'"' {
        return char::from(number).to_ascii_uppercase().to_string();
    }
    KEY_NAMES
        .iter()
        .find_map(|&(_, key, label)| (key == number).then_some(label.to_owned()))
        .unwrap_or_else(|| format!("0x{number:02x}"))
}

const KEY_NAMES: &[(&str, u8, &str)] = &[
    ("TAB", 9, "KEY_TAB"),
    ("ENTER", 13, "KEY_ENTER"),
    ("ESCAPE", 27, "KEY_ESCAPE"),
    ("SPACE", 32, "KEY_SPACE"),
    ("BACKSPACE", 127, "KEY_BACKSPACE"),
    ("UPARROW", 154, "KEY_UPARROW"),
    ("DOWNARROW", 155, "KEY_DOWNARROW"),
    ("LEFTARROW", 156, "KEY_LEFTARROW"),
    ("RIGHTARROW", 157, "KEY_RIGHTARROW"),
    ("ALT", 158, "KEY_ALT"),
    ("CTRL", 159, "KEY_CTRL"),
    ("SHIFT", 160, "KEY_SHIFT"),
    ("CAPSLOCK", 151, "KEY_CAPSLOCK"),
    ("F1", 167, "KEY_F1"),
    ("F2", 168, "KEY_F2"),
    ("F3", 169, "KEY_F3"),
    ("F4", 170, "KEY_F4"),
    ("F5", 171, "KEY_F5"),
    ("F6", 172, "KEY_F6"),
    ("F7", 173, "KEY_F7"),
    ("F8", 174, "KEY_F8"),
    ("F9", 175, "KEY_F9"),
    ("F10", 176, "KEY_F10"),
    ("F11", 177, "KEY_F11"),
    ("F12", 178, "KEY_F12"),
    ("INS", 161, "KEY_INS"),
    ("DEL", 162, "KEY_DEL"),
    ("PGDN", 163, "KEY_PGDN"),
    ("PGUP", 164, "KEY_PGUP"),
    ("HOME", 165, "KEY_HOME"),
    ("END", 166, "KEY_END"),
    ("MOUSE1", 200, "KEY_MOUSE1"),
    ("MOUSE2", 201, "KEY_MOUSE2"),
    ("MOUSE3", 202, "KEY_MOUSE3"),
    ("MOUSE4", 203, "KEY_MOUSE4"),
    ("MOUSE5", 204, "KEY_MOUSE5"),
    ("MWHEELUP", 206, "KEY_MWHEELUP"),
    ("MWHEELDOWN", 205, "KEY_MWHEELDOWN"),
    ("AUX1", 207, "KEY_AUX1"),
    ("AUX2", 208, "KEY_AUX2"),
    ("AUX3", 209, "KEY_AUX3"),
    ("AUX4", 210, "KEY_AUX4"),
    ("AUX5", 211, "KEY_AUX5"),
    ("AUX6", 212, "KEY_AUX6"),
    ("AUX7", 213, "KEY_AUX7"),
    ("AUX8", 214, "KEY_AUX8"),
    ("AUX9", 215, "KEY_AUX9"),
    ("AUX10", 216, "KEY_AUX10"),
    ("AUX11", 217, "KEY_AUX11"),
    ("AUX12", 218, "KEY_AUX12"),
    ("AUX13", 219, "KEY_AUX13"),
    ("AUX14", 220, "KEY_AUX14"),
    ("AUX15", 221, "KEY_AUX15"),
    ("AUX16", 222, "KEY_AUX16"),
    ("KP_HOME", 182, "KEY_KP_HOME"),
    ("KP_UPARROW", 183, "KEY_KP_UPARROW"),
    ("KP_PGUP", 184, "KEY_KP_PGUP"),
    ("KP_LEFTARROW", 185, "KEY_KP_LEFTARROW"),
    ("KP_5", 186, "KEY_KP_5"),
    ("KP_RIGHTARROW", 187, "KEY_KP_RIGHTARROW"),
    ("KP_END", 188, "KEY_KP_END"),
    ("KP_DOWNARROW", 189, "KEY_KP_DOWNARROW"),
    ("KP_PGDN", 190, "KEY_KP_PGDN"),
    ("KP_ENTER", 191, "KEY_KP_ENTER"),
    ("KP_INS", 192, "KEY_KP_INS"),
    ("KP_DEL", 193, "KEY_KP_DEL"),
    ("KP_SLASH", 194, "KEY_KP_SLASH"),
    ("KP_MINUS", 195, "KEY_KP_MINUS"),
    ("KP_PLUS", 196, "KEY_KP_PLUS"),
    ("KP_NUMLOCK", 197, "KEY_KP_NUMLOCK"),
    ("KP_STAR", 198, "KEY_KP_STAR"),
    ("KP_EQUALS", 199, "KEY_KP_EQUALS"),
    ("PAUSE", 153, "KEY_PAUSE"),
    ("SEMICOLON", 59, "KEY_SEMICOLON"),
    ("COMMAND", 150, "KEY_COMMAND"),
    ("181", 128, "µ"),
    ("191", 129, "¿"),
    ("223", 130, "ß"),
    ("224", 131, "à"),
    ("225", 132, "á"),
    ("228", 133, "ä"),
    ("229", 134, "å"),
    ("230", 135, "æ"),
    ("231", 136, "ç"),
    ("232", 137, "è"),
    ("233", 138, "é"),
    ("236", 139, "ì"),
    ("241", 140, "ñ"),
    ("242", 141, "ò"),
    ("243", 142, "ó"),
    ("246", 143, "ö"),
    ("248", 144, "ø"),
    ("249", 145, "ù"),
    ("250", 146, "ú"),
    ("252", 147, "ü"),
];
