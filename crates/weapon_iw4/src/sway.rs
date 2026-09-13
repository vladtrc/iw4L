pub const SWAY_FRAME_HZ: f32 = 60.0;

pub const TRACK_SNAP_EPS: f32 = 0.001;

pub const SWAY_SHELLSHOCK_SMOOTH_PEAK: f32 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayParams {
    pub max_angle: f32,
    pub lerp_speed: f32,
    pub pitch_scale: f32,
    pub yaw_scale: f32,
    pub horiz_scale: f32,
    pub vert_scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwaySpringState {
    pub horiz: f32,

    pub vert: f32,

    pub pitch: f32,

    pub yaw: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwayContribution {
    pub origin: [f32; 3],

    pub angles: [f32; 3],
}

#[inline]
pub fn angle_normalize_180(delta: f32) -> f32 {
    let mut d = delta;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

#[inline]
pub fn angle_delta(current: f32, previous: f32) -> f32 {
    angle_normalize_180(current - previous)
}

pub fn track(target: f32, current: f32, speed: f32, dt: f32) -> f32 {
    let diff = target - current;
    if abs_f32(diff) <= TRACK_SNAP_EPS {
        return target;
    }
    let step = diff * speed * dt;
    if abs_f32(step) >= abs_f32(diff) {
        target
    } else {
        current + step
    }
}

pub fn track_a(target: f32, current: f32, speed: f32, dt: f32) -> f32 {
    let wrapped_target = current + angle_delta(target, current);
    angle_normalize_180(track(wrapped_target, current, speed, dt))
}

#[inline]
pub fn lerp_sway_params(
    hip: WeaponSwayParams,
    ads: WeaponSwayParams,
    frac: f32,
) -> WeaponSwayParams {
    let t = frac;
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    WeaponSwayParams {
        max_angle: lerp(hip.max_angle, ads.max_angle),
        lerp_speed: lerp(hip.lerp_speed, ads.lerp_speed),
        pitch_scale: lerp(hip.pitch_scale, ads.pitch_scale),
        yaw_scale: lerp(hip.yaw_scale, ads.yaw_scale),
        horiz_scale: lerp(hip.horiz_scale, ads.horiz_scale),
        vert_scale: lerp(hip.vert_scale, ads.vert_scale),
    }
}

#[inline]
pub fn clamp_abs(v: f32, max: f32) -> f32 {
    let m = abs_f32(max);
    if v > m {
        m
    } else if v < -m {
        -m
    } else {
        v
    }
}

#[inline]
pub fn sway_shellshock_landing_scale(
    remaining_ms: i32,
    duration_ms: i32,
    shell_shock_scale: f32,
) -> f32 {
    if remaining_ms <= 0 {
        return 1.0;
    }
    let mut t = 1.0_f32;
    if duration_ms > 0 && remaining_ms < duration_ms {
        t = remaining_ms as f32 / duration_ms as f32;
    }

    let weight = (SWAY_SHELLSHOCK_SMOOTH_PEAK - (t + t)) * t * t;
    (shell_shock_scale - 1.0) * weight + 1.0
}

pub fn bg_calculate_weapon_movement_sway(
    springs: &mut SwaySpringState,
    view_angles: [f32; 3],
    prev_view_angles: [f32; 3],
    params: WeaponSwayParams,
    landing_scale: f32,
    dt_secs: f32,
) {
    if dt_secs == 0.0 {
        return;
    }
    let frame_scale = dt_secs * SWAY_FRAME_HZ;
    if frame_scale == 0.0 {
        return;
    }
    let inv = 1.0 / frame_scale;
    let mut pitch_d = angle_delta(view_angles[0], prev_view_angles[0]) * inv;
    let mut yaw_d = angle_delta(view_angles[1], prev_view_angles[1]) * inv;
    pitch_d = clamp_abs(pitch_d, params.max_angle);
    yaw_d = clamp_abs(yaw_d, params.max_angle);
    let land = landing_scale;

    springs.horiz = track(
        land * params.horiz_scale * yaw_d,
        springs.horiz,
        params.lerp_speed,
        dt_secs,
    );
    springs.vert = track(
        land * params.vert_scale * pitch_d,
        springs.vert,
        params.lerp_speed,
        dt_secs,
    );
    springs.pitch = track_a(
        land * params.pitch_scale * pitch_d,
        springs.pitch,
        params.lerp_speed,
        dt_secs,
    );
    springs.yaw = track_a(
        land * params.yaw_scale * yaw_d,
        springs.yaw,
        params.lerp_speed,
        dt_secs,
    );
}

#[inline]
pub fn sway_contribution(springs: SwaySpringState) -> SwayContribution {
    SwayContribution {
        origin: [0.0, -springs.horiz, springs.vert],

        angles: [-springs.pitch, -springs.yaw, 0.0],
    }
}

#[inline]
fn abs_f32(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}
