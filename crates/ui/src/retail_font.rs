use assets::{FontDef, GlyphCapture, MenuCatalog};
use bevy::prelude::*;

const R_TEXT_EM: f32 = 48.0;

const UI_SMALL_FONT: f32 = 0.25;

const UI_BIG_FONT: f32 = 0.4;

const UI_EXTRA_BIG_FONT: f32 = 0.55;

pub(crate) fn ui_get_font_handle(
    font_enum: i32,
    placement_scale: f32,
    text_scale: f32,
) -> &'static str {
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
            } else if scale >= UI_EXTRA_BIG_FONT {
                "fonts/extrabigfont"
            } else if scale >= UI_BIG_FONT {
                "fonts/bigfont"
            } else {
                "fonts/normalfont"
            }
        }
    }
}

pub(crate) fn r_normalized_text_scale(pixel_height: i32, text_scale: f32) -> f32 {
    if pixel_height <= 0 {
        return 0.0;
    }
    text_scale * R_TEXT_EM / pixel_height as f32
}

pub(crate) fn item_text_origin(
    rect_w: f32,
    rect_h: f32,
    text_align_mode: i32,
    text_align_x: f32,
    text_align_y: f32,
    measured_w: f32,
    measured_h: f32,
) -> (f32, f32) {
    let mut x = text_align_x;
    match text_align_mode & 3 {
        1 => x += (rect_w - measured_w) * 0.5,
        2 | 3 => x += rect_w - measured_w,
        _ => {}
    }
    let y = match text_align_mode & 0xc {
        0 => text_align_y,
        4 => text_align_y + measured_h,
        8 => text_align_y + (rect_h + measured_h) * 0.5,
        _ => text_align_y + rect_h,
    };
    (x, y)
}

pub(crate) fn catalog_font<'a>(
    catalog: &'a MenuCatalog,
    font_enum: i32,
    placement_scale: f32,
    text_scale: f32,
) -> Option<&'a FontDef> {
    catalog.font(ui_get_font_handle(font_enum, placement_scale, text_scale))
}

pub(crate) fn next_letter(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<u32> {
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

pub(crate) fn glyph_screen_quad(
    glyph: &GlyphCapture,
    cursor_x: f32,
    cursor_y: f32,
    scale: f32,
    tex_w: f32,
    tex_h: f32,
) -> Option<(f32, f32, f32, f32, Rect)> {
    let w = glyph.pixel_width as f32 * scale;
    let h = glyph.pixel_height as f32 * scale;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let rect = Rect {
        min: Vec2::new(glyph.s0 * tex_w, glyph.t0 * tex_h),
        max: Vec2::new(glyph.s1 * tex_w, glyph.t1 * tex_h),
    };
    Some((
        cursor_x + glyph.x0 as f32 * scale,
        cursor_y + glyph.y0 as f32 * scale,
        w,
        h,
        rect,
    ))
}
