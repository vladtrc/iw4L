use playerstate_iw4::{PlayerState, weap_flags};

pub fn bg_get_viewmodel_weapon_index(ps: &PlayerState) -> u32 {
    if ps.weap_flags & weap_flags::OFFHAND_VIEW != 0 {
        ps.off_hand_index as u32
    } else {
        ps.weapon
    }
}
