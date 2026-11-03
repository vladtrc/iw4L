use crate::bullet::bullet_damage_at_distance;
use crate::bullet_collision::{
    BulletTraceQuery, ColliderId, EntityCollisionEpoch, EntityCollisionTraceGeom,
    HistorySampleVerdict, MASK_BULLET_WORLD, bullet_trace_segments_filtered, glass_piece_from_hit,
};
use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, MatchRng, PelletId, ShotId};
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::world::{ClientId, Tick};
use crate::world_objects::glass_piece_is_solid;
use anim_iw4::ANIM_ET_RAISEWEAPON;
use entity_iw4::glass_add_damage;
use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};
use std::cell::RefCell;
use std::collections::HashMap;
use weapon_iw4::{
    FireWeaponKind, MELEE_TRACE_OFFSETS, MeleeTrace, PLAYER_MELEE_HEIGHT_DEFAULT,
    PLAYER_MELEE_RANGE_DEFAULT, PLAYER_MELEE_WIDTH_DEFAULT, fire_weapon_kind,
};

type HitDumpOutput = (
    Option<[f32; 3]>,
    Option<[f32; 3]>,
    Option<u32>,
    Option<String>,
);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedShot {
    pub shot_id: ShotId,
    pub fire_cause: Option<crate::FireCause>,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,

    pub hand: u8,
    pub weapon: u32,
    pub ammo_used: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub ads_frac: f32,

    pub view_height_current: f32,

    pub aim_spread_scale: f32,

    pub perks0: u32,
    pub combat_seed: u32,
    pub owner_velocity: [f32; 3],
    pub spread_degrees: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emission {
    pub combat_seed: u32,
    pub shot_id: ShotId,
    pub fire_cause: Option<crate::FireCause>,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub hand: u8,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub max_range: f32,
    pub base_damage: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCollisionRepresentation {
    StandingAabbV1,
    PosedBonesV1,
}

impl PlayerCollisionRepresentation {
    pub const fn dump_label(self) -> &'static str {
        match self {
            Self::StandingAabbV1 => "aabb",
            Self::PosedBonesV1 => "bones",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityClipKind {
    CollTris,
    BoneBoxes,
    LinkedBrush,
}

impl EntityClipKind {
    pub const fn dump_label(self) -> &'static str {
        match self {
            Self::CollTris => "colltris",
            Self::BoneBoxes => "boxes",
            Self::LinkedBrush => "brush",
        }
    }
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

    pub entity_clip: Option<EntityClipKind>,

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
    command_sequence: Option<crate::CommandSequence>,
) -> Vec<AcceptedShot> {
    let mut accepted = Vec::new();
    if !world
        .client_meta(id)
        .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
    {
        return accepted;
    }
    let Some(mut state) = world.player(id).copied() else {
        return accepted;
    };
    let old_buttons = world.old_buttons(id).unwrap_or(0);
    let old_angles = world.old_cmd_angles(id).unwrap_or(cmd.angles);
    let meta = world.client_meta(id).expect("alive metadata");
    let life = meta.life_sequence;
    let can_fire = world
        .missile_launch_facts(state.weapon)
        .is_none_or(|facts| {
            !facts.require_lock_to_fire || meta.weapon_lock.can_fire(state.weapon, life.0)
        });
    let content = world.content();
    let script = world.player_anim_script();
    let animation = crate::bg::weapon_animation::AnimationInputs {
        script: script.as_deref(),
        last_movetype: world.last_anim_movetype(id),
        strafing: world.last_anim_strafing(id),
        buttons: world.anim_command_buttons(id),
    };
    let mut anim_seed = world.anim_event_seed();
    let inputs = weapon_iw4::FireInputs {
        cmd,
        root_seed: world.root_seed(),
        life,
        client: id,
        command: command_sequence,
        mantle_weapon_inactive: movement_iw4::mantle::is_weapon_inactive(&state, true),
        old_buttons,
        old_angles,
        msec,
        can_fire,
        linked_origin: None,
    };
    let mut animator = crate::bg::weapon_animation::PlayerWeaponAnimation {
        content: &content,
        inputs: animation,
        seed: &mut anim_seed,
    };
    let advanced = weapon_iw4::pm_weapon(
        &mut state,
        &mut world.client_meta_mut(id).weapon_carry,
        content.as_ref(),
        &inputs,
        &mut animator,
    );
    *world.player_mut(id).expect("present player") = state;
    world.set_anim_event_seed(anim_seed);
    let Some(step) = advanced else {
        return accepted;
    };
    for entry in step.events.into_iter().flatten() {
        let shot_id =
            matches!(entry.1, weapon_iw4::WeaponEvent::Fire { .. }).then(|| world.alloc_shot_id());
        let mut state = *world.player(id).expect("weapon event retains its player");
        let mut event_inputs = inputs;
        event_inputs.linked_origin = world
            .client_meta(id)
            .and_then(|meta| meta.linked_weapon_view)
            .map(|view| view.origin);
        let animation = crate::bg::weapon_animation::AnimationInputs {
            script: script.as_deref(),
            last_movetype: world.last_anim_movetype(id),
            strafing: world.last_anim_strafing(id),
            buttons: world.anim_command_buttons(id),
        };
        let mut anim_seed = world.anim_event_seed();
        let mut animator = crate::bg::weapon_animation::PlayerWeaponAnimation {
            content: &content,
            inputs: animation,
            seed: &mut anim_seed,
        };
        let action = step.apply_event(
            weapon_iw4::WeaponEventInstance {
                hand: entry.0,
                event: entry.1,
                shot: shot_id,
            },
            &mut state,
            &mut world.client_meta_mut(id).weapon_carry,
            &event_inputs,
            content.as_ref(),
            &mut animator,
        );
        *world
            .player_mut(id)
            .expect("weapon event retains its player") = state;
        world.set_anim_event_seed(anim_seed);
        match action {
            weapon_iw4::WeaponAction::None => {}
            weapon_iw4::WeaponAction::Detonate { weapon } => {
                world
                    .weapon_notes
                    .push(crate::equipment::WeaponNote::DetonationRequested { owner: id, weapon });
            }
            weapon_iw4::WeaponAction::Brass {
                weapon,
                hand,
                pending,
                origin,
            } => {
                let kind = if hand == 1 {
                    entity_iw4::EntityEventKind::EJECT_BRASS_LEFT
                } else {
                    entity_iw4::EntityEventKind::EJECT_BRASS
                };
                world.push_entity_event(
                    tick,
                    EventAudience::All,
                    kind,
                    crate::EntityEventPayload {
                        number: id.0 as i32,
                        weapon,
                        hand,
                        origin,
                        correlation: pending.map_or(0, |pending| pending.shot.0),
                        fire_cause: pending.and_then(|pending| pending.cause),
                        ..Default::default()
                    },
                );
            }
            weapon_iw4::WeaponAction::Fire(fire) => {
                let combat_seed = fire
                    .combat_seed
                    .unwrap_or_else(|| world.combat_rng_mut().next_u32());
                world.push_entity_event(
                    tick,
                    EventAudience::All,
                    entity_iw4::predicted_weapon_fire_event(fire.hand as i32, fire.last_shot),
                    crate::EntityEventPayload {
                        number: id.0 as i32,
                        weapon: fire.weapon,
                        correlation: fire.shot_id.0,
                        fire_cause: fire.cause,
                        hand: fire.hand,
                        origin: fire.origin,
                        direction: fire.angles,
                        ..Default::default()
                    },
                );
                world
                    .weapon_notes
                    .push(crate::equipment::WeaponNote::Fired { owner: id });
                accepted.push(AcceptedShot {
                    shot_id: fire.shot_id,
                    fire_cause: fire.cause,
                    attacker: id,
                    attacker_life: life,
                    hand: fire.hand,
                    weapon: fire.weapon,
                    ammo_used: fire.ammo_used,
                    origin: fire.origin,
                    angles: fire.angles,
                    ads_frac: fire.ads_frac,
                    view_height_current: fire.view_height,
                    aim_spread_scale: fire.aim_spread_scale,
                    perks0: fire.perks0,
                    combat_seed,
                    owner_velocity: fire.velocity,
                    spread_degrees: fire.spread_degrees,
                });
            }
            weapon_iw4::WeaponAction::Pullback { weapon } => {
                world
                    .weapon_notes
                    .push(crate::equipment::WeaponNote::Pullback { owner: id, weapon });
            }
            weapon_iw4::WeaponAction::OffhandUsed {
                weapon,
                remaining_fuse_ms,
                cause,
                combat_seed,
                origin,
                angles,
                velocity,
            } => {
                let combat_seed = combat_seed.unwrap_or_else(|| world.combat_rng_mut().next_u32());
                if crate::equipment::spawn_grenade_projectile(
                    world,
                    id,
                    weapon,
                    crate::equipment::ProjectileLaunch {
                        tick,
                        origin,
                        angles,
                        owner_velocity: velocity,
                        combat_seed,
                    },
                    crate::equipment::GrenadeLaunchKind::Thrown { remaining_fuse_ms },
                    cause,
                ) {
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
            }
            weapon_iw4::WeaponAction::CookedOff { weapon } => {
                crate::equipment::explode_offhand_in_hand(world, id, weapon, tick);
            }
            weapon_iw4::WeaponAction::ReloadStarted => {
                world
                    .weapon_notes
                    .push(crate::equipment::WeaponNote::ReloadStarted { owner: id });
            }
            weapon_iw4::WeaponAction::Melee {
                weapon,
                origin,
                angles,
            } => {
                world.push_entity_event(
                    tick,
                    EventAudience::All,
                    entity_iw4::EntityEventKind::FIRE_MELEE,
                    crate::EntityEventPayload {
                        number: id.0 as i32,
                        weapon,
                        origin,
                        direction: angles,
                        ..Default::default()
                    },
                );
                fire_weapon_melee(world, tick, id, life, weapon, origin, angles);
            }
        }
    }
    step.finish_events(&mut world.client_meta_mut(id).weapon_carry, life);
    accepted
}

pub(crate) fn apply_player_anim_event(world: &mut FrameWorld, id: ClientId, event: u8) {
    apply_player_anim_event_with_target(world, id, event, None);
}

fn apply_player_anim_event_with_target(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    next_weapon: Option<u32>,
) {
    apply_player_anim_event_inner(
        world,
        id,
        event,
        next_weapon,
        !matches!(event, ANIM_ET_RAISEWEAPON | 11..=16),
    );
}

pub(crate) fn apply_player_anim_event_forced(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    force: bool,
) {
    apply_player_anim_event_inner(world, id, event, None, force);
}

fn apply_player_anim_event_inner(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    next_weapon: Option<u32>,
    force: bool,
) {
    let Some(script) = world.player_anim_script() else {
        return;
    };
    let content = world.content();
    let animation = crate::bg::weapon_animation::AnimationInputs {
        script: Some(script.as_ref()),
        last_movetype: world.last_anim_movetype(id),
        strafing: world.last_anim_strafing(id),
        buttons: world.anim_command_buttons(id),
    };
    let mut seed = world.anim_event_seed();
    let Some(ps) = world.player_mut(id) else {
        return;
    };
    crate::bg::weapon_animation::apply_animation_event(
        ps,
        &content,
        &animation,
        &mut seed,
        event,
        next_weapon,
        force,
    );
    world.set_anim_event_seed(seed);
}

pub fn spread_pellet_direction(
    angles: [f32; 3],
    spread_degrees: f32,
    rng: &mut MatchRng,
) -> [f32; 3] {
    spread_direction_on_plane(angles, spread_degrees, rng, 1.0)
}

pub fn spread_direction_on_plane(
    angles: [f32; 3],
    spread_degrees: f32,
    rng: &mut MatchRng,
    plane: f32,
) -> [f32; 3] {
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    if spread_degrees <= 0.0 {
        return forward;
    }
    let unit = |draw: u32| draw as f32 / u32::MAX as f32;
    let radius = unit(rng.next_u32());
    let theta = unit(rng.next_u32()) * core::f32::consts::TAU;
    let lateral = plane * spread_degrees.to_radians().tan() * radius;
    let q = [
        plane * forward[0] + lateral * (theta.cos() * right[0] + theta.sin() * up[0]),
        plane * forward[1] + lateral * (theta.cos() * right[1] + theta.sin() * up[1]),
        plane * forward[2] + lateral * (theta.cos() * right[2] + theta.sin() * up[2]),
    ];
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if len > 0.0 {
        [q[0] / len, q[1] / len, q[2] / len]
    } else {
        forward
    }
}

fn flame_burns_this_tick(facts: &weapon_iw4::WeaponCombatFacts, now_ms: i32) -> bool {
    let interval = facts
        .flame_damage_interval_ms
        .max(facts.fire_time_ms)
        .max(1);
    let window = facts.fire_time_ms.max(1);
    now_ms.div_euclid(interval) != (now_ms - window).div_euclid(interval)
}

pub fn phase_emit(
    content: &crate::SimContent,
    tick: Tick,
    shots: &[AcceptedShot],
) -> Vec<Emission> {
    let mut out = Vec::new();
    for shot in shots {
        let Some(facts) = content.weapon_combat_facts(shot.weapon) else {
            continue;
        };

        match fire_weapon_kind(facts.weap_type, facts.weap_class) {
            Some(FireWeaponKind::Bullet) => {
                if facts.damage <= 0 || facts.bullet_range() <= 0.0 {
                    continue;
                }
            }
            Some(FireWeaponKind::Flame) => {
                if facts.damage <= 0
                    || facts.bullet_range() <= 0.0
                    || !flame_burns_this_tick(&facts, crate::level_time_ms(tick))
                {
                    continue;
                }
            }
            _ => continue,
        }
        let mut rng = MatchRng::new(shot.combat_seed as u64);
        let barrels = if facts.fire_type == 5 {
            shot.ammo_used.max(1)
        } else {
            1
        };
        let pellet_count = (facts.pellet_count() * barrels).clamp(1, u16::MAX as i32) as u16;
        for pellet in 0..pellet_count {
            out.push(Emission {
                combat_seed: shot.combat_seed,
                shot_id: shot.shot_id,
                fire_cause: shot.fire_cause,
                pellet: PelletId(pellet),
                attacker: shot.attacker,
                attacker_life: shot.attacker_life,
                hand: shot.hand,
                weapon: shot.weapon,
                origin: shot.origin,
                direction: spread_pellet_direction(shot.angles, shot.spread_degrees, &mut rng),
                max_range: facts.bullet_range(),
                base_damage: facts.damage,
            });
        }
    }
    out
}

fn segment_is_shield(collider: Option<ColliderId>) -> bool {
    matches!(
        collider,
        Some(ColliderId::Player {
            hitloc: crate::shield::HITLOC,
            ..
        })
    )
}

pub(crate) fn phase_trace(
    world: &mut FrameWorld,
    tick: Tick,
    emissions: &[Emission],
) -> TracePhaseOutput {
    let mut output = TracePhaseOutput::default();
    let mut segment_ordinals = HashMap::<(ShotId, PelletId), u16>::new();
    let mut pending: std::collections::VecDeque<_> =
        emissions.iter().copied().map(|em| (em, 0u8)).collect();
    while let Some((em, bounces)) = pending.pop_front() {
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
        let (segments, terminal) = trace_emission(
            EmissionCollision {
                content: &world.content(),
                glass_pairs: &world.world_objects().glass_damage_pairs(),
                players: &query.players.poses,
                entities: &query.entities.rows,
            },
            &em,
            end,
            &facts,
            pen,
            world.world_objects().portal_blocked_exits(),
        );
        if let Some(next) = ricochet_emission(&em, bounces, &facts, pen, &segments) {
            pending.push_back((next, bounces + 1));
        }
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
                    && entity_iw4::bullet_hit_event(
                        if segment_is_shield(s.collider) {
                            3
                        } else {
                            facts.impact_type
                        },
                        false,
                    )
                    .is_some()
            })
            .count() as u32;
        let (bone_center, bone_half_size, xmodel_contents, model_key) =
            hit_dump(terminal, &query.entities.rows);
        output.shot_verdicts.push(ShotCollisionVerdict {
            shot_id: em.shot_id,
            pellet: em.pellet,
            attacker: em.attacker,
            geometry: shot_collision_geometry(
                terminal,
                query.players.verdict,
                entity_epoch,
                player_representation(terminal, &query.players.poses),
            ),
            terminal,
            startsolid,
            bone_center,
            bone_half_size,
            xmodel_contents,
            model_key,
            end: segments.last().map(|s| s.end),
            entity_clip: entity_clip_kind(terminal, &query.entities.rows),
            impact_n,
            event_n,
        });
        if segments.is_empty() {
            continue;
        }
        let mut glass_hit: Vec<u32> = Vec::new();
        for segment in &segments {
            let ordinal = segment_ordinals.entry((em.shot_id, em.pellet)).or_default();
            let segment_ordinal = *ordinal;
            *ordinal = ordinal
                .checked_add(1)
                .expect("bullet segment count exceeds wire limit");
            let exit = segment.surface_flags & fx_iw4::FX_IMPACT_EXIT_SURFACE_FLAG != 0;
            let dist = {
                let dx = segment.end[0] - em.origin[0];
                let dy = segment.end[1] - em.origin[1];
                let dz = segment.end[2] - em.origin[2];
                segment
                    .travel_distance
                    .unwrap_or_else(|| (dx * dx + dy * dy + dz * dz).sqrt())
            };
            let scaled =
                ((bullet_damage_at_distance(&facts, dist) as f32) * segment.damage_mult) as i32;
            if !exit {
                let means = crate::script_player::means(
                    world,
                    DamageSource::Shot(em.shot_id),
                    em.weapon,
                    0,
                    false,
                );
                crate::script::host::triggers::damage_line(
                    world.ecs(),
                    segment.start,
                    segment.end,
                    scaled,
                    em.attacker,
                    None,
                    means,
                );
            }
            let mut flesh_flags = 0u8;
            if !exit
                && let Some(ColliderId::Player {
                    client: victim,
                    life: victim_life,
                    hitloc,
                }) = segment.collider
            {
                let head = hud_iw4::obituary_is_headshot(hitloc);
                let mut fatal = false;
                if scaled > 0
                    && let Some(vmeta) = world.client_meta(victim)
                    && vmeta.lifecycle == ClientLifecycle::Alive
                {
                    let attempt = crate::DamageAttempt {
                        splash: false,
                        source: DamageSource::Shot(em.shot_id),
                        pellet: em.pellet,
                        attacker: em.attacker,
                        attacker_life: em.attacker_life,
                        target: victim,
                        target_life: victim_life,
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
                if hitloc != crate::shield::HITLOC {
                    flesh_flags = fx_iw4::flesh_hit_flags(head, fatal) as u8;
                }
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
                        .unwrap_or(ENTITYNUM_NONE),
                    Some(ColliderId::Player { client, .. }) => client.0 as i32,
                    _ => ENTITYNUM_NONE,
                },
                event_parm: i32::from(flesh_flags),
                weapon: em.weapon,
                correlation: em.shot_id.0,
                fire_cause: em.fire_cause,
                pellet: em.pellet.0,
                segment: segment_ordinal,
                hand: em.hand,
                origin: segment.end,
                origin2: segment.start,
                direction: segment.normal,
                surf_type: segment.surf_type,
                surface_flags: segment.surface_flags,
                simulation_flags: u8::from(segment.penetrated),
            };
            let victim = match segment.collider {
                Some(ColliderId::Player { client, .. }) => Some(client),
                _ => None,
            };
            if bullet_process_on_hit(segment.collider) {
                if let Some(world_event) = entity_iw4::bullet_hit_event(
                    if segment_is_shield(segment.collider) {
                        3
                    } else {
                        facts.impact_type
                    },
                    false,
                ) {
                    let world_audience =
                        victim.map_or(EventAudience::All, EventAudience::AllExcept);
                    world.push_entity_event(tick, world_audience, world_event, payload);
                    if let Some(victim) = victim
                        && let Some(local_event) = entity_iw4::bullet_hit_event(
                            if segment_is_shield(segment.collider) {
                                3
                            } else {
                                facts.impact_type
                            },
                            true,
                        )
                    {
                        world.push_entity_event(
                            tick,
                            EventAudience::Client(victim),
                            local_event,
                            payload,
                        );
                    }
                } else if fx_iw4::impact_table_row(facts.impact_type, false).is_some() {
                    world.push_pellet_fx(crate::PelletFxRecord {
                        attacker: em.attacker.0 as i32,
                        weapon: em.weapon,
                        correlation: em.shot_id.0,
                        fire_cause: em.fire_cause,
                        pellet: em.pellet.0,
                        segment: segment_ordinal,
                        hand: em.hand,
                        start: segment.start,
                        end: segment.end,
                        normal: segment.normal,
                        surf_type: segment.surf_type,
                        surface_flags: segment.surface_flags,
                        flesh_flags,
                    });
                }
            } else {
                world.push_pellet_fx(crate::PelletFxRecord {
                    attacker: em.attacker.0 as i32,
                    weapon: em.weapon,
                    correlation: em.shot_id.0,
                    fire_cause: em.fire_cause,
                    pellet: em.pellet.0,
                    segment: segment_ordinal,
                    hand: em.hand,
                    start: segment.start,
                    end: segment.end,
                    normal: [0.0; 3],
                    surf_type: segment.surf_type,
                    surface_flags: segment.surface_flags,
                    flesh_flags: 0,
                });
            }
            if exit || scaled <= 0 {
                continue;
            }
            match segment.collider {
                Some(ColliderId::Player { .. }) => {}
                Some(
                    ColliderId::EntityDObjBone { owner, .. }
                    | ColliderId::EntityLinkedBrush { owner, .. },
                ) => {
                    if let Some(ColliderId::EntityDObjBone { bone, .. }) = segment.collider
                        && crate::model_behaviors::apply_hit(
                            world,
                            tick,
                            owner,
                            bone,
                            scaled as u32,
                            Some(em.attacker),
                        )
                    {
                        continue;
                    }
                    if let Some(target) = owner.script_model() {
                        let bone = match segment.collider {
                            Some(ColliderId::EntityDObjBone { bone, .. }) => {
                                Some(usize::from(bone))
                            }
                            _ => None,
                        };
                        let means = crate::script_player::means(
                            world,
                            DamageSource::Shot(em.shot_id),
                            em.weapon,
                            0,
                            false,
                        );
                        crate::script::damage_entity(
                            world.ecs(),
                            &crate::script::EntityHit {
                                target,
                                amount: scaled,
                                attacker: Some(em.attacker),
                                means,
                                weapon: em.weapon,
                                point: segment.end,
                                dir: em.direction,
                                bone,
                                flags: 0,
                            },
                        );
                    }
                }
                Some(ColliderId::World { .. }) => {
                    if let Some(piece) = glass_piece_from_hit(segment.hit_type, segment.hit_id) {
                        if glass_hit.contains(&piece) {
                            continue;
                        }
                        glass_hit.push(piece);
                        let at_time_ms = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS))
                            .unwrap_or(i32::MAX);
                        let mut holdrand = *world.stuck_holdrand_mut();
                        world.world_objects_mut().apply_glass_hit(
                            piece,
                            scaled as u32,
                            at_time_ms,
                            crate::world_objects::GlassHit {
                                position: segment.end,
                                direction: em.direction,
                            },
                            &mut || crate::item::random_unit(&mut holdrand),
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
    let trace = MeleeTrace {
        muzzle: origin,
        forward,
        right,
        up,
        range,
        width,
        height,
    };
    for (index, offset) in MELEE_TRACE_OFFSETS
        .iter()
        .take(trace.sample_count())
        .enumerate()
    {
        let end = trace.endpoint(*offset);
        let (segments, _) = bullet_trace_segments_filtered(
            crate::world_graph::AtlasClip {
                brushes: world.clip_brushes(),
                bsp: world.clip_bsp(),
                cmodels: world.clip_cmodels(),
                mesh: world.clip_mesh(),
            },
            &query.players.poses,
            &query.entities.rows,
            &BulletTraceQuery {
                start: origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(attacker),
                ignore_hit: None,
                ignore_model: None,
            },
            weapon_iw4::BulletPenFacts::default(),
            world.penetration_table(),
            crate::bullet_collision::GlassTrace {
                is_solid: &is_solid,
                on_hit: None,
            },
        );
        if index == 0
            && let Some(segment) = segments.first()
        {
            crate::script::host::triggers::damage_line(
                world.ecs(),
                origin,
                segment.end,
                facts.melee_damage,
                attacker,
                None,
                "MOD_MELEE",
            );
        }
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
    let (kind, other) = match segment.collider {
        Some(ColliderId::Player { client, .. }) => {
            (entity_iw4::EntityEventKind::MELEE_HIT, client.0 as i32)
        }
        _ => (
            entity_iw4::EntityEventKind::MELEE_MISS,
            i32::from(trace_iw4::ENTITYNUM_WORLD),
        ),
    };
    world.push_entity_event(
        tick,
        EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number: attacker.0 as i32,
            attacker_entity_num: attacker.0 as i32,
            other_entity_num: other,
            event_parm: i32::from(facts.knife_model != 0),
            weapon,
            origin: segment.end,
            direction: forward,
            surf_type: segment.surf_type,
            surface_flags: segment.surface_flags,
            ..Default::default()
        },
    );
    match segment.collider {
        Some(ColliderId::Player {
            client: victim,
            life: victim_life,
            hitloc,
        }) => {
            if let Some(vmeta) = world.client_meta(victim)
                && vmeta.lifecycle == ClientLifecycle::Alive
            {
                let attempt = crate::DamageAttempt {
                    splash: false,
                    source: DamageSource::Melee,
                    pellet: PelletId(0),
                    attacker,
                    attacker_life,
                    target: victim,
                    target_life: victim_life,
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
                    crate::world_objects::GlassHit {
                        position: segment.end,
                        direction: forward,
                    },
                    &mut || crate::item::random_unit(&mut holdrand),
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

fn hit_dump(terminal: Option<ColliderId>, rows: &[EntityCollisionTraceGeom]) -> HitDumpOutput {
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

fn player_representation(
    terminal: Option<ColliderId>,
    poses: &[crate::bullet_collision::PlayerCollisionPose],
) -> PlayerCollisionRepresentation {
    let Some(ColliderId::Player { client, .. }) = terminal else {
        return PlayerCollisionRepresentation::StandingAabbV1;
    };
    match poses.iter().find(|pose| pose.client == client) {
        Some(pose) if !pose.bones.is_empty() => PlayerCollisionRepresentation::PosedBonesV1,
        _ => PlayerCollisionRepresentation::StandingAabbV1,
    }
}

fn entity_clip_kind(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityClipKind> {
    match terminal {
        Some(ColliderId::EntityLinkedBrush { .. }) => Some(EntityClipKind::LinkedBrush),
        Some(ColliderId::EntityDObjBone { owner, .. }) => {
            let coll = rows
                .iter()
                .find(|row| row.owner == owner)
                .and_then(|row| row.collision.as_ref())
                .and_then(|collision| collision.coll.as_ref());
            Some(
                if crate::bullet_collision::coll_tris_clip_available(coll, MASK_BULLET_WORLD) {
                    EntityClipKind::CollTris
                } else {
                    EntityClipKind::BoneBoxes
                },
            )
        }
        _ => None,
    }
}

fn shot_collision_geometry(
    terminal: Option<ColliderId>,
    player_history: HistorySampleVerdict,
    entity_epoch: Option<EntityCollisionEpoch>,
    representation: PlayerCollisionRepresentation,
) -> ShotCollisionGeometry {
    match terminal {
        Some(ColliderId::Player { .. }) => ShotCollisionGeometry::Player {
            representation,
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
                representation,
                history: player_history,
            },
            _ => ShotCollisionGeometry::Miss,
        },
    }
}

pub(crate) fn bullet_process_on_hit(collider: Option<ColliderId>) -> bool {
    collider.is_some()
}

pub(crate) fn raise_given_weapon(
    ps: &mut PlayerState,
    weapon: u32,
    hand: &weapon_iw4::WeaponHandState,
) {
    weapon_iw4::mark_weapon_used(ps, weapon);
    if hand.weap_anim as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK
        == weapon_iw4::weap_anim_event::FIRST_RAISE
    {
        movement_iw4::add_predictable_event(
            ps,
            entity_iw4::EntityEventKind::FIRST_RAISE_WEAPON.0,
            0,
        );
    }
}

#[derive(Clone, Copy)]
struct EmissionCollision<'a> {
    content: &'a crate::SimContent,
    glass_pairs: &'a [(u32, u16)],
    players: &'a [crate::PlayerCollisionPose],
    entities: &'a [EntityCollisionTraceGeom],
}

fn trace_emission(
    collision: EmissionCollision<'_>,
    em: &Emission,
    end: [f32; 3],
    facts: &weapon_iw4::WeaponCombatFacts,
    pen: weapon_iw4::BulletPenFacts,
    blocked: &[[bool; 2]],
) -> (Vec<crate::BulletTraceSegment>, Option<ColliderId>) {
    let content = collision.content;
    let Some(atlas) = content.atlas().filter(|atlas| !atlas.portals.is_empty()) else {
        return trace_emission_world(collision, em, end, facts, pen, 0.0);
    };
    let mut current = *em;
    let mut finish = end;
    let mut distance = 0.0;
    let mut damage_mult = 1.0;
    let mut all = Vec::new();
    for _ in 0..16 {
        let crossing = atlas.crossing(current.origin, finish);
        let leg_end = crossing.map_or(finish, |(t, _, _)| {
            std::array::from_fn(|i| current.origin[i] + (finish[i] - current.origin[i]) * t)
        });
        let (mut segments, terminal) =
            trace_emission_world(collision, &current, leg_end, facts, pen, distance);
        for segment in &mut segments {
            segment.damage_mult *= damage_mult;
        }
        if let Some(last) = segments.last() {
            damage_mult = last.damage_mult;
        }
        if terminal.is_none()
            && let Some(crate::TraceOutcome::Hit {
                end: at,
                normal,
                collider,
                ..
            }) = atlas.unavailable_exit_hit(blocked, current.origin, finish)
        {
            let tail = segments.last().filter(|s| s.collider.is_none()).copied();
            if tail.is_some() {
                segments.pop();
            }
            let mut impact = crate::bullet_collision::make_segment(
                tail.map_or(current.origin, |s| s.start),
                crate::bullet_collision::SegmentContact {
                    end: at,
                    normal,
                    collider: Some(collider),
                    startsolid: false,
                },
                false,
                0.,
                damage_mult,
                crate::BulletPath::Extended,
            );
            impact.travel_distance = Some(
                distance
                    + glam::Vec3::from_array(at).distance(glam::Vec3::from_array(current.origin)),
            );
            segments.push(impact);
            all.extend(segments);
            return (all, Some(collider));
        }
        all.extend(segments);
        if terminal.is_some() || crossing.is_none() {
            return (all, terminal);
        }
        let traveled =
            glam::Vec3::from_array(leg_end).distance(glam::Vec3::from_array(current.origin));
        distance += traveled;
        let (_, portal, from_a) = crossing.unwrap();
        let (entered, exit, yaw) = atlas.portals[portal.index()].oriented(from_a);
        current.max_range = (current.max_range - traveled).max(0.0);
        if current.max_range < 0.01 {
            return (all, None);
        }
        current.origin = crate::world_graph::carry_point(&entered, &exit, yaw, leg_end);
        current.direction = crate::world_graph::rotate_yaw(current.direction, yaw);
        current.origin = std::array::from_fn(|i| current.origin[i] + current.direction[i] * 0.02);
        finish =
            std::array::from_fn(|i| current.origin[i] + current.direction[i] * current.max_range);
    }
    (all, None)
}

fn trace_emission_world(
    collision: EmissionCollision<'_>,
    em: &Emission,
    end: [f32; 3],
    facts: &weapon_iw4::WeaponCombatFacts,
    pen: weapon_iw4::BulletPenFacts,
    distance_before: f32,
) -> (Vec<crate::BulletTraceSegment>, Option<ColliderId>) {
    let EmissionCollision {
        content,
        glass_pairs,
        players,
        entities,
    } = collision;
    let world = content
        .atlas()
        .map_or(0, |_| crate::world_graph::world_at(em.origin));
    let offset = crate::world_graph::world_offset(world);
    let local_players: Vec<_> = players
        .iter()
        .filter(|p| content.atlas().is_none() || crate::world_graph::world_at(p.origin) == world)
        .map(|p| crate::world_graph::shifted(p, offset))
        .collect();
    let original = *em;
    let mut local = original;
    local.origin = std::array::from_fn(|i| original.origin[i] - offset[i]);
    let end = std::array::from_fn(|i| end[i] - offset[i]);
    let em = &local;
    let beyond = content
        .atlas()
        .and_then(|atlas| world.checked_sub(1).and_then(|i| atlas.worlds.get(i)));
    let (brushes, bsp, cmodels, mesh) = match beyond {
        Some(map) => (&map.brushes[..], &map.bsp, &map.cmodels, &map.mesh),
        None => (
            content.clip_brushes(),
            content.clip_bsp(),
            content.clip_cmodels(),
            content.clip_mesh(),
        ),
    };
    let entities = if world == 0 { entities } else { &[] };
    let glass_damage = RefCell::new(glass_pairs.iter().copied().collect::<HashMap<u32, u16>>());
    let glass_seen = RefCell::new(Vec::<u32>::new());
    let on_glass_hit = |piece: u16, end: [f32; 3]| {
        let pane = u32::from(piece);
        if glass_seen.borrow().contains(&pane) {
            return;
        }
        glass_seen.borrow_mut().push(pane);
        let dist = {
            let dx = end[0] - em.origin[0];
            let dy = end[1] - em.origin[1];
            let dz = end[2] - em.origin[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        let scaled = bullet_damage_at_distance(facts, distance_before + dist).max(0) as u32;
        let mut map = glass_damage.borrow_mut();
        let cur = map.entry(pane).or_insert(0);
        *cur = glass_add_damage(*cur, scaled);
    };
    let (mut segments, terminal) = bullet_trace_segments_filtered(
        crate::world_graph::AtlasClip {
            brushes,
            bsp,
            cmodels,
            mesh,
        },
        &local_players,
        entities,
        &BulletTraceQuery {
            start: em.origin,
            end,
            mask: MASK_BULLET_WORLD,
            ignore: Some(em.attacker),
            ignore_hit: None,
            ignore_model: None,
        },
        pen,
        &content.weapons().pen_table,
        crate::bullet_collision::GlassTrace {
            is_solid: &|piece| {
                glass_piece_is_solid(
                    glass_damage
                        .borrow()
                        .get(&(u32::from(piece)))
                        .copied()
                        .unwrap_or(0),
                )
            },
            on_hit: Some(&on_glass_hit),
        },
    );
    for segment in &mut segments {
        segment.travel_distance = Some(
            distance_before
                + glam::Vec3::from_array(segment.end).distance(glam::Vec3::from_array(em.origin)),
        );
        for (i, offset_entry) in offset[..3].iter().enumerate() {
            segment.start[i] += *offset_entry;
            segment.end[i] += *offset_entry;
        }
    }
    (segments, terminal)
}

fn ricochet_emission(
    em: &Emission,
    bounces: u8,
    facts: &weapon_iw4::WeaponCombatFacts,
    pen: weapon_iw4::BulletPenFacts,
    segments: &[crate::BulletTraceSegment],
) -> Option<Emission> {
    if bounces >= 8
        || facts.weap_type != weapon_iw4::WEAPTYPE_BULLET
        || facts.weap_class == weapon_iw4::WEAPCLASS_SPREAD
        || pen.explosive_bullet
    {
        return None;
    }
    let hit = segments
        .last()
        .filter(|hit| !hit.startsolid && segment_is_shield(hit.collider))?;
    let mut rng = MatchRng::new(
        u64::from(em.combat_seed) ^ (u64::from(em.pellet.0) << 32) ^ u64::from(bounces),
    );
    let chance = (pen.ricochet_chance * 0.5).clamp(0.0, 1.0);
    let roll = (rng.next_u32() >> 8) as f32 / 16_777_216.0;
    if roll <= chance && chance > 0.0 {
        let direction = crate::shield::ricochet_direction(em.direction, hit.normal)?;
        Some(Emission {
            origin: std::array::from_fn(|i| hit.end[i] + direction[i]),
            direction,
            ..*em
        })
    } else {
        None
    }
}

#[derive(Default)]
pub struct OwnerShotEffects {
    pub events: Vec<(entity_iw4::EntityEventKind, crate::EntityEventPayload)>,
    pub pellets: Vec<crate::PelletFxRecord>,
}

pub fn owner_shot_effects(
    scene: &crate::CollisionScene,
    snapshot: &crate::Snapshot,
    tick: Tick,
    owner: ClientId,
    fire: &weapon_iw4::FiredWeapon,
) -> OwnerShotEffects {
    let content = &scene.content;
    let cause = fire
        .cause
        .expect("sequenced owner fire requires its command cause");
    let shot = AcceptedShot {
        shot_id: fire.shot_id,
        fire_cause: fire.cause,
        attacker: owner,
        attacker_life: cause.life,
        hand: fire.hand,
        weapon: fire.weapon,
        ammo_used: fire.ammo_used,
        origin: fire.origin,
        angles: fire.angles,
        ads_frac: fire.ads_frac,
        view_height_current: fire.view_height,
        aim_spread_scale: fire.aim_spread_scale,
        perks0: fire.perks0,
        combat_seed: fire
            .combat_seed
            .expect("sequenced owner fire requires its combat seed"),
        owner_velocity: fire.velocity,
        spread_degrees: fire.spread_degrees,
    };
    let emissions = phase_emit(content, tick, std::slice::from_ref(&shot));
    if emissions.is_empty() {
        return OwnerShotEffects::default();
    }
    let players: Vec<_> = snapshot
        .players
        .iter()
        .filter(|(id, _)| *id != owner)
        .filter_map(|(id, ps)| {
            let meta = snapshot.meta.for_client(*id)?;
            (meta.lifecycle == ClientLifecycle::Alive).then(|| crate::PlayerCollisionPose {
                client: *id,
                origin: ps.origin,
                mins: crate::PLAYER_MINS,
                maxs: crate::PLAYER_MAXS,
                life_sequence: meta.life_sequence,
                hit_volume: crate::HitVolumeKind::Standing,
                bones: meta
                    .shield_collision
                    .as_ref()
                    .map_or_else(Vec::new, |collision| collision.bones.clone()),
                viewangles: [0.0; 3],
                shield: meta.shield,
                shield_normal: meta
                    .shield_collision
                    .as_ref()
                    .map(|collision| collision.normal),
            })
        })
        .collect();
    let entities: Vec<_> = scene
        .capabilities
        .iter()
        .map(crate::EntityCollisionCapabilities::trace_geom)
        .collect();
    let glass = scene.world_objects.glass_damage_pairs();
    let mut out = OwnerShotEffects::default();
    let mut ordinals = HashMap::<(ShotId, PelletId), u16>::new();
    let mut pending: std::collections::VecDeque<_> =
        emissions.into_iter().map(|em| (em, 0u8)).collect();
    while let Some((em, bounces)) = pending.pop_front() {
        let Some(facts) = content.weapon_combat_facts(em.weapon) else {
            continue;
        };
        let pen = content
            .weapons()
            .bullet_pen
            .get(em.weapon as usize)
            .copied()
            .unwrap_or_else(weapon_iw4::BulletPenFacts::default);
        let end = std::array::from_fn(|i| em.origin[i] + em.direction[i] * em.max_range);
        let (segments, _) = trace_emission(
            EmissionCollision {
                content,
                glass_pairs: &glass,
                players: &players,
                entities: &entities,
            },
            &em,
            end,
            &facts,
            pen,
            scene.world_objects.portal_blocked_exits(),
        );
        if let Some(next) = ricochet_emission(&em, bounces, &facts, pen, &segments) {
            pending.push_back((next, bounces + 1));
        }
        for segment in segments {
            let ordinal = ordinals.entry((em.shot_id, em.pellet)).or_default();
            let segment_ordinal = *ordinal;
            *ordinal = ordinal
                .checked_add(1)
                .expect("bullet segment count exceeds wire limit");
            let exit = segment.surface_flags & fx_iw4::FX_IMPACT_EXIT_SURFACE_FLAG != 0;
            if matches!(segment.collider, Some(ColliderId::Player { .. })) {
                out.pellets.push(crate::PelletFxRecord {
                    attacker: owner.0 as i32,
                    weapon: em.weapon,
                    correlation: em.shot_id.0,
                    fire_cause: em.fire_cause,
                    pellet: em.pellet.0,
                    segment: segment_ordinal,
                    hand: em.hand,
                    start: segment.start,
                    end: segment.end,
                    normal: [0.0; 3],
                    surf_type: segment.surf_type,
                    surface_flags: segment.surface_flags,
                    flesh_flags: 0,
                });
                continue;
            }
            let flesh_flags = if !exit {
                match segment.collider {
                    Some(ColliderId::Player { hitloc, .. }) if hitloc != crate::shield::HITLOC => {
                        fx_iw4::flesh_hit_flags(hud_iw4::obituary_is_headshot(hitloc), false) as u8
                    }
                    _ => 0,
                }
            } else {
                0
            };
            if bullet_process_on_hit(segment.collider) {
                let impact_type = if segment_is_shield(segment.collider) {
                    3
                } else {
                    facts.impact_type
                };
                if let Some(event) = entity_iw4::bullet_hit_event(impact_type, false) {
                    out.events.push((
                        event,
                        crate::EntityEventPayload {
                            number: owner.0 as i32,
                            attacker_entity_num: owner.0 as i32,
                            other_entity_num: match segment.collider {
                                Some(
                                    ColliderId::EntityDObjBone { owner, .. }
                                    | ColliderId::EntityLinkedBrush { owner, .. },
                                ) => owner
                                    .script_model()
                                    .and_then(|id| {
                                        snapshot
                                            .meta
                                            .script_movers
                                            .iter()
                                            .find(|mover| mover.id == id)
                                    })
                                    .map_or(ENTITYNUM_NONE, |mover| mover.state.number),
                                Some(ColliderId::Player { client, .. }) => client.0 as i32,
                                _ => ENTITYNUM_NONE,
                            },
                            event_parm: i32::from(flesh_flags),
                            weapon: em.weapon,
                            correlation: em.shot_id.0,
                            fire_cause: em.fire_cause,
                            pellet: em.pellet.0,
                            segment: segment_ordinal,
                            hand: em.hand,
                            origin: segment.end,
                            origin2: segment.start,
                            direction: segment.normal,
                            surf_type: segment.surf_type,
                            surface_flags: segment.surface_flags,
                            simulation_flags: u8::from(segment.penetrated),
                        },
                    ));
                    continue;
                }
                if fx_iw4::impact_table_row(facts.impact_type, false).is_none() {
                    continue;
                }
            }
            out.pellets.push(crate::PelletFxRecord {
                attacker: owner.0 as i32,
                weapon: em.weapon,
                correlation: em.shot_id.0,
                fire_cause: em.fire_cause,
                pellet: em.pellet.0,
                segment: segment_ordinal,
                hand: em.hand,
                start: segment.start,
                end: segment.end,
                normal: if bullet_process_on_hit(segment.collider) {
                    segment.normal
                } else {
                    [0.0; 3]
                },
                surf_type: segment.surf_type,
                surface_flags: segment.surface_flags,
                flesh_flags: if bullet_process_on_hit(segment.collider) {
                    flesh_flags
                } else {
                    0
                },
            });
        }
    }
    out
}
