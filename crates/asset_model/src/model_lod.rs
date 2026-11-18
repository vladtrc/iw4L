/// Authored LOD data. Selection semantics belong to the owning family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLodSelector {
    pub family: asset_core::FamilyId,
    pub lod_start: u8,
    pub num_lods: i16,
    pub lod_dist: [f32; 4],
}
impl ModelLodSelector {
    pub const fn lod_dist(self) -> [f32; 4] {
        self.lod_dist
    }
    pub const fn num_lods(self) -> i16 {
        self.num_lods
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodControls {
    pub scale: f32,
    pub bias: f32,
    pub fov_threshold: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodProjectionState {
    pub applied_scale: f32,
    pub distance_scale: f32,
    pub distance_bias: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LodSelectionSample {
    pub offset: [f32; 3],
    pub placement_scale: f32,
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,
    pub world_unit: Option<f32>,
    pub no_cull_out: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelLodPolicy {
    pub rigid: LodControls,
    pub skinned: LodControls,
    pub update_view: fn(&mut LodProjectionState, f32, LodControls),
    pub select: fn(ModelLodSelector, LodSelectionSample, LodProjectionState) -> Option<u8>,
}

pub const REFERENCE_MODEL_LOD_POLICY: ModelLodPolicy = ModelLodPolicy {
    rigid: LodControls {
        scale: 1.0,
        bias: 0.0,
        fov_threshold: 1.0,
    },
    skinned: LodControls {
        scale: 1.0,
        bias: 0.0,
        fov_threshold: 1.0,
    },
    update_view: |_, _, _| {},
    select: reference_lod,
};

fn reference_lod(
    lod: ModelLodSelector,
    sample: LodSelectionSample,
    _: LodProjectionState,
) -> Option<u8> {
    use dpvs_iw4::{xmodel_get_lod_for_dist, xmodel_lod_camera_dist, xmodel_lod_scaled_dists};
    let [x, y, z] = sample.offset;
    let dist = xmodel_lod_camera_dist(x, y, z, sample.placement_scale, sample.world_unit);
    let (mid, last) = match (sample.scale_mid, sample.bias_mid, sample.scale_last) {
        (Some(s), Some(b), Some(l)) => xmodel_lod_scaled_dists(dist, s, b, l),
        (Some(s), Some(b), None) => {
            let (mid, _) = xmodel_lod_scaled_dists(dist, s, b, 0.0);
            (mid, dist)
        }
        (None, None, Some(l)) => {
            let (_, last) = xmodel_lod_scaled_dists(dist, 0.0, 0.0, l);
            (dist, last)
        }
        _ => (dist, dist),
    };
    xmodel_get_lod_for_dist(
        lod.lod_start,
        u8::try_from(lod.num_lods).ok()?,
        lod.lod_dist,
        mid,
        last,
    )
}
