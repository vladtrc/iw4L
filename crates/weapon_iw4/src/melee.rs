use crate::pm_weapon::{WeaponCombatFacts, WeaponHandState};
use crate::weap_anim::{pm_set_weap_anim, pm_start_weapon_anim, weap_anim_event};
use crate::weaponstate::WeaponState;
use playerstate_iw4::pm_flags;

pub const BUTTON_MELEE: u32 = playerstate_iw4::buttons::MELEE_CHARGE;

pub const PLAYER_MELEE_RANGE_DEFAULT: f32 = 64.0;

pub const PLAYER_MELEE_WIDTH_DEFAULT: f32 = 10.0;
pub const PLAYER_MELEE_HEIGHT_DEFAULT: f32 = 10.0;

pub const MELEE_TRACE_OFFSETS: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [-0.5, 0.0],
    [0.0, 0.5],
    [0.0, -0.5],
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
];

pub fn melee_trace_count(width: f32, height: f32) -> usize {
    if width > 0.0 || height > 0.0 {
        MELEE_TRACE_OFFSETS.len()
    } else {
        1
    }
}

pub fn melee_trace_end(
    muzzle: [f32; 3],
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    range: f32,
    width: f32,
    height: f32,
    offset: [f32; 2],
) -> [f32; 3] {
    let wr = width * offset[0];
    let hu = height * offset[1];
    [
        muzzle[0] + forward[0] * range + right[0] * wr + up[0] * hu,
        muzzle[1] + forward[1] * range + right[1] * wr + up[1] * hu,
        muzzle[2] + forward[2] * range + right[2] * wr + up[2] * hu,
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeleeChargeState {
    pub pm_flags: u32,
    pub pm_type: i32,
    pub e_flags: u32,
    pub melee_charge_yaw: f32,
    pub melee_charge_dist: i32,
    pub melee_charge_time: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeleeWeaponFacts {
    pub melee_damage: i32,

    pub overlay_reticle: i32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub melee_charge_anim: bool,

    pub knife_model: u32,

    pub quick_raise_time_ms: i32,
}

impl MeleeWeaponFacts {
    pub fn from_combat(facts: &WeaponCombatFacts) -> Self {
        Self {
            melee_damage: facts.melee_damage,
            overlay_reticle: facts.overlay_reticle,
            melee_time_ms: facts.melee_time_ms,
            melee_delay_ms: facts.melee_delay_ms,
            melee_charge_time_ms: facts.melee_charge_time_ms,
            melee_charge_delay_ms: facts.melee_charge_delay_ms,
            melee_charge_anim: facts.melee_charge_anim,
            knife_model: facts.knife_model,
            quick_raise_time_ms: facts.quick_raise_time_ms,
        }
    }
}

pub fn melee_weaponstate_blocks(ws: i32) -> bool {
    matches!(ws, 0xd | 0xe | 0xf | 0x1a | 0x1b | 0x1d | 0x1e) || (0x10..=0x15).contains(&ws)
}

fn hand_blocks_melee(hand: &WeaponHandState) -> bool {
    let ws = hand.weaponstate;
    if hand.weapon_time != 0 && !matches!(ws, 0x8 | 0x9 | 0xa | 0xb | 0xc) {
        return true;
    }
    (1..=5).contains(&ws)
}

pub fn pm_weapon_has_charge_melee(facts: &MeleeWeaponFacts) -> bool {
    facts.melee_charge_anim && facts.melee_charge_time_ms > 0
}

pub fn pm_weapon_start_melee_uses_charge(
    charge: &MeleeChargeState,
    player_melee_range: f32,
    facts: &MeleeWeaponFacts,
) -> bool {
    (charge.pm_flags & pm_flags::MELEE_CHARGE) != 0
        && (charge.melee_charge_dist as f32) > player_melee_range
        && pm_weapon_has_charge_melee(facts)
}

pub fn pm_melee_charge_start(
    charge: &mut MeleeChargeState,
    cmd_melee_charge_yaw: f32,
    cmd_melee_charge_dist: u8,
    is_in_air: bool,
) {
    let can_start = (charge.pm_flags & pm_flags::MELEE_CHARGE) == 0
        && cmd_melee_charge_dist != 0
        && charge.pm_type == 0
        && (charge.e_flags & 0xc00) == 0
        && (charge.pm_flags & 0xc) == 0
        && !is_in_air;
    if can_start {
        charge.pm_flags |= pm_flags::MELEE_CHARGE;
        charge.melee_charge_yaw = cmd_melee_charge_yaw;
        charge.melee_charge_dist = i32::from(cmd_melee_charge_dist);
        charge.melee_charge_time = 0;
    } else {
        charge.pm_flags &= !pm_flags::MELEE_CHARGE;
        charge.melee_charge_yaw = 0.0;
        charge.melee_charge_dist = 0;
        charge.melee_charge_time = 0;
    }
}

pub fn pm_weapon_settle_ready(
    hand: &mut WeaponHandState,
    weap_flags: &mut u32,
    pm_flags_word: &mut u32,
    pm_type: i32,
) {
    *weap_flags &= !playerstate_iw4::weap_flags::OFFHAND_VIEW;
    *pm_flags_word &= !playerstate_iw4::pm_flags::PRONEMOVE_OVERRIDDEN;
    hand.weapon_time = 0;
    hand.weapon_delay = 0;
    hand.weaponstate = WeaponState::Ready as i32;
    crate::weap_anim::pm_weapon_idle_weap_anim(&mut hand.weap_anim, pm_type);
}

pub fn pm_weapon_start_melee(
    primary: &mut WeaponHandState,
    mut secondary: Option<&mut WeaponHandState>,
    facts: &MeleeWeaponFacts,
    last_weapon_hand: i32,
    pm_type: i32,
    use_charge: bool,
) {
    let (time, delay, anim) = if use_charge {
        (
            facts.melee_charge_time_ms,
            facts.melee_charge_delay_ms,
            weap_anim_event::MELEE_CHARGE,
        )
    } else {
        (
            facts.melee_time_ms,
            facts.melee_delay_ms,
            weap_anim_event::MELEE,
        )
    };

    primary.weapon_time = time.max(1);
    primary.weapon_delay = delay.max(0);
    primary.shot_count = 0;
    primary.burst_latch = false;

    if pm_type <= 7 {
        let mut sec_anim = secondary.as_ref().map(|h| h.weap_anim).unwrap_or(0);
        pm_set_weap_anim(
            &mut primary.weap_anim,
            &mut sec_anim,
            last_weapon_hand,
            anim,
        );
        if let Some(sec) = secondary.as_mut() {
            sec.weap_anim = sec_anim;
        }
    } else if let Some(sec) = secondary.as_mut() {
        if last_weapon_hand != 1 {
            sec.weap_anim = 0;
        }
    }

    primary.weaponstate = WeaponState::MeleeInit as i32;
    if last_weapon_hand == 1 {
        if let Some(sec) = secondary {
            sec.weaponstate = WeaponState::MeleeInit as i32;
            sec.weapon_time = primary.weapon_time;
            sec.weapon_delay = primary.weapon_delay;
        }
    }
}

pub fn pm_weapon_try_melee(
    hands: &mut [WeaponHandState],
    facts: &MeleeWeaponFacts,
    buttons: u32,
    old_buttons: u32,
    f_weapon_pos_frac: f32,
    last_weapon_hand: i32,
    charge: &mut MeleeChargeState,
    cmd_melee_charge_yaw: f32,
    cmd_melee_charge_dist: u8,
    is_in_air: bool,
    player_melee_range: f32,
) -> bool {
    if hands.is_empty() {
        return false;
    }
    if melee_weaponstate_blocks(hands[0].weaponstate) {
        return false;
    }
    if facts.melee_damage == 0 {
        return false;
    }
    if buttons & BUTTON_MELEE == 0 || old_buttons & BUTTON_MELEE != 0 {
        return false;
    }

    if f_weapon_pos_frac > 0.0 && facts.overlay_reticle != 0 {
        return false;
    }

    let last = last_weapon_hand.clamp(0, 1) as usize;
    let n = hands.len().min(last + 1);
    for hand in hands.iter().take(n) {
        if hand_blocks_melee(hand) {
            return false;
        }
    }

    pm_melee_charge_start(
        charge,
        cmd_melee_charge_yaw,
        cmd_melee_charge_dist,
        is_in_air,
    );
    let use_charge = pm_weapon_start_melee_uses_charge(charge, player_melee_range, facts);

    let (primary, rest) = hands.split_first_mut().expect("non-empty");
    let secondary = rest.first_mut();
    pm_weapon_start_melee(
        primary,
        secondary,
        facts,
        last_weapon_hand,
        charge.pm_type,
        use_charge,
    );
    true
}

pub fn pm_weapon_melee_to_fire(hand: &mut WeaponHandState) {
    hand.weaponstate = WeaponState::MeleeFire as i32;
}

pub fn pm_weapon_melee_to_end(
    hand: &mut WeaponHandState,
    facts: &MeleeWeaponFacts,
    weap_flags: &mut u32,
    pm_flags_word: &mut u32,
    pm_type: i32,
) {
    if facts.knife_model != 0 {
        hand.weaponstate = WeaponState::MeleeEnd as i32;
        hand.weapon_time = facts.quick_raise_time_ms.max(1);
        hand.weapon_delay = 0;
        if pm_type < 8 {
            pm_start_weapon_anim(&mut hand.weap_anim, weap_anim_event::QUICK_RAISE);
        }
    } else {
        pm_weapon_settle_ready(hand, weap_flags, pm_flags_word, pm_type);
    }
}

pub fn pm_weapon_advance_melee(
    hand: &mut WeaponHandState,
    facts: &MeleeWeaponFacts,
    weap_flags: &mut u32,
    pm_flags_word: &mut u32,
    pm_type: i32,
) -> Option<crate::WeaponTickEvent> {
    let Ok(ws) = WeaponState::from_i32(hand.weaponstate) else {
        return None;
    };
    if hand.weapon_time > 0 {
        return None;
    }
    match ws {
        WeaponState::MeleeInit => {
            pm_weapon_melee_to_fire(hand);
            Some(crate::WeaponTickEvent::MeleeFired)
        }
        WeaponState::MeleeFire => {
            pm_weapon_melee_to_end(hand, facts, weap_flags, pm_flags_word, pm_type);
            None
        }
        WeaponState::MeleeEnd => {
            pm_weapon_settle_ready(hand, weap_flags, pm_flags_word, pm_type);
            None
        }
        _ => None,
    }
}
