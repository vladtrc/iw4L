use crate::ConsoleRegistry;
use crate::debug_scalar::debug_scalars;
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::dof::GlowDvars;

debug_scalars! {
    GlowDvars;
    "r_glow" { path: [enable], min: 0.0, max: 1.0, kind: bool },
    "r_glowUseTweaks" { path: [use_tweaks], min: 0.0, max: 1.0, kind: bool },
    "r_glowTweakEnable" { path: [tweak_enable], min: 0.0, max: 1.0, kind: bool },
    "r_glowTweakRadius0" { path: [tweak_radius], min: 0.0, max: 32.0, kind: float },
    "r_glowTweakBloomIntensity0" { path: [tweak_intensity], min: 0.0, max: 20.0, kind: float },
    "r_glowTweakBloomCutoff" { path: [tweak_cutoff], min: 0.0, max: 1.0, kind: float },
    "r_glowTweakBloomDesaturation" { path: [tweak_desaturation], min: 0.0, max: 1.0, kind: float },
    "r_glow_allowed" { path: [allowed], min: 0.0, max: 1.0, kind: bool },
    "r_glow_allowed_script_forced" { path: [allowed_script_forced], min: 0.0, max: 1.0, kind: bool },
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for &name in NAMES {
        if registry.resolve(name).is_none() {
            registry
                .register(crate::CommandSpec::new(name).usage(format!("{name} [value] — glow")));
        }
    }
}
