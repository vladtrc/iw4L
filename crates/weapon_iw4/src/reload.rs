use crate::pm_weapon::{WeaponCombatFacts, WeaponHandState};
use crate::weaponstate::WeaponState;

pub fn pm_weapon_allow_reload(hand: &WeaponHandState, facts: &WeaponCombatFacts) -> bool {
    if hand.stock <= 0 || hand.clip >= facts.clip_size {
        return false;
    }
    if !facts.no_partial_reload {
        return true;
    }
    let add = facts.reload_ammo_add;
    if add != 0 && add < facts.clip_size {
        facts.clip_size - hand.clip >= add
    } else {
        hand.clip == 0
    }
}

pub fn pm_weapon_process_input_wants_reload(
    hand: &WeaponHandState,
    facts: &WeaponCombatFacts,
    reload_requested: bool,
    pm_flags: u32,
) -> bool {
    if pm_flags & 0x80000 != 0 {
        return false;
    }
    let ws = hand.weaponstate;
    if crate::offhand::in_offhand_family(ws) || matches!(ws, 0xd | 0xe | 0xf) {
        return false;
    }
    match WeaponState::from_i32(ws) {
        Ok(state) if state.is_raise_or_drop() => return false,
        Ok(
            WeaponState::Reloading
            | WeaponState::ReloadingInterrupt
            | WeaponState::ReloadStart
            | WeaponState::ReloadStartInterrupt
            | WeaponState::ReloadEnd,
        ) => return false,
        _ => {}
    }
    let button = reload_requested && pm_weapon_allow_reload(hand, facts);
    let empty_auto = hand.clip <= 0
        && hand.stock > 0
        && ws != WeaponState::Firing as i32
        && !WeaponState::from_i32(ws).is_ok_and(WeaponState::is_sprint);
    button || empty_auto
}

pub fn reload_weaponstate_may_credit(weaponstate: i32) -> bool {
    matches!(
        WeaponState::from_i32(weaponstate),
        Ok(WeaponState::Reloading)
            | Ok(WeaponState::ReloadingInterrupt)
            | Ok(WeaponState::ReloadStart)
            | Ok(WeaponState::ReloadStartInterrupt)
    )
}

pub fn pm_weapon_arm_reload_add_delay(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    full_ms: i32,
) {
    let mut delay = match WeaponState::from_i32(hand.weaponstate) {
        Ok(WeaponState::ReloadStart) | Ok(WeaponState::ReloadStartInterrupt) => {
            let add = facts.reload_start_add_time_ms;
            if add <= 0 {
                0
            } else if full_ms > 0 && add >= full_ms {
                full_ms
            } else {
                add
            }
        }
        Ok(WeaponState::Reloading) | Ok(WeaponState::ReloadingInterrupt) => {
            let empty = hand.clip <= 0 && facts.weap_type == 0;
            let add = if empty && facts.reload_empty_add_time_ms > 0 {
                facts.reload_empty_add_time_ms
            } else {
                facts.reload_add_time_ms
            };
            if add > 0 && full_ms > 0 && add < full_ms {
                add
            } else if full_ms > 0 {
                full_ms
            } else {
                0
            }
        }
        _ => 0,
    };

    if facts.bolt_action && hand.rechamber_pending && hand.weapon != 0 {
        if delay == 0 {
            delay = hand.weapon_time;
        }
        if facts.rechamber_bolt_delay_ms < delay {
            delay = facts.rechamber_bolt_delay_ms;
        }
        if delay == 0 {
            delay = 1;
        }
        hand.weapon_delay = delay;
        return;
    }
    if delay > 0 {
        hand.weapon_delay = delay;
    }
}

pub fn pm_reload_clip(hand: &mut WeaponHandState, facts: &WeaponCombatFacts) -> i32 {
    if facts.clip_size <= 0 || hand.stock <= 0 {
        return 0;
    }
    let room = (facts.clip_size - hand.clip).max(0);
    if room <= 0 {
        return 0;
    }
    let mut take = room.min(hand.stock);

    match WeaponState::from_i32(hand.weaponstate) {
        Ok(WeaponState::ReloadStart) | Ok(WeaponState::ReloadStartInterrupt) => {
            let start_add = facts.reload_start_add;
            if start_add == 0 {
                return 0;
            }
            if start_add < facts.clip_size && take > start_add {
                take = start_add;
            }
        }
        Ok(WeaponState::Reloading) | Ok(WeaponState::ReloadingInterrupt) => {
            let add = facts.reload_ammo_add;
            if add != 0 && add < facts.clip_size && take > add {
                take = add;
            }
        }
        _ => return 0,
    }

    if take <= 0 {
        return 0;
    }
    hand.clip += take;
    hand.stock -= take;
    take
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReloadDelayedOutcome {
    pub shells: i32,
    pub rechamber_event: bool,
}

pub fn pm_weapon_reload_delayed_action(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    delayed_action: bool,
) -> ReloadDelayedOutcome {
    if !delayed_action || !reload_weaponstate_may_credit(hand.weaponstate) {
        return ReloadDelayedOutcome::default();
    }
    if facts.bolt_action && hand.rechamber_pending && hand.weapon != 0 {
        return bolt_reload_delayed_action(hand, facts);
    }
    ReloadDelayedOutcome {
        shells: pm_reload_clip(hand, facts),
        rechamber_event: false,
    }
}

fn bolt_reload_delayed_action(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
) -> ReloadDelayedOutcome {
    hand.rechamber_pending = false;
    let start = matches!(
        WeaponState::from_i32(hand.weaponstate),
        Ok(WeaponState::ReloadStart) | Ok(WeaponState::ReloadStartInterrupt)
    );

    if start && facts.reload_start_add_time_ms == 0 {
        return ReloadDelayedOutcome {
            shells: 0,
            rechamber_event: true,
        };
    }
    if hand.weapon_time == 0 {
        return ReloadDelayedOutcome {
            shells: pm_reload_clip(hand, facts),
            rechamber_event: true,
        };
    }
    let reload_time = bolt_reload_segment_ms(hand, facts, start);
    let mut bolt = facts.rechamber_bolt_delay_ms;
    if reload_time <= bolt {
        bolt = 1;
    }
    let remain = reload_time - bolt;
    if remain > 0 {
        hand.weapon_delay = remain;
        return ReloadDelayedOutcome {
            shells: 0,
            rechamber_event: true,
        };
    }
    ReloadDelayedOutcome {
        shells: pm_reload_clip(hand, facts),
        rechamber_event: true,
    }
}

fn bolt_reload_segment_ms(hand: &WeaponHandState, facts: &WeaponCombatFacts, start: bool) -> i32 {
    if start {
        let add = facts.reload_start_add_time_ms;
        let start_t = facts.reload_start_time_ms;
        if start_t <= add { start_t } else { add }
    } else {
        let empty = hand.clip <= 0 && facts.weap_type == 0;
        let mut reload_time = if empty {
            facts.reload_empty_time_ms
        } else {
            facts.reload_time_ms
        };
        let add = if empty && facts.reload_empty_add_time_ms > 0 {
            facts.reload_empty_add_time_ms
        } else {
            facts.reload_add_time_ms
        };
        if add != 0 && add < reload_time {
            reload_time = add;
        }
        reload_time
    }
}
