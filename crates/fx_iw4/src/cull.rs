use crate::flags::fx_elem_spawn_frustum_cull;

pub const FX_ELEM_FLAG_CULL_DRAW_5_PLANES: i32 = 0x400;

#[inline]
pub fn fx_cull_elem_for_spawn_allows(
    spawn_range_base: f32,
    spawn_range_amplitude: f32,
    camera_distance_inches: f32,
    flags: i32,
    sphere_outside_frustum: bool,
) -> bool {
    if spawn_range_amplitude != 0.0 {
        let dist = camera_distance_inches - spawn_range_base;
        if dist < 0.0 || spawn_range_amplitude < dist {
            return false;
        }
    }
    if fx_elem_spawn_frustum_cull(flags) && sphere_outside_frustum {
        return false;
    }
    true
}

#[inline]
pub const fn fx_cull_cloud_plane_count(elem_flags: i32, view_plane_count: u32) -> u32 {
    if (elem_flags & FX_ELEM_FLAG_CULL_DRAW_5_PLANES) != 0 {
        5
    } else {
        view_plane_count
    }
}

#[inline]
pub fn fx_cull_cloud_radius(size0: f32, size1: f32, scale: f32) -> f32 {
    let mut r = size1;
    if r < size0 {
        r = size0;
    }
    r + scale
}

#[inline]
pub fn fx_cull_sphere(planes: &[[f32; 4]], plane_count: u32, pos: [f32; 3], radius: f32) -> bool {
    if plane_count == 0 {
        return false;
    }
    let neg_r = -radius;
    let n = plane_count as usize;
    let mut i = 0usize;
    while i < n && i < planes.len() {
        let p = planes[i];
        let f = p[0] * pos[0] + p[1] * pos[1] + p[2] * pos[2] + p[3];
        if f < neg_r {
            return true;
        }
        i += 1;
    }
    false
}

#[inline]
pub fn fx_cull_cloud(
    cull_elem_draw: bool,
    planes: &[[f32; 4]],
    view_plane_count: u32,
    elem_flags: i32,
    pos: [f32; 3],
    size0: f32,
    size1: f32,
    scale: f32,
) -> bool {
    if !cull_elem_draw {
        return false;
    }
    let count = fx_cull_cloud_plane_count(elem_flags, view_plane_count);
    fx_cull_sphere(
        planes,
        count,
        pos,
        fx_cull_cloud_radius(size0, size1, scale),
    )
}

#[inline]
pub fn fx_cull_elem_light(
    cull_elem_draw: bool,
    planes: &[[f32; 4]],
    view_plane_count: u32,
    elem_flags: i32,
    pos: [f32; 3],
    size0: f32,
) -> bool {
    if !cull_elem_draw {
        return false;
    }
    let count = fx_cull_cloud_plane_count(elem_flags, view_plane_count);
    fx_cull_sphere(planes, count, pos, size0)
}

#[inline]
pub fn fx_elem_light_color_bgr(color_rgba: [u8; 4], scale: f32) -> [f32; 3] {
    let s = scale * (crate::FX_RECIP_255 as f32);
    [
        color_rgba[2] as f32 * s,
        color_rgba[1] as f32 * s,
        color_rgba[0] as f32 * s,
    ]
}
