use bevy::prelude::*;
use net::{ClientActionInbox, LocalPresentClient};
use sim::ClientAction;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_debug_script_mover_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("rotatevelocity").is_none() {
        registry.register(crate::CommandSpec::new("rotatevelocity").usage(
            "rotatevelocity <deg_per_sec> — supplied GSC rotatevelocity speed on spawned fan blades (listen)",
        ));
    }
}

pub(crate) fn route_debug_script_mover_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    local: Res<LocalPresentClient>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut inbox: Option<ResMut<ClientActionInbox>>,
    mut seq: ResMut<net::ActionRequestIds>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "rotatevelocity" {
            continue;
        }
        if cmd.args.len() != 1 {
            echo(
                "usage: rotatevelocity <deg_per_sec>".into(),
                &mut console,
                &mut line,
            );
            continue;
        }
        let speed = match cmd.args[0].parse::<f32>() {
            Ok(v) if v.is_finite() => v,
            _ => {
                echo(
                    format!("rotatevelocity: not a finite number `{}`", cmd.args[0]),
                    &mut console,
                    &mut line,
                );
                continue;
            }
        };
        if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
            echo(
                "rotatevelocity: cheats are off".into(),
                &mut console,
                &mut line,
            );
            continue;
        }
        let Some(inbox) = inbox.as_deref_mut() else {
            echo(
                "rotatevelocity: no action inbox (not a listen host)".into(),
                &mut console,
                &mut line,
            );
            continue;
        };
        let request_id = seq.allocate();
        if let Err(error) = inbox.push(
            local.0,
            ClientAction::BeginScriptMoverRotateVelocity { request_id, speed },
        ) {
            echo(format!("rotatevelocity: {error}"), &mut console, &mut line);
            continue;
        }
        echo(
            format!(
                "rotatevelocity: queued speed={speed} request_id={request_id} (supplied, not randomfloatrange)"
            ),
            &mut console,
            &mut line,
        );
    }
}
