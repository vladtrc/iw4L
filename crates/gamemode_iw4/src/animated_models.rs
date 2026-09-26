pub const FAN_BLADE_ROTATE_TIME: f32 = 20000.0;

pub const FAN_BLADE_AXIS_DOT: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanBladeRotateChannel {
    Pitch,
    Yaw,
}

pub fn fan_blade_right(angles: [f32; 3]) -> [f32; 3] {
    math_iw4::angle_vectors(angles).1
}

pub fn fan_blade_dots(right: [f32; 3]) -> [f32; 3] {
    [
        libm::fabsf(right[0]),
        libm::fabsf(right[1]),
        libm::fabsf(right[2]),
    ]
}

pub fn fan_blade_rotate_channel(right: [f32; 3]) -> FanBladeRotateChannel {
    let [dot_x, dot_y, _] = fan_blade_dots(right);
    if dot_x > FAN_BLADE_AXIS_DOT || dot_y > FAN_BLADE_AXIS_DOT {
        FanBladeRotateChannel::Pitch
    } else {
        FanBladeRotateChannel::Yaw
    }
}

pub fn fan_blade_rotate_delta(right: [f32; 3], speed: f32) -> [f32; 3] {
    match fan_blade_rotate_channel(right) {
        FanBladeRotateChannel::Pitch => [speed, 0.0, 0.0],
        FanBladeRotateChannel::Yaw => [0.0, speed, 0.0],
    }
}
