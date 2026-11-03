use movement_iw4::{
    AirMoveContext, CmdScaleWalkContext, JumpLaunchContext, SprintContext, ViewAngleClamp,
    WalkMoveContext, get_max_sprint_time,
};
use movement_iw4::{
    CollisionBackend, MoveBounds, PmoveSingleContext, footsteps_anim_move_type, pmove,
};
use playerstate_iw4::{PlayerState, UserCmd};

pub struct PlayerMoveOutput {
    pub walking: i32,
    pub linked_bounds: MoveBounds,
    pub anim_movetype: Option<u8>,
    pub view_w: u32,
    pub primary: u32,
    pub moved_from: [f32; 3],
    pub moved_to: [f32; 3],
    pub stance_event: Option<u8>,
    pub reset_torso: bool,
    pub jump_animations: [Option<(movement_iw4::JumpAnimation, bool)>; 4],
    pub force_movement_anim: bool,
    pub landing_animation: bool,
    pub fall_damage: i32,
}

/// Advance one player against explicit immutable collision and movement facts.
/// Authority applies damage, links and script notifications from the returned facts.
pub fn player_step(
    ps: &mut PlayerState,
    cmd: &mut UserCmd,
    context: PmoveSingleContext,
    backend: &impl CollisionBackend,
    mantle: &crate::MantleXAnimBind,
    level_time: i32,
) -> PlayerMoveOutput {
    if ps.shellshock_time.wrapping_add(ps.shellshock_duration) < level_time {
        ps.pm_flags &= !playerstate_iw4::pm_flags::SHELLSHOCKED;
    }
    let moved_from = ps.origin;
    let result = pmove(ps, cmd, context, backend, mantle, mantle);
    let pml = result.pml;
    let anim_movetype = pml.mantle_movetype.or_else(|| {
        footsteps_anim_move_type(
            ps,
            cmd.forwardmove,
            cmd.rightmove,
            pml.almost_ground_plane != 0,
        )
    });
    let (view_w, primary) = crate::pmove_anim_weapon_ids(ps);
    PlayerMoveOutput {
        walking: pml.walking as i32,
        linked_bounds: result.bounds,
        anim_movetype,
        view_w,
        primary,
        moved_from,
        moved_to: ps.origin,
        stance_event: result.stance_event,
        reset_torso: result.reset_torso,
        jump_animations: pml.jump_animations,
        force_movement_anim: pml.mantle_movetype.is_some(),
        landing_animation: pml.landing_animation,
        fall_damage: pml.fall_damage,
    }
}

pub fn constrain_cmd(
    held: u32,
    controls: &mut crate::ScriptControls,
    cmd: &mut playerstate_iw4::UserCmd,
) {
    use playerstate_iw4::buttons;
    if controls.switch_to != 0 && controls.switch_to == held {
        controls.switch_to = 0;
    }
    if controls.frozen {
        cmd.forwardmove = 0;
        cmd.rightmove = 0;
        cmd.buttons &= buttons::PRONE | buttons::CROUCH | buttons::STANCE_HELD;
    }
    if controls.linked {
        cmd.forwardmove = 0;
        cmd.rightmove = 0;
        cmd.buttons &= !(buttons::JUMP | buttons::SPRINT);
    }
    if controls.jump_disabled {
        cmd.buttons &= !buttons::JUMP;
    }
    if controls.weapons_disabled {
        cmd.buttons &= !(buttons::ATTACK | buttons::THROW | buttons::ADS | buttons::MELEE_CHARGE);
    }
    if controls.offhands_disabled || controls.weapons_disabled {
        cmd.buttons &= !(buttons::FRAG | buttons::SMOKE);
    }
    if controls.usability_disabled {
        cmd.buttons &= !(buttons::USE | buttons::USE_RELOAD);
    }
    if controls.switch_to != 0 {
        cmd.weapon = controls.switch_to as u16;
    } else if controls.switch_disabled || controls.frozen {
        cmd.weapon = held as u16;
    }
}

pub fn pmove_context(
    old_buttons: u32,
    weapon_scales: (f32, f32, f32),
    ads_intent: movement_iw4::AdsIntentContext,
    ads_frac: movement_iw4::AdsFracContext,
    melee_charge: movement_iw4::MeleeChargeWeaponDelays,
    can_hold_breath: bool,
    shellshock_affects_movement: bool,
) -> PmoveSingleContext {
    let player_sprint_time = 4.0_f32;
    let weapon_max_sprint_time = get_max_sprint_time(weapon_scales.2, player_sprint_time);

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
        ads_intent,
        ads_frac,
        melee_charge,
        player_melee_range: movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
        old_buttons,
        weapon_blocks_prone: false,
        can_hold_breath,
    }
}

pub fn context_for_player(
    content: &crate::SimContent,
    ps: &PlayerState,
    old_buttons: u32,
    shellshock_affects_movement: bool,
) -> PmoveSingleContext {
    let weapon = ps.weapon;
    let scales = content
        .weapons()
        .weapon_def_scales
        .get(weapon as usize)
        .copied()
        .unwrap_or((0.0, 0.0, 1.0));
    let facts = content.weapon_combat_facts(weapon);
    let aim_down_sight = facts.is_some_and(|f| f.aim_down_sight);
    let ads_allowed = facts.is_some_and(|f| {
        weapon != 0 && {
            let mut allow = f.ads_allow_facts();

            allow.clip_index = weapon_iw4::clip_table_key(allow.clip_index, weapon);
            weapon_iw4::is_ads_allowed(&ps, &allow, &ps.ammoclip, ps.last_weapon_hand.min(1) as u8)
        }
    });
    let overlay_reticle = facts.map(|f| f.overlay_reticle).unwrap_or(0);
    let (ads_in_rate, ads_out_rate, rechamber_while_ads, ads_fire_only) = facts
        .map(|f| f.ads_frac_context())
        .unwrap_or((1.0 / 200.0, 1.0 / 200.0, true, false));
    let (melee_delay_ms, melee_charge_delay_ms) = facts
        .map(|f| (f.melee_delay_ms, f.melee_charge_delay_ms))
        .unwrap_or((0, 0));
    let mut context = pmove_context(
        old_buttons,
        scales,
        movement_iw4::AdsIntentContext {
            ads_allowed,
            weapon_def_scope: overlay_reticle != 0,
            sprint_hold_ads: false,
        },
        movement_iw4::AdsFracContext {
            aim_down_sight,
            ads_in_rate,
            ads_out_rate,
            ads_reload_trans_time_ms: facts.map_or(0, |f| f.ads_reload_trans_time_ms),
            segmented_reload: facts.is_some_and(|f| f.segmented_reload),
            rechamber_while_ads,
            ads_fire_only,
        },
        movement_iw4::MeleeChargeWeaponDelays {
            melee_delay_ms,
            melee_charge_delay_ms,
        },
        facts.is_some_and(|f| f.can_hold_breath),
        shellshock_affects_movement,
    );
    context.view_angles.unclamped_pitch_bit =
        ps.link_flags & playerstate_iw4::LINK_FLAGS_WEAPON_VIEW_ONLY != 0;
    context
}
