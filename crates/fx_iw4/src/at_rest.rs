pub const FX_AT_REST_SCALE: f64 = 255.0;

pub const FX_AT_REST_BIAS: f64 = 0.25;

pub const FX_RECIP_255_AT_REST: f64 = 1.0 / 255.0;

pub const FX_ON_GROUND_NORMAL_Z: f32 = 0.7;

#[inline]
pub fn fx_get_at_rest_fraction(msec_now: f32, msec_begin: f32, recip_life_ms: f32) -> f32 {
    libm::floorf(
        (msec_now - msec_begin) * (FX_AT_REST_SCALE as f32) * recip_life_ms
            - (FX_AT_REST_BIAS as f32),
    )
}

#[inline]
pub fn fx_msec_for_sampling_axis(at_rest_fraction: u8, msec_life_span: f32) -> f32 {
    (at_rest_fraction as f32) * msec_life_span * (FX_RECIP_255_AT_REST as f32)
}
