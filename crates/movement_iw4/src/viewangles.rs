use math_iw4::angle_subtract;
use playerstate_iw4::{PlayerState, UserCmd};

pub const SHORT2ANGLE: f32 = 0.005_493_164_062_5;

pub const ANGLE2SHORT: f32 = 65536.0 / 360.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewAngleClamp {
    pub pitch_up: f32,

    pub pitch_down: f32,

    pub unclamped_pitch_bit: bool,
}

pub fn pm_update_view_angles(ps: &mut PlayerState, cmd: &UserCmd, clamp: ViewAngleClamp) {
    let unclamped = (ps.pm_type == 1 || ps.pm_type == 9) && clamp.unclamped_pitch_bit;

    for axis in 0..3usize {
        let commanded = cmd.angles[axis] as f32 * SHORT2ANGLE;
        let mut temp = wrap_signed(commanded + ps.delta_angles[axis]);

        if axis == 0 && !unclamped {
            if clamp.pitch_down < temp {
                ps.delta_angles[0] = clamp.pitch_down - commanded;
                temp = clamp.pitch_down;
            } else if temp < -clamp.pitch_up {
                ps.delta_angles[0] = -clamp.pitch_up - commanded;
                temp = -clamp.pitch_up;
            }
        }

        ps.viewangles[axis] = wrap_signed(temp);
    }
}

fn wrap_signed(degrees: f32) -> f32 {
    angle_subtract(degrees, 0.0)
}
