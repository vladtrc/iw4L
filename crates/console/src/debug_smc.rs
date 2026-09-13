use bevy::prelude::*;
use render_frontend::prepare::scene::smodel_geom_cache::SmcEnableDvar;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_smc_enable_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("r_smc_enable").is_none() {
        registry.register(
            crate::CommandSpec::new("r_smc_enable")
                .usage("r_smc_enable [0|1] — static model cache"),
        );
    }
}

pub(crate) fn route_smc_enable_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut smc: ResMut<SmcEnableDvar>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "r_smc_enable" {
            continue;
        }
        match cmd.args.as_slice() {
            [] => echo(
                format!(
                    "r_smc_enable = {}",
                    match smc.enabled {
                        None => "unread",
                        Some(true) => "1",
                        Some(false) => "0",
                    }
                ),
                &mut console,
                &mut line,
            ),
            [arg] => match parse_enable(arg) {
                Some(v) => {
                    smc.enabled = Some(v);
                    echo(
                        format!("r_smc_enable = {}", u8::from(v)),
                        &mut console,
                        &mut line,
                    );
                }
                None => echo("usage: r_smc_enable [0|1]".into(), &mut console, &mut line),
            },
            _ => echo("usage: r_smc_enable [0|1]".into(), &mut console, &mut line),
        }
    }
}

fn parse_enable(arg: &str) -> Option<bool> {
    match arg {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    }
}
