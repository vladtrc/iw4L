const NEAREST_PROBE_DIST_SENTINEL: f32 = f32::MAX;

fn dist_sq(point: [f32; 3], origin: [f32; 3]) -> f32 {
    let dx = point[0] - origin[0];
    let dy = point[1] - origin[1];
    let dz = point[2] - origin[2];
    dx * dx + dy * dy + dz * dz
}

pub fn nearest_reflection_probe_skip_0(origins: &[[f32; 3]], point: [f32; 3]) -> u8 {
    if origins.len() <= 1 {
        return 0;
    }
    let mut best = 0u8;
    let mut best_d2 = NEAREST_PROBE_DIST_SENTINEL;
    for (i, origin) in origins.iter().enumerate().skip(1) {
        let d2 = dist_sq(point, *origin);
        if d2 < best_d2 {
            best = i as u8;
            best_d2 = d2;
        }
    }
    best
}

pub fn nearest_reflection_probe_in_cell_list(
    origins: &[[f32; 3]],
    indices: &[u8],
    point: [f32; 3],
) -> u8 {
    let mut best = 0u8;
    let mut best_d2 = NEAREST_PROBE_DIST_SENTINEL;
    for &index in indices {
        let Some(origin) = origins.get(usize::from(index)) else {
            continue;
        };
        let d2 = dist_sq(point, *origin);
        if d2 < best_d2 {
            best = index;
            best_d2 = d2;
        }
    }
    best
}

pub fn lighting_info_reflection_probe_for_point(
    origins: &[[f32; 3]],
    cell_indices: Option<&[u8]>,
    point: [f32; 3],
) -> u8 {
    match cell_indices {
        None => nearest_reflection_probe_skip_0(origins, point),
        Some(indices) => nearest_reflection_probe_in_cell_list(origins, indices, point),
    }
}
