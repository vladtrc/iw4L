use crate::{WeaponCarry, WeaponCombatFacts, WeaponHandState, WeaponTickEvent};
use playerstate_iw4::{PlayerState, UserCmd};
#[derive(Clone, Copy)]
pub struct FireInputs {
    pub cmd: UserCmd,
    pub root_seed: u64,
    pub client: playerstate_iw4::ClientId,
    pub life: playerstate_iw4::LifeSequence,
    pub command: Option<playerstate_iw4::CommandSequence>,
    pub mantle_weapon_inactive: bool,
    pub linked_origin: Option<[f32; 3]>,
    pub old_buttons: u32,
    pub old_angles: [i32; 3],
    pub msec: i32,
    pub can_fire: bool,
}

pub struct WeaponStep {
    pub facts: WeaponCombatFacts,
    pub hands: [WeaponHandState; 2],
    pub events: [Option<(u8, WeaponEvent)>; 2],
    pub last_hand: i32,
}

pub trait WeaponFacts {
    fn weapon_combat_row(&self, weapon: u32) -> Option<WeaponCombatFacts>;
    fn weapon_combat_facts(&self, weapon: u32) -> Option<WeaponCombatFacts>;
    fn equipment_facts_for(&self, weapon: u32) -> Option<crate::EquipmentRuntimeFacts>;
    fn weapon_script_name(&self, weapon: u32) -> &str;
}

pub trait WeaponAnimation {
    fn apply(&mut self, state: &mut PlayerState, event: u8, next: Option<u32>, force: bool);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponEvent {
    StunnedStarted,
    Detonated {
        weapon: u32,
    },
    Fire {
        ammo_used: i32,
        cause: Option<playerstate_iw4::FireCause>,
        combat_seed: Option<u32>,
    },
    EmptyClick,
    ReloadStarted,

    ReloadInsert,

    ReloadEnded,

    ReloadAmmoAdded {
        shells: i32,
    },

    RechamberWeapon,

    EjectBrass,
    RaiseFinished,
    DropFinished,

    PutawayStarted,
    AlternateStarted,

    RaiseStarted,

    OffhandUsed {
        weapon: u32,
        remaining_fuse_ms: Option<i32>,
        cause: Option<playerstate_iw4::FireCause>,
        combat_seed: Option<u32>,
    },

    OffhandCookedOff {
        weapon: u32,
    },

    OffhandPrepare {
        weapon: u32,
    },

    MeleeFired,
}

pub(super) fn apply_anim_event(
    state: &mut PlayerState,
    animation: &mut impl WeaponAnimation,
    event: u8,
    next: Option<u32>,
) {
    animation.apply(
        state,
        event,
        next,
        !matches!(event, ANIM_ET_RAISEWEAPON | 11..=16),
    );
}

pub const AIRDROP_MARKER_WEAPON: &str = "airdrop_marker_mp";
use crate::{
    AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT, AimSpreadMotion, AimSpreadState, CURSOR_HINT_NONE,
    MeleeChargeState, OFFHAND_INV_SLOTS, OffhandCmd, OffhandInvRow, PLAYER_MELEE_RANGE_DEFAULT,
    WeaponCmd, adjust_aim_spread_scale, ammo_row_present, clip_row_present, get_ammo_not_in_clip,
    get_clip_for_hand, num_hands_for_held, set_ammo_not_in_clip, set_clip_for_hand, weapon_hands,
};
use anim_iw4::{
    ANIM_ET_DROPWEAPON, ANIM_ET_KNIFE_MELEE, ANIM_ET_KNIFE_MELEE_CHARGE, ANIM_ET_MELEEATTACK,
    ANIM_ET_RAISEWEAPON,
};
use playerstate_iw4::{ENTITYNUM_NONE, mantle_flags};

pub fn pm_weapon(
    state: &mut PlayerState,
    carry: &mut WeaponCarry,
    content: &impl WeaponFacts,
    inputs: &FireInputs,
    animation: &mut impl WeaponAnimation,
) -> Option<WeaponStep> {
    let cmd = &inputs.cmd;
    let msec = inputs.msec.clamp(1, 200);
    let frametime = msec as f32 / 1000.0;

    {
        let mut ps = *state;
        let parent = u32::from(cmd.weapon_mapped);
        let target = u32::from(cmd.weapon);
        if target != 0
            && parent != target
            && ps.weapons.contains(&(parent as i32))
            && content
                .weapon_combat_facts(parent)
                .is_some_and(|f| f.alternate_weapon == target)
        {
            if let Some(facts) = content.weapon_combat_facts(target) {
                let ammo = crate::ammo_table_key(facts.ammo_index, target);
                let clip = crate::clip_table_key(facts.clip_index, target);
                let (initial_clip, _, initial_stock) = crate::spawn_clip_stock(&facts, 0);
                if !ammo_row_present(&ps.ammo, ammo) {
                    set_ammo_not_in_clip(&mut ps.ammo, ammo, initial_stock);
                }
                if !clip_row_present(&ps.ammoclip, clip) {
                    set_clip_for_hand(&mut ps.ammoclip, clip, 0, initial_clip);
                }
                carry.set_ammo(
                    target,
                    get_clip_for_hand(&ps.ammoclip, clip, 0),
                    get_ammo_not_in_clip(&ps.ammo, ammo),
                );
                *state = ps;
            }
        }
    }
    let old_buttons = inputs.old_buttons;
    let old_angles = inputs.old_angles;
    let ps = *state;

    let facts_weapon = if ps.weapon != 0 {
        ps.weapon
    } else if cmd.weapon != 0 {
        u32::from(cmd.weapon)
    } else {
        return None;
    };
    let Some(facts) = content.weapon_combat_facts(facts_weapon) else {
        return None;
    };

    {
        let mut spread_state = AimSpreadState {
            aim_spread_scale: ps.aim_spread_scale,
            spread_override: ps.spread_override,
            spread_override_state: ps.spread_override_state,
        };
        let motion = AimSpreadMotion {
            frametime,
            cmd_angles: cmd.angles,
            old_angles,
            forwardmove: cmd.forwardmove,
            rightmove: cmd.rightmove,
            velocity_xy: [ps.velocity[0], ps.velocity[1]],
            speed: ps.speed,
            move_speed_threshold: AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT,
        };
        adjust_aim_spread_scale(
            &mut spread_state,
            &facts.spread_facts(),
            &facts.aim_spread_decay_facts(),
            crate::AimSpreadPosture {
                ground_entity_num: ps.ground_entity_num,
                pm_type: ps.pm_type,
                e_flags: ps.e_flags,
                weapon_pos_frac: ps.f_weapon_pos_frac,
            },
            &motion,
        );
        {
            let ps_mut = &mut *state;
            ps_mut.aim_spread_scale = spread_state.aim_spread_scale;
            ps_mut.spread_override_state = spread_state.spread_override_state;
        }
    }
    let ps = *state;
    let meta = &*carry;
    let started_weapon = ps.weapon;
    let ammo_weapon = if started_weapon != 0 {
        started_weapon
    } else {
        facts_weapon
    };
    let last_hand = num_hands_for_held(&ps.weapons, &ps.weapon_data, ammo_weapon);
    let (meta_clip, meta_stock) = meta.ammo_for(ammo_weapon);
    let ammo_index = crate::ammo_table_key(facts.ammo_index, ammo_weapon);
    let clip_index = crate::clip_table_key(facts.clip_index, ammo_weapon);

    let stock = if ammo_row_present(&ps.ammo, ammo_index) {
        get_ammo_not_in_clip(&ps.ammo, ammo_index)
    } else {
        meta_stock
    };
    let clip0 = if clip_row_present(&ps.ammoclip, clip_index) {
        get_clip_for_hand(&ps.ammoclip, clip_index, 0)
    } else {
        meta_clip
    };
    let clip1 = if clip_row_present(&ps.ammoclip, clip_index) {
        get_clip_for_hand(&ps.ammoclip, clip_index, 1)
    } else {
        0
    };

    let quick_reload = facts.dual_mag.is_some() && meta.quick_reload_ready(ammo_weapon);
    let mut hands = [
        WeaponHandState {
            weapon: started_weapon,
            weaponstate: ps.weaponstate_primary,
            weapon_time: ps.weapon_time,
            weapon_delay: ps.weapon_delay,
            weap_anim: ps.weap_anim,
            hand_index: 0,
            clip: clip0,
            stock,
            shot_count: meta.weapon_shot_count,
            burst_latch: meta.burst_latch,
            rechamber_pending: crate::weapon_rechamber_pending(
                &ps.weapons,
                &ps.weapon_data,
                started_weapon,
                0,
            ),
            delayed_rechamber: false,
            weapon_restrict_kick_time: ps.weapon_restrict_kick_time,
            quick_reload,
        },
        WeaponHandState {
            weapon: started_weapon,
            weaponstate: ps.weaponstate_secondary,
            weapon_time: ps.weapon_time_secondary,
            weapon_delay: ps.weapon_delay_secondary,
            weap_anim: ps.weap_anim_secondary,
            hand_index: 1,
            clip: if last_hand >= 1 { clip1 } else { 0 },
            stock,
            shot_count: ps.weapon_shot_count_secondary as u8,
            burst_latch: meta.burst_latch_secondary,
            rechamber_pending: crate::weapon_rechamber_pending(
                &ps.weapons,
                &ps.weapon_data,
                started_weapon,
                1,
            ),
            delayed_rechamber: false,
            weapon_restrict_kick_time: ps.weapon_restrict_kick_time_secondary,
            quick_reload,
        },
    ];
    let locked_fire = inputs.can_fire;
    let fire_buttons = if locked_fire {
        cmd.buttons
    } else {
        cmd.buttons & !playerstate_iw4::buttons::ATTACK
    };
    let selected_airdrop_marker = content.weapon_script_name(ps.weapon) == AIRDROP_MARKER_WEAPON;
    let marker_offhand_class = i32::MAX;
    let mut wcmd = WeaponCmd {
        msec,
        server_time: cmd.server_time,
        stun_time: ps.stun_time,
        buttons: fire_buttons
            | if selected_airdrop_marker && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0 {
                playerstate_iw4::buttons::FRAG
            } else {
                0
            },
        old_buttons: old_buttons
            | if selected_airdrop_marker && old_buttons & playerstate_iw4::buttons::ATTACK != 0 {
                playerstate_iw4::buttons::FRAG
            } else {
                0
            },

        cmd_weapon: if cmd.weapon != 0 {
            cmd.weapon
        } else {
            ps.weapon as u16
        },
        pm_flags: ps.pm_flags,
        weap_flags: ps.weap_flags,
        pm_type: ps.pm_type,
        e_flags: ps.e_flags,
        last_weapon_hand: last_hand,
        f_weapon_pos_frac: ps.f_weapon_pos_frac,
        melee_charge_yaw: cmd.melee_charge_yaw,
        melee_charge_dist: cmd.melee_charge_dist,
        player_melee_range: PLAYER_MELEE_RANGE_DEFAULT,

        is_in_air: ps.ground_entity_num == ENTITYNUM_NONE && ps.pm_type != 1 && ps.pm_type != 7,
        melee_charge: MeleeChargeState {
            pm_flags: ps.pm_flags,
            pm_type: ps.pm_type,
            e_flags: ps.e_flags,
            melee_charge_yaw: ps.melee_charge_yaw,
            melee_charge_dist: ps.melee_charge_dist,
            melee_charge_time: ps.melee_charge_time,
        },
        melee_started: None,

        mantle_weapon_inactive: inputs.mantle_weapon_inactive,
        mantle_quick_raise: (ps.mantle_flags & mantle_flags::QUICK) != 0,
        cmd_weapon_owned: {
            let w = if cmd.weapon != 0 {
                u32::from(cmd.weapon)
            } else {
                ps.weapon
            };
            w == 0
                || ps.weapons.contains(&(w as i32))
                || (ps.weapons.contains(&(i32::from(cmd.weapon_mapped)))
                    && content
                        .weapon_combat_facts(u32::from(cmd.weapon_mapped))
                        .is_some_and(|f| f.alternate_weapon == w))
        },

        cmd_weapon_pistol_quick: content
            .weapon_combat_facts(u32::from(cmd.weapon))
            .is_some_and(|f| f.weap_class == 5),
        alternate_switch: cmd.weapon != 0
            && (facts.alternate_weapon == u32::from(cmd.weapon)
                || (facts.inventory_type == 3 && ps.weapon_primary == u32::from(cmd.weapon))),
        switch_alternate_raise_time_ms: content
            .weapon_combat_facts(u32::from(cmd.weapon))
            .map_or(0, |f| f.alternate_raise_time_ms),
        switch_raise_time_ms: {
            let w = if cmd.weapon != 0 {
                u32::from(cmd.weapon)
            } else {
                ps.weapon
            };
            content
                .weapon_combat_facts(w)
                .map(|f| f.raise_time_ms)
                .unwrap_or(0)
        },
        switch_quick_raise_time_ms: {
            let w = if cmd.weapon != 0 {
                u32::from(cmd.weapon)
            } else {
                ps.weapon
            };
            content
                .weapon_combat_facts(w)
                .map(|f| f.quick_raise_time_ms)
                .unwrap_or(0)
        },
        switch_first_raise_time_ms: {
            let w = if cmd.weapon != 0 {
                u32::from(cmd.weapon)
            } else {
                ps.weapon
            };
            if weapon_used_before(&ps, w) {
                0
            } else {
                content
                    .weapon_combat_facts(w)
                    .map_or(0, |f| f.first_raise_time_ms.max(0))
            }
        },
        perks0: ps.perks[0],
        perks1: ps.perks[1],
        perk_weap_reload_multiplier: crate::PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT,
        offhand: {
            let mut inventory = [OffhandInvRow::default(); OFFHAND_INV_SLOTS];
            for (i, &slot) in ps.weapons.iter().enumerate() {
                if slot <= 0 {
                    continue;
                }
                let weapon = slot as u32;
                let (clip, stock) = meta.ammo_for(weapon);
                let eq = content.equipment_facts_for(weapon);
                let combat = content.weapon_combat_facts(weapon);
                inventory[i] = OffhandInvRow {
                    weapon,
                    offhand_class: if selected_airdrop_marker && weapon == ps.weapon {
                        marker_offhand_class
                    } else {
                        eq.map(|e| e.offhand_class).unwrap_or(0)
                    },
                    ammo: clip + stock,
                    hold_fire_time_ms: eq.map(|e| e.hold_fire_time_ms).unwrap_or(0),
                    fire_time_ms: combat.map(|f| f.fire_time_ms).unwrap_or(0),
                    fire_delay_ms: combat.map(|f| f.fire_delay_ms).unwrap_or(0),
                    fuse_time_ms: eq.map(|e| e.fuse_time_ms).unwrap_or(0),
                    cook_off_hold: eq.map(|e| e.cook_off_hold).unwrap_or(false),
                    offhand_hold_is_cancelable: combat.and_then(|f| f.offhand_hold_is_cancelable),
                    weap_type: combat.map(|f| f.weap_type).unwrap_or(0),
                    has_detonator: eq.is_some_and(|e| e.has_detonator),
                    detonate_delay_ms: eq.map_or(0, |e| e.detonate_delay_ms),
                    detonate_time_ms: eq.map_or(0, |e| e.detonate_time_ms),
                };
            }
            OffhandCmd {
                inventory,
                offhand_primary: if selected_airdrop_marker {
                    marker_offhand_class
                } else {
                    ps.offhand_primary
                },
                offhand_secondary: ps.offhand_secondary,
                cmd_off_hand_index: cmd.off_hand_index,
                cmd_off_hand_owned: cmd.off_hand_index != 0
                    && ps
                        .weapons
                        .iter()
                        .any(|&slot| slot == i32::from(cmd.off_hand_index)),
                cursor_hint_ent: CURSOR_HINT_NONE,
                held_quick_drop_time_ms: facts.quick_drop_time_ms,
                off_hand_index: ps.off_hand_index,
                grenade_time_left: ps.grenade_time_left,
            }
        },
    };
    let clip_before = hands[0].clip;
    let events = weapon_hands(&mut hands, &facts, &mut wcmd, last_hand);
    let hand0 = hands[0];
    for &(hand, event) in events.iter().flatten() {
        if hand != 0 {
            continue;
        }
        match event {
            WeaponTickEvent::PutawayStarted
                if ps.pm_flags & playerstate_iw4::pm_flags::MANTLE == 0 =>
            {
                apply_anim_event(
                    state,
                    animation,
                    ANIM_ET_DROPWEAPON,
                    Some(u32::from(wcmd.cmd_weapon)),
                );
            }
            WeaponTickEvent::RaiseStarted
                if started_weapon != 0
                    && hand0.weaponstate != crate::WeaponState::RaisingAltswitch as i32 =>
            {
                apply_anim_event(state, animation, ANIM_ET_RAISEWEAPON, None);
            }
            _ => {}
        }
    }
    if let Some(charge) = wcmd.melee_started {
        {
            let ps_mut = &mut *state;
            playerstate_iw4::add_predictable_event(
                ps_mut,
                entity_iw4::EntityEventKind::MELEE_SWIPE.0,
                0,
            );
        }
        let anim = match (facts.knife_model != 0, charge) {
            (true, true) => ANIM_ET_KNIFE_MELEE_CHARGE,
            (true, false) => ANIM_ET_KNIFE_MELEE,
            (false, _) => ANIM_ET_MELEEATTACK,
        };
        apply_anim_event(state, animation, anim, None);
    }

    {
        let ps_mut = &mut *state;
        for (index, hand) in hands.iter().enumerate().take(last_hand as usize + 1) {
            crate::set_weapon_rechamber_pending(
                &ps_mut.weapons,
                &mut ps_mut.weapon_data,
                started_weapon,
                index,
                hand.rechamber_pending,
            );
        }
        crate::update_scope_zoom(ps_mut, cmd.buttons, old_buttons, facts.scope_zoom);
        ps_mut.weapon = hand0.weapon;
        if hand0.weapon != started_weapon {
            ps_mut.scope_zoom_level = 0;
            if hand0.weaponstate == crate::WeaponState::RaisingAltswitch as i32 {
                ps_mut.aim_spread_scale = ps_mut.aim_spread_scale.max(128.0);
            }
            ps_mut.weapon_primary = if hand0.weapon == u32::from(cmd.weapon) {
                u32::from(cmd.weapon_mapped)
            } else {
                0
            };
        }
        ps_mut.weaponstate_primary = hand0.weaponstate;
        ps_mut.weapon_time = hand0.weapon_time;
        ps_mut.weapon_delay = hand0.weapon_delay;
        ps_mut.weap_anim = hand0.weap_anim;
        ps_mut.weapon_restrict_kick_time = hand0.weapon_restrict_kick_time;
        ps_mut.last_weapon_hand = last_hand;
        ps_mut.pm_flags = wcmd.pm_flags;
        ps_mut.weap_flags = wcmd.weap_flags;
        ps_mut.off_hand_index = wcmd.offhand.off_hand_index;
        ps_mut.grenade_time_left = wcmd.offhand.grenade_time_left;
        ps_mut.melee_charge_yaw = wcmd.melee_charge.melee_charge_yaw;
        ps_mut.melee_charge_dist = wcmd.melee_charge.melee_charge_dist;
        ps_mut.melee_charge_time = wcmd.melee_charge.melee_charge_time;
        if last_hand >= 1 {
            let h1 = hands[1];
            ps_mut.weaponstate_secondary = h1.weaponstate;
            ps_mut.weapon_time_secondary = h1.weapon_time;
            ps_mut.weapon_delay_secondary = h1.weapon_delay;
            ps_mut.weap_anim_secondary = h1.weap_anim;
            ps_mut.weapon_shot_count_secondary = i32::from(h1.shot_count);
            ps_mut.weapon_restrict_kick_time_secondary = h1.weapon_restrict_kick_time;
        }
    }
    let held_rechamber_pending: [bool; 2] = core::array::from_fn(|hand| {
        crate::weapon_rechamber_pending(&state.weapons, &state.weapon_data, state.weapon, hand)
    });
    let meta = &mut *carry;
    if hand0.weapon != started_weapon {
        if started_weapon != 0 {
            meta.set_ammo(started_weapon, hand0.clip, hand0.stock);
        }
    } else if hand0.weapon != 0 {
        meta.set_ammo(hand0.weapon, hand0.clip, hand0.stock);
    }
    if hand0.weapon != 0 {
        meta.mirror_held_ammo(hand0.weapon);
    }
    meta.weapon_shot_count = hand0.shot_count;
    meta.burst_latch = hand0.burst_latch;
    meta.rechamber_pending = held_rechamber_pending[0];
    meta.burst_latch_secondary = last_hand >= 1 && hands[1].burst_latch;
    meta.rechamber_pending_secondary = held_rechamber_pending[1];
    if facts.dual_mag.is_some() && started_weapon != 0 {
        meta.set_quick_reload_ready(started_weapon, hand0.quick_reload);
    }

    if ammo_index != 0 || clip_index != 0 {
        {
            let ps_mut = &mut *state;
            if ammo_index != 0 {
                let _ = set_ammo_not_in_clip(&mut ps_mut.ammo, ammo_index, hands[0].stock);
            }
            if clip_index != 0 {
                let _ = set_clip_for_hand(&mut ps_mut.ammoclip, clip_index, 0, hands[0].clip);
                if last_hand >= 1 {
                    let _ = set_clip_for_hand(&mut ps_mut.ammoclip, clip_index, 1, hands[1].clip);
                }
            }
        }
    }

    if hands[0].delayed_rechamber {
        {
            let ps = &mut *state;
            playerstate_iw4::add_predictable_event(
                ps,
                entity_iw4::EntityEventKind::RECHAMBER_WEAPON.0,
                0,
            );
        }
    }
    if hands[0].clip > clip_before {
        {
            let ps = &mut *state;
            playerstate_iw4::add_predictable_event(
                ps,
                entity_iw4::EntityEventKind::RELOAD_ADDAMMO.0,
                0,
            );
        }
    }

    let mut ordinal = 0u16;
    let events = events.map(|slot| {
        slot.map(|(hand, event)| {
            let mut next_cause = || {
                let cause = inputs.command.map(|command| playerstate_iw4::FireCause {
                    client: inputs.client,
                    life: inputs.life,
                    command,
                    ordinal,
                    hand,
                });
                ordinal += 1;
                cause
            };
            let event = match event {
                WeaponTickEvent::ShotAccepted { ammo_used } => {
                    let cause = next_cause();
                    WeaponEvent::Fire {
                        ammo_used,
                        cause,
                        combat_seed: cause.map(|cause| cause.combat_seed(inputs.root_seed)),
                    }
                }
                WeaponTickEvent::StunnedStarted => WeaponEvent::StunnedStarted,
                WeaponTickEvent::Detonated { weapon } => WeaponEvent::Detonated { weapon },
                WeaponTickEvent::EmptyClick => WeaponEvent::EmptyClick,
                WeaponTickEvent::ReloadStarted => WeaponEvent::ReloadStarted,
                WeaponTickEvent::ReloadInsert => WeaponEvent::ReloadInsert,
                WeaponTickEvent::ReloadEnded => WeaponEvent::ReloadEnded,
                WeaponTickEvent::ReloadAmmoAdded { shells } => {
                    WeaponEvent::ReloadAmmoAdded { shells }
                }
                WeaponTickEvent::RechamberWeapon => WeaponEvent::RechamberWeapon,
                WeaponTickEvent::EjectBrass => WeaponEvent::EjectBrass,
                WeaponTickEvent::RaiseFinished => WeaponEvent::RaiseFinished,
                WeaponTickEvent::DropFinished => WeaponEvent::DropFinished,
                WeaponTickEvent::PutawayStarted => WeaponEvent::PutawayStarted,
                WeaponTickEvent::AlternateStarted => WeaponEvent::AlternateStarted,
                WeaponTickEvent::RaiseStarted => WeaponEvent::RaiseStarted,
                WeaponTickEvent::OffhandUsed {
                    weapon,
                    remaining_fuse_ms,
                } => {
                    let cause = next_cause();
                    WeaponEvent::OffhandUsed {
                        weapon,
                        remaining_fuse_ms,
                        cause,
                        combat_seed: cause.map(|cause| cause.combat_seed(inputs.root_seed)),
                    }
                }
                WeaponTickEvent::OffhandCookedOff { weapon } => {
                    WeaponEvent::OffhandCookedOff { weapon }
                }
                WeaponTickEvent::OffhandPrepare { weapon } => {
                    WeaponEvent::OffhandPrepare { weapon }
                }
                WeaponTickEvent::MeleeFired => WeaponEvent::MeleeFired,
            };
            (hand, event)
        })
    });
    Some(WeaponStep {
        facts,
        hands,
        events,
        last_hand,
    })
}

fn weapon_used_before(ps: &PlayerState, weapon: u32) -> bool {
    ps.weapons
        .iter()
        .position(|&held| held == weapon as i32)
        .is_none_or(|slot| ps.weapon_data[slot * 5] != 0)
}
