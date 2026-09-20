use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, PelletId};
use crate::match_state::{
    CLASS_CATALOG_DANGER_CLOSE, CLASS_CATALOG_STOPPING_POWER, ClientLifecycle, EventAudience,
    SimEvent, class_catalog_has,
};
use crate::world::{ClientId, Tick};
use crate::world_objects::{DestructibleExplodeEvent, GlassPaneBasis, GlassPieceId};
use gamemode_iw4::{
    G_CAN_DAMAGE_CONTENTS_MASK, g_can_damage_player_vis_scale, g_radius_damage_amount,
    radius_damage_distance_to_aabb,
};

const AREA_ENTITY_CAPACITY: usize = 0x800;

const CONCUSSION_GSC_WEAPON: &str = "concussion_grenade_mp";

const CONCUSSION_GSC_RADIUS: f32 = 512.0;

const CONCUSSION_GSC_WAIT_TICKS: u32 = 1;
const _: () = assert!(crate::MATCH_TICK_MS == 50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelayedConcussion {
    pub due_tick: u32,
    pub target: ClientId,
    pub target_life: LifeSequence,
    pub duration_ms: i32,
}

pub(crate) const HOST_SHOCK_FLASHBANG_MP: i32 = hud_iw4::HOST_SHOCK_FLASHBANG_MP;

pub(crate) const HOST_SHOCK_CONCUSSION_GRENADE_MP: i32 = hud_iw4::HOST_SHOCK_CONCUSSION_GRENADE_MP;

#[must_use]
pub(crate) fn shellshock_dump_affects_movement(shellshock_index: i32) -> bool {
    shellshock_index == HOST_SHOCK_CONCUSSION_GRENADE_MP
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageAttempt {
    pub source: crate::DamageSource,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub target: ClientId,
    pub target_life: LifeSequence,
    pub weapon: u32,
    pub amount: i32,

    pub killcam_entity_start_time: i32,

    pub inflictor_origin: Option<[f32; 3]>,

    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageRefusal {
    FriendlyFire,
    NonPositive,
    MissingTarget,
    TargetNotAlive,
    StaleLife,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathCommit {
    pub victim: ClientId,
    pub victim_life: LifeSequence,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub source: crate::DamageSource,
    pub pellet: PelletId,
    pub weapon: u32,
    pub amount: i32,
    pub killcam_entity_start_time: i32,
    pub inflictor_origin: Option<[f32; 3]>,
    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageOutcome {
    Refused(DamageRefusal),
    Nonlethal { health_after: i32 },
    Died(DeathCommit),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExplosionBlast {
    pub origin: [f32; 3],
    pub radius: f32,
    pub inner_damage: f32,
    pub outer_damage: f32,
    pub weapon: u32,
    pub source: DamageSource,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub killcam_entity_start_time: i32,
}

impl ExplosionBlast {
    pub(crate) fn from_destructible(explode: &DestructibleExplodeEvent) -> Self {
        Self {
            origin: explode.origin,
            radius: explode.explode_range_mp as f32,
            inner_damage: explode.explode_damage.1 as f32,
            outer_damage: explode.explode_damage.0 as f32,
            weapon: 0,
            source: explode.source,
            attacker: explode.attacker,
            attacker_life: explode.attacker_life,
            killcam_entity_start_time: 0,
        }
    }
}

struct GlassBlastHit {
    id: GlassPieceId,
    amount: u32,
    hit: [f32; 3],
    dir: [f32; 3],
}

pub(crate) fn apply_explosion_blast(world: &mut FrameWorld, tick: Tick, blast: &ExplosionBlast) {
    if !world.publishes_snapshot() {
        return;
    }
    let attempts = radius_player_attempts(world, blast);
    let glass = radius_glass_hits(world, blast);
    for attempt in &attempts {
        let _ = apply_damage_attempt(world, tick, attempt);
    }
    apply_glass_blast_hits(world, tick, glass);
}

pub(crate) fn apply_explosion_destructibles(
    world: &mut FrameWorld,
    blast: &ExplosionBlast,
    skip_owner: Option<crate::ScriptModelId>,
) -> Vec<DestructibleExplodeEvent> {
    if !world.publishes_snapshot() {
        return Vec::new();
    }
    let explode = DestructibleExplodeEvent {
        owner: skip_owner.unwrap_or(crate::ScriptModelId::from_wire(u32::MAX)),
        origin: blast.origin,
        attacker: blast.attacker,
        attacker_life: blast.attacker_life,
        source: blast.source,
        explode_range_mp: blast.radius.max(0.0) as u32,
        explode_damage: (
            blast.outer_damage.max(0.0) as u32,
            blast.inner_damage.max(0.0) as u32,
        ),
    };
    let intents = world
        .world_objects()
        .destructible_radius_intents(&[explode]);
    world
        .world_objects_mut()
        .apply_destructible_damage_batch(&intents)
        .explodes
}

fn radius_player_attempts(world: &FrameWorld, blast: &ExplosionBlast) -> Vec<DamageAttempt> {
    let mut intents = Vec::new();
    if blast.radius <= 0.0 {
        return intents;
    }
    for target in radius_player_candidates(world, blast.origin, blast.radius) {
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        let bounds = world.player_area_bounds(target).unwrap_or_else(|| {
            panic!("CM_AreaEntities returned a player without linked absolute Bounds");
        });
        let dist = radius_damage_distance_to_aabb(blast.origin, bounds.mid(), bounds.half());
        let vis_scale = player_radius_vis_scale(world, blast.origin, target);
        let amount = g_radius_damage_amount(
            blast.inner_damage,
            blast.outer_damage,
            blast.radius,
            dist,
            vis_scale,
        );
        if amount <= 0 {
            continue;
        }
        intents.push(DamageAttempt {
            source: blast.source,
            pellet: PelletId(0),
            attacker: blast.attacker,
            attacker_life: blast.attacker_life,
            target,
            target_life: meta.life_sequence,
            weapon: blast.weapon,
            amount,
            killcam_entity_start_time: blast.killcam_entity_start_time,
            inflictor_origin: Some(blast.origin),
            hitloc: 0,
        });
    }
    intents
}

fn radius_glass_hits(world: &FrameWorld, blast: &ExplosionBlast) -> Vec<GlassBlastHit> {
    let mut hits = Vec::new();
    if blast.radius <= 0.0 {
        return hits;
    }
    for (id, pane) in world.world_objects().glass_radius_targets() {
        let (mid, half) = glass_pane_aabb(pane);
        let dist = radius_damage_distance_to_aabb(blast.origin, mid, half);
        let amount = g_radius_damage_amount(
            blast.inner_damage,
            blast.outer_damage,
            blast.radius,
            dist,
            1.0,
        );
        if amount <= 0 {
            continue;
        }
        let dir = [
            mid[0] - blast.origin[0],
            mid[1] - blast.origin[1],
            mid[2] - blast.origin[2],
        ];
        hits.push(GlassBlastHit {
            id,
            amount: amount as u32,
            hit: mid,
            dir,
        });
    }
    hits
}

fn apply_glass_blast_hits(world: &mut FrameWorld, tick: Tick, hits: Vec<GlassBlastHit>) {
    if hits.is_empty() {
        return;
    }
    let at_time_ms = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
    let mut holdrand = *world.stuck_holdrand_mut();
    for hit in hits {
        world.world_objects_mut().apply_glass_hit(
            hit.id,
            hit.amount,
            at_time_ms,
            hit.hit,
            hit.dir,
            &mut || crate::item::g_random(&mut holdrand),
        );
    }
    *world.stuck_holdrand_mut() = holdrand;
}

fn glass_pane_aabb(pane: GlassPaneBasis) -> ([f32; 3], [f32; 3]) {
    let mut min = pane.origin;
    let mut max = pane.origin;
    for corner in [
        [
            pane.origin[0] + pane.axis_s[0],
            pane.origin[1] + pane.axis_s[1],
            pane.origin[2] + pane.axis_s[2],
        ],
        [
            pane.origin[0] + pane.axis_t[0],
            pane.origin[1] + pane.axis_t[1],
            pane.origin[2] + pane.axis_t[2],
        ],
        [
            pane.origin[0] + pane.axis_s[0] + pane.axis_t[0],
            pane.origin[1] + pane.axis_s[1] + pane.axis_t[1],
            pane.origin[2] + pane.axis_s[2] + pane.axis_t[2],
        ],
    ] {
        for i in 0..3 {
            min[i] = min[i].min(corner[i]);
            max[i] = max[i].max(corner[i]);
        }
    }
    let mid = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = [
        (max[0] - min[0]) * 0.5,
        (max[1] - min[1]) * 0.5,
        (max[2] - min[2]) * 0.5,
    ];
    (mid, half)
}

pub(crate) fn radius_player_candidates(
    world: &FrameWorld,
    origin: [f32; 3],
    radius: f32,
) -> Vec<ClientId> {
    let area_half = gamemode_iw4::g_radius_damage_area_half_extent(radius);
    let query =
        clipmap_iw4::AreaBounds::from_mid_half(origin, [area_half; 3]).unwrap_or_else(|_| {
            panic!("G_RadiusDamage query Bounds are invalid");
        });
    world
        .area_entity_candidates(query, u32::MAX, AREA_ENTITY_CAPACITY)
        .into_iter()
        .map(|entity_num| ClientId(u32::from(entity_num)))
        .collect()
}

pub(crate) fn apply_shared_glass_blast(
    world: &mut FrameWorld,
    tick: Tick,
    origin: [f32; 3],
    inner_damage: i32,
    outer_damage: i32,
    radius: f32,
) {
    let time = crate::level_time_ms(tick);
    let mut holdrand = *world.stuck_holdrand_mut();
    world.world_objects_mut().apply_glass_blast(
        origin,
        inner_damage,
        outer_damage,
        radius,
        time,
        &mut || crate::item::g_random(&mut holdrand),
    );
    *world.stuck_holdrand_mut() = holdrand;
}

pub(crate) fn apply_explode_glass_blast(
    world: &mut FrameWorld,
    tick: Tick,
    explode: &DestructibleExplodeEvent,
) {
    apply_shared_glass_blast(
        world,
        tick,
        explode.origin,
        explode.explode_damage.1 as i32,
        explode.explode_damage.0 as i32,
        explode.explode_range_mp as f32,
    );
}

fn player_radius_vis_scale(world: &FrameWorld, inflictor: [f32; 3], target: ClientId) -> f32 {
    let Some(ps) = world.player(target) else {
        return 1.0;
    };
    let (_, right, _) = math_iw4::angle_vectors(ps.viewangles);
    g_can_damage_player_vis_scale(
        ps.origin,
        ps.view_height_current,
        right,
        inflictor,
        |start, end| {
            let trace =
                world.trace_world(start, end, [0.0; 3], [0.0; 3], G_CAN_DAMAGE_CONTENTS_MASK);
            t_trace_passed(&trace)
        },
    )
}

fn t_trace_passed(trace: &trace_iw4::Trace) -> bool {
    trace.fraction >= 1.0 && trace.startsolid == 0
}

pub(crate) fn apply_flashbang_blast(
    world: &mut FrameWorld,
    tick: Tick,
    origin: [f32; 3],
    radius_max: f32,
    radius_min: f32,
) {
    let min_r = radius_min.max(1.0);
    let max_r = if radius_max < min_r {
        min_r
    } else {
        radius_max
    };
    let time_ms = crate::level_time_ms(tick);
    let mut hits = Vec::new();
    for target in radius_player_candidates(world, origin, max_r) {
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let Some(ps) = world.player(target) else {
            continue;
        };
        let dist = {
            let dx = ps.origin[0] - origin[0];
            let dy = ps.origin[1] - origin[1];
            let dz = ps.origin[2] - origin[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        if dist > max_r {
            continue;
        }
        if player_radius_vis_scale(world, origin, target) <= 0.0 {
            continue;
        }
        let amount_distance = flashbang_amount_distance(dist, min_r, max_r);
        let view_origin = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ];
        let (forward, _, _) = math_iw4::angle_vectors(ps.viewangles);
        let to_blast = [
            origin[0] - view_origin[0],
            origin[1] - view_origin[1],
            origin[2] - view_origin[2],
        ];
        let len =
            (to_blast[0] * to_blast[0] + to_blast[1] * to_blast[1] + to_blast[2] * to_blast[2])
                .sqrt();
        let amount_angle = if len > 0.0 {
            flashbang_amount_angle(
                (to_blast[0] * forward[0] + to_blast[1] * forward[1] + to_blast[2] * forward[2])
                    / len,
            )
        } else {
            flashbang_amount_angle(1.0)
        };
        let Some(duration_ms) = flashbang_gsc_duration_ms(amount_distance, amount_angle) else {
            continue;
        };
        hits.push((target, duration_ms));
    }
    for (target, duration_ms) in hits {
        let Some(ps) = world.player_mut(target) else {
            continue;
        };
        let duration_ms = if ps.shellshock_time == time_ms {
            duration_ms.max(ps.shellshock_duration)
        } else {
            duration_ms
        };
        ps.shellshock_index = HOST_SHOCK_FLASHBANG_MP;
        ps.shellshock_time = time_ms;
        ps.shellshock_duration = duration_ms;
        ps.pm_flags |= playerstate_iw4::pm_flags::SHELLSHOCKED;
    }
}

pub(crate) fn apply_concussion_leftover(
    world: &mut FrameWorld,
    tick: Tick,
    intent: &DamageAttempt,
) {
    if world.weapon_script_name(intent.weapon) != CONCUSSION_GSC_WEAPON {
        return;
    }
    let Some(origin) = intent.inflictor_origin else {
        return;
    };
    let Some(ps) = world.player(intent.target) else {
        return;
    };
    let dist = {
        let dx = ps.origin[0] - origin[0];
        let dy = ps.origin[1] - origin[1];
        let dz = ps.origin[2] - origin[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    };
    let duration_ms = concussion_gsc_duration_ms(dist);
    world.pending_concussion_mut().push(DelayedConcussion {
        due_tick: tick.0.saturating_add(CONCUSSION_GSC_WAIT_TICKS),
        target: intent.target,
        target_life: intent.target_life,
        duration_ms,
    });
}

pub(crate) fn tick_delayed_concussion(world: &mut FrameWorld, tick: Tick) {
    let due: Vec<DelayedConcussion> = {
        let pending = world.pending_concussion_mut();
        let mut due = Vec::new();
        pending.retain(|row| {
            if row.due_tick <= tick.0 {
                due.push(*row);
                false
            } else {
                true
            }
        });
        due
    };
    let time_ms = crate::level_time_ms(tick);
    for row in due {
        let Some(meta) = world.client_meta(row.target) else {
            continue;
        };
        if meta.lifecycle == ClientLifecycle::Dead {
            continue;
        }
        if meta.life_sequence != row.target_life {
            continue;
        }
        let Some(ps) = world.player_mut(row.target) else {
            continue;
        };
        ps.shellshock_index = HOST_SHOCK_CONCUSSION_GRENADE_MP;
        ps.shellshock_time = time_ms;
        ps.shellshock_duration = row.duration_ms;
        ps.pm_flags |= playerstate_iw4::pm_flags::SHELLSHOCKED;
    }
}

fn concussion_gsc_duration_ms(dist: f32) -> i32 {
    let scale = (1.0 - dist / CONCUSSION_GSC_RADIUS).max(0.0);
    ((2.0 + 4.0 * scale) * 1000.0).round() as i32
}

fn flashbang_amount_distance(dist: f32, min_r: f32, max_r: f32) -> f32 {
    if dist <= min_r || max_r <= min_r {
        1.0
    } else {
        1.0 - (dist - min_r) / (max_r - min_r)
    }
}

fn flashbang_amount_angle(dot: f32) -> f32 {
    (dot + 1.0) * 0.5
}

fn flashbang_gsc_duration_ms(amount_distance: f32, amount_angle: f32) -> Option<i32> {
    let angle = if amount_angle < 0.5 {
        0.5
    } else if amount_angle > 0.8 {
        1.0
    } else {
        amount_angle
    };
    let duration_s = amount_distance * angle * 6.0;
    if duration_s < 0.25 {
        None
    } else {
        Some((duration_s * 1000.0).round() as i32)
    }
}

fn flinch_damage_dir(world: &FrameWorld, intent: &DamageAttempt) -> Option<[f32; 3]> {
    let victim = world.player(intent.target)?.origin;
    let from = intent.inflictor_origin.or_else(|| {
        matches!(intent.source, crate::DamageSource::Shot(_))
            .then(|| world.player(intent.attacker).map(|ps| ps.origin))
            .flatten()
    })?;
    Some([
        victim[0] - from[0],
        victim[1] - from[1],
        victim[2] - from[2],
    ])
}

pub(crate) fn apply_damage_attempt(
    world: &mut FrameWorld,
    tick: Tick,
    intent: &DamageAttempt,
) -> DamageOutcome {
    if intent.amount <= 0 {
        return DamageOutcome::Refused(DamageRefusal::NonPositive);
    }
    let Some(meta) = world.client_meta(intent.target) else {
        return DamageOutcome::Refused(DamageRefusal::MissingTarget);
    };
    if meta.lifecycle != ClientLifecycle::Alive {
        return DamageOutcome::Refused(DamageRefusal::TargetNotAlive);
    }

    if world.bootstrap_ref().kind.is_team()
        && intent.attacker != intent.target
        && world.client_meta(intent.attacker).is_some_and(|attacker| {
            attacker.client_state_team != entity_iw4::TEAM_FREE
                && attacker.client_state_team == meta.client_state_team
        })
    {
        return DamageOutcome::Refused(DamageRefusal::FriendlyFire);
    }
    if meta.life_sequence != intent.target_life {
        return DamageOutcome::Refused(DamageRefusal::StaleLife);
    }

    let mut incoming = intent.amount;
    if !matches!(intent.source, crate::DamageSource::Melee) {
        let scale = world
            .combat_facts_for(intent.weapon)
            .map(|facts| facts.location_scale(intent.hitloc))
            .unwrap_or(1.0);
        incoming = (incoming as f32 * scale) as i32;
        if incoming <= 0 {
            return DamageOutcome::Refused(DamageRefusal::NonPositive);
        }
    }

    let attacker_perks = world
        .client_meta(intent.attacker)
        .and_then(|m| m.loadout.as_ref())
        .map(|loadout| loadout.perks)
        .unwrap_or([0; 3]);
    let inherits_perks = world
        .combat_facts_for(intent.weapon)
        .map(|facts| facts.inherits_perks)
        .unwrap_or(false);
    let means = match intent.source {
        crate::DamageSource::Shot(_) => gamemode_iw4::CacDamageMeans::Primary,
        crate::DamageSource::Projectile(_) | crate::DamageSource::Radius(_) => {
            gamemode_iw4::CacDamageMeans::Explosive
        }
        crate::DamageSource::Melee => gamemode_iw4::CacDamageMeans::Other,
    };
    let amount = gamemode_iw4::cac_modified_damage(
        incoming,
        means,
        inherits_perks,
        class_catalog_has(attacker_perks, CLASS_CATALOG_STOPPING_POWER),
        class_catalog_has(attacker_perks, CLASS_CATALOG_DANGER_CLOSE),
        {
            let now = crate::hudelem::hud_level_time_ms(tick);
            world
                .client_meta(intent.target)
                .is_some_and(|m| gamemode_iw4::combathigh_is_active(m.combathigh_until_ms, now))
        },
        gamemode_iw4::cac_weapon_is_throwingknife(world.weapon_script_name(intent.weapon)),
    );
    if amount <= 0 {
        return DamageOutcome::Refused(DamageRefusal::NonPositive);
    }

    let damage_dir = flinch_damage_dir(world, intent);
    let health_after = {
        let Some(ps) = world.player_mut(intent.target) else {
            return DamageOutcome::Refused(DamageRefusal::MissingTarget);
        };
        movement_iw4::pm_update_damage_timer(ps, amount, damage_dir);
        ps.health = (ps.health - amount).max(0);
        ps.damage_count = ps.damage_count.saturating_add(1);
        ps.damage_event = ps.damage_event.wrapping_add(1);
        ps.health
    };

    if intent.attacker != intent.target {
        world.record_damage_feedback(
            intent.attacker,
            intent.target,
            gamemode_iw4::TypeHit::Standard,
            amount,
            crate::hudelem::hud_level_time_ms(tick),
        );
    }

    if health_after > 0 {
        apply_concussion_leftover(world, tick, intent);
        return DamageOutcome::Nonlethal { health_after };
    }

    let now = crate::hudelem::hud_level_time_ms(tick);
    let already_down = world
        .client_meta(intent.target)
        .is_some_and(|m| m.laststand_until_ms.is_some());
    let armed = world
        .client_meta(intent.target)
        .is_some_and(|m| m.pistoldeath_this_life);
    let knife = gamemode_iw4::cac_weapon_is_throwingknife(world.weapon_script_name(intent.weapon));
    if !already_down
        && armed
        && gamemode_iw4::may_do_laststand(
            means,
            hud_iw4::obituary_is_headshot(intent.hitloc),
            knife,
        )
    {
        if let Some(ps) = world.player_mut(intent.target) {
            ps.health = 1;
            ps.pm_type = playerstate_iw4::PM_TYPE_LAST_STAND;
            ps.view_height_target = movement_iw4::view_height::LAST_STAND;
            ps.pm_flags |= playerstate_iw4::pm_flags::LAST_STAND;
        }
        world.client_meta_mut(intent.target).laststand_until_ms =
            Some(now.saturating_add(gamemode_iw4::FINALSTAND_DURATION_MS));
        apply_concussion_leftover(world, tick, intent);
        return DamageOutcome::Nonlethal { health_after: 1 };
    }

    let commit = DeathCommit {
        victim: intent.target,
        victim_life: intent.target_life,
        attacker: intent.attacker,
        attacker_life: intent.attacker_life,
        source: intent.source,
        pellet: intent.pellet,
        weapon: intent.weapon,
        amount,
        killcam_entity_start_time: intent.killcam_entity_start_time,
        inflictor_origin: intent.inflictor_origin,
        hitloc: intent.hitloc,
    };
    {
        let meta = world.client_meta_mut(intent.target);
        meta.lifecycle = ClientLifecycle::Dead;
        meta.dead_since_tick = Some(tick.0);
    }
    world.unlink_player_area(intent.target);
    world.push_event(
        tick,
        EventAudience::All,
        SimEvent::Died {
            victim: commit.victim,
            life_sequence: commit.victim_life,
            attacker: Some(commit.attacker),
            attacker_life: Some(commit.attacker_life),
            source: Some(commit.source),
            weapon: commit.weapon,
            killcam_entity_start_time: commit.killcam_entity_start_time,
        },
    );
    let event_parm = pack_obituary_parm(world, &commit);
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::OBITUARY,
        crate::EntityEventPayload {
            number: commit.victim.0 as i32,
            other_entity_num: commit.victim.0 as i32,
            attacker_entity_num: commit.attacker.0 as i32,
            event_parm,
            weapon: commit.weapon,
            ..Default::default()
        },
    );
    crate::score::apply_death_score(world, tick, commit.victim, Some(commit.attacker));
    apply_player_killed(
        world,
        tick,
        commit.victim,
        Some(commit.attacker),
        Some(&commit),
    );
    DamageOutcome::Died(commit)
}

fn pack_obituary_parm(world: &FrameWorld, commit: &DeathCommit) -> i32 {
    let (weap_type, weap_class) = world
        .combat_facts_for(commit.weapon)
        .map(|f| (f.weap_type, f.weap_class))
        .unwrap_or((0, 0));
    let means = match commit.source {
        crate::DamageSource::Shot(_) if hud_iw4::obituary_is_headshot(commit.hitloc) => {
            hud_iw4::MOD_HEAD_SHOT
        }
        crate::DamageSource::Melee => hud_iw4::MOD_MELEE,
        _ => 0,
    };
    hud_iw4::pack_obituary_event_parm(commit.weapon, means, weap_type, weap_class)
}

pub(crate) fn push_suicide_obituary(world: &mut FrameWorld, tick: Tick, victim: ClientId) {
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::OBITUARY,
        crate::EntityEventPayload {
            number: victim.0 as i32,
            other_entity_num: victim.0 as i32,
            attacker_entity_num: victim.0 as i32,
            event_parm: hud_iw4::pack_obituary_event_parm(0, hud_iw4::MOD_SUICIDE, 0, 0),
            weapon: 0,
            ..Default::default()
        },
    );
}

pub(crate) fn apply_player_killed(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
    commit: Option<&DeathCommit>,
) {
    let Some(self_ps) = world.player(victim).copied() else {
        return;
    };
    let attacker_origin = attacker
        .filter(|&id| id != victim)
        .and_then(|id| world.player(id).map(|p| p.origin));
    let yaw =
        playerstate_iw4::look_at_killer_yaw(attacker_origin, self_ps.origin, self_ps.viewangles[1]);

    let dead_viewangles = [0.0, self_ps.viewangles[1], 0.0];

    let inflictor_origin = commit.and_then(|death| death.inflictor_origin);
    let hitscan_origin = match commit.map(|death| death.source) {
        Some(crate::DamageSource::Shot(_)) | Some(crate::DamageSource::Melee) => attacker_origin,
        _ => None,
    };
    let mut conds = crate::AnimConditions::default();

    let mt = world
        .last_anim_movetype(victim)
        .unwrap_or(anim_iw4::ANIM_MT_IDLE);
    conds.set_bit(anim_iw4::ANIM_COND_MOVETYPE, mt);
    if let Some(commit) = commit {
        let blast_origin = inflictor_origin.or(hitscan_origin);
        let dist_sq = blast_origin.map(|origin| {
            let dx = origin[0] - self_ps.origin[0];
            let dy = origin[1] - self_ps.origin[1];
            let dz = origin[2] - self_ps.origin[2];
            dx * dx + dy * dy + dz * dz
        });
        let weap_class = world.combat_facts_for(commit.weapon).map(|f| f.weap_class);
        let offhand_class = world
            .equipment_facts_for(commit.weapon)
            .map(|f| f.offhand_class);
        conds.set_value(
            anim_iw4::ANIM_COND_DAMAGETYPE,
            anim_script_damagetype(commit.source, weap_class, offhand_class, dist_sq),
        );
        conds.set_value(
            anim_iw4::ANIM_COND_HITLOCATION,
            u32::from(anim_iw4::anim_script_hit_location(commit.hitloc)),
        );

        let hitdir = match blast_origin {
            Some(origin) => {
                let v_dir = [self_ps.origin[0] - origin[0], self_ps.origin[1] - origin[1]];
                let (forward, _, _) = math_iw4::angle_vectors(self_ps.viewangles);
                anim_iw4::anim_script_hit_direction([forward[0], forward[1]], v_dir)
            }
            None => 0,
        };
        conds.set_value(anim_iw4::ANIM_COND_HITDIRECTION, u32::from(hitdir));
    }

    let script = world.player_anim_script();
    let mut seed = world.anim_event_seed();
    if let Some(script) = script.as_ref() {
        if let Some(ps) = world.player_mut(victim) {
            ps.viewangles = dead_viewangles;
            ps.pm_type = playerstate_iw4::PM_TYPE_DEAD;
            let _ = script.apply_event(ps, anim_iw4::ANIM_ET_DEATH, &conds, &mut seed);
        }
    } else if let Some(ps) = world.player_mut(victim) {
        ps.viewangles = dead_viewangles;
        ps.pm_type = playerstate_iw4::PM_TYPE_DEAD;
    }
    world.set_anim_event_seed(seed);
    crate::item::try_drop_scavenger_for_death(world, tick, victim, attacker);
    crate::item::try_drop_weapon_for_death(world, tick, victim);

    let Some(self_ps) = world.player(victim).copied() else {
        return;
    };
    let slot = {
        let time_ms = crate::corpse::level_time_ms(tick);
        crate::corpse::occupy_player_clone(world, victim, &self_ps, time_ms)
    };
    if let Some(ps) = world.player_mut(victim) {
        ps.corpse_index = i32::from(slot);
        ps.e_flags |= 0x20000;
        ps.health = 0;
    }
    world.client_meta_mut(victim).look_at_killer_yaw = yaw;
    crate::voice::play_death_sound(world, tick, victim);
    if let Some(attacker) = attacker {
        crate::voice::schedule_killfirm(world, tick, attacker, victim);
    }
    queue_death_player_cards(world, victim, attacker);
}

fn queue_death_player_cards(world: &mut FrameWorld, victim: ClientId, attacker: Option<ClientId>) {
    match attacker {
        Some(att) if att != victim => {
            world.push_player_card_slot(att, victim, hud_iw4::PLAYER_CARD_SLOT_YOUKILLED);
            world.push_player_card_open(att, hud_iw4::SCRIPT_MENU_YOUKILLED_DISPLAY);
            world.push_player_card_slot(victim, att, hud_iw4::PLAYER_CARD_SLOT_KILLEDBY);
            world.push_player_card_open(victim, hud_iw4::SCRIPT_MENU_KILLEDBY_DISPLAY);
            world.push_player_card_open(victim, hud_iw4::SCRIPT_MENU_PERK_HIDE);
        }
        _ => {
            world.push_player_card_slot(victim, victim, hud_iw4::PLAYER_CARD_SLOT_KILLEDBY);
            world.push_player_card_open(victim, hud_iw4::SCRIPT_MENU_KILLEDBY_DISPLAY);
            world.push_player_card_open(victim, hud_iw4::SCRIPT_MENU_PERK_HIDE);
        }
    }
}

fn anim_script_damagetype(
    source: crate::DamageSource,
    weap_class: Option<i32>,
    offhand_class: Option<i32>,
    dist_sq: Option<f32>,
) -> u32 {
    match source {
        crate::DamageSource::Projectile(_) | crate::DamageSource::Radius(_) => {
            if offhand_class == Some(1) {
                if dist_sq.is_some_and(|d| d < anim_iw4::ANIM_DAMAGE_EXPLOSION_NEAR_DIST_SQ) {
                    2
                } else {
                    1
                }
            } else {
                2
            }
        }
        crate::DamageSource::Shot(_) => {
            if weap_class == Some(4) {
                1
            } else {
                0
            }
        }
        crate::DamageSource::Melee => 0,
    }
}
