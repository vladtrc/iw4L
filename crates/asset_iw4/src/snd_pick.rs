pub const SND_LCG_MUL: u32 = 0x343fd;

pub const SND_LCG_ADD: u32 = 0x269ec3;

pub const SND_LCG_UNIT_SCALE: f32 = 32768.0;

pub fn snd_advance_lcg(state: &mut u32) {
    *state = state.wrapping_mul(SND_LCG_MUL).wrapping_add(SND_LCG_ADD);
}

pub fn snd_unit_random(state: &mut u32) -> f32 {
    snd_advance_lcg(state);
    ((*state >> 16) & 0x7fff) as f32 / SND_LCG_UNIT_SCALE
}

pub fn lerp_range(min: f32, max: f32, t: f32) -> f32 {
    let lo = if min < max { min } else { max };
    let hi = if min < max { max } else { min };
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    lo + (hi - lo) * t
}

pub fn pick_weighted_variant_index(weights: &[f32], rng: &mut u32, avoid: Option<usize>) -> usize {
    debug_assert!(!weights.is_empty());
    let n = weights.len();
    let total: f32 = weights
        .iter()
        .enumerate()
        .map(|(i, &w)| if avoid == Some(i) && n > 1 { 0.0 } else { w })
        .sum();
    if total <= 1e-8 {
        return ((snd_unit_random(rng) * n as f32) as usize).min(n - 1);
    }
    let mut cursor = snd_unit_random(rng) * total;
    for (index, &weight) in weights.iter().enumerate() {
        let w = if avoid == Some(index) && n > 1 {
            0.0
        } else {
            weight
        };
        cursor -= w;
        if cursor <= 0.0 {
            return index;
        }
    }
    n - 1
}
