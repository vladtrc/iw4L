use asset_material::material_bindings::*;
mod iw5;

pub const POLICY: MaterialBindingFamily = MaterialBindingFamily {
    prepare_frame: |requested| {
        let mut steps = Vec::new();
        iw5::prepare(&mut steps, requested);
        steps
    },
    prepare_local_light: |_| Vec::new(),
};
