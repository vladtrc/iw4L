extern crate alloc;

use alloc::string::String;

pub const PERK_SLOT_NAMES: [&str; 8] = [
    "perk1",
    "perk2",
    "perk3",
    "perk4",
    "upgrade1",
    "upgrade2",
    "upgrade3",
    "equipment",
];

pub const SPECIALTY_NULL: &str = "specialty_null";

pub const SCRIPT_MENU_PERK_DISPLAY: i32 = 3;

pub const SCRIPT_MENU_PERK_HIDE: i32 = 4;
pub const SCRIPT_MENU_PERK_DISPLAY_NAME: &str = "perk_display";
pub const SCRIPT_MENU_PERK_HIDE_NAME: &str = "perk_hide";

pub const PERKS_INFO_HD_MENU: &str = "perks_info_hd";
pub const WEAPONBAR_HD_MENU: &str = "weaponbar_hd";

pub const WEAPON_NAME_FADE_DURATION_MS: i32 = 1800;
pub const WEAPON_NAME_FADE_TAIL_MS: i32 = 700;

#[must_use]
pub fn bg_get_perk_slot_index(name: &str) -> Option<usize> {
    PERK_SLOT_NAMES
        .iter()
        .position(|slot| slot.eq_ignore_ascii_case(name))
}

#[must_use]
pub fn bg_perk_code_key(code: u32) -> Option<String> {
    if code == 0 {
        None
    } else {
        Some(alloc::format!("{code}"))
    }
}
