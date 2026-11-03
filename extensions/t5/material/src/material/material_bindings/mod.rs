use asset_material::material_bindings::*;
mod t5;
mod t5_light;

pub const POLICY: MaterialBindingFamily = MaterialBindingFamily {
    prepare_frame: |requested| {
        use crate::material::t5_code_remap::*;
        let mut steps = Vec::new();
        t5::prepare_camera(&mut steps, requested);
        fog::prepare(
            &mut steps,
            requested,
            fog::FogLayout {
                rows: [
                    T5_CODE_FOG_COLOR,
                    T5_CODE_FOG,
                    T5_CODE_FOG2,
                    T5_CODE_SUN_FOG_DIR,
                    T5_CODE_SUN_FOG_COLOR,
                    T5_CODE_SUN_FOG,
                ]
                .map(|i| LEFTOVER_T5_CODE_BASE + i),
                hdr_color_scale: false,
            },
        );
        t5::prepare_sun(&mut steps, requested);
        t5::prepare_exposure(&mut steps, requested);
        sky::prepare(
            &mut steps,
            requested,
            &[(LEFTOVER_T5_CODE_BASE + T5_CODE_SKY_TRANSITION, [0; 4])],
            &[LEFTOVER_T5_CODE_BASE + T5_CODE_SKY_COLOR_MULTIPLIER],
        );
        t5::prepare_defaults(&mut steps, requested);
        t5::prepare_water(&mut steps, requested);
        t5::prepare_environment_defaults(&mut steps, requested);
        steps
    },
    prepare_local_light: |requested| {
        t5_light::prepare(requested)
            .into_iter()
            .map(|operation| {
                std::sync::Arc::new(operation) as std::sync::Arc<dyn LocalLightProducer>
            })
            .collect()
    },
};
