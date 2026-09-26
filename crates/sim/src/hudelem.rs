use hud_iw4::{
    GAME_HUDELEM_CAPACITY, HE_TYPE_FREE, HE_TYPE_TEXT, HUDELEM_BANK_CAPACITY, HudElem, color_rgba,
    rebase_archival_times,
};
use playerstate_iw4::ENTITYNUM_NONE;

use crate::world::ClientId;

pub const HUDELEM_UPDATE_ARCHIVAL: u8 = 1;

pub const HUDELEM_UPDATE_CURRENT: u8 = 2;

pub const HUDELEM_UPDATE_BOTH: u8 = HUDELEM_UPDATE_ARCHIVAL | HUDELEM_UPDATE_CURRENT;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameHudElemSlot {
    pub elem: HudElem,

    pub client_num: i32,

    pub team: i32,

    pub archived: i32,
}

impl GameHudElemSlot {
    #[must_use]
    pub fn occupied(&self) -> bool {
        self.elem.elem_type != HE_TYPE_FREE
    }
}

#[must_use]
pub fn default_hud_elem() -> HudElem {
    HudElem {
        elem_type: HE_TYPE_TEXT,
        font_scale: 1.0,
        from_font_scale: 1.0,
        color_rgba: color_rgba(255, 255, 255, 255),
        from_color_rgba: color_rgba(255, 255, 255, 255),
        target_ent_num: ENTITYNUM_NONE,
        ..HudElem::default()
    }
}

pub fn alloc_hud_elem(
    pool: &mut Vec<GameHudElemSlot>,
    client_num: i32,
    team: i32,
) -> Option<usize> {
    let slot = GameHudElemSlot {
        elem: default_hud_elem(),
        client_num,
        team,
        archived: 1,
    };
    if let Some(index) = pool.iter().position(|s| !s.occupied()) {
        pool[index] = slot;
        return Some(index);
    }
    if pool.len() >= GAME_HUDELEM_CAPACITY {
        return None;
    }
    pool.push(slot);
    Some(pool.len() - 1)
}

pub fn free_hud_elem(pool: &mut [GameHudElemSlot], index: usize) {
    if let Some(slot) = pool.get_mut(index) {
        *slot = GameHudElemSlot::default();
    }
}

#[must_use]
pub fn hud_elem_update_client(
    pool: &[GameHudElemSlot],
    client: ClientId,
    team: i32,
    which: u8,
) -> (Vec<HudElem>, Vec<HudElem>) {
    let mut archival = Vec::new();
    let mut current = Vec::new();
    let client_num = i32::try_from(client.0).unwrap_or(i32::MAX);
    for slot in pool.iter().take(GAME_HUDELEM_CAPACITY) {
        if !slot.occupied() {
            continue;
        }
        if slot.team != 0 && slot.team != team {
            continue;
        }
        if slot.client_num != ENTITYNUM_NONE && slot.client_num != client_num {
            continue;
        }
        if slot.archived != 0 {
            if (which & HUDELEM_UPDATE_ARCHIVAL) != 0 && archival.len() < HUDELEM_BANK_CAPACITY {
                archival.push(slot.elem);
            }
        } else if (which & HUDELEM_UPDATE_CURRENT) != 0 && current.len() < HUDELEM_BANK_CAPACITY {
            current.push(slot.elem);
        }
    }
    (archival, current)
}

pub fn rebase_hud_archival(bank: &mut [HudElem], rebase_ms: i32) {
    for elem in bank {
        rebase_archival_times(elem, rebase_ms);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PulseFxSoundIds {
    last_assigned: i32,
}

impl PulseFxSoundIds {
    fn next(&mut self) -> i32 {
        let slots = i32::try_from(hud_iw4::HUDELEM_SOUND_SLOTS).unwrap_or(32);
        let mut id = (self.last_assigned + 1).rem_euclid(slots);
        if id == 0 {
            id += 1;
        }
        self.last_assigned = id;
        id
    }
}

pub fn set_pulse_fx(
    elem: &mut HudElem,
    now_ms: i32,
    speed: i32,
    decay_start_time: i32,
    decay_duration: i32,
    ids: &mut PulseFxSoundIds,
) {
    elem.fx_birth_time = now_ms;
    elem.fx_letter_time = speed;
    elem.fx_decay_start_time = decay_start_time;
    elem.fx_decay_duration = decay_duration;
    elem.sound_id = ids.next();
}
