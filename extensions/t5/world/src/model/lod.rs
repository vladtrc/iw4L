use asset_model::{
    LodControls, LodProjectionState, LodSelectionSample, ModelLodPolicy, ModelLodSelector,
};

pub const POLICY: ModelLodPolicy = ModelLodPolicy {
    rigid: LodControls {
        scale: 1.0,
        bias: 0.0,
        fov_threshold: 2.4,
    },
    skinned: LodControls {
        scale: 1.0,
        bias: 0.0,
        fov_threshold: 2.4,
    },
    update_view,
    select,
};
fn update_view(state: &mut LodProjectionState, tan: f32, controls: LodControls) {
    let target = tan * 2.118_673_1;
    if state.applied_scale <= target || controls.fov_threshold < state.applied_scale / target {
        state.applied_scale = target;
    }
    state.distance_scale = state.applied_scale * controls.scale;
    state.distance_bias = state.applied_scale * controls.bias;
}
fn select(
    lod: ModelLodSelector,
    sample: LodSelectionSample,
    state: LodProjectionState,
) -> Option<u8> {
    let [dx, dy, dz] = sample.offset;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let inv = 1.0 / sample.placement_scale.max(1.0);
    let ramped = inv * (dist * state.distance_scale + state.distance_bias);
    let base = inv * dist;
    let mut selected = first_lod_under(lod, ramped);
    if selected.is_none() && ramped != base {
        selected = first_lod_under(lod, base);
    }
    match selected {
        Some(lod) => Some(lod),
        None if sample.no_cull_out => Some((i32::from(lod.num_lods) - 2).clamp(0, 3) as u8),
        None => None,
    }
}

fn first_lod_under(lod: ModelLodSelector, dist: f32) -> Option<u8> {
    let count = i32::from(lod.num_lods).clamp(0, 4) as usize;
    (0..count)
        .find(|&index| dist < lod.lod_dist[index])
        .map(|index| index as u8)
}
