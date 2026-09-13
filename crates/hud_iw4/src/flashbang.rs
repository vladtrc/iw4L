pub const SCREEN_BLEND_FLASHED: i32 = 1;

#[must_use]
pub fn cg_is_flashbanged(cg_time: i32, start_time: i32, duration: i32, screen_type: i32) -> i32 {
    let remaining = start_time.wrapping_sub(cg_time).wrapping_add(duration);
    if remaining < 1 {
        0
    } else if screen_type != SCREEN_BLEND_FLASHED {
        0
    } else {
        remaining
    }
}

pub const FLASHBANG_WHITE_FADE_MS: i32 = 3500;

pub const FLASHBANG_SHOT_FADE_MS: i32 = 1000;

const FLASH_FADE_HALF: f32 = 0.5;

const FLASH_FADE_PI: f32 = 3.141592741012573;

#[must_use]
pub fn cg_shellshock_flash_fade_sin_cos(percent: f32) -> f32 {
    let s = libm::sinf((percent - FLASH_FADE_HALF) * FLASH_FADE_PI);
    (s + 1.0) * FLASH_FADE_HALF
}

#[must_use]
pub fn cg_shellshock_flash_blend(
    remaining_ms: i32,
    white_fade_ms: i32,
    shot_fade_ms: i32,
) -> Option<(f32, f32)> {
    if remaining_ms < 1 {
        return None;
    }
    let dt = remaining_ms as f32;
    let white_lin = if white_fade_ms <= 0 || (white_fade_ms as f32) <= dt {
        1.0
    } else {
        dt / white_fade_ms as f32
    };
    let shot_lin = if shot_fade_ms <= 0 || (shot_fade_ms as f32) <= dt {
        1.0
    } else {
        dt / shot_fade_ms as f32
    };
    Some((
        cg_shellshock_flash_fade_sin_cos(white_lin),
        cg_shellshock_flash_fade_sin_cos(shot_lin),
    ))
}

pub const HOST_SHOCK_FLASHBANG_MP: i32 = 0;

pub const HOST_SHOCK_CONCUSSION_GRENADE_MP: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockLookParms {
    pub affect: bool,

    pub fade_ms: i32,

    pub mouse_sensitivity: f32,

    pub max_pitch_speed: f32,

    pub max_yaw_speed: f32,
}

pub const FLASHBANG_LOOK_PARMS: ShellshockLookParms = ShellshockLookParms {
    affect: false,
    fade_ms: 2000,
    mouse_sensitivity: 0.5,
    max_pitch_speed: 90.0,
    max_yaw_speed: 90.0,
};

pub const CONCUSSION_LOOK_PARMS: ShellshockLookParms = ShellshockLookParms {
    affect: true,
    fade_ms: 2000,
    mouse_sensitivity: 0.1,
    max_pitch_speed: 75.0,
    max_yaw_speed: 75.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockLookState {
    pub sensitivity: f32,

    pub max_pitch_speed: f32,

    pub max_yaw_speed: f32,
}

const LOOK_ENDED: ShellshockLookState = ShellshockLookState {
    sensitivity: 1.0,
    max_pitch_speed: 0.0,
    max_yaw_speed: 0.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellshockSoundParms {
    pub affect: bool,

    pub loop_alias: &'static str,

    pub end_alias: &'static str,

    pub abort_alias: &'static str,
}

pub const FLASHBANG_SOUND_PARMS: ShellshockSoundParms = ShellshockSoundParms {
    affect: true,
    loop_alias: "flashbang_tinnitus_loop",
    end_alias: "flashbang_tinnitus_end",
    abort_alias: "flashbang_tinnitus_abort",
};

pub const CONCUSSION_SOUND_PARMS: ShellshockSoundParms = FLASHBANG_SOUND_PARMS;

#[must_use]
pub fn shellshock_sound_parms(shellshock_index: i32) -> ShellshockSoundParms {
    if shellshock_index == HOST_SHOCK_CONCUSSION_GRENADE_MP {
        CONCUSSION_SOUND_PARMS
    } else {
        FLASHBANG_SOUND_PARMS
    }
}

#[must_use]
pub fn shellshock_remaining_ms(cg_time: i32, start_time: i32, duration: i32) -> i32 {
    if start_time == 0 {
        return 0;
    }
    let elapsed = cg_time.wrapping_sub(start_time);
    if elapsed < 0 {
        return 0;
    }
    let remaining = duration.wrapping_sub(elapsed);
    if remaining < 1 { 0 } else { remaining }
}

#[must_use]
pub fn shellshock_look_parms(shellshock_index: i32) -> ShellshockLookParms {
    if shellshock_index == HOST_SHOCK_CONCUSSION_GRENADE_MP {
        CONCUSSION_LOOK_PARMS
    } else {
        FLASHBANG_LOOK_PARMS
    }
}

#[must_use]
pub fn update_shellshock_look_control(
    cg_time: i32,
    start_time: i32,
    duration: i32,
    parms: ShellshockLookParms,
) -> ShellshockLookState {
    let elapsed = cg_time.wrapping_sub(start_time);
    if start_time == 0 || elapsed < 0 || !parms.affect {
        return LOOK_ENDED;
    }
    let remaining = duration.wrapping_sub(elapsed);
    if remaining < parms.fade_ms {
        if remaining < 1 {
            return LOOK_ENDED;
        }
        let fade = remaining as f32 / parms.fade_ms as f32;
        if fade == 1.0 {
            return ShellshockLookState {
                sensitivity: parms.mouse_sensitivity,
                max_pitch_speed: parms.max_pitch_speed,
                max_yaw_speed: parms.max_yaw_speed,
            };
        }
        ShellshockLookState {
            sensitivity: fade * (parms.mouse_sensitivity - 1.0) + 1.0,
            max_pitch_speed: parms.max_pitch_speed / fade,
            max_yaw_speed: parms.max_yaw_speed / fade,
        }
    } else {
        ShellshockLookState {
            sensitivity: parms.mouse_sensitivity,
            max_pitch_speed: parms.max_pitch_speed,
            max_yaw_speed: parms.max_yaw_speed,
        }
    }
}
