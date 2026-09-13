pub const FX_LIGHTING_FRAC_CHANNEL_MAP: [usize; 3] = [2, 1, 0];

#[inline]
pub const fn fx_effect_def_needs_lighting_sample(flags: i32) -> bool {
    (flags & 1) != 0
}

#[inline]
pub const fn fx_apply_lighting_frac_channel(color: u8, sample: u8, lighting_frac: u8) -> u8 {
    let frac = lighting_frac as i32;
    let c = color as i32;
    let s = sample as i32;
    let lit = ((s * 2 - 0xff) * frac) / 0xff + 0xff;
    let mut out = (lit * c) / 0xff;
    if out > 0xff {
        out = 0xff;
    } else if out < 0 {
        out = 0;
    }
    out as u8
}

#[inline]
pub const fn fx_elem_uses_lighting_frac(lighting_frac: u8) -> bool {
    lighting_frac != 0
}

#[inline]
pub fn fx_apply_lighting_frac_bgra(
    mut color: [u8; 4],
    sample_rgb: [u8; 3],
    lighting_frac: u8,
) -> [u8; 4] {
    if lighting_frac == 0 {
        return color;
    }
    for i in 0..3 {
        let dest = FX_LIGHTING_FRAC_CHANNEL_MAP[i];
        color[dest] = fx_apply_lighting_frac_channel(color[dest], sample_rgb[i], lighting_frac);
    }
    color
}
