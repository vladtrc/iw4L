use crate::pm_weapon::{WeaponCombatFacts, WeaponHandState};
use crate::weaponstate::WeaponState;

pub const PMF_SPRINTING: u32 = 0x4000;

pub fn pm_weapon_check_for_sprint(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    pm_flags: u32,
) {
    if hand.weapon == 0 {
        return;
    }
    let Ok(ws) = WeaponState::from_i32(hand.weaponstate) else {
        return;
    };
    if !check_for_sprint_allowed(ws) {
        return;
    }
    let sprinting = pm_flags & PMF_SPRINTING != 0;
    if sprinting && !ws.is_sprint() {
        begin_sprint(hand, facts);
    } else if !sprinting && matches!(ws, WeaponState::SprintIn | WeaponState::SprintLoop) {
        begin_sprint_out(hand, facts);
    }
}

fn check_for_sprint_allowed(ws: WeaponState) -> bool {
    !matches!(
        ws,
        WeaponState::Raising
            | WeaponState::RaisingAltswitch
            | WeaponState::Dropping
            | WeaponState::DroppingQuick
            | WeaponState::DroppingAltswitch
            | WeaponState::Firing
            | WeaponState::Rechambering
            | WeaponState::MeleeInit
            | WeaponState::MeleeFire
            | WeaponState::MeleeEnd
            | WeaponState::OffhandInit
            | WeaponState::OffhandPrepare
            | WeaponState::OffhandHold
            | WeaponState::OffhandStart
            | WeaponState::Offhand
            | WeaponState::OffhandEnd
            | WeaponState::NightVisionWear
            | WeaponState::NightVisionRemove
    )
}

fn begin_sprint(hand: &mut WeaponHandState, facts: &WeaponCombatFacts) {
    let time = if facts.sprint_raise_time_ms > 0 {
        facts.sprint_raise_time_ms
    } else {
        1
    };
    hand.weaponstate = WeaponState::SprintIn as i32;
    hand.weapon_time = time;
    hand.weapon_delay = 0;
    hand.shot_count = 0;
    hand.burst_latch = false;
    crate::weap_anim::pm_start_weapon_anim(
        &mut hand.weap_anim,
        crate::weap_anim::weap_anim_event::SPRINT_IN,
    );
}

fn sprint_loop(hand: &mut WeaponHandState) {
    hand.weaponstate = WeaponState::SprintLoop as i32;
    hand.weapon_time = 0;
    hand.weapon_delay = 0;
    crate::weap_anim::pm_start_weapon_anim(
        &mut hand.weap_anim,
        crate::weap_anim::weap_anim_event::SPRINT_LOOP,
    );
}

fn begin_sprint_out(hand: &mut WeaponHandState, facts: &WeaponCombatFacts) {
    let time = if facts.sprint_drop_time_ms > 0 {
        facts.sprint_drop_time_ms
    } else {
        1
    };
    hand.weaponstate = WeaponState::SprintOut as i32;
    hand.weapon_time = time;
    hand.weapon_delay = 0;
    crate::weap_anim::pm_start_weapon_anim(
        &mut hand.weap_anim,
        crate::weap_anim::weap_anim_event::SPRINT_OUT,
    );
}

pub fn pm_weapon_advance_sprint(
    hand: &mut WeaponHandState,
    weap_flags: &mut u32,
    pm_flags_word: &mut u32,
    pm_type: i32,
) {
    match WeaponState::from_i32(hand.weaponstate) {
        Ok(WeaponState::SprintIn) if hand.weapon_time <= 0 => sprint_loop(hand),
        Ok(WeaponState::SprintOut) if hand.weapon_time <= 0 => {
            crate::melee::pm_weapon_settle_ready(hand, weap_flags, pm_flags_word, pm_type);
        }
        _ => {}
    }
}
