use crate::entity_state::EntityState;

pub const CORPSE_INFO_WALK_DWORDS: usize = 0x14f;

pub const CORPSE_INFO_LEGS_ANIM_AT: usize = 0x418;

pub const CORPSE_INFO_TORSO_ANIM_AT: usize = 0x41c;

pub const CORPSE_INFO_TORSO_PITCH_AT: usize = 0x420;

pub const CORPSE_INFO_WAIST_PITCH_AT: usize = 0x424;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CorpseInfoPlayerAnimCopy {
    pub legs_anim: i32,

    pub torso_anim: i32,

    pub torso_pitch: u32,

    pub waist_pitch: u32,
}

impl CorpseInfoPlayerAnimCopy {
    pub fn from_entity_state(es: &EntityState) -> Self {
        Self {
            legs_anim: es.legs_anim,
            torso_anim: es.torso_anim,
            torso_pitch: es.torso_pitch,
            waist_pitch: es.waist_pitch,
        }
    }
}

pub fn g_corpse_info_slot_for_entnum(entnums: &[i32], s_number: i32) -> usize {
    let mut i = 0;
    while i < entnums.len() {
        if entnums[i] == s_number {
            return i;
        }
        i += 1;
    }
    0
}

pub fn g_corpse_info_entnum_matched(entnums: &[i32], s_number: i32) -> bool {
    let mut i = 0;
    while i < entnums.len() {
        if entnums[i] == s_number {
            return true;
        }
        i += 1;
    }
    false
}

pub fn g_corpse_info_copy_player_anims(
    server_dobj: bool,
    src: CorpseInfoPlayerAnimCopy,
) -> Option<CorpseInfoPlayerAnimCopy> {
    if server_dobj { Some(src) } else { None }
}
