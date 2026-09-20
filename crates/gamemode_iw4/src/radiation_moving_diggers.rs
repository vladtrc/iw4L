pub const TARGETNAME: &str = "moving_digger";

pub fn is_moving_digger(targetname: &str) -> bool {
    targetname == TARGETNAME
}

pub fn origin_at(elapsed_ms: u32, start: [f32; 3], legs: &[([f32; 3], u32)]) -> [f32; 3] {
    let mut from = start;
    let mut remaining = elapsed_ms;
    for &(to, duration_ms) in legs {
        if duration_ms == 0 {
            from = to;
            continue;
        }
        if remaining < duration_ms {
            let t = remaining as f32 / duration_ms as f32;
            return core::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
        }
        remaining -= duration_ms;
        from = to;
    }
    from
}
