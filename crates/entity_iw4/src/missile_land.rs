use math_iw4::{angle_normalize_360, angle_subtract, pitch_for_yaw_on_normal, vect_to_angles};

use crate::trajectory::{
    TR_GRAVITY, TR_LINEAR, TR_STATIONARY, Trajectory, bg_evaluate_trajectory, truncated_tr_delta,
};

pub const GRENADE_APOS_PITCH_OFS: f32 = 120.0;

pub const GRENADE_SPIN_PITCH_MIN: f32 = 320.0;

pub const GRENADE_SPIN_PITCH_MAX: f32 = 800.0;

pub const GRENADE_SPIN_ROLL_MIN: f32 = 180.0;

pub const GRENADE_SPIN_ROLL_MAX: f32 = 540.0;

pub const GRENADE_BLADE_SPIN_PITCH: f32 = 2000.0;

const LAND_FLOOR_NORMAL_Z: f32 = 0.10000000149011612;

const LAND_PITCH_INVERT_DEG: f32 = 80.0;

const LAND_SPIN_RAND_SCALE: f32 = 0.30000001192092896;

const LAND_SPIN_RAND_BIAS: f32 = 0.8500000238418579;

const LAND_SPIN_INVERT: f32 = -1.0;

const LAND_SNAP_PITCH_DEG: f32 = 45.0;

const LAND_FLIP_PITCH_DEG: f32 = 90.0;

pub const MISSILE_NODRAW_SPEED_SCALE: f32 = -35.0;

pub const MISSILE_NODRAW_SPEED_DIV: f32 = 600.0;

pub const MISSILE_NODRAW_BASE_MS: f32 = 85.0;

pub const MISSILE_NODRAW_MIN_MS: i32 = 20;

pub const MISSILE_NODRAW_MAX_MS: i32 = 50;

pub fn g_fire_grenade_no_draw_ms(speed: f32) -> i32 {
    let raw = (speed * MISSILE_NODRAW_SPEED_SCALE / MISSILE_NODRAW_SPEED_DIV
        + MISSILE_NODRAW_BASE_MS) as i32;
    raw.clamp(MISSILE_NODRAW_MIN_MS, MISSILE_NODRAW_MAX_MS)
}

pub fn cg_missile_nodraw(e_flags: u32, launch_time: i32, cg_time: i32) -> Option<&'static str> {
    if e_flags & 0x20 != 0 {
        Some("eflags_nodraw")
    } else if cg_time < launch_time {
        Some("launch_time")
    } else {
        None
    }
}

pub fn g_init_grenade_pos(start: [f32; 3], dir: [f32; 3], level_time_ms: i32) -> Trajectory {
    Trajectory {
        tr_time: level_time_ms,
        tr_type: TR_GRAVITY,
        tr_duration: 0,
        tr_delta: truncated_tr_delta(dir),
        tr_base: start,
    }
}

pub fn g_init_grenade_apos(
    dir: [f32; 3],
    level_time_ms: i32,
    pitch_rate: f32,
    roll_rate: f32,
) -> Trajectory {
    let mut angles = vect_to_angles(dir);
    angles[0] = angle_normalize_360(angles[0] - GRENADE_APOS_PITCH_OFS);
    Trajectory {
        tr_time: level_time_ms,
        tr_type: TR_LINEAR,
        tr_duration: 0,
        tr_delta: [pitch_rate, 0.0, roll_rate],
        tr_base: angles,
    }
}

pub fn g_fire_missile_apos(dir: [f32; 3]) -> Trajectory {
    Trajectory {
        tr_time: 0,
        tr_type: TR_STATIONARY,
        tr_duration: 0,
        tr_delta: [0.0; 3],
        tr_base: vect_to_angles(dir),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MissileLandAnglesIn {
    pub apos: Trajectory,
    pub normal: [f32; 3],
    pub hit_time_ms: i32,
    pub force_align: bool,

    pub g_random: f32,

    pub wall_spin_addend: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileLandAnglesOut {
    pub angles: [f32; 3],
    pub apos: Trajectory,
}

pub fn missile_land_angles(input: MissileLandAnglesIn) -> MissileLandAnglesOut {
    let mut apos = input.apos;
    let mut angles = bg_evaluate_trajectory(&apos, input.hit_time_ms);
    if input.normal[2] <= LAND_FLOOR_NORMAL_Z {
        if !input.force_align {
            apos.tr_delta[0] = angle_normalize_360(input.wall_spin_addend + apos.tr_delta[0]);
        }
        return MissileLandAnglesOut { angles, apos };
    }

    let surface_pitch = pitch_for_yaw_on_normal(angles[1], input.normal);
    let angle_delta = angle_subtract(surface_pitch, angles[0]);
    let abs_delta = if angle_delta < 0.0 {
        -angle_delta
    } else {
        angle_delta
    };
    if !input.force_align {
        apos.tr_base = angles;
        apos.tr_time = input.hit_time_ms;
        let keep = input.g_random * LAND_SPIN_RAND_SCALE + LAND_SPIN_RAND_BIAS;
        apos.tr_delta[0] = if LAND_PITCH_INVERT_DEG <= abs_delta {
            keep * apos.tr_delta[0]
        } else {
            keep * apos.tr_delta[0] * LAND_SPIN_INVERT
        };
    }

    angles[0] = angle_subtract(angles[0], 0.0);
    if input.force_align || LAND_SNAP_PITCH_DEG > abs_delta {
        let wrapped_abs = if angles[0] < 0.0 {
            -angles[0]
        } else {
            angles[0]
        };
        angles[0] = if wrapped_abs <= LAND_FLIP_PITCH_DEG {
            angle_normalize_360(surface_pitch)
        } else {
            angle_normalize_360(surface_pitch + 180.0)
        };
    } else if LAND_PITCH_INVERT_DEG <= abs_delta {
        angles[0] = angle_normalize_360(angles[0]);
    } else {
        angles[0] = angle_normalize_360(angle_delta * 0.25 + angles[0]);
    }
    MissileLandAnglesOut { angles, apos }
}
