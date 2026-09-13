pub fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let pitch = angles[0] * RETAIL_DEGREES_TO_RADIANS;
    let yaw = angles[1] * RETAIL_DEGREES_TO_RADIANS;
    let roll = angles[2] * RETAIL_DEGREES_TO_RADIANS;
    let cy = libm::cosf(yaw);
    let sy = libm::sinf(yaw);
    let cp = libm::cosf(pitch);
    let sp = libm::sinf(pitch);
    let cr = libm::cosf(roll);
    let sr = libm::sinf(roll);

    (
        [cp * cy, cp * sy, -sp],
        [
            -sy * cr * RETAIL_RIGHT_AXIS_SIGN - cy * sr * sp,
            cy * cr * RETAIL_RIGHT_AXIS_SIGN - sr * sp * sy,
            RETAIL_RIGHT_AXIS_SIGN * sr * cp,
        ],
        [cy * cr * sp - -sy * sr, cr * sp * sy - sr * cy, cr * cp],
    )
}

pub fn angles_to_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (forward, right, up) = angle_vectors(angles);
    [forward, [-right[0], -right[1], -right[2]], up]
}

pub fn angle_normalize_360(angle: f32) -> f32 {
    let scaled = angle * RETAIL_DEGREES_TO_TURNS;
    let turns = retail_floor(scaled);
    let normalized = (scaled - turns) * RETAIL_TURN_DEGREES;
    if 0.0 <= normalized - RETAIL_TURN_DEGREES {
        normalized - RETAIL_TURN_DEGREES
    } else {
        normalized
    }
}

pub fn track(current: f32, target: f32, scale: f32, frame: f32) -> f32 {
    let delta = current - target;
    if delta.abs() <= RETAIL_TRACK_SNAP_EPSILON {
        return current;
    }

    let step = delta * scale * frame;
    if delta.abs() < step.abs() {
        return current;
    }

    target + step
}

pub fn track_angle(current: f32, target: f32, scale: f32, frame: f32) -> f32 {
    let mut wrapped_current = current;
    while RETAIL_TRACK_ANGLE_LOWER < wrapped_current - target {
        wrapped_current -= RETAIL_TURN_DEGREES;
    }
    while wrapped_current - target < RETAIL_TRACK_ANGLE_UPPER {
        wrapped_current += RETAIL_TURN_DEGREES;
    }

    let tracked = track(wrapped_current, target, scale, frame);
    angle_subtract(tracked, 0.0)
}

const RETAIL_TRACK_SNAP_EPSILON: f32 = 0.001;

const RETAIL_DEGREES_TO_RADIANS: f32 = 0.01745329238474369_f32;

const RETAIL_RADIANS_TO_DEGREES: f32 = 57.295780181884766_f32;

pub fn vec_to_yaw(x: f32, y: f32) -> f32 {
    if y == 0.0 && x == 0.0 {
        return 0.0;
    }
    let degrees = libm::atan2f(y, x) * RETAIL_RADIANS_TO_DEGREES;
    if 0.0 <= degrees {
        degrees
    } else {
        degrees + RETAIL_TURN_DEGREES
    }
}

const VECT_TO_ANGLES_PITCH_SCALE: f32 = -57.2957763671875_f32;

const VECT_TO_ANGLES_PITCH_DOWN: f32 = 270.0;

const VECT_TO_ANGLES_PITCH_UP: f32 = 90.0;

pub fn vect_to_angles(forward: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = forward;
    if y == 0.0 && x == 0.0 {
        let pitch = if 0.0 <= -z {
            VECT_TO_ANGLES_PITCH_DOWN
        } else {
            VECT_TO_ANGLES_PITCH_UP
        };
        return [pitch, 0.0, 0.0];
    }
    let yaw = vec_to_yaw(x, y);
    let hypot = libm::sqrtf(x * x + y * y);
    let degrees = libm::atan2f(z, hypot) * VECT_TO_ANGLES_PITCH_SCALE;
    let pitch = if 0.0 <= degrees {
        degrees
    } else {
        degrees + RETAIL_TURN_DEGREES
    };
    [pitch, yaw, 0.0]
}

const AXIS_TO_ANGLES_ROLL_PITCH_NEG: f32 = -90.0;

const AXIS_TO_ANGLES_ROLL_FLIP_POS: f32 = 180.0;

const AXIS_TO_ANGLES_ROLL_FLIP_NEG: f32 = -180.0;

fn axis_to_angles_roll_atan(v: [f32; 3]) -> f32 {
    let [x, y, z] = v;
    if y == 0.0 && x == 0.0 {
        return if 0.0 <= -z {
            VECT_TO_ANGLES_PITCH_UP
        } else {
            AXIS_TO_ANGLES_ROLL_PITCH_NEG
        };
    }
    let hypot = libm::sqrtf(x * x + y * y);
    libm::atan2f(z, hypot) * VECT_TO_ANGLES_PITCH_SCALE
}

pub fn axis_to_angles(axis: [[f32; 3]; 3]) -> [f32; 3] {
    let mut angles = vect_to_angles(axis[0]);
    let yaw_rad = -angles[1] * RETAIL_DEGREES_TO_RADIANS;
    let cy = libm::cosf(yaw_rad);
    let sy = libm::sinf(yaw_rad);
    let left = axis[1];
    let yawed_x = cy * left[0] - sy * left[1];
    let yawed_y = left[0] * sy + cy * left[1];
    let pitch_rad = -angles[0] * RETAIL_DEGREES_TO_RADIANS;
    let cp = libm::cosf(pitch_rad);
    let sp = libm::sinf(pitch_rad);
    let unrot = [
        sp * left[2] + cp * yawed_x,
        yawed_y,
        left[2] * cp - sp * yawed_x,
    ];
    let atan = axis_to_angles_roll_atan(unrot);
    angles[2] = if 0.0 <= unrot[1] {
        -atan
    } else if atan < 0.0 {
        atan + AXIS_TO_ANGLES_ROLL_FLIP_POS
    } else {
        atan + AXIS_TO_ANGLES_ROLL_FLIP_NEG
    };
    angles
}

const SNAP_ANGLES_EPS2: f32 = f32::from_bits(0x3586_37be);

const SNAP_ANGLES_ROUND_BIAS: f32 = 0.0;

pub fn snap_angles(angles: [f32; 3]) -> [f32; 3] {
    let mut out = angles;
    for a in &mut out {
        let rounded = libm::roundf(*a + SNAP_ANGLES_ROUND_BIAS);
        let delta = rounded - *a;
        if delta * delta < SNAP_ANGLES_EPS2 {
            *a = rounded;
        }
    }
    out
}

const PITCH_FOR_YAW_VERTICAL: f32 = 270.0;

const PITCH_FOR_YAW_ATAN_NUM: f32 = 180.0;
const PITCH_FOR_YAW_ATAN_DEN: f32 = 3.141592741012573_f32;

pub fn pitch_for_yaw_on_normal(yaw_degrees: f32, normal: [f32; 3]) -> f32 {
    let (forward, _) = yaw_vectors_2d(yaw_degrees);
    if normal[2] == 0.0 {
        return PITCH_FOR_YAW_VERTICAL;
    }
    let t = (normal[0] * forward[0] + normal[1] * forward[1]) / normal[2];
    libm::atanf(t) * PITCH_FOR_YAW_ATAN_NUM / PITCH_FOR_YAW_ATAN_DEN
}

pub fn yaw_vectors_2d(yaw_degrees: f32) -> ([f32; 2], [f32; 2]) {
    let yaw = yaw_degrees * RETAIL_DEGREES_TO_RADIANS;
    let cy = libm::cosf(yaw);
    let sy = libm::sinf(yaw);
    ([cy, sy], [sy, -cy])
}

const RETAIL_RIGHT_AXIS_SIGN: f32 = -1.0;

const RETAIL_DEGREES_TO_TURNS: f32 = 1.0 / 360.0;

const RETAIL_TRACK_ANGLE_LOWER: f32 = 180.0;
const RETAIL_TRACK_ANGLE_UPPER: f32 = -180.0;

const RETAIL_HALF_TURN: f32 = 0.5;

const RETAIL_TURN_DEGREES: f32 = 360.0;

pub fn angle_subtract(a: f32, b: f32) -> f32 {
    let scaled = (a - b) * RETAIL_DEGREES_TO_TURNS;
    let turns = retail_floor(scaled + RETAIL_HALF_TURN);
    (scaled - turns) * RETAIL_TURN_DEGREES
}

fn retail_floor(value: f32) -> f32 {
    let bits = value.to_bits();
    let sign = bits >> 31;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let fraction_bits = bits & 0x007f_ffff;

    if exponent == 0xff {
        return value;
    }
    if exponent < 127 {
        if (bits & 0x7fff_ffff) == 0 {
            return value;
        }
        return if sign == 0 { 0.0 } else { -1.0 };
    }

    let fractional_bits = 150 - exponent;
    if fractional_bits <= 0 {
        return value;
    }

    let mask = (1u32 << fractional_bits) - 1;
    let truncated = f32::from_bits(bits & !mask);
    if sign != 0 && (fraction_bits & mask) != 0 {
        truncated - 1.0
    } else {
        truncated
    }
}
