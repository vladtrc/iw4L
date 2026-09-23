use hud_iw4::{
    DAMAGE_FEEDBACK_ALIGN_SCREEN, GAME_HUDELEM_CAPACITY, HE_TYPE_FREE, HE_TYPE_MATERIAL,
    HE_TYPE_PLAYERNAME, HE_TYPE_TEXT, HE_TYPE_VALUE, HUDELEM_BANK_CAPACITY, HudElem,
    MATCH_START_ALIGN_SCREEN, OUTCOME_ALIGN_SCREEN, SCORE_POPUP_ALIGN_SCREEN,
    TEXT_CENTERED_ALIGN_ORG, color_rgba, flags, rebase_archival_times, unpack_rgba,
};
use playerstate_iw4::ENTITYNUM_NONE;

use crate::world::{ClientId, Tick};
use gamemode_iw4::{
    DamageFeedbackPulse, SCORE_POPUP_ALPHA, SCORE_POPUP_FADE_MS, SCORE_POPUP_FONT_INDEX,
    SCORE_POPUP_FONT_SCALE, SCORE_POPUP_MAX_FONT_SCALE, SCORE_POPUP_PULSE_IN_MS,
    SCORE_POPUP_PULSE_OUT_MS, SCORE_POPUP_RGB, SCORE_POPUP_SORT, SCORE_POPUP_X, SCORE_POPUP_Y,
    score_popup_fade_starts_at, score_popup_idle_at,
};

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

pub fn pulse_damage_feedback(
    slot: &mut GameHudElemSlot,
    pulse: DamageFeedbackPulse,
    material_index: u8,
    now_ms: i32,
) {
    slot.archived = 1;
    slot.elem.elem_type = HE_TYPE_MATERIAL;
    slot.elem.material_index = i32::from(material_index);
    slot.elem.x = pulse.x as f32;
    slot.elem.y = pulse.y as f32;
    slot.elem.width = pulse.width;
    slot.elem.height = pulse.height;
    slot.elem.align_screen = DAMAGE_FEEDBACK_ALIGN_SCREEN;
    slot.elem.from_color_rgba = color_rgba(255, 255, 255, 255);
    slot.elem.color_rgba = color_rgba(255, 255, 255, 0);
    slot.elem.fade_start_time = now_ms;
    slot.elem.fade_time = pulse.fade_ms;
}

pub fn ensure_damage_feedback_slot(
    pool: &mut Vec<GameHudElemSlot>,
    client: ClientId,
) -> Option<&mut GameHudElemSlot> {
    let client_num = i32::try_from(client.0).ok()?;
    if let Some(idx) = pool.iter().position(|s| {
        s.archived != 0 && s.client_num == client_num && s.elem.elem_type == HE_TYPE_MATERIAL
    }) {
        return pool.get_mut(idx);
    }
    if pool.len() >= GAME_HUDELEM_CAPACITY {
        return None;
    }
    pool.push(GameHudElemSlot {
        client_num,
        team: 0,
        archived: 1,
        elem: HudElem::default(),
    });
    pool.last_mut()
}

pub fn ensure_score_popup_slot(
    pool: &mut Vec<GameHudElemSlot>,
    client: ClientId,
) -> Option<&mut GameHudElemSlot> {
    let client_num = i32::try_from(client.0).ok()?;
    if let Some(idx) = pool.iter().position(|s| {
        s.archived == 0 && s.client_num == client_num && s.elem.elem_type == HE_TYPE_VALUE
    }) {
        return pool.get_mut(idx);
    }
    if pool.len() >= GAME_HUDELEM_CAPACITY {
        return None;
    }
    pool.push(GameHudElemSlot {
        client_num,
        team: 0,
        archived: 0,
        elem: HudElem::default(),
    });
    pool.last_mut()
}

fn score_popup_rgba(alpha: f32) -> u32 {
    let ch = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
    let a = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    color_rgba(
        ch(SCORE_POPUP_RGB[0]),
        ch(SCORE_POPUP_RGB[1]),
        ch(SCORE_POPUP_RGB[2]),
        a,
    )
}

pub fn pulse_score_popup(slot: &mut GameHudElemSlot, amount: f32, now_ms: i32) {
    slot.archived = 0;
    let live = slot.elem.elem_type == HE_TYPE_VALUE && unpack_rgba(slot.elem.color_rgba)[3] > 0;
    let from_font_scale = if live {
        hud_iw4::hud_elem_lerp_font_scale(&slot.elem, now_ms)
    } else {
        SCORE_POPUP_FONT_SCALE
    };
    let total = if live {
        slot.elem.value + amount
    } else {
        amount
    };
    slot.elem.elem_type = HE_TYPE_VALUE;
    slot.elem.x = SCORE_POPUP_X;
    slot.elem.y = SCORE_POPUP_Y;
    slot.elem.align_screen = SCORE_POPUP_ALIGN_SCREEN;
    slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
    slot.elem.font = SCORE_POPUP_FONT_INDEX;
    slot.elem.font_scale = SCORE_POPUP_MAX_FONT_SCALE;
    slot.elem.from_font_scale = from_font_scale;
    slot.elem.font_scale_start_time = now_ms;
    slot.elem.font_scale_time = SCORE_POPUP_PULSE_IN_MS;
    slot.elem.sort = SCORE_POPUP_SORT;
    slot.elem.value = total;
    slot.elem.time = now_ms;
    let rgba = score_popup_rgba(SCORE_POPUP_ALPHA);
    slot.elem.color_rgba = rgba;
    slot.elem.from_color_rgba = rgba;
    slot.elem.fade_start_time = now_ms;
    slot.elem.fade_time = 0;
}

fn is_score_popup_value(slot: &GameHudElemSlot) -> bool {
    slot.elem.elem_type == HE_TYPE_VALUE && slot.client_num != ENTITYNUM_NONE
}

fn is_match_start_text(slot: &GameHudElemSlot) -> bool {
    slot.elem.elem_type == HE_TYPE_TEXT
        && slot.client_num == ENTITYNUM_NONE
        && (slot.elem.y - gamemode_iw4::MATCH_START_TEXT_Y).abs() < 0.5
}

fn is_match_start_value(slot: &GameHudElemSlot) -> bool {
    slot.elem.elem_type == HE_TYPE_VALUE
        && slot.client_num == ENTITYNUM_NONE
        && slot.elem.y.abs() < 0.5
}

fn match_start_rgba() -> u32 {
    color_rgba(255, 255, 255, 255)
}

fn match_start_value_rgba() -> u32 {
    let ch = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
    color_rgba(
        ch(gamemode_iw4::MATCH_START_VALUE_RGB[0]),
        ch(gamemode_iw4::MATCH_START_VALUE_RGB[1]),
        ch(gamemode_iw4::MATCH_START_VALUE_RGB[2]),
        255,
    )
}

fn ensure_server_slot(
    pool: &mut Vec<GameHudElemSlot>,
    pred: impl Fn(&GameHudElemSlot) -> bool,
) -> Option<&mut GameHudElemSlot> {
    if let Some(idx) = pool.iter().position(pred) {
        return pool.get_mut(idx);
    }
    if pool.len() >= GAME_HUDELEM_CAPACITY {
        return None;
    }
    pool.push(GameHudElemSlot {
        client_num: ENTITYNUM_NONE,
        team: 0,
        archived: 0,
        elem: HudElem::default(),
    });
    pool.last_mut()
}

pub fn sync_match_start_elems(
    pool: &mut Vec<GameHudElemSlot>,
    display: Option<gamemode_iw4::MatchStartDisplay>,
    now_ms: i32,
) {
    let Some(display) = display else {
        for slot in pool.iter_mut() {
            if is_match_start_text(slot) || is_match_start_value(slot) {
                slot.elem.elem_type = HE_TYPE_FREE;
            }
        }
        return;
    };
    let pulse_age = now_ms.rem_euclid(1000);
    if let Some(slot) = ensure_server_slot(pool, is_match_start_text) {
        slot.archived = 0;
        slot.client_num = ENTITYNUM_NONE;
        slot.elem.elem_type = HE_TYPE_TEXT;
        slot.elem.x = 0.0;
        slot.elem.y = gamemode_iw4::MATCH_START_TEXT_Y;
        slot.elem.font_scale = gamemode_iw4::MATCH_START_TEXT_FONT_SCALE;
        slot.elem.from_font_scale = gamemode_iw4::MATCH_START_TEXT_FONT_SCALE;
        slot.elem.font = gamemode_iw4::MATCH_START_TEXT_FONT;
        slot.elem.align_screen = MATCH_START_ALIGN_SCREEN;
        slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
        slot.elem.sort = gamemode_iw4::MATCH_START_SORT;
        slot.elem.flags = flags::HIDEWHENINMENU;
        slot.elem.color_rgba = match_start_rgba();
        slot.elem.from_color_rgba = match_start_rgba();
        slot.elem.time = now_ms;
        slot.elem.label = display.kind.label();
    }
    if let Some(slot) = ensure_server_slot(pool, is_match_start_value) {
        slot.archived = 0;
        slot.client_num = ENTITYNUM_NONE;
        slot.elem.elem_type = HE_TYPE_VALUE;
        slot.elem.x = 0.0;
        slot.elem.y = gamemode_iw4::MATCH_START_VALUE_Y;
        slot.elem.font = gamemode_iw4::MATCH_START_VALUE_FONT;
        slot.elem.align_screen = MATCH_START_ALIGN_SCREEN;
        slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
        slot.elem.sort = gamemode_iw4::MATCH_START_SORT;
        slot.elem.flags = flags::HIDEWHENINMENU;
        slot.elem.value = display.count as f32;
        slot.elem.font_scale = gamemode_iw4::match_start_value_font_scale(pulse_age);
        slot.elem.from_font_scale = gamemode_iw4::MATCH_START_VALUE_FONT_SCALE;
        slot.elem.color_rgba = match_start_value_rgba();
        slot.elem.from_color_rgba = match_start_value_rgba();
        slot.elem.time = now_ms;
    }
}

pub const NOTIFY_TEXT_PULSE_SPEED: i32 = 100;

pub const NOTIFY_PULSE_FADE_OUT_MS: i32 = 1_000;

pub const OUTCOME_PULSE_SPEED: i32 = 100;

pub const OUTCOME_PULSE_DURATION_MS: i32 = 60_000;

pub const OUTCOME_PULSE_FADE_OUT_MS: i32 = 1_000;

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

pub fn clear_pulse_fx(elem: &mut HudElem) {
    elem.fx_birth_time = 0;
    elem.fx_letter_time = 0;
    elem.fx_decay_start_time = 0;
    elem.fx_decay_duration = 0;
    elem.sound_id = 0;
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

fn is_outcome_title(slot: &GameHudElemSlot, client_num: i32) -> bool {
    slot.elem.elem_type == HE_TYPE_TEXT
        && slot.client_num == client_num
        && (slot.elem.y - gamemode_iw4::OUTCOME_TITLE_Y).abs() < 0.5
}

fn is_outcome_reason(slot: &GameHudElemSlot, client_num: i32) -> bool {
    slot.elem.elem_type == HE_TYPE_TEXT
        && slot.client_num == client_num
        && (slot.elem.y - gamemode_iw4::OUTCOME_REASON_Y).abs() < 0.5
}

pub fn clear_outcome_elems(pool: &mut Vec<GameHudElemSlot>) {
    pool.retain(|s| {
        let placement = s.elem.elem_type == HE_TYPE_PLAYERNAME
            && matches!(
                s.elem.label,
                gamemode_iw4::LABEL_FIRSTPLACE_NAME
                    | gamemode_iw4::LABEL_SECONDPLACE_NAME
                    | gamemode_iw4::LABEL_THIRDPLACE_NAME
            );
        !(placement || is_outcome_title(s, s.client_num) || is_outcome_reason(s, s.client_num))
    });
}

pub fn sync_outcome_elems(
    pool: &mut Vec<GameHudElemSlot>,
    client: ClientId,
    title_label: i32,
    reason_label: i32,
    placement: &[(ClientId, i32, f32, f32)],
    now_ms: i32,
    sound_ids: &mut PulseFxSoundIds,
) {
    let Ok(client_num) = i32::try_from(client.0) else {
        return;
    };
    let glow = match title_label {
        gamemode_iw4::LABEL_DEFEAT => color_rgba(178, 76, 51, 255),
        _ => color_rgba(51, 76, 178, 255),
    };
    let write = |pool: &mut Vec<GameHudElemSlot>,
                 pred: fn(&GameHudElemSlot, i32) -> bool,
                 y: f32,
                 scale: f32,
                 label: i32,
                 sound_ids: &mut PulseFxSoundIds,
                 pulse: bool| {
        let idx = pool.iter().position(|s| pred(s, client_num));
        let slot = if let Some(idx) = idx {
            &mut pool[idx]
        } else if pool.len() < GAME_HUDELEM_CAPACITY {
            pool.push(GameHudElemSlot {
                client_num,
                team: 0,
                archived: 0,
                elem: HudElem::default(),
            });
            match pool.last_mut() {
                Some(s) => s,
                None => return,
            }
        } else {
            return;
        };
        slot.archived = 0;
        slot.client_num = client_num;
        slot.elem.elem_type = HE_TYPE_TEXT;
        slot.elem.x = 0.0;
        slot.elem.y = y;
        slot.elem.font = gamemode_iw4::MATCH_START_TEXT_FONT;
        slot.elem.font_scale = scale;
        slot.elem.from_font_scale = scale;
        slot.elem.align_screen = OUTCOME_ALIGN_SCREEN;
        slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
        slot.elem.sort = gamemode_iw4::MATCH_START_SORT;
        slot.elem.color_rgba = color_rgba(255, 255, 255, 255);
        slot.elem.from_color_rgba = color_rgba(255, 255, 255, 255);
        slot.elem.glow_color_rgba = glow;
        slot.elem.label = label;
        slot.elem.time = now_ms;
        if pulse {
            set_pulse_fx(
                &mut slot.elem,
                now_ms,
                OUTCOME_PULSE_SPEED,
                OUTCOME_PULSE_DURATION_MS,
                OUTCOME_PULSE_FADE_OUT_MS,
                sound_ids,
            );
        } else {
            clear_pulse_fx(&mut slot.elem);
        }
    };

    write(
        pool,
        is_outcome_title,
        gamemode_iw4::OUTCOME_TITLE_Y,
        gamemode_iw4::OUTCOME_TITLE_FONT_SCALE,
        title_label,
        sound_ids,
        true,
    );

    write(
        pool,
        is_outcome_reason,
        gamemode_iw4::OUTCOME_REASON_Y,
        2.0,
        reason_label,
        sound_ids,
        false,
    );
    for (placed, label, y, scale) in placement {
        let idx = pool.iter().position(|s| {
            s.client_num == client_num
                && s.elem.elem_type == HE_TYPE_PLAYERNAME
                && s.elem.label == *label
        });
        let slot = if let Some(idx) = idx {
            &mut pool[idx]
        } else if pool.len() < GAME_HUDELEM_CAPACITY {
            pool.push(GameHudElemSlot {
                client_num,
                team: 0,
                archived: 0,
                elem: HudElem::default(),
            });
            match pool.last_mut() {
                Some(s) => s,
                None => continue,
            }
        } else {
            continue;
        };
        slot.archived = 0;
        slot.client_num = client_num;
        slot.elem.elem_type = HE_TYPE_PLAYERNAME;
        slot.elem.x = 0.0;
        slot.elem.y = *y;
        slot.elem.font = gamemode_iw4::MATCH_START_TEXT_FONT;
        slot.elem.font_scale = *scale;
        slot.elem.from_font_scale = *scale;
        slot.elem.align_screen = OUTCOME_ALIGN_SCREEN;
        slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
        slot.elem.sort = gamemode_iw4::MATCH_START_SORT;
        slot.elem.color_rgba = color_rgba(255, 255, 255, 255);
        slot.elem.from_color_rgba = color_rgba(255, 255, 255, 255);
        slot.elem.glow_color_rgba = if *label == gamemode_iw4::LABEL_FIRSTPLACE_NAME {
            color_rgba(76, 178, 51, 255)
        } else {
            color_rgba(51, 76, 178, 255)
        };
        slot.elem.label = *label;
        slot.elem.value = placed.0 as f32;
        slot.elem.time = now_ms;

        set_pulse_fx(
            &mut slot.elem,
            now_ms,
            OUTCOME_PULSE_SPEED,
            OUTCOME_PULSE_DURATION_MS,
            OUTCOME_PULSE_FADE_OUT_MS,
            sound_ids,
        );
    }
}

fn is_objective_hint(slot: &GameHudElemSlot, client_num: i32) -> bool {
    slot.elem.elem_type == HE_TYPE_TEXT
        && slot.client_num == client_num
        && slot.elem.label == gamemode_iw4::LABEL_OBJECTIVE_HINT
}

fn hint_glow_rgba() -> u32 {
    color_rgba(
        (gamemode_iw4::HINT_GLOW_RGB[0] * 255.0) as u8,
        (gamemode_iw4::HINT_GLOW_RGB[1] * 255.0) as u8,
        (gamemode_iw4::HINT_GLOW_RGB[2] * 255.0) as u8,
        255,
    )
}

pub fn sync_hint_elems(
    pool: &mut Vec<GameHudElemSlot>,
    client: ClientId,
    now_ms: i32,
    sound_ids: &mut PulseFxSoundIds,
) {
    let Ok(client_num) = i32::try_from(client.0) else {
        return;
    };
    let idx = pool.iter().position(|s| is_objective_hint(s, client_num));
    let slot = if let Some(idx) = idx {
        &mut pool[idx]
    } else if pool.len() < GAME_HUDELEM_CAPACITY {
        pool.push(GameHudElemSlot {
            client_num,
            team: 0,
            archived: 0,
            elem: HudElem::default(),
        });
        match pool.last_mut() {
            Some(s) => s,
            None => return,
        }
    } else {
        return;
    };
    slot.archived = 0;
    slot.client_num = client_num;
    slot.elem.elem_type = HE_TYPE_TEXT;
    slot.elem.x = 0.0;
    slot.elem.y = gamemode_iw4::HINT_TEXT_Y;
    slot.elem.font = gamemode_iw4::MATCH_START_TEXT_FONT;
    slot.elem.font_scale = gamemode_iw4::HINT_FONT_SCALE;
    slot.elem.from_font_scale = gamemode_iw4::HINT_FONT_SCALE;
    slot.elem.align_screen = OUTCOME_ALIGN_SCREEN;
    slot.elem.align_org = TEXT_CENTERED_ALIGN_ORG;
    slot.elem.sort = gamemode_iw4::MATCH_START_SORT;
    slot.elem.color_rgba = color_rgba(255, 255, 255, 255);
    slot.elem.from_color_rgba = color_rgba(255, 255, 255, 255);
    slot.elem.glow_color_rgba = hint_glow_rgba();
    slot.elem.label = gamemode_iw4::LABEL_OBJECTIVE_HINT;
    slot.elem.time = now_ms;

    set_pulse_fx(
        &mut slot.elem,
        now_ms,
        NOTIFY_TEXT_PULSE_SPEED,
        gamemode_iw4::HINT_DURATION_MS,
        NOTIFY_PULSE_FADE_OUT_MS,
        sound_ids,
    );
}

fn pulse_fx_visible_ms(elem: &HudElem) -> i32 {
    if elem.fx_birth_time == 0 {
        return gamemode_iw4::HINT_DURATION_MS;
    }
    elem.fx_decay_start_time
        .saturating_add(elem.fx_decay_duration)
}

pub fn tick_hint_slots(pool: &mut [GameHudElemSlot], now_ms: i32) {
    for slot in pool.iter_mut() {
        if slot.elem.elem_type != HE_TYPE_TEXT
            || slot.elem.label != gamemode_iw4::LABEL_OBJECTIVE_HINT
        {
            continue;
        }
        let age = now_ms.wrapping_sub(slot.elem.time);
        if age > pulse_fx_visible_ms(&slot.elem) {
            slot.elem.elem_type = HE_TYPE_FREE;
        }
    }
}

pub fn tick_score_popup_slots(pool: &mut [GameHudElemSlot], now_ms: i32) {
    for slot in pool.iter_mut() {
        if !is_score_popup_value(slot) {
            continue;
        }
        let age = now_ms.wrapping_sub(slot.elem.time);
        if age >= score_popup_idle_at() {
            slot.elem.elem_type = HE_TYPE_FREE;
            continue;
        }
        if age >= SCORE_POPUP_PULSE_IN_MS
            && slot.elem.font_scale == SCORE_POPUP_MAX_FONT_SCALE
            && slot.elem.font_scale_time == SCORE_POPUP_PULSE_IN_MS
        {
            slot.elem.from_font_scale = SCORE_POPUP_MAX_FONT_SCALE;
            slot.elem.font_scale = SCORE_POPUP_FONT_SCALE;
            slot.elem.font_scale_start_time = slot.elem.time + SCORE_POPUP_PULSE_IN_MS;
            slot.elem.font_scale_time = SCORE_POPUP_PULSE_OUT_MS;
        }
        if age >= score_popup_fade_starts_at() && slot.elem.fade_time == 0 {
            slot.elem.from_color_rgba = slot.elem.color_rgba;
            slot.elem.color_rgba = score_popup_rgba(0.0);
            slot.elem.fade_start_time = now_ms;
            slot.elem.fade_time = SCORE_POPUP_FADE_MS;
        }
    }
}

#[must_use]
pub fn hud_level_time_ms(tick: Tick) -> i32 {
    i32::try_from(tick.0)
        .unwrap_or(i32::MAX)
        .saturating_mul(i32::try_from(crate::score::MATCH_TICK_MS).unwrap_or(50))
}
