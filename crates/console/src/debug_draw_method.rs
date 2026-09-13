use bevy::prelude::*;
use render_frontend::assemble::drawsurf::ColourDrawMethod;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_debug_draw_method_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("r_fullbright").is_none() {
        registry.register(
            crate::CommandSpec::new("r_fullbright")
                .usage("r_fullbright [0|1] — Colour draw-method standard (9) or fullbright (4)"),
        );
    }
}

pub(crate) fn route_debug_draw_method_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut draw_method: ResMut<ColourDrawMethod>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "r_fullbright" {
            continue;
        }
        match cmd.args.as_slice() {
            [] => echo(
                format!(
                    "r_fullbright = {} ({})",
                    u8::from(*draw_method != ColourDrawMethod::Standard),
                    match *draw_method {
                        ColourDrawMethod::Standard => "standard tech 9",
                        ColourDrawMethod::Fullbright => "fullbright tech 4",
                        ColourDrawMethod::DebugMaterial => "debug-material tech 46",
                    }
                ),
                &mut console,
                &mut line,
            ),
            [arg] => match parse_fullbright_arg(arg) {
                Some(true) => {
                    *draw_method = ColourDrawMethod::Fullbright;
                    echo(
                        "r_fullbright = 1 (ColourDrawMethod::Fullbright, tech 4)".into(),
                        &mut console,
                        &mut line,
                    );
                }
                Some(false) => {
                    *draw_method = ColourDrawMethod::Standard;
                    echo(
                        "r_fullbright = 0 (ColourDrawMethod::Standard, tech 9)".into(),
                        &mut console,
                        &mut line,
                    );
                }
                None => echo("usage: r_fullbright [0|1]".into(), &mut console, &mut line),
            },
            _ => echo("usage: r_fullbright [0|1]".into(), &mut console, &mut line),
        }
    }
}

fn parse_fullbright_arg(arg: &str) -> Option<bool> {
    match arg {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    }
}
