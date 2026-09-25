use crate::frame::FrameWorld;
use crate::match_state::{
    CareFlybyPhase, CarePackage, ClientLifecycle, EventAudience, PaveLow, RemoteMissile, SimEvent,
    Uav,
};
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
    drop_spent_weapon(world, id);
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
    if !world.player(id).is_some_and(|ps| ps.weapon == weapon)
        || world
            .client_meta(id)
            .is_some_and(|m| m.spent_streak_weapon == weapon)
    {
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
    consume_top(world, tick, id, weapon);
}

pub(crate) fn marker_fired(world: &mut FrameWorld, tick: Tick, id: ClientId, weapon: u32) {
    if world.weapon_script_name(weapon) != killstreaks::AIRDROP_MARKER_WEAPON {
        return;
    }
    if world.client_meta(id).and_then(|m| m.owned_streaks.first()) != Some(&Killstreak::Airdrop) {
        return;
    }
    consume_top(world, tick, id, weapon);
}

fn consume_top(world: &mut FrameWorld, tick: Tick, id: ClientId, weapon: u32) {
    world.client_meta_mut(id).owned_streaks.remove(0);
    world.client_meta_mut(id).spent_streak_weapon = weapon;
    let remembered = world.client_meta(id).map_or(0, |m| m.last_combat_weapon);
    let restore = world.player(id).map_or(0, |ps| {
        if remembered != 0 && ps.weapons.contains(&(remembered as i32)) {
            remembered
        } else {
            ps.weapons
                .iter()
                .filter_map(|&w| (w > 0).then_some(w as u32))
                .find(|&w| !is_streak_weapon(world, w))
                .unwrap_or(0)
        }
    });
    sync_inventory(world, id);
    if restore != 0 {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::WeaponSwitchRequested { weapon: restore },
        );
    }
}

fn is_streak_weapon(world: &FrameWorld, weapon: u32) -> bool {
    let name = world.weapon_script_name(weapon);
    name.starts_with("killstreak_") || name == killstreaks::AIRDROP_MARKER_WEAPON
}

fn drop_spent_weapon(world: &mut FrameWorld, id: ClientId) {
    let Some(spent) = world
        .client_meta(id)
        .map(|m| m.spent_streak_weapon)
        .filter(|&w| w != 0)
    else {
        return;
    };
    if world.player(id).is_none_or(|ps| ps.weapon == spent) {
        return;
    }
    world.client_meta_mut(id).spent_streak_weapon = 0;
    let still_owned = world.client_meta(id).is_some_and(|m| {
        m.owned_streaks
            .iter()
            .any(|streak| world.weapon_index_by_script_name(streak.weapon()) == Some(spent))
    });
    if still_owned {
        return;
    }
    if let Some(ps) = world.player_mut(id) {
        for slot in &mut ps.weapons {
            if *slot == spent as i32 {
                *slot = 0;
            }
        }
    }
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
    let team = world.client_meta(owner).map_or(0, |m| m.client_state_team);
    let contents = killstreaks::crate_contents(world.combat_rng_mut().next_u32());
    let drop_yaw = (world.combat_rng_mut().next_u32() % 36_000) as f32 / 100.0;
    let height = world
        .bootstrap_ref()
        .airstrike_height
        .unwrap_or(origin[2] + killstreaks::FLY_HEIGHT_OVER_SITE);
    let path = flyby_path(world, origin, drop_yaw, height);
    let approach_yaw = heading(path[0], path[1]);
    let angles = [0.0, approach_yaw, 0.0];
    let package = CarePackage {
        id,
        owner,
        team,
        origin,
        path,
        approach_yaw,
        flyby: CareFlybyPhase::Approach,
        phase_started_ms: now,
        bird_speed: 0.0,
        bird_velocity: [0.0; 2],
        bird_tilt: [0.0; 4],
        bird_health: killstreaks::LITTLE_BIRD_HEALTH,
        bird_spin: 0.0,
        bird_crash_ms: i32::MAX,
        crate_slung: true,
        crate_fall_speed: 0.0,
        contents,
        ready_at_ms: i32::MAX,
        expires_at_ms: i32::MAX,
        capturer: None,
        capture_ms: 0,
    };
    let _ = world.spawn_script_mover(model_id(LITTLE_BIRD_MODEL_KIND, id), path[0], angles);
    let _ = world.spawn_script_mover(
        model_id(CRATE_MODEL_KIND, id),
        slung_crate_origin(path[0], angles),
        angles,
    );
    world.care_packages.push(package);
}

// The crate is gone but its bird is still leaving.
const CRATE_CLAIMED: i32 = i32::MAX - 1;

// Player clip caps the playable volume below the flyby height; a crate stopped by it hangs midair.
const CRATE_FALL_MASK: u32 = crate::bullet_collision::MASK_PLAYER_SOLID & !0x0001_0000;

fn yaw_forward(yaw: f32) -> [f32; 2] {
    let (sin, cos) = yaw.to_radians().sin_cos();
    [cos, sin]
}

fn heading(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

fn flat_dir(from: [f32; 3], to: [f32; 3]) -> [f32; 2] {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = d[0].hypot(d[1]);
    if len < 1.0 {
        [0.0; 2]
    } else {
        [d[0] / len, d[1] / len]
    }
}

fn jitter(world: &mut FrameWorld, amount: f32) -> f32 {
    ((world.combat_rng_mut().next_u32() % 20_001) as f32 / 10_000.0 - 1.0) * amount
}

fn flyby_path(world: &mut FrameWorld, site: [f32; 3], drop_yaw: f32, height: f32) -> [[f32; 3]; 3] {
    let forward = yaw_forward(drop_yaw);
    let side = yaw_forward(drop_yaw + 90.0);
    let far = killstreaks::FLYBY_DISTANCE;
    let short = killstreaks::FLYBY_GOAL_SHORT_OF_SITE;
    let start = [
        site[0] - forward[0] * far + jitter(world, killstreaks::FLYBY_START_JITTER),
        site[1] - forward[1] * far + jitter(world, killstreaks::FLYBY_START_JITTER),
        height,
    ];
    let end = [
        site[0] + side[0] * far + jitter(world, killstreaks::FLYBY_END_JITTER),
        site[1] + side[1] * far + jitter(world, killstreaks::FLYBY_END_JITTER),
        height,
    ];
    let goal = [
        site[0] - forward[0] * short,
        site[1] - forward[1] * short,
        height,
    ];
    [start, goal, end]
}

fn bird_point(bird: [f32; 3], angles: [f32; 3], local: [f32; 3]) -> [f32; 3] {
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    std::array::from_fn(|i| {
        bird[i] + forward[i] * local[0] - right[i] * local[1] + up[i] * local[2]
    })
}

fn slung_crate_origin(bird: [f32; 3], angles: [f32; 3]) -> [f32; 3] {
    let tag = killstreaks::LITTLE_BIRD_TAG_GROUND;
    let link = killstreaks::CRATE_TAG_GROUND_OFFSET;
    bird_point(bird, angles, std::array::from_fn(|i| tag[i] + link[i]))
}

// Vehicle_SetSpeed toward a setVehGoalPos(.., 1) goal: brake at `decel` so the
// bird reaches the goal at rest.
fn vehicle_speed(speed: f32, max: f32, accel: f32, decel: f32, remaining: f32, dt: f32) -> f32 {
    let target = max.min((2.0 * decel * remaining.max(0.0)).sqrt());
    if speed < target {
        (speed + accel * dt).min(target)
    } else {
        (speed - decel * dt).max(target)
    }
}

fn yaw_turn(turn: f32, since_ms: i32) -> f32 {
    let accel = killstreaks::FLYBY_YAW_ACCEL_DEG;
    let size = turn.abs();
    let half = (size / accel).sqrt();
    let t = since_ms.max(0) as f32 / 1000.0;
    let done = if t >= 2.0 * half {
        size
    } else if t < half {
        0.5 * accel * t * t
    } else {
        size - 0.5 * accel * (2.0 * half - t).powi(2)
    };
    done.copysign(turn)
}

fn approach_value(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

fn step_tilt(angle: &mut f32, vel: &mut f32, target: f32, accel: f32, decel: f32, dt: f32) {
    let delta = math_iw4::angle_subtract(target, *angle);
    if delta * delta < 1.0e-4 && *vel * *vel < 0.0025 {
        *angle = target;
        *vel = 0.0;
        return;
    }
    let speed = vel.abs();
    let (mut goal_vel, mut rate) = (killstreaks::VEHICLE_MAX_TILT_VEL, accel);
    if *vel * delta >= 0.0 && delta.abs() <= speed / decel * speed * 0.5 {
        goal_vel = 0.0;
        rate = decel;
    }
    if delta < 0.0 {
        goal_vel = -goal_vel;
    }
    if rate * dt <= speed || speed * dt <= delta.abs() {
        *vel = approach_value(*vel, goal_vel, rate * dt);
        *angle = math_iw4::angle_subtract(*angle + *vel * dt, 0.0);
    } else {
        *angle = target;
        *vel = 0.0;
    }
}

fn update_tilt(
    package: &mut CarePackage,
    velocity: [f32; 2],
    yaw: f32,
    manual_accel: f32,
    remaining: f32,
    dt: f32,
) {
    let mut accel = [
        (velocity[0] - package.bird_velocity[0]) / dt,
        (velocity[1] - package.bird_velocity[1]) / dt,
    ];
    package.bird_velocity = velocity;
    let speed = velocity[0].hypot(velocity[1]);
    let drag_speed = killstreaks::VEHICLE_FAKE_DRAG_MPH * killstreaks::MPH_TO_UNITS;
    if speed > 0.0 {
        let drag =
            (speed.min(drag_speed) / drag_speed).powi(2) * killstreaks::VEHICLE_FAKE_DRAG_ACCEL;
        accel[0] += drag * velocity[0] / speed;
        accel[1] += drag * velocity[1] / speed;
    }
    let horizontal = accel[0].hypot(accel[1]);
    let def_accel = killstreaks::LITTLE_BIRD_DEF_ACCEL;
    let (mut pitch, mut roll) = (0.0, 0.0);
    let levelling = remaining < 15.0 && speed < 10.0 * killstreaks::MPH_TO_UNITS;
    if !levelling && horizontal > 0.0 {
        let frac = (horizontal / def_accel).min(1.0);
        let stop_time = speed / horizontal;
        let stopping = frac * 2.5 + (1.0 - frac) * 3.5;
        let settle = if stop_time < stopping {
            stop_time / stopping
        } else {
            1.0
        };
        let scale = (frac + (1.0 - frac) * 0.1) * settle;
        let n = [accel[0] / horizontal, accel[1] / horizontal];
        let (sin, cos) = yaw.to_radians().sin_cos();
        pitch = killstreaks::LITTLE_BIRD_MAX_PITCH * scale * (n[0] * cos + n[1] * sin);
        roll = killstreaks::LITTLE_BIRD_MAX_ROLL * scale * (n[0] * sin - n[1] * cos);
    }
    let f = (manual_accel / def_accel).clamp(0.0, 1.0);
    let rate = f * 45.0 + (1.0 - f);
    let [p, r, pv, rv] = &mut package.bird_tilt;
    step_tilt(p, pv, pitch, rate, rate * 0.4, dt);
    step_tilt(r, rv, roll, rate, rate * 0.4, dt);
}

fn advance_flyby(world: &mut FrameWorld, tick: Tick, package: &mut CarePackage, now: i32, dt: f32) {
    let Some(number) = world.gentity_number(model_id(LITTLE_BIRD_MODEL_KIND, package.id)) else {
        package.flyby = CareFlybyPhase::Gone;
        return;
    };
    let Some((bird, angles)) = world
        .script_mover_by_number(number)
        .map(|mover| (mover.state.tr_base, mover.state.apos_tr_base))
    else {
        return;
    };
    let mph = killstreaks::MPH_TO_UNITS;
    let [start, goal, end] = package.path;
    let mut yaw = angles[1];
    let (target, dir, max, accel) = match package.flyby {
        CareFlybyPhase::Approach => {
            yaw = package.approach_yaw;
            let (max, accel) =
                if now - package.phase_started_ms < killstreaks::FLYBY_SLOW_AFTER_MS as i32 {
                    (
                        killstreaks::FLYBY_APPROACH_MPH,
                        killstreaks::FLYBY_APPROACH_ACCEL_MPH,
                    )
                } else {
                    (
                        killstreaks::FLYBY_SLOW_MPH,
                        killstreaks::FLYBY_SLOW_ACCEL_MPH,
                    )
                };
            (goal, flat_dir(start, goal), max, accel)
        }
        CareFlybyPhase::Hover => {
            if now - package.phase_started_ms >= killstreaks::FLYBY_DROP_AFTER_GOAL_MS as i32 {
                package.flyby = CareFlybyPhase::Leave;
                package.phase_started_ms = now;
                package.crate_slung = false;
            }
            (goal, [0.0; 2], 0.0, killstreaks::FLYBY_SLOW_ACCEL_MPH)
        }
        CareFlybyPhase::Leave => {
            let turn = math_iw4::angle_subtract(heading(goal, end), package.approach_yaw);
            yaw = package.approach_yaw + yaw_turn(turn, now - package.phase_started_ms);
            (
                end,
                flat_dir(goal, end),
                killstreaks::FLYBY_LEAVE_MPH,
                killstreaks::FLYBY_LEAVE_ACCEL_MPH,
            )
        }
        CareFlybyPhase::Dying => {
            if now >= package.bird_crash_ms {
                crash_bird(world, tick, package, bird);
                return;
            }
            let spun = ((now - package.phase_started_ms) as f32 / 1000.0).min(1.0);
            yaw += package.bird_spin * spun * dt;
            let target = if package.crate_slung { goal } else { end };
            (
                target,
                flat_dir(bird, target),
                killstreaks::LITTLE_BIRD_DYING_MPH,
                killstreaks::LITTLE_BIRD_DYING_ACCEL_MPH,
            )
        }
        CareFlybyPhase::Gone => return,
    };
    let remaining = (target[0] - bird[0]) * dir[0] + (target[1] - bird[1]) * dir[1];
    package.bird_speed = vehicle_speed(
        package.bird_speed,
        max * mph,
        accel * mph,
        accel * mph,
        remaining,
        dt,
    );
    let step = (package.bird_speed * dt).min(remaining.max(0.0));
    let arrived = remaining - step <= 1.0;
    if arrived && package.flyby == CareFlybyPhase::Leave {
        remove_model(world, LITTLE_BIRD_MODEL_KIND, package.id);
        package.flyby = CareFlybyPhase::Gone;
        return;
    }
    let next = if arrived && package.flyby == CareFlybyPhase::Approach {
        package.flyby = CareFlybyPhase::Hover;
        package.phase_started_ms = now;
        package.bird_speed = 0.0;
        goal
    } else {
        [bird[0] + dir[0] * step, bird[1] + dir[1] * step, goal[2]]
    };
    let velocity = [(next[0] - bird[0]) / dt, (next[1] - bird[1]) / dt];
    update_tilt(package, velocity, yaw, accel * mph, remaining - step, dt);
    world.set_script_mover_pose(
        number,
        now,
        next,
        [package.bird_tilt[0], yaw, package.bird_tilt[1]],
    );
}

fn crash_bird(world: &mut FrameWorld, tick: Tick, package: &mut CarePackage, bird: [f32; 3]) {
    package.crate_slung = false;
    package.flyby = CareFlybyPhase::Gone;
    remove_model(world, LITTLE_BIRD_MODEL_KIND, package.id);
    push_world_event(
        world,
        tick,
        entity_iw4::EntityEventKind::PLAY_FX,
        killstreaks::LITTLE_BIRD_DEATH_FX,
        bird,
    );
    push_world_event(
        world,
        tick,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        killstreaks::LITTLE_BIRD_CRASH_SOUND,
        bird,
    );
}

fn push_world_event(
    world: &mut FrameWorld,
    tick: Tick,
    kind: entity_iw4::EntityEventKind,
    name: &str,
    origin: [f32; 3],
) {
    let event_parm = if kind == entity_iw4::EntityEventKind::PLAY_FX {
        world.effect_name_index(name)
    } else {
        world.sound_alias_index(name)
    };
    world.push_entity_event(
        tick,
        EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number: i32::from(trace_iw4::ENTITYNUM_WORLD),
            event_parm: i32::from(event_parm),
            origin,
            direction: [0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
}

fn bird_alive(package: &CarePackage) -> bool {
    matches!(
        package.flyby,
        CareFlybyPhase::Approach | CareFlybyPhase::Hover | CareFlybyPhase::Leave
    )
}

fn bird_hit_center(world: &FrameWorld, package: &CarePackage) -> Option<[f32; 3]> {
    let number = world.gentity_number(model_id(LITTLE_BIRD_MODEL_KIND, package.id))?;
    let mover = world.script_mover_by_number(number)?;
    Some(bird_point(
        mover.state.tr_base,
        mover.state.apos_tr_base,
        killstreaks::LITTLE_BIRD_HIT_CENTER,
    ))
}

// The owner may shoot their own bird; teammates may not.
fn bird_spares(world: &FrameWorld, package: &CarePackage, attacker: ClientId) -> bool {
    attacker != package.owner
        && world.bootstrap_ref().kind.is_team()
        && world
            .client_meta(attacker)
            .is_some_and(|m| m.client_state_team == package.team)
}

fn damage_care_bird(world: &mut FrameWorld, tick: Tick, id: u32, attacker: ClientId, damage: i32) {
    let now = crate::level_time_ms(tick);
    let Some(index) = world
        .care_packages
        .iter()
        .position(|p| p.id == id && bird_alive(p))
    else {
        return;
    };
    world.record_damage_feedback(
        attacker,
        attacker,
        gamemode_iw4::TypeHit::Standard,
        damage,
        now,
    );
    let package = &mut world.care_packages[index];
    package.bird_health = package.bird_health.saturating_sub(damage.max(0));
    if package.bird_health > 0 {
        return;
    }
    package.flyby = CareFlybyPhase::Dying;
    package.phase_started_ms = now;
    let [spin_lo, spin_hi] = killstreaks::LITTLE_BIRD_SPIN_DEG;
    let [crash_lo, crash_hi] = killstreaks::LITTLE_BIRD_CRASH_DELAY_MS;
    let spin = spin_lo + world.combat_rng_mut().next_u32() % (spin_hi - spin_lo);
    let crash = crash_lo + world.combat_rng_mut().next_u32() % (crash_hi - crash_lo);
    let package = &mut world.care_packages[index];
    package.bird_spin = spin as f32;
    package.bird_crash_ms = now.saturating_add(crash as i32);
    let tail = world
        .gentity_number(model_id(LITTLE_BIRD_MODEL_KIND, id))
        .and_then(|number| world.script_mover_by_number(number))
        .map(|mover| {
            bird_point(
                mover.state.tr_base,
                mover.state.apos_tr_base,
                killstreaks::LITTLE_BIRD_TAIL_ROTOR,
            )
        });
    if let Some(tail) = tail {
        push_world_event(
            world,
            tick,
            entity_iw4::EntityEventKind::PLAY_FX,
            killstreaks::LITTLE_BIRD_TAIL_FX,
            tail,
        );
    }
}

// Returns false once the crate fell out of the world and was deleted.
fn advance_crate_fall(
    world: &mut FrameWorld,
    package: &mut CarePackage,
    now: i32,
    dt: f32,
) -> bool {
    let Some(number) = world.gentity_number(model_id(CRATE_MODEL_KIND, package.id)) else {
        return false;
    };
    if package.crate_slung {
        if let Some(bird) = world
            .gentity_number(model_id(LITTLE_BIRD_MODEL_KIND, package.id))
            .and_then(|bird| world.script_mover_by_number(bird))
        {
            let angles = bird.state.apos_tr_base;
            world.set_script_mover_pose(
                number,
                now,
                slung_crate_origin(bird.state.tr_base, angles),
                angles,
            );
        }
        return true;
    }
    let Some(from) = world
        .script_mover_by_number(number)
        .map(|mover| mover.state.tr_base)
    else {
        return false;
    };
    package.crate_fall_speed += killstreaks::CRATE_GRAVITY * dt;
    let to = [from[0], from[1], from[2] - package.crate_fall_speed * dt];
    let trace = world.trace_world(
        from,
        to,
        [-8.0, -8.0, 0.0],
        [8.0, 8.0, 8.0],
        CRATE_FALL_MASK,
    );
    if trace.fraction < 1.0 || trace.startsolid != 0 {
        let rest = if trace.startsolid != 0 {
            from
        } else {
            trace.endpos
        };
        let yaw = world
            .script_mover_by_number(number)
            .map_or(0.0, |mover| mover.state.apos_tr_base[1]);
        world.set_script_mover_origin(number, rest);
        world.set_script_mover_angles(number, [0.0, yaw, 0.0]);
        package.origin = rest;
        package.crate_fall_speed = 0.0;
        package.ready_at_ms = now;
        package.expires_at_ms = now.saturating_add(killstreaks::CRATE_TIMEOUT_MS as i32);
        return true;
    }
    if to[2] < package.origin[2] - killstreaks::CRATE_LOST_BELOW_SITE {
        remove_model(world, CRATE_MODEL_KIND, package.id);
        return false;
    }
    let angles = world
        .script_mover_by_number(number)
        .map_or([0.0; 3], |mover| mover.state.apos_tr_base);
    world.set_script_mover_pose(number, now, to, angles);
    true
}

pub(crate) fn advance_crates(
    world: &mut FrameWorld,
    tick: Tick,
    msec: i32,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    let now = crate::level_time_ms(tick);
    let dt = msec as f32 / 1000.0;
    let mut packages = std::mem::take(&mut world.care_packages);
    for mut package in packages.drain(..) {
        if now >= package.expires_at_ms {
            remove_model(world, CRATE_MODEL_KIND, package.id);
            remove_model(world, LITTLE_BIRD_MODEL_KIND, package.id);
            continue;
        }
        advance_flyby(world, tick, &mut package, now, dt);
        if package.ready_at_ms == CRATE_CLAIMED {
            if package.flyby != CareFlybyPhase::Gone {
                world.care_packages.push(package);
            }
            continue;
        }
        if package.ready_at_ms == i32::MAX && !advance_crate_fall(world, &mut package, now, dt) {
            if package.flyby != CareFlybyPhase::Gone {
                world.care_packages.push(package);
            }
            continue;
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
                if package.flyby != CareFlybyPhase::Gone {
                    package.ready_at_ms = CRATE_CLAIMED;
                    package.capturer = None;
                    world.care_packages.push(package);
                }
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
            world.set_script_mover_pose(
                number,
                now,
                heli.origin,
                [0.0, angle.to_degrees() + 90.0, 0.0],
            );
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
            let angles = world
                .script_mover_by_number(number)
                .map_or([0.0; 3], |mover| mover.state.apos_tr_base);
            world.set_script_mover_pose(number, now, uav.origin, angles);
        }
        world.uavs.push(uav);
    }
}

fn shot_reaches(
    world: &FrameWorld,
    shot: &crate::combat::Emission,
    center: [f32; 3],
    radius: f32,
) -> bool {
    let delta: [f32; 3] = std::array::from_fn(|i| center[i] - shot.origin[i]);
    let along = delta
        .iter()
        .zip(shot.direction)
        .map(|(a, b)| a * b)
        .sum::<f32>();
    if !(0.0..=shot.max_range).contains(&along) {
        return false;
    }
    let miss2 = delta
        .iter()
        .zip(shot.direction)
        .map(|(a, b)| (a - b * along).powi(2))
        .sum::<f32>();
    if miss2 > radius.powi(2) {
        return false;
    }
    let trace = world.sensor_trace(crate::BulletTraceQuery {
        start: shot.origin,
        end: center,
        mask: crate::MASK_BULLET_WORLD,
        ignore: Some(shot.attacker),
        ignore_hit: None,
    });
    match trace {
        crate::TraceOutcome::Miss { .. } => true,
        crate::TraceOutcome::Hit { fraction, .. } => fraction >= 0.98,
        _ => false,
    }
}

pub(crate) fn trace_aircraft_shots(
    world: &mut FrameWorld,
    tick: Tick,
    shots: &[crate::combat::Emission],
) {
    if !world.publishes_snapshot() {
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
            if shot_reaches(world, shot, heli.origin, 200.0) {
                damage_pave_low(world, heli.id, shot.base_damage);
            }
        }
        for package in world.care_packages.clone() {
            if !bird_alive(&package) || bird_spares(world, &package, shot.attacker) {
                continue;
            }
            let Some(center) = bird_hit_center(world, &package) else {
                continue;
            };
            if shot_reaches(world, shot, center, killstreaks::LITTLE_BIRD_HIT_RADIUS) {
                damage_care_bird(world, tick, package.id, shot.attacker, shot.base_damage);
            }
        }
    }
}

pub(crate) fn blast_aircraft(
    world: &mut FrameWorld,
    tick: Tick,
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
    let distance = |to: [f32; 3]| {
        origin
            .iter()
            .zip(to)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt()
    };
    for heli in world.pave_lows.clone() {
        if world.bootstrap_ref().kind.is_team() && heli.team == team {
            continue;
        }
        if distance(heli.origin) <= radius {
            damage_pave_low(world, heli.id, damage);
        }
    }
    for package in world.care_packages.clone() {
        if !bird_alive(&package) || bird_spares(world, &package, attacker) {
            continue;
        }
        let Some(center) = bird_hit_center(world, &package) else {
            continue;
        };
        if distance(center) - killstreaks::LITTLE_BIRD_HIT_RADIUS <= radius {
            damage_care_bird(world, tick, package.id, attacker, damage);
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
        grounded: false,
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
