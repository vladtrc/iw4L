use asset_material::material_bindings::*;
mod t6;

pub const POLICY: MaterialBindingFamily = MaterialBindingFamily {
    prepare_frame: |requested| {
        use crate::material::codes::*;
        let mut steps = Vec::new();
        fog::prepare(
            &mut steps,
            requested,
            fog::FogLayout {
                rows: CODE_T6_FOG,
                hdr_color_scale: true,
            },
        );
        t6::prepare_exposure(&mut steps, requested);
        sky::prepare(&mut steps, requested, &[], &[CODE_T6_SKY_COLOR_MULTIPLIER]);
        steps
    },
    prepare_local_light: |_| Vec::new(),
};
