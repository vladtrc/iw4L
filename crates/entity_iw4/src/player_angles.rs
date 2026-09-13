use math_iw4::{angle_normalize_360, angle_subtract};

pub const PLAYER_MOVE_FACTOR_ON_TORSO: f32 = 0.0;

pub const BG_SWING_SPEED: f32 = 0.2;

pub const BG_LEG_YAW_TOLERANCE: f32 = 20.0;

pub const LEGS_YAW_CLAMP: f32 = 150.0;

pub const TORSO_YAW_CLAMP: f32 = 90.0;

const SWING_SCALE_K: f32 = 0.050_000_000_745_058_06;

const SWING_SCALE_MIN: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingState {
    pub angle: f32,
    pub swinging: bool,
}

impl SwingState {
    pub const fn new(angle: f32) -> Self {
        Self {
            angle,
            swinging: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerAngleDvars {
    pub move_factor_on_torso: f32,
    pub swing_speed: f32,
    pub leg_yaw_tolerance: f32,
}

impl Default for PlayerAngleDvars {
    fn default() -> Self {
        Self {
            move_factor_on_torso: PLAYER_MOVE_FACTOR_ON_TORSO,
            swing_speed: BG_SWING_SPEED,
            leg_yaw_tolerance: BG_LEG_YAW_TOLERANCE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerAngleInput {
    pub base_yaw: f32,

    pub movement_yaw: f32,
    pub dvars: PlayerAngleDvars,

    pub frametime_ms: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerAngleOutput {
    pub torso: SwingState,
    pub legs: SwingState,

    pub torso_dest: f32,

    pub legs_dest: f32,
}

pub fn cg_swing_angles(
    destination: f32,
    swing_tolerance: f32,
    clamp_tolerance: f32,
    speed: f32,
    frametime_ms: f32,
    state: &mut SwingState,
) {
    if !state.swinging {
        let delta = angle_subtract(state.angle, destination);
        if delta <= swing_tolerance && -swing_tolerance <= delta {
            return;
        }
        state.swinging = true;
    }

    let swing = angle_subtract(destination, state.angle);
    let mut scale = swing.abs() * SWING_SCALE_K;
    if scale < SWING_SCALE_MIN {
        scale = SWING_SCALE_MIN;
    }

    let step_angle = if swing < 0.0 {
        let mut step = -frametime_ms * scale * speed;
        if swing < step {
            state.swinging = true;
        } else {
            step = swing;
            state.swinging = false;
        }
        step
    } else {
        let mut step = frametime_ms * scale * speed;
        if step < swing {
            state.swinging = true;
        } else {
            step = swing;
            state.swinging = false;
        }
        step
    };

    state.angle = angle_normalize_360(state.angle + step_angle);

    let after = angle_subtract(destination, state.angle);
    if after > clamp_tolerance {
        state.angle = angle_normalize_360(destination - clamp_tolerance);
    } else if after < -clamp_tolerance {
        state.angle = angle_normalize_360(destination + clamp_tolerance);
    }
}

pub fn cg_player_angles(
    input: PlayerAngleInput,
    torso: &mut SwingState,
    legs: &mut SwingState,
) -> PlayerAngleOutput {
    let base = angle_normalize_360(input.base_yaw);
    let movement = input.movement_yaw;
    let torso_dest = input.dvars.move_factor_on_torso * movement + base;
    let legs_dest = angle_normalize_360(base + movement);

    cg_swing_angles(
        torso_dest,
        0.0,
        TORSO_YAW_CLAMP,
        input.dvars.swing_speed,
        input.frametime_ms,
        torso,
    );

    let leg_tol = if legs.swinging {
        0.0
    } else {
        input.dvars.leg_yaw_tolerance
    };
    cg_swing_angles(
        legs_dest,
        leg_tol,
        LEGS_YAW_CLAMP,
        input.dvars.swing_speed,
        input.frametime_ms,
        legs,
    );

    PlayerAngleOutput {
        torso: *torso,
        legs: *legs,
        torso_dest,
        legs_dest,
    }
}

pub fn legs_offset_deg(torso_yaw: f32, legs_yaw: f32) -> f32 {
    angle_subtract(legs_yaw, torso_yaw)
}
