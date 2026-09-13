use crate::post_light::FxPostLight;

pub const FX_LASER_POST_LIGHT_MIN_SPAN: f32 = 4.0;

pub const FX_LASER_POST_LIGHT_HALF: f32 = 0.5;

pub const FX_LASER_POST_LIGHT_PAD: f32 = 2.0;

pub const FX_LASER_RADIUS_DIST_SCALE: f32 = 0.01;

pub const FX_LASER_RADIUS_DIST_BIAS: f32 = 1.0;

pub const FX_LASER_POST_LIGHT_COLOR: u32 = 0xffff_ffff;

pub const FX_LASER_POST_LIGHT_MATERIAL: &str = "gfx_laser_light";

pub const FX_LASER_SURF_EXTRA_END: u32 = 0x200_4000;

pub const FX_LASER_TRACE_BOUNDS: [f32; 3] = [0.0; 3];

pub const FX_LASER_TAG: &str = "tag_laser";

#[inline]
pub const fn fx_laser_brush_trace_allows(startsolid: bool) -> bool {
    !startsolid
}

#[inline]
pub const fn fx_laser_post_light_allows(light_dvar: bool, param_4: i32) -> bool {
    light_dvar && param_4 == 1
}

#[inline]
pub fn fx_laser_post_light_end_t(
    fraction: f32,
    range: f32,
    end_nudge: f32,
    extra: f32,
    surf: u32,
) -> f32 {
    let mut t = fraction * range - end_nudge;
    if (surf & FX_LASER_SURF_EXTRA_END) != 0 {
        t += extra;
    }
    t
}

#[inline]
pub fn fx_laser_post_light_span(begin_pad: f32, end_t: f32) -> (f32, f32) {
    if end_t - begin_pad < FX_LASER_POST_LIGHT_MIN_SPAN {
        let mid = (begin_pad + end_t) * FX_LASER_POST_LIGHT_HALF;
        (mid - FX_LASER_POST_LIGHT_PAD, mid + FX_LASER_POST_LIGHT_PAD)
    } else {
        (begin_pad, end_t)
    }
}

#[inline]
pub fn fx_laser_radius_from_dist(width_dvar: f32, dist: f32) -> f32 {
    width_dvar * dist * FX_LASER_RADIUS_DIST_SCALE + FX_LASER_RADIUS_DIST_BIAS
}

#[inline]
pub fn fx_laser_point_on_ray(origin: [f32; 3], dir: [f32; 3], t: f32) -> [f32; 3] {
    [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ]
}

pub fn fx_laser_post_light(
    hit: bool,
    light_dvar: bool,
    param_4: i32,
    origin: [f32; 3],
    dir: [f32; 3],
    begin_t: f32,
    end_t: f32,
    radius: f32,
) -> Option<FxPostLight> {
    if !hit || !fx_laser_post_light_allows(light_dvar, param_4) {
        return None;
    }
    let (begin_t, end_t) = fx_laser_post_light_span(begin_t, end_t);
    Some(FxPostLight {
        begin: fx_laser_point_on_ray(origin, dir, begin_t),
        end: fx_laser_point_on_ray(origin, dir, end_t),
        radius,
        color_packed: FX_LASER_POST_LIGHT_COLOR,
        material_name: FX_LASER_POST_LIGHT_MATERIAL,
    })
}

pub fn fx_laser_from_tag_orientation(
    origin: [f32; 3],
    forward: [f32; 3],
    view: [f32; 3],
    range: f32,
    begin_pad: f32,
    end_nudge: f32,
    light_dvar: bool,
    param_4: i32,
    width_dvar: f32,
) -> Option<FxPostLight> {
    let end = fx_laser_point_on_ray(origin, forward, range);
    let view_dist = crate::vec::fx_vec3_distance(end, view);
    fx_laser_from_brush_trace(
        false, 1.0, 0, origin, forward, range, begin_pad, end_nudge, 0.0, light_dvar, param_4,
        width_dvar, view_dist,
    )
}

pub fn fx_laser_from_brush_trace(
    startsolid: bool,
    fraction: f32,
    contents: u32,
    origin: [f32; 3],
    dir: [f32; 3],
    range: f32,
    begin_pad: f32,
    end_nudge: f32,
    extra: f32,
    light_dvar: bool,
    param_4: i32,
    width_dvar: f32,
    view_dist: f32,
) -> Option<FxPostLight> {
    if !fx_laser_brush_trace_allows(startsolid) {
        return None;
    }
    let end_t = fx_laser_post_light_end_t(fraction, range, end_nudge, extra, contents);
    let radius = fx_laser_radius_from_dist(width_dvar, view_dist);
    fx_laser_post_light(
        true, light_dvar, param_4, origin, dir, begin_pad, end_t, radius,
    )
}
