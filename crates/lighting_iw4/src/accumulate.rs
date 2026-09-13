use crate::atapoint::{
    LIGHT_GRID_COLORS_STRIDE, LIGHT_GRID_FIXED_WEIGHT_SCALE, LIGHT_GRID_ROUND_BIAS,
};

pub const LIGHT_GRID_COLORS_BYTE_COUNT: usize = LIGHT_GRID_COLORS_STRIDE as usize;

pub const LIGHT_GRID_COLORS_ACCUM_COUNT: usize = LIGHT_GRID_COLORS_BYTE_COUNT;

pub const LIGHT_GRID_TWEAK_ACCUM_CLAMP: u32 = 0xff00;

pub const LIGHT_GRID_PACK_BIAS: u16 = 0x7f;

pub fn light_grid_expand_colors(colors: &[u8], weight: u16, out: &mut [u16]) {
    debug_assert_eq!(colors.len(), LIGHT_GRID_COLORS_BYTE_COUNT);
    debug_assert_eq!(out.len(), LIGHT_GRID_COLORS_ACCUM_COUNT);
    for i in 0..LIGHT_GRID_COLORS_BYTE_COUNT {
        out[i] = (u16::from(colors[i])).wrapping_mul(weight);
    }
}

pub fn light_grid_add_colors(colors: &[u8], weight: u16, out: &mut [u16]) {
    debug_assert_eq!(colors.len(), LIGHT_GRID_COLORS_BYTE_COUNT);
    debug_assert_eq!(out.len(), LIGHT_GRID_COLORS_ACCUM_COUNT);
    for i in 0..LIGHT_GRID_COLORS_BYTE_COUNT {
        out[i] = out[i].wrapping_add((u16::from(colors[i])).wrapping_mul(weight));
    }
}

pub fn light_grid_pack_colors(accum: &[u16], out: &mut [u8]) {
    debug_assert_eq!(accum.len(), LIGHT_GRID_COLORS_ACCUM_COUNT);
    debug_assert_eq!(out.len(), LIGHT_GRID_COLORS_BYTE_COUNT);
    for i in 0..LIGHT_GRID_COLORS_BYTE_COUNT {
        out[i] = (accum[i].wrapping_add(LIGHT_GRID_PACK_BIAS) >> 8) as u8;
    }
}

pub fn light_grid_apply_intensity_tweaks(accum: &mut [u16], intensity: f32) {
    debug_assert_eq!(accum.len(), LIGHT_GRID_COLORS_ACCUM_COUNT);
    let fixed = light_grid_tweak_fixed_from_float(intensity);
    for slot in accum.iter_mut() {
        let mut v = (u32::from(*slot).wrapping_mul(u32::from(fixed))) >> 8;
        if v > LIGHT_GRID_TWEAK_ACCUM_CLAMP {
            v = LIGHT_GRID_TWEAK_ACCUM_CLAMP;
        }
        *slot = v as u16;
    }
}

#[inline]
pub fn light_grid_tweak_fixed_from_float(value: f32) -> u16 {
    let v = libm::floorf(
        value * (LIGHT_GRID_FIXED_WEIGHT_SCALE as f32) + (LIGHT_GRID_ROUND_BIAS as f32),
    );
    if v <= 0.0 {
        0
    } else if v >= 65535.0 {
        65535
    } else {
        v as u16
    }
}

pub fn light_grid_apply_contrast_tweaks(accum: &mut [u16], contrast_fixed: u16) {
    debug_assert_eq!(accum.len(), LIGHT_GRID_COLORS_ACCUM_COUNT);
    let mut sum: u32 = 0;
    for &slot in accum.iter() {
        sum = sum.wrapping_add(u32::from(slot));
    }
    let mean = sum / (LIGHT_GRID_COLORS_BYTE_COUNT as u32);
    let cf = u32::from(contrast_fixed);
    for slot in accum.iter_mut() {
        let scaled = (u32::from(*slot).wrapping_mul(cf)) >> 8;
        let mean_scaled = (mean.wrapping_mul(cf)) >> 8;
        let mut v = scaled.wrapping_add(mean.wrapping_sub(mean_scaled)) as i32;
        if v > LIGHT_GRID_TWEAK_ACCUM_CLAMP as i32 {
            v = LIGHT_GRID_TWEAK_ACCUM_CLAMP as i32;
        } else if v < 0 {
            v = 0;
        }
        *slot = v as u16;
    }
}

pub fn light_grid_accumulate_colors(colors: &[&[u8]], weights: &[u16], out: &mut [u8]) -> bool {
    if colors.is_empty()
        || colors.len() != weights.len()
        || out.len() != LIGHT_GRID_COLORS_BYTE_COUNT
    {
        return false;
    }
    for row in colors {
        if row.len() != LIGHT_GRID_COLORS_BYTE_COUNT {
            return false;
        }
    }

    let mut accum = [0u16; LIGHT_GRID_COLORS_ACCUM_COUNT];
    light_grid_expand_colors(colors[0], weights[0], &mut accum);
    for i in 1..colors.len() {
        light_grid_add_colors(colors[i], weights[i], &mut accum);
    }
    light_grid_pack_colors(&accum, out);
    true
}

pub const LIGHT_GRID_COMPRESS_R_OFFSETS: [usize; 8] =
    [0x00, 0x09, 0x24, 0x2d, 0x78, 0x81, 0x9c, 0xa5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightGridCompressedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,

    pub weight: u8,
}

impl LightGridCompressedColor {
    #[inline]
    pub const fn to_u32(self) -> u32 {
        ((self.weight as u32) << 24)
            | ((self.b as u32) << 16)
            | ((self.g as u32) << 8)
            | (self.r as u32)
    }
}

pub fn light_grid_compress_colors(colors: &[u8], weight: u8) -> Option<LightGridCompressedColor> {
    if colors.len() < LIGHT_GRID_COLORS_BYTE_COUNT {
        return None;
    }
    let mut sum_r: u32 = 4;
    let mut sum_g: u32 = 4;
    let mut sum_b: u32 = 4;
    for &off in &LIGHT_GRID_COMPRESS_R_OFFSETS {
        sum_r = sum_r.wrapping_add(u32::from(colors[off]));
        sum_g = sum_g.wrapping_add(u32::from(colors[off + 1]));
        sum_b = sum_b.wrapping_add(u32::from(colors[off + 2]));
    }
    Some(LightGridCompressedColor {
        r: (sum_r >> 3) as u8,
        g: (sum_g >> 3) as u8,
        b: (sum_b >> 3) as u8,
        weight,
    })
}
