use bevy::prelude::*;
use render_frontend::prepare::scene::smodel_geom_cache::{LodRampDvar, LodRampSkinnedDvar};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_lod_ramp_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("r_lodScaleRigid").is_none() {
        registry.register(
            crate::CommandSpec::new("r_lodScaleRigid")
                .usage("r_lodScaleRigid [f] — rigid LOD distance scale"),
        );
    }
    if registry.resolve("r_lodBiasRigid").is_none() {
        registry.register(
            crate::CommandSpec::new("r_lodBiasRigid")
                .usage("r_lodBiasRigid [f] — rigid LOD distance bias"),
        );
    }
    if registry.resolve("r_lodScaleSkinned").is_none() {
        registry.register(
            crate::CommandSpec::new("r_lodScaleSkinned")
                .usage("r_lodScaleSkinned [f] — skinned LOD distance scale"),
        );
    }
    if registry.resolve("r_lodBiasSkinned").is_none() {
        registry.register(
            crate::CommandSpec::new("r_lodBiasSkinned")
                .usage("r_lodBiasSkinned [f] — skinned LOD distance bias"),
        );
    }
}

pub(crate) fn route_lod_ramp_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut ramps: (ResMut<LodRampDvar>, ResMut<LodRampSkinnedDvar>),
) {
    let (ramp, skinned) = &mut ramps;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let show_rigid = |ramp: &LodRampDvar| {
        format!(
            "r_lodScaleRigid = {}  r_lodBiasRigid = {}  scale_last = {}  world_unit = {}  t5 scale/bias/applied = {}/{}/{}",
            opt_f32(ramp.scale_mid),
            opt_f32(ramp.bias_mid),
            opt_f32(ramp.scale_last),
            opt_f32(ramp.world_unit),
            ramp.t5_scale,
            ramp.t5_bias,
            ramp.t5.applied_inv_scale
        )
    };
    let show_skinned = |ramp: &LodRampSkinnedDvar| {
        format!(
            "r_lodScaleSkinned = {}  r_lodBiasSkinned = {}  scale_last = {}  world_unit = {}  t5 scale/bias/applied = {}/{}/{}",
            opt_f32(ramp.scale_mid),
            opt_f32(ramp.bias_mid),
            opt_f32(ramp.scale_last),
            opt_f32(ramp.world_unit),
            ramp.t5_scale,
            ramp.t5_bias,
            ramp.t5.applied_inv_scale
        )
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "r_lodScaleRigid" => match cmd.args.as_slice() {
                [] => echo(show_rigid(ramp), &mut console, &mut line),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        ramp.scale_mid = Some(v);
                        ramp.t5_scale = v;
                        echo(show_rigid(ramp), &mut console, &mut line);
                    }
                    _ => echo(
                        "usage: r_lodScaleRigid [float]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_lodScaleRigid [float]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            "r_lodBiasRigid" => match cmd.args.as_slice() {
                [] => echo(show_rigid(ramp), &mut console, &mut line),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        ramp.bias_mid = Some(v);
                        ramp.t5_bias = v;
                        echo(show_rigid(ramp), &mut console, &mut line);
                    }
                    _ => echo(
                        "usage: r_lodBiasRigid [float]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_lodBiasRigid [float]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            "r_lodScaleSkinned" => match cmd.args.as_slice() {
                [] => echo(show_skinned(skinned), &mut console, &mut line),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        skinned.scale_mid = Some(v);
                        skinned.t5_scale = v;
                        echo(show_skinned(skinned), &mut console, &mut line);
                    }
                    _ => echo(
                        "usage: r_lodScaleSkinned [float]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_lodScaleSkinned [float]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            "r_lodBiasSkinned" => match cmd.args.as_slice() {
                [] => echo(show_skinned(skinned), &mut console, &mut line),
                [arg] => match arg.parse::<f32>() {
                    Ok(v) if v.is_finite() => {
                        skinned.bias_mid = Some(v);
                        skinned.t5_bias = v;
                        echo(show_skinned(skinned), &mut console, &mut line);
                    }
                    _ => echo(
                        "usage: r_lodBiasSkinned [float]".into(),
                        &mut console,
                        &mut line,
                    ),
                },
                _ => echo(
                    "usage: r_lodBiasSkinned [float]".into(),
                    &mut console,
                    &mut line,
                ),
            },
            _ => {}
        }
    }
}

fn opt_f32(v: Option<f32>) -> String {
    match v {
        None => "unread".into(),
        Some(f) => format!("{f}"),
    }
}
