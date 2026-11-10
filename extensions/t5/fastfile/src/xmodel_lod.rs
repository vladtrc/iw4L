pub const XMODEL_LOD_INFO_OFF: usize = 0x28;

pub const XMODEL_LOD_INFO_STRIDE: usize = 0x20;

pub const XMODEL_NUM_LODS_OFF: usize = 0xd8;

pub const MAX_LODS: usize = 4;

pub const PLACEMENT_SCALE_FLOOR: f32 = 1.0;

pub const FOV_SCALE_CONST: f32 = 2.118_673_1;

pub const R_LOD_SCALE_RIGID_DEFAULT: f32 = 1.0;

pub const R_LOD_BIAS_RIGID_DEFAULT: f32 = 0.0;

pub const R_LOD_SCALE_SKINNED_DEFAULT: f32 = 1.0;

pub const R_LOD_BIAS_SKINNED_DEFAULT: f32 = 0.0;

pub const R_FOV_SCALE_THRESHOLD_DEFAULT: f32 = 2.4;

#[must_use]
pub fn xmodel_get_lod_for_dist(
    num_lods: i16,
    lod_dist: [f32; MAX_LODS],
    dist: f32,
    base_dist: f32,
    no_lod_cull_out: bool,
) -> Option<u8> {
    let mut lod = first_lod_under(num_lods, lod_dist, dist);
    if lod.is_none() && dist != base_dist {
        lod = first_lod_under(num_lods, lod_dist, base_dist);
    }
    match lod {
        Some(lod) => Some(lod),

        None if no_lod_cull_out => Some((i32::from(num_lods) - 2).clamp(0, 3) as u8),
        None => None,
    }
}

fn first_lod_under(num_lods: i16, lod_dist: [f32; MAX_LODS], dist: f32) -> Option<u8> {
    let n = i32::from(num_lods).clamp(0, MAX_LODS as i32) as usize;
    (0..n)
        .find(|&lod| dist < lod_dist[lod])
        .map(|lod| lod as u8)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodParmsAxis {
    pub applied_inv_scale: f32,

    pub scale: f32,

    pub bias: f32,
}

impl LodParmsAxis {
    pub fn update(
        &mut self,
        tan_half_fov_y: f32,
        scale_dvar: f32,
        bias_dvar: f32,
        fov_scale_threshold: f32,
    ) {
        let target = tan_half_fov_y * FOV_SCALE_CONST;
        if self.applied_inv_scale <= target || fov_scale_threshold < self.applied_inv_scale / target
        {
            self.applied_inv_scale = target;
        }
        self.scale = self.applied_inv_scale * scale_dvar;
        self.bias = self.applied_inv_scale * bias_dvar;
    }

    #[must_use]
    pub fn smodel_call_dists(self, dist: f32, placement_scale: f32) -> (f32, f32) {
        let inv = PLACEMENT_SCALE_FLOOR / placement_scale.max(PLACEMENT_SCALE_FLOOR);
        (inv * (dist * self.scale + self.bias), inv * dist)
    }
}
