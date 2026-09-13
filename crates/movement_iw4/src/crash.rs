use playerstate_iw4::PlayerState;

use crate::{Pml, add_predictable_event, pm_ground_surface_type};

const EV_FOOTSTEP_RUN: i32 = 0x6c;
const EV_FOOTSTEP_WALK: i32 = 0x6d;

const EV_LANDING_FIRST: i32 = 0x70;

const FALL_LIGHT_IN: f32 = 4.0;

const FALL_MEDIUM_IN: f32 = 8.0;

const FALL_HARD_IN: f32 = 12.0;

const HARD_LAND_VEL_SCALE: f32 = 0.67;

const HALF: f32 = 0.5;

const FOUR: f32 = 4.0;

const TWO: f32 = 2.0;

const NEG_ONE: f32 = -1.0;

pub fn pm_crash_land(ps: &mut PlayerState, pml: &Pml) {
    let Some(fall_height) = crash_land_fall_height(ps, pml) else {
        return;
    };
    let surface = pm_ground_surface_type(pml.ground_trace[4]);
    crash_land_apply_sfx(ps, fall_height, surface);
}

pub fn crash_land_fall_height(ps: &PlayerState, pml: &Pml) -> Option<f32> {
    if ps.gravity == 0 {
        return None;
    }
    let dist = pml.previous_origin[2] - ps.origin[2];
    let vel = pml.previous_velocity[2];
    let acc = -(ps.gravity as f32);
    let a = acc * HALF;
    let den = vel * vel - FOUR * a * dist;
    if den < 0.0 {
        return None;
    }
    let two_a = a * TWO;
    if two_a == 0.0 {
        return None;
    }
    let t = (-vel - libm::sqrtf(den)) / two_a;
    let land_vel = (t * acc + vel) * NEG_ONE;
    Some((land_vel * land_vel) / ((ps.gravity as f32) * TWO))
}

fn crash_land_apply_sfx(ps: &mut PlayerState, fall_height: f32, surface: i32) {
    if fall_height <= FALL_LIGHT_IN {
        return;
    }
    if fall_height < FALL_MEDIUM_IN {
        if surface != 0 {
            add_predictable_event(ps, EV_FOOTSTEP_WALK, surface);
        }
        return;
    }
    if fall_height < FALL_HARD_IN {
        if surface != 0 {
            add_predictable_event(ps, EV_FOOTSTEP_RUN, surface);
        }
        return;
    }
    ps.velocity[0] *= HARD_LAND_VEL_SCALE;
    ps.velocity[1] *= HARD_LAND_VEL_SCALE;
    ps.velocity[2] *= HARD_LAND_VEL_SCALE;
    if surface != 0 {
        add_predictable_event(ps, EV_LANDING_FIRST + surface, 0);
    }
}
