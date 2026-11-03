use crate::bg::player_collision::{PlayerBodyClip, PlayerCollision};
use crate::frame::FrameWorld;

use bevy_ecs::prelude::{Resource, World};
use movement_iw4::{ANGLE2SHORT, SHORT2ANGLE, consume_player_events};

use crate::bullet_collision::{LinkedBrushCollisionBrush, PLAYER_MAXS, PLAYER_MINS};
use crate::combat::{advance_weapon_command, phase_emit, phase_trace};
use crate::identities::MatchPhase;
use crate::input::{ClientAction, TickInput};
use crate::match_state::{ClientLifecycle, EventAudience, SimEvent};
use crate::snapshot::Snapshot;
use crate::world::{
    ClientId, SimState, Tick, give_weapon_to_ps_akimbo, gsc_give_weapon_is_akimbo,
    inventory_add_weapon,
};
use crate::{TickEffects, TickResult};
use playerstate_iw4::PlayerState;
use playerstate_iw4::buttons;

#[derive(Resource)]
pub(crate) struct StepRequest {
    pub(crate) tick: Tick,
    pub(crate) input: TickInput,
    msec: i32,

    output: Option<TickResult>,
}

pub(crate) fn run_frame(
    ecs: &mut World,
    tick: Tick,
    input: &TickInput,
    msec: i32,
) -> Result<TickResult, crate::script::Fault> {
    crate::script::preflight(ecs, tick)?;
    assert!(
        !ecs.contains_resource::<StepRequest>(),
        "simulation already has a pending frame"
    );
    ecs.insert_resource(StepRequest {
        tick,
        input: input.clone(),
        msec: msec.clamp(1, 200),

        output: None,
    });
    advance_time_system(ecs);
    expire_transient_events_system(ecs);
    crate::script::apply_disconnects(ecs);
    crate::script::advance_mechanics(ecs);
    crate::script::advance_scheduler(ecs);
    if ecs.resource::<crate::script::RoundScript>().fault.is_none() {
        apply_script_signals_system(ecs);
        apply_actions_system(ecs);
        crate::script::sync_players(ecs);
        crate::script::sync_presence(ecs);
        run_players_system(ecs);
        crate::script::publish_projectile_launches(ecs);
        record_collision_state_system(ecs);
        run_entity_types_system(ecs);
        dispatch_touches_system(ecs);
        crate::script::sync_engine_events(ecs);
        finalize_system(ecs);
        crate::script::host::natives::iw4::deliver_local_presentation_dvars(ecs);
        publish_snapshot_system(ecs);
    }
    let request = ecs
        .remove_resource::<StepRequest>()
        .expect("simulation request missing");
    if let Some(fault) = ecs.resource::<crate::script::RoundScript>().fault.as_ref() {
        return Err(fault.clone());
    }
    Ok(request
        .output
        .expect("simulation frame did not publish a snapshot"))
}

fn frame_world(world: &mut World) -> FrameWorld<'_> {
    crate::frame::FrameWorld::from_world(world)
}

fn apply_script_signals_system(world: &mut World) {
    let signals = crate::script::take_signals(world);
    if signals.is_empty() {
        return;
    }
    let mut frame = frame_world(world);
    for signal in signals {
        match &*signal {
            crate::script::EXIT_LEVEL
                if !matches!(
                    frame.phase(),
                    MatchPhase::Intermission | MatchPhase::PostGame
                ) =>
            {
                frame.set_phase(MatchPhase::Intermission);
            }
            crate::script::MAP_RESTART => {
                let tick = frame.ecs().resource::<StepRequest>().tick;
                crate::script::restart_level(frame.ecs(), tick);
            }
            name => crate::script::host::iw4_gametype::apply_level_notify(&mut frame, name),
        }
    }
}

pub fn phase_materialize_entity_dobjs(world: &mut SimState) {
    for capabilities in world.entity_collision_capabilities_mut() {
        if let Some(dobj) = &mut capabilities.dobj {
            dobj.materialize();
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
        if let Some(id) = capabilities.owner.script_model()
            && let Some((_, apos)) = mover_apos.iter().find(|(mover_id, _)| *mover_id == id)
        {
            dobj.apply_trajectory_apos_at(*apos, at_time);
            continue;
        }
        if dobj.apos.is_some() {
            dobj.apply_apos_at(at_time);
        }
    }
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
    let samples = world.resource::<StepRequest>().input.shot_samples.clone();
    frame_world(world).set_lagcomp_commands(samples);
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
    for (client, action) in &actions {
        let applied = apply_action(&mut frame, tick, *client, action);
        perf::counter!("client_actions_processed", "iw4l.sim", 1.0);
        if applied {
            perf::counter!("client_actions_applied", "iw4l.sim", 1.0);
        } else {
            perf::counter!("client_actions_refused", "iw4l.sim", 1.0);
        }
    }
}

fn run_players_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let level_time = crate::level_time_ms(tick);
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunPlayers);

    let allow_move = matches!(world.phase(), MatchPhase::Playing | MatchPhase::Warmup);
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
        world.refresh_portal_availability();
        let portal_blocked_exits = world.world_objects().portal_blocked_exits().to_vec();
        let model_brushes = world.model_movement_brushes();
        for command in &input.cmds {
            let id = &command.client;
            let cmd = &command.command;
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
            let facts = content.weapon_combat_facts(ps.weapon);
            let context = crate::bg::player_step::context_for_player(
                &content,
                &ps,
                old_buttons,
                world
                    .client_meta(*id)
                    .and_then(|m| m.shellshock.as_ref())
                    .is_some_and(|shock| shock.movement),
            );
            crate::script::player_commands(world.ecs(), id.0, cmd.buttons, old_buttons);
            let mut cmd = *cmd;
            crate::script_player::constrain_cmd(&mut world, *id, &mut cmd);
            crate::script::hold_killcam_viewer(world.ecs(), *id, &mut cmd);
            crate::script::select_location(world.ecs(), id.0, &mut cmd, old_buttons);
            if world
                .client_meta(*id)
                .is_some_and(|m| m.remote_missile.is_some())
            {
                crate::remote_missile::steer(&mut world, *id, &cmd, delta.min(200));
                cmd.forwardmove = 0;
                cmd.rightmove = 0;
                cmd.buttons &= playerstate_iw4::buttons::CROUCH | playerstate_iw4::buttons::PRONE;
            }
            if facts.is_some_and(|f| f.scope_zoom.is_variable())
                && ps.f_weapon_pos_frac == 1.0
                && cmd.buttons & playerstate_iw4::buttons::CHANGE_ZOOM != 0
            {
                cmd.buttons &= !playerstate_iw4::buttons::MELEE_CHARGE;
            }
            let commanded_move = cmd.forwardmove != 0 || cmd.rightmove != 0;
            world.set_anim_command_buttons(*id, cmd.buttons);
            let linked_brushes: Vec<LinkedBrushCollisionBrush> = world
                .entity_collision_capabilities()
                .iter()
                .flat_map(|c| c.solid_brushes().iter().cloned())
                .collect();
            let bodies = alive_body_clips(&world);
            let glass_damage = world.world_objects().glass_damage_pairs();
            let atlas_world = crate::world_graph::world_at(ps.origin);
            let beyond = content
                .atlas()
                .and_then(|atlas| atlas.worlds.get(atlas_world.checked_sub(1)?));
            let backend = match beyond {
                None => PlayerCollision {
                    brushes,
                    bsp,
                    mesh,
                    glass_damage: &glass_damage,
                    bodies: &bodies,
                    self_entnum: id.0 as u16,
                    cmodels: cmodel_models,
                    linked_brushes: &linked_brushes,
                    model_brushes: &model_brushes,
                    offset: [0.0; 3],
                },
                Some(beyond) => PlayerCollision {
                    brushes: &beyond.brushes,
                    bsp: &beyond.bsp,
                    mesh: &beyond.mesh,
                    glass_damage: &[],
                    bodies: &bodies,
                    self_entnum: id.0 as u16,
                    cmodels: &beyond.cmodels.models,
                    linked_brushes: &[],
                    model_brushes: &[],
                    offset: crate::world_graph::world_offset(atlas_world),
                },
            };
            let script = world.player_anim_script();
            let mantle = world.xanims();
            let crate::bg::player_step::PlayerMoveOutput {
                walking,
                linked_bounds,
                anim_movetype,
                view_w,
                primary,
                moved_from,
                moved_to,
                stance_event,
                reset_torso,
                jump_animations,
                force_movement_anim,
                landing_animation,
                fall_damage,
            } = {
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");
                crate::bg::player_step::player_step(
                    ps,
                    &mut cmd,
                    context,
                    &backend,
                    mantle.as_ref(),
                    level_time,
                )
            };
            world
                .client_meta_mut(*id)
                .input_receipt
                .record(commanded_move, moved_from, moved_to);
            if let Some(atlas) = content.atlas()
                && let Some(ps) = world.player_mut(*id)
            {
                let before = ps.origin;
                if let Some((portal, from_a, exit_world)) = atlas
                    .carry_player_checked_with_availability(
                        ps,
                        moved_from,
                        moved_to,
                        linked_bounds,
                        &portal_blocked_exits,
                        |input| {
                            let exit_backend = if let Some(exit) =
                                crate::world_graph::world_at(input.start)
                                    .checked_sub(1)
                                    .and_then(|index| atlas.worlds.get(index))
                            {
                                PlayerCollision {
                                    brushes: &exit.brushes,
                                    bsp: &exit.bsp,
                                    mesh: &exit.mesh,
                                    cmodels: &exit.cmodels.models,
                                    linked_brushes: &[],
                                    model_brushes: &[],
                                    glass_damage: &[],
                                    offset: crate::world_graph::world_offset(
                                        crate::world_graph::world_at(input.start),
                                    ),
                                    ..backend
                                }
                            } else {
                                PlayerCollision {
                                    brushes,
                                    bsp,
                                    mesh,
                                    cmodels: cmodel_models,
                                    linked_brushes: &linked_brushes,
                                    model_brushes: &model_brushes,
                                    glass_damage: &glass_damage,
                                    offset: [0.0; 3],
                                    ..backend
                                }
                            };
                            movement_iw4::CollisionBackend::trace(&exit_backend, input)
                        },
                    )
                {
                    diag::info!(
                        Sim,
                        "atlas: client {} through door {portal} (from a: {from_a}) {before:?} -> world {exit_world} {:?}",
                        id.0,
                        ps.origin
                    );
                }
            }
            if reset_torso && let Some(ps) = world.player_mut(*id) {
                crate::player_anim_script::reset_stance_torso(ps);
            }
            if let Some(event) = stance_event {
                crate::combat::apply_player_anim_event(&mut world, *id, event);
            }
            for (animation, force) in jump_animations.into_iter().flatten() {
                let event = match animation {
                    movement_iw4::JumpAnimation::Forward => 3,
                    movement_iw4::JumpAnimation::Backward => 4,
                };
                crate::combat::apply_player_anim_event_forced(&mut world, *id, event, force);
            }
            if landing_animation {
                crate::combat::apply_player_anim_event(&mut world, *id, 5);
            }
            if fall_damage > 0 {
                apply_fall_damage(&mut world, tick, *id, fall_damage, moved_to);
            }
            if let Some(movetype) = anim_movetype {
                let view_facts = world.combat_facts_for(view_w);
                let primary_facts = world.combat_facts_for(primary);
                let previous_movetype = world.last_anim_movetype(*id);
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");
                let strafing =
                    crate::player_anim_script::anim_strafing(ps, cmd.forwardmove, cmd.rightmove);
                let conds = crate::anim_conditions_from_pmove(
                    ps,
                    view_facts,
                    primary_facts,
                    previous_movetype,
                    strafing,
                    cmd.buttons,
                );
                if let Some(script) = script.as_ref()
                    && let Some(selected) =
                        script.apply(ps, movetype, id.0, &conds, force_movement_anim)
                {
                    world.set_anim_movement(*id, selected, strafing);
                }
            }
            world.set_pmove_walking(*id, walking);
            world.link_player_area(*id, linked_bounds);

            let shots = advance_weapon_command(
                &mut world,
                tick,
                *id,
                cmd,
                delta.min(200),
                command.sequence,
            );

            for shot in shots {
                if let Some(mut run) = world
                    .ecs()
                    .get_resource_mut::<crate::input_program::InputProgramRun>()
                    && let Some(actor) = run.actor_mut(*id)
                {
                    actor.shots.push(shot);
                }
                crate::missile::fire_accepted_shot(&mut world, tick, &shot);

                world.record_collision_history(tick, 0);
                world.record_entity_collision_history(tick);
                let scripted = world
                    .ecs()
                    .get_resource::<crate::input_program::InputProgramRun>()
                    .is_some_and(|run| run.actor(*id).is_some());
                if scripted {
                    let targets = world
                        .hitvol_dump(|client| world.player(client).copied())
                        .into_iter()
                        .filter_map(|row| {
                            let client = row.client?;
                            Some(crate::input_program::InputTargetSample {
                                client,
                                player: *world.player(client)?,
                                head: row
                                    .head_x
                                    .zip(row.head_y)
                                    .zip(row.head_z)
                                    .map(|((x, y), z)| [x, y, z]),
                            })
                        })
                        .collect();
                    if let Some(mut run) = world
                        .ecs()
                        .get_resource_mut::<crate::input_program::InputProgramRun>()
                        && let Some(actor) = run.actor_mut(*id)
                    {
                        actor
                            .shot_scenes
                            .push(crate::input_program::InputShotScene {
                                tick,
                                shot: shot.shot_id,
                                targets,
                            });
                    }
                }
                let emissions = phase_emit(&world.content(), tick, core::slice::from_ref(&shot));
                let verdict_start = world.shot_collision_verdicts().len();
                phase_trace(&mut world, tick, &emissions);
                let verdicts = world.shot_collision_verdicts()[verdict_start..].to_vec();
                if let Some(mut run) = world
                    .ecs()
                    .get_resource_mut::<crate::input_program::InputProgramRun>()
                    && let Some(actor) = run.actor_mut(*id)
                {
                    actor.shot_verdicts.extend(verdicts);
                }
            }
            crate::equipment::phase_offhand(&mut world, tick, &[(*id, cmd)]);
            world.set_old_cmd(*id, cmd.buttons, cmd.angles);
            let applied = crate::PlayerCommand {
                command: cmd,
                ..*command
            };
            let receipt_player = world.player(*id).copied();
            if let Some(mut run) = world
                .ecs()
                .get_resource_mut::<crate::input_program::InputProgramRun>()
                && let Some(actor) = run.actor_mut(*id)
                && let Some(player) = receipt_player
            {
                actor.receipts.push(crate::input_program::InputReceipt {
                    tick,
                    command: applied,
                    player,
                });
            }
            consumed.push(crate::PlayerCommand {
                command: cmd,
                ..*command
            });
        }
    }

    *world.old_buttons_mut() = original_buttons;
    *world.old_cmd_angles_mut() = original_angles;

    crate::script::apply_player_links(world.ecs());
    restamp_debug_move_look(&mut world, &input.actions);

    if allow_move {
        phase_stuck_in_client(&mut world);
    }

    let mut request = ecs.resource_mut::<StepRequest>();
    if allow_move {
        request.input.cmds = consumed;
    }
}

fn apply_fall_damage(world: &mut FrameWorld, tick: Tick, id: ClientId, amount: i32, at: [f32; 3]) {
    let hit = crate::script::ScriptHit {
        piece: None,
        target: crate::script::HitTarget::Player(id),
        amount,
        origin: at,
        attacker: None,
        inflictor: None,
        means: "MOD_FALLING",
        weapon: 0,
        flags: 0,
        hitloc: 0,
    };
    crate::damage::apply_script_hit(world, tick, &hit);
}

fn record_collision_state_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RecordCollisionState);
    world.record_collision_history(tick, msec);
}

fn run_entity_types_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let mut world = frame_world(ecs);
    let allow_move = world.phase() == MatchPhase::Playing;
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunEntityTypes);
    if allow_move {
        phase_materialize_entity_dobjs(&mut world);

        world.record_entity_collision_history(tick);

        crate::remote_missile::advance(&mut world, tick);
        crate::entity_run::phase_run_entity_thinks(&mut world, tick);
        world.world_objects_mut().glass_update(
            i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX),
        );
        phase_animated_map_models(&mut world, tick, msec);
    } else {
        crate::entity_run::phase_walk_entity_thinks(&mut world);
    }
}

fn dispatch_touches_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
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
        .map(|command| (command.client.0, command.command.buttons))
        .collect();
    let latest: std::collections::BTreeMap<_, _> = input
        .cmds
        .iter()
        .map(|command| (command.client, command.command))
        .collect();
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
    world.stamp_use_presses(presses.clone());
    if allow_move {
        crate::item::phase_use_items(&mut world, tick, &presses, &cmds);
    }
}

fn finalize_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::Finalize);
    for command in &input.cmds {
        let (id, cmd) = (command.client, command.command);
        emit_attack_events(&mut world, tick, &[(id, cmd)]);
        if world.client_meta(id).is_some() {
            world.set_old_cmd(id, cmd.buttons, cmd.angles);
        }
    }

    let mut old_buttons = std::mem::take(world.old_buttons_mut());
    old_buttons.retain(|(id, _)| world.client_meta(*id).is_some());
    *world.old_buttons_mut() = old_buttons;
    let mut old_angles = std::mem::take(world.old_cmd_angles_mut());
    old_angles.retain(|(id, _)| world.client_meta(*id).is_some());
    *world.old_cmd_angles_mut() = old_angles;

    harvest_predictable_events(&mut world, tick);
    debug_assert!(
        world.validate_entity_payloads().is_ok(),
        "{:?}",
        world.validate_entity_payloads()
    );
}

fn publish_snapshot_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::PublishSnapshot);

    emit_player_ticks(&world, tick);
    world.refresh_portal_availability();
    let snapshot = world.snapshot(tick);
    emit_snapshot_events(&world, &snapshot);

    let mut world = frame_world(ecs);
    let weapon_script_names = world.weapon_script_names();
    let pending_final_kill = world.take_pending_final_kill();
    let mut effects = TickEffects {
        prints: world.take_pending_prints(),
        local_sounds: world.take_pending_local_sounds(),
        player_cards: world.take_pending_player_cards(),
        script_audio: world.take_pending_script_audio(),
        kicks: Vec::new(),
    };

    let script_seats = crate::script::script_seats(ecs);
    let mut runtime = ecs.resource_mut::<crate::script::RoundScript>();
    let script_exit_level = std::mem::take(&mut runtime.exit_level);
    effects.kicks = std::mem::take(&mut runtime.kicks)
        .into_iter()
        .map(|(client, reason)| (ClientId(client), reason))
        .collect();

    let mut request = ecs.resource_mut::<StepRequest>();
    request.output = Some(TickResult {
        input: request.input.clone(),
        snapshot,
        effects,
        weapon_script_names,
        pending_final_kill,
        script_seats,
        script_exit_level,
    });
}

fn emit_player_ticks(world: &FrameWorld, tick: Tick) {
    if !perf::enabled() {
        return;
    }
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
        perf::instant!("player_tick", "iw4l.sim", |ctx| {
            let (client_id, is_bot, buttons, ammo_clip, walking, lifecycle): (
                u32,
                bool,
                u32,
                i32,
                Option<i32>,
                &'static str,
            ) = (
                id.0,
                meta.name == bot_name,
                world.old_buttons(id).unwrap_or(0),
                meta.weapon_carry.ammo_clip,
                world.pmove_walking(id),
                lifecycle_label(meta.lifecycle),
            );
            ctx.arg("client_id", i64::from(client_id));
            ctx.arg("is_bot", i64::from(u8::from(is_bot)));
            ctx.arg("time_ms", i64::from(time_ms));
            ctx.arg("level_time_ms", i64::from(level_time_ms));
            ctx.arg("origin_x", f64::from(origin[0]));
            ctx.arg("origin_y", f64::from(origin[1]));
            ctx.arg("origin_z", f64::from(origin[2]));
            ctx.arg("vz", f64::from(vz));
            ctx.arg("yaw", f64::from(yaw));
            ctx.arg("pitch", f64::from(pitch));
            ctx.arg("jump_time", i64::from(jump_time));
            ctx.arg("buttons", i64::from(buttons));
            ctx.arg("weaponstate", i64::from(weaponstate));
            ctx.arg("ammo_clip", i64::from(ammo_clip));
            ctx.arg("walking", walking.map(i64::from));
            ctx.arg("legs_anim", i64::from(legs_anim));
            ctx.arg("torso_anim", i64::from(torso_anim));
            ctx.arg("lifecycle", lifecycle);
        });
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
    if !perf::enabled() {
        return;
    }
    world.emit_script_entity_trace();
    for slot in &snapshot.meta.corpses.slots {
        perf::instant!("corpse", "iw4l.sim", |ctx| {
            let (occupied, origin_z): (i64, Option<f32>) = (
                i64::from(slot.occupied),
                slot.occupied.then_some(slot.origin[2]),
            );
            ctx.arg("occupied", occupied);
            ctx.arg("origin_z", origin_z.map(f64::from));
        });
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
        perf::instant!("item", "iw4l.sim", |ctx| {
            let (e_type, clip_r, scavenger): (i32, Option<i32>, Option<i32>) = (
                es.e_type,
                ammo.map(|row| row.clip_r),
                ammo.map(|row| row.scavenger),
            );
            ctx.arg("e_type", i64::from(e_type));
            ctx.arg("clip_r", clip_r.map(i64::from));
            ctx.arg("scavenger", scavenger.map(i64::from));
        });
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
        ClientAction::JoinMatch
            | ClientAction::LeaveMatch
            | ClientAction::SetName { .. }
            | ClientAction::SetProfile { .. }
            | ClientAction::Chat { .. }
            | ClientAction::UseCopycat
            | ClientAction::ActionSlot { .. }
            | ClientAction::ChooseDefaultClass { .. }
            | ClientAction::MenuResponse { .. }
    )
}

fn apply_action(world: &mut FrameWorld, tick: Tick, id: ClientId, action: &ClientAction) -> bool {
    match *action {
        ClientAction::JoinMatch => {
            let meta = world.client_meta_mut(id);
            if meta.lifecycle == ClientLifecycle::Connecting {
                meta.lifecycle = ClientLifecycle::ChoosingClass;
            }

            true
        }
        ClientAction::ChooseDefaultClass { index } => {
            crate::script::answer_join(world.ecs(), id.0);
            choose_bot_class(world, id, index);

            true
        }
        ClientAction::MenuResponse { menu, response } => {
            let menu = crate::menu_response_text(&menu);
            let response = crate::menu_response_text(&response);
            crate::script::note_team_answer(world.ecs(), id.0, menu);
            if let Some(outcome) = answer_custom_class(world, id, menu, response) {
                return outcome;
            } else {
                crate::script::answer_menu(world.ecs(), id.0, menu, response);
            }

            true
        }
        ClientAction::LeaveMatch => {
            world.retire_client(id);

            true
        }
        ClientAction::SetName { name } => {
            world.client_meta_mut(id).name = name;

            true
        }
        ClientAction::Chat {
            team_only, text, ..
        } => {
            let end = text.iter().position(|b| *b == 0).unwrap_or(text.len());
            let Ok(text) = core::str::from_utf8(&text[..end]) else {
                return false;
            };
            crate::chat::send(world, id, team_only, text)
        }
        ClientAction::UseCopycat | ClientAction::SpawnClient => false,
        ClientAction::ActionSlot { slot } => {
            crate::script::action_slot_command(world.ecs(), id.0, slot);

            true
        }
        ClientAction::SelectClass { class_id, loadout } => {
            apply_select_class(world, id, class_id, loadout)
        }
        ClientAction::GiveWeapon { weapon, model } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return false;
            }
            let outcome = apply_give_weapon(world, id, weapon);
            if !outcome {
                return outcome;
            }
            if let Some(ps) = world.player_mut(id) {
                weapon_iw4::set_weapon_model_for_held(
                    &ps.weapons,
                    &mut ps.weapon_data,
                    weapon,
                    model,
                );
            }

            true
        }
        ClientAction::ChangeWeaponCamo { weapon, model, .. } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
                || !world.weapon_camouflage_allowed(weapon, model)
            {
                return false;
            }
            let Some(ps) = world.player_mut(id) else {
                return false;
            };
            if weapon == 0 || ps.weapon != weapon || !ps.weapons.contains(&(weapon as i32)) {
                return false;
            }
            weapon_iw4::set_weapon_model_for_held(&ps.weapons, &mut ps.weapon_data, weapon, model);
            true
        }
        ClientAction::ChangeWeaponConfiguration { from, to } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return false;
            }
            apply_configuration_change(world, id, from, to)
        }
        ClientAction::ForceSpawn { pick } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return false;
            }
            apply_force_spawn(world, id, pick)
        }
        ClientAction::SpawnIntermission => apply_spawn_intermission(world, id),
        ClientAction::ResupplyAmmo => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return false;
            }
            for weapon in
                crate::script_player::weapons(world, id, crate::script_player::WeaponList::All)
            {
                crate::script_player::give_max_ammo(world, id, weapon);
            }

            true
        }
        ClientAction::SetProfile { profile, .. } => {
            crate::script::set_profile(world.ecs(), id.0, profile)
        }
        ClientAction::GiveKillstreak { name } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return false;
            }
            let name = crate::menu_response_text(&name);
            crate::script::give_killstreak(world.ecs(), id.0, name)
        }
        ClientAction::ForceDeath => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return false;
            }
            crate::script::force_death(world.ecs(), tick, id.0);

            true
        }
        ClientAction::SetMatchPhase { phase } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return false;
            }
            if phase == MatchPhase::Playing && world.phase() == MatchPhase::Warmup {
                if crate::script::host::iw4_gametype::force_match_start(world.ecs(), tick) {
                    return true;
                }
                crate::score::finish_prematch(world);
            } else {
                world.set_phase(phase);
            }

            true
        }
        ClientAction::Move { origin, angles } => apply_debug_move(world, id, origin, angles),
        ClientAction::BeginScriptMoverRotateVelocity { speed } => {
            if !world.bootstrap_ref().allow_debug_actions || !speed.is_finite() {
                return false;
            }
            world.begin_script_movers_rotate_velocity_supplied(
                speed,
                crate::corpse::level_time_ms(tick),
            );

            true
        }
        ClientAction::ToggleGod => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return false;
            }
            let meta = world.client_meta_mut(id);
            meta.god_mode = !meta.god_mode;
            diag::info!(
                Sim,
                "god: client={} {}",
                id.0,
                if meta.god_mode { "on" } else { "off" }
            );

            true
        }
        ClientAction::EndGracePeriod => {
            world.bootstrap_ref().allow_debug_actions
                && crate::script::host::iw4_gametype::end_grace_period(world.ecs())
        }
        ClientAction::ToggleProjectileFlight => {
            if !world.bootstrap_ref().allow_debug_actions || world.client_meta(id).is_none() {
                return false;
            }
            let meta = world.client_meta_mut(id);
            meta.projectile_flight = !meta.projectile_flight;
            diag::info!(
                Sim,
                "projectile_flight: client={} {}",
                id.0,
                if meta.projectile_flight { "on" } else { "off" }
            );

            true
        }
        ClientAction::DebugDamage { amount } => apply_debug_damage(world, tick, id, amount),
    }
}

fn apply_force_spawn(world: &mut FrameWorld, id: ClientId, pick: crate::SpawnPick) -> bool {
    if world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        crate::script_player::move_to_forced_spawn(world, id, pick);
        true
    } else {
        world.client_meta_mut(id).forced_spawn = Some(pick);
        true
    }
}

fn apply_spawn_intermission(world: &mut FrameWorld, id: ClientId) -> bool {
    let Some(meta) = world.client_meta(id) else {
        return false;
    };
    if meta.lifecycle == ClientLifecycle::Intermission {
        return true;
    }
    let view = world.bootstrap_ref().intermission_view.clone();
    {
        let meta = world.client_meta_mut(id);
        meta.lifecycle = ClientLifecycle::Intermission;
        meta.dead_since_tick = None;
    }
    world.unlink_player_area(id);
    let Some(view) = view else {
        return true;
    };
    let Some(ps) = world.player_mut(id) else {
        return true;
    };
    ps.origin = view.origin;
    ps.velocity = [0.0, 0.0, 0.0];
    ps.viewangles = view.angles;
    ps.delta_angles = packed_look_delta(view.angles);
    ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
    true
}

fn apply_debug_move(
    world: &mut FrameWorld,
    id: ClientId,
    origin: [f32; 3],
    angles: [f32; 3],
) -> bool {
    if !world.bootstrap_ref().allow_debug_actions {
        return false;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return false;
    }
    if !origin.iter().chain(angles.iter()).all(|v| v.is_finite()) {
        return false;
    }
    let old_origin = {
        let Some(ps) = world.player_mut(id) else {
            return false;
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
    true
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

fn apply_debug_damage(world: &mut FrameWorld, tick: Tick, id: ClientId, amount: i32) -> bool {
    if !world.bootstrap_ref().allow_debug_actions {
        return false;
    }
    if amount <= 0 {
        return false;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return false;
    }
    crate::script_player::debug_damage(world, tick, id, amount);
    true
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

fn apply_configuration_change(world: &mut FrameWorld, id: ClientId, from: u32, to: u32) -> bool {
    let reject = |_world: &mut FrameWorld, reason: &'static str| {
        diag::warn!(
            Sim,
            "configuration change: refused client={} reason={}",
            id.0,
            reason
        );
        false
    };
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return reject(world, "not_alive");
    }
    let Some(ps) = world.player(id).copied() else {
        return reject(world, "not_alive");
    };
    if ps.weapon != from {
        return reject(world, "stale_source");
    }
    if !world.can_transition_weapon(from, to) {
        return reject(world, "different_family");
    }
    let (Some(old), Some(new)) = (world.combat_facts_for(from), world.combat_facts_for(to)) else {
        return reject(world, "invalid_target");
    };
    if world
        .equipment_facts_for(to)
        .is_some_and(|eq| eq.is_offhand())
    {
        return reject(world, "invalid_target");
    }
    if ps.weaponstate_primary != weapon_iw4::WeaponState::Ready as i32
        || ps.weapon_time > 0
        || (ps.last_weapon_hand >= 1
            && (ps.weaponstate_secondary != weapon_iw4::WeaponState::Ready as i32
                || ps.weapon_time_secondary > 0))
        || ps.weap_flags & playerstate_iw4::weap_flags::OFFHAND_VIEW != 0
    {
        return reject(world, "busy");
    }
    let Some(slot) = ps.weapons.iter().position(|&weapon| weapon == from as i32) else {
        return reject(world, "no_inventory_slot");
    };
    if ps.weapons.contains(&(to as i32)) {
        return reject(world, "invalid_target");
    }
    let old_ammo_key = weapon_iw4::ammo_table_key(old.ammo_index, from);
    let old_clip_key = weapon_iw4::clip_table_key(old.clip_index, from);
    let new_ammo_key = weapon_iw4::ammo_table_key(new.ammo_index, to);
    let new_clip_key = weapon_iw4::clip_table_key(new.clip_index, to);
    let another_owns = |key: i32, clip: bool| {
        ps.weapons
            .iter()
            .filter(|&&weapon| weapon > 0 && weapon != from as i32)
            .any(|&weapon| {
                world.combat_facts_for(weapon as u32).is_some_and(|facts| {
                    key == if clip {
                        weapon_iw4::clip_table_key(facts.clip_index, weapon as u32)
                    } else {
                        weapon_iw4::ammo_table_key(facts.ammo_index, weapon as u32)
                    }
                })
            })
    };
    let clip0 = weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip_key, 0);
    let raw_clip1 = weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip_key, 1);
    let clip1 = if ps.last_weapon_hand >= 1 {
        raw_clip1
    } else {
        0
    };
    let stock = weapon_iw4::get_ammo_not_in_clip(&ps.ammo, old_ammo_key);
    let akimbo = world
        .combat_facts_for(to)
        .is_some_and(|facts| facts.dual_wield)
        || gsc_give_weapon_is_akimbo(world.weapon_script_name(to));
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
        return reject(world, "shared_ammo_conflict");
    }
    let mut next = ps;
    next.weapons[slot] = to as i32;
    next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
    weapon_iw4::latch_weapon_dual_wield(&next.weapons, &mut next.weapon_data, to, akimbo);
    next.weapon = to;
    next.weapon_primary = to;
    next.last_weapon_hand = weapon_iw4::num_hands_for_held(&next.weapons, &next.weapon_data, to);
    if old_ammo_key != new_ammo_key {
        for row in next.ammo.as_chunks_mut::<8>().0 {
            if row[..4] == old_ammo_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if old_clip_key != new_clip_key {
        for row in next.ammoclip.as_chunks_mut::<12>().0 {
            if row[..4] == old_clip_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if !weapon_iw4::set_ammo_not_in_clip(&mut next.ammo, new_ammo_key, next_stock)
        || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip_key, 0, next_clip0)
        || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip_key, 1, next_clip1)
    {
        return reject(world, "ammo_table_full");
    }
    let alternate_ammo = match (
        world.combat_facts_for(old.alternate_weapon),
        world.combat_facts_for(new.alternate_weapon),
    ) {
        (Some(old_alt), Some(new_alt))
            if old.alternate_weapon != 0
                && new.alternate_weapon != 0
                && old_alt.weap_type == new_alt.weap_type
                && old_alt.weap_class == new_alt.weap_class =>
        {
            let old_key = weapon_iw4::ammo_table_key(old_alt.ammo_index, old.alternate_weapon);
            let old_clip = weapon_iw4::clip_table_key(old_alt.clip_index, old.alternate_weapon);
            let new_key = weapon_iw4::ammo_table_key(new_alt.ammo_index, new.alternate_weapon);
            let new_clip = weapon_iw4::clip_table_key(new_alt.clip_index, new.alternate_weapon);
            if old_key != old_ammo_key
                && old_clip != old_clip_key
                && new_key != new_ammo_key
                && new_clip != new_clip_key
                && weapon_iw4::ammo_row_present(&ps.ammo, old_key)
                && weapon_iw4::clip_row_present(&ps.ammoclip, old_clip)
            {
                let shared = ps
                    .weapons
                    .iter()
                    .filter(|&&w| w > 0 && w != from as i32)
                    .any(|&w| {
                        let alt = world
                            .combat_facts_for(w as u32)
                            .map_or(0, |f| f.alternate_weapon);
                        [w as u32, alt].into_iter().filter(|&w| w != 0).any(|w| {
                            world.combat_facts_for(w).is_some_and(|f| {
                                let ammo = weapon_iw4::ammo_table_key(f.ammo_index, w);
                                let clip = weapon_iw4::clip_table_key(f.clip_index, w);
                                (old_key != new_key && (ammo == old_key || ammo == new_key))
                                    || (old_clip != new_clip
                                        && (clip == old_clip || clip == new_clip))
                            })
                        })
                    });
                if shared {
                    return reject(world, "shared_ammo_conflict");
                }
                let (clip, _, stock) = configuration_change_ammo(
                    weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip, 0),
                    0,
                    weapon_iw4::get_ammo_not_in_clip(&ps.ammo, old_key),
                    new_alt.clip_size,
                    new_alt.max_ammo,
                    false,
                );
                if old_key != new_key {
                    for row in next.ammo.as_chunks_mut::<8>().0 {
                        if row[..4] == old_key.to_le_bytes() {
                            row.fill(0);
                        }
                    }
                }
                if old_clip != new_clip {
                    for row in next.ammoclip.as_chunks_mut::<12>().0 {
                        if row[..4] == old_clip.to_le_bytes() {
                            row.fill(0);
                        }
                    }
                }
                if !weapon_iw4::set_ammo_not_in_clip(&mut next.ammo, new_key, stock)
                    || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip, 0, clip)
                {
                    return reject(world, "ammo_table_full");
                }
                Some((new.alternate_weapon, clip, stock))
            } else {
                None
            }
        }
        _ => None,
    };
    let hand = weapon_iw4::spawn_weapon_hand(to, &new, false);
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
    if let Some((alternate, clip, stock)) = alternate_ammo {
        meta.weapon_carry
            .ammo_by_weapon
            .retain(|row| row.0 != old.alternate_weapon);
        meta.weapon_carry.set_ammo(alternate, clip, stock);
    }
    let quick_reload_ready = meta.weapon_carry.quick_reload_ready(from);
    meta.weapon_carry.ammo_by_weapon.retain(|row| row.0 != from);
    meta.weapon_carry.set_ammo(to, next_clip0, next_stock);
    meta.weapon_carry.set_quick_reload_ready(from, true);
    meta.weapon_carry
        .set_quick_reload_ready(to, quick_reload_ready);
    meta.weapon_carry.mirror_held_ammo(to);
    meta.weapon_carry.weapon_shot_count = 0;
    meta.weapon_carry.burst_latch = false;
    meta.weapon_carry.burst_latch_secondary = false;
    meta.weapon_carry.rechamber_pending = false;
    meta.weapon_carry.rechamber_pending_secondary = false;
    meta.weapon_carry.pending_brass = [None; 2];

    true
}

fn apply_give_weapon(world: &mut FrameWorld, id: ClientId, weapon: u32) -> bool {
    let reject = |_world: &mut FrameWorld, reason: &'static str| {
        diag::warn!(Sim, "give: refused client={} reason={}", id.0, reason);
        false
    };

    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return reject(world, "not_alive");
    }
    if weapon == 0 {
        return reject(world, "invalid_weapon");
    }
    let table_len = world.weapon_combat_len();
    if (weapon as usize) >= table_len {
        return reject(world, "unknown_weapon_id");
    }
    if !world.weapon_runnable(weapon) {
        return reject(world, "unsupported_weapon");
    }
    let Some(facts) = world.combat_facts_for(weapon) else {
        return reject(world, "unknown_weapon_id");
    };
    if world
        .equipment_facts_for(weapon)
        .is_some_and(|eq| eq.is_offhand())
    {
        return apply_give_offhand(world, id, weapon, &facts);
    }
    if facts.fire_time_ms <= 0 && facts.raise_time_ms <= 0 {
        return reject(world, "empty_combat_profile");
    }

    if world.weapon_script_name(weapon).starts_with("killstreak_") {
        let Some(ps) = world.player_mut(id) else {
            return reject(world, "not_alive");
        };
        inventory_add_weapon(ps, weapon, false);
        if !ps.weapons.contains(&(weapon as i32)) {
            return reject(world, "invalid_weapon");
        }
        let (clip, clip_alt, stock) = weapon_iw4::spawn_clip_stock(&facts, 0);
        seed_ps_ammo_tables(ps, weapon, &facts, clip, clip_alt, false, stock);
        world
            .client_meta_mut(id)
            .weapon_carry
            .set_ammo(weapon, clip, stock);

        return true;
    }
    let akimbo = world
        .combat_facts_for(weapon)
        .is_some_and(|facts| facts.dual_wield)
        || gsc_give_weapon_is_akimbo(world.weapon_script_name(weapon));
    let Some(mut next) = world.player(id).copied() else {
        return reject(world, "not_alive");
    };

    let outgoing = if world
        .combat_facts_for(next.weapon)
        .is_some_and(|facts| facts.inventory_type == 3)
        && next.weapons.contains(&(next.weapon_primary as i32))
    {
        next.weapon_primary
    } else {
        next.weapon
    };
    let mut replaced = false;
    if !next.weapons.contains(&(weapon as i32))
        && let Some(slot) = next
            .weapons
            .iter()
            .position(|&w| w != 0 && w == outgoing as i32)
    {
        next.weapons[slot] = weapon as i32;
        next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
        replaced = true;
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
                weapon_iw4::clip_table_key(outgoing_facts.clip_index, outgoing)
            } else {
                weapon_iw4::ammo_table_key(outgoing_facts.ammo_index, outgoing)
            };
            let owned = next.weapons.iter().filter(|&&w| w > 0).any(|&w| {
                world.combat_facts_for(w as u32).is_some_and(|f| {
                    key == if clip {
                        weapon_iw4::clip_table_key(f.clip_index, w as u32)
                    } else {
                        weapon_iw4::ammo_table_key(f.ammo_index, w as u32)
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
        meta.weapon_carry
            .ammo_by_weapon
            .retain(|row| row.0 != outgoing);
        meta.weapon_carry.set_quick_reload_ready(outgoing, true);
    }
    meta.weapon_carry.set_ammo(weapon, clip, stock);
    meta.weapon_carry.set_quick_reload_ready(weapon, true);
    meta.weapon_carry.mirror_held_ammo(weapon);
    meta.weapon_carry.weapon_shot_count = 0;
    meta.weapon_carry.burst_latch = false;
    meta.weapon_carry.burst_latch_secondary = false;
    meta.weapon_carry.rechamber_pending = false;
    meta.weapon_carry.rechamber_pending_secondary = false;
    meta.weapon_carry.pending_brass = [None; 2];

    true
}

fn apply_give_offhand(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) -> bool {
    let reject = |_world: &mut FrameWorld, reason: &'static str| {
        diag::warn!(Sim, "give: refused client={} reason={}", id.0, reason);
        false
    };
    let Some(eq) = world.equipment_facts_for(weapon) else {
        return reject(world, "invalid_weapon");
    };
    let Some(mut next) = world.player(id).copied() else {
        return reject(world, "not_alive");
    };
    for slot in &mut next.weapons {
        if *slot > 0 && *slot != weapon as i32 {
            let same_slot = world
                .equipment_facts_for(*slot as u32)
                .is_some_and(|other| {
                    (matches!(eq.offhand_class, 1 | 4 | 5)
                        && matches!(other.offhand_class, 1 | 4 | 5))
                        || (matches!(eq.offhand_class, 2 | 3)
                            && matches!(other.offhand_class, 2 | 3))
                });
            if same_slot {
                *slot = 0;
            }
        }
    }
    inventory_add_weapon(&mut next, weapon, false);
    if !next.weapons.contains(&(weapon as i32)) {
        return reject(world, "invalid_weapon");
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
        meta.weapon_carry
            .ammo_by_weapon
            .retain(|(id, _, _)| next.weapons.contains(&(*id as i32)));
        meta.weapon_carry.set_ammo(weapon, clip, 0);
        if let Some(loadout) = meta.loadout.as_mut() {
            match eq.offhand_class {
                1 | 4 | 5 => loadout.lethal = weapon,
                2 | 3 => loadout.tactical = weapon,
                _ => {}
            }
        }
    }

    true
}

fn choose_bot_class(world: &mut FrameWorld, id: ClientId, index: u8) {
    let custom_classes = crate::script::bot_custom_classes_allowed(world.ecs());
    let bot_classes = &world.bootstrap_ref().bot_classes;
    let def = (!bot_classes.is_empty() && custom_classes)
        .then(|| bot_classes[index as usize % bot_classes.len()].clone())
        .filter(|def| !def.locked && validate_class_content(world, def).is_ok())
        .filter(|def| {
            world
                .ecs_ref()
                .resource::<crate::PersistentDataStore>()
                .temporary_client(id)
                || crate::progression::validate_class(world, id, def).is_ok()
        });
    match def {
        Some(def) => crate::script::choose_class(world.ecs(), id.0, &def),
        None => crate::script::choose_default_class(world.ecs(), id.0, index),
    }
}

fn answer_custom_class(
    world: &mut FrameWorld,
    id: ClientId,
    menu: &str,
    response: &str,
) -> Option<bool> {
    if !menu.eq_ignore_ascii_case("changeclass") {
        return None;
    }
    let slot = response
        .strip_prefix("custom")
        .and_then(|n| n.parse::<u32>().ok())
        .and_then(|n| n.checked_sub(1))?;
    let Some(def) = crate::script::personal_class(world.ecs(), id.0, crate::ClassId(slot)) else {
        return Some(false);
    };
    if validate_class_content(world, &def)
        .and_then(|()| crate::progression::validate_class(world, id, &def))
        .is_err()
    {
        return Some(false);
    }
    crate::script::choose_class(world.ecs(), id.0, &def);
    Some(true)
}

fn apply_select_class(
    world: &mut FrameWorld,
    id: ClientId,
    class_id: crate::ClassId,
    loadout: crate::PersonalClass,
) -> bool {
    let accepted = (class_id.0 < crate::match_state::PERSONAL_CLASS_SLOTS as u32)
        .then(|| loadout.definition(class_id))
        .flatten();
    let result = accepted
        .ok_or(crate::ClassRejectReason::UnknownOrStaleClass)
        .and_then(|def| {
            validate_class_content(world, &def)
                .and_then(|()| crate::progression::validate_class(world, id, &def))?;
            Ok(def)
        });
    match result {
        Err(reason) => {
            world.client_meta_mut(id).class_refusal = Some((class_id, reason));
            diag::warn!(
                Sim,
                "class select: rejected client={} id={} reason={}",
                id.0,
                class_id.0,
                reason.as_str()
            );
            false
        }
        Ok(def) => {
            crate::script::answer_join(world.ecs(), id.0);
            crate::script::choose_class(world.ecs(), id.0, &def);
            let meta = world.client_meta_mut(id);
            meta.equipped_class = Some(class_id);
            meta.class_refusal = None;
            diag::info!(
                Sim,
                "class select: equipped client={} id={}",
                id.0,
                class_id.0
            );
            true
        }
    }
}

fn validate_class_content(
    world: &FrameWorld,
    def: &crate::ClassDef,
) -> Result<(), crate::ClassRejectReason> {
    for (slot, perk) in def.perks.iter().copied().enumerate() {
        if perk != 0 && crate::match_state::perk_slot_from_class_catalog(perk) != Some(slot) {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }
    let max_attachments = if def.perks[0] == 12 { 2 } else { 1 };
    let table_len = world.weapon_combat_len();
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
        if !row.is_usable()
            || !world.weapon_runnable(id)
            || world
                .weapon_setup(id)
                .is_some_and(|setup| setup.attachments.len() > max_attachments)
        {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }
    for (slot, id) in [def.lethal, def.tactical].into_iter().enumerate() {
        if id == 0 {
            continue;
        }
        if (id as usize) >= table_len {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        }
        let Some(facts) = world.offhand_loadout_row(id) else {
            return Err(crate::ClassRejectReason::LockedContent);
        };
        if !world.weapon_runnable(id)
            || !matches!((slot, facts.offhand_class), (0, 1 | 4 | 5) | (1, 2 | 3))
        {
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

pub(crate) fn log_forced_spawn(
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
        .map(|(_, reason)| format!(" fallback={reason}"))
        .unwrap_or(String::new());
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

fn alive_body_clips(world: &FrameWorld) -> Vec<PlayerBodyClip> {
    world
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
        .collect()
}

pub(crate) fn script_slide(
    world: &mut World,
    object: u64,
    origin: [f32; 3],
    slide: &mut crate::script::host::mechanics::Slide,
) -> [f32; 3] {
    let runtime = world.resource::<crate::script::RoundScript>();
    let linked = |mut child: u64| {
        for _ in 0..runtime.entities.len() {
            if child == object {
                return true;
            }
            let parent = runtime
                .player_client(child)
                .and_then(|client| runtime.players.get(&client))
                .and_then(|player| player.link.as_ref().map(|link| link.parent))
                .or_else(|| {
                    runtime
                        .entities
                        .get(&child)
                        .and_then(|entity| entity.linked_to.as_ref().map(|link| link.parent))
                });
            let Some(parent) = parent else {
                return false;
            };
            child = parent;
        }
        false
    };
    let excluded_models: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(id, _)| linked(**id))
        .filter_map(|(_, entity)| entity.presence)
        .collect();
    let excluded_clients: Vec<_> = runtime
        .players
        .iter()
        .filter(|(_, player)| linked(player.object))
        .map(|(client, _)| *client as u16)
        .collect();
    let mut mask = crate::bullet_collision::MASK_PLAYER_SOLID;
    let mut parent = runtime
        .entities
        .get(&object)
        .and_then(|entity| entity.linked_to.as_ref().map(|link| link.parent));
    for _ in 0..runtime.entities.len() {
        let Some(entity) = parent.and_then(|parent| runtime.entities.get(&parent)) else {
            break;
        };
        mask &= !(entity.contents as u32);
        parent = entity.linked_to.as_ref().map(|link| link.parent);
    }
    let frame = FrameWorld::from_world(world);
    let content = frame.content();
    let linked_brushes: Vec<_> = frame
        .entity_collision_capabilities()
        .iter()
        .filter(|row| {
            !row.owner
                .script_model()
                .is_some_and(|model| excluded_models.contains(&model))
        })
        .flat_map(|row| row.solid_brushes().iter().cloned())
        .collect();
    let model_brushes = frame.model_movement_brushes_where(|row| {
        !row.owner
            .script_model()
            .is_some_and(|model| excluded_models.contains(&model))
    });
    let mut bodies = alive_body_clips(&frame);
    bodies.retain(|body| !excluded_clients.contains(&body.entnum));
    let glass_damage = frame.world_objects().glass_damage_pairs();
    let backend = PlayerCollision {
        brushes: content.clip_brushes(),
        bsp: content.clip_bsp(),
        mesh: content.clip_mesh(),
        glass_damage: &glass_damage,
        bodies: &bodies,
        self_entnum: playerstate_iw4::ENTITYNUM_NONE as u16,
        cmodels: &content.clip_cmodels().models,
        linked_brushes: &linked_brushes,
        model_brushes: &model_brushes,
        offset: [0.0; 3],
    };
    slide.advance(origin, &backend, mask)
}

pub(crate) fn script_mantle(
    world: &mut FrameWorld,
    id: ClientId,
    force: bool,
) -> Result<bool, String> {
    let Some(meta) = world.client_meta(id) else {
        return Err("player has disconnected".into());
    };
    if meta.lifecycle != ClientLifecycle::Alive || meta.controls.linked {
        return Ok(false);
    }
    let mut ps = *world.player(id).ok_or("player has not spawned")?;
    if ps
        .origin
        .iter()
        .chain(ps.viewangles.iter())
        .any(|v| !v.is_finite())
    {
        return Err("mantle pose must be finite".into());
    }
    let content = world.content();
    let linked_brushes: Vec<_> = world
        .entity_collision_capabilities()
        .iter()
        .flat_map(|c| c.solid_brushes().iter().cloned())
        .collect();
    let model_brushes = world.model_movement_brushes();
    let bodies = alive_body_clips(world);
    let glass_damage = world.world_objects().glass_damage_pairs();
    let mut backend = PlayerCollision {
        brushes: content.clip_brushes(),
        bsp: content.clip_bsp(),
        mesh: content.clip_mesh(),
        glass_damage: &glass_damage,
        bodies: &bodies,
        self_entnum: id.0 as u16,
        cmodels: &content.clip_cmodels().models,
        linked_brushes: &linked_brushes,
        model_brushes: &model_brushes,
        offset: [0.0; 3],
    };
    let forward = math_iw4::angle_vectors(ps.viewangles).0;
    let mantle = world.xanims();
    let available = movement_iw4::mantle::check(
        &mut ps,
        movement_iw4::MantleCheckContext {
            find: movement_iw4::MantleFindLedgeContext::default(),
            buttons: buttons::JUMP,
            forwardmove: 0,
            facing_xy: [forward[0], forward[1]],
            tracemask: crate::bullet_collision::MASK_PLAYER_SOLID,
        },
        &mut backend,
        mantle.as_ref(),
        mantle.as_ref(),
    );
    if available && force {
        *world.player_mut(id).expect("checked player") = ps;
        world.link_player_standing_area(id);
    }
    Ok(available)
}

pub(crate) fn seed_ps_ammo_tables(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
    clip: i32,
    clip_alt: i32,
    dual: bool,
    stock: i32,
) {
    let ammo_index = weapon_iw4::ammo_table_key(facts.ammo_index, weapon);
    let clip_index = weapon_iw4::clip_table_key(facts.clip_index, weapon);
    if ammo_index != 0 {
        let _ = weapon_iw4::set_ammo_not_in_clip(&mut ps.ammo, ammo_index, stock);
    }
    if clip_index != 0 {
        let _ = weapon_iw4::set_clip_for_hand(&mut ps.ammoclip, clip_index, 0, clip);
        if dual {
            let _ = weapon_iw4::set_clip_for_hand(&mut ps.ammoclip, clip_index, 1, clip_alt);
        }
    }
}

pub(crate) fn arm_held_weapon(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) -> (i32, i32) {
    ps.weapon_primary = weapon;
    let last_hand = weapon_iw4::num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon);
    ps.last_weapon_hand = last_hand;
    let (clip0, clip1, stock) = weapon_iw4::spawn_clip_stock(facts, last_hand);
    let hand = weapon_iw4::spawn_weapon_hand(weapon, facts, true);
    crate::combat::raise_given_weapon(ps, weapon, &hand);
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
