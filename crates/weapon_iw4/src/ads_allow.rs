use crate::ammo::{AMMOCLIP_TABLE_BYTES, bg_get_clip_for_hand};
use playerstate_iw4::PlayerState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdsAllowWeaponFacts {
    pub aim_down_sight: bool,

    pub no_ads_when_mag_empty: bool,

    pub clip_index: i32,
}

pub const OTHER_FLAG_PLAYER: u32 = playerstate_iw4::other_flags::PLAYER;

pub const WEAP_FLAG_NO_ADS: u32 = playerstate_iw4::weap_flags::NO_ADS;

pub fn pm_is_ads_allowed(
    ps: &PlayerState,
    weap: &AdsAllowWeaponFacts,
    ammoclip: &[u8; AMMOCLIP_TABLE_BYTES],
    clip_hand: u8,
) -> bool {
    if ps.last_weapon_hand == 1 && (ps.e_flags & 0xc00) == 0 {
        return false;
    }

    match ps.pm_type {
        2 | 3 | 5 | 6 | 8 | 9 => return false,
        _ => {}
    }

    if (ps.other_flags & playerstate_iw4::other_flags::PLAYER) == 0 {
        return false;
    }
    if !weap.aim_down_sight {
        return false;
    }

    let ws = ps.weaponstate_primary;

    if (0x10..=0x15).contains(&ws) {
        return false;
    }
    match ws {
        0x1 | 0x2 | 0x3 | 0x4 | 0x5 => return false,
        0xd | 0xe | 0xf => return false,
        0x1d | 0x1e => return false,
        _ => {}
    }

    if (ps.weap_flags & playerstate_iw4::weap_flags::NO_ADS) != 0 {
        return false;
    }

    if weap.no_ads_when_mag_empty {
        let clip = bg_get_clip_for_hand(ammoclip, weap.clip_index, clip_hand);
        if clip == 0 {
            return false;
        }
    }

    true
}
