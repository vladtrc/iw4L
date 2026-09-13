use crate::quat::{QUAT_IDENTITY, Quat, VEC3_ZERO, Vec3, vec3_add_scaled};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Local {
    pub rotation: Quat,

    pub control: bool,

    pub translation: Vec3,
}

impl Local {
    pub const fn uncontrolled(bind_rotation: Quat) -> Self {
        Self {
            rotation: bind_rotation,
            translation: VEC3_ZERO,
            control: false,
        }
    }

    pub const fn identity() -> Self {
        Self {
            rotation: QUAT_IDENTITY,
            translation: VEC3_ZERO,
            control: false,
        }
    }
}

pub fn compose_translation(bind_translation: Vec3, scale: f32, anim_delta: Vec3) -> Vec3 {
    vec3_add_scaled(bind_translation, anim_delta, scale)
}

pub fn compose_rotation(anim_rotation: Quat) -> Quat {
    anim_rotation
}
