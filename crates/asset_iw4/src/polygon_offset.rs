pub const GFXS1_POLYGON_OFFSET_MASK: u32 = 0x30;

pub const GFXS1_POLYGON_OFFSET_SHIFT: u32 = 4;

pub const GFXS1_POLYGON_OFFSET_SHADOWMAP_LEVEL: u32 = 3;

pub const R_POLYGON_OFFSET_SCALE_DEFAULT: f32 = -1.0;

pub const R_POLYGON_OFFSET_SCALE_MIN: f32 = -4.0;

pub const R_POLYGON_OFFSET_BIAS_DEFAULT: f32 = -1.0;

pub const R_POLYGON_OFFSET_BIAS_MIN: f32 = -16.0;

pub const R_POLYGON_OFFSET_MAX: f32 = 0.0;

pub const SM_POLYGON_OFFSET_SCALE_DEFAULT: f32 = 2.0;

pub const SM_POLYGON_OFFSET_SCALE_MAX: f32 = 8.0;

pub const SM_POLYGON_OFFSET_BIAS_DEFAULT: f32 = 0.125;

pub const SM_POLYGON_OFFSET_BIAS_MAX: f32 = 32.0;

pub const POLYGON_OFFSET_BIAS_TO_D3D: f64 = 1.525_878_906_25e-5;

#[inline]
pub const fn polygon_offset_level(word1: u32) -> u32 {
    (word1 & GFXS1_POLYGON_OFFSET_MASK) >> GFXS1_POLYGON_OFFSET_SHIFT
}

#[inline]
pub fn polygon_offset_d3d(
    level: u32,
    r_scale: f32,
    r_bias: f32,
    sm_scale: f32,
    sm_bias: f32,
) -> (f32, f32) {
    if level == 0 {
        return (0.0, 0.0);
    }
    if level == GFXS1_POLYGON_OFFSET_SHADOWMAP_LEVEL {
        let bias = (f64::from(sm_bias) * POLYGON_OFFSET_BIAS_TO_D3D) as f32;
        return (sm_scale, bias);
    }
    let n = level as f32;
    let scale = n * r_scale;
    let bias = (f64::from(n) * f64::from(r_bias) * POLYGON_OFFSET_BIAS_TO_D3D) as f32;
    (scale, bias)
}

#[inline]
pub fn polygon_offset_d3d_defaults(level: u32) -> (f32, f32) {
    polygon_offset_d3d(
        level,
        R_POLYGON_OFFSET_SCALE_DEFAULT,
        R_POLYGON_OFFSET_BIAS_DEFAULT,
        SM_POLYGON_OFFSET_SCALE_DEFAULT,
        SM_POLYGON_OFFSET_BIAS_DEFAULT,
    )
}

#[inline]
pub fn d3d_depth_bias_to_wgpu_constant(bias: f32) -> i32 {
    const N: f32 = 1.0 / 8_388_608.0;
    let scaled = bias / N;

    if scaled >= 0.0 {
        (scaled + 0.5) as i32
    } else {
        (scaled - 0.5) as i32
    }
}

#[inline]
pub fn polygon_offset_wgpu_defaults(level: u32) -> (f32, i32) {
    let (scale, bias) = polygon_offset_d3d_defaults(level);
    (scale, d3d_depth_bias_to_wgpu_constant(bias))
}
