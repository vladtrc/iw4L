use bevy::prelude::*;
use render_fx::{FxMarkDvars, HostFxSystem};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_fx_mark_commands(registry: &mut ConsoleRegistry) {
    for (name, usage) in [
        ("fx_marks", "fx_marks [0|1] — ImpactMark outer gate"),
        (
            "fx_marks_smodels",
            "fx_marks_smodels [0|1] — models Generate OR with _ents",
        ),
        (
            "fx_marks_ents",
            "fx_marks_ents [0|1] — models Generate OR only; AddEntity* typed gap",
        ),
        (
            "fx_mark_profile",
            "fx_mark_profile [0|1] — marks census log/dump, not overlay UI",
        ),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(crate::CommandSpec::new(name).usage(usage));
        }
    }
}

pub(crate) fn route_fx_mark_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut dvars: ResMut<FxMarkDvars>,
    host: Option<Res<HostFxSystem>>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        let slot = match cmd.name.as_str() {
            "fx_marks" => &mut dvars.fx_marks,
            "fx_marks_smodels" => &mut dvars.fx_marks_smodels,
            "fx_marks_ents" => &mut dvars.fx_marks_ents,
            "fx_mark_profile" => &mut dvars.fx_mark_profile,
            _ => continue,
        };
        match cmd.args.as_slice() {
            [] => {
                let mut msg = format_mark_value(cmd.name.as_str(), *slot);
                if cmd.name == "fx_mark_profile" && *slot {
                    if let Some(host) = host.as_ref() {
                        msg.push(' ');
                        msg.push_str(&host.0.mark_profile_line());
                    }
                }
                echo(msg, &mut console, &mut line);
            }
            [arg] => match parse_01(arg) {
                Some(v) => {
                    *slot = v;
                    echo(
                        format_mark_value(cmd.name.as_str(), v),
                        &mut console,
                        &mut line,
                    );
                }
                None => echo(
                    format!("usage: {} [0|1]", cmd.name),
                    &mut console,
                    &mut line,
                ),
            },
            _ => echo(
                format!("usage: {} [0|1]", cmd.name),
                &mut console,
                &mut line,
            ),
        }
    }
}

fn format_mark_value(name: &str, value: bool) -> String {
    let bit = u8::from(value);
    match name {
        "fx_marks_ents" => {
            format!("{name} = {bit} (AddEntity* typed gap; models Generate OR only)")
        }
        "fx_mark_profile" => format!("{name} = {bit} (census log/dump, not overlay UI)"),
        "fx_marks" => {
            format!("{name} = {bit} (alloc + world verts; dyn packed fill)")
        }
        _ => format!("{name} = {bit}"),
    }
}

fn parse_01(arg: &str) -> Option<bool> {
    match arg {
        "0" | "off" | "false" => Some(false),
        "1" | "on" | "true" => Some(true),
        _ => None,
    }
}
