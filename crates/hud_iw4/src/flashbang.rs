extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

pub const SCREEN_BLEND_BLURRED: i32 = 0;

pub const SCREEN_BLEND_FLASHED: i32 = 1;

pub const SCREEN_BLEND_NONE: i32 = 2;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockLookParms {
    pub affect: bool,

    pub fade_ms: i32,

    pub mouse_sensitivity: f32,

    pub max_pitch_speed: f32,

    pub max_yaw_speed: f32,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellshockSoundParms {
    pub affect: bool,

    pub loop_alias: String,

    pub end_alias: String,

    pub abort_alias: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShockParams {
    pub screen_type: i32,

    pub white_fade_ms: i32,

    pub shot_fade_ms: i32,

    pub look: ShellshockLookParms,

    pub sound: ShellshockSoundParms,

    pub movement: bool,
}

impl ShockParams {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            values.insert(key.to_ascii_lowercase(), value.to_owned());
        }
        let text = |key: &str| {
            values
                .get(&format!("bg_shock_{}", key.to_ascii_lowercase()))
                .cloned()
                .ok_or_else(|| format!("bg_shock_{key} is missing"))
        };
        let number = |key: &str| -> Result<f32, String> {
            let value = text(key)?;
            value
                .parse::<f32>()
                .map_err(|_| format!("bg_shock_{key} \"{value}\" is not a number"))
        };
        let ms = |key: &str| number(key).map(|seconds| libm::roundf(seconds * 1000.0) as i32);
        let flag = |key: &str| number(key).map(|value| value != 0.0);
        let screen_type = match text("screenType")?.to_ascii_lowercase().as_str() {
            "blurred" => SCREEN_BLEND_BLURRED,
            "flashed" => SCREEN_BLEND_FLASHED,
            "none" => SCREEN_BLEND_NONE,
            other => {
                return Err(format!(
                    "bg_shock_screenType \"{other}\" is not blurred, flashed or none"
                ));
            }
        };
        Ok(Self {
            screen_type,
            white_fade_ms: ms("screenFlashWhiteFadeTime")?,
            shot_fade_ms: ms("screenFlashShotFadeTime")?,
            look: ShellshockLookParms {
                affect: flag("lookControl")?,
                fade_ms: ms("lookControl_fadeTime")?,
                mouse_sensitivity: number("lookControl_mousesensitivityscale")?,
                max_pitch_speed: number("lookControl_maxpitchspeed")?,
                max_yaw_speed: number("lookControl_maxyawspeed")?,
            },
            sound: ShellshockSoundParms {
                affect: flag("sound")?,
                loop_alias: text("soundLoop")?,
                end_alias: text("soundEnd")?,
                abort_alias: text("soundEndAbort")?,
            },
            movement: flag("movement")?,
        })
    }
}
