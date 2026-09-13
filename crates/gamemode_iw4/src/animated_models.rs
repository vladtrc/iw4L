pub const ANIMATED_MODEL_TARGETNAME: &str = "animated_model";

pub const FAN_BLADE_ROTATE_TARGETNAME: &str = "com_wall_fan_blade_rotate";

pub const FAN_BLADE_ROTATE_FAST_TARGETNAME: &str = "com_wall_fan_blade_rotate_fast";

pub const TOY_CEILING_FAN_TYPE: &str = "toy_ceiling_fan";

pub const TOY_WALL_FAN_TYPE: &str = "toy_wall_fan";

pub const TOY_CEILING_FAN_IDLE_MPANIM: &str = "me_fanceil1_spin";

pub const TOY_WALL_FAN_IDLE_MPANIM: &str = "wall_fan_rotate";

pub fn animprop_machine(targetname: &str, destructible_type: &str) -> Option<&'static str> {
    if targetname == ANIMATED_MODEL_TARGETNAME {
        return Some("animated_model");
    }
    if targetname == FAN_BLADE_ROTATE_TARGETNAME || targetname == FAN_BLADE_ROTATE_FAST_TARGETNAME {
        return Some("fan_blade");
    }
    if destructible_type == TOY_CEILING_FAN_TYPE || destructible_type == TOY_WALL_FAN_TYPE {
        return Some("toy_fan");
    }
    None
}

pub const FAN_BLADE_ROTATE_TIME: f32 = 20000.0;

pub const FAN_BLADE_AXIS_DOT: f32 = 0.9;

pub const FAN_BLADE_SLOW_SPEED_MIN: f32 = 100.0;

pub const FAN_BLADE_SLOW_SPEED_MAX: f32 = 360.0;

pub const FAN_BLADE_FAST_SPEED_MIN: f32 = 720.0;

pub const FAN_BLADE_FAST_SPEED_MAX: f32 = 1000.0;

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

pub fn fan_blade_speed_bounds(fast: bool) -> (f32, f32) {
    if fast {
        (FAN_BLADE_FAST_SPEED_MIN, FAN_BLADE_FAST_SPEED_MAX)
    } else {
        (FAN_BLADE_SLOW_SPEED_MIN, FAN_BLADE_SLOW_SPEED_MAX)
    }
}

pub fn mp_clip_for_toy_fan(destructible_type: &str) -> Option<&'static str> {
    match destructible_type {
        TOY_CEILING_FAN_TYPE => Some(TOY_CEILING_FAN_IDLE_MPANIM),
        TOY_WALL_FAN_TYPE => Some(TOY_WALL_FAN_IDLE_MPANIM),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimPropModel {
    pub model: &'static str,
    pub clip: &'static str,
}

pub const ANIM_PROP_MODELS: &[AnimPropModel] = &[
    AnimPropModel {
        model: "foliage_cod5_tree_jungle_01_animated",
        clip: "foliage_cod5_tree_jungle_01_sway",
    },
    AnimPropModel {
        model: "foliage_cod5_tree_jungle_02_animated",
        clip: "foliage_cod5_tree_jungle_02_sway",
    },
    AnimPropModel {
        model: "foliage_cod5_tree_jungle_03_animated",
        clip: "foliage_cod5_tree_jungle_03_sway",
    },
    AnimPropModel {
        model: "foliage_desertbrush_1_animated",
        clip: "foliage_desertbrush_1_sway",
    },
    AnimPropModel {
        model: "foliage_pacific_bushtree02_animated",
        clip: "foliage_pacific_bushtree01_sway",
    },
    AnimPropModel {
        model: "foliage_tree_oak_1_animated2",
        clip: "foliage_tree_oak_1_sway",
    },
];

pub fn mp_clip_for_animated_model(model: &str) -> Option<&'static str> {
    ANIM_PROP_MODELS
        .iter()
        .find(|row| row.model == model)
        .map(|row| row.clip)
}
