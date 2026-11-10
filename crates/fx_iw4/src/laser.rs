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
pub const fn laser_brush_trace_allows(startsolid: bool) -> bool {
    !startsolid
}

#[inline]
pub const fn laser_post_light_allows(light_dvar: bool, param_4: i32) -> bool {
    light_dvar && param_4 == 1
}

#[inline]
pub fn laser_post_light_end_t(
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
pub fn laser_post_light_span(begin_pad: f32, end_t: f32) -> (f32, f32) {
    if end_t - begin_pad < FX_LASER_POST_LIGHT_MIN_SPAN {
        let mid = (begin_pad + end_t) * FX_LASER_POST_LIGHT_HALF;
        (mid - FX_LASER_POST_LIGHT_PAD, mid + FX_LASER_POST_LIGHT_PAD)
    } else {
        (begin_pad, end_t)
    }
}

#[inline]
pub fn laser_radius_from_dist(width_dvar: f32, dist: f32) -> f32 {
    width_dvar * dist * FX_LASER_RADIUS_DIST_SCALE + FX_LASER_RADIUS_DIST_BIAS
}

#[inline]
pub fn laser_point_on_ray(origin: [f32; 3], dir: [f32; 3], t: f32) -> [f32; 3] {
    [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct FxLaserRay {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub range: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxLaserTrace {
    pub startsolid: bool,
    pub fraction: f32,
    pub contents: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxLaserLightSettings {
    pub enabled: bool,
    pub begin_pad: f32,
    pub end_nudge: f32,
    pub width: f32,
}

impl FxLaserRay {
    pub fn post_light(
        &self,
        hit: bool,
        enabled: bool,
        begin_t: f32,
        end_t: f32,
        radius: f32,
    ) -> Option<FxPostLight> {
        if !hit || !enabled {
            return None;
        }
        let (begin_t, end_t) = laser_post_light_span(begin_t, end_t);
        Some(FxPostLight {
            begin: laser_point_on_ray(self.origin, self.direction, begin_t),
            end: laser_point_on_ray(self.origin, self.direction, end_t),
            radius,
            color_packed: FX_LASER_POST_LIGHT_COLOR,
            material_name: FX_LASER_POST_LIGHT_MATERIAL,
        })
    }

    pub fn light_from_tag(
        &self,
        view: [f32; 3],
        settings: FxLaserLightSettings,
    ) -> Option<FxPostLight> {
        let end = laser_point_on_ray(self.origin, self.direction, self.range);
        let view_dist = crate::vec::vec3_distance(end, view);
        self.light_from_brush_trace(
            FxLaserTrace {
                startsolid: false,
                fraction: 1.0,
                contents: 0,
            },
            settings,
            0.0,
            view_dist,
        )
    }

    pub fn light_from_brush_trace(
        &self,
        trace: FxLaserTrace,
        settings: FxLaserLightSettings,
        extra: f32,
        view_dist: f32,
    ) -> Option<FxPostLight> {
        if !laser_brush_trace_allows(trace.startsolid) {
            return None;
        }
        let end_t = laser_post_light_end_t(
            trace.fraction,
            self.range,
            settings.end_nudge,
            extra,
            trace.contents,
        );
        let radius = laser_radius_from_dist(settings.width, view_dist);
        self.post_light(true, settings.enabled, settings.begin_pad, end_t, radius)
    }
}
