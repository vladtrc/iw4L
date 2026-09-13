use crate::{KbuttonSet, key_state};

pub const CL_YAWSPEED_DEFAULT: f32 = 140.0;

pub const CL_PITCHSPEED_DEFAULT: f32 = 140.0;

pub const CL_ANGLESPEEDKEY_DEFAULT: f32 = 1.5;

#[derive(Clone, Copy, Debug)]
pub struct AdjustAnglesInput {
    pub dt: f32,

    pub now_msec: i32,

    pub frame_msec: u32,

    pub cl_yawspeed: f32,

    pub cl_pitchspeed: f32,

    pub cl_anglespeedkey: f32,

    pub cgame_max_yaw_speed: f32,

    pub cgame_max_pitch_speed: f32,

    pub frozen: bool,
}

pub fn cl_adjust_angles(kb: &mut KbuttonSet, input: AdjustAnglesInput) -> (f32, f32) {
    if input.frozen {
        return (0.0, 0.0);
    }
    let mut speed = input.dt;
    if kb.speed.active {
        speed *= input.cl_anglespeedkey;
    }
    let mut yaw = 0.0;
    if !kb.strafe.active {
        let max = capped_speed(input.cl_yawspeed, input.cgame_max_yaw_speed);
        let step = max * speed;
        yaw -= key_state(&mut kb.right, input.now_msec, input.frame_msec) * step;
        yaw += key_state(&mut kb.left, input.now_msec, input.frame_msec) * step;
    }
    let max = capped_speed(input.cl_pitchspeed, input.cgame_max_pitch_speed);
    let step = max * speed;
    let mut pitch = 0.0;
    pitch -= key_state(&mut kb.lookup, input.now_msec, input.frame_msec) * step;
    pitch += key_state(&mut kb.lookdown, input.now_msec, input.frame_msec) * step;
    (pitch, yaw)
}

fn capped_speed(dvar: f32, cgame_max: f32) -> f32 {
    if cgame_max <= 0.0 {
        dvar
    } else if dvar < cgame_max {
        dvar
    } else {
        cgame_max
    }
}
