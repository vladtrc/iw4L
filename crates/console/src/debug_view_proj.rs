use bevy::prelude::*;
use render_frontend::prepare::scene::view_parms::{
    PreparedSceneView, RLockPvs, RSubwindowDvar, RZnearDepthhackDvar, RZnearDvar,
};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_view_proj_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("r_znear").is_none() {
        registry.register(
            crate::CommandSpec::new("r_znear").usage("r_znear [f] — DPVS infinite near; default 4"),
        );
    }
    if registry.resolve("r_lockPvs").is_none() {
        registry.register(
            crate::CommandSpec::new("r_lockPvs")
                .usage("r_lockPvs [0|1] — freeze portal-walk viewpoint (`lockPvsViewParms`)"),
        );
    }
    if registry.resolve("r_znear_depthhack").is_none() {
        registry.register(
            crate::CommandSpec::new("r_znear_depthhack")
                .usage("r_znear_depthhack [f] — viewmodel reverse-Z near; default 0.1"),
        );
    }
    if registry.resolve("r_subwindow").is_none() {
        registry.register(
            crate::CommandSpec::new("r_subwindow")
                .usage("r_subwindow [l r t b] — draw rect left,right,top,bottom; default 0 1 0 1"),
        );
    }
}

pub(crate) fn route_view_proj_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut znear: ResMut<RZnearDvar>,
    mut lock_pvs: ResMut<RLockPvs>,
    mut depthhack: ResMut<RZnearDepthhackDvar>,
    mut subwindow: ResMut<RSubwindowDvar>,
    prepared: Res<PreparedSceneView>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "r_znear" => match cmd.args.as_slice() {
                [] => echo(
                    format!("r_znear = {}", znear.value),
                    &mut console,
                    &mut line,
                ),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        znear.value = v;
                        echo(
                            format!("r_znear = {}", znear.value),
                            &mut console,
                            &mut line,
                        );
                    }
                    _ => echo("usage: r_znear [f]".into(), &mut console, &mut line),
                },
                _ => echo("usage: r_znear [f]".into(), &mut console, &mut line),
            },
            "r_lockPvs" => match cmd.args.as_slice() {
                [] => echo(
                    format!(
                        "r_lockPvs = {} frozen={}",
                        u8::from(lock_pvs.enabled),
                        u8::from(lock_pvs.frozen.is_some())
                    ),
                    &mut console,
                    &mut line,
                ),
                [arg] => match parse_bool(arg) {
                    Some(true) => {
                        lock_pvs.enabled = true;
                        lock_pvs.recapture = true;
                        if prepared.ready {
                            lock_pvs.apply_after_stamp(&prepared);
                        }
                        echo("r_lockPvs = 1".into(), &mut console, &mut line);
                    }
                    Some(false) => {
                        lock_pvs.enabled = false;
                        lock_pvs.recapture = false;
                        lock_pvs.frozen = None;
                        echo("r_lockPvs = 0".into(), &mut console, &mut line);
                    }
                    None => echo("usage: r_lockPvs [0|1]".into(), &mut console, &mut line),
                },
                _ => echo("usage: r_lockPvs [0|1]".into(), &mut console, &mut line),
            },
            "r_znear_depthhack" => match cmd.args.as_slice() {
                [] => echo(
                    format!("r_znear_depthhack = {}", depthhack.value),
                    &mut console,
                    &mut line,
                ),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        depthhack.value = v;
                        echo(
                            format!("r_znear_depthhack = {}", depthhack.value),
                            &mut console,
                            &mut line,
                        );
                    }
                    _ => echo(
                        "usage: r_znear_depthhack [f]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_znear_depthhack [f]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            "r_subwindow" => match cmd.args.as_slice() {
                [] => echo(
                    format!(
                        "r_subwindow = {} {} {} {}",
                        subwindow.left, subwindow.right, subwindow.top, subwindow.bottom
                    ),
                    &mut console,
                    &mut line,
                ),
                [a, b, c, d] => match (
                    a.parse::<f32>(),
                    b.parse::<f32>(),
                    c.parse::<f32>(),
                    d.parse::<f32>(),
                ) {
                    (Ok(l), Ok(r), Ok(t), Ok(bot))
                        if l.is_finite() && r.is_finite() && t.is_finite() && bot.is_finite() =>
                    {
                        subwindow.left = l;
                        subwindow.right = r;
                        subwindow.top = t;
                        subwindow.bottom = bot;
                        echo(
                            format!(
                                "r_subwindow = {} {} {} {}",
                                subwindow.left, subwindow.right, subwindow.top, subwindow.bottom
                            ),
                            &mut console,
                            &mut line,
                        );
                    }
                    _ => echo(
                        "usage: r_subwindow [l r t b]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_subwindow [l r t b]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            _ => {}
        }
    }
}

fn parse_bool(arg: &str) -> Option<bool> {
    match arg {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    }
}
