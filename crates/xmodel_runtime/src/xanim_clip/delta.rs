use super::{AnimClip, Rotation};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootMotionDelta {
    pub translation: [f32; 3],
    pub rotation: [f32; 2],
}

impl Default for RootMotionDelta {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 1.0],
        }
    }
}

impl RootMotionDelta {
    pub fn yaw(self) -> f32 {
        let [z, w] = self.rotation;
        let norm = z * z + w * w;
        if norm == 0.0 {
            return 0.0;
        }
        let scale = 2.0 / norm;
        let sine = z * w * scale;
        let cosine = 1.0 - z * z * scale;
        (f64::from(sine).atan2(f64::from(cosine)) * 180.0 / core::f64::consts::PI) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootMotionPose {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}
impl Default for RootMotionPose {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: anim_iw4::QUAT_IDENTITY,
        }
    }
}
impl RootMotionPose {
    pub fn world_pose(self, origin: [f32; 3], axis: [[f32; 3]; 3]) -> ([f32; 3], [f32; 3]) {
        let [x, y, z, w] = self.rotation;
        let norm = x * x + y * y + z * z + w * w;
        let s = if norm == 0.0 { 0.0 } else { 2.0 / norm };
        let local = [
            [
                1.0 - (y * y + z * z) * s,
                (x * y + z * w) * s,
                (x * z - y * w) * s,
            ],
            [
                (x * y - z * w) * s,
                1.0 - (x * x + z * z) * s,
                (y * z + x * w) * s,
            ],
            [
                (x * z + y * w) * s,
                (y * z - x * w) * s,
                1.0 - (x * x + y * y) * s,
            ],
        ];
        (
            math_iw4::matrix_transform_vector43(self.translation, axis, origin),
            math_iw4::axis_to_angles(math_iw4::matrix_multiply(local, axis)),
        )
    }
}

impl AnimClip {
    pub fn absolute_delta(&self, time: f32) -> Option<RootMotionPose> {
        let rotation = if self.delta_enabled {
            let [z, w] = self.abs_delta_rotation(time);
            [0.0, 0.0, z, w]
        } else if self.delta_3d_enabled {
            self.abs_delta_rotation_3d(time)
        } else {
            return None;
        };
        Some(RootMotionPose {
            translation: self.abs_delta_trans(time),
            rotation,
        })
    }

    pub fn abs_delta_rotation(&self, frac: f32) -> [f32; 2] {
        let value = sample_delta_rotation(&self.delta_rotation, frac, self.numframes);
        [value[2], value[3]]
    }

    pub fn abs_delta_rotation_3d(&self, frac: f32) -> [f32; 4] {
        sample_delta_rotation(&self.delta_rotation_3d, frac, self.numframes)
    }

    pub fn relative_delta(&self, start: f32, end: f32) -> RootMotionDelta {
        if !self.delta_enabled {
            return RootMotionDelta::default();
        }
        let first = self.abs_delta_trans(start);
        let last = self.abs_delta_trans(end);
        let mut translation = std::array::from_fn(|axis| last[axis] - first[axis]);
        if self.looping && start > end {
            let initial = self.abs_delta_trans(0.0);
            let terminal = self.abs_delta_trans(1.0);
            for axis in 0..3 {
                translation[axis] += terminal[axis] - initial[axis];
            }
        }
        let [z0, w0] = self.abs_delta_rotation(start);
        let [z1, w1] = self.abs_delta_rotation(end);
        let rotation = [z1 * w0 - w1 * z0, z1 * z0 + w1 * w0];
        let norm = z0 * z0 + w0 * w0;
        if norm != 0.0 {
            let scale = 2.0 / norm;
            let zz = z0 * z0 * scale;
            let zw = z0 * w0 * scale;
            let x = (1.0 - zz) * translation[0] + zw * translation[1];
            translation[1] -= zw * translation[0] + zz * translation[1];
            translation[0] = x;
        }
        RootMotionDelta {
            translation,
            rotation: if rotation == [0.0; 2] {
                [0.0, 1.0]
            } else {
                rotation
            },
        }
    }
}

fn sample_delta_rotation(rotation: &Rotation, frac: f32, frames: u16) -> [f32; 4] {
    if frac == 1.0 || frames == 0 {
        return match rotation {
            Rotation::Default => anim_iw4::QUAT_IDENTITY,
            Rotation::HalfConstant(value) | Rotation::FullConstant(value) => *value,
            Rotation::HalfKeyed(keys) | Rotation::FullKeyed(keys) => keys
                .values
                .last()
                .copied()
                .unwrap_or(anim_iw4::QUAT_IDENTITY),
        };
    }
    rotation
        .sample(frac.clamp(0.0, 1.0) * f32::from(frames))
        .unwrap_or(anim_iw4::QUAT_IDENTITY)
}
