pub const KC_INFO_WAITING_TO_SPAWN: &str = "MP_WAITING_TO_SPAWN";

pub const KC_INFO_PRESS_TO_SKIP: &str = "PLATFORM_PRESS_TO_SKIP";

pub const KC_INFO_PRESS_TO_RESPAWN: &str = "PLATFORM_PRESS_TO_RESPAWN";

pub const LOWER_TEXT_Y: f32 = 70.0;

pub const LOWER_TEXT_FONT_SIZE: f32 = 1.6;

pub const LOWER_MESSAGE_ALPHA: f32 = 0.85;

pub fn kc_info_loc_key(time_until_respawn: f32, game_ended: bool) -> Option<&'static str> {
    if gsc_truthy_f32(time_until_respawn) && !game_ended {
        if time_until_respawn > 0.0 {
            Some(KC_INFO_WAITING_TO_SPAWN)
        } else {
            Some(KC_INFO_PRESS_TO_SKIP)
        }
    } else if !game_ended {
        Some(KC_INFO_PRESS_TO_RESPAWN)
    } else {
        None
    }
}

fn gsc_truthy_f32(value: f32) -> bool {
    value != 0.0 && !value.is_nan()
}
