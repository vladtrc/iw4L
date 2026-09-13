use crate::quat::{Quat, Vec3, normalize};

pub fn half_quat(values: [i16; 2]) -> Quat {
    normalize([
        0.0,
        0.0,
        values[0] as f32 / 32767.0,
        values[1] as f32 / 32767.0,
    ])
}

pub fn full_quat(values: [i16; 4]) -> Quat {
    normalize([
        values[0] as f32 / 32767.0,
        values[1] as f32 / 32767.0,
        values[2] as f32 / 32767.0,
        values[3] as f32 / 32767.0,
    ])
}

pub fn quat16(values: [i16; 4]) -> Quat {
    full_quat(values)
}

pub fn quantized_trans<Q: Into<f32> + Copy>(mins: Vec3, step: Vec3, values: [Q; 3]) -> Vec3 {
    [
        mins[0] + step[0] * values[0].into(),
        mins[1] + step[1] * values[1].into(),
        mins[2] + step[2] * values[2].into(),
    ]
}
