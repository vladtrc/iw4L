pub const LIGHT_GRID_COLORS_STRIDE: u32 = 0xa8;

pub const LIGHT_GRID_SAMPLE_WEIGHT_SCALE: f64 = 255.0;

pub const LIGHT_GRID_ROUND_BIAS: f64 = 0.5;

pub const LIGHT_GRID_FIXED_WEIGHT_SCALE: f64 = 256.0;

pub const LIGHT_GRID_FIXED_WEIGHT_SUM: u16 = 0x100;

#[inline]
pub const fn light_grid_colors_byte_offset(index: u32) -> u32 {
    index.wrapping_mul(LIGHT_GRID_COLORS_STRIDE)
}

#[inline]
pub const fn light_grid_default_colors_index(color_count: u32) -> Option<u32> {
    color_count.checked_sub(1)
}

#[inline]
pub fn light_grid_encode_sample_weight(weight: f32) -> u8 {
    let v = libm::floorf(
        weight * (LIGHT_GRID_SAMPLE_WEIGHT_SCALE as f32) + (LIGHT_GRID_ROUND_BIAS as f32),
    );
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

pub fn light_grid_fixed_point_blend_weights(
    weights: &[f32],
    inv_total: f32,
    out: &mut [u16],
) -> Option<()> {
    let count = weights.len();
    if count == 0 || count > out.len() {
        return None;
    }
    let scale = inv_total * (LIGHT_GRID_FIXED_WEIGHT_SCALE as f32);
    let mut sum: i32 = 0;
    let mut max_i = 0usize;
    let mut max_v: u16 = 0;
    for (i, &w) in weights.iter().enumerate() {
        let rounded = libm::floorf(w * scale + (LIGHT_GRID_ROUND_BIAS as f32));
        let v = if rounded <= 0.0 {
            0u16
        } else if rounded >= 65535.0 {
            65535
        } else {
            rounded as u16
        };
        out[i] = v;
        sum = sum.wrapping_add(i32::from(v));
        if v >= max_v {
            max_v = v;
            max_i = i;
        }
    }
    let adjust = i32::from(LIGHT_GRID_FIXED_WEIGHT_SUM).wrapping_sub(sum);
    out[max_i] = (i32::from(out[max_i]).wrapping_add(adjust)) as u16;
    Some(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightGridAtPointPath {
    SetFromIndex,

    BlendAndSet,

    MissingUseIndex,

    SetDefault,
}

pub const LIGHT_GRID_ATPOINT_EMPTY_PRIMARY: u8 = 1;

#[inline]
pub const fn light_grid_atapoint_return_primary(
    path: LightGridAtPointPath,
    picked_primary: u8,
) -> u8 {
    match path {
        LightGridAtPointPath::SetFromIndex | LightGridAtPointPath::BlendAndSet => picked_primary,
        LightGridAtPointPath::MissingUseIndex => LIGHT_GRID_ATPOINT_EMPTY_PRIMARY,
        LightGridAtPointPath::SetDefault => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridAtPointEmptyGate {
    pub prefer_default_when_missing: bool,

    pub fallback_colors_index: u32,

    pub show_missing_light_grid: bool,

    pub color_count: u32,
}

pub fn light_grid_atapoint_select_path(
    sample_count: u32,
    empty: LightGridAtPointEmptyGate,
) -> LightGridAtPointPath {
    if sample_count != 0 {
        if sample_count == 1 {
            LightGridAtPointPath::SetFromIndex
        } else {
            LightGridAtPointPath::BlendAndSet
        }
    } else {
        let in_range = empty.fallback_colors_index < empty.color_count;
        let skip_default = !(empty.prefer_default_when_missing
            && empty.fallback_colors_index == 0
            && empty.show_missing_light_grid)
            && in_range;
        if skip_default {
            LightGridAtPointPath::MissingUseIndex
        } else {
            LightGridAtPointPath::SetDefault
        }
    }
}
