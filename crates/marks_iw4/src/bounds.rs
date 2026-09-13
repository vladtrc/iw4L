use crate::R_MARK_FRAGMENTS_WORLD_SURF_STACK;

#[inline]
pub fn fx_mark_sphere_hits_bounds(
    origin: [f32; 3],
    radius_sq: f32,
    mid: [f32; 3],
    half: [f32; 3],
) -> bool {
    let mut acc = 0.0f32;
    let mut i = 0;
    while i < 3 {
        let mut a = (origin[i] - mid[i]).abs() - half[i];
        if a < 0.0 {
            a = 0.0;
        }
        acc += a * a;
        i += 1;
    }
    acc <= radius_sq
}

pub fn fx_mark_model_local_box(
    mark_origin: [f32; 3],
    mark_radius: f32,
    model_origin: [f32; 3],
    axis: [[f32; 3]; 3],
    scale: f32,
) -> Option<([f32; 3], [f32; 3])> {
    if !(scale > 0.0) || !scale.is_finite() || !mark_radius.is_finite() {
        return None;
    }
    let inv = 1.0 / scale;
    let dx = mark_origin[0] - model_origin[0];
    let dy = mark_origin[1] - model_origin[1];
    let dz = mark_origin[2] - model_origin[2];
    let mut local = [0.0f32; 3];
    let mut i = 0;
    while i < 3 {
        local[i] = (axis[i][0] * dx + axis[i][1] * dy + axis[i][2] * dz) * inv;
        i += 1;
    }
    let r = mark_radius * inv;
    Some((
        [local[0] - r, local[1] - r, local[2] - r],
        [local[0] + r, local[1] + r, local[2] + r],
    ))
}

pub fn fx_mark_tri_aabb_hits_box(
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
) -> bool {
    let mut i = 0;
    while i < 3 {
        let tmin = v0[i].min(v1[i]).min(v2[i]);
        let tmax = v0[i].max(v1[i]).max(v2[i]);
        if tmax < mins[i] || tmin > maxs[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkWorldBoundsHits {
    pub hit: u32,

    pub raw: u32,

    pub capped: bool,
}

pub fn fx_mark_count_world_bounds_hits(
    origin: [f32; 3],
    radius_sq: f32,
    mids: &[[f32; 3]],
    halves: &[[f32; 3]],
) -> MarkWorldBoundsHits {
    let n = core::cmp::min(mids.len(), halves.len());
    let mut raw = 0u32;
    let mut i = 0;
    while i < n {
        if fx_mark_sphere_hits_bounds(origin, radius_sq, mids[i], halves[i]) {
            raw = raw.saturating_add(1);
        }
        i += 1;
    }
    let cap = R_MARK_FRAGMENTS_WORLD_SURF_STACK;
    let capped = raw > cap;
    MarkWorldBoundsHits {
        hit: if capped { cap } else { raw },
        raw,
        capped,
    }
}

#[inline]
pub fn fx_mark_sorted_bit_surf(sorted_surf_index: &[u16], bit: usize) -> Option<usize> {
    sorted_surf_index.get(bit).copied().map(usize::from)
}

#[inline]
pub fn fx_mark_aabb_overlaps_bounds(
    origin: [f32; 3],
    radius: f32,
    mid: [f32; 3],
    half: [f32; 3],
) -> bool {
    let mut i = 0;
    while i < 3 {
        let mark_min = origin[i] - radius;
        let mark_max = origin[i] + radius;
        let surf_min = mid[i] - half[i];
        let surf_max = mid[i] + half[i];
        if mark_max < surf_min || mark_min > surf_max {
            return false;
        }
        i += 1;
    }
    true
}
