use crate::bullet::{angles_to_forward, bullet_damage_at_distance};
use crate::bullet_collision::{
    BulletTraceQuery, ColliderId, EntityCollisionEpoch, EntityCollisionTraceGeom,
    HistorySampleVerdict, MASK_BULLET_WORLD, bullet_trace_segments_filtered, glass_piece_from_hit,
};
use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, MatchRng, PelletId, ShotId};
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::world::{ClientId, Tick};
use anim_iw4::{ANIM_ET_FIREWEAPON, ANIM_ET_RELOAD};
use movement_iw4::{Pml, mantle_is_weapon_inactive, pm_is_in_air};
use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, mantle_flags};
use weapon_iw4::{
    AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT, AimSpreadMotion, AimSpreadState, CURSOR_HINT_NONE,
    FireWeaponKind, MELEE_TRACE_OFFSETS, MeleeChargeState, OFFHAND_INV_SLOTS, OffhandCmd,
    OffhandInvRow, PLAYER_MELEE_HEIGHT_DEFAULT, PLAYER_MELEE_RANGE_DEFAULT,
    PLAYER_MELEE_WIDTH_DEFAULT, SpreadOverrideState, WeaponCmd, WeaponHandState, WeaponTickEvent,
    bg_ammo_row_present, bg_ammo_table_key, bg_clip_row_present, bg_clip_table_key,
    bg_get_ammo_not_in_clip, bg_get_clip_for_hand, bg_get_spread_for_weapon,
    bg_set_ammo_not_in_clip, bg_set_clip_for_hand, fire_weapon_kind, fire_weapon_spread_degrees,
    melee_trace_count, melee_trace_end, pm_add_aim_spread_fire, pm_adjust_aim_spread_scale,
    pm_begin_reload_event, pm_num_hands_for_held, pm_reload_insert_event, pm_weapon_hands,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedShot {
    pub shot_id: ShotId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub weapon: u32,
    pub ammo_used: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub ads_frac: f32,

    pub view_height_current: f32,

    pub aim_spread_scale: f32,

    pub perks0: u32,
    pub combat_seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emission {
    pub shot_id: ShotId,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub max_range: f32,
    pub base_damage: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCollisionRepresentation {
    StandingAabbV1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotCollisionGeometry {
    Miss,
    World,
    Player {
        representation: PlayerCollisionRepresentation,
        history: HistorySampleVerdict,
    },
    Entity {
        epoch: EntityCollisionEpoch,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShotCollisionVerdict {
    pub shot_id: ShotId,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub geometry: ShotCollisionGeometry,
    pub terminal: Option<ColliderId>,
    pub startsolid: bool,
    pub bone_center: Option<[f32; 3]>,
    pub bone_half_size: Option<[f32; 3]>,
    pub xmodel_contents: Option<u32>,
    pub model_key: Option<String>,

    pub end: Option<[f32; 3]>,

    pub impact_n: u32,

    pub event_n: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TracePhaseOutput {
    pub shot_verdicts: Vec<ShotCollisionVerdict>,
}

pub(crate) fn advance_weapon_command(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    cmd: playerstate_iw4::UserCmd,
    msec: i32,
) -> Vec<AcceptedShot> {
    let msec = msec.clamp(1, 200);
    let frametime = msec as f32 / 1000.0;
    let mut accepted = Vec::new();
    {
        let id = &id;
        let cmd = &cmd;
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            return accepted;
        }
        let old_buttons = world
            .old_buttons_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(0, |(_, b)| *b);
        let old_angles = world
            .old_cmd_angles_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(cmd.angles, |(_, a)| *a);

        let Some(ps) = world.player(*id).copied() else {
            return accepted;
        };

        let facts_weapon = if ps.weapon != 0 {
            ps.weapon
        } else if cmd.weapon != 0 {
            u32::from(cmd.weapon)
        } else {
            return accepted;
        };
        let Some(facts) = world.combat_facts_for(facts_weapon) else {
            return accepted;
        };

        {
            let mut state = AimSpreadState {
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
            pm_adjust_aim_spread_scale(
                &mut state,
                &facts.spread_facts(),
                &facts.aim_spread_decay_facts(),
                ps.ground_entity_num,
                ps.pm_type,
                ps.e_flags,
                ps.f_weapon_pos_frac,
                &motion,
            );
            if let Some(ps_mut) = world.player_mut(*id) {
                ps_mut.aim_spread_scale = state.aim_spread_scale;
                ps_mut.spread_override_state = state.spread_override_state;
            }
        }
        let Some(ps) = world.player(*id).copied() else {
            return accepted;
        };
        let meta = world.client_meta(*id).cloned().unwrap_or_default();
        let started_weapon = ps.weapon;
        let ammo_weapon = if started_weapon != 0 {
            started_weapon
        } else {
            facts_weapon
        };
        let last_hand = pm_num_hands_for_held(&ps.weapons, &ps.weapon_data, ammo_weapon);
        let (meta_clip, meta_stock) = meta.ammo_for(ammo_weapon);
        let ammo_index = weapon_iw4::bg_ammo_table_key(facts.ammo_index, ammo_weapon);
        let clip_index = weapon_iw4::bg_clip_table_key(facts.clip_index, ammo_weapon);

        let stock = if bg_ammo_row_present(&ps.ammo, ammo_index) {
            bg_get_ammo_not_in_clip(&ps.ammo, ammo_index)
        } else {
            meta_stock
        };
        let clip0 = if bg_clip_row_present(&ps.ammoclip, clip_index) {
            bg_get_clip_for_hand(&ps.ammoclip, clip_index, 0)
        } else {
            meta_clip
        };
        let clip1 = if bg_clip_row_present(&ps.ammoclip, clip_index) {
            bg_get_clip_for_hand(&ps.ammoclip, clip_index, 1)
        } else {
            0
        };

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
                rechamber_pending: meta.rechamber_pending,
                delayed_rechamber: false,
                weapon_restrict_kick_time: ps.weapon_restrict_kick_time,
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
                burst_latch: false,
                rechamber_pending: false,
                delayed_rechamber: false,
                weapon_restrict_kick_time: ps.weapon_restrict_kick_time_secondary,
            },
        ];
        let mut wcmd = WeaponCmd {
            msec,
            buttons: cmd.buttons,
            old_buttons,

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

            is_in_air: {
                let pml = Pml {
                    forward: [0.0; 3],
                    right: [0.0; 3],
                    up: [0.0; 3],
                    frametime,
                    msec,
                    walking: 0,
                    ground_plane: u32::from(ps.ground_entity_num != ENTITYNUM_NONE),
                    almost_ground_plane: 0,
                    ground_trace: [0; 11],
                    previous_origin: [0.0; 3],
                    previous_velocity: [0.0; 3],
                    holdrand: 0,
                };
                pm_is_in_air(&ps, &pml)
            },
            melee_charge: MeleeChargeState {
                pm_flags: ps.pm_flags,
                pm_type: ps.pm_type,
                e_flags: ps.e_flags,
                melee_charge_yaw: ps.melee_charge_yaw,
                melee_charge_dist: ps.melee_charge_dist,
                melee_charge_time: ps.melee_charge_time,
            },

            mantle_weapon_inactive: mantle_is_weapon_inactive(&ps, true),
            mantle_quick_raise: (ps.mantle_flags & mantle_flags::QUICK) != 0,
            cmd_weapon_owned: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                w == 0 || ps.weapons.iter().any(|&slot| slot == w as i32)
            },

            cmd_weapon_pistol_quick: world
                .combat_facts_for(u32::from(cmd.weapon))
                .is_some_and(|f| f.weap_class == 5),
            switch_raise_time_ms: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                world
                    .combat_facts_for(w)
                    .map(|f| f.raise_time_ms)
                    .unwrap_or(0)
            },
            switch_quick_raise_time_ms: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                world
                    .combat_facts_for(w)
                    .map(|f| f.quick_raise_time_ms)
                    .unwrap_or(0)
            },
            perks0: ps.perks[0],
            perk_weap_reload_multiplier: weapon_iw4::PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT,
            offhand: {
                let mut inventory = [OffhandInvRow::default(); OFFHAND_INV_SLOTS];
                for (i, &slot) in ps.weapons.iter().enumerate() {
                    if slot <= 0 {
                        continue;
                    }
                    let weapon = slot as u32;
                    let (clip, stock) = meta.ammo_for(weapon);
                    let eq = world.equipment_facts_for(weapon);
                    let combat = world.combat_facts_for(weapon);
                    inventory[i] = OffhandInvRow {
                        weapon,
                        offhand_class: eq.map(|e| e.offhand_class).unwrap_or(0),
                        ammo: clip + stock,
                        hold_fire_time_ms: eq.map(|e| e.hold_fire_time_ms).unwrap_or(0),
                        fire_time_ms: combat.map(|f| f.fire_time_ms).unwrap_or(0),
                        fire_delay_ms: combat.map(|f| f.fire_delay_ms).unwrap_or(0),
                        fuse_time_ms: eq.map(|e| e.fuse_time_ms).unwrap_or(0),
                        cook_off_hold: eq.map(|e| e.cook_off_hold).unwrap_or(false),
                        offhand_hold_is_cancelable_at_0x681: combat
                            .and_then(|f| f.offhand_hold_is_cancelable_at_0x681),
                        weap_type: combat.map(|f| f.weap_type).unwrap_or(0),
                    };
                }
                OffhandCmd {
                    inventory,
                    offhand_primary: ps.offhand_primary,
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
        let events = pm_weapon_hands(&mut hands, &facts, &mut wcmd, last_hand);
        let hand0 = hands[0];

        if let Some(ps_mut) = world.player_mut(*id) {
            ps_mut.weapon = hand0.weapon;
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
        let life = world
            .client_meta(*id)
            .map(|m| m.life_sequence)
            .unwrap_or_default();
        let meta = world.client_meta_mut(*id);
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
        meta.rechamber_pending = hand0.rechamber_pending;

        if ammo_index != 0 || clip_index != 0 {
            if let Some(ps_mut) = world.player_mut(*id) {
                if ammo_index != 0 {
                    let _ = bg_set_ammo_not_in_clip(&mut ps_mut.ammo, ammo_index, hands[0].stock);
                }
                if clip_index != 0 {
                    let _ =
                        bg_set_clip_for_hand(&mut ps_mut.ammoclip, clip_index, 0, hands[0].clip);
                    if last_hand >= 1 {
                        let _ = bg_set_clip_for_hand(
                            &mut ps_mut.ammoclip,
                            clip_index,
                            1,
                            hands[1].clip,
                        );
                    }
                }
            }
        }

        if hands[0].delayed_rechamber {
            if let Some(ps) = world.player_mut(*id) {
                movement_iw4::add_predictable_event(
                    ps,
                    entity_iw4::EntityEventKind::RECHAMBER_WEAPON.0,
                    0,
                );
            }
        }
        if hands[0].clip > clip_before {
            if let Some(ps) = world.player_mut(*id) {
                movement_iw4::add_predictable_event(
                    ps,
                    entity_iw4::EntityEventKind::RELOAD_ADDAMMO.0,
                    0,
                );
            }
        }

        for slot in events.into_iter().flatten() {
            let (_hand_i, ev) = slot;
            match ev {
                WeaponTickEvent::ReloadAmmoAdded { shells: _ } => {}
                WeaponTickEvent::RechamberWeapon => {
                    if !hands[_hand_i as usize].delayed_rechamber {
                        if let Some(ps) = world.player_mut(*id) {
                            movement_iw4::add_predictable_event(
                                ps,
                                entity_iw4::EntityEventKind::RECHAMBER_WEAPON.0,
                                0,
                            );
                        }
                    }
                }
                WeaponTickEvent::EjectBrass => {
                    let kind = if _hand_i == 1 {
                        entity_iw4::EntityEventKind::EJECT_BRASS_LEFT
                    } else {
                        entity_iw4::EntityEventKind::EJECT_BRASS
                    };
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(ps, kind.0, 0);
                    }
                }
                WeaponTickEvent::ShotAccepted { ammo_used } => {
                    let hand = &hands[_hand_i as usize];
                    let shot_id = world.alloc_shot_id();
                    let weapon = hand.weapon;
                    let Some(ps) = world.player(*id).copied() else {
                        continue;
                    };
                    apply_weapon_anim_event(world, *id, ANIM_ET_FIREWEAPON);
                    let combat_seed = if facts.damage > 0 {
                        world.combat_rng_mut().next_u32()
                    } else {
                        0
                    };
                    let origin = [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ];

                    let last_shot = hand.clip == 0 && facts.fire_type != 5;
                    world.push_entity_event(
                        tick,
                        EventAudience::All,
                        entity_iw4::cg_predicted_weapon_fire_event(_hand_i as i32, last_shot),
                        crate::EntityEventPayload {
                            number: id.0 as i32,
                            weapon,
                            correlation: shot_id.0,
                            origin,
                            direction: ps.viewangles,
                            ..Default::default()
                        },
                    );
                    crate::voice::on_begin_firing(world, tick, *id, weapon);

                    if let Some(ps_mut) = world.player_mut(*id) {
                        pm_add_aim_spread_fire(
                            &mut ps_mut.aim_spread_scale,
                            ps.f_weapon_pos_frac,
                            facts.hip_spread_fire_add,
                        );
                    }
                    let aim_spread_scale = world
                        .player(*id)
                        .map(|p| p.aim_spread_scale)
                        .unwrap_or(ps.aim_spread_scale);
                    accepted.push(AcceptedShot {
                        shot_id,
                        attacker: *id,
                        attacker_life: life,
                        weapon,
                        ammo_used,
                        origin,
                        angles: ps.viewangles,
                        ads_frac: ps.f_weapon_pos_frac.clamp(0.0, 1.0),
                        view_height_current: ps.view_height_current,
                        aim_spread_scale,
                        perks0: ps.perks[0],
                        combat_seed,
                    });
                }
                WeaponTickEvent::OffhandPrepare { weapon } => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::PREP_OFFHAND.0,
                            weapon as i32,
                        );
                    }
                }
                WeaponTickEvent::OffhandUsed { weapon } => {
                    let combat = world.weapon_combat_row(weapon);
                    let meta = world.client_meta_mut(*id);
                    let (clip, stock) = meta.ammo_for(weapon);
                    if clip + stock > 0 {
                        if clip > 0 {
                            meta.set_ammo(weapon, clip - 1, stock);
                        } else {
                            meta.set_ammo(weapon, 0, stock - 1);
                        }
                    }
                    if let Some(facts) = combat {
                        if let Some(ps) = world.player_mut(*id) {
                            spend_ps_offhand_round(ps, weapon, facts);
                        }
                    }
                    if crate::equipment::spawn_offhand_projectile(world, *id, weapon, tick) {
                        if let Some(ps) = world.player(*id).copied() {
                            let origin = [
                                ps.origin[0],
                                ps.origin[1],
                                ps.origin[2] + ps.view_height_current,
                            ];
                            world.push_entity_event(
                                tick,
                                EventAudience::All,
                                entity_iw4::EntityEventKind::USE_OFFHAND,
                                crate::EntityEventPayload {
                                    number: id.0 as i32,
                                    weapon,
                                    origin,
                                    event_parm: weapon as i32,
                                    ..Default::default()
                                },
                            );
                        }
                        crate::voice::on_grenade_fire(world, tick, *id, weapon);
                    }
                }
                WeaponTickEvent::ReloadStarted => {
                    apply_weapon_anim_event(world, *id, ANIM_ET_RELOAD);
                    let hand = &hands[_hand_i as usize];
                    let event = pm_begin_reload_event(&facts, hand.clip);
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::RESET_ADS.0,
                            0,
                        );
                        movement_iw4::add_predictable_event(ps, event, 0);
                    }
                    crate::voice::on_reload_start(world, tick, *id);
                }
                WeaponTickEvent::ReloadInsert => {
                    let hand = &hands[_hand_i as usize];
                    let event = pm_reload_insert_event(&facts, hand.clip);
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(ps, event, 0);
                    }
                }
                WeaponTickEvent::ReloadEnded => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::RELOAD_END.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::EmptyClick => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::NOAMMO.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::PutawayStarted => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::PUTAWAY_WEAPON.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::RaiseStarted => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::RAISE_WEAPON.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::MeleeFired => {
                    let hand = &hands[_hand_i as usize];
                    let weapon = hand.weapon;
                    let Some(ps) = world.player(*id).copied() else {
                        continue;
                    };
                    let origin = [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ];
                    world.push_entity_event(
                        tick,
                        EventAudience::All,
                        entity_iw4::EntityEventKind::FIRE_MELEE,
                        crate::EntityEventPayload {
                            number: id.0 as i32,
                            weapon,
                            origin,
                            direction: ps.viewangles,
                            ..Default::default()
                        },
                    );
                    if world.publishes_snapshot() {
                        fire_weapon_melee(world, tick, *id, life, weapon, origin, ps.viewangles);
                    }
                }
                WeaponTickEvent::RaiseFinished | WeaponTickEvent::DropFinished => {}
            }
        }
    }
    accepted
}

fn apply_weapon_anim_event(world: &mut FrameWorld, id: ClientId, event: u8) {
    let Some(script) = world.player_anim_script() else {
        world.count_fire_anim_gap();
        return;
    };
    let mut seed = world.anim_event_seed();
    let (view_w, primary) = {
        let Some(ps) = world.player_mut(id) else {
            world.count_fire_anim_gap();
            return;
        };
        crate::pmove_anim_weapon_ids(ps)
    };
    let view_facts = world.combat_facts_for(view_w);
    let primary_facts = world.combat_facts_for(primary);
    let applied = {
        let Some(ps) = world.player_mut(id) else {
            world.count_fire_anim_gap();
            return;
        };
        let conds = crate::anim_conditions_from_pmove(ps, view_facts, primary_facts);
        script.apply_event(ps, event, &conds, &mut seed)
    };
    world.set_anim_event_seed(seed);
    if !applied {
        world.count_fire_anim_gap();
    }
}

pub fn spread_pellet_direction(
    angles: [f32; 3],
    spread_degrees: f32,
    rng: &mut MatchRng,
) -> [f32; 3] {
    let forward = angles_to_forward(angles);
    if spread_degrees <= 0.0 {
        return forward;
    }
    let yaw = angles[1].to_radians();
    let right = [yaw.sin(), -yaw.cos(), 0.0];
    let up = [
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    ];
    let unit = |draw: u32| draw as f32 / u32::MAX as f32;
    let radius = unit(rng.next_u32()).sqrt() * spread_degrees.to_radians().tan();
    let theta = unit(rng.next_u32()) * core::f32::consts::TAU;
    let x = radius * theta.cos();
    let y = radius * theta.sin();
    let direction = [
        forward[0] + right[0] * x + up[0] * y,
        forward[1] + right[1] * x + up[1] * y,
        forward[2] + right[2] * x + up[2] * y,
    ];
    let len =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    if len > 0.0 {
        [direction[0] / len, direction[1] / len, direction[2] / len]
    } else {
        forward
    }
}

pub(crate) fn phase_emit(world: &FrameWorld, shots: &[AcceptedShot]) -> Vec<Emission> {
    let mut out = Vec::new();
    for shot in shots {
        let Some(facts) = world.combat_facts_for(shot.weapon) else {
            continue;
        };

        match fire_weapon_kind(facts.weap_type, facts.weap_class) {
            Some(FireWeaponKind::Bullet) => {
                if facts.damage <= 0 || facts.bullet_range() <= 0.0 {
                    continue;
                }
            }
            _ => continue,
        }
        let ads_frac = shot.ads_frac.clamp(0.0, 1.0);
        let cone = bg_get_spread_for_weapon(
            shot.view_height_current,
            0,
            SpreadOverrideState::None,
            &facts.spread_facts(),
            weapon_iw4::perk_weap_spread_multiplier(shot.perks0),
        );

        let spread =
            fire_weapon_spread_degrees(cone, facts.ads_spread, ads_frac, shot.aim_spread_scale);
        let mut rng = MatchRng::new(shot.combat_seed as u64);
        let pellet_count = facts.pellet_count().clamp(1, u16::MAX as i32) as u16;
        for pellet in 0..pellet_count {
            out.push(Emission {
                shot_id: shot.shot_id,
                pellet: PelletId(pellet),
                attacker: shot.attacker,
                attacker_life: shot.attacker_life,
                weapon: shot.weapon,
                origin: shot.origin,
                direction: spread_pellet_direction(shot.angles, spread, &mut rng),
                max_range: facts.bullet_range(),
                base_damage: facts.damage,
            });
        }
    }
    out
}

pub(crate) fn phase_trace(
    world: &mut FrameWorld,
    tick: Tick,
    emissions: &[Emission],
) -> TracePhaseOutput {
    let mut output = TracePhaseOutput::default();
    for em in emissions {
        let Some(facts) = world.combat_facts_for(em.weapon) else {
            continue;
        };
        let query = world.lagcomp_query_for(em.attacker, tick);
        let end = [
            em.origin[0] + em.direction[0] * em.max_range,
            em.origin[1] + em.direction[1] * em.max_range,
            em.origin[2] + em.direction[2] * em.max_range,
        ];
        let pen = world.bullet_pen_facts_for(em.weapon);
        let glass_pairs = world.world_objects().glass_damage_pairs();
        let (segments, terminal) = bullet_trace_segments_filtered(
            world.clip_brushes(),
            world.clip_bsp(),
            world.clip_cmodels(),
            world.clip_mesh(),
            &query.players.poses,
            &query.entities.rows,
            &BulletTraceQuery {
                start: em.origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(em.attacker),
                ignore_hit: None,
            },
            pen,
            world.penetration_table(),
            &|piece| {
                crate::world_objects::glass_piece_is_solid(
                    glass_pairs
                        .iter()
                        .find(|(id, _)| *id == u32::from(piece))
                        .map(|(_, d)| *d)
                        .unwrap_or(0),
                )
            },
        );
        let entity_epoch = entity_collision_epoch(terminal, &query.entities.rows);
        let startsolid = segments.first().is_some_and(|s| s.startsolid);
        let impact_n = segments
            .iter()
            .filter(|s| bullet_process_on_hit(s.collider))
            .count() as u32;
        let event_n = segments
            .iter()
            .filter(|s| {
                bullet_process_on_hit(s.collider)
                    && entity_iw4::bg_bullet_hit_event(facts.impact_type, false).is_some()
            })
            .count() as u32;
        let (bone_center, bone_half_size, xmodel_contents, model_key) =
            dobj_hit_dump(terminal, &query.entities.rows);
        output.shot_verdicts.push(ShotCollisionVerdict {
            shot_id: em.shot_id,
            pellet: em.pellet,
            attacker: em.attacker,
            geometry: shot_collision_geometry(terminal, query.players.verdict, entity_epoch),
            terminal,
            startsolid,
            bone_center,
            bone_half_size,
            xmodel_contents,
            model_key,
            end: segments.last().map(|s| s.end),
            impact_n,
            event_n,
        });
        if segments.is_empty() {
            continue;
        }
        for segment in &segments {
            let exit = segment.surface_flags & fx_iw4::FX_IMPACT_EXIT_SURFACE_FLAG != 0;
            let dist = {
                let dx = segment.end[0] - em.origin[0];
                let dy = segment.end[1] - em.origin[1];
                let dz = segment.end[2] - em.origin[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            };
            let scaled =
                ((bullet_damage_at_distance(&facts, dist) as f32) * segment.damage_mult) as i32;
            let mut flesh_flags = 0u8;
            if !exit
                && let Some(ColliderId::Player {
                    client: victim,
                    hitloc,
                }) = segment.collider
            {
                let head = hud_iw4::obituary_is_headshot(hitloc);
                let mut fatal = false;
                if world.publishes_snapshot()
                    && scaled > 0
                    && let Some(vmeta) = world.client_meta(victim)
                    && vmeta.lifecycle == ClientLifecycle::Alive
                {
                    let attempt = crate::DamageAttempt {
                        source: DamageSource::Shot(em.shot_id),
                        pellet: em.pellet,
                        attacker: em.attacker,
                        attacker_life: em.attacker_life,
                        target: victim,
                        target_life: vmeta.life_sequence,
                        weapon: em.weapon,
                        amount: scaled,
                        killcam_entity_start_time: 0,
                        inflictor_origin: None,
                        hitloc,
                    };
                    fatal = matches!(
                        crate::damage::apply_damage_attempt(world, tick, &attempt),
                        crate::DamageOutcome::Died(_)
                    );
                }
                flesh_flags = fx_iw4::fx_flesh_hit_flags(head, fatal) as u8;
            }
            let payload = crate::EntityEventPayload {
                number: em.attacker.0 as i32,
                attacker_entity_num: em.attacker.0 as i32,
                other_entity_num: match segment.collider {
                    Some(
                        ColliderId::EntityDObjBone { owner, .. }
                        | ColliderId::EntityLinkedBrush { owner, .. },
                    ) => owner
                        .script_model()
                        .and_then(|id| world.gentity_number(id))
                        .unwrap_or(i32::from(trace_iw4::ENTITYNUM_NONE)),
                    Some(ColliderId::Player { client, .. }) => client.0 as i32,
                    _ => i32::from(trace_iw4::ENTITYNUM_NONE),
                },
                event_parm: i32::from(flesh_flags),
                weapon: em.weapon,
                correlation: em.shot_id.0,
                origin: segment.end,
                origin2: segment.start,
                direction: segment.normal,
                surf_type: segment.surf_type,
                surface_flags: segment.surface_flags,
                simulation_flags: u8::from(segment.penetrated),
                ..Default::default()
            };
            let victim = match segment.collider {
                Some(ColliderId::Player { client, .. }) => Some(client),
                _ => None,
            };
            if bullet_process_on_hit(segment.collider) {
                if let Some(world_event) = entity_iw4::bg_bullet_hit_event(facts.impact_type, false)
                {
                    let world_audience =
                        victim.map_or(EventAudience::All, EventAudience::AllExcept);
                    world.push_entity_event(tick, world_audience, world_event, payload);
                    if let Some(victim) = victim
                        && let Some(local_event) =
                            entity_iw4::bg_bullet_hit_event(facts.impact_type, true)
                    {
                        world.push_entity_event(
                            tick,
                            EventAudience::Client(victim),
                            local_event,
                            payload,
                        );
                    }
                } else if fx_iw4::fx_impact_table_row(facts.impact_type, false).is_some() {
                    world.push_pellet_fx(crate::PelletFxRecord {
                        attacker: em.attacker.0 as i32,
                        weapon: em.weapon,
                        correlation: em.shot_id.0,
                        pellet: em.pellet.0,
                        start: segment.start,
                        end: segment.end,
                        normal: segment.normal,
                        surf_type: segment.surf_type,
                        surface_flags: segment.surface_flags,
                        flesh_flags,
                    });
                }
            }
            if exit || !world.publishes_snapshot() || scaled <= 0 {
                continue;
            }
            match segment.collider {
                Some(ColliderId::Player { .. }) => {}
                Some(
                    ColliderId::EntityDObjBone { owner, .. }
                    | ColliderId::EntityLinkedBrush { owner, .. },
                ) => {
                    if let Some(ColliderId::EntityDObjBone { bone, .. }) = segment.collider
                        && crate::t5_destructible::apply_hit(
                            world,
                            tick,
                            owner,
                            bone,
                            scaled as u32,
                        )
                    {
                        continue;
                    }
                    if let Some(ColliderId::EntityDObjBone { bone, .. }) = segment.collider
                        && crate::vehicle_glass::apply_hit(world, tick, owner, bone, scaled as u32)
                    {
                        continue;
                    }
                    if let (Some(target), Some(epoch)) = (owner.script_model(), entity_epoch) {
                        let intent = crate::DestructibleDamageIntent {
                            source: DamageSource::Shot(em.shot_id),
                            pellet: em.pellet,
                            attacker: em.attacker,
                            attacker_life: em.attacker_life,
                            target,
                            amount: scaled as u32,
                            epoch,
                        };
                        let report = world
                            .world_objects_mut()
                            .apply_destructible_damage_batch(&[intent]);
                        for explode in &report.explodes {
                            let attempts =
                                crate::damage::radius_attempts_from_truck_explode(world, explode);
                            for attempt in &attempts {
                                let _ = crate::damage::apply_damage_attempt(world, tick, attempt);
                            }
                        }
                    }
                }
                Some(ColliderId::World { .. }) => {
                    if let Some(piece) = glass_piece_from_hit(segment.hit_type, segment.hit_id) {
                        let at_time_ms = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS))
                            .unwrap_or(i32::MAX);
                        let mut holdrand = *world.stuck_holdrand_mut();
                        world.world_objects_mut().apply_glass_hit(
                            piece,
                            scaled as u32,
                            at_time_ms,
                            segment.end,
                            em.direction,
                            &mut || crate::item::g_random(&mut holdrand),
                        );
                        *world.stuck_holdrand_mut() = holdrand;
                    }
                }
                None => {}
            }
        }
    }
    world.record_shot_collision_verdicts(&output.shot_verdicts);
    output
}

fn fire_weapon_melee(
    world: &mut FrameWorld,
    tick: Tick,
    attacker: ClientId,
    attacker_life: LifeSequence,
    weapon: u32,
    origin: [f32; 3],
    angles: [f32; 3],
) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    if facts.melee_damage <= 0 {
        return;
    }
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    let range = PLAYER_MELEE_RANGE_DEFAULT;
    let width = PLAYER_MELEE_WIDTH_DEFAULT;
    let height = PLAYER_MELEE_HEIGHT_DEFAULT;
    let query = world.lagcomp_query_for(attacker, tick);
    let glass_pairs = world.world_objects().glass_damage_pairs();
    let is_solid = |piece| {
        crate::world_objects::glass_piece_is_solid(
            glass_pairs
                .iter()
                .find(|(id, _)| *id == u32::from(piece))
                .map(|(_, d)| *d)
                .unwrap_or(0),
        )
    };
    let mut best_frac = 1.0f32;
    let mut best_hit: Option<crate::bullet_collision::BulletTraceSegment> = None;
    let n = melee_trace_count(width, height);
    for offset in MELEE_TRACE_OFFSETS.iter().take(n) {
        let end = melee_trace_end(origin, forward, right, up, range, width, height, *offset);
        let (segments, _) = bullet_trace_segments_filtered(
            world.clip_brushes(),
            world.clip_bsp(),
            world.clip_cmodels(),
            world.clip_mesh(),
            &query.players.poses,
            &query.entities.rows,
            &BulletTraceQuery {
                start: origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(attacker),
                ignore_hit: None,
            },
            weapon_iw4::BulletPenFacts::default(),
            world.penetration_table(),
            &is_solid,
        );
        let Some(segment) = segments.iter().find(|s| s.collider.is_some()) else {
            continue;
        };
        if segment.surface_flags & 0x10 != 0 {
            continue;
        }
        let ray = [end[0] - origin[0], end[1] - origin[1], end[2] - origin[2]];
        let hit = [
            segment.end[0] - origin[0],
            segment.end[1] - origin[1],
            segment.end[2] - origin[2],
        ];
        let ray_len2 = ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2];
        if ray_len2 <= 0.0 {
            continue;
        }
        let frac = (hit[0] * hit[0] + hit[1] * hit[1] + hit[2] * hit[2]).sqrt() / ray_len2.sqrt();
        if frac >= 1.0 || frac > best_frac {
            continue;
        }
        best_frac = frac;
        best_hit = Some(*segment);
    }
    let Some(segment) = best_hit else {
        return;
    };
    let amount = facts.melee_damage + (world.combat_rng_mut().next_u32() % 5) as i32;
    match segment.collider {
        Some(ColliderId::Player {
            client: victim,
            hitloc,
        }) => {
            if let Some(vmeta) = world.client_meta(victim)
                && vmeta.lifecycle == ClientLifecycle::Alive
            {
                let attempt = crate::DamageAttempt {
                    source: DamageSource::Melee,
                    pellet: PelletId(0),
                    attacker,
                    attacker_life,
                    target: victim,
                    target_life: vmeta.life_sequence,
                    weapon,
                    amount,
                    killcam_entity_start_time: 0,
                    inflictor_origin: Some(origin),
                    hitloc,
                };
                let _ = crate::damage::apply_damage_attempt(world, tick, &attempt);
                world.push_entity_event(
                    tick,
                    EventAudience::All,
                    entity_iw4::EntityEventKind::MELEE_BLOOD,
                    crate::EntityEventPayload {
                        number: attacker.0 as i32,
                        attacker_entity_num: attacker.0 as i32,
                        other_entity_num: victim.0 as i32,
                        weapon,
                        origin: segment.end,
                        direction: forward,
                        surf_type: segment.surf_type,
                        surface_flags: segment.surface_flags,
                        ..Default::default()
                    },
                );
            }
        }
        Some(ColliderId::World { .. }) => {
            if let Some(piece) = glass_piece_from_hit(segment.hit_type, segment.hit_id) {
                let at_time_ms =
                    i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
                let mut holdrand = *world.stuck_holdrand_mut();
                world.world_objects_mut().apply_glass_hit(
                    piece,
                    u32::from(crate::world_objects::GLASS_MELEE_DAMAGE),
                    at_time_ms,
                    segment.end,
                    forward,
                    &mut || crate::item::g_random(&mut holdrand),
                );
                *world.stuck_holdrand_mut() = holdrand;
            }
        }
        _ => {}
    }
}

fn entity_collision_epoch(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityCollisionEpoch> {
    let owner = match terminal? {
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. } => {
            owner
        }
        ColliderId::World { .. } | ColliderId::Player { .. } => return None,
    };
    rows.iter()
        .find(|row| row.owner == owner)
        .map(|row| row.epoch)
}

fn dobj_hit_dump(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> (
    Option<[f32; 3]>,
    Option<[f32; 3]>,
    Option<u32>,
    Option<String>,
) {
    let Some(ColliderId::EntityDObjBone { owner, bone, .. }) = terminal else {
        return (None, None, None, None);
    };
    let Some(row) = rows.iter().find(|row| row.owner == owner) else {
        return (None, None, None, None);
    };
    let bone = row
        .collision
        .as_ref()
        .and_then(|collision| collision.bones.iter().find(|b| b.bone == bone));
    (
        bone.map(|b| b.center),
        bone.map(|b| b.half_size),
        row.dobj_contents,
        row.model_key.clone(),
    )
}

fn shot_collision_geometry(
    terminal: Option<ColliderId>,
    player_history: HistorySampleVerdict,
    entity_epoch: Option<EntityCollisionEpoch>,
) -> ShotCollisionGeometry {
    match terminal {
        Some(ColliderId::Player { .. }) => ShotCollisionGeometry::Player {
            representation: PlayerCollisionRepresentation::StandingAabbV1,
            history: player_history,
        },
        Some(ColliderId::EntityDObjBone { .. } | ColliderId::EntityLinkedBrush { .. }) => {
            match entity_epoch {
                Some(epoch) => ShotCollisionGeometry::Entity { epoch },
                None => ShotCollisionGeometry::Miss,
            }
        }
        Some(ColliderId::World { .. }) => ShotCollisionGeometry::World,
        None => match player_history {
            HistorySampleVerdict::Refused { .. } => ShotCollisionGeometry::Player {
                representation: PlayerCollisionRepresentation::StandingAabbV1,
                history: player_history,
            },
            _ => ShotCollisionGeometry::Miss,
        },
    }
}

pub(crate) fn bullet_process_on_hit(collider: Option<ColliderId>) -> bool {
    collider.is_some()
}

fn spend_ps_offhand_round(ps: &mut PlayerState, weapon: u32, facts: weapon_iw4::WeaponCombatFacts) {
    let clip_key = bg_clip_table_key(facts.clip_index, weapon);
    if clip_key != 0 && bg_clip_row_present(&ps.ammoclip, clip_key) {
        let clip = bg_get_clip_for_hand(&ps.ammoclip, clip_key, 0);
        if clip > 0 {
            let _ = bg_set_clip_for_hand(&mut ps.ammoclip, clip_key, 0, clip - 1);
            return;
        }
    }
    let ammo_key = bg_ammo_table_key(facts.ammo_index, weapon);
    if ammo_key != 0 && bg_ammo_row_present(&ps.ammo, ammo_key) {
        let stock = bg_get_ammo_not_in_clip(&ps.ammo, ammo_key);
        if stock > 0 {
            let _ = bg_set_ammo_not_in_clip(&mut ps.ammo, ammo_key, stock - 1);
        }
    }
}
