use bevy::prelude::*;
use net::ClientActionInput;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_cl_yawspeed_command(registry: &mut ConsoleRegistry) {
    if registry.resolve("cl_yawspeed").is_none() {
        registry.register(
            crate::CommandSpec::new("cl_yawspeed")
                .usage("cl_yawspeed [deg/s] — keyboard +left/+right rate; default 140"),
        );
    }
}

pub(crate) fn route_cl_yawspeed_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut actions: ResMut<ClientActionInput>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "cl_yawspeed" {
            continue;
        }
        match cmd.args.as_slice() {
            [] => echo(
                format!("cl_yawspeed = {}", actions.cl_yawspeed),
                &mut console,
                &mut line,
            ),
            [arg] => match arg.parse::<f32>() {
                Ok(v) if v.is_finite() => {
                    actions.cl_yawspeed = v;
                    echo(
                        format!("cl_yawspeed = {}", actions.cl_yawspeed),
                        &mut console,
                        &mut line,
                    );
                }
                _ => echo("usage: cl_yawspeed [deg/s]".into(), &mut console, &mut line),
            },
            _ => echo("usage: cl_yawspeed [deg/s]".into(), &mut console, &mut line),
        }
    }
}
