use crate::frame::FrameWorld;
use crate::match_state::{CarePackage, ClientLifecycle, PaveLow, RemoteMissile, Uav};
use crate::world::{ClientId, Tick, inventory_add_weapon};
use gamemode_iw4::killstreaks::{self, Killstreak};

pub const fn model_source(kind: u32, id: u32) -> u32 {
    0x8000_0000 | (kind << 28) | (id & 0x0fff_ffff)
}

pub const UAV_MODEL_KIND: u32 = 1;
pub const CRATE_MODEL_KIND: u32 = 2;
pub const PAVELOW_MODEL_KIND: u32 = 3;
pub const LITTLE_BIRD_MODEL_KIND: u32 = 4;

fn model_id(kind: u32, id: u32) -> crate::ScriptModelId {
    crate::ScriptModelId::from_wire(model_source(kind, id))
}

fn remove_model(world: &mut FrameWorld, kind: u32, id: u32) {
    if let Some(number) = world.gentity_number(model_id(kind, id)) {
        world.remove_script_mover_by_number(number);
    }
}

pub(crate) fn on_death(world: &mut FrameWorld, victim: ClientId) {
    let meta = world.client_meta_mut(victim);
    meta.kill_streak = 0;
    meta.last_earned_streak = None;
}

pub(crate) fn remember_combat_weapon(world: &mut FrameWorld, id: ClientId) {
    let Some(weapon) = world.player(id).map(|ps| ps.weapon).filter(|&w| w != 0) else {
        return;
    };
    let name = world.weapon_script_name(weapon);
    if name.starts_with("killstreak_") || name == killstreaks::AIRDROP_MARKER_WEAPON {
        return;
    }
    world.client_meta_mut(id).last_combat_weapon = weapon;
}

pub(crate) fn on_kill(world: &mut FrameWorld, victim: ClientId, attacker: ClientId) {
    let victim_streak = world.client_meta(victim).map_or(0, |m| m.kill_streak);
    let victim_hardline = world.client_meta(victim).is_some_and(|m| {
        m.loadout.as_ref().is_some_and(|loadout| {
            crate::match_state::class_catalog_has(
                loadout.perks,
                crate::match_state::CLASS_CATALOG_HARDLINE,
            )
        })
    });
    if killstreaks::is_buzzkill(
        &killstreaks::DEFAULT_LOADOUT,
        victim_streak,
        killstreaks::streak_modifier(victim_hardline),
    ) {
        world.push_hud_splash(attacker, "buzzkill", 0, 100);
    }
    if !world
        .client_meta(attacker)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    let (count, last, hardline) = {
        let meta = world.client_meta_mut(attacker);
        meta.kill_streak = meta.kill_streak.saturating_add(1);
        (
            meta.kill_streak,
            meta.last_earned_streak,
            meta.loadout.as_ref().is_some_and(|loadout| {
                crate::match_state::class_catalog_has(
                    loadout.perks,
                    crate::match_state::CLASS_CATALOG_HARDLINE,
                )
            }),
        )
    };
    let awards: Vec<_> = killstreaks::earned(
        &killstreaks::DEFAULT_LOADOUT,
        count,
        last,
        killstreaks::streak_modifier(hardline),
    )
    .collect();
    let awarded = !awards.is_empty();
    for (streak, _count) in awards {
        let meta = world.client_meta_mut(attacker);
        meta.last_earned_streak = Some(streak);
        meta.owned_streaks.insert(0, streak);
        world.push_hud_splash(attacker, streak.pickup_splash(), 0, 0);
    }
    if awarded {
        sync_inventory(world, attacker);
    }
}

pub(crate) fn sync_inventory(world: &mut FrameWorld, id: ClientId) {
    let Some(owned) = world.client_meta(id).map(|m| m.owned_streaks.clone()) else {
        return;
    };
    let weapons: Vec<_> = owned
        .iter()
        .filter_map(|streak| {
            world
                .weapon_index_by_script_name(streak.weapon())
                .map(|index| {
                    let facts = world.combat_facts_for(index);
                    let (mut clip, _, mut stock) =
                        facts.map_or((0, 0, 0), |f| weapon_iw4::spawn_clip_stock(&f, 0));
                    if let Some(eq) = world
                        .equipment_facts_for(index)
                        .filter(|eq| eq.is_offhand())
                    {
                        clip = eq.spawn_clip_count();
                        stock = 0;
                    }
                    (*streak, index, facts, clip, stock)
                })
        })
        .collect();
    if let Some(ps) = world.player_mut(id) {
        for (_, weapon, facts, clip, stock) in &weapons {
            inventory_add_weapon(ps, *weapon, false);
            if let Some(facts) = facts {
                crate::step::seed_ps_ammo_tables(ps, *weapon, facts, *clip, 0, false, *stock);
            }
        }
        if let Some((_, weapon, _, _, _)) = weapons
            .first()
            .filter(|(_, weapon, _, _, _)| ps.weapons.contains(&(*weapon as i32)))
        {
            ps.action_slot_type[3] = 1;
            ps.action_slot_param[3] = *weapon as i32;
        } else {
            ps.action_slot_type[3] = 0;
            ps.action_slot_param[3] = 0;
        }
    }
    for (_, weapon, facts, clip, stock) in weapons {
        if facts.is_some() {
            world.client_meta_mut(id).set_ammo(weapon, clip, stock);
        }
    }
}

pub(crate) fn use_selected(world: &mut FrameWorld, tick: Tick, id: ClientId) {
    let Some(streak) = world
        .client_meta(id)
        .filter(|m| m.lifecycle == ClientLifecycle::Alive)
        .and_then(|m| m.owned_streaks.first().copied())
    else {
        return;
    };
    let Some(weapon) = world.weapon_index_by_script_name(streak.weapon()) else {
        return;
    };
    if !world.player(id).is_some_and(|ps| ps.weapon == weapon) {
        return;
    }

    match streak {
        Killstreak::Uav => {
            let team = world.client_meta(id).map_or(0, |m| m.client_state_team);
            let team_based = world.bootstrap_ref().kind.is_team();
            let until =
                crate::level_time_ms(tick).saturating_add(killstreaks::UAV_DURATION_MS as i32);
            for recipient in world.client_ids_sorted() {
                if recipient == id
                    || (team_based
                        && world
                            .client_meta(recipient)
                            .is_some_and(|m| m.client_state_team == team))
                {
                    let meta = world.client_meta_mut(recipient);
                    meta.radar_until_ms = meta.radar_until_ms.max(until);
                }
            }
            let center = world.player(id).map_or([0.0; 3], |ps| ps.origin);
            let id_value = tick.0.wrapping_mul(256).wrapping_add(id.0);
            let origin = [center[0] + 6_000.0, center[1], center[2] + 4_000.0];
            world.uavs.push(Uav {
                id: id_value,
                owner: id,
                team,
                center,
                origin,
                started_at_ms: crate::level_time_ms(tick),
                expires_at_ms: until,
            });
            let _ = world.spawn_script_mover(model_id(UAV_MODEL_KIND, id_value), origin, [0.0; 3]);
        }
        Killstreak::Airdrop => return,
        Killstreak::PredatorMissile => {
            if world.publishes_snapshot() && !launch_predator(world, tick, id) {
                return;
            }
        }
        Killstreak::HelicopterFlares => {
            if world.pave_lows.len() >= 1 {
                return;
            }
            let now = crate::level_time_ms(tick);
            let center = world.player(id).map_or([0.0, 0.0, 850.0], |ps| {
                [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE,
                ]
            });
            let origin = [center[0] + 1_800.0, center[1], center[2]];
            let team = world.client_meta(id).map_or(0, |m| m.client_state_team);
            let burst_remaining = killstreaks::PAVELOW_BURST[0]
                + world.combat_rng_mut().next_index(
                    (killstreaks::PAVELOW_BURST[1] - killstreaks::PAVELOW_BURST[0] + 1) as usize,
                ) as u32;
            let id_value = tick.0.wrapping_mul(256).wrapping_add(id.0);
            world.pave_lows.push(PaveLow {
                id: id_value,
                owner: id,
                team,
                origin,
                center,
                started_at_ms: now,
                expires_at_ms: now.saturating_add(killstreaks::PAVELOW_LOOP_MS as i32),
                health: 3_000,
                next_shot_ms: now.saturating_add(1_000),
                burst_remaining,
            });
            let _ =
                world.spawn_script_mover(model_id(PAVELOW_MODEL_KIND, id_value), origin, [0.0; 3]);
        }
    }
    consume_top(world, id, streak, weapon);
}

pub(crate) fn marker_fired(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    if world.weapon_script_name(weapon) != killstreaks::AIRDROP_MARKER_WEAPON {
        return;
    }
    if world.client_meta(id).and_then(|m| m.owned_streaks.first()) != Some(&Killstreak::Airdrop) {
        return;
    }
    consume_top(world, id, Killstreak::Airdrop, weapon);
}

fn consume_top(world: &mut FrameWorld, id: ClientId, streak: Killstreak, weapon: u32) {
    world.client_meta_mut(id).owned_streaks.remove(0);
    let still_owned = world
        .client_meta(id)
        .is_some_and(|m| m.owned_streaks.contains(&streak));
    let remembered = world.client_meta(id).map_or(0, |m| m.last_combat_weapon);
    let restore = world.player(id).map_or(0, |ps| {
        if remembered != 0 && ps.weapons.contains(&(remembered as i32)) {
            remembered
        } else {
            ps.weapons
                .iter()
                .filter_map(|&w| (w > 0).then_some(w as u32))
                .find(|&w| {
                    let name = world.weapon_script_name(w);
                    !name.starts_with("killstreak_") && name != killstreaks::AIRDROP_MARKER_WEAPON
                })
                .unwrap_or(0)
        }
    });
    if let Some(ps) = world.player_mut(id) {
        ps.weapon = restore;
        ps.weapon_primary = restore;
        ps.weaponstate_primary = 0;
        ps.weapon_time = 0;
        ps.weapon_delay = 0;
        if !still_owned {
            for slot in &mut ps.weapons {
                if *slot == weapon as i32 {
                    *slot = 0;
                }
            }
        }
    }
    sync_inventory(world, id);
}

pub(crate) fn marker_impact(
    world: &mut FrameWorld,
    tick: Tick,
    owner: ClientId,
    id: u32,
    origin: [f32; 3],
) {
    if !world.publishes_snapshot() || world.care_packages.iter().any(|p| p.id == id) {
        return;
    }
    let now = crate::level_time_ms(tick);
    let flight_ms = (killstreaks::FLYBY_DISTANCE
        / (killstreaks::FLYBY_APPROACH_MPH * killstreaks::MPH_TO_UNITS)
        * 1000.0) as i32
        + killstreaks::FLYBY_SLOW_AFTER_MS as i32;
    let ready_at_ms = now
        .saturating_add(flight_ms)
        .saturating_add(killstreaks::CRATE_DROP_MS as i32);
    let team = world.client_meta(owner).map_or(0, |m| m.client_state_team);
    let contents = killstreaks::crate_contents(world.combat_rng_mut().next_u32());
    world.care_packages.push(CarePackage {
        id,
        owner,
        team,
        origin,
        contents,
        ready_at_ms,
        expires_at_ms: ready_at_ms.saturating_add(killstreaks::CRATE_TIMEOUT_MS as i32),
        capturer: None,
        capture_ms: 0,
    });
    let start = [
        origin[0] - killstreaks::FLYBY_DISTANCE,
        origin[1],
        origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE,
    ];
    let _ = world.spawn_script_mover(model_id(LITTLE_BIRD_MODEL_KIND, id), start, [0.0; 3]);
}

pub(crate) fn advance_crates(
    world: &mut FrameWorld,
    tick: Tick,
    msec: i32,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    let now = crate::level_time_ms(tick);
    let mut packages = std::mem::take(&mut world.care_packages);
    for mut package in packages.drain(..) {
        if now >= package.expires_at_ms {
            remove_model(world, CRATE_MODEL_KIND, package.id);
            remove_model(world, LITTLE_BIRD_MODEL_KIND, package.id);
            continue;
        }
        let drop_at = package.ready_at_ms - killstreaks::CRATE_DROP_MS as i32;
        if now < drop_at {
            let flight_ms = (killstreaks::FLYBY_DISTANCE
                / (killstreaks::FLYBY_APPROACH_MPH * killstreaks::MPH_TO_UNITS)
                * 1000.0) as i32
                + killstreaks::FLYBY_SLOW_AFTER_MS as i32;
            let fraction =
                ((now - (drop_at - flight_ms)) as f32 / flight_ms.max(1) as f32).clamp(0.0, 1.0);
            let bird = [
                package.origin[0] - killstreaks::FLYBY_DISTANCE * (1.0 - fraction),
                package.origin[1],
                package.origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE,
            ];
            if let Some(number) = world.gentity_number(model_id(LITTLE_BIRD_MODEL_KIND, package.id))
            {
                world.set_script_mover_origin(number, bird);
            }
            world.care_packages.push(package);
            continue;
        }
        remove_model(world, LITTLE_BIRD_MODEL_KIND, package.id);
        if world
            .gentity_number(model_id(CRATE_MODEL_KIND, package.id))
            .is_none()
        {
            let _ = world.spawn_script_mover(
                model_id(CRATE_MODEL_KIND, package.id),
                [
                    package.origin[0],
                    package.origin[1],
                    package.origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE,
                ],
                [0.0; 3],
            );
        }
        let fraction = ((now - drop_at) as f32 / killstreaks::CRATE_DROP_MS as f32).clamp(0.0, 1.0);
        if let Some(number) = world.gentity_number(model_id(CRATE_MODEL_KIND, package.id)) {
            world.set_script_mover_origin(
                number,
                [
                    package.origin[0],
                    package.origin[1],
                    package.origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE * (1.0 - fraction),
                ],
            );
        }
        if now < package.ready_at_ms {
            world.care_packages.push(package);
            continue;
        }
        let claimant = cmds.iter().find_map(|(id, cmd)| {
            if cmd.buttons & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                == 0
            {
                return None;
            }
            let meta = world.client_meta(*id)?;
            if meta.lifecycle != ClientLifecycle::Alive {
                return None;
            }
            let ps = world.player(*id)?;
            let dist2 = ps
                .origin
                .iter()
                .zip(package.origin)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>();
            (dist2 <= killstreaks::CRATE_USE_RADIUS.powi(2)).then_some(*id)
        });
        if claimant != package.capturer {
            package.capturer = claimant;
            package.capture_ms = 0;
        }
        if let Some(id) = claimant {
            package.capture_ms = package.capture_ms.saturating_add(msec);
            let needed = if id == package.owner {
                killstreaks::CRATE_OWNER_USE_MS
            } else {
                killstreaks::CRATE_OTHER_USE_MS
            };
            if package.capture_ms >= needed {
                capture_crate(world, id, &package, now);
                remove_model(world, CRATE_MODEL_KIND, package.id);
                continue;
            }
        }
        world.care_packages.push(package);
    }
}

fn capture_crate(world: &mut FrameWorld, id: ClientId, package: &CarePackage, now: i32) {
    if world.bootstrap_ref().kind.is_team()
        && world
            .client_meta(id)
            .is_some_and(|m| m.client_state_team != package.team)
    {
        crate::score::award_splash(world, id, "hijacker", 100, now);
    }
    match package.contents {
        killstreaks::CrateContents::Ammo => {
            let weapons = world.player(id).map_or(Vec::new(), |ps| {
                ps.weapons
                    .iter()
                    .filter_map(|&w| (w > 0).then_some(w as u32))
                    .collect()
            });
            for weapon in weapons {
                let Some(facts) = world.combat_facts_for(weapon) else {
                    continue;
                };
                let stock = facts.max_ammo.max(facts.start_ammo);
                let clip = facts.clip_size.max(0);
                if let Some(ps) = world.player_mut(id) {
                    crate::step::seed_ps_ammo_tables(ps, weapon, &facts, clip, clip, false, stock);
                }
                world.client_meta_mut(id).set_ammo(weapon, clip, stock);
            }
        }
        killstreaks::CrateContents::Streak(streak) => {
            world.client_meta_mut(id).owned_streaks.insert(0, streak);
            world.push_hud_splash(id, streak.pickup_splash(), 0, 0);
            sync_inventory(world, id);
        }
    }
}

pub(crate) fn advance_pave_lows(world: &mut FrameWorld, tick: Tick) {
    if !world.publishes_snapshot() {
        return;
    }
    let now = crate::level_time_ms(tick);
    let mut helis = std::mem::take(&mut world.pave_lows);
    for mut heli in helis.drain(..) {
        if now >= heli.expires_at_ms || heli.health <= 0 {
            remove_model(world, PAVELOW_MODEL_KIND, heli.id);
            continue;
        }
        let angle = (now - heli.started_at_ms) as f32 / 1000.0
            * killstreaks::PAVELOW_DEFAULT_MPH
            * killstreaks::MPH_TO_UNITS
            / 1_800.0;
        heli.origin = [
            heli.center[0] + angle.cos() * 1_800.0,
            heli.center[1] + angle.sin() * 1_800.0,
            heli.center[2],
        ];
        if let Some(number) = world.gentity_number(model_id(PAVELOW_MODEL_KIND, heli.id)) {
            world.set_script_mover_origin(number, heli.origin);
        }
        if now >= heli.next_shot_ms {
            let team_based = world.bootstrap_ref().kind.is_team();
            let target = world
                .client_ids_sorted()
                .into_iter()
                .filter(|&id| id != heli.owner)
                .filter_map(|id| {
                    let meta = world.client_meta(id)?;
                    let ps = world.player(id)?;
                    if meta.lifecycle != ClientLifecycle::Alive
                        || (team_based && meta.client_state_team == heli.team)
                        || now.saturating_sub(meta.item_use_spawn_ms)
                            < killstreaks::PAVELOW_SPAWN_PROTECTION_MS
                        || ps.perks[0] & playerstate_iw4::PERK_COLDBLOODED != 0
                    {
                        return None;
                    }
                    let dist2 = ps
                        .origin
                        .iter()
                        .zip(heli.origin)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f32>();
                    let eye = [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ];
                    let sight = world.trace_world(
                        heli.origin,
                        eye,
                        [0.0; 3],
                        [0.0; 3],
                        gamemode_iw4::G_CAN_DAMAGE_CONTENTS_MASK,
                    );
                    if sight.fraction < 1.0 || sight.startsolid != 0 {
                        return None;
                    }
                    (dist2 <= killstreaks::PAVELOW_RANGE.powi(2)).then_some((
                        id,
                        dist2,
                        meta.life_sequence,
                    ))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((target, _, life)) = target {
                let weapon = world
                    .weapon_index_by_script_name(killstreaks::PAVELOW_MINIGUN)
                    .unwrap_or(0);
                let amount = world
                    .combat_facts_for(weapon)
                    .map_or(25, |f| f.damage.max(1));
                let attacker_life = world
                    .client_meta(heli.owner)
                    .map_or_default(|m| m.life_sequence);
                for _ in 0..2 {
                    let shot = world.alloc_shot_id();
                    let _ = crate::damage::apply_damage_attempt(
                        world,
                        tick,
                        &crate::DamageAttempt {
                            source: crate::DamageSource::Shot(shot),
                            pellet: crate::PelletId(0),
                            attacker: heli.owner,
                            attacker_life,
                            target,
                            target_life: life,
                            weapon,
                            amount,
                            killcam_entity_start_time: now,
                            inflictor_origin: Some(heli.origin),
                            hitloc: 0,
                        },
                    );
                }
            }
            heli.burst_remaining = heli.burst_remaining.saturating_sub(1);
            if heli.burst_remaining == 0 {
                heli.burst_remaining = killstreaks::PAVELOW_BURST[0]
                    + world.combat_rng_mut().next_index(
                        (killstreaks::PAVELOW_BURST[1] - killstreaks::PAVELOW_BURST[0] + 1)
                            as usize,
                    ) as u32;
                heli.next_shot_ms = now.saturating_add(
                    killstreaks::PAVELOW_BURST_PAUSE_MS[0] as i32
                        + world.combat_rng_mut().next_index(
                            (killstreaks::PAVELOW_BURST_PAUSE_MS[1]
                                - killstreaks::PAVELOW_BURST_PAUSE_MS[0]
                                + 1) as usize,
                        ) as i32,
                );
            } else {
                heli.next_shot_ms = now.saturating_add(killstreaks::PAVELOW_SHOT_MS as i32);
            }
        }
        world.pave_lows.push(heli);
    }
}

pub(crate) fn advance_uavs(world: &mut FrameWorld, tick: Tick) {
    if !world.publishes_snapshot() {
        return;
    }
    let now = crate::level_time_ms(tick);
    let mut uavs = std::mem::take(&mut world.uavs);
    for mut uav in uavs.drain(..) {
        if now >= uav.expires_at_ms {
            remove_model(world, UAV_MODEL_KIND, uav.id);
            continue;
        }
        let angle = (now - uav.started_at_ms) as f32 / killstreaks::UAV_ORBIT_PERIOD_MS as f32
            * core::f32::consts::TAU;
        uav.origin = [
            uav.center[0] + angle.cos() * 6_000.0,
            uav.center[1] + angle.sin() * 6_000.0,
            uav.center[2] + 4_000.0,
        ];
        if let Some(number) = world.gentity_number(model_id(UAV_MODEL_KIND, uav.id)) {
            world.set_script_mover_origin(number, uav.origin);
        }
        world.uavs.push(uav);
    }
}

pub(crate) fn trace_pave_low_shots(world: &mut FrameWorld, shots: &[crate::combat::Emission]) {
    if world.pave_lows.is_empty() || !world.publishes_snapshot() {
        return;
    }
    for shot in shots {
        for heli in world.pave_lows.clone() {
            if world.bootstrap_ref().kind.is_team()
                && world
                    .client_meta(shot.attacker)
                    .is_some_and(|m| m.client_state_team == heli.team)
            {
                continue;
            }
            let delta = [
                heli.origin[0] - shot.origin[0],
                heli.origin[1] - shot.origin[1],
                heli.origin[2] - shot.origin[2],
            ];
            let along = delta
                .iter()
                .zip(shot.direction)
                .map(|(a, b)| a * b)
                .sum::<f32>();
            if !(0.0..=shot.max_range).contains(&along) {
                continue;
            }
            let miss2 = delta
                .iter()
                .zip(shot.direction)
                .map(|(a, b)| (a - b * along).powi(2))
                .sum::<f32>();
            if miss2 > 200.0_f32.powi(2) {
                continue;
            }
            let trace = world.sensor_trace(crate::BulletTraceQuery {
                start: shot.origin,
                end: heli.origin,
                mask: crate::MASK_BULLET_WORLD,
                ignore: Some(shot.attacker),
                ignore_hit: None,
            });
            let clear = match trace {
                crate::TraceOutcome::Miss { .. } => true,
                crate::TraceOutcome::Hit { fraction, .. } => fraction >= 0.98,
                _ => false,
            };
            if clear {
                damage_pave_low(world, heli.id, shot.base_damage);
            }
        }
    }
}

pub(crate) fn blast_pave_lows(
    world: &mut FrameWorld,
    attacker: ClientId,
    origin: [f32; 3],
    radius: f32,
    damage: i32,
) {
    if !world.publishes_snapshot() {
        return;
    }
    let team = world
        .client_meta(attacker)
        .map_or(0, |m| m.client_state_team);
    for heli in world.pave_lows.clone() {
        if world.bootstrap_ref().kind.is_team() && heli.team == team {
            continue;
        }
        let dist2 = origin
            .iter()
            .zip(heli.origin)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>();
        if dist2 <= radius.powi(2) {
            damage_pave_low(world, heli.id, damage);
        }
    }
}

fn damage_pave_low(world: &mut FrameWorld, id: u32, damage: i32) {
    if let Some(heli) = world.pave_lows.iter_mut().find(|h| h.id == id) {
        heli.health = heli.health.saturating_sub(damage.max(0));
    }
}

fn launch_predator(world: &mut FrameWorld, tick: Tick, id: ClientId) -> bool {
    let Some(weapon) = world.weapon_index_by_script_name(killstreaks::PREDATOR_PROJECTILE) else {
        return false;
    };
    let Some(ps) = world.player(id).copied() else {
        return false;
    };
    let (forward, _, _) = math_iw4::angle_vectors([0.0, ps.viewangles[1], 0.0]);
    let start = [
        ps.origin[0] - forward[0] * killstreaks::PREDATOR_LAUNCH_BACK,
        ps.origin[1] - forward[1] * killstreaks::PREDATOR_LAUNCH_BACK,
        ps.origin[2] + killstreaks::PREDATOR_LAUNCH_HEIGHT,
    ];
    let target = [
        ps.origin[0] + forward[0] * killstreaks::PREDATOR_TARGET_AHEAD,
        ps.origin[1] + forward[1] * killstreaks::PREDATOR_TARGET_AHEAD,
        ps.origin[2],
    ];
    let delta = [
        target[0] - start[0],
        target[1] - start[1],
        target[2] - start[2],
    ];
    let length = math_iw4::vec3_length(delta);
    let dir = [delta[0] / length, delta[1] / length, delta[2] / length];
    let speed = world
        .missile_launch_facts(weapon)
        .map_or(killstreaks::PREDATOR_SPEED_RANGE[0], |f| {
            f.projectile_speed as f32
        });
    let Ok(slot) = world.allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile) else {
        return false;
    };
    let entnum = slot.number();
    let projectile = world.allocate_projectile_id();
    let now = crate::level_time_ms(tick);
    let velocity = entity_iw4::truncated_tr_delta([dir[0] * speed, dir[1] * speed, dir[2] * speed]);
    let owner_life = world.client_meta(id).map_or_default(|m| m.life_sequence);
    world.push_projectile(crate::equipment::ProjectileState {
        id: projectile,
        owner: id,
        owner_life,
        weapon,
        origin: start,
        velocity,
        pos: entity_iw4::Trajectory {
            tr_time: now,
            tr_type: entity_iw4::TR_LINEAR,
            tr_duration: 0,
            tr_delta: velocity,
            tr_base: start,
        },
        apos: entity_iw4::g_fire_missile_apos(dir),
        entnum,
        launch_time: now,
        spawn_time_ms: now,
        detonate_at_ms: None,
        cleanup_at_ms: now.saturating_add(crate::equipment::ROCKET_CLEANUP_MS),
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
    });
    world.client_meta_mut(id).remote_missile = Some(RemoteMissile {
        projectile,
        entnum,
        angles: math_iw4::vect_to_angles(dir),
        ..RemoteMissile::default()
    });
    true
}

pub(crate) fn steer_remote_missile(
    world: &mut FrameWorld,
    id: ClientId,
    cmd: &playerstate_iw4::UserCmd,
    msec: i32,
) {
    if !world.publishes_snapshot() {
        return;
    }
    let Some(mut link) = world
        .client_meta(id)
        .and_then(|m| m.remote_missile)
        .filter(|link| link.unlink_at_ms.is_none())
    else {
        return;
    };
    let seconds = msec as f32 * 0.001;
    let delta = math_iw4::angles_to_axis([
        f32::from(cmd.remote_control[0] as i8) / 127.0 * seconds * killstreaks::PREDATOR_PITCH_RATE,
        f32::from(cmd.remote_control[1] as i8) / 127.0 * seconds * killstreaks::PREDATOR_YAW_RATE,
        0.0,
    ]);
    let transposed = core::array::from_fn(|row| core::array::from_fn(|col| delta[col][row]));
    let mut angles = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
        transposed,
        math_iw4::angles_to_axis(link.angles),
    ));
    angles[0] = (angles[0] / 360.0 - (angles[0] / 360.0 + 0.5).floor()) * 360.0;
    angles[0] = angles[0].clamp(
        killstreaks::PREDATOR_PITCH_RANGE[0],
        killstreaks::PREDATOR_PITCH_RANGE[1],
    );
    link.angles = angles;
    let held = cmd.buttons & playerstate_iw4::buttons::REMOTE_CONTROL != 0;
    link.attack = held && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0;
    link.armed |= held && !link.attack;
    world.client_meta_mut(id).remote_missile = Some(link);
}

pub(crate) fn advance_remote_missiles(world: &mut FrameWorld, tick: Tick) {
    if !world.publishes_snapshot() {
        return;
    }
    let now = crate::level_time_ms(tick);
    for id in world.client_ids_sorted() {
        let Some(mut link) = world.client_meta(id).and_then(|m| m.remote_missile) else {
            continue;
        };
        let alive = world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive);
        if !alive || link.unlink_at_ms.is_some_and(|at| now >= at) {
            world.client_meta_mut(id).remote_missile = None;
            continue;
        }
        if link.unlink_at_ms.is_some() {
            continue;
        }
        let Some(projectile) = world
            .projectile_mut_by_number(link.entnum)
            .filter(|p| p.id == link.projectile && p.live)
        else {
            link.unlink_at_ms = Some(now.saturating_add(killstreaks::PREDATOR_STATIC_MS));
            world.client_meta_mut(id).remote_missile = Some(link);
            continue;
        };
        let (dir, _, _) = math_iw4::angle_vectors(link.angles);
        let seconds = crate::MATCH_TICK_MS as f32 * 0.001;
        let mut speed = projectile
            .velocity
            .iter()
            .zip(dir)
            .map(|(v, d)| v * d)
            .sum::<f32>();
        if link.armed && !link.boosted && link.attack {
            speed = killstreaks::PREDATOR_SPEED_RANGE[1];
            link.boosted = true;
        } else {
            let target = killstreaks::PREDATOR_SPEED_RANGE[0];
            speed = if speed < target {
                (speed + killstreaks::PREDATOR_SPEED_UP * seconds).min(target)
            } else {
                (speed - killstreaks::PREDATOR_SPEED_DOWN * seconds).max(target)
            };
        }
        let velocity =
            entity_iw4::truncated_tr_delta([dir[0] * speed, dir[1] * speed, dir[2] * speed]);
        projectile.velocity = velocity;
        projectile.pos = entity_iw4::Trajectory {
            tr_time: now.saturating_sub(crate::MATCH_TICK_MS as i32),
            tr_type: entity_iw4::TR_LINEAR,
            tr_duration: 0,
            tr_delta: velocity,
            tr_base: projectile.origin,
        };
        projectile.apos = entity_iw4::g_fire_missile_apos(dir);
        world.client_meta_mut(id).remote_missile = Some(link);
    }
}
