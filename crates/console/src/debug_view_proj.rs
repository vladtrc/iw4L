use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use render_frontend::prepare::scene::view_parms::{
    PreparedSceneView, RLockPvs, RSubwindowDvar, RZnearDepthhackDvar, RZnearDvar,
};

use crate::{ConsoleCommand, ConsoleRegistry};

pub(crate) fn register_view_proj_commands(registry: &mut ConsoleRegistry) {
    for (name, usage) in [
        (
            "r_portalOnly",
            "r_portalOnly [-1|camera|view_id] — full-screen native view without aperture masks",
        ),
        (
            "r_portalMaxViews",
            "r_portalMaxViews [1..8] — frame view budget",
        ),
        (
            "r_portalDepth",
            "r_portalDepth [0..3] — rendered portal depth; deeper openings get fog",
        ),
        (
            "r_portalTint",
            "r_portalTint [0|1] — colour stencil regions by level",
        ),
    ] {
        registry.register(crate::CommandSpec::new(name).usage(usage));
    }
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

#[derive(SystemParam)]
pub(crate) struct ViewProjectionControls<'w> {
    znear: ResMut<'w, RZnearDvar>,
    lock_pvs: ResMut<'w, RLockPvs>,
    depthhack: ResMut<'w, RZnearDepthhackDvar>,
    subwindow: ResMut<'w, RSubwindowDvar>,
    portals: ResMut<'w, render_frontend::prepare::scene::view_parms::PortalRenderControls>,
}

pub(crate) fn route_view_proj_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: crate::feature_dispatch::ConsoleEcho,
    controls: ViewProjectionControls,
    prepared: Res<PreparedSceneView>,
) {
    let ViewProjectionControls {
        mut znear,
        mut lock_pvs,
        mut depthhack,
        mut subwindow,
        mut portals,
    } = controls;
    let mut echo = |msg: String| output.write(msg);

    for cmd in events.read() {
        match cmd.name.as_str() {
            "r_portalOnly" => {
                match cmd.args.as_slice() {
                    [] => {}
                    [v] if v == "-1" => portals.0.only = None,
                    [v] if v == "camera" => portals.0.only = Some(usize::MAX),
                    [v] => match v.parse::<usize>() {
                        Ok(i) if i < 255 => portals.0.only = Some(i),
                        _ => {
                            echo("usage: r_portalOnly [-1|camera|view_id]".into());
                            continue;
                        }
                    },
                    _ => {
                        echo("usage: r_portalOnly [-1|camera|view_id]".into());
                        continue;
                    }
                }
                echo(format!("portal controls: {:?}", portals.0));
            }
            "r_portalMaxViews" | "r_portalDepth" | "r_portalTint" => {
                if let [value] = cmd.args.as_slice() {
                    let valid = match cmd.name.as_str() {
                        "r_portalMaxViews" => value
                            .parse::<usize>()
                            .ok()
                            .filter(|v| (1..=8).contains(v))
                            .map(|v| portals.0.max_views = v)
                            .is_some(),
                        "r_portalDepth" => value
                            .parse::<u8>()
                            .ok()
                            .filter(|&v| v <= 3)
                            .map(|v| portals.0.depth = v)
                            .is_some(),
                        _ => parse_bool(value).map(|v| portals.0.tint = v).is_some(),
                    };
                    if !valid {
                        echo(format!("invalid {} value: {value}", cmd.name));
                        continue;
                    }
                } else if !cmd.args.is_empty() {
                    echo(format!("invalid {} arguments", cmd.name));
                    continue;
                }
                echo(format!("portal controls: {:?}", portals.0));
            }
            "r_znear" => match cmd.args.as_slice() {
                [] => echo(format!("r_znear = {}", znear.value)),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        znear.value = v;
                        echo(format!("r_znear = {}", znear.value));
                    }
                    _ => echo("usage: r_znear [f]".into()),
                },
                _ => echo("usage: r_znear [f]".into()),
            },
            "r_lockPvs" => match cmd.args.as_slice() {
                [] => echo(format!(
                    "r_lockPvs = {} frozen={}",
                    u8::from(lock_pvs.enabled),
                    u8::from(lock_pvs.frozen.is_some())
                )),
                [arg] => match parse_bool(arg) {
                    Some(true) => {
                        lock_pvs.enabled = true;
                        lock_pvs.recapture = true;
                        if prepared.ready {
                            lock_pvs.apply_after_stamp(&prepared);
                        }
                        echo("r_lockPvs = 1".into());
                    }
                    Some(false) => {
                        lock_pvs.enabled = false;
                        lock_pvs.recapture = false;
                        lock_pvs.frozen = None;
                        echo("r_lockPvs = 0".into());
                    }
                    None => echo("usage: r_lockPvs [0|1]".into()),
                },
                _ => echo("usage: r_lockPvs [0|1]".into()),
            },
            "r_znear_depthhack" => match cmd.args.as_slice() {
                [] => echo(format!("r_znear_depthhack = {}", depthhack.value)),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        depthhack.value = v;
                        echo(format!("r_znear_depthhack = {}", depthhack.value));
                    }
                    _ => echo("usage: r_znear_depthhack [f]".into()),
                },
                _ => echo("usage: r_znear_depthhack [f]".into()),
            },
            "r_subwindow" => match cmd.args.as_slice() {
                [] => echo(format!(
                    "r_subwindow = {} {} {} {}",
                    subwindow.left, subwindow.right, subwindow.top, subwindow.bottom
                )),
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
                        echo(format!(
                            "r_subwindow = {} {} {} {}",
                            subwindow.left, subwindow.right, subwindow.top, subwindow.bottom
                        ));
                    }
                    _ => echo("usage: r_subwindow [l r t b]".into()),
                },
                _ => echo("usage: r_subwindow [l r t b]".into()),
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
