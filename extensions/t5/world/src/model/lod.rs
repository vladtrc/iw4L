use asset_model::{
    LodControls, LodProjectionState, LodSelectionSample, ModelLodPolicy, ModelLodSelector,
};
use ext_t5_fastfile::xmodel_lod as native;

pub const POLICY: ModelLodPolicy = ModelLodPolicy {
    rigid: LodControls {
        scale: native::R_LOD_SCALE_RIGID_DEFAULT,
        bias: native::R_LOD_BIAS_RIGID_DEFAULT,
        fov_threshold: native::R_FOV_SCALE_THRESHOLD_DEFAULT,
    },
    skinned: LodControls {
        scale: native::R_LOD_SCALE_SKINNED_DEFAULT,
        bias: native::R_LOD_BIAS_SKINNED_DEFAULT,
        fov_threshold: native::R_FOV_SCALE_THRESHOLD_DEFAULT,
    },
    update_view,
    select,
};
fn axis(state: LodProjectionState) -> native::LodParmsAxis {
    native::LodParmsAxis {
        applied_inv_scale: state.applied_scale,
        scale: state.distance_scale,
        bias: state.distance_bias,
    }
}
fn update_view(state: &mut LodProjectionState, tan: f32, controls: LodControls) {
    let mut value = axis(*state);
    value.update(tan, controls.scale, controls.bias, controls.fov_threshold);
    *state = LodProjectionState {
        applied_scale: value.applied_inv_scale,
        distance_scale: value.scale,
        distance_bias: value.bias,
    };
}
fn select(
    lod: ModelLodSelector,
    sample: LodSelectionSample,
    state: LodProjectionState,
) -> Option<u8> {
    let [dx, dy, dz] = sample.offset;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let (ramped, base) = axis(state).smodel_call_dists(dist, sample.placement_scale);
    native::xmodel_get_lod_for_dist(lod.num_lods, lod.lod_dist, ramped, base, sample.no_cull_out)
}
