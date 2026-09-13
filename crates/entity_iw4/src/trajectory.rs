pub const TR_STATIONARY: i32 = 0;

pub const TR_INTERPOLATE: i32 = 1;

pub const TR_LINEAR: i32 = 2;

pub const TR_LINEAR_STOP: i32 = 3;

pub const TR_GRAVITY: i32 = 5;

const TRAJECTORY_MSEC_TO_SEC: f32 = 0.001;

const TRAJECTORY_GRAVITY: f32 = 400.0;

const TRAJECTORY_GRAVITY_DELTA: f32 = 800.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trajectory {
    pub tr_time: i32,
    pub tr_type: i32,
    pub tr_duration: i32,
    pub tr_delta: [f32; 3],
    pub tr_base: [f32; 3],
}

pub fn bg_evaluate_trajectory(tr: &Trajectory, at_time: i32) -> [f32; 3] {
    match tr.tr_type {
        0 | 1 | 9 | 0xc => tr.tr_base,
        2 | 10 => vec3_mad(tr.tr_base, trajectory_dt(tr.tr_time, at_time), tr.tr_delta),
        3 => evaluate_linear_stop(tr, at_time),
        5 | 6 | 0xb => evaluate_gravity(tr, at_time),
        4 | 7 | 8 => panic!("BG_EvaluateTrajectory SINE/accel cases are not ported"),
        _ => panic!("BG_EvaluateTrajectory unknown trType"),
    }
}

fn trajectory_dt(tr_time: i32, at_time: i32) -> f32 {
    (at_time.wrapping_sub(tr_time) as f32) * TRAJECTORY_MSEC_TO_SEC
}

fn evaluate_linear_stop(tr: &Trajectory, at_time: i32) -> [f32; 3] {
    let stop_at = tr.tr_time.wrapping_add(tr.tr_duration);
    let at_time = if stop_at < at_time { stop_at } else { at_time };
    let dt = trajectory_dt(tr.tr_time, at_time);
    let dt = if dt < 0.0 { 0.0 } else { dt };
    vec3_mad(tr.tr_base, dt, tr.tr_delta)
}

fn vec3_mad(base: [f32; 3], scale: f32, delta: [f32; 3]) -> [f32; 3] {
    [
        base[0] + scale * delta[0],
        base[1] + scale * delta[1],
        base[2] + scale * delta[2],
    ]
}

fn evaluate_gravity(tr: &Trajectory, at_time: i32) -> [f32; 3] {
    let dt = trajectory_dt(tr.tr_time, at_time);
    let mut origin = vec3_mad(tr.tr_base, dt, tr.tr_delta);
    origin[2] -= TRAJECTORY_GRAVITY * dt * dt;
    origin
}

pub fn bg_evaluate_trajectory_delta(tr: &Trajectory, at_time: i32) -> [f32; 3] {
    match tr.tr_type {
        0 | 1 | 0xc => [0.0; 3],
        2 | 10 => tr.tr_delta,
        3 => {
            let stop_at = tr.tr_time.wrapping_add(tr.tr_duration);
            if stop_at < at_time {
                [0.0; 3]
            } else {
                tr.tr_delta
            }
        }
        5 | 6 | 0xb => {
            let dt = trajectory_dt(tr.tr_time, at_time);
            [
                tr.tr_delta[0],
                tr.tr_delta[1],
                tr.tr_delta[2] - dt * TRAJECTORY_GRAVITY_DELTA,
            ]
        }
        4 | 7 | 8 => panic!("BG_EvaluateTrajectoryDelta SINE/accel cases are not ported"),
        _ => panic!("BG_EvaluateTrajectoryDelta unknown trType"),
    }
}

pub fn truncated_tr_delta(delta: [f32; 3]) -> [f32; 3] {
    [
        delta[0] as i32 as f32,
        delta[1] as i32 as f32,
        delta[2] as i32 as f32,
    ]
}
