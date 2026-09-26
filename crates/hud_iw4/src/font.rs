extern crate alloc;

use alloc::string::String;

pub const R_TEXT_EM: f32 = 48.0;

pub const HUDELEM_FONT_DEFAULT_BASE_SCALE: f32 = 0.25;

pub const HUDELEM_FONT_HALF_BASE_SCALE: f32 = 0.5;

pub const HUDELEM_FONT_THIRD_BASE_SCALE: f32 = 1.0 / 3.0;

#[must_use]
pub fn hudelem_font_ui_enum(elem_font: i32) -> i32 {
    hudelem_font_info(elem_font).0
}

#[must_use]
pub fn hudelem_font_base_scale(elem_font: i32) -> f32 {
    hudelem_font_info(elem_font).1
}

#[must_use]
pub fn hudelem_text_scale(elem_font: i32, elem_font_scale: f32) -> f32 {
    hudelem_font_base_scale(elem_font) * elem_font_scale
}

#[must_use]
pub fn hudelem_em_px(elem_font: i32, elem_font_scale: f32, scale_virtual_to_real_y: f32) -> f32 {
    ui_text_height(hudelem_text_scale(elem_font, elem_font_scale)) * scale_virtual_to_real_y
}

fn hudelem_font_info(elem_font: i32) -> (i32, f32) {
    match elem_font {
        1 => (4, HUDELEM_FONT_HALF_BASE_SCALE),
        2 => (5, HUDELEM_FONT_THIRD_BASE_SCALE),
        3 => (6, HUDELEM_FONT_DEFAULT_BASE_SCALE),
        4 => (2, HUDELEM_FONT_DEFAULT_BASE_SCALE),
        5 => (3, HUDELEM_FONT_DEFAULT_BASE_SCALE),
        6 => (9, HUDELEM_FONT_HALF_BASE_SCALE),
        7 => (10, HUDELEM_FONT_THIRD_BASE_SCALE),
        8 => (8, HUDELEM_FONT_DEFAULT_BASE_SCALE),
        _ => (0, HUDELEM_FONT_DEFAULT_BASE_SCALE),
    }
}

const UI_SMALL_FONT: f32 = 0.25;

const UI_BIG_FONT: f32 = 0.4;

const UI_EXTRA_BIG_FONT: f32 = 0.55;

pub fn ui_get_font_handle(font_enum: i32, placement_scale: f32, text_scale: f32) -> &'static str {
    match font_enum {
        2 => "fonts/bigfont",
        3 => "fonts/smallfont",
        4 => "fonts/boldfont",
        5 => "fonts/consolefont",
        6 => "fonts/objectivefont",
        7 => "fonts/normalfont",
        8 => "fonts/extrabigfont",
        9 => "fonts/hudbigfont",
        10 => "fonts/hudsmallfont",
        _ => {
            let scale = placement_scale * text_scale;
            if scale <= UI_SMALL_FONT {
                "fonts/smallfont"
            } else if scale > UI_EXTRA_BIG_FONT {
                "fonts/extrabigfont"
            } else if scale > UI_BIG_FONT {
                "fonts/bigfont"
            } else {
                "fonts/normalfont"
            }
        }
    }
}

pub fn r_normalized_text_scale(pixel_height: i32, text_scale: f32) -> f32 {
    if pixel_height <= 0 {
        return 0.0;
    }
    text_scale * R_TEXT_EM / pixel_height as f32
}

pub fn ui_text_height(text_scale: f32) -> f32 {
    text_scale * R_TEXT_EM
}

pub fn item_text_origin(
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
    text_align_mode: i32,
    text_align_x: f32,
    text_align_y: f32,
    measured_w: f32,
    measured_h: f32,
) -> (f32, f32) {
    let mut x = text_align_x;
    let h_mode = text_align_mode & 3;
    if h_mode != 0 {
        let mut d = rect_w - measured_w;
        if h_mode == 1 {
            d *= 0.5;
        }
        x += d;
    }
    x += rect_x;
    let y =
        item_get_text_placement_y(text_align_mode & 0xc, text_align_y, rect_h, measured_h) + rect_y;
    (x, y)
}

pub fn item_get_text_placement_y(align_y: i32, y0: f32, container_h: f32, self_h: f32) -> f32 {
    match align_y {
        0 => y0,
        4 => y0 + self_h,
        8 => y0 + (container_h + self_h) * 0.5,
        _ => y0 + container_h,
    }
}

pub fn next_letter(chars: &mut core::iter::Peekable<core::str::Chars<'_>>) -> Option<u32> {
    loop {
        let c = chars.next()?;
        if c == '^' {
            if let Some(n) = chars.peek().copied() {
                if n.is_ascii_digit() {
                    chars.next();
                    continue;
                }
            }
        }
        return Some(c as u32);
    }
}

pub const G_COLOR_TABLE: [[f32; 4]; 8] = [
    [0.0, 0.0, 0.0, 1.0],
    [1.0, 0.36, 0.36, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [0.0, 1.0, 1.0, 1.0],
    [1.0, 0.36, 1.0, 1.0],
    [1.0, 1.0, 1.0, 1.0],
];

#[must_use]
pub fn color_from_caret_digit(digit: char) -> [f32; 4] {
    let Some(i) = digit.to_digit(10) else {
        return [1.0, 1.0, 1.0, 1.0];
    };
    G_COLOR_TABLE
        .get(i as usize)
        .copied()
        .unwrap_or([1.0, 1.0, 1.0, 1.0])
}

pub fn seconds_to_countdown_display(seconds: i32) -> String {
    if seconds < 0 {
        return String::new();
    }
    alloc::format!("{:>2}:{:02}", seconds / 60, seconds % 60)
}
