use crate::frame::FrameWorld;
use crate::match_state::{CareFlybyPhase, CarePackage, ClientLifecycle, EventAudience, SimEvent};
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

pub(crate) fn on_death(world: &mut FrameWorld, victim: ClientId) {
    let meta = world.client_meta_mut(victim);
    meta.kill_streak = 0;
    meta.last_earned_streak = None;
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

// Player clip caps the playable volume below the flyby height; a crate stopped by it hangs midair.

fn yaw_forward(yaw: f32) -> [f32; 2] {
    let (sin, cos) = yaw.to_radians().sin_cos();
    [cos, sin]
}

fn heading(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
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
