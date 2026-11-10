use bevy::prelude::*;

use crate::{BindButton, KeyBinds};

fn script_key_name(button: BindButton) -> String {
    let display = crate::binds::display_button(button);
    match display.as_str() {
        "QUOTE" => "'",
        "COMMA" => ",",
        "PERIOD" => ".",
        "SLASH" => "/",
        "MINUS" => "-",
        "EQUAL" => "=",
        "BRACKETLEFT" => "[",
        "BRACKETRIGHT" => "]",
        "BACKSLASH" => "\\",
        _ => return display,
    }
    .into()
}

pub(crate) fn snapshot(binds: &KeyBinds) -> sim::LocalKeyBindings {
    let mut keys: Vec<_> = binds
        .iter()
        .filter(|(button, _)| !matches!(button, BindButton::Pad(_)))
        .collect();
    // Modifier aliases need stable physical-key order, independent of HashMap iteration.
    keys.sort_by_key(|(button, _)| crate::binds::host_keynum(*button));
    sim::LocalKeyBindings::from_commands(keys.into_iter().filter_map(|(button, id)| {
        input_iw4::command_name(id).map(|command| (script_key_name(button), command.to_owned()))
    }))
}

pub(crate) fn sync(
    binds: Res<KeyBinds>,
    mut bindings: ResMut<sim::LocalKeyBindings>,
    authority: Option<ResMut<net::AuthorityWorld>>,
) {
    if !binds.is_changed() {
        return;
    }
    let current = snapshot(&binds);
    if *bindings != current {
        *bindings = current.clone();
    }
    if let Some(mut authority) = authority {
        authority.0.set_local_key_bindings(current);
    }
}
