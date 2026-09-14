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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parks_at_the_last_struct_after_the_path() {
        let start = [0.0, 0.0, 0.0];
        let legs = [([10.0, 0.0, 0.0], 1_000), ([10.0, 20.0, 0.0], 2_000)];
        assert_eq!(origin_at(0, start, &legs), start);
        let mid = origin_at(500, start, &legs);
        assert!((mid[0] - 5.0).abs() < 0.01);
        assert_eq!(origin_at(3_000, start, &legs), [10.0, 20.0, 0.0]);
        assert_eq!(origin_at(9_000, start, &legs), [10.0, 20.0, 0.0]);
    }

    #[test]
    fn zero_time_leg_snaps() {
        let start = [0.0, 0.0, 0.0];
        let legs = [([4.0, 0.0, 0.0], 0)];
        assert_eq!(origin_at(0, start, &legs), [4.0, 0.0, 0.0]);
    }
}
