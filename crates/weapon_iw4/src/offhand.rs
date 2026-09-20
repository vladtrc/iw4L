use crate::pm_weapon::{WeaponCmd, WeaponHandState, WeaponTickEvent};
use crate::weap_anim::{pm_start_weapon_anim, weap_anim_event};
use crate::weaponstate::WeaponState;
use playerstate_iw4::buttons;
use playerstate_iw4::pm_flags;
use playerstate_iw4::weap_flags;

pub const BUTTON_FRAG: u32 = buttons::FRAG;

pub const BUTTON_SMOKE: u32 = buttons::SMOKE;

pub const CURSOR_HINT_NONE: i32 = 0x7ff;

pub const OFFHAND_INV_SLOTS: usize = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OffhandInvRow {
    pub weapon: u32,

    pub offhand_class: i32,

    pub ammo: i32,

    pub hold_fire_time_ms: i32,

    pub fire_time_ms: i32,

    pub fire_delay_ms: i32,

    pub fuse_time_ms: i32,

    pub cook_off_hold: bool,

    pub offhand_hold_is_cancelable_at_0x681: Option<bool>,

    pub weap_type: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffhandCmd {
    pub inventory: [OffhandInvRow; OFFHAND_INV_SLOTS],

    pub offhand_primary: i32,

    pub offhand_secondary: i32,

    pub cmd_off_hand_index: u16,
    pub cmd_off_hand_owned: bool,

    pub cursor_hint_ent: i32,

    pub held_quick_drop_time_ms: i32,

    pub off_hand_index: i32,

    pub grenade_time_left: i32,
}

impl Default for OffhandCmd {
    fn default() -> Self {
        Self {
            inventory: [OffhandInvRow::default(); OFFHAND_INV_SLOTS],
            offhand_primary: 0,
            offhand_secondary: 0,
            cmd_off_hand_index: 0,
            cmd_off_hand_owned: false,
            cursor_hint_ent: CURSOR_HINT_NONE,
            held_quick_drop_time_ms: 0,
            off_hand_index: 0,
            grenade_time_left: 0,
        }
    }
}

pub fn bg_get_first_available_offhand(
    inventory: &[OffhandInvRow; OFFHAND_INV_SLOTS],
    wanted_class: i32,
) -> u32 {
    if wanted_class == 0 {
        return 0;
    }
    for row in inventory {
        if row.weapon == 0 {
            continue;
        }
        if row.offhand_class != wanted_class {
            continue;
        }
        if row.ammo > 0 {
            return row.weapon;
        }
    }
    0
}

fn offhand_row(cmd: &OffhandCmd, weapon: u32) -> Option<OffhandInvRow> {
    cmd.inventory.iter().copied().find(|r| r.weapon == weapon)
}

fn offhand_hold_cancel_requested(cmd: &WeaponCmd) -> bool {
    if cmd.buttons & buttons::OFFHAND_HOLD_CANCEL == 0 {
        return false;
    }
    offhand_row(&cmd.offhand, cmd.offhand.off_hand_index.max(0) as u32)
        .and_then(|r| r.offhand_hold_is_cancelable_at_0x681)
        .unwrap_or_else(|| panic!("offhand hold cancel flag +0x681 missing in source format"))
}

fn admits_check_for_offhand(weaponstate: i32) -> bool {
    if weaponstate == WeaponState::NightVisionWear as i32
        || weaponstate == WeaponState::NightVisionRemove as i32
    {
        return false;
    }
    if (0x10..0x15).contains(&weaponstate) {
        return false;
    }
    true
}

pub(crate) fn in_offhand_family(weaponstate: i32) -> bool {
    (0x10..=0x15).contains(&weaponstate)
}

pub fn pm_weapon_update_grenade_throw(
    hand: &mut WeaponHandState,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    if hand.hand_index != 0 {
        return None;
    }
    let weap = if (cmd.weap_flags & weap_flags::OFFHAND_VIEW) != 0 {
        cmd.offhand.off_hand_index as u32
    } else if hand.weapon == 0 {
        return None;
    } else {
        hand.weapon
    };
    let Some(row) = offhand_row(&cmd.offhand, weap) else {
        return None;
    };
    if row.weap_type != crate::WEAPTYPE_GRENADE {
        return None;
    }
    let t = cmd.offhand.grenade_time_left;
    if t == 0 {
        return None;
    }
    if t < 0 {
        if row.cook_off_hold {
            let weapon = cmd.offhand.off_hand_index as u32;
            spend_offhand_inventory_round(cmd, weapon);
            pm_weapon_offhand_end(hand, cmd);
            cmd.offhand.grenade_time_left = 0;
            return (weapon != 0).then_some(WeaponTickEvent::OffhandCookedOff { weapon });
        }
        cmd.offhand.grenade_time_left = 0;
        return None;
    }
    if row.cook_off_hold {
        cmd.offhand.grenade_time_left = t - cmd.msec;
    }
    if cmd.offhand.grenade_time_left < 1 {
        cmd.offhand.grenade_time_left = -1;
        let weapon = cmd.offhand.off_hand_index as u32;
        spend_offhand_inventory_round(cmd, weapon);
        pm_weapon_offhand_end(hand, cmd);
        cmd.offhand.grenade_time_left = 0;
        return (weapon != 0).then_some(WeaponTickEvent::OffhandCookedOff { weapon });
    }
    None
}

fn spend_offhand_inventory_round(cmd: &mut WeaponCmd, weapon: u32) {
    if weapon == 0 {
        return;
    }
    if let Some(row) = cmd
        .offhand
        .inventory
        .iter_mut()
        .find(|row| row.weapon == weapon)
        && row.ammo > 0
    {
        row.ammo -= 1;
    }
}

pub fn pm_weapon_enter_offhand(hand: &mut WeaponHandState, cmd: &mut WeaponCmd) {
    let prior = hand.weaponstate;
    let putting_away = matches!(
        prior,
        x if x == WeaponState::Dropping as i32
            || x == WeaponState::DroppingQuick as i32
            || x == WeaponState::DroppingAltswitch as i32
    );
    cmd.weap_flags &= !weap_flags::OFFHAND_VIEW;
    hand.weaponstate = WeaponState::OffhandInit as i32;
    hand.weapon_delay = 0;
    if hand.weapon == 0 {
        hand.weapon_time = 100;
    } else if !putting_away {
        hand.weapon_time = cmd.offhand.held_quick_drop_time_ms;
        if cmd.pm_type < 8 {
            pm_start_weapon_anim(&mut hand.weap_anim, weap_anim_event::QUICK_DROP);
        }
    }
}

pub fn pm_weapon_offhand_prepare(
    hand: &mut WeaponHandState,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    let hold = offhand_row(&cmd.offhand, cmd.offhand.off_hand_index as u32)
        .map(|r| r.hold_fire_time_ms)
        .unwrap_or(0);
    hand.weaponstate = WeaponState::OffhandPrepare as i32;
    cmd.weap_flags |= weap_flags::OFFHAND_VIEW;
    hand.weapon_time = hold;
    hand.weapon_delay = 0;
    if cmd.pm_type < 8 {
        pm_start_weapon_anim(&mut hand.weap_anim, weap_anim_event::HOLD_FIRE);
    }
    (cmd.offhand.off_hand_index != 0).then_some(WeaponTickEvent::OffhandPrepare {
        weapon: cmd.offhand.off_hand_index as u32,
    })
}

pub fn pm_weapon_offhand_hold(hand: &mut WeaponHandState, cmd: &mut WeaponCmd) {
    cmd.weap_flags |= weap_flags::OFFHAND_VIEW;
    hand.weaponstate = WeaponState::OffhandHold as i32;
    hand.weapon_time = 0;
    hand.weapon_delay = 0;
    cmd.offhand.grenade_time_left = offhand_row(&cmd.offhand, cmd.offhand.off_hand_index as u32)
        .map(|r| r.fuse_time_ms)
        .unwrap_or(0);
}

pub fn pm_weapon_offhand_start(hand: &mut WeaponHandState, cmd: &mut WeaponCmd) {
    let still_held = (cmd.old_buttons & (BUTTON_FRAG | BUTTON_SMOKE)) != 0
        && (cmd.buttons & (BUTTON_FRAG | BUTTON_SMOKE)) != 0;

    if still_held && (cmd.weap_flags & 0x1080) == 0 {
        if offhand_hold_cancel_requested(cmd) {
            pm_weapon_offhand_end(hand, cmd);
            cmd.offhand.grenade_time_left = 0;
            return;
        }
        hand.weapon_delay = 1;
        return;
    }
    let row = offhand_row(&cmd.offhand, cmd.offhand.off_hand_index as u32);
    let fire_time = row.map(|r| r.fire_time_ms).unwrap_or(0).max(0);
    let fire_delay = row.map(|r| r.fire_delay_ms).unwrap_or(0).max(0);
    hand.weaponstate = WeaponState::OffhandStart as i32;
    hand.weapon_time = fire_time;

    hand.weapon_delay = fire_delay;
    cmd.weap_flags |= weap_flags::OFFHAND_VIEW;
    if cmd.pm_type < 8 {
        pm_start_weapon_anim(&mut hand.weap_anim, weap_anim_event::FIRE);
    }
}

pub fn pm_weapon_offhand_throw(
    hand: &mut WeaponHandState,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    let _ = hand;
    let weapon = cmd.offhand.off_hand_index as u32;
    if weapon == 0 {
        return None;
    }
    if let Some(row) = cmd
        .offhand
        .inventory
        .iter_mut()
        .find(|row| row.weapon == weapon)
    {
        if row.ammo > 0 {
            row.ammo -= 1;
        }
    }
    cmd.weap_flags |= weap_flags::OFFHAND_VIEW;
    let remaining_fuse_ms =
        (cmd.offhand.grenade_time_left > 0).then_some(cmd.offhand.grenade_time_left);
    cmd.offhand.grenade_time_left = 0;
    Some(WeaponTickEvent::OffhandUsed {
        weapon,
        remaining_fuse_ms,
    })
}

pub fn pm_weapon_offhand_end(hand: &mut WeaponHandState, cmd: &mut WeaponCmd) {
    if hand.weapon == 0 {
        hand.weapon_time = 0;
        hand.weapon_delay = 1;
    } else {
        hand.weapon_time = cmd.switch_quick_raise_time_ms;
        hand.weapon_delay = 0;
        if cmd.pm_type < 8 {
            pm_start_weapon_anim(&mut hand.weap_anim, weap_anim_event::QUICK_RAISE);
        }
    }
    hand.weaponstate = WeaponState::OffhandEnd as i32;
    cmd.weap_flags &= !weap_flags::OFFHAND_VIEW;
    cmd.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    cmd.offhand.grenade_time_left = 0;
}

pub fn pm_weapon_advance_offhand(
    hand: &mut WeaponHandState,
    cmd: &mut WeaponCmd,
    delayed_action: bool,
) -> Option<WeaponTickEvent> {
    if hand.hand_index != 0 {
        return None;
    }
    let cancel = matches!(
        WeaponState::from_i32(hand.weaponstate),
        Ok(WeaponState::OffhandInit | WeaponState::OffhandPrepare)
    ) && offhand_hold_cancel_requested(cmd);
    if cancel {
        pm_weapon_offhand_end(hand, cmd);
        cmd.offhand.grenade_time_left = 0;
        return None;
    }
    if !delayed_action && (hand.weapon_time > 0 || hand.weapon_delay > 0) {
        return None;
    }
    match WeaponState::from_i32(hand.weaponstate) {
        Ok(WeaponState::OffhandInit) if hand.weapon_time <= 0 => {
            pm_weapon_offhand_prepare(hand, cmd)
        }
        Ok(WeaponState::OffhandPrepare) if hand.weapon_time <= 0 => {
            pm_weapon_offhand_hold(hand, cmd);
            None
        }
        Ok(WeaponState::OffhandHold) if hand.weapon_time <= 0 => {
            if cmd.offhand.grenade_time_left >= 0 {
                pm_weapon_offhand_start(hand, cmd);
            } else {
                pm_weapon_offhand_end(hand, cmd);
            }
            None
        }
        Ok(WeaponState::OffhandStart) => {
            if delayed_action {
                pm_weapon_offhand_throw(hand, cmd)
            } else {
                pm_weapon_offhand_end(hand, cmd);
                None
            }
        }
        Ok(WeaponState::OffhandEnd) if hand.weapon_time <= 0 => {
            crate::melee::pm_weapon_settle_ready(
                hand,
                &mut cmd.weap_flags,
                &mut cmd.pm_flags,
                cmd.pm_type,
            );
            None
        }
        _ => None,
    }
}

pub fn pm_weapon_check_for_offhand(
    hand: &mut WeaponHandState,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    if hand.hand_index != 0 {
        return None;
    }

    if (cmd.e_flags & 0xc00) != 0 {
        return None;
    }
    if (cmd.weap_flags & 0x80) != 0 {
        return None;
    }
    if (cmd.weap_flags & 0x1000) != 0 {
        return None;
    }
    if (cmd.e_flags & 0x100000) != 0 {
        return None;
    }
    if (cmd.pm_flags & pm_flags::BLOCK_OFFHAND_OTS) != 0 {
        return None;
    }
    if !admits_check_for_offhand(hand.weaponstate) {
        return None;
    }

    if cmd.offhand.cmd_off_hand_owned && cmd.offhand.cmd_off_hand_index != 0 {
        cmd.offhand.off_hand_index = i32::from(cmd.offhand.cmd_off_hand_index);
    }

    let wanted = if (cmd.buttons & BUTTON_FRAG) != 0 && (cmd.old_buttons & BUTTON_FRAG) == 0 {
        cmd.offhand.offhand_primary
    } else if (cmd.buttons & BUTTON_SMOKE) != 0 && (cmd.old_buttons & BUTTON_SMOKE) == 0 {
        cmd.offhand.offhand_secondary
    } else {
        return None;
    };

    let picked = bg_get_first_available_offhand(&cmd.offhand.inventory, wanted);
    if picked == 0 {
        return None;
    }
    cmd.offhand.off_hand_index = picked as i32;

    let prepare = cmd.offhand.cursor_hint_ent == CURSOR_HINT_NONE
        && (hand.weapon == 0 || hand.weaponstate == WeaponState::OffhandEnd as i32);
    if prepare {
        pm_weapon_offhand_prepare(hand, cmd)
    } else {
        pm_weapon_enter_offhand(hand, cmd);
        None
    }
}
