use crate::Pml;
use playerstate_iw4::PlayerState;

const VELOCITY_MOVE_RATIO: f32 = 0.25;

pub fn snap_vector(v: &mut [f32; 3]) {
    v[0] = snap_component(v[0]);
    v[1] = snap_component(v[1]);
    v[2] = snap_component(v[2]);
}

fn snap_component(x: f32) -> f32 {
    round_ties_even(x) as i32 as f32
}

fn round_ties_even(x: f32) -> f32 {
    let abs = libm::fabsf(x);
    let truncated = libm::truncf(abs);
    let frac = abs - truncated;
    let keep_even_tie = frac == 0.5 && (truncated as i32) & 1 == 0;
    let rounded = if frac < 0.5 || keep_even_tie {
        truncated
    } else {
        truncated + 1.0
    };
    libm::copysignf(rounded, x)
}

pub fn pm_end_tick_velocity(ps: &mut PlayerState, pml: &Pml) {
    let dt = pml.frametime;
    if dt > 0.0 {
        let mv = [
            ps.origin[0] - pml.previous_origin[0],
            ps.origin[1] - pml.previous_origin[1],
            ps.origin[2] - pml.previous_origin[2],
        ];
        let real_sq = (mv[0] * mv[0] + mv[1] * mv[1] + mv[2] * mv[2]) / (dt * dt);
        let vel_sq = ps.velocity[0] * ps.velocity[0]
            + ps.velocity[1] * ps.velocity[1]
            + ps.velocity[2] * ps.velocity[2];
        if real_sq < vel_sq * VELOCITY_MOVE_RATIO {
            let inv = 1.0 / dt;
            ps.velocity = [mv[0] * inv, mv[1] * inv, mv[2] * inv];
        }
    }
    snap_vector(&mut ps.velocity);
}
