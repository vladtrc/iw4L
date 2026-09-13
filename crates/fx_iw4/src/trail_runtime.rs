#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxTrail {
    pub next_trail_handle: u16,
    pub first_elem_handle: u16,
    pub last_elem_handle: u16,
    pub def_index: i8,
    pub sequence: i8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTrailElem {
    pub origin: [f32; 3],
    pub spawn_dist: f32,
    pub msec_begin: i32,
    pub next_trail_elem_handle: u16,
    pub base_vel_z: i16,
    pub basis: [i8; 6],
    pub sequence: u8,
}

pub const FX_TRAIL_SPLIT_UNIT: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxTrailSplit {
    Hold { leftover: f32 },

    Interpolate { leftover: f32, extra: i32 },
}

#[inline]
pub fn fx_trail_split_window(
    leftover: f32,
    inv_split_time: f32,
    dt_msec: f32,
    inv_split_dist: f32,
    dist_delta: f32,
    inv_split_arc: f32,
    arc_delta: f32,
) -> FxTrailSplit {
    let acc = leftover
        + inv_split_time * dt_msec
        + inv_split_dist * dist_delta
        + inv_split_arc * arc_delta;
    if acc < FX_TRAIL_SPLIT_UNIT {
        FxTrailSplit::Hold { leftover: acc }
    } else {
        let extra = libm::floorf(acc) as i32;
        FxTrailSplit::Interpolate {
            leftover: acc - extra as f32,
            extra,
        }
    }
}

#[inline]
pub fn fx_trail_split_skips_update(
    sequence: u8,
    spawn_range_base: f32,
    spawn_range_amp: f32,
    camera: [f32; 3],
    origin: [f32; 3],
) -> bool {
    if sequence == 0 {
        return false;
    }
    let range = spawn_range_base + spawn_range_amp;
    if range == 0.0 {
        return false;
    }
    let scale = 1.0 + (sequence.trailing_zeros() as f32);
    let r = range * scale;
    crate::sort::fx_sort_dist_to_cam_sq(camera, origin) > r * r
}

#[inline]
pub fn fx_trail_split_interpolant_t(k: f32, leftover: f32, acc: f32) -> f32 {
    (k - leftover) / (acc - leftover)
}

#[inline]
pub fn fx_trail_split_interpolant_msec(prev_msec: i32, msec_now: i32, t: f32) -> i32 {
    (prev_msec as f32 + t * (msec_now - prev_msec) as f32) as i32
}

#[inline]
pub fn fx_trail_split_lerp_quat(begin: [f32; 4], end: [f32; 4], t: f32) -> [f32; 4] {
    crate::quat::fx_quat_normalize([
        begin[0] + t * (end[0] - begin[0]),
        begin[1] + t * (end[1] - begin[1]),
        begin[2] + t * (end[2] - begin[2]),
        begin[3] + t * (end[3] - begin[3]),
    ])
}

#[inline]
pub fn fx_trail_split_lerp_origin(begin: [f32; 3], end: [f32; 3], t: f32) -> [f32; 3] {
    [
        begin[0] + t * (end[0] - begin[0]),
        begin[1] + t * (end[1] - begin[1]),
        begin[2] + t * (end[2] - begin[2]),
    ]
}

#[inline]
pub fn fx_trail_split_lerp_axis(begin: [[f32; 3]; 3], end: [[f32; 3]; 3], t: f32) -> [[f32; 3]; 3] {
    crate::glass::fx_unit_quat_to_axis(fx_trail_split_lerp_quat(
        crate::quat::fx_axis_to_quat(begin),
        crate::quat::fx_axis_to_quat(end),
        t,
    ))
}
