pub const FLOAT64_ONE: f32 = 1.0;

#[must_use]
pub fn xmodel_get_lod_for_dist(
    lod_start: u8,
    num_lods: u8,
    lod_dist: [f32; 4],
    dist_mid: f32,
    dist_last: f32,
) -> Option<u8> {
    let start = i32::from(lod_start);
    let last = i32::from(num_lods) - 1;
    if start < last {
        let mut lod = start;
        while lod < last {
            let i = lod as usize;
            if i < 4 && dist_mid < lod_dist[i] {
                return u8::try_from(lod).ok();
            }
            lod += 1;
        }
    }
    if start <= last {
        let i = last as usize;
        if i < 4 && dist_last < lod_dist[i] {
            return u8::try_from(last).ok();
        }
    }
    None
}

#[must_use]
pub fn xmodel_lod_camera_dist(
    dx: f32,
    dy: f32,
    dz: f32,
    placement_scale: f32,
    world_unit: Option<f32>,
) -> f32 {
    let len = libm::sqrtf(dx * dx + dy * dy + dz * dz);
    if placement_scale == 0.0 {
        return f32::INFINITY;
    }
    match world_unit {
        Some(w) => len * (w / placement_scale),
        None => len / placement_scale,
    }
}

#[must_use]
pub fn xmodel_lod_scaled_dists(
    dist: f32,
    scale_mid: f32,
    bias_mid: f32,
    scale_last: f32,
) -> (f32, f32) {
    (dist * scale_mid + bias_mid, dist * scale_last)
}
