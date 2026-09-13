pub const HUDELEM_STRIDE: usize = 0xa8;

pub const HUDELEM_BANK_CAPACITY: usize = 0x1f;

pub const PLAYERSTATE_HUD_CURRENT: usize = 0x848;

pub const PLAYERSTATE_HUD_ARCHIVAL: usize = 0x1ca0;

pub const PLAYERSTATE_HUD_BANKS_END: usize = 0x30f8;

pub const HUDELEM_ARCHIVAL_REMAPPED_TIMES: &[(usize, &'static str)] = &[
    (0x7c, "time"),
    (0x3c, "fadeStartTime"),
    (0x70, "scaleStartTime"),
    (0x60, "moveStartTime"),
    (0x1c, "fontScaleStartTime"),
];

pub const GAME_HUDELEM_STRIDE: usize = 0xb4;

pub const GAME_HUDELEM_CAPACITY: usize = 0x400;

pub const GAME_HUDELEM_ARCHIVED: usize = 0xb0;

pub const HUDELEM_TYPE_NAMES: &[&str] = &[
    "free",
    "text",
    "value",
    "playername",
    "material",
    "timer down",
    "timer up",
    "timer static",
    "timer down tenths",
    "timer up tenths",
    "timer static tenths",
    "clock down",
    "clock up",
    "waypoint",
];

pub mod flags {
    pub const FOREGROUND: i32 = 0x1;

    pub const HIDEWHENDEAD: i32 = 0x2;

    pub const HIDEWHENINMENU: i32 = 0x4;

    pub const WAYPOINT_DRAW: i32 = 0x8;

    pub const SPLATTER: i32 = 0x80;

    pub const LOWRESBACKGROUND: i32 = 0x100;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudElem {
    pub elem_type: i32,
    pub y: f32,
    pub x: f32,
    pub z: f32,
    pub target_ent_num: i32,
    pub font_scale: f32,
    pub from_font_scale: f32,
    pub font_scale_start_time: i32,
    pub font_scale_time: i32,
    pub label: i32,
    pub font: i32,
    pub align_org: i32,
    pub align_screen: i32,
    pub color_rgba: u32,
    pub from_color_rgba: u32,
    pub fade_start_time: i32,
    pub fade_time: i32,
    pub height: i32,
    pub width: i32,
    pub material_index: i32,
    pub from_y: f32,
    pub from_x: f32,
    pub from_align_org: i32,
    pub from_align_screen: i32,
    pub move_start_time: i32,
    pub move_time: i32,
    pub from_height: i32,
    pub from_width: i32,
    pub scale_start_time: i32,
    pub scale_time: i32,
    pub value: f32,
    pub time: i32,
    pub duration: i32,
    pub text: i32,
    pub sort: f32,
    pub glow_color_rgba: u32,
    pub fx_birth_time: i32,
    pub fx_letter_time: i32,
    pub fx_decay_start_time: i32,
    pub fx_decay_duration: i32,
    pub sound_id: i32,
    pub flags: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameHudElem {
    pub archived: i32,
}

const _: () = {
    assert!(
        PLAYERSTATE_HUD_CURRENT + HUDELEM_BANK_CAPACITY * HUDELEM_STRIDE
            == PLAYERSTATE_HUD_ARCHIVAL
    );
    assert!(
        PLAYERSTATE_HUD_CURRENT + 2 * HUDELEM_BANK_CAPACITY * HUDELEM_STRIDE
            == PLAYERSTATE_HUD_BANKS_END
    );
    assert!(HUDELEM_ARCHIVAL_REMAPPED_TIMES.len() == 5);
};

pub const HE_TYPE_FREE: i32 = 0;

pub const HE_TYPE_TEXT: i32 = 1;

pub const HE_TYPE_VALUE: i32 = 2;

pub const HE_TYPE_PLAYERNAME: i32 = 3;

pub const HE_TYPE_MATERIAL: i32 = 4;

pub const HORZ_ALIGN_CENTER: i32 = 2;

pub const VERT_ALIGN_MIDDLE: i32 = 2;

pub const ALIGN_SCREEN_HORZ_SHIFT: i32 = 3;

#[must_use]
pub const fn align_screen(horz: i32, vert: i32) -> i32 {
    (horz << ALIGN_SCREEN_HORZ_SHIFT) | vert
}

pub const DAMAGE_FEEDBACK_ALIGN_SCREEN: i32 = align_screen(HORZ_ALIGN_CENTER, VERT_ALIGN_MIDDLE);

pub const SCORE_POPUP_ALIGN_SCREEN: i32 = DAMAGE_FEEDBACK_ALIGN_SCREEN;

pub const MATCH_START_ALIGN_SCREEN: i32 = DAMAGE_FEEDBACK_ALIGN_SCREEN;

pub const VERT_ALIGN_TOP: i32 = 1;

pub const OUTCOME_ALIGN_SCREEN: i32 = align_screen(HORZ_ALIGN_CENTER, VERT_ALIGN_TOP);

#[must_use]
pub const fn color_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24)
}

#[must_use]
pub const fn unpack_rgba(dword: u32) -> [u8; 4] {
    [
        (dword & 0xff) as u8,
        ((dword >> 8) & 0xff) as u8,
        ((dword >> 16) & 0xff) as u8,
        ((dword >> 24) & 0xff) as u8,
    ]
}

#[must_use]
pub fn bg_lerp_hud_colors(elem: &HudElem, time: i32) -> [u8; 4] {
    let elapsed = time.wrapping_sub(elem.fade_start_time);
    let dur = elem.fade_time;
    if dur <= 0 || elapsed >= dur {
        return unpack_rgba(elem.color_rgba);
    }
    let elapsed = if elapsed < 0 { 0 } else { elapsed };
    let lerp = elapsed as f32 / dur as f32;
    let from = unpack_rgba(elem.from_color_rgba);
    let to = unpack_rgba(elem.color_rgba);
    [
        lerp_channel(from[0], to[0], lerp),
        lerp_channel(from[1], to[1], lerp),
        lerp_channel(from[2], to[2], lerp),
        lerp_channel(from[3], to[3], lerp),
    ]
}

#[must_use]
pub fn hud_elem_lerp_font_scale(elem: &HudElem, time: i32) -> f32 {
    let elapsed = time.wrapping_sub(elem.font_scale_start_time);
    let duration = elem.font_scale_time;
    if duration <= 0 || elapsed >= duration {
        return elem.font_scale;
    }
    let elapsed = elapsed.max(0);
    elem.from_font_scale
        + (elem.font_scale - elem.from_font_scale) * elapsed as f32 / duration as f32
}

fn lerp_channel(from: u8, to: u8, lerp: f32) -> u8 {
    let v = from as f32 + lerp * (to as i32 - from as i32) as f32;
    let rounded = libm::roundf(v);
    if rounded <= 0.0 {
        0
    } else if rounded >= 255.0 {
        255
    } else {
        rounded as u8
    }
}

#[must_use]
pub fn copy_in_use_prefix(src: &[HudElem]) -> &[HudElem] {
    let n = match src.iter().position(|elem| elem.elem_type == HE_TYPE_FREE) {
        Some(i) => i,
        None => src.len(),
    };
    &src[..n]
}

pub fn rebase_archival_times(elem: &mut HudElem, rebase_ms: i32) {
    if elem.time != 0 {
        elem.time = elem.time.saturating_add(rebase_ms);
    }
    if elem.fade_start_time != 0 {
        elem.fade_start_time = elem.fade_start_time.saturating_add(rebase_ms);
    }
    if elem.scale_start_time != 0 {
        elem.scale_start_time = elem.scale_start_time.saturating_add(rebase_ms);
    }
    if elem.move_start_time != 0 {
        elem.move_start_time = elem.move_start_time.saturating_add(rebase_ms);
    }
    if elem.font_scale_start_time != 0 {
        elem.font_scale_start_time = elem.font_scale_start_time.saturating_add(rebase_ms);
    }
}
