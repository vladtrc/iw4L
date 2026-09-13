use bevy::prelude::*;
use render_frontend::adapters::anim::view_kick::CgGunOffset;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_cg_gun_commands(registry: &mut ConsoleRegistry) {
    for (name, axis) in [
        ("cg_gun_x", "view forward"),
        ("cg_gun_y", "view right"),
        ("cg_gun_z", "view up"),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(
                crate::CommandSpec::new(name)
                    .usage(format!("{name} [inches] — viewmodel offset along {axis}")),
            );
        }
    }
}

pub(crate) fn route_cg_gun_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut gun: ResMut<CgGunOffset>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        let slot = match cmd.name.as_str() {
            "cg_gun_x" => &mut gun.x,
            "cg_gun_y" => &mut gun.y,
            "cg_gun_z" => &mut gun.z,
            _ => continue,
        };
        match cmd.args.as_slice() {
            [] => echo(format!("{} = {}", cmd.name, *slot), &mut console, &mut line),
            [arg] => match arg.parse::<f32>() {
                Ok(v) if v.is_finite() => {
                    *slot = v;
                    echo(format!("{} = {}", cmd.name, v), &mut console, &mut line);
                }
                _ => echo(
                    format!("usage: {} [inches]", cmd.name),
                    &mut console,
                    &mut line,
                ),
            },
            _ => echo(
                format!("usage: {} [inches]", cmd.name),
                &mut console,
                &mut line,
            ),
        }
    }
}
