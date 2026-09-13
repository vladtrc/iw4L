extern crate alloc;

pub const CG_OWNERDRAW_MANTLE: i32 = 80;

pub const HINT_MANTLE_MATERIAL: &str = "hint_mantle";

pub const PLATFORM_MANTLE: &str = "PLATFORM_MANTLE";

pub const MANTLE_HINT_FLAG: u32 = 8;

const HALF: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleHintLayout {
    pub text_x: f32,
    pub text_y: f32,
    pub pic_x: f32,
    pub pic_y: f32,
}

#[must_use]
pub fn cg_draw_mantle_hint_visible(mantle_flags: u32) -> bool {
    (mantle_flags & MANTLE_HINT_FLAG) != 0
}

#[must_use]
pub fn cg_draw_mantle_hint_layout(
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
    text_width: f32,
    text_height: f32,
) -> MantleHintLayout {
    let text_x = rect_x - (rect_w + text_width) * HALF;
    let text_y = text_height * HALF + rect_y;
    MantleHintLayout {
        text_x,
        text_y,
        pic_x: text_x + text_width,
        pic_y: rect_y - rect_h * HALF,
    }
}

#[must_use]
pub fn mantle_hint_replace_bind(template: &str, bind: &str) -> alloc::string::String {
    crate::centerprint_replace_name(template, bind)
}
