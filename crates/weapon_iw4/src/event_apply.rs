use crate::pm_weapon::apply_anim_event;
use crate::{FireInputs, WeaponAnimation, WeaponCarry, WeaponEvent, WeaponFacts, WeaponStep};
use entity_iw4::EntityEventKind as Event;
use playerstate_iw4::{PendingBrass, PlayerState, ShotId, add_predictable_event};

pub struct FiredWeapon {
    pub shot_id: ShotId,
    pub cause: Option<playerstate_iw4::FireCause>,
    pub combat_seed: Option<u32>,
    pub hand: u8,
    pub weapon: u32,
    pub ammo_used: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub last_shot: bool,
    pub ads_frac: f32,
    pub view_height: f32,
    pub aim_spread_scale: f32,
    pub perks0: u32,
    pub velocity: [f32; 3],
    pub spread_degrees: f32,
}

pub enum WeaponAction {
    None,
    Detonate {
        weapon: u32,
    },
    Brass {
        weapon: u32,
        hand: u8,
        pending: Option<PendingBrass>,
        origin: [f32; 3],
    },
    Fire(FiredWeapon),
    Pullback {
        weapon: u32,
    },
    OffhandUsed {
        weapon: u32,
        remaining_fuse_ms: Option<i32>,
        cause: Option<playerstate_iw4::FireCause>,
        combat_seed: Option<u32>,
        origin: [f32; 3],
        angles: [f32; 3],
        velocity: [f32; 3],
    },
    CookedOff {
        weapon: u32,
    },
    ReloadStarted,
    Melee {
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponEventInstance {
    pub hand: u8,
    pub event: WeaponEvent,
    pub shot: Option<ShotId>,
}

impl WeaponStep {
    /// Apply one event, then let authority perform its world action before the next event.
    pub fn apply_event(
        &self,
        instance: WeaponEventInstance,
        ps: &mut PlayerState,
        carry: &mut WeaponCarry,
        inputs: &FireInputs,
        facts: &impl WeaponFacts,
        animation: &mut impl WeaponAnimation,
    ) -> WeaponAction {
        let WeaponEventInstance {
            hand: index,
            event,
            shot,
        } = instance;
        let hand = &self.hands[index as usize];
        match event {
            WeaponEvent::Detonated { weapon } => {
                add_predictable_event(ps, Event::DETONATE.0, weapon as i32);
                WeaponAction::Detonate { weapon }
            }
            WeaponEvent::ReloadAmmoAdded { .. } => WeaponAction::None,
            WeaponEvent::RechamberWeapon => {
                if !hand.delayed_rechamber {
                    add_predictable_event(ps, Event::RECHAMBER_WEAPON.0, 0);
                }
                WeaponAction::None
            }
            WeaponEvent::EjectBrass => {
                let pending = carry.pending_brass[index as usize]
                    .take()
                    .filter(|pending| pending.life == inputs.life && pending.weapon == hand.weapon);
                WeaponAction::Brass {
                    weapon: hand.weapon,
                    hand: index,
                    pending,
                    origin: ps.origin,
                }
            }
            WeaponEvent::Fire {
                ammo_used,
                cause,
                combat_seed,
            } => {
                let shot_id = shot.expect("a firing event requires its existing shot allocation");
                if self.facts.bolt_action {
                    carry.pending_brass[index as usize] = Some(PendingBrass {
                        cause,
                        life: inputs.life,
                        shot: shot_id,
                        weapon: hand.weapon,
                    });
                }
                let before = *ps;
                apply_anim_event(ps, animation, anim_iw4::ANIM_ET_FIREWEAPON, None);
                let origin = inputs.linked_origin.unwrap_or([
                    before.origin[0],
                    before.origin[1],
                    before.origin[2] + before.view_height_current,
                ]);
                let mut angles = before.viewangles;
                for (angle, offset) in angles.iter_mut().zip(inputs.cmd.gun_angle_offset) {
                    if offset.is_finite() {
                        *angle += offset.clamp(-45.0, 45.0);
                    }
                }
                crate::add_aim_spread_fire(
                    &mut ps.aim_spread_scale,
                    before.f_weapon_pos_frac,
                    self.facts.hip_spread_fire_add,
                );
                let ads_frac = before.f_weapon_pos_frac.clamp(0.0, 1.0);
                let cone = crate::get_spread_for_weapon(
                    before.view_height_current,
                    before.spread_override,
                    crate::SpreadOverrideState::from_i32(before.spread_override_state),
                    &self.facts.spread_facts(),
                    crate::perk_weap_spread_multiplier(before.perks[0]),
                );
                WeaponAction::Fire(FiredWeapon {
                    shot_id,
                    cause,
                    combat_seed,
                    hand: index,
                    weapon: hand.weapon,
                    ammo_used,
                    origin,
                    angles,
                    last_shot: hand.clip == 0 && self.facts.fire_type != 5,
                    ads_frac,
                    view_height: before.view_height_current,
                    aim_spread_scale: ps.aim_spread_scale,
                    perks0: before.perks[0],
                    velocity: before.velocity,
                    spread_degrees: crate::fire_weapon_spread_degrees(
                        cone,
                        self.facts.ads_spread,
                        ads_frac,
                        ps.aim_spread_scale,
                    ),
                })
            }
            WeaponEvent::OffhandPrepare { weapon } => {
                add_predictable_event(ps, Event::PREP_OFFHAND.0, weapon as i32);
                WeaponAction::Pullback { weapon }
            }
            WeaponEvent::OffhandUsed {
                weapon,
                remaining_fuse_ms,
                cause,
                combat_seed,
            } => {
                spend_offhand(ps, carry, facts, weapon);
                WeaponAction::OffhandUsed {
                    weapon,
                    remaining_fuse_ms,
                    cause,
                    combat_seed,
                    origin: [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ],
                    angles: ps.viewangles,
                    velocity: ps.velocity,
                }
            }
            WeaponEvent::OffhandCookedOff { weapon } => {
                spend_offhand(ps, carry, facts, weapon);
                WeaponAction::CookedOff { weapon }
            }
            WeaponEvent::ReloadStarted => {
                apply_anim_event(ps, animation, anim_iw4::ANIM_ET_RELOAD, None);
                add_predictable_event(ps, Event::RESET_ADS.0, 0);
                add_predictable_event(ps, crate::begin_reload_event(&self.facts, hand.clip), 0);
                WeaponAction::ReloadStarted
            }
            WeaponEvent::ReloadInsert => {
                add_predictable_event(ps, crate::reload_insert_event(&self.facts, hand.clip), 0);
                WeaponAction::None
            }
            WeaponEvent::ReloadEnded => {
                add_predictable_event(ps, Event::RELOAD_END.0, 0);
                WeaponAction::None
            }
            WeaponEvent::EmptyClick => {
                add_predictable_event(ps, Event::NOAMMO.0, 0);
                WeaponAction::None
            }
            WeaponEvent::AlternateStarted => {
                add_predictable_event(ps, Event::WEAPON_ALT.0, i32::from(inputs.cmd.weapon));
                WeaponAction::None
            }
            WeaponEvent::PutawayStarted => {
                add_predictable_event(ps, Event::PUTAWAY_WEAPON.0, 0);
                WeaponAction::None
            }
            WeaponEvent::RaiseStarted => {
                let first = hand.weap_anim as u32 & crate::WEAP_ANIM_EVENT_MASK
                    == crate::weap_anim_event::FIRST_RAISE;
                mark_weapon_used(ps, hand.weapon);
                add_predictable_event(
                    ps,
                    if first {
                        Event::FIRST_RAISE_WEAPON.0
                    } else {
                        Event::RAISE_WEAPON.0
                    },
                    0,
                );
                WeaponAction::None
            }
            WeaponEvent::MeleeFired => WeaponAction::Melee {
                weapon: hand.weapon,
                origin: [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ],
                angles: ps.viewangles,
            },
            WeaponEvent::StunnedStarted => {
                apply_anim_event(ps, animation, 21, None);
                WeaponAction::None
            }
            WeaponEvent::RaiseFinished | WeaponEvent::DropFinished => WeaponAction::None,
        }
    }

    pub fn finish_events(&self, carry: &mut WeaponCarry, life: playerstate_iw4::LifeSequence) {
        for (index, hand) in self.hands.iter().enumerate() {
            if !hand.rechamber_pending
                || index > self.last_hand as usize
                || carry.pending_brass[index]
                    .is_some_and(|pending| pending.life != life || pending.weapon != hand.weapon)
            {
                carry.pending_brass[index] = None;
            }
        }
    }
}

pub fn mark_weapon_used(ps: &mut PlayerState, weapon: u32) {
    if let Some(slot) = ps.weapons.iter().position(|&held| held == weapon as i32) {
        ps.weapon_data[slot * 5] = 1;
    }
}

fn spend_offhand(
    ps: &mut PlayerState,
    carry: &mut WeaponCarry,
    facts: &impl WeaponFacts,
    weapon: u32,
) {
    let (clip, stock) = carry.ammo_for(weapon);
    if clip + stock > 0 {
        if clip > 0 {
            carry.set_ammo(weapon, clip - 1, stock);
        } else {
            carry.set_ammo(weapon, 0, stock - 1);
        }
    }
    if let Some(facts) = facts.weapon_combat_row(weapon) {
        spend_ps_offhand_round(ps, weapon, facts);
    }
}

fn spend_ps_offhand_round(ps: &mut PlayerState, weapon: u32, facts: crate::WeaponCombatFacts) {
    let clip = crate::clip_table_key(facts.clip_index, weapon);
    if clip != 0 && crate::clip_row_present(&ps.ammoclip, clip) {
        let count = crate::get_clip_for_hand(&ps.ammoclip, clip, 0);
        if count > 0 {
            let _ = crate::set_clip_for_hand(&mut ps.ammoclip, clip, 0, count - 1);
            return;
        }
    }
    let ammo = crate::ammo_table_key(facts.ammo_index, weapon);
    if ammo != 0 && crate::ammo_row_present(&ps.ammo, ammo) {
        let count = crate::get_ammo_not_in_clip(&ps.ammo, ammo);
        if count > 0 {
            let _ = crate::set_ammo_not_in_clip(&mut ps.ammo, ammo, count - 1);
        }
    }
}
