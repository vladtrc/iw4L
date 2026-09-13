#[inline]
pub fn fx_sample_life_span_msec(base: i32, amplitude: i32, rand16: u16) -> i32 {
    let span = amplitude.wrapping_add(1);
    base.wrapping_add(((rand16 as i32).wrapping_mul(span)) >> 16)
}

#[inline]
pub fn fx_trail_elem_keep(msec_now: i32, msec_begin: i32, life_ms: i32) -> bool {
    msec_now < msec_begin.wrapping_add(life_ms)
}

#[inline]
pub fn fx_trail_elem_norm_ages(
    prev_msec: i32,
    msec_now: i32,
    msec_begin: i32,
    life_ms: i32,
) -> (f32, f32) {
    let prev = if prev_msec < msec_begin {
        msec_begin
    } else {
        prev_msec
    };
    let recip = if life_ms == 0 {
        0.0
    } else {
        1.0 / life_ms as f32
    };
    (
        (prev.wrapping_sub(msec_begin) as f32) * recip,
        (msec_now.wrapping_sub(msec_begin) as f32) * recip,
    )
}

#[inline]
pub fn fx_trail_elem_base_vel_z_pack(vel_z: f32) -> i16 {
    let t = vel_z as i32;
    if t < -0x8000 {
        -0x8000
    } else if t > 0x7fff {
        0x7fff
    } else {
        t as i16
    }
}

#[inline]
pub fn fx_life_span_range_from_bytes(bytes: &[u8; 8]) -> (i32, i32) {
    let base = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let amplitude = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (base, amplitude)
}
