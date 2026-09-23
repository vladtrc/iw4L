pub const SCORE_POPUP_X: f32 = 0.0;

pub const SCORE_POPUP_Y: f32 = -60.0;

pub const SCORE_POPUP_FONT_SCALE: f32 = 0.75;

pub const SCORE_POPUP_MAX_FONT_SCALE: f32 = 3.0;

pub const SCORE_POPUP_PULSE_IN_MS: i32 = 100;

pub const SCORE_POPUP_PULSE_OUT_MS: i32 = 200;

pub const SCORE_POPUP_HOLD_MS: i32 = 1_000;

pub const SCORE_POPUP_FADE_MS: i32 = 750;

pub const SCORE_POPUP_ALPHA: f32 = 0.85;

pub const SCORE_POPUP_RGB: [f32; 3] = [1.0, 1.0, 0.5];

pub const SCORE_POPUP_SORT: f32 = 10_000.0;

pub const SCORE_POPUP_LABEL: &str = "MP_PLUS";

pub const SCORE_POPUP_FONT_INDEX: i32 = 6;

pub const SCORE_POPUP_FONT: &str = "fonts/hudbigfont";

pub const FIRSTBLOOD_SCORE_INFO: i32 = 100;

pub const FIRSTBLOOD_SPLASH_KEY: &str = "firstblood";

#[must_use]
pub fn score_popup_fade_starts_at() -> i32 {
    SCORE_POPUP_HOLD_MS
}

#[must_use]
pub fn score_popup_idle_at() -> i32 {
    SCORE_POPUP_HOLD_MS + SCORE_POPUP_FADE_MS
}
