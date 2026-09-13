use bevy::prelude::*;
use render_frontend::prepare::scene::view_parms::{SmEnableDvar, SmSunEnableDvar};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_sm_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("sm_enable").is_none() {
        registry.register(
            crate::CommandSpec::new("sm_enable").usage("sm_enable [0|1] — shadow-map gate"),
        );
    }
    if registry.resolve("sm_sunEnable").is_none() {
        registry.register(
            crate::CommandSpec::new("sm_sunEnable").usage("sm_sunEnable [0|1] — used bits 1..=sun"),
        );
    }
}

pub(crate) fn route_sm_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut sm_enable: ResMut<SmEnableDvar>,
    mut sm_sun: ResMut<SmSunEnableDvar>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "sm_enable" => match cmd.args.as_slice() {
                [] => echo(
                    format!("sm_enable = {}", format_opt(sm_enable.enabled)),
                    &mut console,
                    &mut line,
                ),
                [arg] => match parse_enable(arg) {
                    Some(v) => {
                        sm_enable.enabled = Some(v);
                        echo(
                            format!("sm_enable = {}", u8::from(v)),
                            &mut console,
                            &mut line,
                        );
                    }
                    None => echo("usage: sm_enable [0|1]".into(), &mut console, &mut line),
                },
                _ => echo("usage: sm_enable [0|1]".into(), &mut console, &mut line),
            },
            "sm_sunEnable" => match cmd.args.as_slice() {
                [] => echo(
                    format!("sm_sunEnable = {}", format_opt(sm_sun.enabled)),
                    &mut console,
                    &mut line,
                ),
                [arg] => match parse_enable(arg) {
                    Some(v) => {
                        sm_sun.enabled = Some(v);
                        echo(
                            format!("sm_sunEnable = {}", u8::from(v)),
                            &mut console,
                            &mut line,
                        );
                    }
                    None => echo("usage: sm_sunEnable [0|1]".into(), &mut console, &mut line),
                },
                _ => echo("usage: sm_sunEnable [0|1]".into(), &mut console, &mut line),
            },
            _ => {}
        }
    }
}

fn format_opt(v: Option<bool>) -> &'static str {
    match v {
        None => "unread",
        Some(true) => "1",
        Some(false) => "0",
    }
}

fn parse_enable(arg: &str) -> Option<bool> {
    match arg {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    }
}
