extern crate alloc;

use alloc::string::String;

pub const PLAYER_CARD_SCRIPT_SLOT_COUNT: usize = 18;

pub const PLAYER_CARD_SLOT_KILLEDBY: i32 = 7;

pub const PLAYER_CARD_SLOT_YOUKILLED: i32 = 8;

pub const SCRIPT_MENU_KILLEDBY_DISPLAY: i32 = 0;
pub const SCRIPT_MENU_KILLEDBY_HIDE: i32 = 1;
pub const SCRIPT_MENU_YOUKILLED_DISPLAY: i32 = 2;

pub const SCRIPT_MENU_KILLEDBY_DISPLAY_NAME: &str = "killedby_card_display";
pub const SCRIPT_MENU_KILLEDBY_HIDE_NAME: &str = "killedby_card_hide";
pub const SCRIPT_MENU_YOUKILLED_DISPLAY_NAME: &str = "youkilled_card_display";

pub const PLAYERCARD_YOU_KILLED_MENU: &str = "playercard_youkilled_hd";
pub const PLAYERCARD_KILLED_BY_MENU: &str = "playercard_killedby_hd";

pub const PLAYERCARD_INFO_VALID: i32 = 0;
pub const PLAYERCARD_INFO_TITLE: i32 = 1;
pub const PLAYERCARD_INFO_ICON: i32 = 2;
pub const PLAYERCARD_INFO_NAMEPLATE: i32 = 3;
pub const PLAYERCARD_INFO_RANK: i32 = 4;
pub const PLAYERCARD_INFO_PRESTIGE: i32 = 5;
pub const PLAYERCARD_INFO_TEAM: i32 = 6;
pub const PLAYERCARD_INFO_AGE: i32 = 7;
pub const PLAYERCARD_INFO_NAME: i32 = 8;
pub const PLAYERCARD_INFO_CLAN: i32 = 9;
pub const PLAYERCARD_INFO_STR: i32 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerCardData {
    pub last_update_time: i32,

    pub title: i32,

    pub icon: i32,

    pub nameplate: i32,

    pub rank: i32,

    pub prestige: i32,

    pub team: i32,

    pub name: [u8; 32],
}

impl Default for PlayerCardData {
    fn default() -> Self {
        Self {
            last_update_time: 0,
            title: 0,
            icon: 0,
            nameplate: 0,
            rank: 0,
            prestige: 0,
            team: 0,
            name: [0; 32],
        }
    }
}

impl PlayerCardData {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.name[0] != 0
    }

    #[must_use]
    pub fn name_str(&self) -> &str {
        let end = self
            .name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.name.len());
        core::str::from_utf8(&self.name[..end]).unwrap_or("")
    }
}

#[must_use]
pub fn cg_player_cards_set_script_slot(
    cg_time: i32,
    name16: &[u8; 16],
    team: i32,
    rank: i32,
    prestige: i32,
    icon: u32,
    title: u32,
    nameplate: u32,
) -> PlayerCardData {
    let mut name = [0u8; 32];
    let n = name16.len().min(name.len());
    name[..n].copy_from_slice(&name16[..n]);
    if name16.iter().all(|&b| b == 0) {
        name = [0u8; 32];
    }
    PlayerCardData {
        last_update_time: cg_time,
        title: title as i32,
        icon: icon as i32,
        nameplate: nameplate as i32,
        rank,
        prestige,
        team,
        name,
    }
}

#[must_use]
pub fn ui_run_op_get_player_card_info(slot: &PlayerCardData, field: i32) -> crate::expr::Operand {
    use crate::expr::Operand;
    match field {
        PLAYERCARD_INFO_VALID => Operand::Int(i32::from(slot.valid())),
        PLAYERCARD_INFO_TITLE => Operand::Int(slot.title),
        PLAYERCARD_INFO_ICON => Operand::Int(slot.icon),
        PLAYERCARD_INFO_NAMEPLATE => Operand::Int(slot.nameplate),
        PLAYERCARD_INFO_RANK => Operand::Int(slot.rank),
        PLAYERCARD_INFO_PRESTIGE => Operand::Int(slot.prestige),
        PLAYERCARD_INFO_TEAM => Operand::Int(slot.team),
        PLAYERCARD_INFO_AGE => Operand::Int(0),
        PLAYERCARD_INFO_NAME => Operand::Str(String::from(slot.name_str())),
        PLAYERCARD_INFO_CLAN | PLAYERCARD_INFO_STR => Operand::Str(String::new()),
        _ => Operand::Int(0),
    }
}

#[must_use]
pub fn script_menu_name(cs_index: i32) -> Option<&'static str> {
    match cs_index {
        SCRIPT_MENU_KILLEDBY_DISPLAY => Some(SCRIPT_MENU_KILLEDBY_DISPLAY_NAME),
        SCRIPT_MENU_KILLEDBY_HIDE => Some(SCRIPT_MENU_KILLEDBY_HIDE_NAME),
        SCRIPT_MENU_YOUKILLED_DISPLAY => Some(SCRIPT_MENU_YOUKILLED_DISPLAY_NAME),
        crate::SCRIPT_MENU_PERK_DISPLAY => Some(crate::SCRIPT_MENU_PERK_DISPLAY_NAME),
        crate::SCRIPT_MENU_PERK_HIDE => Some(crate::SCRIPT_MENU_PERK_HIDE_NAME),
        _ => None,
    }
}

#[must_use]
pub fn script_menu_cs_index(name: &str) -> Option<i32> {
    match name {
        SCRIPT_MENU_KILLEDBY_DISPLAY_NAME => Some(SCRIPT_MENU_KILLEDBY_DISPLAY),
        SCRIPT_MENU_KILLEDBY_HIDE_NAME => Some(SCRIPT_MENU_KILLEDBY_HIDE),
        SCRIPT_MENU_YOUKILLED_DISPLAY_NAME => Some(SCRIPT_MENU_YOUKILLED_DISPLAY),
        crate::SCRIPT_MENU_PERK_DISPLAY_NAME => Some(crate::SCRIPT_MENU_PERK_DISPLAY),
        crate::SCRIPT_MENU_PERK_HIDE_NAME => Some(crate::SCRIPT_MENU_PERK_HIDE),
        _ => None,
    }
}
