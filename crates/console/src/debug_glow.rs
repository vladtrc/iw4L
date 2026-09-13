use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::dof::GlowDvars;

const NAMES: &[&str] = &[
    "r_glow",
    "r_glowUseTweaks",
    "r_glowTweakEnable",
    "r_glowTweakRadius0",
    "r_glowTweakBloomIntensity0",
    "r_glowTweakBloomCutoff",
    "r_glowTweakBloomDesaturation",
    "r_glow_allowed",
    "r_glow_allowed_script_forced",
];

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for &name in NAMES {
        if registry.resolve(name).is_none() {
            registry.register(
                crate::CommandSpec::new(name)
                    .usage(format!("{name} [value] — glow (`R_SetGlowInfo`)")),
            );
        }
    }
}

pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    mut dvars: ResMut<GlowDvars>,
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
        let (current, min, max, is_bool) = match name {
            "r_glow" => (u8::from(dvars.enable) as f32, 0.0, 1.0, true),
            "r_glowUseTweaks" => (u8::from(dvars.use_tweaks) as f32, 0.0, 1.0, true),
            "r_glowTweakEnable" => (u8::from(dvars.tweak_enable) as f32, 0.0, 1.0, true),
            "r_glowTweakRadius0" => (dvars.tweak_radius, 0.0, 32.0, false),
            "r_glowTweakBloomIntensity0" => (dvars.tweak_intensity, 0.0, 20.0, false),
            "r_glowTweakBloomCutoff" => (dvars.tweak_cutoff, 0.0, 1.0, false),
            "r_glowTweakBloomDesaturation" => (dvars.tweak_desaturation, 0.0, 1.0, false),
            "r_glow_allowed" => (u8::from(dvars.allowed) as f32, 0.0, 1.0, true),
            "r_glow_allowed_script_forced" => {
                (u8::from(dvars.allowed_script_forced) as f32, 0.0, 1.0, true)
            }
            _ => unreachable!(),
        };
        let msg = match parsed {
            Ok(None) => format!("{name} = {current} (domain {min}..{max})"),
            Ok(Some(value))
                if (min..=max).contains(&value) && (!is_bool || value == 0.0 || value == 1.0) =>
            {
                match name {
                    "r_glow" => dvars.enable = value != 0.0,
                    "r_glowUseTweaks" => dvars.use_tweaks = value != 0.0,
                    "r_glowTweakEnable" => dvars.tweak_enable = value != 0.0,
                    "r_glowTweakRadius0" => dvars.tweak_radius = value,
                    "r_glowTweakBloomIntensity0" => dvars.tweak_intensity = value,
                    "r_glowTweakBloomCutoff" => dvars.tweak_cutoff = value,
                    "r_glowTweakBloomDesaturation" => dvars.tweak_desaturation = value,
                    "r_glow_allowed" => dvars.allowed = value != 0.0,
                    "r_glow_allowed_script_forced" => dvars.allowed_script_forced = value != 0.0,
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
