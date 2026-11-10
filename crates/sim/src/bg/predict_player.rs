use crate::bg::player_collision::PlayerBodyClip;
use crate::bg::weapon_animation::{AnimationInputs, PlayerWeaponAnimation, apply_animation_event};
use crate::{
    ClientId, ClientSnapshotMeta, CollisionScene, CommandSequence, MatchPhase, SimContent, Tick,
};
use playerstate_iw4::{PlayerState, ShotId, UserCmd};

pub struct PlayerCommandState {
    pub old_buttons: u32,
    pub old_angles: Option<[i32; 3]>,
    pub last_movetype: Option<u8>,
    pub strafing: u8,
    pub animation_buttons: u32,
    pub animation_seed: u32,
    next_shot: ShotId,
}

impl Default for PlayerCommandState {
    fn default() -> Self {
        Self {
            old_buttons: 0,
            old_angles: None,
            last_movetype: None,
            strafing: 0,
            animation_buttons: 0,
            animation_seed: 1,
            next_shot: ShotId(1),
        }
    }
}

pub struct PredictionWorld<'a> {
    pub content: &'a SimContent,
    pub collision: &'a CollisionScene,
    pub bodies: &'a [PlayerBodyClip],
}

pub struct PredictionCommand {
    pub local: ClientId,
    pub tick: Tick,
    pub sequence: CommandSequence,
    pub cmd: UserCmd,
}

pub fn predict_player(
    ps: &mut PlayerState,
    meta: &mut ClientSnapshotMeta,
    state: &mut PlayerCommandState,
    world: PredictionWorld<'_>,
    phase: MatchPhase,
    root_seed: u64,
    command: PredictionCommand,
) -> Vec<weapon_iw4::WeaponAction> {
    let PredictionWorld {
        content,
        collision,
        bodies,
    } = world;
    let PredictionCommand {
        local,
        tick,
        sequence,
        mut cmd,
    } = command;
    let mut actions = Vec::new();
    let delta = cmd.server_time.wrapping_sub(ps.command_time);
    if matches!(phase, MatchPhase::Playing | MatchPhase::Warmup)
        && meta.lifecycle == crate::ClientLifecycle::Alive
        && delta > 0
    {
        let facts = content.weapon_combat_facts(ps.weapon);
        let context = crate::bg::player_step::context_for_player(
            content,
            ps,
            state.old_buttons,
            meta.shellshock.as_ref().is_some_and(|shock| shock.movement),
        );
        crate::bg::player_step::constrain_cmd(ps.weapon, &mut meta.controls, &mut cmd);
        if meta.location_selection.is_some() {
            cmd.buttons &= playerstate_iw4::buttons::CROUCH | playerstate_iw4::buttons::PRONE;
        }
        if meta.remote_missile.is_some() {
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
        state.animation_buttons = cmd.buttons;
        let scene = collision.player_collision();
        let backend = scene.backend(bodies, local.0 as u16, ps.origin);
        let moved = crate::bg::player_step::player_step(
            ps,
            &mut cmd,
            context,
            &backend,
            content.xanims().as_ref(),
            crate::level_time_ms(tick),
        );
        if let Some(atlas) = content.atlas() {
            atlas.carry_player_checked_with_availability(
                ps,
                moved.moved_from,
                moved.moved_to,
                moved.linked_bounds,
                collision.portal_blocked_exits(),
                |input| {
                    let exit_backend = scene.backend(bodies, local.0 as u16, input.start);
                    movement_iw4::CollisionBackend::trace(&exit_backend, input)
                },
            );
        }
        if moved.reset_torso {
            crate::player_anim_script::reset_stance_torso(ps);
        }
        let animation = AnimationInputs {
            script: content.player_anim_script().as_deref(),
            last_movetype: state.last_movetype,
            strafing: state.strafing,
            buttons: state.animation_buttons,
        };
        if let Some(event) = moved.stance_event {
            apply_animation_event(
                ps,
                content,
                &animation,
                &mut state.animation_seed,
                event,
                None,
                false,
            );
        }
        for (animation_event, force) in moved.jump_animations.into_iter().flatten() {
            let event = match animation_event {
                movement_iw4::JumpAnimation::Forward => 3,
                movement_iw4::JumpAnimation::Backward => 4,
            };
            apply_animation_event(
                ps,
                content,
                &animation,
                &mut state.animation_seed,
                event,
                None,
                force,
            );
        }
        if moved.landing_animation {
            apply_animation_event(
                ps,
                content,
                &animation,
                &mut state.animation_seed,
                5,
                None,
                false,
            );
        }
        if let Some(movetype) = moved.anim_movetype {
            let strafing =
                crate::player_anim_script::anim_strafing(ps, cmd.forwardmove, cmd.rightmove);
            let conditions = crate::anim_conditions_from_pmove(
                ps,
                content.weapon_combat_facts(moved.view_w),
                content.weapon_combat_facts(moved.primary),
                state.last_movetype,
                strafing,
                cmd.buttons,
            );
            if let Some(script) = content.player_anim_script().as_ref()
                && let Some(selected) = script.apply(
                    ps,
                    movetype,
                    local.0,
                    &conditions,
                    moved.force_movement_anim,
                )
            {
                state.last_movetype = Some(selected);
                state.strafing = strafing;
            }
        }
        let animation = AnimationInputs {
            script: content.player_anim_script().as_deref(),
            last_movetype: state.last_movetype,
            strafing: state.strafing,
            buttons: state.animation_buttons,
        };
        let can_fire = content
            .weapons()
            .equipment_runtime
            .get(ps.weapon as usize)
            .copied()
            .filter(|facts| facts.projectile_speed > 0)
            .is_none_or(|facts| {
                !facts.require_lock_to_fire
                    || meta.weapon_lock.can_fire(ps.weapon, meta.life_sequence.0)
            });
        let inputs = weapon_iw4::FireInputs {
            cmd,
            root_seed,
            client: local,
            life: meta.life_sequence,
            command: Some(sequence),
            mantle_weapon_inactive: movement_iw4::mantle::is_weapon_inactive(ps, true),
            linked_origin: meta.linked_weapon_view.map(|view| view.origin),
            old_buttons: state.old_buttons,
            old_angles: state.old_angles.unwrap_or(cmd.angles),
            msec: delta.min(200),
            can_fire,
        };
        let mut animator = PlayerWeaponAnimation {
            content,
            inputs: animation,
            seed: &mut state.animation_seed,
        };
        if let Some(step) =
            weapon_iw4::pm_weapon(ps, &mut meta.weapon_carry, content, &inputs, &mut animator)
        {
            for entry in step.events.into_iter().flatten() {
                let shot = matches!(entry.1, weapon_iw4::WeaponEvent::Fire { .. }).then(|| {
                    let shot = state.next_shot;
                    state.next_shot = shot.next();
                    shot
                });
                actions.push(step.apply_event(
                    weapon_iw4::WeaponEventInstance {
                        hand: entry.0,
                        event: entry.1,
                        shot,
                    },
                    ps,
                    &mut meta.weapon_carry,
                    &inputs,
                    content,
                    &mut animator,
                ));
            }
            step.finish_events(&mut meta.weapon_carry, meta.life_sequence);
        }
        if cmd.off_hand_index != 0 {
            ps.off_hand_index = i32::from(cmd.off_hand_index);
        }
        state.old_buttons = cmd.buttons;
        state.old_angles = Some(cmd.angles);
    } else if !matches!(phase, MatchPhase::Playing | MatchPhase::Warmup) {
        state.old_buttons = cmd.buttons;
        state.old_angles = Some(cmd.angles);
    }
    let mut cursor = ps.old_event_sequence;
    movement_iw4::consume_player_events(ps, &mut cursor, |_| {});
    ps.old_event_sequence = cursor;
    actions
}
