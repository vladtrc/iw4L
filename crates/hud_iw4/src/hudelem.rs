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

pub const ALIGN_SCREEN_HORZ_SHIFT: i32 = 4;

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

const HUD_ELEM_ORG_ANCHOR: [f32; 4] = [0.0, 0.5, 1.0, 0.0];

const ALIGN_ORG_HORZ_SHIFT: i32 = 2;

const ALIGN_ORG_FIELD: i32 = 3;

pub const ORG_LEADING: i32 = 0;

pub const ORG_MIDDLE: i32 = 1;

pub const ORG_TRAILING: i32 = 2;

#[must_use]
pub const fn align_org(horz: i32, vert: i32) -> i32 {
    (horz << ALIGN_ORG_HORZ_SHIFT) | vert
}

pub const TEXT_CENTERED_ALIGN_ORG: i32 = align_org(ORG_MIDDLE, ORG_LEADING);

const ALIGN_SCREEN_FIELD: i32 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudElemPlacement {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl HudElemPlacement {
    #[must_use]
    pub fn text_baseline_y(&self) -> f32 {
        self.y + self.h
    }
}

#[must_use]
pub const fn hud_elem_screen_align(align_screen: i32) -> (i32, i32) {
    (
        (align_screen >> ALIGN_SCREEN_HORZ_SHIFT) & ALIGN_SCREEN_FIELD,
        align_screen & ALIGN_SCREEN_FIELD,
    )
}

#[must_use]
pub fn hud_elem_movement_frac(elem: &HudElem, time: i32) -> f32 {
    if elem.move_time <= 0 {
        return 1.0;
    }
    let elapsed = time.wrapping_sub(elem.move_start_time);
    if elapsed <= 0 {
        return 0.0;
    }
    if elapsed >= elem.move_time {
        return 1.0;
    }
    elapsed as f32 / elem.move_time as f32
}

#[must_use]
pub fn hud_elem_scale_frac(elem: &HudElem, time: i32) -> Option<f32> {
    if elem.scale_time <= 0 {
        return None;
    }
    let elapsed = time.wrapping_sub(elem.scale_start_time);
    if elapsed >= elem.scale_time {
        return None;
    }
    Some(if elapsed <= 0 {
        0.0
    } else {
        elapsed as f32 / elem.scale_time as f32
    })
}

fn align_hud_elem_axis(align_org: i32, position: f32, extent: f32, horizontal: bool) -> f32 {
    let field = if horizontal {
        (align_org >> ALIGN_ORG_HORZ_SHIFT) & ALIGN_ORG_FIELD
    } else {
        align_org & ALIGN_ORG_FIELD
    };
    position - extent * HUD_ELEM_ORG_ANCHOR[field as usize]
}

#[must_use]
pub fn hud_elem_origin(
    place: &crate::ScreenPlacement,
    align_org: i32,
    align_screen: i32,
    x_virtual: f32,
    y_virtual: f32,
    width: f32,
    height: f32,
) -> (f32, f32) {
    let (horz, vert) = hud_elem_screen_align(align_screen);
    let applied = place.apply_rect(x_virtual, y_virtual, 0.0, 0.0, horz, vert);
    (
        align_hud_elem_axis(align_org, applied.x, width, true),
        align_hud_elem_axis(align_org, applied.y, height, false),
    )
}

fn material_extent(
    scale_to_real: f32,
    scale_to_full: f32,
    align_is_fullscreen: bool,
    size_virtual: i32,
    font_height: f32,
) -> f32 {
    if size_virtual == 0 {
        return font_height;
    }
    let scale = if align_is_fullscreen {
        scale_to_full
    } else {
        scale_to_real
    };
    scale * size_virtual as f32
}

#[must_use]
pub fn hud_elem_material_size(
    place: &crate::ScreenPlacement,
    elem: &HudElem,
    time: i32,
    font_height: f32,
) -> (f32, f32) {
    let extents = |align_screen: i32, width: i32, height: i32| {
        let (horz, vert) = hud_elem_screen_align(align_screen);
        (
            material_extent(
                place.scale_virtual_to_real[0],
                place.scale_virtual_to_full[0],
                horz == crate::ALIGN_FULLSCREEN,
                width,
                font_height,
            ),
            material_extent(
                place.scale_virtual_to_real[1],
                place.scale_virtual_to_full[1],
                vert == crate::ALIGN_FULLSCREEN,
                height,
                font_height,
            ),
        )
    };
    let (w, h) = extents(elem.align_screen, elem.width, elem.height);
    let (w, h) = match hud_elem_scale_frac(elem, time) {
        None => (w, h),
        Some(lerp) => {
            let (from_w, from_h) =
                extents(elem.from_align_screen, elem.from_width, elem.from_height);
            (from_w + (w - from_w) * lerp, from_h + (h - from_h) * lerp)
        }
    };
    (w, h.max(font_height))
}

#[must_use]
pub fn hud_elem_position(
    place: &crate::ScreenPlacement,
    elem: &HudElem,
    time: i32,
    width: f32,
    height: f32,
) -> (f32, f32) {
    let to = hud_elem_origin(
        place,
        elem.align_org,
        elem.align_screen,
        elem.x,
        elem.y,
        width,
        height,
    );
    let lerp = hud_elem_movement_frac(elem, time);
    if lerp >= 1.0 {
        return (libm::floorf(to.0 + 0.5), libm::floorf(to.1 + 0.5));
    }
    let from = hud_elem_origin(
        place,
        elem.from_align_org,
        elem.from_align_screen,
        elem.from_x,
        elem.from_y,
        width,
        height,
    );
    (
        from.0 + (to.0 - from.0) * lerp,
        from.1 + (to.1 - from.1) * lerp,
    )
}

#[must_use]
pub fn hud_elem_placement(
    place: &crate::ScreenPlacement,
    elem: &HudElem,
    time: i32,
    text_width: f32,
    font_height: f32,
) -> HudElemPlacement {
    let (w, h) = if elem.elem_type == HE_TYPE_MATERIAL {
        hud_elem_material_size(place, elem, time, font_height)
    } else {
        (text_width, font_height)
    };
    let (x, y) = hud_elem_position(place, elem, time, w, h);
    HudElemPlacement { x, y, w, h }
}

#[must_use]
pub fn hud_elem_glow_color(elem: &HudElem, faded: [u8; 4]) -> Option<[f32; 4]> {
    let glow = unpack_rgba(elem.glow_color_rgba);
    if glow[3] == 0 {
        return None;
    }
    Some([
        glow[0] as f32 / 255.0,
        glow[1] as f32 / 255.0,
        glow[2] as f32 / 255.0,
        glow[3] as f32 / 255.0 * (faded[3] as f32 / 255.0),
    ])
}

pub const OBJECTIVE_MARKER_ALPHA: f32 = 0.5;

pub const OBJECTIVE_FLASH_DIM: f32 = 0.35;

pub const OBJECTIVE_FLASH_HALF_MS: i32 = 750;

#[must_use]
pub fn objective_flash_elem(rgb: [u8; 3], base_alpha: f32, start_ms: i32, time: i32) -> HudElem {
    let period = OBJECTIVE_FLASH_HALF_MS.saturating_mul(2);
    let elapsed = time.wrapping_sub(start_ms).max(0);
    let leg = elapsed / OBJECTIVE_FLASH_HALF_MS;
    let leg_start = start_ms.wrapping_add(leg.saturating_mul(OBJECTIVE_FLASH_HALF_MS));
    let bright = (base_alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let dim = (base_alpha.clamp(0.0, 1.0) * OBJECTIVE_FLASH_DIM * 255.0) as u8;
    let falling = elapsed.rem_euclid(period) < OBJECTIVE_FLASH_HALF_MS;
    let (from, to) = if falling {
        (bright, dim)
    } else {
        (dim, bright)
    };
    HudElem {
        from_color_rgba: color_rgba(rgb[0], rgb[1], rgb[2], from),
        color_rgba: color_rgba(rgb[0], rgb[1], rgb[2], to),
        fade_start_time: leg_start,
        fade_time: OBJECTIVE_FLASH_HALF_MS,
        ..HudElem::default()
    }
}
