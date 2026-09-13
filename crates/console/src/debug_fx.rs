use bevy::prelude::*;
use render_fx::FxDumpRequest;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_debug_fx_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("fx_dump").is_none() {
        registry.register(
            crate::CommandSpec::new("fx_dump")
                .usage("fx_dump [radius] — log nearby CodeMesh sprites (needs a live world)"),
        );
    }
}

pub(crate) fn route_debug_fx_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut dump: Option<ResMut<FxDumpRequest>>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "fx_dump" {
            continue;
        }
        let Some(dump) = dump.as_deref_mut() else {
            echo(
                "fx_dump: render FX dump request missing (no RenderPlugin)".into(),
                &mut console,
                &mut line,
            );
            continue;
        };
        match cmd.args.as_slice() {
            [] => {}
            [radius] => match radius.parse::<f32>() {
                Ok(value) if value.is_finite() && value > 0.0 => dump.radius = value,
                _ => {
                    echo(
                        format!("usage: fx_dump [radius] (got `{radius}`)"),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
            },
            _ => {
                echo("usage: fx_dump [radius]".into(), &mut console, &mut line);
                continue;
            }
        }
        dump.pending = true;
        echo(
            format!(
                "fx_dump: queued radius={:.0} (next Effects tick)",
                dump.radius
            ),
            &mut console,
            &mut line,
        );
    }
}
