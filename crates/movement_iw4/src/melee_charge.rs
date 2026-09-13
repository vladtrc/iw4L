use math_iw4::yaw_vectors_2d;
use playerstate_iw4::{PlayerState, pm_flags};

use crate::collision::CollisionBackend;
use crate::pml::Pml;
use crate::slide::pm_step_slide_move;

const MS_TO_SECONDS: f32 = 0.001;

const HALF: f32 = 0.5;

pub const PLAYER_MELEE_RANGE_DEFAULT: f32 = 64.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeleeChargeWeaponDelays {
    pub melee_delay_ms: i32,

    pub melee_charge_delay_ms: i32,
}

pub fn pm_melee_charge_clear(ps: &mut PlayerState) {
    ps.pm_flags &= !pm_flags::MELEE_CHARGE;
    ps.melee_charge_yaw = 0.0;
    ps.melee_charge_dist = 0;
    ps.melee_charge_time = 0;
}

pub fn pm_calc_melee_charge_time(
    ps: &mut PlayerState,
    delays: MeleeChargeWeaponDelays,
    player_melee_range: f32,
) {
    if (ps.pm_flags & pm_flags::MELEE_CHARGE) == 0 {
        pm_melee_charge_clear(ps);
        return;
    }
    if ps.melee_charge_time != 0 {
        return;
    }

    let delay_ms = if (ps.melee_charge_dist as f32) <= player_melee_range {
        delays.melee_delay_ms
    } else {
        delays.melee_charge_delay_ms
    };
    let charge_time_sec = delay_ms as f32 * MS_TO_SECONDS;

    if charge_time_sec <= 0.0 {
        pm_melee_charge_clear(ps);
        return;
    }

    let speed = (ps.melee_charge_dist as f32 / charge_time_sec) * 2.0;
    let (forward, _) = yaw_vectors_2d(ps.melee_charge_yaw);
    ps.velocity[0] = speed * forward[0];
    ps.velocity[1] = speed * forward[1];

    ps.melee_charge_time = delay_ms;
}

fn project_to_ground(velocity: &mut [f32; 3], ground_trace: &[u32; 11]) {
    let normal = [
        f32::from_bits(ground_trace[1]),
        f32::from_bits(ground_trace[2]),
        f32::from_bits(ground_trace[3]),
    ];
    crate::pm_project_velocity(velocity, &normal);
}

pub fn pm_melee_charge_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    collision: &C,
) {
    let speed = {
        let vx = ps.velocity[0];
        let vy = ps.velocity[1];
        let vz = ps.velocity[2];
        libm::sqrtf(vx * vx + vy * vy + vz * vz)
    };

    let mut dir = [0.0_f32; 3];
    let mut new_speed = 0.0_f32;
    if speed > 0.0 {
        dir[0] = ps.velocity[0] / speed;
        dir[1] = ps.velocity[1] / speed;
        dir[2] = ps.velocity[2] / speed;
        let time_sec = ps.melee_charge_time as f32 * MS_TO_SECONDS;
        new_speed = speed - (speed / time_sec) * pml.frametime;
        if new_speed < 0.0 {
            new_speed = 0.0;
        }
        let mid = (speed + new_speed) * HALF;
        ps.velocity[0] = mid * dir[0];
        ps.velocity[1] = mid * dir[1];
        ps.velocity[2] = mid * dir[2];
    }

    project_to_ground(&mut ps.velocity, &pml.ground_trace);

    if ps.velocity[0] != 0.0 || ps.velocity[1] != 0.0 {
        pm_step_slide_move(ps, pml, collision, mins, maxs, tracemask, None);
    }

    ps.velocity[0] = new_speed * dir[0];
    ps.velocity[1] = new_speed * dir[1];
    ps.velocity[2] = new_speed * dir[2];

    ps.melee_charge_time -= pml.msec;
    if ps.melee_charge_time > 0 {
        return;
    }

    ps.velocity[0] = 0.0;
    ps.velocity[1] = 0.0;
    pm_melee_charge_clear(ps);
}
