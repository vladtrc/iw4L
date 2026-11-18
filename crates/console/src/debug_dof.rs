use crate::debug_scalar::debug_scalars;
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::dof::DofDvars;

debug_scalars! {
    DofDvars;
    "r_dof_enable" { path: [enable], min: 0.0, max: 1.0, kind: bool },
    "r_dof_tweak" { path: [tweak], min: 0.0, max: 1.0, kind: bool },
    "r_dof_nearBlur" { path: [values.near_blur], min: 4.0, max: 10.0, kind: float },
    "r_dof_farBlur" { path: [values.far_blur], min: 0.0, max: 10.0, kind: float },
    "r_dof_viewModelStart" { path: [values.view_model_start], min: 0.0, max: 128.0, kind: float },
    "r_dof_viewModelEnd" { path: [values.view_model_end], min: 0.0, max: 128.0, kind: float },
    "r_dof_nearStart" { path: [values.near_start], min: 0.0, max: 1000.0, kind: float },
    "r_dof_nearEnd" { path: [values.near_end], min: 0.0, max: 1000.0, kind: float },
    "r_dof_farStart" { path: [values.far_start], min: 0.0, max: 80000.0, kind: float },
    "r_dof_farEnd" { path: [values.far_end], min: 0.0, max: 80000.0, kind: float },
    "r_dof_bias" { path: [bias], min: 0.1, max: 3.0, kind: float },
}
pub(crate) fn register(registry: &mut crate::ConsoleRegistry) {
    for &name in NAMES {
        registry.register(
            crate::CommandSpec::new(name).usage(format!("{name} [value] — depth of field")),
        );
    }
}
