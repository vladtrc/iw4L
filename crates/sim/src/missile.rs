use crate::combat::{AcceptedShot, spread_direction_on_plane};
use crate::equipment::{
    GrenadeLaunchKind, LaunchMotion, ProjectileLaunch, ProjectileState, spawn_grenade_projectile,
};
use crate::frame::FrameWorld;
use crate::identities::MatchRng;
use entity_iw4::{
    TR_LINEAR, Trajectory, fire_grenade_no_draw_ms, fire_missile_apos, truncated_tr_delta,
};
use math_iw4::vec3_length;
use weapon_iw4::{FireWeaponKind, ROCKET_SPREAD_PLANE, fire_weapon_kind};

pub(crate) fn fire_accepted_shot(world: &mut FrameWorld, tick: crate::Tick, shot: &AcceptedShot) {
    let Some(combat) = world.combat_facts_for(shot.weapon) else {
        return;
    };
    match fire_weapon_kind(combat.weap_type, combat.weap_class) {
        Some(FireWeaponKind::Bullet | FireWeaponKind::Flame) => {}
        Some(FireWeaponKind::GrenadeLauncher) => {
            spawn_grenade_projectile(
                world,
                shot.attacker,
                shot.weapon,
                crate::equipment::ProjectileLaunch {
                    tick,
                    origin: shot.origin,
                    angles: shot.angles,
                    owner_velocity: shot.owner_velocity,
                    combat_seed: shot.combat_seed,
                },
                GrenadeLaunchKind::Launcher,
                shot.fire_cause,
            );
        }
        Some(FireWeaponKind::Missile) => {
            fire_missile(world, tick, shot);
        }
        Some(FireWeaponKind::ThrownGrenade) => {
            spawn_grenade_projectile(
                world,
                shot.attacker,
                shot.weapon,
                crate::equipment::ProjectileLaunch {
                    tick,
                    origin: shot.origin,
                    angles: shot.angles,
                    owner_velocity: shot.owner_velocity,
                    combat_seed: shot.combat_seed,
                },
                GrenadeLaunchKind::Thrown {
                    remaining_fuse_ms: None,
                },
                shot.fire_cause,
            );
        }
        None => {}
    }
}

impl ProjectileLaunch {
    pub fn missile_motion(
        self,
        facts: &crate::EquipmentRuntimeFacts,
        spread_degrees: f32,
    ) -> LaunchMotion {
        let Self {
            tick,
            origin,
            angles,
            owner_velocity,
            combat_seed,
        } = self;
        let mut rng = MatchRng::new(combat_seed as u64);
        let mut dir =
            spread_direction_on_plane(angles, spread_degrees, &mut rng, ROCKET_SPREAD_PLANE);
        let mut origin = origin;
        if facts.missile_guidance == 3 {
            dir = glam::Vec3::new(dir[0], dir[1], dir[2] + 0.3)
                .normalize()
                .to_array();
            let (_, right, _) = math_iw4::angle_vectors(angles);
            origin = core::array::from_fn(|i| origin[i] + right[i] * 2.5);
        }
        let speed = facts.projectile_speed as f32;
        let raw_velocity: [f32; 3] = core::array::from_fn(|i| dir[i] * speed + owner_velocity[i]);
        let time_ms = crate::level_time_ms(tick);
        LaunchMotion {
            origin,
            pos: Trajectory {
                tr_time: time_ms,
                tr_type: if facts.missile_guidance == 3 {
                    entity_iw4::TR_GRAVITY
                } else {
                    TR_LINEAR
                },
                tr_duration: 0,
                tr_delta: truncated_tr_delta(raw_velocity),
                tr_base: origin,
            },
            apos: fire_missile_apos(dir),
            launch_time: time_ms + fire_grenade_no_draw_ms(vec3_length(raw_velocity)),
        }
    }
}

/// Launch trajectory of a weapon shot, shared by authority and client prediction.
pub fn shot_launch_motion(
    content: &crate::SimContent,
    tick: crate::Tick,
    fire: &weapon_iw4::FiredWeapon,
    combat_seed: u32,
) -> Option<LaunchMotion> {
    let combat = content.weapon_combat_facts(fire.weapon)?;
    match fire_weapon_kind(combat.weap_type, combat.weap_class)? {
        FireWeaponKind::Bullet | FireWeaponKind::Flame => None,
        FireWeaponKind::Missile => content.missile_launch_facts(fire.weapon).map(|facts| {
            crate::equipment::ProjectileLaunch {
                tick,
                origin: fire.origin,
                angles: fire.angles,
                owner_velocity: fire.velocity,
                combat_seed,
            }
            .missile_motion(&facts, fire.spread_degrees)
        }),
        kind @ (FireWeaponKind::GrenadeLauncher | FireWeaponKind::ThrownGrenade) => {
            let launch = if kind == FireWeaponKind::GrenadeLauncher {
                GrenadeLaunchKind::Launcher
            } else {
                GrenadeLaunchKind::Thrown {
                    remaining_fuse_ms: None,
                }
            };
            content.equipment_facts_for(fire.weapon).map(|facts| {
                crate::equipment::ProjectileLaunch {
                    tick,
                    origin: fire.origin,
                    angles: fire.angles,
                    owner_velocity: fire.velocity,
                    combat_seed,
                }
                .grenade_motion(&facts, launch)
            })
        }
    }
}

pub(crate) fn magic_bullet(
    world: &mut FrameWorld,
    tick: crate::Tick,
    owner: crate::ClientId,
    weapon: u32,
    start: [f32; 3],
    end: [f32; 3],
) -> Result<ProjectileState, String> {
    let combat = world
        .combat_facts_for(weapon)
        .ok_or_else(|| format!("weapon {weapon} has no combat facts"))?;
    let attacker_life = world
        .client_meta(owner)
        .ok_or("MagicBullet owner is not connected")?
        .life_sequence;
    let shot = AcceptedShot {
        shot_id: crate::ShotId(0),
        fire_cause: None,
        attacker: owner,
        attacker_life,
        hand: 0,
        weapon,
        ammo_used: 0,
        origin: start,
        angles: math_iw4::vect_to_angles(std::array::from_fn(|i| end[i] - start[i])),
        ads_frac: 1.0,
        view_height_current: 0.0,
        aim_spread_scale: 0.0,
        perks0: 0,
        combat_seed: world.combat_rng_mut().next_u32(),
        owner_velocity: [0.0; 3],
        spread_degrees: 0.0,
    };
    let launched = match fire_weapon_kind(combat.weap_type, combat.weap_class) {
        Some(FireWeaponKind::Missile) => fire_missile(world, tick, &shot),
        Some(kind @ (FireWeaponKind::GrenadeLauncher | FireWeaponKind::ThrownGrenade)) => {
            let launch = if kind == FireWeaponKind::GrenadeLauncher {
                GrenadeLaunchKind::Launcher
            } else {
                GrenadeLaunchKind::Thrown {
                    remaining_fuse_ms: None,
                }
            };
            spawn_grenade_projectile(
                world,
                owner,
                weapon,
                crate::equipment::ProjectileLaunch {
                    tick,
                    origin: start,
                    angles: shot.angles,
                    owner_velocity: [0.0; 3],
                    combat_seed: shot.combat_seed,
                },
                launch,
                None,
            )
            .then(|| {
                let now = crate::level_time_ms(tick);
                crate::frame::collect_projectiles(world.ecs())
                    .into_iter()
                    .filter(|p| p.owner == owner && p.weapon == weapon && p.spawn_time_ms == now)
                    .max_by_key(|p| p.id.0)
            })
            .flatten()
        }
        Some(FireWeaponKind::Bullet | FireWeaponKind::Flame) | None => {
            return Err("MagicBullet bullets are not simulated".into());
        }
    };
    launched.ok_or_else(|| format!("weapon {weapon} launched no projectile"))
}

fn fire_missile(
    world: &mut FrameWorld,
    tick: crate::Tick,
    shot: &AcceptedShot,
) -> Option<ProjectileState> {
    let Some(facts) = world.missile_launch_facts(shot.weapon) else {
        diag::warn!(
            Sim,
            "weapon {} fires no missile: no projectile speed",
            shot.weapon
        );
        return None;
    };
    let lock = world
        .client_meta(shot.attacker)
        .map(|m| m.weapon_lock)
        .filter(|lock| lock.can_fire(shot.weapon, shot.attacker_life.0));
    let guide = crate::MissileGuide {
        target: lock.and_then(|lock| lock.aim),
        top: lock.is_some_and(|lock| lock.flags & 4 != 0),
        ..Default::default()
    };
    let id = world.allocate_projectile_id();
    let entnum = match world.allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile) {
        Ok(entity) => entity.number(),
        Err(error) => {
            diag::warn!(Sim, "missile not spawned: {error:?}");
            return None;
        }
    };
    let time_ms = crate::level_time_ms(tick);
    let LaunchMotion {
        origin,
        pos,
        apos,
        launch_time,
    } = crate::equipment::ProjectileLaunch {
        tick,
        origin: shot.origin,
        angles: shot.angles,
        owner_velocity: shot.owner_velocity,
        combat_seed: shot.combat_seed,
    }
    .missile_motion(&facts, shot.spread_degrees);
    perf::instant!("projectile", "iw4l.sim", |ctx| {
        let weapon: u32 = shot.weapon;
        ctx.arg("weapon", i64::from(weapon));
        ctx.arg("weapon_kind", "missile");
    });
    let projectile = ProjectileState {
        id,
        owner: shot.attacker,
        owner_life: shot.attacker_life,
        weapon: shot.weapon,
        origin,
        velocity: pos.tr_delta,
        pos,
        apos,
        entnum,
        launch_time,
        spawn_time_ms: time_ms,
        detonate_at_ms: None,
        cleanup_at_ms: time_ms.saturating_add(crate::equipment::ROCKET_CLEANUP_MS),
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
        grounded: false,
        guide,
        attached_to: None,
        cause: shot.fire_cause,
    };
    if let Some(mut run) = world
        .ecs()
        .get_resource_mut::<crate::input_program::InputProgramRun>()
        && let Some(actor) = run.actor_mut(shot.attacker)
    {
        actor.launches.push(projectile);
    }
    world.push_projectile(projectile);
    Some(projectile)
}
