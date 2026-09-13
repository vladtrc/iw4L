pub const KICK_STEP_MS: i32 = 5;

pub const MS_TO_SEC: f32 = 0.001;

pub const VIEW_KICK_CLAMP_DEG: f32 = 10.0;

pub const VIEW_KICK_RETURN_SCALE: f32 = 0.06;

pub const VIEW_KICK_ADS_FRAC: f32 = 0.5;

pub const VIEW_KICK_NO_WEAPON_CENTER_SPEED: f32 = 2400.0;

pub const REDUCED_KICK_PERCENT_SCALE: f32 = 0.01;

pub const KICK_AVEL_ROLL_FROM_YAW: f32 = -0.5;

pub const RECOIL_SCALE_DIVISOR: f32 = 100.0;

pub const DOUBLEBARREL_PITCH_SCALE: f32 = 5.0;

pub const GUN_KICK_OFS_EPS: f32 = 0.25;

pub const GUN_KICK_SPEED_EPS: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewKickRange {
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub yaw_min: f32,
    pub yaw_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKickRange {
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub yaw_min: f32,
    pub yaw_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunRecoilPlacementState {
    pub pitch_offset: f32,
    pub pitch_speed: f32,
    pub yaw_offset: f32,
    pub yaw_speed: f32,
}

#[inline]
pub fn gun_recoil_angle_contribution(state: GunRecoilPlacementState) -> [f32; 3] {
    [state.pitch_offset, state.yaw_offset, 0.0]
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKickSpring {
    pub accel: f32,
    pub speed_max: f32,
    pub speed_decay: f32,
    pub static_decay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireRecoilImpulse {
    pub kick_avel: [f32; 3],

    pub gun_speed_delta: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FireRecoilPsScales {
    pub reduce_window_active: bool,

    pub reduced_percent: f32,

    pub weap_flags: u32,

    pub recoil_scale: i32,
}

#[inline]
pub fn fire_recoil_amplitude_scale(ps: FireRecoilPsScales) -> f32 {
    let mut scale = fire_recoil_reduce_scale(ps.reduce_window_active, ps.reduced_percent);
    if ps.weap_flags & 0x400 != 0 {
        scale *= ps.recoil_scale as f32 / RECOIL_SCALE_DIVISOR;
    }
    scale
}

#[inline]
pub fn fire_recoil_pitch_extra_scale(weap_flags: u32) -> f32 {
    if weap_flags & 0x200 != 0 {
        DOUBLEBARREL_PITCH_SCALE
    } else {
        1.0
    }
}

#[inline]
pub fn fire_recoil_view_range(
    weapon_pos_frac: f32,
    hip: ViewKickRange,
    ads: ViewKickRange,
) -> ViewKickRange {
    if weapon_pos_frac == 1.0 { ads } else { hip }
}

#[inline]
pub fn fire_recoil_gun_range(
    weapon_pos_frac: f32,
    hip: GunKickRange,
    ads: GunKickRange,
) -> GunKickRange {
    if weapon_pos_frac <= 0.0 { hip } else { ads }
}

#[inline]
pub fn fire_recoil_reduce_scale(window_active: bool, reduced_percent: f32) -> f32 {
    if window_active {
        reduced_percent * REDUCED_KICK_PERCENT_SCALE
    } else {
        1.0
    }
}

#[inline]
pub fn start_firing_restrict_kick_time(
    weapon_pos_frac: f32,
    ads_reduced_bullets: i32,
    hip_reduced_bullets: i32,
    fire_time_ms: i32,
    fire_delay_ms: i32,
) -> i32 {
    let bullets = if weapon_pos_frac == 1.0 {
        ads_reduced_bullets
    } else {
        hip_reduced_bullets
    };
    bullets
        .saturating_mul(fire_time_ms)
        .saturating_add(fire_delay_ms)
}

pub fn bg_weapon_fire_recoil(
    view: ViewKickRange,
    gun: GunKickRange,
    ps: FireRecoilPsScales,
    unit01: [f32; 4],
) -> FireRecoilImpulse {
    let scale = fire_recoil_amplitude_scale(ps);
    let pitch_x = fire_recoil_pitch_extra_scale(ps.weap_flags);
    let pitch = lerp(view.pitch_min, view.pitch_max, unit01[0]) * scale * pitch_x;
    let yaw = lerp(view.yaw_min, view.yaw_max, unit01[1]) * scale;
    FireRecoilImpulse {
        kick_avel: [-pitch, yaw, yaw * KICK_AVEL_ROLL_FROM_YAW],
        gun_speed_delta: [
            lerp(gun.pitch_min, gun.pitch_max, unit01[2]) * scale * pitch_x,
            lerp(gun.yaw_min, gun.yaw_max, unit01[3]) * scale,
        ],
    }
}

#[inline]
pub fn kick_angles_center_speed(
    weapon_index: i32,
    weapon_pos_frac: f32,
    hip_center_speed: f32,
    ads_center_speed: f32,
) -> f32 {
    if weapon_index == 0 {
        VIEW_KICK_NO_WEAPON_CENTER_SPEED
    } else if weapon_pos_frac <= VIEW_KICK_ADS_FRAC {
        hip_center_speed
    } else {
        ads_center_speed
    }
}

pub fn cg_kick_angles_step_axis(angle: &mut f32, avel: &mut f32, dt: f32, center_speed: f32) {
    if *avel == 0.0 && *angle == 0.0 {
        return;
    }
    if *angle != 0.0 {
        let toward_zero = if *angle <= 0.0 { 1.0 } else { -1.0 };
        *avel += toward_zero * center_speed * dt;
    }
    let mut change = *avel * dt;
    if *angle * change < 0.0 {
        change *= VIEW_KICK_RETURN_SCALE;
    }
    let next = *angle + change;
    if next * *angle < 0.0 {
        *angle = 0.0;
        *avel = 0.0;
    } else {
        *angle = next;
        if *angle == 0.0 {
            *avel = 0.0;
        } else if abs_f32(*angle) > VIEW_KICK_CLAMP_DEG {
            *angle = if *angle <= 0.0 {
                -VIEW_KICK_CLAMP_DEG
            } else {
                VIEW_KICK_CLAMP_DEG
            };
            *avel = 0.0;
        }
    }
}

pub fn cg_kick_angles(
    angles: &mut [f32; 3],
    avel: &mut [f32; 3],
    frametime_ms: i32,
    center_speed: f32,
) {
    let mut t = frametime_ms;
    while t > 0 {
        let step_ms = if t < KICK_STEP_MS { t } else { KICK_STEP_MS };
        let dt = step_ms as f32 * MS_TO_SEC;
        for i in 0..3 {
            cg_kick_angles_step_axis(&mut angles[i], &mut avel[i], dt, center_speed);
        }
        t -= KICK_STEP_MS;
    }
}

#[inline]
pub fn lerp_gun_kick_spring(hip: GunKickSpring, ads: GunKickSpring, frac: f32) -> GunKickSpring {
    let t = frac;
    GunKickSpring {
        accel: lerp(hip.accel, ads.accel, t),
        speed_max: lerp(hip.speed_max, ads.speed_max, t),
        speed_decay: lerp(hip.speed_decay, ads.speed_decay, t),
        static_decay: lerp(hip.static_decay, ads.static_decay, t),
    }
}

pub fn gun_recoil_single_angle(
    offset: &mut f32,
    speed: &mut f32,
    dt: f32,
    ofs_cap: f32,
    spring: GunKickSpring,
) -> bool {
    if abs_f32(*offset) < GUN_KICK_OFS_EPS && abs_f32(*speed) < GUN_KICK_SPEED_EPS {
        *offset = 0.0;
        *speed = 0.0;
        return true;
    }
    let cap = abs_f32(ofs_cap);
    *offset += *speed * dt;
    if *offset > cap {
        *offset = cap;
        if *speed > 0.0 {
            *speed = 0.0;
        }
    } else if *offset < -cap {
        *offset = -cap;
        if *speed < 0.0 {
            *speed = 0.0;
        }
    }
    if *offset < 0.0 {
        *speed += spring.accel * dt;
    } else if *offset > 0.0 {
        *speed -= spring.accel * dt;
    }
    *speed -= *speed * spring.speed_decay * dt;
    if *speed <= 0.0 {
        *speed += spring.static_decay * dt;
        if *speed > 0.0 {
            *speed = 0.0;
        }
    } else {
        *speed -= spring.static_decay * dt;
        if *speed < 0.0 {
            *speed = 0.0;
        }
    }
    if *speed > spring.speed_max {
        *speed = spring.speed_max;
    } else if *speed < -spring.speed_max {
        *speed = -spring.speed_max;
    }
    false
}

pub fn bg_calculate_weapon_position_gun_recoil(
    state: &mut GunRecoilPlacementState,
    frametime_secs: f32,
    weapon_pos_frac: f32,
    overlay_active: bool,
    hip: GunKickSpring,
    ads: GunKickSpring,
    gun_max_pitch: f32,
    gun_max_yaw: f32,
) {
    if !overlay_active || frametime_secs <= 0.0 {
        return;
    }
    let spring = lerp_gun_kick_spring(hip, ads, weapon_pos_frac);
    let step_secs = KICK_STEP_MS as f32 * MS_TO_SEC;
    let mut remaining = frametime_secs;
    while remaining > 0.0 {
        let dt = if remaining < step_secs {
            remaining
        } else {
            step_secs
        };
        let _ = gun_recoil_single_angle(
            &mut state.pitch_offset,
            &mut state.pitch_speed,
            dt,
            gun_max_pitch,
            spring,
        );
        let _ = gun_recoil_single_angle(
            &mut state.yaw_offset,
            &mut state.yaw_speed,
            dt,
            gun_max_yaw,
            spring,
        );
        remaining -= dt;
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn abs_f32(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}
