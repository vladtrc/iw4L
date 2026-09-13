pub const FX_RECIP_255: f64 = 0.003_921_568_859_368_563;

pub const FX_ROT_TIME_EASE_LIMIT_MS: f32 = 300.0;

pub const FX_ROT_TIME_MAX_LEAD_MS: f64 = 150.0;

pub const FX_ROT_TIME_EASE_RECIP: f64 = 0.001_666_666_707_023_978_2;

#[inline]
pub fn fx_clamp_elem_rotation_time(draw_time_ms: f32, sim_time_ms: f32, def_byte: u8) -> f32 {
    let draw = draw_time_ms as f64;
    let sim = sim_time_ms as f64;
    let reference = (def_byte as f64) * sim * FX_RECIP_255;
    let lead = draw - reference;
    if lead <= 0.0 {
        return draw_time_ms;
    }
    if lead > FX_ROT_TIME_EASE_LIMIT_MS as f64 {
        return (reference + FX_ROT_TIME_MAX_LEAD_MS) as f32;
    }
    (draw - lead * lead * FX_ROT_TIME_EASE_RECIP) as f32
}
