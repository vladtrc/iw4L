use crate::{ConsoleCommand, ConsoleLine, ConsoleSettings, ConsoleState};
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::dof::DofDvars;

const NAMES: &[&str] = &[
    "r_dof_enable",
    "r_dof_tweak",
    "r_dof_nearBlur",
    "r_dof_farBlur",
    "r_dof_viewModelStart",
    "r_dof_viewModelEnd",
    "r_dof_nearStart",
    "r_dof_nearEnd",
    "r_dof_farStart",
    "r_dof_farEnd",
    "r_dof_bias",
];
pub(crate) fn register(registry: &mut crate::ConsoleRegistry) {
    for &name in NAMES {
        registry.register(
            crate::CommandSpec::new(name).usage(format!("{name} [value] — depth of field")),
        );
    }
}
pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    mut dvars: ResMut<DofDvars>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
) {
    for cmd in commands.read() {
        let name = cmd.name.as_str();
        if !NAMES.contains(&name) {
            continue;
        }
        let parsed = match cmd.args.as_slice() {
            [] => Ok(None),
            [value] => value
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map(Some)
                .ok_or(()),
            _ => Err(()),
        };
        let (current, min, max) = match name {
            "r_dof_enable" => (u8::from(dvars.enable) as f32, 0.0, 1.0),
            "r_dof_tweak" => (u8::from(dvars.tweak) as f32, 0.0, 1.0),
            "r_dof_nearBlur" => (dvars.values.near_blur, 4.0, 10.0),
            "r_dof_farBlur" => (dvars.values.far_blur, 0.0, 10.0),
            "r_dof_viewModelStart" => (dvars.values.view_model_start, 0.0, 128.0),
            "r_dof_viewModelEnd" => (dvars.values.view_model_end, 0.0, 128.0),
            "r_dof_nearStart" => (dvars.values.near_start, 0.0, 1000.0),
            "r_dof_nearEnd" => (dvars.values.near_end, 0.0, 1000.0),
            "r_dof_farStart" => (dvars.values.far_start, 0.0, 80000.0),
            "r_dof_farEnd" => (dvars.values.far_end, 0.0, 80000.0),
            "r_dof_bias" => (dvars.bias, 0.1, 3.0),
            _ => unreachable!(),
        };
        let msg = match parsed {
            Ok(None) => format!("{name} = {current} (domain {min}..{max})"),
            Ok(Some(value))
                if (min..=max).contains(&value)
                    && (!(name == "r_dof_enable" || name == "r_dof_tweak")
                        || value == 0.0
                        || value == 1.0) =>
            {
                match name {
                    "r_dof_enable" => dvars.enable = value != 0.0,
                    "r_dof_tweak" => dvars.tweak = value != 0.0,
                    "r_dof_nearBlur" => dvars.values.near_blur = value,
                    "r_dof_farBlur" => dvars.values.far_blur = value,
                    "r_dof_viewModelStart" => dvars.values.view_model_start = value,
                    "r_dof_viewModelEnd" => dvars.values.view_model_end = value,
                    "r_dof_nearStart" => dvars.values.near_start = value,
                    "r_dof_nearEnd" => dvars.values.near_end = value,
                    "r_dof_farStart" => dvars.values.far_start = value,
                    "r_dof_farEnd" => dvars.values.far_end = value,
                    "r_dof_bias" => dvars.bias = value,
                    _ => unreachable!(),
                }
                format!("{name} = {value}")
            }
            _ => format!("usage: {name} [finite value {min}..{max}]"),
        };
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, settings.log_capacity);
    }
}
