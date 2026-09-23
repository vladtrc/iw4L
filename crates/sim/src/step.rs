use crate::frame::FrameWorld;
use std::cell::Cell;

use bevy_ecs::prelude::{Resource, World};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use movement_iw4::{
    ANGLE2SHORT, AirMoveContext, CmdScaleWalkContext, CollisionBackend, GroundTraceInput,
    JumpLaunchContext, MoveBounds, PmoveSingleContext, SHORT2ANGLE, SprintContext, ViewAngleClamp,
    WalkMoveContext, bg_get_max_sprint_time, consume_player_events, pm_footsteps_anim_move_type,
    pm_move,
};
use trace_iw4::{HITTYPE_ENTITY, Trace};

use crate::bullet_collision::{LinkedBrushCollisionBrush, PLAYER_MAXS, PLAYER_MINS};
use crate::combat::{advance_weapon_command, phase_emit, phase_trace};
use crate::damage::{ExplosionBlast, apply_explode_glass_blast, apply_explosion_blast};
use crate::identities::MatchPhase;
use crate::input::{ClientAction, TickInput};
use crate::match_state::{
    ClientLifecycle, EntityEventPayload, EventAudience, LoadoutSpec, SimEvent,
};
use crate::snapshot::Snapshot;
use crate::spawn::{SpawnReject, decide_spawn_seeded_report};
use crate::world::{
    ClientId, SimBrush, SimClipBsp, SimClipMesh, SimState, Tick, clip_move_to_bmodels, clip_trace,
    give_weapon_to_ps_akimbo, gsc_give_weapon_is_akimbo, inventory_add_weapon, spawn_player_state,
};
use playerstate_iw4::PlayerState;
use playerstate_iw4::buttons;

#[derive(Resource)]
struct StepRequest {
    tick: Tick,
    input: TickInput,
    msec: i32,
    reason: crate::StepReason,
    output: Option<Snapshot>,
}

pub(crate) fn schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            advance_time_system,
            expire_transient_events_system,
            apply_actions_system,
            run_players_system,
            record_collision_state_system,
            run_entity_types_system,
            dispatch_touches_system,
            finalize_system,
            publish_snapshot_system,
        )
            .chain(),
    );
    schedule
}

pub(crate) fn run_schedule(
    ecs: &mut World,
    schedule: &mut Schedule,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: crate::StepReason,
) -> Snapshot {
    assert!(
        !ecs.contains_resource::<StepRequest>(),
        "simulation schedule already has a pending frame"
    );
    ecs.insert_resource(StepRequest {
        tick,
        input: input.clone(),
        msec: msec.clamp(1, 200),
        reason,
        output: None,
    });
    schedule.run(ecs);
    ecs.remove_resource::<StepRequest>()
        .and_then(|request| request.output)
        .expect("simulation schedule did not publish a snapshot")
}

fn frame_world(world: &mut World) -> FrameWorld<'_> {
    crate::frame::FrameWorld::from_world(world)
}

pub fn phase_materialize_entity_dobjs(world: &mut SimState) {
    for capabilities in world.entity_collision_capabilities_mut() {
        if let Some(dobj) = &mut capabilities.dobj {
            dobj.materialize();
        }
    }
    let toy_ids: Vec<crate::ScriptModelId> = world
        .world_objects()
        .toy_bodies()
        .iter()
        .map(|(id, _, _)| *id)
        .collect();
    if toy_ids.is_empty() {
        return;
    }
    for capabilities in world.entity_collision_capabilities_mut() {
        let Some(id) = capabilities.owner.script_model() else {
            continue;
        };
        if !toy_ids.iter().any(|have| *have == id) {
            continue;
        }
        if let Some(dobj) = capabilities.dobj.as_mut() {
            dobj.ensure_bounds_collision();
        }
    }
}

fn phase_destructible_death_presentation(world: &mut FrameWorld, msec: i32) {
    let dt = msec as f32 / 1000.0;
    let deaths: Vec<(crate::ScriptModelId, &'static str, &'static str)> = world
        .world_objects()
        .vehicle_bodies()
        .iter()
        .filter(|(_, kind, body)| body.state_index >= kind.destroyed_state())
        .map(|(id, kind, _)| {
            let death = kind.definition().death;
            (*id, death.husk, death.clip)
        })
        .collect();
    let mut started = Vec::new();
    let mut times = Vec::new();
    for (id, husk, clip) in deaths {
        for capabilities in world.entity_collision_capabilities_mut() {
            if capabilities.owner.script_model() != Some(id) {
                continue;
            }
            let Some(dobj) = capabilities.dobj.as_mut() else {
                continue;
            };
            let already = dobj
                .semantic_state
                .tree
                .as_ref()
                .and_then(|tree| tree.nodes.first())
                .and_then(|node| node.clip.as_deref())
                == Some(clip);
            if !already {
                dobj.begin_destructible_death(husk, clip);
                started.push(id);
            } else {
                times.push((id, dobj.advance_destructible_death(dt)));
            }
        }
    }
    for id in started {
        world.world_objects_mut().note_death_anim_started(id);
    }
    for (id, time) in times {
        world.world_objects_mut().set_death_anim_time(id, time);
    }
    apply_explodable_barrel_death_presentation(world);
    apply_toy_stage_presentation(world);
    apply_flammable_crate_death_presentation(world);
    apply_destructable_death_presentation(world);
}

pub fn apply_explodable_barrel_death_presentation(world: &mut SimState) {
    let downs = world.world_objects_mut().take_explodable_barrel_downs();
    for id in &downs {
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::ExplodableBarrelPhysics {
                source_ordinal: id.to_wire(),
            });
    }
    let ids: Vec<crate::ScriptModelId> = world
        .world_objects()
        .explodable_barrel_bodies()
        .iter()
        .filter(|(_, body)| body.state_index >= crate::EXPLODABLE_BARREL_DESTROYED_STATE)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        for capabilities in world.entity_collision_capabilities_mut() {
            if capabilities.owner.script_model() != Some(id) {
                continue;
            }
            capabilities.linked_brushes.clear();
            if let Some(dobj) = capabilities.dobj.as_mut() {
                dobj.set_stage_model(crate::EXPLODABLE_BARREL_HUSK);
            }
        }
    }
}

pub fn apply_toy_stage_presentation(world: &mut SimState) {
    for id in world.world_objects_mut().take_toy_part_launches() {
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::DestructiblePartLaunch {
                source_ordinal: id.to_wire(),
            });
    }
    let stages: Vec<(
        crate::ScriptModelId,
        Option<&'static str>,
        Vec<&'static str>,
    )> = world
        .world_objects()
        .toy_bodies()
        .iter()
        .map(|(id, kind, body)| {
            let def = kind.definition();
            (
                *id,
                def.stage_model(body.state_index),
                def.hidden_tags(body.state_index).collect(),
            )
        })
        .collect();
    for (id, model, hidden) in stages {
        for capabilities in world.entity_collision_capabilities_mut() {
            if capabilities.owner.script_model() != Some(id) {
                continue;
            }
            let Some(dobj) = capabilities.dobj.as_mut() else {
                continue;
            };
            if let Some(model) = model {
                dobj.set_stage_model(model);
            }
            dobj.hide_tags(&hidden);
        }
    }
}

pub fn apply_flammable_crate_death_presentation(world: &mut SimState) {
    let downs = world.world_objects_mut().take_flammable_crate_downs();
    for id in &downs {
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::FlammableCrateFx {
                source_ordinal: id.to_wire(),
            });
        world
            .script_gaps_mut()
            .raise(gamemode_iw4::ScriptGapCause::FlammableCratePhysics {
                source_ordinal: id.to_wire(),
            });
    }
    let ids = world.world_objects().destroyed_flammable_crate_ids();
    for id in ids {
        for capabilities in world.entity_collision_capabilities_mut() {
            if capabilities.owner.script_model() != Some(id) {
                continue;
            }
            capabilities.linked_brushes.clear();
            if let Some(dobj) = capabilities.dobj.as_mut() {
                dobj.set_stage_model(gamemode_iw4::FLAMMABLE_CRATE_HUSK);
            }
        }
    }
}

pub fn apply_destructable_death_presentation(world: &mut SimState) {
    let downs = world.world_objects_mut().take_destructable_downs();
    for down in &downs {
        if down.play_fx {
            world
                .script_gaps_mut()
                .raise(gamemode_iw4::ScriptGapCause::DestructablePlayFx {
                    source_ordinal: down.id.to_wire(),
                });
        }
    }
    let ids: Vec<crate::ScriptModelId> = world.world_objects().destroyed_destructable_ids();
    for id in ids {
        for capabilities in world.entity_collision_capabilities_mut() {
            if capabilities.owner.script_model() != Some(id) {
                continue;
            }
            capabilities.linked_brushes.clear();
            capabilities.dobj = None;
        }
    }
}

fn phase_animated_map_models(world: &mut FrameWorld, tick: Tick, msec: i32) {
    let dt = msec as f32 / 1000.0;

    let at_time = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
    let mut mover_apos = Vec::new();
    world.visit_script_movers(|mover| {
        mover_apos.push((
            mover.id,
            crate::gentity::apos_from_entity_state(&mover.state),
        ));
    });
    for capabilities in world.entity_collision_capabilities_mut() {
        let Some(dobj) = capabilities.dobj.as_mut() else {
            continue;
        };
        if dobj.play_anim.is_some() {
            dobj.advance_script_model_play_anim(dt);
        }
        if let Some(id) = capabilities.owner.script_model() {
            if let Some((_, apos)) = mover_apos.iter().find(|(mover_id, _)| *mover_id == id) {
                dobj.apply_trajectory_apos_at(*apos, at_time);
                continue;
            }
        }
        if dobj.apos.is_some() {
            dobj.apply_apos_at(at_time);
        }
    }
}

fn emit_vehicle_fx_events(world: &mut FrameWorld, tick: Tick) {
    let (death_fx, death_sounds) = world.world_objects_mut().take_new_death_fx_pulses();
    let (stage_fx, stage_sounds) = world.world_objects_mut().take_new_stage_pulses();
    let loop_fx = world
        .world_objects_mut()
        .tick_vehicle_loopfx(crate::MATCH_TICK_MS);
    for pulse in death_sounds.into_iter().chain(stage_sounds) {
        let origin = pulse_world_origin(world, pulse.owner, pulse.tag, pulse.origin);
        let event_parm = i32::from(world.sound_alias_index(pulse.alias));
        world.push_entity_event(
            tick,
            EventAudience::All,
            entity_iw4::EntityEventKind::SOUND_ALIAS,
            EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                event_parm,
                origin,
                correlation: pulse.owner.to_wire(),
                ..Default::default()
            },
        );
    }
    for pulse in death_fx.into_iter().chain(loop_fx).chain(stage_fx) {
        let (origin, direction) = pulse_world_pose(world, &pulse);
        let event_parm = i32::from(world.effect_name_index(pulse.def_name));
        world.push_entity_event(
            tick,
            EventAudience::All,
            entity_iw4::EntityEventKind::PLAY_FX,
            EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                event_parm,
                origin,
                direction,
                correlation: pulse.owner.to_wire(),
                ..Default::default()
            },
        );
    }
}

/// Publishes the loops the destructibles are speaking, aliases interned into
/// the configstring the client resolves them through.
fn publish_destructible_loop_sounds(world: &mut FrameWorld) {
    let speaking = world.world_objects().speaking_loop_sounds();
    let rows = speaking
        .into_iter()
        .map(
            |(owner, alias, origin)| crate::world_objects::DestructibleLoopSound {
                owner,
                alias_index: world.sound_alias_index(alias),
                origin,
            },
        )
        .collect();
    world.world_objects_mut().set_destructible_loop_sounds(rows);
}

fn pulse_world_pose(
    world: &FrameWorld,
    pulse: &crate::world_objects::VehicleFxPulse,
) -> ([f32; 3], [f32; 3]) {
    let Some(tag) = pulse.tag else {
        return (pulse.origin, gamemode_iw4::VEHICLE_DEATH_FX_FORWARD);
    };
    let pose = script_model_dobj(world, pulse.owner)
        .and_then(|dobj| dobj.tag_world_pose(tag))
        .unwrap_or((pulse.origin, gamemode_iw4::VEHICLE_DEATH_FX_FORWARD));
    if pulse.use_tag_angles {
        pose
    } else {
        (pose.0, gamemode_iw4::VEHICLE_DEATH_FX_FORWARD)
    }
}

fn pulse_world_origin(
    world: &FrameWorld,
    owner: crate::ScriptModelId,
    tag: Option<&str>,
    fallback: [f32; 3],
) -> [f32; 3] {
    let Some(tag) = tag else {
        return fallback;
    };
    script_model_dobj(world, owner)
        .and_then(|dobj| dobj.tag_world_pose(tag))
        .map(|(origin, _)| origin)
        .unwrap_or(fallback)
}

fn script_model_dobj<'a>(
    world: &'a FrameWorld,
    id: crate::ScriptModelId,
) -> Option<&'a crate::bullet_collision::AuthorityDObjState> {
    world
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(id))?
        .dobj
        .as_ref()
}

fn phase_stuck_in_client(world: &mut FrameWorld) {
    let mut ids: Vec<ClientId> = world.client_ids_sorted();
    ids.retain(|id| {
        world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            && world.player(*id).is_some()
    });
    let mut rows: Vec<gamemode_iw4::StuckClient> = ids
        .iter()
        .filter_map(|id| world.player(*id).map(stuck_row))
        .collect();
    let mut pairs = Vec::new();
    let eject_speed = gamemode_iw4::G_PLAYER_COLLISION_EJECT_SPEED_DEFAULT;
    let mut holdrand = *world.stuck_holdrand_mut();
    for i in 0..rows.len() {
        if let Some(pair) = gamemode_iw4::stuck_in_client(i, &mut rows, eject_speed, &mut holdrand)
        {
            pairs.push((ids[pair.self_idx], ids[pair.other_idx]));
        }
    }
    *world.stuck_holdrand_mut() = holdrand;
    for (id, row) in ids.iter().zip(rows.iter()) {
        if let Some(ps) = world.player_mut(*id) {
            ps.velocity[0] = row.velocity[0];
            ps.velocity[1] = row.velocity[1];
            ps.pm_time = row.pm_time;
            ps.pm_flags = row.pm_flags;
        }
    }
    world.set_stuck_ejects(pairs);
}

fn phase_health_regen(world: &mut FrameWorld, tick: Tick) {
    let now_ms = crate::corpse::level_time_ms(tick);
    let ids: Vec<ClientId> = world.client_ids_sorted();
    for id in ids {
        if !world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let Some(ps) = world.player(id) else {
            continue;
        };
        if ps.health <= 0 {
            continue;
        }
        let health = ps.health;
        let max_health = ps.max_health;
        let mut regen = world
            .client_meta(id)
            .map(|m| m.health_regen)
            .unwrap_or_default();
        let step = gamemode_iw4::player_health_regen_tick(health, max_health, now_ms, &mut regen);
        {
            let meta = world.client_meta_mut(id);
            meta.health_regen = regen;
            meta.last_named_sound = step.sound;
        }
        if let Some(next) = step.health {
            if let Some(ps) = world.player_mut(id) {
                ps.health = next;
            }
        }
    }
}

pub(crate) fn phase_finalstand_timer(world: &mut FrameWorld, tick: Tick) {
    let now_ms = crate::hudelem::hud_level_time_ms(tick);
    let ids: Vec<ClientId> = world.client_ids_sorted();
    for id in ids {
        let Some(until) = world.client_meta(id).and_then(|m| m.laststand_until_ms) else {
            continue;
        };
        if now_ms < until {
            continue;
        }
        if !world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let max_health = world.player(id).map(|ps| ps.max_health).unwrap_or(0);
        if let Some(ps) = world.player_mut(id) {
            ps.pm_type = 0;
            ps.view_height_target = movement_iw4::view_height::CROUCH;
            ps.pm_flags &= !playerstate_iw4::pm_flags::LAST_STAND;
            ps.perks[0] &= !playerstate_iw4::PERK_PISTOLDEATH;
            ps.health = max_health;
        }
        let meta = world.client_meta_mut(id);
        meta.pistoldeath_this_life = false;
        meta.laststand_until_ms = None;
    }
}

fn stuck_row(ps: &PlayerState) -> gamemode_iw4::StuckClient {
    gamemode_iw4::StuckClient {
        origin: ps.origin,
        velocity: ps.velocity,
        speed: ps.speed,
        other_flags: ps.other_flags,
        health: ps.health,
        pm_time: ps.pm_time,
        pm_flags: ps.pm_flags,
        maxs_x: PLAYER_MAXS[0],
        bounds_mid: [
            ps.origin[0] + (PLAYER_MINS[0] + PLAYER_MAXS[0]) * 0.5,
            ps.origin[1] + (PLAYER_MINS[1] + PLAYER_MAXS[1]) * 0.5,
            ps.origin[2] + (PLAYER_MINS[2] + PLAYER_MAXS[2]) * 0.5,
        ],
        bounds_half: [
            (PLAYER_MAXS[0] - PLAYER_MINS[0]) * 0.5,
            (PLAYER_MAXS[1] - PLAYER_MINS[1]) * 0.5,
            (PLAYER_MAXS[2] - PLAYER_MINS[2]) * 0.5,
        ],
    }
}

fn advance_time_system(world: &mut World) {
    let tick;
    {
        let mut request = world.resource_mut::<StepRequest>();
        request.input.canonicalize();
        tick = request.tick;
    }
    let mut frame = frame_world(world);
    frame.mark_running();
    frame.begin_entity_frame(tick);
}

fn expire_transient_events_system(world: &mut World) {
    let tick = world.resource::<StepRequest>().tick;
    let mut frame = frame_world(world);
    frame.enter_kernel_phase(crate::gentity::KernelPhase::ExpireTransientEvents);
    frame.expire_dying_missiles(tick);
    frame.clear_tick_events(tick);
}

fn apply_actions_system(world: &mut World) {
    let tick = world.resource::<StepRequest>().tick;
    let actions = world.resource::<StepRequest>().input.actions.clone();
    let mut frame = frame_world(world);
    for (id, action) in &actions {
        if action_needs_player_row(action) {
            let _ = frame.ensure_player(*id);
        } else {
            let _ = frame.client_meta_mut(*id);
        }
    }

    frame.enter_kernel_phase(crate::gentity::KernelPhase::ApplyActions);
    apply_actions(&mut frame, tick, &actions);
    advance_death_timers(&mut frame, tick);
    resolve_pending_spawns(&mut frame, tick);
}

fn run_players_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let level_time = crate::level_time_ms(tick);
    let mut input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    world.objectives.constrain_cmds(&mut input.cmds);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunPlayers);

    let allow_move = world.phase() == MatchPhase::Playing;
    let content = world.content();
    let cmodel_models = &content.clip_cmodels().models;
    let (brushes, bsp, mesh) = (
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_mesh(),
    );
    let original_buttons = world.old_buttons_mut().clone();
    let original_angles = world.old_cmd_angles_mut().clone();
    let mut consumed = Vec::new();
    if allow_move {
        phase_materialize_entity_dobjs(&mut world);
        for (id, cmd) in &input.cmds {
            world.select_lagcomp_command(*id, cmd.server_time);
            if !world
                .client_meta(*id)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                continue;
            }
            let Some(ps) = world.player(*id) else {
                continue;
            };

            let delta = cmd.server_time.wrapping_sub(ps.command_time);
            if delta <= 0 {
                continue;
            }

            let old_buttons = world
                .old_buttons_mut()
                .iter()
                .find(|(c, _)| c == id)
                .map_or(0, |(_, buttons)| *buttons);
            let Some(ps) = world.player(*id).copied() else {
                continue;
            };
            let weapon = ps.weapon;
            let scales = world.scales_for(weapon);
            let facts = world.combat_facts_for(weapon);
            let aim_down_sight = facts.is_some_and(|f| f.aim_down_sight);
            let ads_allowed = facts.is_some_and(|f| {
                weapon != 0 && {
                    let mut allow = f.ads_allow_facts();

                    allow.clip_index = weapon_iw4::bg_clip_table_key(allow.clip_index, weapon);
                    weapon_iw4::pm_is_ads_allowed(
                        &ps,
                        &allow,
                        &ps.ammoclip,
                        ps.last_weapon_hand.min(1) as u8,
                    )
                }
            });
            let overlay_reticle = facts.map(|f| f.overlay_reticle).unwrap_or(0);
            let (ads_in_rate, ads_out_rate, rechamber_while_ads, ads_fire_only) = facts
                .map(|f| f.ads_frac_context())
                .unwrap_or((1.0 / 200.0, 1.0 / 200.0, true, false));
            let (melee_delay_ms, melee_charge_delay_ms) = facts
                .map(|f| (f.melee_delay_ms, f.melee_charge_delay_ms))
                .unwrap_or((0, 0));
            let context = pmove_context(
                old_buttons,
                scales,
                ads_allowed,
                aim_down_sight,
                ads_in_rate,
                ads_out_rate,
                rechamber_while_ads,
                ads_fire_only,
                melee_delay_ms,
                melee_charge_delay_ms,
                overlay_reticle,
                crate::damage::shellshock_dump_affects_movement(ps.shellshock_index),
            );
            let mut cmd = *cmd;
            let commanded_move = cmd.forwardmove != 0 || cmd.rightmove != 0;
            let linked_brushes: Vec<LinkedBrushCollisionBrush> = world
                .entity_collision_capabilities()
                .iter()
                .filter(|c| world.objectives.collision_active(c.owner))
                .flat_map(|c| c.linked_brushes.iter().cloned())
                .collect();
            let bodies = alive_body_clips(&world);
            let glass_damage = world.world_objects().glass_damage_pairs();
            let backend = ClipBackend {
                brushes: &brushes,
                bsp: &bsp,
                mesh: &mesh,
                glass_damage: &glass_damage,
                bodies: &bodies,
                self_entnum: id.0 as u16,
                cmodels: &cmodel_models,
                linked_brushes: &linked_brushes,
            };
            let script = world.player_anim_script();
            let mantle = world.mantle_xanims();
            let applied_mt = Cell::new(None);
            let (walking, linked_bounds, anim_movetype, view_w, primary, moved_from, moved_to) = {
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");

                if ps.shellshock_time.wrapping_add(ps.shellshock_duration) < level_time {
                    ps.pm_flags &= !playerstate_iw4::pm_flags::SHELLSHOCKED;
                }
                let moved_from = ps.origin;
                let result = pm_move(
                    ps,
                    &mut cmd,
                    context,
                    &backend,
                    mantle.as_ref(),
                    mantle.as_ref(),
                );
                let pml = result.pml;
                let anim_movetype = pm_footsteps_anim_move_type(
                    ps,
                    cmd.forwardmove,
                    cmd.rightmove,
                    pml.almost_ground_plane != 0,
                );
                let (view_w, primary) = crate::pmove_anim_weapon_ids(ps);
                let moved_to = ps.origin;
                (
                    pml.walking as i32,
                    result.bounds,
                    anim_movetype,
                    view_w,
                    primary,
                    moved_from,
                    moved_to,
                )
            };
            world
                .client_meta_mut(*id)
                .input_receipt
                .record(commanded_move, moved_from, moved_to);
            if let Some(movetype) = anim_movetype {
                let view_facts = world.combat_facts_for(view_w);
                let primary_facts = world.combat_facts_for(primary);
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");
                let conds = crate::anim_conditions_from_pmove(ps, view_facts, primary_facts);
                let requested_matches = script
                    .as_ref()
                    .map(|script| script.matches_movetype(movetype, &conds))
                    .unwrap_or(true);
                if let Some(script) = script.as_ref() {
                    script.apply(ps, movetype, id.0, &conds);
                }
                if requested_matches {
                    applied_mt.set(Some(movetype));
                }
            }
            if let Some(mt) = applied_mt.get() {
                world.set_last_anim_movetype(*id, mt);
            }
            world.set_pmove_walking(*id, walking);
            world.link_player_area(*id, linked_bounds);

            let shots = advance_weapon_command(&mut world, tick, *id, cmd, delta.min(200));
            for shot in shots {
                crate::missile::fire_accepted_shot(&mut world, tick, &shot);

                let hitbox_ids = [*id];
                let hitboxes = if world.publishes_snapshot() {
                    None
                } else {
                    Some(hitbox_ids.as_slice())
                };
                world.record_collision_history(tick, 0, hitboxes);
                if world.publishes_snapshot() {
                    world.record_entity_collision_history(tick);
                }
                let emissions = phase_emit(&world, core::slice::from_ref(&shot));
                phase_trace(&mut world, tick, &emissions);
            }
            crate::equipment::phase_offhand(&mut world, tick, &[(*id, cmd)]);
            world.set_old_cmd(*id, cmd.buttons, cmd.angles);
            consumed.push((*id, cmd));
        }
    }

    *world.old_buttons_mut() = original_buttons;
    *world.old_cmd_angles_mut() = original_angles;

    restamp_debug_move_look(&mut world, &input.actions);

    if allow_move && world.publishes_snapshot() {
        phase_stuck_in_client(&mut world);
    }
    drop(world);
    let mut request = ecs.resource_mut::<StepRequest>();
    if allow_move {
        request.input.cmds = consumed;
    }
}

fn record_collision_state_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let cmd_ids: Vec<crate::world::ClientId> = ecs
        .resource::<StepRequest>()
        .input
        .cmds
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RecordCollisionState);

    let hitbox_cmds = if world.publishes_snapshot() {
        None
    } else {
        Some(cmd_ids)
    };
    world.record_collision_history(tick, msec, hitbox_cmds.as_deref());
}

fn run_entity_types_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let mut world = frame_world(ecs);
    let allow_move = world.phase() == MatchPhase::Playing;
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunEntityTypes);
    if allow_move {
        phase_materialize_entity_dobjs(&mut world);

        if world.publishes_snapshot() {
            world.record_entity_collision_history(tick);
        }

        crate::entity_run::phase_run_entity_thinks(&mut world, tick);
        if world.publishes_snapshot() {
            let drain = world
                .world_objects_mut()
                .tick_vehicle_healthdrain(crate::MATCH_TICK_MS);
            world.world_objects_mut().glass_update(
                i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX),
            );
            let burn = world
                .world_objects_mut()
                .tick_explodable_barrel_burn(crate::MATCH_TICK_MS);
            let crate_burn = world
                .world_objects_mut()
                .tick_flammable_crate_burn(crate::MATCH_TICK_MS);
            let toy_drain = world
                .world_objects_mut()
                .tick_toy_healthdrain(crate::MATCH_TICK_MS);
            let mut drain_explodes = drain.explodes;
            drain_explodes.extend(burn.explodes);
            drain_explodes.extend(crate_burn.explodes);
            drain_explodes.extend(toy_drain.explodes);
            let drain_chain_intents = world
                .world_objects()
                .destructible_radius_intents(&drain_explodes);
            let drain_chain = world
                .world_objects_mut()
                .apply_destructible_damage_batch(&drain_chain_intents);
            drain_explodes.extend(drain_chain.explodes);
            for explode in &drain_explodes {
                apply_explosion_blast(
                    &mut world,
                    tick,
                    &ExplosionBlast::from_destructible(explode),
                );
                apply_explode_glass_blast(&mut world, tick, explode);
            }
            phase_health_regen(&mut world, tick);
            phase_finalstand_timer(&mut world, tick);
            emit_vehicle_fx_events(&mut world, tick);
            publish_destructible_loop_sounds(&mut world);
            phase_destructible_death_presentation(&mut world, msec);
            phase_animated_map_models(&mut world, tick, msec);
        } else {
            // A client presents the stages it adopted; the effects that go
            // with them arrive as entity events from the host.
            phase_destructible_death_presentation(&mut world, msec);
        }
    } else {
        crate::entity_run::phase_walk_entity_thinks(&mut world);
    }
}

fn dispatch_touches_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    let allow_move = world.phase() == MatchPhase::Playing;
    world.enter_kernel_phase(crate::gentity::KernelPhase::DispatchTouches);
    if allow_move {
        crate::item::phase_touch_items(&mut world, tick);
    }
    let command_buttons: Vec<_> = input
        .cmds
        .iter()
        .map(|(id, cmd)| (id.0, cmd.buttons))
        .collect();
    let latest: std::collections::BTreeMap<_, _> = input.cmds.iter().copied().collect();
    let latest_cmds: Vec<_> = latest.into_iter().collect();
    let cmds: Vec<_> = latest_cmds
        .iter()
        .map(|(id, cmd)| (id.0, cmd.buttons))
        .collect();
    let old: Vec<(u32, u32)> = world
        .old_buttons_mut()
        .iter()
        .map(|(id, bits)| (id.0, *bits))
        .collect();
    let presses = crate::collect_use_presses(&command_buttons, &old);
    crate::map_doors::advance(&mut world, tick, &latest_cmds);
    crate::map_lights::advance(&mut world, tick);
    crate::map_diggers::advance(&mut world, tick);
    crate::map_moving_diggers::advance(&mut world, tick);
    crate::map_conveyer::advance(&mut world);
    world.stamp_use_presses(presses.clone());
    if allow_move && world.publishes_snapshot() {
        crate::use_object::phase_use_objects(&mut world, tick, msec as u32, &presses, &cmds);
    }
    crate::objectives::advance(&mut world, tick, &cmds);
    if allow_move && world.publishes_snapshot() {
        crate::item::phase_use_items(&mut world, tick, &presses, &cmds);
    }
}

fn finalize_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let input = ecs.resource::<StepRequest>().input.clone();
    let reason = ecs.resource::<StepRequest>().reason;
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::Finalize);
    for &(id, cmd) in &input.cmds {
        emit_attack_events(&mut world, tick, &[(id, cmd)]);
        if world
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            world.set_old_cmd(id, cmd.buttons, cmd.angles);
        }
    }

    if reason.advances_authority_world() {
        crate::voice::tick_delayed(&mut world, tick);
        crate::damage::tick_delayed_concussion(&mut world, tick);
        crate::score::finish_recent_kills(&mut world, tick);
        crate::score::advance_match_clock(&mut world, tick);
    }

    let mut old_buttons = std::mem::take(world.old_buttons_mut());
    old_buttons.retain(|(id, _)| {
        world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    });
    *world.old_buttons_mut() = old_buttons;
    let mut old_angles = std::mem::take(world.old_cmd_angles_mut());
    old_angles.retain(|(id, _)| {
        world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    });
    *world.old_cmd_angles_mut() = old_angles;

    harvest_predictable_events(&mut world, tick);
    if reason.advances_authority_world() {
        world.tick_scripted_hud(tick);
    }
    world.script_gaps_mut().report();
}

fn publish_snapshot_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::PublishSnapshot);

    if world.publishes_snapshot() {
        emit_player_ticks(&world, tick);
    }
    let snapshot = if world.publishes_snapshot() {
        let snapshot = world.snapshot(tick);
        emit_snapshot_events(&world, &snapshot);
        snapshot
    } else {
        Snapshot::unpublished(tick)
    };
    drop(world);
    ecs.resource_mut::<StepRequest>().output = Some(snapshot);
}

fn emit_player_ticks(world: &FrameWorld, tick: Tick) {
    let bot_name = entity_iw4::pack_client_state_name("bot");
    let level_time_ms = crate::level_time_ms(tick);
    let mut rows = Vec::new();
    world.visit_players(|id, ps| {
        rows.push((
            id,
            ps.command_time,
            ps.origin,
            ps.velocity[2],
            ps.viewangles[1],
            ps.viewangles[0],
            ps.jump_time,
            ps.weaponstate_primary,
            ps.legs_anim,
            ps.torso_anim,
        ));
    });
    for (id, time_ms, origin, vz, yaw, pitch, jump_time, weaponstate, legs_anim, torso_anim) in rows
    {
        let Some(meta) = world.client_meta(id) else {
            continue;
        };
        perf::player_tick(
            id.0,
            meta.name == bot_name,
            time_ms,
            level_time_ms,
            origin,
            vz,
            yaw,
            pitch,
            jump_time,
            world.old_buttons(id).unwrap_or(0),
            weaponstate,
            meta.ammo_clip,
            world.pmove_walking(id),
            legs_anim,
            torso_anim,
            lifecycle_label(meta.lifecycle),
        );
    }
}

fn lifecycle_label(life: ClientLifecycle) -> &'static str {
    match life {
        ClientLifecycle::Connecting => "Connecting",
        ClientLifecycle::ChoosingClass => "ChoosingClass",
        ClientLifecycle::SpawnPending => "SpawnPending",
        ClientLifecycle::Alive => "Alive",
        ClientLifecycle::Dead => "Dead",
        ClientLifecycle::RespawnPending => "RespawnPending",
        ClientLifecycle::Spectating => "Spectating",
        ClientLifecycle::Intermission => "Intermission",
    }
}

fn emit_snapshot_events(world: &FrameWorld, snapshot: &Snapshot) {
    for row in world.world_objects().vehicle_dump_rows() {
        perf::truck(
            row.id.to_wire(),
            Some(i64::from(row.body.state_index)),
            Some(i64::from(row.body.health)),
            row.death_clip,
            None,
        );
    }
    for slot in &snapshot.meta.corpses.slots {
        perf::corpse(
            i64::from(slot.occupied),
            slot.occupied.then_some(slot.origin[2]),
            None,
            None,
        );
    }
    for es in snapshot
        .meta
        .entities
        .iter()
        .filter(|es| es.e_type == entity_iw4::ET_ITEM)
    {
        let ammo = snapshot
            .meta
            .item_ammo
            .iter()
            .find(|row| row.entnum == es.number);
        perf::item(
            es.e_type,
            ammo.map(|row| row.clip_r),
            ammo.map(|row| row.scavenger),
            None,
        );
    }
}

fn harvest_predictable_events(world: &mut FrameWorld, _tick: Tick) {
    world.for_each_player_mut(|_, ps| {
        let mut cursor = ps.old_event_sequence;
        consume_player_events(ps, &mut cursor, |_| {});
        ps.old_event_sequence = cursor;
    });
}

fn emit_attack_events(
    world: &mut FrameWorld,
    tick: Tick,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    for (id, cmd) in cmds {
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let old = world
            .old_buttons_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(0, |(_, b)| *b);
        let attack = cmd.buttons & buttons::ATTACK != 0;
        let was = old & buttons::ATTACK != 0;
        if !attack && was {
            world.push_event(tick, EventAudience::Client(*id), SimEvent::AttackReleased);
        }
    }
}

fn action_needs_player_row(action: &ClientAction) -> bool {
    !matches!(
        action,
        ClientAction::JoinMatch { .. }
            | ClientAction::LeaveMatch { .. }
            | ClientAction::SetName { .. }
            | ClientAction::UseCopycat { .. }
    )
}

fn apply_actions(world: &mut FrameWorld, tick: Tick, actions: &[(ClientId, ClientAction)]) {
    for (id, action) in actions {
        match *action {
            ClientAction::JoinMatch { request_id: _ } => {
                let meta = world.client_meta_mut(*id);
                if meta.lifecycle == ClientLifecycle::Connecting {
                    meta.lifecycle = ClientLifecycle::ChoosingClass;
                }
                assign_team(world, *id);
            }
            ClientAction::LeaveMatch { request_id: _ } => {
                world.retire_client(*id);
            }
            ClientAction::SetName {
                request_id: _,
                name,
            } => {
                world.client_meta_mut(*id).name = name;
            }
            ClientAction::UseCopycat { request_id: _ } => {
                apply_use_copycat(world, *id);
            }
            ClientAction::SelectClass {
                request_id,
                class_id,
                revision,
            } => {
                apply_select_class(world, tick, *id, request_id, class_id, revision);
            }
            ClientAction::GiveWeapon { request_id, weapon } => {
                apply_give_weapon(world, tick, *id, request_id, weapon);
            }
            ClientAction::ChangeWeaponConfiguration {
                request_id,
                from,
                to,
            } => {
                apply_configuration_change(world, tick, *id, request_id, from, to);
            }
            ClientAction::SpawnClient { request_id: _ } => {
                apply_spawn_client(world, *id);
            }
            ClientAction::ForceSpawn {
                request_id: _,
                pick,
            } => {
                if !world.bootstrap_ref().allow_debug_actions {
                    continue;
                }
                apply_force_spawn(world, *id, pick);
            }
            ClientAction::SpawnIntermission { request_id: _ } => {
                apply_spawn_intermission(world, *id);
            }
            ClientAction::ForceDeath { request_id: _ } => {
                if !world.bootstrap_ref().allow_debug_actions {
                    continue;
                }
                let life = {
                    let meta = world.client_meta_mut(*id);
                    if meta.lifecycle != ClientLifecycle::Alive {
                        continue;
                    }
                    let life = meta.life_sequence;
                    meta.lifecycle = ClientLifecycle::Dead;
                    meta.dead_since_tick = Some(tick.0);
                    life
                };
                world.unlink_player_area(*id);
                world.push_event(
                    tick,
                    EventAudience::All,
                    SimEvent::Died {
                        victim: *id,
                        life_sequence: life,
                        attacker: None,
                        attacker_life: None,
                        source: None,
                        weapon: 0,
                        killcam_entity_start_time: 0,
                    },
                );
                crate::score::apply_death_score(world, tick, *id, None, None);
                crate::damage::apply_player_killed(world, tick, *id, None, None);
                crate::damage::push_suicide_obituary(world, tick, *id);
            }
            ClientAction::SetMatchPhase {
                request_id: _,
                phase,
            } => {
                if !world.bootstrap_ref().allow_debug_actions {
                    continue;
                }
                if phase == MatchPhase::Playing && world.phase() == MatchPhase::Warmup {
                    crate::score::finish_prematch(world, tick);
                } else {
                    world.set_phase(phase);
                }
            }
            ClientAction::Move {
                request_id: _,
                origin,
                angles,
            } => {
                apply_debug_move(world, *id, origin, angles);
            }
            ClientAction::BeginScriptMoverRotateVelocity {
                request_id: _,
                speed,
            } => {
                if !world.bootstrap_ref().allow_debug_actions {
                    continue;
                }
                world.begin_script_movers_rotate_velocity_supplied(
                    speed,
                    crate::corpse::level_time_ms(tick),
                );
            }
            ClientAction::DebugDamage {
                request_id: _,
                amount,
            } => {
                apply_debug_damage(world, tick, *id, amount);
            }
        }
    }
}

fn apply_spawn_client(world: &mut FrameWorld, id: ClientId) {
    let meta = world.client_meta_mut(id);
    if meta.lifecycle != ClientLifecycle::Dead {
        return;
    }
    meta.lifecycle = ClientLifecycle::RespawnPending;
    meta.dead_since_tick = None;
}

fn apply_force_spawn(world: &mut FrameWorld, id: ClientId, pick: crate::SpawnPick) {
    let was_alive = {
        let meta = world.client_meta_mut(id);
        if meta.loadout.is_none() {
            diag::info!(
                Sim,
                "spawn: forced client={} refused — no class selected",
                id.0
            );
            return;
        }
        let was_alive = meta.lifecycle == ClientLifecycle::Alive;
        meta.lifecycle = ClientLifecycle::RespawnPending;
        meta.dead_since_tick = None;
        meta.forced_spawn = Some(pick);
        was_alive
    };
    if was_alive {
        world.unlink_player_area(id);
    }
}

fn apply_use_copycat(world: &mut FrameWorld, id: ClientId) {
    let meta = world.client_meta_mut(id);
    if meta.lifecycle != ClientLifecycle::Dead {
        return;
    }
    let Some(stash) = meta.copycat_loadout.as_mut() else {
        return;
    };
    stash.in_use = true;
}

fn apply_spawn_intermission(world: &mut FrameWorld, id: ClientId) {
    if world
        .client_meta(id)
        .is_none_or(|m| m.lifecycle == ClientLifecycle::Intermission)
    {
        return;
    }
    let view = world.bootstrap_ref().intermission_view.clone();
    {
        let meta = world.client_meta_mut(id);
        meta.lifecycle = ClientLifecycle::Intermission;
        meta.dead_since_tick = None;
    }
    world.unlink_player_area(id);
    let Some(view) = view else {
        return;
    };
    let Some(ps) = world.player_mut(id) else {
        return;
    };
    ps.origin = view.origin;
    ps.velocity = [0.0, 0.0, 0.0];
    ps.viewangles = view.angles;
    ps.delta_angles = packed_look_delta(view.angles);
    ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
}

fn apply_debug_move(world: &mut FrameWorld, id: ClientId, origin: [f32; 3], angles: [f32; 3]) {
    if !world.bootstrap_ref().allow_debug_actions {
        return;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    if !origin.iter().chain(angles.iter()).all(|v| v.is_finite()) {
        return;
    }
    let old_origin = {
        let Some(ps) = world.player_mut(id) else {
            return;
        };
        let old_origin = ps.origin;
        ps.origin = origin;
        ps.velocity = [0.0, 0.0, 0.0];
        ps.viewangles = angles;

        ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        ps.delta_angles = packed_look_delta(angles);
        old_origin
    };
    world.translate_player_area(
        id,
        [
            origin[0] - old_origin[0],
            origin[1] - old_origin[1],
            origin[2] - old_origin[2],
        ],
    );
}

fn packed_look_delta(viewangles: [f32; 3]) -> [f32; 3] {
    [
        viewangles[0] - (viewangles[0] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
        viewangles[1] - (viewangles[1] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
        viewangles[2] - (viewangles[2] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
    ]
}

fn restamp_debug_move_look(world: &mut FrameWorld, actions: &[(ClientId, ClientAction)]) {
    for (id, action) in actions {
        let ClientAction::Move { angles, .. } = *action else {
            continue;
        };
        if !world.bootstrap_ref().allow_debug_actions {
            continue;
        }
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let Some(ps) = world.player_mut(*id) else {
            continue;
        };
        ps.viewangles = angles;
        ps.delta_angles = packed_look_delta(angles);
    }
}

fn apply_debug_damage(world: &mut FrameWorld, tick: Tick, id: ClientId, amount: i32) {
    if !world.bootstrap_ref().allow_debug_actions {
        return;
    }
    if amount <= 0 {
        return;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    let health_after = {
        let Some(ps) = world.player_mut(id) else {
            return;
        };

        movement_iw4::pm_update_damage_timer(ps, amount, None);
        ps.health = (ps.health - amount).max(0);
        ps.damage_count = ps.damage_count.saturating_add(1);
        ps.damage_event = ps.damage_event.wrapping_add(1);
        ps.health
    };
    if health_after > 0 {
        return;
    }
    let life = {
        let meta = world.client_meta_mut(id);
        meta.lifecycle = ClientLifecycle::Dead;
        meta.dead_since_tick = Some(tick.0);
        meta.life_sequence
    };
    world.unlink_player_area(id);
    world.push_event(
        tick,
        EventAudience::All,
        SimEvent::Died {
            victim: id,
            life_sequence: life,
            attacker: None,
            attacker_life: None,
            source: None,
            weapon: 0,
            killcam_entity_start_time: 0,
        },
    );
    crate::score::apply_death_score(world, tick, id, None, None);
    crate::damage::apply_player_killed(world, tick, id, None, None);
    crate::damage::push_suicide_obituary(world, tick, id);
}

fn configuration_change_ammo(
    clip0: i32,
    clip1: i32,
    stock: i32,
    clip_capacity: i32,
    max_ammo: i32,
    dual: bool,
) -> (i32, i32, i32) {
    let total = clip0
        .max(0)
        .saturating_add(clip1.max(0))
        .saturating_add(stock.max(0));
    let cap = clip_capacity.max(0);
    let first = clip0.max(0).min(cap).min(total);
    let second = if dual {
        clip1.max(0).min(cap).min(total - first)
    } else {
        0
    };
    (first, second, (total - first - second).min(max_ammo.max(0)))
}

fn apply_configuration_change(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    from: u32,
    to: u32,
) {
    let reject = |world: &mut FrameWorld, reason: crate::ConfigurationChangeRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ConfigurationChangeRejected {
                request_id,
                from,
                to,
                reason,
            },
        );
    };
    use crate::ConfigurationChangeRejectReason as Reason;
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        reject(world, Reason::NotAlive);
        return;
    }
    let Some(ps) = world.player(id).copied() else {
        reject(world, Reason::NotAlive);
        return;
    };
    if ps.weapon != from {
        reject(world, Reason::StaleSource);
        return;
    }
    if !world.can_transition_weapon(from, to) {
        reject(world, Reason::DifferentFamily);
        return;
    }
    let (Some(old), Some(new)) = (world.combat_facts_for(from), world.combat_facts_for(to)) else {
        reject(world, Reason::InvalidTarget);
        return;
    };
    if world
        .equipment_facts_for(to)
        .is_some_and(|eq| eq.is_offhand())
    {
        reject(world, Reason::InvalidTarget);
        return;
    }
    if ps.weaponstate_primary != weapon_iw4::WeaponState::Ready as i32
        || ps.weapon_time > 0
        || (ps.last_weapon_hand >= 1
            && (ps.weaponstate_secondary != weapon_iw4::WeaponState::Ready as i32
                || ps.weapon_time_secondary > 0))
        || ps.weap_flags & playerstate_iw4::weap_flags::OFFHAND_VIEW != 0
    {
        reject(world, Reason::Busy);
        return;
    }
    let Some(slot) = ps.weapons.iter().position(|&weapon| weapon == from as i32) else {
        reject(world, Reason::NoInventorySlot);
        return;
    };
    if ps.weapons.contains(&(to as i32)) {
        reject(world, Reason::InvalidTarget);
        return;
    }
    let old_ammo_key = weapon_iw4::bg_ammo_table_key(old.ammo_index, from);
    let old_clip_key = weapon_iw4::bg_clip_table_key(old.clip_index, from);
    let new_ammo_key = weapon_iw4::bg_ammo_table_key(new.ammo_index, to);
    let new_clip_key = weapon_iw4::bg_clip_table_key(new.clip_index, to);
    let another_owns = |key: i32, clip: bool| {
        ps.weapons
            .iter()
            .filter(|&&weapon| weapon > 0 && weapon != from as i32)
            .any(|&weapon| {
                world.combat_facts_for(weapon as u32).is_some_and(|facts| {
                    key == if clip {
                        weapon_iw4::bg_clip_table_key(facts.clip_index, weapon as u32)
                    } else {
                        weapon_iw4::bg_ammo_table_key(facts.ammo_index, weapon as u32)
                    }
                })
            })
    };
    let clip0 = weapon_iw4::bg_get_clip_for_hand(&ps.ammoclip, old_clip_key, 0);
    let raw_clip1 = weapon_iw4::bg_get_clip_for_hand(&ps.ammoclip, old_clip_key, 1);
    let clip1 = if ps.last_weapon_hand >= 1 {
        raw_clip1
    } else {
        0
    };
    let stock = weapon_iw4::bg_get_ammo_not_in_clip(&ps.ammo, old_ammo_key);
    let akimbo = gsc_give_weapon_is_akimbo(world.weapon_script_name(to));
    let (next_clip0, next_clip1, next_stock) =
        configuration_change_ammo(clip0, clip1, stock, new.clip_size, new.max_ammo, akimbo);
    if (old_ammo_key != new_ammo_key
        && (another_owns(old_ammo_key, false) || another_owns(new_ammo_key, false)))
        || (old_clip_key != new_clip_key
            && (another_owns(old_clip_key, true) || another_owns(new_clip_key, true)))
        || (old_ammo_key == new_ammo_key
            && next_stock != stock
            && another_owns(old_ammo_key, false))
        || (old_clip_key == new_clip_key
            && (next_clip0 != clip0 || next_clip1 != raw_clip1)
            && another_owns(old_clip_key, true))
    {
        reject(world, Reason::SharedAmmoConflict);
        return;
    }
    let mut next = ps;
    next.weapons[slot] = to as i32;
    next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
    weapon_iw4::bg_latch_weapon_dual_wield(&next.weapons, &mut next.weapon_data, to, akimbo);
    next.weapon = to;
    next.last_weapon_hand = weapon_iw4::pm_num_hands_for_held(&next.weapons, &next.weapon_data, to);
    if old_ammo_key != new_ammo_key {
        for row in next.ammo.chunks_exact_mut(8) {
            if row[..4] == old_ammo_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if old_clip_key != new_clip_key {
        for row in next.ammoclip.chunks_exact_mut(12) {
            if row[..4] == old_clip_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if !weapon_iw4::bg_set_ammo_not_in_clip(&mut next.ammo, new_ammo_key, next_stock)
        || !weapon_iw4::bg_set_clip_for_hand(&mut next.ammoclip, new_clip_key, 0, next_clip0)
        || !weapon_iw4::bg_set_clip_for_hand(&mut next.ammoclip, new_clip_key, 1, next_clip1)
    {
        reject(world, Reason::AmmoTableFull);
        return;
    }
    let hand = weapon_iw4::spawn_weapon_hand(to, &new);
    next.weaponstate_primary = hand.weaponstate;
    next.weapon_time = hand.weapon_time;
    next.weapon_delay = hand.weapon_delay;
    next.weap_anim = hand.weap_anim;
    next.weaponstate_secondary = hand.weaponstate;
    next.weapon_time_secondary = hand.weapon_time;
    next.weapon_delay_secondary = hand.weapon_delay;
    next.weap_anim_secondary = hand.weap_anim;
    next.weapon_shot_count = 0;
    next.weapon_shot_count_secondary = 0;
    next.weap_flags &= !(playerstate_iw4::weap_flags::NO_ADS
        | playerstate_iw4::weap_flags::DOUBLEBARREL_RECOIL
        | playerstate_iw4::weap_flags::RECOIL_SCALE);
    *world.player_mut(id).expect("validated alive player") = next;
    let meta = world.client_meta_mut(id);
    let quick_reload_ready = meta.quick_reload_ready(from);
    meta.ammo_by_weapon.retain(|row| row.0 != from);
    meta.set_ammo(to, next_clip0, next_stock);
    meta.set_quick_reload_ready(from, true);
    meta.set_quick_reload_ready(to, quick_reload_ready);
    meta.mirror_held_ammo(to);
    meta.weapon_shot_count = 0;
    meta.burst_latch = false;
    meta.rechamber_pending = false;
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::ConfigurationChangeAccepted {
            request_id,
            from,
            to,
        },
    );
}

fn apply_give_weapon(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    weapon: u32,
) {
    let reject = |world: &mut FrameWorld, reason: crate::GiveRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveRejected {
                request_id,
                weapon,
                reason,
            },
        );
    };

    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        reject(world, crate::GiveRejectReason::NotAlive);
        return;
    }
    if weapon == 0 {
        reject(world, crate::GiveRejectReason::InvalidWeapon);
        return;
    }
    let table_len = world.weapon_combat_len();
    if (weapon as usize) >= table_len {
        reject(world, crate::GiveRejectReason::UnknownWeaponId);
        return;
    }
    if !world.weapon_runnable(weapon) {
        reject(world, crate::GiveRejectReason::UnsupportedWeapon);
        return;
    }
    let Some(facts) = world.combat_facts_for(weapon) else {
        reject(world, crate::GiveRejectReason::UnknownWeaponId);
        return;
    };
    if world
        .equipment_facts_for(weapon)
        .is_some_and(|eq| eq.is_offhand())
    {
        apply_give_offhand(world, tick, id, request_id, weapon, &facts);
        return;
    }
    if facts.fire_time_ms <= 0 && facts.raise_time_ms <= 0 {
        reject(world, crate::GiveRejectReason::EmptyCombatProfile);
        return;
    }

    if world.weapon_script_name(weapon).starts_with("killstreak_") {
        let Some(ps) = world.player_mut(id) else {
            reject(world, crate::GiveRejectReason::NotAlive);
            return;
        };
        inventory_add_weapon(ps, weapon, false);
        if !ps.weapons.contains(&(weapon as i32)) {
            reject(world, crate::GiveRejectReason::InvalidWeapon);
            return;
        }
        ps.action_slot_type[3] = 1;
        ps.action_slot_param[3] = weapon as i32;
        let (clip, clip_alt, stock) = weapon_iw4::spawn_clip_stock(&facts, 0);
        seed_ps_ammo_tables(ps, weapon, &facts, clip, clip_alt, false, stock);
        world.client_meta_mut(id).set_ammo(weapon, clip, stock);
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveAccepted { request_id, weapon },
        );
        return;
    }
    let akimbo = gsc_give_weapon_is_akimbo(world.weapon_script_name(weapon));
    let Some(mut next) = world.player(id).copied() else {
        reject(world, crate::GiveRejectReason::NotAlive);
        return;
    };

    let outgoing = next.weapon;
    let mut replaced = false;
    if !next.weapons.contains(&(weapon as i32)) {
        if let Some(slot) = next
            .weapons
            .iter()
            .position(|&w| w != 0 && w == outgoing as i32)
        {
            next.weapons[slot] = weapon as i32;
            next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
            replaced = true;
        }
    }
    if replaced {
        let outgoing_facts = world
            .combat_facts_for(outgoing)
            .expect("held weapon combat facts");

        for (table, stride, clip) in [
            (&mut next.ammo[..], 8, false),
            (&mut next.ammoclip[..], 12, true),
        ] {
            let key = if clip {
                weapon_iw4::bg_clip_table_key(outgoing_facts.clip_index, outgoing)
            } else {
                weapon_iw4::bg_ammo_table_key(outgoing_facts.ammo_index, outgoing)
            };
            let owned = next.weapons.iter().filter(|&&w| w > 0).any(|&w| {
                world.combat_facts_for(w as u32).is_some_and(|f| {
                    key == if clip {
                        weapon_iw4::bg_clip_table_key(f.clip_index, w as u32)
                    } else {
                        weapon_iw4::bg_ammo_table_key(f.ammo_index, w as u32)
                    }
                })
            });
            if !owned {
                for row in table.chunks_exact_mut(stride) {
                    if row[..4] == key.to_le_bytes() {
                        row.fill(0);
                    }
                }
            }
        }
    }
    give_weapon_to_ps_akimbo(&mut next, weapon, akimbo);
    *world.player_mut(id).expect("validated alive player") = next;
    let (clip, stock) = if let Some(ps_mut) = world.player_mut(id) {
        arm_held_weapon(ps_mut, weapon, &facts)
    } else {
        (0, 0)
    };
    let meta = world.client_meta_mut(id);
    if replaced {
        meta.ammo_by_weapon.retain(|row| row.0 != outgoing);
        meta.set_quick_reload_ready(outgoing, true);
    }
    meta.set_ammo(weapon, clip, stock);
    meta.set_quick_reload_ready(weapon, true);
    meta.mirror_held_ammo(weapon);
    meta.weapon_shot_count = 0;
    meta.burst_latch = false;
    meta.rechamber_pending = false;
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::GiveAccepted { request_id, weapon },
    );
}

fn apply_give_offhand(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) {
    let reject = |world: &mut FrameWorld, reason: crate::GiveRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveRejected {
                request_id,
                weapon,
                reason,
            },
        );
    };
    let Some(eq) = world.equipment_facts_for(weapon) else {
        reject(world, crate::GiveRejectReason::InvalidWeapon);
        return;
    };
    let Some(mut next) = world.player(id).copied() else {
        reject(world, crate::GiveRejectReason::NotAlive);
        return;
    };
    inventory_add_weapon(&mut next, weapon, false);
    if !next.weapons.contains(&(weapon as i32)) {
        reject(world, crate::GiveRejectReason::InvalidWeapon);
        return;
    }
    match eq.offhand_class {
        1 | 4 | 5 => next.offhand_primary = eq.offhand_class,
        2 | 3 => next.offhand_secondary = eq.offhand_class,
        _ => {}
    }
    let clip = eq.spawn_clip_count();
    seed_ps_ammo_tables(&mut next, weapon, facts, clip, 0, false, 0);
    *world.player_mut(id).expect("validated alive player") = next;
    {
        let meta = world.client_meta_mut(id);
        meta.set_ammo(weapon, clip, 0);
        if let Some(loadout) = meta.loadout.as_mut() {
            match eq.offhand_class {
                1 | 4 | 5 => loadout.lethal = weapon,
                2 | 3 => loadout.tactical = weapon,
                _ => {}
            }
        }
    }
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::GiveAccepted { request_id, weapon },
    );
}

fn apply_select_class(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    class_id: crate::ClassId,
    revision: u32,
) {
    let accepted = world
        .bootstrap_ref()
        .class(class_id)
        .filter(|def| def.revision == revision)
        .cloned();
    let Some(def) = accepted else {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason=unknown_or_stale_class",
            id.0,
            class_id.0,
            revision
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason: crate::ClassRejectReason::UnknownOrStaleClass,
            },
        );
        return;
    };

    if def.locked {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason=locked_content",
            id.0,
            class_id.0,
            revision
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason: crate::ClassRejectReason::LockedContent,
            },
        );
        return;
    }

    if let Err(reason) = validate_class_content(world, &def) {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason={}",
            id.0,
            class_id.0,
            revision,
            reason.as_str()
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason,
            },
        );
        return;
    }

    if world.bootstrap_ref().spawns.is_empty() {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason=no_spawn_available (empty pool)",
            id.0,
            class_id.0,
            revision
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason: crate::ClassRejectReason::NoSpawnAvailable,
            },
        );
        return;
    }

    let loadout = LoadoutSpec {
        class_id: def.id,
        revision: def.revision,
        primary: def.primary,
        secondary: def.secondary,
        primary_attachments: def.primary_attachments,
        secondary_attachments: def.secondary_attachments,
        lethal: def.lethal,
        tactical: def.tactical,
        perks: def.perks,
    };

    {
        let meta = world.client_meta_mut(id);
        meta.loadout = Some(loadout);
        meta.dead_since_tick = None;
        meta.lifecycle = ClientLifecycle::SpawnPending;
    }
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::ClassAccepted {
            request_id,
            class_id,
            revision,
        },
    );
    let _ = world.ensure_player(id);
    assign_team(world, id);
}

fn assign_team(world: &mut FrameWorld, id: ClientId) {
    if world.bootstrap_ref().kind.is_team() {
        assign_session_team(world, id);
    } else {
        assign_ffa_team(world, id);
    }
}

fn assign_ffa_team(world: &mut FrameWorld, id: ClientId) {
    world.client_meta_mut(id).client_state_team = entity_iw4::TEAM_FREE;
    if world.client_meta(id).is_some_and(|m| m.ffa_team.is_some()) {
        return;
    }
    let pick = {
        let rng = world.spawn_rng_mut();
        rng.next_index(2) as u8
    };
    world.client_meta_mut(id).ffa_team = Some(pick);
}

fn assign_session_team(world: &mut FrameWorld, id: ClientId) {
    let sticky = world
        .client_meta(id)
        .map(|m| m.client_state_team)
        .unwrap_or(entity_iw4::TEAM_FREE);
    if sticky == entity_iw4::TEAM_AXIS || sticky == entity_iw4::TEAM_ALLIES {
        world.client_meta_mut(id).ffa_team = None;
        return;
    }
    let session = count_players_assignment(world, id);
    let meta = world.client_meta_mut(id);
    meta.client_state_team = entity_iw4::client_state_team_from_sessionteam(true, session);
    meta.ffa_team = None;
}

fn count_players_assignment(world: &mut FrameWorld, self_id: ClientId) -> &'static str {
    let mut allies = 0u32;
    let mut axis = 0u32;
    for cid in world.client_ids_sorted() {
        if cid == self_id {
            continue;
        }
        let Some(m) = world.client_meta(cid) else {
            continue;
        };
        let pers = if m.client_state_team == entity_iw4::TEAM_ALLIES {
            Some("allies")
        } else if m.client_state_team == entity_iw4::TEAM_AXIS {
            Some("axis")
        } else {
            None
        };
        gamemode_iw4::count_players_inc(&mut allies, &mut axis, pers);
    }
    let scores = world.team_scores();
    match gamemode_iw4::team_assignment_from_counts(allies, axis, scores.allies, scores.axis) {
        gamemode_iw4::TeamAssignment::Allies => "allies",
        gamemode_iw4::TeamAssignment::Axis => "axis",
        gamemode_iw4::TeamAssignment::CoinToss => {
            let pick = world.spawn_rng_mut().next_index(2);
            ["allies", "axis"][pick]
        }
    }
}

fn validate_class_content(
    world: &FrameWorld,
    def: &crate::ClassDef,
) -> Result<(), crate::ClassRejectReason> {
    let table_len = world.weapon_combat_len();
    if table_len == 0 {
        return Ok(());
    }

    for id in [def.primary, def.secondary] {
        if id == 0 {
            continue;
        }
        if (id as usize) >= table_len {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        }
        let Some(row) = world.weapon_combat_row(id) else {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        };
        if !row.is_usable() {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }
    for id in [def.lethal, def.tactical] {
        if id == 0 {
            continue;
        }
        if (id as usize) >= table_len {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        }
        if world.offhand_loadout_row(id).is_none() {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }

    if def.primary_attachments.iter().any(|&a| a != 0)
        || def.secondary_attachments.iter().any(|&a| a != 0)
    {
        return Err(crate::ClassRejectReason::LockedContent);
    }

    Ok(())
}

fn advance_death_timers(world: &mut FrameWorld, tick: Tick) {
    if world.bootstrap_ref().host_owns_respawn {
        return;
    }
    let delay = world.bootstrap_ref().respawn_delay_ticks;
    let ids = world.client_ids_sorted();
    for id in ids {
        let meta = world.client_meta_mut(id);
        if meta.lifecycle != ClientLifecycle::Dead {
            continue;
        }
        let Some(since) = meta.dead_since_tick else {
            meta.dead_since_tick = Some(tick.0);
            continue;
        };
        if tick.0.saturating_sub(since) >= delay {
            meta.lifecycle = ClientLifecycle::RespawnPending;
            meta.dead_since_tick = None;
        }
    }
}

fn log_forced_spawn(
    id: ClientId,
    pick: crate::SpawnPick,
    report: &crate::spawn::SpawnAttemptReport,
) {
    let asked = match pick {
        crate::SpawnPick::Seeded(seed) => format!("seed={seed}"),
        crate::SpawnPick::At { origin, yaw } => format!(
            "at=[{:.1}, {:.1}, {:.1}] yaw={yaw:.1}",
            origin[0], origin[1], origin[2]
        ),
    };
    let fallback = report
        .rejected
        .first()
        .filter(|(source, _)| *source == crate::spawn::FORCED_SPAWN_SOURCE)
        .map(|(_, reason)| format!(" fallback={}", reason.as_str()))
        .unwrap_or_default();
    match &report.accepted {
        Some(d) => diag::info!(
            Sim,
            "spawn: forced client={} {asked} origin=[{:.1}, {:.1}, {:.1}] yaw={:.1} source={} tried={}{fallback}",
            id.0,
            d.traced_origin[0],
            d.traced_origin[1],
            d.traced_origin[2],
            d.raw_angles[1],
            if d.source_index == crate::spawn::FORCED_SPAWN_SOURCE {
                String::from("at")
            } else {
                d.source_index.to_string()
            },
            report.tried
        ),
        None => diag::info!(
            Sim,
            "spawn: forced client={} {asked} refused tried={}{fallback}",
            id.0,
            report.tried
        ),
    }
}

fn resolve_pending_spawns(world: &mut FrameWorld, tick: Tick) {
    if matches!(
        world.phase(),
        MatchPhase::Intermission | MatchPhase::PostGame
    ) {
        return;
    }

    let pending: Vec<ClientId> = world
        .client_ids_sorted()
        .into_iter()
        .filter(|id| {
            world.client_meta(*id).is_some_and(|m| {
                matches!(
                    m.lifecycle,
                    ClientLifecycle::SpawnPending | ClientLifecycle::RespawnPending
                )
            })
        })
        .collect();

    if pending.is_empty() {
        return;
    }

    let mut avoid = world.alive_origins();

    for id in pending {
        let client_state_team = world
            .client_meta(id)
            .map(|m| m.client_state_team)
            .unwrap_or(entity_iw4::TEAM_FREE);
        let forced = world.client_meta_mut(id).forced_spawn.take();
        let report = if let Some(pick) = forced {
            let report = crate::spawn::decide_forced_spawn(world, pick, &avoid, client_state_team);
            log_forced_spawn(id, pick, &report);
            report
        } else {
            let mut rng = crate::identities::MatchRng::new(0);
            core::mem::swap(&mut rng, world.spawn_rng_mut());
            let report = decide_spawn_seeded_report(world, &mut rng, &avoid, client_state_team);
            core::mem::swap(&mut rng, world.spawn_rng_mut());
            report
        };

        let Some(decision) = report.accepted else {
            let (class_id, revision, request_id) = {
                let meta = world.client_meta(id);
                let (class_id, revision) = meta
                    .and_then(|m| m.loadout.as_ref().map(|l| (l.class_id, l.revision)))
                    .unwrap_or((crate::ClassId(0), 0));
                let request_id = world
                    .journal()
                    .iter()
                    .rev()
                    .find_map(|record| match record.event {
                        SimEvent::ClassAccepted { request_id, .. }
                            if record.audience.projects_to(id) =>
                        {
                            Some(request_id)
                        }
                        _ => None,
                    })
                    .unwrap_or(0);
                (class_id, revision, request_id)
            };
            let summary = if report.rejected.is_empty() {
                SpawnReject::NoAuthoredCandidates.as_str().to_owned()
            } else {
                let mut parts = Vec::new();
                for reason in [
                    SpawnReject::NoGroundHit,
                    SpawnReject::StartSolid,
                    SpawnReject::Occupied,
                    SpawnReject::UnsupportedCoverage,
                    SpawnReject::AllRejected,
                    SpawnReject::NoAuthoredCandidates,
                ] {
                    let n = report.rejected.iter().filter(|(_, r)| *r == reason).count();
                    if n > 0 {
                        parts.push(format!("{}×{}", reason.as_str(), n));
                    }
                }
                parts.join(", ")
            };
            diag::info!(
                Sim,
                "spawn: refused client={} class={} tried={} — {summary}",
                id.0,
                class_id.0,
                report.tried
            );
            world.client_meta_mut(id).lifecycle = ClientLifecycle::ChoosingClass;
            world.push_event(
                tick,
                EventAudience::Client(id),
                SimEvent::ClassRejected {
                    request_id,
                    class_id,
                    revision,
                    reason: crate::ClassRejectReason::NoSpawnAvailable,
                },
            );
            continue;
        };
        if forced.is_none() {
            diag::info!(
                Sim,
                "spawn: grounded client={} at [{:.1}, {:.1}, {:.1}] source={} tried={}",
                id.0,
                decision.traced_origin[0],
                decision.traced_origin[1],
                decision.traced_origin[2],
                decision.source_index,
                report.tried
            );
        }

        let (loadout, using_copycat) = world.client_meta_mut(id).take_spawn_loadout();
        let class_id = loadout.class_id;

        let mut ps = spawn_player_state(decision.traced_origin, decision.raw_angles);
        if let Some(cmd) = world.old_cmd_angles(id) {
            ps.delta_angles = std::array::from_fn(|axis| {
                decision.raw_angles[axis] - cmd[axis] as f32 * SHORT2ANGLE
            });
        }
        ps.perks = crate::match_state::perk_bits_from_class_catalog(loadout.perks);
        ps.move_speed_scale_multiplier = gamemode_iw4::lightweight_move_speed_scale(
            crate::match_state::class_catalog_has(loadout.perks, crate::CLASS_CATALOG_LIGHTWEIGHT),
        );
        ps.perk_slots = [0; 8];
        for id in loadout.perks {
            if let Some(slot) = crate::match_state::perk_slot_from_class_catalog(id) {
                ps.perk_slots[slot] = crate::match_state::perk_table_code_from_class_catalog(id);
            }
        }
        if loadout.primary != 0 {
            let akimbo = gsc_give_weapon_is_akimbo(world.weapon_script_name(loadout.primary));
            give_weapon_to_ps_akimbo(&mut ps, loadout.primary, akimbo);
        }
        if loadout.secondary != 0 {
            let akimbo = gsc_give_weapon_is_akimbo(world.weapon_script_name(loadout.secondary));
            inventory_add_weapon(&mut ps, loadout.secondary, akimbo);
        }
        for equipment in [loadout.lethal, loadout.tactical] {
            if equipment != 0 && !ps.weapons.iter().any(|&slot| slot == equipment as i32) {
                if let Some(slot) = ps.weapons.iter_mut().find(|slot| **slot == 0) {
                    *slot = equipment as i32;
                }
            }
        }
        if let Some(eq) = world.offhand_loadout_row(loadout.lethal) {
            ps.offhand_primary = eq.offhand_class;
        }
        if let Some(eq) = world.offhand_loadout_row(loadout.tactical) {
            ps.offhand_secondary = eq.offhand_class;
        }

        let prev_teleport = world
            .player(id)
            .map(|p| p.e_flags & playerstate_iw4::eflags::TELEPORT)
            .unwrap_or(0);
        ps.e_flags = (ps.e_flags & !playerstate_iw4::eflags::TELEPORT) | prev_teleport;
        ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        ps.e_flags |= crate::match_state::class_catalog_radar_jam_e_flags(loadout.perks);
        *world.ensure_player(id) = ps;
        avoid.push(decision.traced_origin);

        let facts = world
            .combat_facts_for(loadout.primary)
            .unwrap_or_else(weapon_iw4::WeaponCombatFacts::none);
        let (clip, stock) = if let Some(ps_mut) = world.player_mut(id) {
            arm_held_weapon(ps_mut, loadout.primary, &facts)
        } else {
            (0, 0)
        };

        let secondary_facts = if loadout.secondary != 0 {
            world
                .combat_facts_for(loadout.secondary)
                .unwrap_or_else(weapon_iw4::WeaponCombatFacts::none)
        } else {
            weapon_iw4::WeaponCombatFacts::none()
        };
        let lethal_ammo = world
            .offhand_loadout_row(loadout.lethal)
            .map(crate::equipment::EquipmentRuntimeFacts::spawn_clip_count)
            .unwrap_or(0);
        let tactical_ammo = world
            .offhand_loadout_row(loadout.tactical)
            .map(crate::equipment::EquipmentRuntimeFacts::spawn_clip_count)
            .unwrap_or(0);

        let lethal_combat = world.weapon_combat_row(loadout.lethal);
        let tactical_combat = world.weapon_combat_row(loadout.tactical);
        if let Some(ps_mut) = world.player_mut(id) {
            if loadout.lethal != 0 {
                if let Some(facts) = lethal_combat.as_ref() {
                    seed_ps_ammo_tables(ps_mut, loadout.lethal, facts, lethal_ammo, 0, false, 0);
                }
            }
            if loadout.tactical != 0 {
                if let Some(facts) = tactical_combat.as_ref() {
                    seed_ps_ammo_tables(
                        ps_mut,
                        loadout.tactical,
                        facts,
                        tactical_ammo,
                        0,
                        false,
                        0,
                    );
                }
            }
        }

        let max_health = world.player(id).map(|p| p.max_health).unwrap_or(0);
        let now_ms = crate::hudelem::hud_level_time_ms(tick);
        let deathstreak = if using_copycat {
            String::from(gamemode_iw4::COPYCAT_PERK)
        } else {
            world
                .bootstrap_ref()
                .class(class_id)
                .map(|c| c.deathstreak.clone())
                .unwrap_or_default()
        };
        let life_sequence = {
            let meta = world.client_meta_mut(id);
            meta.life_sequence = meta.life_sequence.next();
            let life_sequence = meta.life_sequence;
            meta.lifecycle = ClientLifecycle::Alive;
            meta.dead_since_tick = None;
            meta.item_use_spawn_ms = crate::corpse::level_time_ms(tick);
            meta.item_use_entity = None;
            meta.clear_ammo_inventory();
            meta.set_ammo(loadout.primary, clip, stock);
            if loadout.secondary != 0 {
                let sec = weapon_iw4::spawn_weapon_hand(loadout.secondary, &secondary_facts);
                meta.set_ammo(loadout.secondary, sec.clip, sec.stock);
            }
            meta.set_ammo(loadout.lethal, lethal_ammo, 0);
            meta.set_ammo(loadout.tactical, tactical_ammo, 0);
            meta.mirror_held_ammo(loadout.primary);
            meta.weapon_shot_count = 0;
            meta.burst_latch = false;
            meta.rechamber_pending = false;
            meta.health_regen = gamemode_iw4::PlayerHealthRegenState::spawned(max_health);
            meta.last_named_sound = gamemode_iw4::HealthRegenSound::None;
            meta.combathigh_until_ms =
                gamemode_iw4::combathigh_until_ms(&deathstreak, meta.cur_death_streak, now_ms);
            meta.pistoldeath_this_life =
                gamemode_iw4::finalstand_should_give(&deathstreak, meta.cur_death_streak);
            meta.copycat_this_life =
                gamemode_iw4::copycat_should_give(&deathstreak, meta.cur_death_streak);
            meta.copycat_class_this_life = using_copycat;
            meta.laststand_until_ms = None;
            life_sequence
        };
        if world
            .client_meta(id)
            .is_some_and(|m| m.pistoldeath_this_life)
        {
            if let Some(ps) = world.player_mut(id) {
                ps.perks[0] |= playerstate_iw4::PERK_PISTOLDEATH;
            }
        }
        world.push_player_card_open(id, hud_iw4::SCRIPT_MENU_KILLEDBY_HIDE);
        world.push_player_card_open(id, hud_iw4::SCRIPT_MENU_PERK_DISPLAY);
        world.push_spawn_music(id);
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::Spawned {
                class_id,
                spawn_index: decision.source_index as u32,
                life_sequence,
            },
        );
        world.link_player_standing_area(id);
    }
}

struct PlayerBodyClip {
    entnum: u16,
    origin: [f32; 3],
}

fn alive_body_clips(world: &FrameWorld) -> Vec<PlayerBodyClip> {
    let mut bodies: Vec<_> = world
        .client_ids_sorted()
        .into_iter()
        .filter(|id| {
            world
                .client_meta(*id)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        })
        .filter_map(|id| {
            world.player(id).map(|ps| PlayerBodyClip {
                entnum: id.0 as u16,
                origin: ps.origin,
            })
        })
        .collect();
    bodies.extend(
        world
            .prediction_remote_bodies()
            .iter()
            .map(|body| PlayerBodyClip {
                entnum: body.client.0 as u16,
                origin: body.origin,
            }),
    );
    bodies
}

#[derive(Clone, Copy)]
struct ClipBackend<'a> {
    brushes: &'a [SimBrush],
    bsp: &'a SimClipBsp,
    mesh: &'a SimClipMesh,
    glass_damage: &'a [(u32, u16)],
    bodies: &'a [PlayerBodyClip],
    self_entnum: u16,
    cmodels: &'a [clipmap_iw4::ClipCmodel],
    linked_brushes: &'a [LinkedBrushCollisionBrush],
}

impl CollisionBackend for ClipBackend<'_> {
    fn trace(&self, input: GroundTraceInput) -> Trace {
        let world_hit = clip_trace(
            self.brushes,
            self.bsp,
            self.mesh,
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
            &|piece| {
                let damage = self
                    .glass_damage
                    .iter()
                    .find(|(id, _)| *id == u32::from(piece))
                    .map_or(0, |(_, d)| *d);
                crate::world_objects::glass_piece_is_solid(damage)
            },
        );
        let with_bmodels = clip_move_to_bmodels(
            world_hit,
            self.cmodels,
            &self.bsp.leafbrushes,
            self.brushes,
            self.linked_brushes,
            input,
        );
        clip_move_to_players(with_bmodels, self.bodies, self.self_entnum, input)
    }
}

fn clip_move_to_players(
    mut hit: Trace,
    bodies: &[PlayerBodyClip],
    self_entnum: u16,
    input: GroundTraceInput,
) -> Trace {
    if hit.fraction == 0.0 {
        return hit;
    }
    for body in bodies {
        if body.entnum == self_entnum {
            continue;
        }
        let other = clipmap_iw4::transformed_temp_capsule_trace(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            body.origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            crate::world::CONTENTS_BODY,
            input.tracemask,
        );
        if other.fraction >= hit.fraction && other.startsolid == 0 && other.allsolid == 0 {
            continue;
        }
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            hit = other;
            hit.hit_type = HITTYPE_ENTITY;
            hit.hit_id = body.entnum;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
    }
    hit
}

fn pmove_context(
    old_buttons: u32,
    weapon_scales: (f32, f32, f32),
    ads_allowed: bool,
    aim_down_sight: bool,
    ads_in_rate: f32,
    ads_out_rate: f32,
    rechamber_while_ads: bool,
    ads_fire_only: bool,
    melee_delay_ms: i32,
    melee_charge_delay_ms: i32,
    overlay_reticle: i32,
    shellshock_affects_movement: bool,
) -> PmoveSingleContext {
    let player_sprint_time = 4.0_f32;
    let weapon_max_sprint_time = bg_get_max_sprint_time(weapon_scales.2, player_sprint_time);

    let air = AirMoveContext {
        player_spectate_speed_scale: 1.0,
        shellshock_gravity_scale: 1.0,
        shellshock_gravity_bias: 0.0,
    };
    PmoveSingleContext {
        walk: WalkMoveContext {
            cmd_scale: CmdScaleWalkContext {
                player_back_speed_scale: 0.7,
                player_strafe_speed_scale: 0.8,
                player_sprint_speed_scale: 1.5,
                player_last_stand_crawl_speed_scale: 0.15,
                weapon_move_speed_scale: weapon_scales.0,
                weapon_ads_move_speed_scale: weapon_scales.1,

                shellshock_affects_movement,
            },
            weapon_move_scale: 1.0,
            old_buttons,
            jump: JumpLaunchContext {
                jump_height: 39.0,
                dive: false,
                crouch_jump_scale: 1.0,
                jump_ladder_push_vel: 128.0,
            },
            air,
        },
        air,
        bounds: MoveBounds {
            mins: [-15.0, -15.0, 0.0],
            maxs: [15.0, 15.0, 70.0],
            tracemask: 0x0281_0011,
        },
        view_angles: ViewAngleClamp {
            pitch_up: 85.0,
            pitch_down: 85.0,
            unclamped_pitch_bit: false,
        },
        sprint: SprintContext {
            weapon_max_sprint_time,
            sprint_forever: false,
            min_sprint_time_seconds: 1.0,
            sprint_delay_seconds: 0.0,

            sprint_forward_minimum: 105,
            stand_up_clear: true,
            sprint_recharge_pause_seconds: 0.0,
        },
        ads_intent: movement_iw4::AdsIntentContext {
            ads_allowed,
            weapon_def_scope: overlay_reticle != 0,
            sprint_hold_ads: false,
        },
        ads_frac: movement_iw4::AdsFracContext {
            aim_down_sight,
            ads_in_rate,
            ads_out_rate,
            rechamber_while_ads,
            ads_fire_only,
        },
        melee_charge: movement_iw4::MeleeChargeWeaponDelays {
            melee_delay_ms,
            melee_charge_delay_ms,
        },
        player_melee_range: movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
        old_buttons,
        weapon_blocks_prone: false,
    }
}

fn seed_ps_ammo_tables(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
    clip: i32,
    clip_alt: i32,
    dual: bool,
    stock: i32,
) {
    let ammo_index = weapon_iw4::bg_ammo_table_key(facts.ammo_index, weapon);
    let clip_index = weapon_iw4::bg_clip_table_key(facts.clip_index, weapon);
    if ammo_index != 0 {
        let _ = weapon_iw4::bg_set_ammo_not_in_clip(&mut ps.ammo, ammo_index, stock);
    }
    if clip_index != 0 {
        let _ = weapon_iw4::bg_set_clip_for_hand(&mut ps.ammoclip, clip_index, 0, clip);
        if dual {
            let _ = weapon_iw4::bg_set_clip_for_hand(&mut ps.ammoclip, clip_index, 1, clip_alt);
        }
    }
}

fn arm_held_weapon(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) -> (i32, i32) {
    let last_hand = weapon_iw4::pm_num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon);
    ps.last_weapon_hand = last_hand;
    let (clip0, clip1, stock) = weapon_iw4::spawn_clip_stock(facts, last_hand);
    let hand = weapon_iw4::spawn_weapon_hand(weapon, facts);
    ps.weaponstate_primary = hand.weaponstate;
    ps.weapon_time = hand.weapon_time;
    ps.weapon_delay = hand.weapon_delay;
    ps.weap_anim = hand.weap_anim;
    if last_hand >= 1 {
        ps.weaponstate_secondary = hand.weaponstate;
        ps.weapon_time_secondary = hand.weapon_time;
        ps.weapon_delay_secondary = hand.weapon_delay;
        ps.weap_anim_secondary = hand.weap_anim;
    }
    seed_ps_ammo_tables(ps, weapon, facts, clip0, clip1, last_hand >= 1, stock);
    (clip0, stock)
}
