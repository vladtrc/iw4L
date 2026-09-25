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
    GLASS_PROJECTILE_PANE_HOPS, GRENADE_SPIN_PITCH_MAX, GRENADE_SPIN_PITCH_MIN,
    GRENADE_SPIN_ROLL_MAX, GRENADE_SPIN_ROLL_MIN, MISSILE_GLASS_SHATTER_VEL, MissileLandAnglesIn,
    TR_GRAVITY, TR_STATIONARY, Trajectory, bg_evaluate_trajectory, bg_evaluate_trajectory_delta,
    g_fire_grenade_no_draw_ms, g_fire_missile_apos, g_init_grenade_apos, g_init_grenade_pos,
    missile_land_angles,
};
use trace_iw4::surface_type_from_flags;

const SURF_TYPE_GLASS: u8 = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EquipmentRuntimeFacts {
    pub offhand_class: i32,
    pub start_ammo: i32,
    pub clip_size: i32,
    pub impact_damage: i32,
    pub fuse_time_ms: i32,

    pub hold_fire_time_ms: i32,

    pub cook_off_hold: bool,

    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub weap_type: i32,
    pub weap_class: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
}

impl EquipmentRuntimeFacts {
    pub fn is_usable(self) -> bool {
        self.projectile_speed > 0 && (self.fuse_time_ms > 0 || self.impact_damage > 0)
    }

    pub(crate) fn is_throwing_knife(self) -> bool {
        self.weap_class == WEAPCLASS_THROWINGKNIFE
    }

    pub fn is_offhand(self) -> bool {
        self.offhand_class != 0
    }

    pub fn spawn_clip_count(self) -> i32 {
        self.start_ammo.max(self.clip_size).max(1)
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
    pub pos: Trajectory,
    pub apos: Trajectory,
    pub entnum: i32,
    pub launch_time: i32,
    pub spawn_time_ms: i32,
    pub detonate_at_ms: Option<i32>,
    pub cleanup_at_ms: i32,
    pub travel_distance: f32,
    pub live: bool,
    pub stuck_pane: Option<u32>,
    pub grounded: bool,
}

impl ProjectileState {
    pub fn origin_at(&self, at_time_ms: i32) -> [f32; 3] {
        bg_evaluate_trajectory(&self.pos, at_time_ms)
    }

    pub fn velocity_at(&self, at_time_ms: i32) -> [f32; 3] {
        bg_evaluate_trajectory_delta(&self.pos, at_time_ms)
    }

    pub fn is_armed(&self, activate_dist: i32) -> bool {
        self.live && (activate_dist <= 0 || self.travel_distance >= activate_dist as f32)
    }
}

pub const GRENADE_FUSE_CAP_MS: i32 = 60_000;
const OFFHAND_CLASS_SMOKE: i32 = 2;
pub const GRENADE_DEFAULT_FUSE_MS: i32 = 30_000;
pub const ROCKET_CLEANUP_MS: i32 = 60_000;
pub const BOUNCE_EVENT_SPEED_DELTA: f32 = 100.0;
const WEAPCLASS_THROWINGKNIFE: i32 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrenadeLaunchKind {
    Thrown { remaining_fuse_ms: Option<i32> },
    Launcher,
}

pub fn projectile_birth_ms(projectile: &ProjectileState) -> i32 {
    projectile.spawn_time_ms
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

fn vec3_length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn vec3_normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = vec3_length(v);
    if len < 1e-6 {
        None
    } else {
        Some([v[0] / len, v[1] / len, v[2] / len])
    }
}

fn flatten_xy(dir: [f32; 3]) -> Option<[f32; 3]> {
    vec3_normalize([dir[0], dir[1], 0.0])
}

fn project_owner_velocity(launch_vel: [f32; 3], owner_vel: [f32; 3]) -> [f32; 3] {
    let Some(dir) = vec3_normalize(launch_vel) else {
        return launch_vel;
    };
    let along = owner_vel[0] * dir[0] + owner_vel[1] * dir[1] + owner_vel[2] * dir[2];
    [
        launch_vel[0] + dir[0] * along,
        launch_vel[1] + dir[1] * along,
        launch_vel[2] + dir[2] * along,
    ]
}

fn grenade_launch_velocity(
    direction: [f32; 3],
    facts: &EquipmentRuntimeFacts,
    owner_vel: [f32; 3],
) -> [f32; 3] {
    let speed = facts.projectile_speed as f32;
    let mut velocity = [
        direction[0] * speed,
        direction[1] * speed,
        direction[2] * speed + facts.projectile_speed_up as f32,
    ];
    if facts.projectile_speed_forward != 0
        && let Some(flat) = flatten_xy(direction)
    {
        let extra = facts.projectile_speed_forward as f32;
        velocity[0] += flat[0] * extra;
        velocity[1] += flat[1] * extra;
        velocity[2] += flat[2] * extra;
    }
    project_owner_velocity(velocity, owner_vel)
}

fn grenade_fuse_due(now_ms: i32, detonate_at_ms: Option<i32>, live: bool) -> bool {
    detonate_at_ms.is_some_and(|deadline| live && deadline <= now_ms)
}

// The airdrop marker's fuse would otherwise pop the flare midair on a steep throw.
fn waits_for_ground(world: &FrameWorld, facts: &EquipmentRuntimeFacts, weapon: u32) -> bool {
    facts.offhand_class == OFFHAND_CLASS_SMOKE
        || world.weapon_script_name(weapon) == gamemode_iw4::killstreaks::AIRDROP_MARKER_WEAPON
}

fn grenade_deadlines(
    facts: &EquipmentRuntimeFacts,
    kind: GrenadeLaunchKind,
    now_ms: i32,
) -> (Option<i32>, i32) {
    let cap_at = now_ms.saturating_add(GRENADE_FUSE_CAP_MS);
    if facts.is_throwing_knife() {
        return (None, cap_at);
    }
    let cleanup_at_ms = cap_at;
    match kind {
        GrenadeLaunchKind::Launcher if facts.fuse_time_ms <= 0 => {
            if facts.projectile_activate_dist != 0 {
                (None, cleanup_at_ms)
            } else {
                (
                    Some(now_ms.saturating_add(GRENADE_DEFAULT_FUSE_MS).min(cap_at)),
                    cleanup_at_ms,
                )
            }
        }
        GrenadeLaunchKind::Thrown { remaining_fuse_ms } => {
            let authored = facts.fuse_time_ms;
            let fuse = if facts.timed_detonation {
                remaining_fuse_ms.filter(|&ms| ms > 0).unwrap_or(authored)
            } else {
                authored
            };
            let fuse = if fuse <= 0 {
                GRENADE_DEFAULT_FUSE_MS
            } else {
                fuse.min(GRENADE_FUSE_CAP_MS)
            };
            (Some(now_ms.saturating_add(fuse)), cleanup_at_ms)
        }
        GrenadeLaunchKind::Launcher => {
            let fuse = if facts.fuse_time_ms <= 0 {
                GRENADE_DEFAULT_FUSE_MS
            } else {
                facts.fuse_time_ms.min(GRENADE_FUSE_CAP_MS)
            };
            (Some(now_ms.saturating_add(fuse)), cleanup_at_ms)
        }
    }
}

pub(crate) fn spawn_grenade_projectile(
    world: &mut FrameWorld,
    owner: ClientId,
    weapon: u32,
    tick: Tick,
    origin: [f32; 3],
    angles: [f32; 3],
    owner_vel: [f32; 3],
    kind: GrenadeLaunchKind,
) -> bool {
    let Some(facts) = world.equipment_facts_for(weapon) else {
        return false;
    };
    if !facts.is_usable() {
        return false;
    }
    let owner_life = world.client_meta(owner).unwrap().life_sequence;
    let direction = forward(angles);
    let time_ms = level_time_ms(tick);
    let (pitch_rate, roll_rate) = match kind {
        GrenadeLaunchKind::Launcher => (0.0, 0.0),
        GrenadeLaunchKind::Thrown { .. } if facts.weap_class == WEAPCLASS_THROWINGKNIFE => {
            (entity_iw4::GRENADE_BLADE_SPIN_PITCH, 0.0)
        }
        GrenadeLaunchKind::Thrown { .. } => grenade_spin_rates(world),
    };
    let apos = if pitch_rate == 0.0 && roll_rate == 0.0 {
        g_fire_missile_apos(direction)
    } else {
        g_init_grenade_apos(direction, time_ms, pitch_rate, roll_rate)
    };
    let velocity = grenade_launch_velocity(direction, &facts, owner_vel);
    let pos = g_init_grenade_pos(origin, velocity, time_ms);
    let speed = vec3_length(velocity);
    let launch_time = time_ms + g_fire_grenade_no_draw_ms(speed);
    let (detonate_at_ms, cleanup_at_ms) = grenade_deadlines(&facts, kind, time_ms);
    let id_projectile = world.allocate_projectile_id();
    let entnum = world
        .allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile)
        .expect("G_Spawn exhausted dynamic entity slots for grenade projectile")
        .number();
    world.push_projectile(ProjectileState {
        id: id_projectile,
        owner,
        owner_life,
        weapon,
        origin,
        velocity: pos.tr_delta,
        pos,
        apos,
        entnum,
        launch_time,
        spawn_time_ms: time_ms,
        detonate_at_ms,
        cleanup_at_ms,
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
        grounded: false,
    });
    true
}

pub(crate) fn spawn_offhand_projectile(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    tick: Tick,
    remaining_fuse_ms: Option<i32>,
) -> bool {
    let (origin, angles, owner_vel) = {
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
            ps.velocity,
        )
    };
    spawn_grenade_projectile(
        world,
        id,
        weapon,
        tick,
        origin,
        angles,
        owner_vel,
        GrenadeLaunchKind::Thrown { remaining_fuse_ms },
    )
}

pub(crate) fn explode_offhand_in_hand(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    tick: Tick,
) {
    let Some(facts) = world.equipment_facts_for(weapon) else {
        return;
    };
    let Some(ps) = world.player(id).copied() else {
        return;
    };
    let owner_life = world
        .client_meta(id)
        .map(|m| m.life_sequence)
        .unwrap_or_default();
    let origin = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let radius = facts
        .explosion_radius
        .max(facts.explosion_radius_min)
        .max(0) as f32;
    if facts.projectile_explosion_type == 2 {
        crate::damage::apply_flashbang_blast(
            world,
            tick,
            origin,
            facts.explosion_radius.max(0) as f32,
            facts.explosion_radius_min.max(0) as f32,
        );
    }
    if radius <= 0.0 || facts.explosion_inner_damage <= 0 {
        return;
    }
    let blast = crate::damage::ExplosionBlast {
        origin,
        radius,
        inner_damage: facts.explosion_inner_damage as f32,
        outer_damage: facts.explosion_outer_damage.max(0) as f32,
        weapon,
        source: DamageSource::Projectile(ProjectileId(0)),
        attacker: id,
        attacker_life: owner_life,
        killcam_entity_start_time: level_time_ms(tick),
    };
    crate::damage::apply_explosion_blast(world, tick, &blast);
    let chain = crate::damage::apply_explosion_destructibles(world, &blast, None);
    for explode in &chain {
        crate::damage::apply_explosion_blast(
            world,
            tick,
            &crate::damage::ExplosionBlast::from_destructible(explode),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileHitGeometry {
    World,
    Player,
    Entity { epoch: EntityCollisionEpoch },
    Bounce,
    Fuse,
    Dud,
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
        splash: bool,
    }
    let mut detonated = Vec::new();
    let mut direct_hits = Vec::new();
    let mut destructible_intents = Vec::new();
    let mut impacts = Vec::new();
    let Some(mut projectile) = world.projectile_by_number(entnum) else {
        return;
    };
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    if time >= projectile.cleanup_at_ms
        && projectile
            .detonate_at_ms
            .is_none_or(|deadline| deadline > projectile.cleanup_at_ms || !projectile.live)
    {
        let _ = world.remove_projectile_by_number(entnum);
        return;
    }
    let facts = required_projectile_facts(world, projectile.weapon);
    let script_models: Vec<EntityCollisionTraceGeom> = world
        .entity_collision_capabilities()
        .iter()
        .map(|capabilities| capabilities.trace_geom())
        .collect();
    let content = world.content();
    let cmodels = content.clip_cmodels();
    let players = world.alive_collision_poses();
    let (brushes, bsp, mesh) = (
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_mesh(),
    );
    let start = projectile.origin;
    let mut eval_time = time;
    if let Some(deadline) = projectile
        .detonate_at_ms
        .filter(|&ms| projectile.live && ms <= time)
    {
        eval_time = eval_time.min(deadline);
    }
    if projectile.cleanup_at_ms > prev && projectile.cleanup_at_ms <= eval_time {
        eval_time = eval_time.min(projectile.cleanup_at_ms);
    }
    let end = projectile.origin_at(eval_time);
    let vel = projectile.velocity_at(eval_time);
    let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
    if let Some(pane) = projectile.stuck_pane {
        if world.world_objects().glass_is_solid(pane) {
            if let Some(row) = world.projectile_mut_by_number(entnum) {
                *row = projectile;
            }
            return;
        }
        unstick_missile(tick, &mut projectile);
        projectile.stuck_pane = None;
    }
    if facts.is_throwing_knife() && projectile.pos.tr_type == TR_STATIONARY {
        return;
    }
    let mut trace_start = start;
    let mut hops = 0u32;
    let mut hop_capped = false;
    let outcome = loop {
        let glass_pairs = world.world_objects().glass_damage_pairs();
        let outcome = bullet_trace_with_entity_models(
            &brushes,
            &bsp,
            &cmodels,
            &mesh,
            &players,
            &script_models,
            &BulletTraceQuery {
                start: trace_start,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(projectile.owner),
                ignore_hit: None,
            },
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
        let punched = match outcome {
            TraceOutcome::Hit {
                end: hit_end,
                collider:
                    ColliderId::World {
                        surface_flags,
                        glass_encoded,
                        ..
                    },
                ..
            }
            | TraceOutcome::StartSolid {
                end: hit_end,
                collider:
                    Some(ColliderId::World {
                        surface_flags,
                        glass_encoded,
                        ..
                    }),
            } if missile_glass_punch_eligible(speed, surface_flags, glass_encoded) => {
                let pane = u32::from(glass_encoded).saturating_sub(1);
                if world.world_objects().glass_is_solid(pane) {
                    let _ = world.world_objects_mut().force_shatter_glass(
                        pane,
                        time,
                        hit_end,
                        vel,
                        entity_iw4::GlassCause::Impact,
                    );
                    hops = hops.saturating_add(1);
                    if hops >= GLASS_PROJECTILE_PANE_HOPS {
                        hop_capped = true;
                        park_missile_at(tick, &mut projectile, hit_end, vel);
                        None
                    } else {
                        let dx = end[0] - trace_start[0];
                        let dy = end[1] - trace_start[1];
                        let dz = end[2] - trace_start[2];
                        let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
                        trace_start = [
                            hit_end[0] + dx / len * 0.25,
                            hit_end[1] + dy / len * 0.25,
                            hit_end[2] + dz / len * 0.25,
                        ];
                        Some(())
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if punched.is_some() && !hop_capped {
            continue;
        }
        break outcome;
    };
    if hop_capped {
        projectile.velocity = vel;
        if let Some(row) = world.projectile_mut_by_number(entnum) {
            *row = projectile;
        }
        return;
    }
    let mut pending_detonation = None;
    let mut hit = match outcome {
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
    let mut contact_origin = None;
    if facts.is_throwing_knife()
        && let Some((end, normal, collider, _)) = hit
        && !matches!(collider, Some(ColliderId::Player { .. }))
    {
        let probe_start = core::array::from_fn(|i| end[i] + normal[i] * 0.135);
        let probe_end = core::array::from_fn(|i| end[i] - normal[i] * 1.5);
        if let TraceOutcome::Hit {
            end,
            normal,
            collider,
            fraction,
        } = bullet_trace_with_entity_models(
            &brushes,
            &bsp,
            &cmodels,
            &mesh,
            &players,
            &script_models,
            &BulletTraceQuery {
                start: probe_start,
                end: probe_end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(projectile.owner),
                ignore_hit: None,
            },
            &|piece| world.world_objects().glass_is_solid(u32::from(piece)),
        ) {
            // Surface clearance precedes the knife's resting/embedded pose offsets.
            contact_origin = Some(core::array::from_fn(|i| end[i] + (end[i] - probe_end[i])));
            hit = Some((end, normal, Some(collider), fraction));
        }
    }
    let mut contact_time = eval_time;
    if let Some((_end, _normal, _collider, fraction)) = hit {
        contact_time = prev.saturating_add(((eval_time - prev) as f32 * fraction) as i32);
    }
    let mut fuse_due = grenade_fuse_due(time, projectile.detonate_at_ms, projectile.live);
    if fuse_due && !projectile.grounded && waits_for_ground(world, &facts, projectile.weapon) {
        fuse_due = false;
        projectile.detonate_at_ms = Some(time.saturating_add(crate::MATCH_TICK_MS as i32));
    }
    let cleanup_due = time >= projectile.cleanup_at_ms;
    let contact_before_fuse = hit.is_some()
        && projectile
            .detonate_at_ms
            .is_none_or(|deadline| contact_time <= deadline);
    enum TickOutcome {
        Contact,
        Fuse,
        Cleanup,
        Fly,
    }
    let tick_outcome = if hit.is_some() && contact_before_fuse {
        TickOutcome::Contact
    } else if fuse_due {
        TickOutcome::Fuse
    } else if cleanup_due {
        TickOutcome::Cleanup
    } else {
        TickOutcome::Fly
    };
    match tick_outcome {
        TickOutcome::Cleanup => {
            let _ = world.remove_projectile_by_number(entnum);
            return;
        }
        TickOutcome::Fuse => {
            let fuse_time = projectile.detonate_at_ms.unwrap_or(eval_time);
            projectile.origin = projectile.origin_at(fuse_time);
            projectile.velocity = projectile.velocity_at(fuse_time);
            pending_detonation = Some(PendingDetonation {
                projectile,
                origin: projectile.origin,
                normal: [0.0, 0.0, 1.0],
                surf_type: 0,
                geometry: ProjectileHitGeometry::Fuse,
                terminal: None,
                amount: 0,
                fraction: None,
                surface_flags: None,
                splash: projectile.live,
            });
        }
        TickOutcome::Fly => {
            let traveled = vec3_length([end[0] - start[0], end[1] - start[1], end[2] - start[2]]);
            projectile.origin = end;
            projectile.travel_distance = projectile.travel_distance + traveled;
            projectile.velocity = projectile.velocity_at(time);
            if let Some(row) = world.projectile_mut_by_number(entnum) {
                *row = projectile;
            }
            return;
        }
        TickOutcome::Contact => {
            let (end, normal, collider, fraction) = hit.expect("contact");
            let traveled = vec3_length([end[0] - start[0], end[1] - start[1], end[2] - start[2]]);
            projectile.origin = contact_origin.unwrap_or(end);
            projectile.travel_distance = projectile.travel_distance + traveled;
            let armed = projectile.is_armed(facts.projectile_activate_dist);
            let (surface_flags, hit_surf) = match collider {
                Some(ColliderId::World { surface_flags, .. }) => (
                    Some(surface_flags),
                    Some(surface_type_from_flags(surface_flags)),
                ),
                _ => (None, None),
            };
            match collider {
                _ if facts.is_throwing_knife() => {
                    let player_hit = matches!(collider, Some(ColliderId::Player { .. }));
                    if let Some(ColliderId::Player { client, .. }) = collider {
                        direct_hits.push((projectile, client));
                    }
                    knife_impact(
                        world,
                        tick,
                        &mut projectile,
                        normal,
                        fraction,
                        hit_surf.unwrap_or(0),
                        player_hit,
                    );
                    if projectile.pos.tr_type == TR_STATIONARY
                        && let Some(ColliderId::World { glass_encoded, .. }) = collider
                        && glass_encoded != 0
                    {
                        projectile.stuck_pane = Some(u32::from(glass_encoded) - 1);
                    }
                    impacts.push(ProjectileImpact {
                        id: projectile.id,
                        owner: projectile.owner,
                        weapon: projectile.weapon,
                        origin: end,
                        geometry: if player_hit {
                            ProjectileHitGeometry::Player
                        } else {
                            ProjectileHitGeometry::Bounce
                        },
                        terminal: collider,
                        amount: if player_hit {
                            facts.impact_damage.max(0)
                        } else {
                            0
                        },
                        fraction: Some(fraction),
                        surface_flags,
                        surf_type: hit_surf,
                        entnum: projectile.entnum,
                    });
                }
                Some(ColliderId::Player { client, .. }) => {
                    if facts.stick_to_players {
                        stick_missile(tick, &mut projectile, end);
                        push_grenade_stick(world, tick, &projectile);
                    } else if !armed {
                        direct_hits.push((projectile, client));
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: collider,
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    } else if facts.proj_impact_explode {
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
                            splash: true,
                        });
                    } else {
                        direct_hits.push((projectile, client));
                        bounce_missile(world, tick, &mut projectile, end, normal, fraction, 0);
                        impacts.push(ProjectileImpact {
                            id: projectile.id,
                            owner: projectile.owner,
                            weapon: projectile.weapon,
                            origin: end,
                            geometry: ProjectileHitGeometry::Bounce,
                            terminal: collider,
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags: None,
                            surf_type: None,
                            entnum: projectile.entnum,
                        });
                    }
                }
                Some(
                    collider @ (ColliderId::EntityDObjBone { .. }
                    | ColliderId::EntityLinkedBrush { .. }),
                ) => {
                    let epoch = entity_collision_epoch(collider, &script_models)
                        .unwrap_or(EntityCollisionEpoch::CurrentTick);
                    if armed && facts.proj_impact_explode {
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
                                splash: false,
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
                            splash: true,
                        });
                    } else if facts.stick_to_players {
                        stick_missile(tick, &mut projectile, end);
                        push_grenade_stick(world, tick, &projectile);
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: Some(collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    } else {
                        bounce_missile(world, tick, &mut projectile, end, normal, fraction, 0);
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
                    if armed && facts.proj_impact_explode {
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
                            splash: true,
                        });
                    } else if facts.stick_to_players {
                        if let ColliderId::World { glass_encoded, .. } = world_collider
                            && glass_encoded != 0
                        {
                            let pane = u32::from(glass_encoded).saturating_sub(1);
                            if world.world_objects().glass_is_solid(pane) {
                                projectile.stuck_pane = Some(pane);
                            }
                        }
                        stick_missile(tick, &mut projectile, end);
                        push_grenade_stick(world, tick, &projectile);
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: Some(world_collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags,
                            splash: false,
                        });
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
                    if armed && facts.proj_impact_explode {
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
                            splash: true,
                        });
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: None,
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    }
                }
            }
        }
    }

    if let Some(info) = pending_detonation {
        detonated.push(info);
    }

    if detonated.is_empty() {
        projectile.velocity = projectile.velocity_at(time);
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
        if info.splash {
            world.note_dying_missile(tick, info.projectile);
        }
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
                killcam_entity_start_time: projectile_birth_ms(&projectile),
                inflictor_origin: Some(projectile.origin),
                hitloc: 0,
            };
            let _ = crate::damage::apply_damage_attempt(world, tick, &intent);
        }
        let destructible = world
            .world_objects_mut()
            .apply_destructible_damage_batch(&destructible_intents);
        for explode in &destructible.explodes {
            crate::damage::apply_explosion_blast(
                world,
                tick,
                &crate::damage::ExplosionBlast::from_destructible(explode),
            );
            crate::damage::apply_explode_glass_blast(world, tick, explode);
        }
    }
    for info in detonated {
        if world.weapon_script_name(info.projectile.weapon)
            == gamemode_iw4::killstreaks::AIRDROP_MARKER_WEAPON
        {
            world.push_entity_event(
                tick,
                EventAudience::All,
                entity_iw4::EntityEventKind::GRENADE_EXPLODE,
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
            crate::killstreaks::marker_impact(
                world,
                tick,
                info.projectile.owner,
                info.projectile.id.0,
                info.origin,
            );
            continue;
        }
        if info.splash {
            let facts = required_projectile_facts(world, info.projectile.weapon);
            crate::killstreaks::blast_aircraft(
                world,
                tick,
                info.projectile.owner,
                info.origin,
                facts
                    .explosion_radius
                    .max(facts.explosion_radius_min)
                    .max(0) as f32,
                facts.explosion_inner_damage.max(facts.impact_damage),
            );
        }
        if !info.splash {
            continue;
        }
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
        if radius <= 0.0 || (facts.explosion_inner_damage <= 0 && facts.explosion_outer_damage <= 0)
        {
            continue;
        }
        let blast = crate::damage::ExplosionBlast {
            origin: info.origin,
            radius,
            inner_damage: facts.explosion_inner_damage as f32,
            outer_damage: facts.explosion_outer_damage.max(0) as f32,
            weapon: info.projectile.weapon,
            source: DamageSource::Projectile(info.projectile.id),
            attacker: info.projectile.owner,
            attacker_life: info.projectile.owner_life,
            killcam_entity_start_time: projectile_birth_ms(&info.projectile),
        };
        crate::damage::apply_explosion_blast(world, tick, &blast);
        let chain = crate::damage::apply_explosion_destructibles(world, &blast, None);
        for explode in &chain {
            crate::damage::apply_explosion_blast(
                world,
                tick,
                &crate::damage::ExplosionBlast::from_destructible(explode),
            );
        }
        crate::damage::apply_shared_glass_blast(
            world,
            tick,
            info.origin,
            facts.explosion_inner_damage,
            facts.explosion_outer_damage,
            radius,
        );
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

fn missile_glass_punch_eligible(speed: f32, surface_flags: u32, glass_encoded: u16) -> bool {
    speed >= MISSILE_GLASS_SHATTER_VEL
        && surface_type_from_flags(surface_flags) == SURF_TYPE_GLASS
        && glass_encoded != 0
}

fn park_missile_at(
    tick: Tick,
    projectile: &mut ProjectileState,
    origin: [f32; 3],
    velocity: [f32; 3],
) {
    let time = level_time_ms(tick);
    projectile.origin = origin;
    projectile.velocity = velocity;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_time = time;
    projectile.pos.tr_delta = velocity;
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
    projectile.apos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: time,
        tr_duration: 0,
        tr_base: bg_evaluate_trajectory(&projectile.apos, time),
        tr_delta: [0.0; 3],
    };
}

fn knife_impact(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    fraction: f32,
    surf_type: u8,
    player_hit: bool,
) {
    let time = level_time_ms(tick);
    let hit_time =
        time - crate::MATCH_TICK_MS as i32 + (crate::MATCH_TICK_MS as f32 * fraction) as i32;
    let facts = required_projectile_facts(world, projectile.weapon);
    projectile.velocity = projectile.velocity_at(hit_time);
    bounce_velocity(projectile, normal, &facts, surf_type);
    let speed = vec3_length(projectile.velocity);
    let direction = vec3_normalize(projectile.velocity).unwrap_or([0.0; 3]);
    let incidence: f32 = (0..3).map(|i| direction[i] * normal[i]).sum();
    let floor = normal[2] > 0.7;
    let stop = (player_hit && facts.stick_to_players)
        || (!player_hit && ((floor && speed < 20.0) || incidence > 0.7));
    if !stop {
        projectile.origin = core::array::from_fn(|i| {
            projectile.origin[i]
                + if i == 2 {
                    (normal[i] * 0.1).min(0.0)
                } else {
                    normal[i] * 0.1
                }
        });
        projectile.pos.tr_base = projectile.origin;
        projectile.pos.tr_time = time;
        projectile.pos.tr_delta = projectile.velocity;
        apply_missile_land_angles(world, tick, projectile, normal, fraction);
        push_grenade_bounce(world, tick, projectile, surf_type);
        return;
    }
    let mut angles = bg_evaluate_trajectory(&projectile.apos, hit_time);
    let mut origin = projectile.origin;
    if !player_hit && floor && (speed < 20.0 || incidence < 0.7) {
        let forward = forward(angles);
        let dot: f32 = (0..3).map(|i| forward[i] * normal[i]).sum();
        angles = math_iw4::vect_to_angles(core::array::from_fn(|i| forward[i] - dot * normal[i]));
        let (_, right, up) = math_iw4::angle_vectors(angles);
        let side: f32 = (0..3).map(|i| normal[i] * right[i]).sum();
        let vertical: f32 = (0..3).map(|i| normal[i] * up[i]).sum();
        angles[2] = side.atan2(vertical).to_degrees() + 90.0;
        origin[2] -= 1.0;
    } else {
        angles[0] = normal[2].atan2(normal[0].hypot(normal[1])).to_degrees()
            + host_flrand(world.combat_rng_mut(), -15.0, 15.0);
        let forward = forward(angles);
        origin = core::array::from_fn(|i| origin[i] - normal[i] * 1.5 - forward[i] * 4.5);
    }
    stick_missile(tick, projectile, origin);
    projectile.apos.tr_base = angles;
    push_grenade_stick(world, tick, projectile);
}

fn unstick_missile(tick: Tick, projectile: &mut ProjectileState) {
    let time = level_time_ms(tick);
    projectile.velocity = [0.0, 0.0, 0.0];
    projectile.pos.tr_type = TR_GRAVITY;
    projectile.pos.tr_time = time;
    projectile.pos.tr_duration = 0;
    projectile.pos.tr_base = projectile.origin;
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
    projectile.grounded |= normal[2] > 0.7;
    projectile.velocity = projectile.velocity_at(hit_time);
    let incoming = projectile.velocity;
    let facts = required_projectile_facts(world, projectile.weapon);
    bounce_velocity(projectile, normal, &facts, surf_type);
    let outgoing = projectile.velocity;
    projectile.origin = origin;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_time = time;
    projectile.pos.tr_delta = projectile.velocity;
    apply_missile_land_angles(world, tick, projectile, normal, fraction);
    let delta = vec3_length([
        outgoing[0] - incoming[0],
        outgoing[1] - incoming[1],
        outgoing[2] - incoming[2],
    ]);
    if delta > BOUNCE_EVENT_SPEED_DELTA {
        push_grenade_bounce(world, tick, projectile, surf_type);
    }
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
    let incoming = projectile.velocity;
    projectile.velocity =
        bounce_reflected_scale(incoming, normal, *coefficients.0, *coefficients.1);
}

fn bounce_reflected_scale(
    incoming: [f32; 3],
    normal: [f32; 3],
    parallel: f32,
    perpendicular: f32,
) -> [f32; 3] {
    let speed = vec3_length(incoming);
    let nlen = vec3_length(normal);
    if speed < 1e-6 || nlen < 1e-6 {
        return [0.0, 0.0, 0.0];
    }
    let n = [normal[0] / nlen, normal[1] / nlen, normal[2] / nlen];
    let d = incoming[0] * n[0] + incoming[1] * n[1] + incoming[2] * n[2];
    let reflected = [
        incoming[0] - 2.0 * d * n[0],
        incoming[1] - 2.0 * d * n[1],
        incoming[2] - 2.0 * d * n[2],
    ];
    let incidence = (-d / speed).clamp(0.0, 1.0);
    let factor = parallel + (perpendicular - parallel) * incidence;
    [
        reflected[0] * factor,
        reflected[1] * factor,
        reflected[2] * factor,
    ]
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
