use crate::bullet_collision::{EntityCollisionEpoch, EntityCollisionTraceGeom};
use crate::damage::DamageAttempt;
use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, PelletId};
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::world_objects::DestructibleDamageIntent;
use crate::{
    BulletTraceQuery, ClientId, ColliderId, MASK_BULLET_WORLD, ProjectileId, Tick, TraceOutcome,
    bullet_trace_with_entity_models, level_time_ms,
};
use entity_iw4::{
    GRENADE_SPIN_PITCH_MAX, GRENADE_SPIN_PITCH_MIN, GRENADE_SPIN_ROLL_MAX, GRENADE_SPIN_ROLL_MIN,
    MissileLandAnglesIn, TR_STATIONARY, Trajectory, bg_evaluate_trajectory,
    bg_evaluate_trajectory_delta, g_fire_grenade_no_draw_ms, g_init_grenade_apos,
    g_init_grenade_pos, missile_land_angles,
};
use trace_iw4::surface_type_from_flags;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EquipmentRuntimeFacts {
    pub offhand_class: i32,
    pub start_ammo: i32,
    pub clip_size: i32,
    pub impact_damage: i32,
    pub fuse_time_ms: i32,

    pub hold_fire_time_ms: i32,

    pub cook_off_hold: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
}

impl EquipmentRuntimeFacts {
    pub fn is_usable(self) -> bool {
        self.projectile_speed > 0 && (self.fuse_time_ms > 0 || self.impact_damage > 0)
    }

    pub fn is_offhand(self) -> bool {
        self.offhand_class != 0
    }

    pub fn spawn_clip_count(self) -> i32 {
        self.start_ammo.max(self.clip_size).max(1)
    }

    pub fn fuse_ticks(self) -> u32 {
        if self.fuse_time_ms <= 0 {
            u32::MAX
        } else {
            (self.fuse_time_ms as u32 + 49) / 50
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileState {
    pub id: ProjectileId,
    pub owner: ClientId,
    pub owner_life: LifeSequence,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub gravity: f32,
    pub age_ticks: u32,
    pub fuse_ticks: u32,

    pub pos: Trajectory,

    pub apos: Trajectory,

    pub entnum: i32,

    pub launch_time: i32,
}

impl ProjectileState {
    pub fn origin_at(&self, at_time_ms: i32) -> [f32; 3] {
        bg_evaluate_trajectory(&self.pos, at_time_ms)
    }

    pub fn velocity_at(&self, at_time_ms: i32) -> [f32; 3] {
        bg_evaluate_trajectory_delta(&self.pos, at_time_ms)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileHitGeometry {
    World,
    Player,
    Entity { epoch: EntityCollisionEpoch },
    Bounce,
    Fuse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileImpact {
    pub id: ProjectileId,
    pub owner: ClientId,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub geometry: ProjectileHitGeometry,
    pub terminal: Option<ColliderId>,
    pub amount: i32,

    pub fraction: Option<f32>,

    pub surface_flags: Option<u32>,

    pub surf_type: Option<u8>,

    pub entnum: i32,
}

pub fn projectile_birth_ms(tick: Tick, age_ticks: u32) -> i32 {
    (tick
        .0
        .saturating_sub(age_ticks)
        .saturating_mul(crate::MATCH_TICK_MS)) as i32
}

fn forward(angles: [f32; 3]) -> [f32; 3] {
    let pitch = angles[0].to_radians();
    let yaw = angles[1].to_radians();
    [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    ]
}

pub(crate) fn spawn_offhand_projectile(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    tick: Tick,
) -> bool {
    let Some(facts) = world.equipment_facts_for(weapon) else {
        return false;
    };
    if !facts.is_usable() {
        return false;
    }
    let (origin, angles, gravity, owner_life) = {
        let Some(ps) = world.player(id) else {
            return false;
        };
        (
            [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] + ps.view_height_current,
            ],
            ps.viewangles,
            ps.gravity.max(0) as f32,
            world.client_meta(id).unwrap().life_sequence,
        )
    };
    let direction = forward(angles);
    let (pitch_rate, roll_rate) = grenade_spin_rates(world);
    let time_ms = level_time_ms(tick);
    let apos = g_init_grenade_apos(direction, time_ms, pitch_rate, roll_rate);
    let velocity = [
        direction[0] * facts.projectile_speed as f32,
        direction[1] * facts.projectile_speed as f32,
        direction[2] * facts.projectile_speed as f32 + facts.projectile_speed_up as f32,
    ];
    let pos = g_init_grenade_pos(origin, velocity, time_ms);
    let speed =
        (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    let launch_time = time_ms + g_fire_grenade_no_draw_ms(speed);
    let id_projectile = world.allocate_projectile_id();
    let entnum = world
        .allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile)
        .expect("G_Spawn exhausted dynamic entity slots for offhand projectile")
        .number();
    world.push_projectile(ProjectileState {
        id: id_projectile,
        owner: id,
        owner_life,
        weapon,
        origin,
        velocity: pos.tr_delta,
        gravity,
        age_ticks: 0,
        fuse_ticks: facts.fuse_ticks(),
        pos,
        apos,
        entnum,
        launch_time,
    });
    true
}

fn grenade_spin_rates(world: &mut FrameWorld) -> (f32, f32) {
    let rng = world.combat_rng_mut();
    let pitch_sign = if rng.next_index(2) == 0 { 1.0 } else { -1.0 };
    let pitch = host_flrand(rng, GRENADE_SPIN_PITCH_MIN, GRENADE_SPIN_PITCH_MAX);
    let roll_sign = if rng.next_index(2) == 0 { 1.0 } else { -1.0 };
    let roll = host_flrand(rng, GRENADE_SPIN_ROLL_MIN, GRENADE_SPIN_ROLL_MAX);
    (pitch_sign * pitch, roll_sign * roll)
}

fn host_flrand(rng: &mut crate::MatchRng, min: f32, max: f32) -> f32 {
    let u = rng.next_u32() as f32 * (1.0 / 4_294_967_296.0);
    min + (max - min) * u
}

pub(crate) fn phase_offhand(
    world: &mut FrameWorld,
    _tick: Tick,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    for (id, cmd) in cmds {
        if !world
            .client_meta(*id)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let requested = u32::from(cmd.off_hand_index);

        if requested != 0 {
            if let Some(ps) = world.player_mut(*id) {
                ps.off_hand_index = requested as i32;
            }
        }
    }
}

pub(crate) fn think_projectile(world: &mut FrameWorld, tick: Tick, entnum: i32) {
    struct PendingDetonation {
        projectile: ProjectileState,
        origin: [f32; 3],
        normal: [f32; 3],
        surf_type: u8,
        geometry: ProjectileHitGeometry,
        terminal: Option<ColliderId>,
        amount: i32,
        fraction: Option<f32>,
        surface_flags: Option<u32>,
    }
    let mut detonated = Vec::new();
    let mut direct_hits = Vec::new();
    let mut destructible_intents = Vec::new();
    let mut impacts = Vec::new();
    let Some(mut projectile) = world.projectile_by_number(entnum) else {
        return;
    };
    if projectile.age_ticks >= projectile.fuse_ticks {
        let _ = world.remove_projectile_by_number(entnum);
        return;
    }
    let script_models: Vec<EntityCollisionTraceGeom> = world
        .entity_collision_capabilities()
        .iter()
        .map(|capabilities| capabilities.trace_geom())
        .collect();
    let cmodels = world.clip_cmodels().clone();
    let players = world.alive_collision_poses();
    let (brushes, bsp, mesh) = world.take_clip_map();
    let time = level_time_ms(tick);
    let start = projectile.origin;
    let end = projectile.origin_at(time);
    let outcome = bullet_trace_with_entity_models(
        &brushes,
        &bsp,
        &cmodels,
        &mesh,
        &players,
        &script_models,
        &BulletTraceQuery {
            start,
            end,
            mask: MASK_BULLET_WORLD,
            ignore: Some(projectile.owner),
            ignore_hit: None,
        },
    );
    let mut pending_detonation = None;
    let hit = match outcome {
        TraceOutcome::Hit {
            fraction,
            end,
            normal,
            collider,
        } => Some((end, normal, Some(collider), fraction)),
        TraceOutcome::StartSolid { collider, end } => {
            Some((end, startsolid_normal(start, end), collider, 0.0))
        }
        TraceOutcome::Miss { .. } | TraceOutcome::Invalid { .. } => None,
    };
    if let Some((end, normal, collider, fraction)) = hit {
        projectile.origin = end;
        let facts = required_projectile_facts(world, projectile.weapon);
        let (surface_flags, hit_surf) = match collider {
            Some(ColliderId::World { surface_flags, .. }) => (
                Some(surface_flags),
                Some(surface_type_from_flags(surface_flags)),
            ),
            _ => (None, None),
        };
        match collider {
            Some(ColliderId::Player { client, .. }) => {
                if facts.stick_to_players {
                    stick_missile(tick, &mut projectile, end);
                    push_grenade_stick(world, tick, &projectile);
                } else {
                    direct_hits.push((projectile, client));
                    pending_detonation = Some(PendingDetonation {
                        projectile,
                        origin: end,
                        normal,
                        surf_type: 0,
                        geometry: ProjectileHitGeometry::Player,
                        terminal: collider,
                        amount: facts.impact_damage.max(0),
                        fraction: Some(fraction),
                        surface_flags: None,
                    });
                    projectile.age_ticks = projectile.fuse_ticks;
                }
            }
            Some(
                collider @ (ColliderId::EntityDObjBone { .. }
                | ColliderId::EntityLinkedBrush { .. }),
            ) => {
                let epoch = entity_collision_epoch(collider, &script_models)
                    .unwrap_or(EntityCollisionEpoch::CurrentTick);
                if facts.proj_impact_explode {
                    if let Some(target) = match collider {
                        ColliderId::EntityDObjBone { owner, .. }
                        | ColliderId::EntityLinkedBrush { owner, .. } => owner.script_model(),
                        _ => None,
                    } {
                        destructible_intents.push(DestructibleDamageIntent {
                            source: DamageSource::Projectile(projectile.id),
                            pellet: PelletId(0),
                            attacker: projectile.owner,
                            attacker_life: projectile.owner_life,
                            target,
                            amount: facts.impact_damage.max(0) as u32,
                            epoch,
                        });
                    }
                    pending_detonation = Some(PendingDetonation {
                        projectile,
                        origin: end,
                        normal,
                        surf_type: 0,
                        geometry: ProjectileHitGeometry::Entity { epoch },
                        terminal: Some(collider),
                        amount: facts.impact_damage.max(0),
                        fraction: Some(fraction),
                        surface_flags: None,
                    });
                    projectile.age_ticks = projectile.fuse_ticks;
                } else if facts.stick_to_players {
                    stick_missile(tick, &mut projectile, end);
                    push_grenade_stick(world, tick, &projectile);
                } else {
                    bounce_missile(world, tick, &mut projectile, end, normal, fraction, 0);
                    push_grenade_bounce(world, tick, &projectile, 0);
                    impacts.push(ProjectileImpact {
                        id: projectile.id,
                        owner: projectile.owner,
                        weapon: projectile.weapon,
                        origin: end,
                        geometry: ProjectileHitGeometry::Bounce,
                        terminal: Some(collider),
                        amount: 0,
                        fraction: Some(fraction),
                        surface_flags: None,
                        surf_type: None,
                        entnum: projectile.entnum,
                    });
                }
            }
            Some(world_collider @ ColliderId::World { .. }) => {
                let surf_type = hit_surf.unwrap_or(0);
                if facts.proj_impact_explode {
                    pending_detonation = Some(PendingDetonation {
                        projectile,
                        origin: end,
                        normal,
                        surf_type,
                        geometry: ProjectileHitGeometry::World,
                        terminal: Some(world_collider),
                        amount: facts.impact_damage.max(0),
                        fraction: Some(fraction),
                        surface_flags,
                    });
                    projectile.age_ticks = projectile.fuse_ticks;
                } else if facts.stick_to_players {
                    stick_missile(tick, &mut projectile, end);
                    push_grenade_stick(world, tick, &projectile);
                } else {
                    bounce_missile(
                        world,
                        tick,
                        &mut projectile,
                        end,
                        normal,
                        fraction,
                        surf_type,
                    );
                    push_grenade_bounce(world, tick, &projectile, surf_type);
                    impacts.push(ProjectileImpact {
                        id: projectile.id,
                        owner: projectile.owner,
                        weapon: projectile.weapon,
                        origin: end,
                        geometry: ProjectileHitGeometry::Bounce,
                        terminal: Some(world_collider),
                        amount: 0,
                        fraction: Some(fraction),
                        surface_flags,
                        surf_type: hit_surf,
                        entnum: projectile.entnum,
                    });
                }
            }
            None => {
                if facts.proj_impact_explode {
                    pending_detonation = Some(PendingDetonation {
                        projectile,
                        origin: end,
                        normal,
                        surf_type: 0,
                        geometry: ProjectileHitGeometry::World,
                        terminal: None,
                        amount: facts.impact_damage.max(0),
                        fraction: Some(fraction),
                        surface_flags: None,
                    });
                    projectile.age_ticks = projectile.fuse_ticks;
                }
            }
        }
    } else {
        projectile.origin = end;
    }
    projectile.velocity = projectile.velocity_at(time);
    projectile.age_ticks = projectile.age_ticks.saturating_add(1);
    if let Some(info) = pending_detonation {
        detonated.push(info);
    } else if projectile.age_ticks >= projectile.fuse_ticks {
        detonated.push(PendingDetonation {
            projectile,
            origin: projectile.origin,
            normal: [0.0, 0.0, 1.0],
            surf_type: 0,
            geometry: ProjectileHitGeometry::Fuse,
            terminal: None,
            amount: 0,
            fraction: None,
            surface_flags: None,
        });
    }
    world.restore_clip_map(brushes, bsp, mesh);
    if detonated.is_empty() {
        if let Some(row) = world.projectile_mut_by_number(entnum) {
            *row = projectile;
        }
    } else {
        let _ = world.remove_projectile_by_number(entnum);
    }
    for info in &detonated {
        impacts.push(ProjectileImpact {
            id: info.projectile.id,
            owner: info.projectile.owner,
            weapon: info.projectile.weapon,
            origin: info.origin,
            geometry: info.geometry,
            terminal: info.terminal,
            amount: info.amount,
            fraction: info.fraction,
            surface_flags: info.surface_flags,
            surf_type: info.surface_flags.map(|_| info.surf_type),
            entnum: info.projectile.entnum,
        });
    }
    world.record_projectile_impacts(tick, &impacts);
    for info in &detonated {
        world.note_dying_missile(tick, info.projectile);
    }
    let resolves_damage = world.publishes_snapshot();
    if resolves_damage {
        for (projectile, target) in direct_hits {
            let facts = required_projectile_facts(world, projectile.weapon);
            let Some(target_meta) = world.client_meta(target) else {
                continue;
            };
            let intent = DamageAttempt {
                source: DamageSource::Projectile(projectile.id),
                pellet: PelletId(0),
                attacker: projectile.owner,
                attacker_life: projectile.owner_life,
                target,
                target_life: target_meta.life_sequence,
                weapon: projectile.weapon,
                amount: facts.impact_damage.max(0),
                killcam_entity_start_time: projectile_birth_ms(tick, projectile.age_ticks),
                inflictor_origin: Some(projectile.origin),
                hitloc: 0,
            };
            let _ = crate::damage::apply_damage_attempt(world, tick, &intent);
        }
        let destructible = world
            .world_objects_mut()
            .apply_destructible_damage_batch(&destructible_intents);
        for explode in &destructible.explodes {
            let splash = crate::damage::radius_attempts_from_truck_explode(world, explode);
            for attempt in &splash {
                let _ = crate::damage::apply_damage_attempt(world, tick, attempt);
            }
        }
    }
    for info in detonated {
        let facts = required_projectile_facts(world, info.projectile.weapon);
        let event_kind = if facts.projectile_explosion_type == 2 {
            entity_iw4::EntityEventKind::FLASHBANG_EXPLODE
        } else {
            entity_iw4::EntityEventKind::GRENADE_EXPLODE
        };
        world.push_entity_event(
            tick,
            EventAudience::All,
            event_kind,
            crate::EntityEventPayload {
                number: info.projectile.entnum,
                attacker_entity_num: info.projectile.owner.0 as i32,
                weapon: info.projectile.weapon,
                correlation: info.projectile.id.0,
                origin: info.origin,
                direction: info.normal,
                surf_type: info.surf_type,
                ..Default::default()
            },
        );
        if !resolves_damage {
            continue;
        }
        if facts.projectile_explosion_type == 2 {
            crate::damage::apply_flashbang_blast(
                world,
                tick,
                info.origin,
                facts.explosion_radius.max(0) as f32,
                facts.explosion_radius_min.max(0) as f32,
            );
        }
        let radius = facts
            .explosion_radius
            .max(facts.explosion_radius_min)
            .max(0) as f32;
        if radius <= 0.0 || facts.explosion_inner_damage <= 0 {
            continue;
        }
        let inner_radius = facts
            .explosion_radius_min
            .max(0)
            .min(facts.explosion_radius) as f32;
        let mut intents = Vec::new();
        for target in crate::damage::radius_player_candidates(world, info.origin, radius) {
            let Some(meta) = world.client_meta(target) else {
                continue;
            };
            if meta.lifecycle != ClientLifecycle::Alive {
                continue;
            }
            let Some(ps) = world.player(target) else {
                continue;
            };
            let dx = ps.origin[0] - info.origin[0];
            let dy = ps.origin[1] - info.origin[1];
            let dz = ps.origin[2] - info.origin[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();
            if distance > radius {
                continue;
            }
            let amount = if distance <= inner_radius || radius <= inner_radius {
                facts.explosion_inner_damage
            } else {
                let fraction = (distance - inner_radius) / (radius - inner_radius);
                (facts.explosion_inner_damage as f32
                    + (facts.explosion_outer_damage - facts.explosion_inner_damage) as f32
                        * fraction)
                    .round() as i32
            };
            intents.push(DamageAttempt {
                source: DamageSource::Projectile(info.projectile.id),
                pellet: PelletId(0),
                attacker: info.projectile.owner,
                attacker_life: info.projectile.owner_life,
                target,
                target_life: meta.life_sequence,
                weapon: info.projectile.weapon,
                amount,
                killcam_entity_start_time: projectile_birth_ms(tick, info.projectile.age_ticks),
                inflictor_origin: Some(info.origin),
                hitloc: 0,
            });
        }
        for attempt in &intents {
            let _ = crate::damage::apply_damage_attempt(world, tick, attempt);
        }
    }
}

fn push_grenade_bounce(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &ProjectileState,
    surf_type: u8,
) {
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::GRENADE_BOUNCE,
        crate::EntityEventPayload {
            number: projectile.entnum,
            weapon: projectile.weapon,
            correlation: projectile.id.0,
            origin: projectile.origin,
            event_parm: i32::from(surf_type),
            surf_type,
            ..Default::default()
        },
    );
}

fn push_grenade_stick(world: &mut FrameWorld, tick: Tick, projectile: &ProjectileState) {
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::GRENADE_STICK,
        crate::EntityEventPayload {
            number: projectile.entnum,
            weapon: projectile.weapon,
            correlation: projectile.id.0,
            origin: projectile.origin,
            ..Default::default()
        },
    );
}

fn stick_missile(tick: Tick, projectile: &mut ProjectileState, origin: [f32; 3]) {
    let time = level_time_ms(tick);
    projectile.origin = origin;
    projectile.velocity = [0.0, 0.0, 0.0];
    projectile.pos.tr_type = TR_STATIONARY;
    projectile.pos.tr_time = time;
    projectile.pos.tr_duration = 0;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_delta = [0.0, 0.0, 0.0];
}

fn apply_missile_land_angles(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    fraction: f32,
) {
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    let hit_time = prev.saturating_add(((time - prev) as f32 * fraction) as i32);
    let (g_random, wall_spin_addend) = {
        let rng = world.combat_rng_mut();
        let g_random = rng.next_u32() as f32 * (1.0 / 4_294_967_296.0);
        let wall_spin_addend = ((rng.next_u32() & 0x7f) as i32 - 63) as f32;
        (g_random, wall_spin_addend)
    };
    projectile.apos = missile_land_angles(MissileLandAnglesIn {
        apos: projectile.apos,
        normal,
        hit_time_ms: hit_time,
        force_align: false,
        g_random,
        wall_spin_addend,
    })
    .apos;
}

fn bounce_missile(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    origin: [f32; 3],
    normal: [f32; 3],
    fraction: f32,
    surf_type: u8,
) {
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    let hit_time = prev.saturating_add(((time - prev) as f32 * fraction) as i32);
    projectile.velocity = projectile.velocity_at(hit_time);
    let facts = required_projectile_facts(world, projectile.weapon);
    bounce_velocity(projectile, normal, &facts, surf_type);
    projectile.origin = origin;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_time = time;
    projectile.pos.tr_delta = projectile.velocity;
    apply_missile_land_angles(world, tick, projectile, normal, fraction);
}

fn required_projectile_facts(world: &FrameWorld, weapon: u32) -> EquipmentRuntimeFacts {
    world
        .equipment_facts_for(weapon)
        .or_else(|| world.missile_launch_facts(weapon))
        .unwrap_or_else(|| {
            panic!("G_RunMissile needs a validated equipment row; bounce/damage must not default to zero");
        })
}

fn bounce_velocity(
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    facts: &EquipmentRuntimeFacts,
    surf_type: u8,
) {
    let coefficients = facts
        .parallel_bounce
        .as_ref()
        .zip(facts.perpendicular_bounce.as_ref())
        .and_then(|(p, n)| p.get(surf_type as usize).zip(n.get(surf_type as usize)))
        .unwrap_or_else(|| panic!("missing projectile surface bounce coefficients"));
    let normal_speed = projectile.velocity[0] * normal[0]
        + projectile.velocity[1] * normal[1]
        + projectile.velocity[2] * normal[2];
    for axis in 0..3 {
        let normal_velocity = normal[axis] * normal_speed;
        let tangent = projectile.velocity[axis] - normal_velocity;
        projectile.velocity[axis] = tangent * coefficients.0 - normal_velocity * coefficients.1;
    }
}

fn startsolid_normal(start: [f32; 3], end: [f32; 3]) -> [f32; 3] {
    let d = [start[0] - end[0], start[1] - end[1], start[2] - end[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [d[0] / len, d[1] / len, d[2] / len]
    }
}

fn entity_collision_epoch(
    terminal: ColliderId,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityCollisionEpoch> {
    let owner = match terminal {
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. } => {
            owner
        }
        ColliderId::World { .. } | ColliderId::Player { .. } => return None,
    };
    rows.iter()
        .find(|row| row.owner == owner)
        .map(|row| row.epoch)
}
