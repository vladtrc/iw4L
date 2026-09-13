#![no_std]
#![forbid(unsafe_code)]

mod ads_allow;
mod ads_overlay;
mod ammo;
pub mod event_sound;
mod fire_sound;
mod fire_weapon;
mod kick;
mod melee;
mod offhand;
mod penetration;
mod placement;
mod player_anim_type;
mod pm_weapon;
mod reload;
mod spread;
mod sprint;
mod sway;
mod unlocated;
mod view_bob;
mod viewmodel;
mod viewweapon;
mod weap_anim;
mod weap_anim_rate;
mod weapon_change;
mod weaponcomplete;
mod weapondef;
mod weaponstate;

pub use ads_allow::{AdsAllowWeaponFacts, OTHER_FLAG_PLAYER, WEAP_FLAG_NO_ADS, pm_is_ads_allowed};
pub use ads_overlay::{AdsOverlayScrub, ads_overlay_scrub};
pub use ammo::{
    AMMO_TABLE_BYTES, AMMOCLIP_TABLE_BYTES, WEAPON_DATA_BYTES, bg_ammo_row_present,
    bg_ammo_table_key, bg_clip_row_present, bg_clip_table_key, bg_create_akimbo_viewmodel_trees,
    bg_ensure_clip_row, bg_get_ammo_index, bg_get_ammo_not_in_clip, bg_get_ammo_player_both_clips,
    bg_get_clip_for_hand, bg_get_clip_index, bg_get_total_ammo_in_clips,
    bg_get_weapon_dual_wield_byte, bg_has_akimbo_viewmodel_anims, bg_latch_weapon_dual_wield,
    bg_player_weapons_find_slot, bg_set_ammo_not_in_clip, bg_set_clip_for_hand,
    bg_set_weapon_dual_wield_byte, bg_spend_clip_for_hand, pm_num_hands, pm_num_hands_for_held,
};
pub use event_sound::{
    EV_RELOAD, EV_RELOAD_END, EV_RELOAD_FROM_EMPTY, EV_RELOAD_START, play_note_mapped_rumble_alias,
    play_note_mapped_sound_alias, pm_begin_reload_event, pm_reload_insert_event,
    slot_for_event as weapondef_sound_slot,
};
pub use fire_sound::{select_cg_fire_sound_ptr, select_fire_last_sound_ptr, select_fire_sound_ptr};
pub use fire_weapon::{
    FireWeaponKind, WEAPCLASS_GRENADE, WEAPTYPE_BULLET, WEAPTYPE_GRENADE, WEAPTYPE_PROJECTILE,
    fire_weapon_kind,
};
pub use kick::{
    DOUBLEBARREL_PITCH_SCALE, FireRecoilImpulse, FireRecoilPsScales, GUN_KICK_OFS_EPS,
    GUN_KICK_SPEED_EPS, GunKickRange, GunKickSpring, GunRecoilPlacementState,
    KICK_AVEL_ROLL_FROM_YAW, KICK_STEP_MS, MS_TO_SEC, RECOIL_SCALE_DIVISOR,
    REDUCED_KICK_PERCENT_SCALE, VIEW_KICK_ADS_FRAC, VIEW_KICK_CLAMP_DEG,
    VIEW_KICK_NO_WEAPON_CENTER_SPEED, VIEW_KICK_RETURN_SCALE, ViewKickRange,
    bg_calculate_weapon_position_gun_recoil, bg_weapon_fire_recoil, cg_kick_angles,
    cg_kick_angles_step_axis, fire_recoil_amplitude_scale, fire_recoil_gun_range,
    fire_recoil_pitch_extra_scale, fire_recoil_reduce_scale, fire_recoil_view_range,
    gun_recoil_angle_contribution, gun_recoil_single_angle, kick_angles_center_speed,
    lerp_gun_kick_spring, start_firing_restrict_kick_time,
};
pub use melee::{
    BUTTON_MELEE, MELEE_TRACE_OFFSETS, MeleeChargeState, MeleeWeaponFacts,
    PLAYER_MELEE_HEIGHT_DEFAULT, PLAYER_MELEE_RANGE_DEFAULT, PLAYER_MELEE_WIDTH_DEFAULT,
    melee_trace_count, melee_trace_end, melee_weaponstate_blocks, pm_melee_charge_start,
    pm_weapon_advance_melee, pm_weapon_has_charge_melee, pm_weapon_melee_to_end,
    pm_weapon_melee_to_fire, pm_weapon_settle_ready, pm_weapon_start_melee,
    pm_weapon_start_melee_uses_charge, pm_weapon_try_melee,
};
pub use offhand::{
    BUTTON_FRAG, BUTTON_SMOKE, CURSOR_HINT_NONE, OFFHAND_INV_SLOTS, OffhandCmd, OffhandInvRow,
    bg_get_first_available_offhand, pm_weapon_advance_offhand, pm_weapon_check_for_offhand,
    pm_weapon_enter_offhand, pm_weapon_offhand_end, pm_weapon_offhand_hold,
    pm_weapon_offhand_prepare, pm_weapon_offhand_start, pm_weapon_offhand_throw,
    pm_weapon_update_grenade_throw,
};
pub use penetration::{
    ADVANCE_DOT_MIN, ADVANCE_TRACE_FWD, ADVANCE_TRACE_REV, BulletPenFacts, CONTENTS_GLASS,
    MAX_EXTENDED_STEPS, MAX_PENETRATE_STEPS, PEN_SURF_TYPE_COUNT, PEN_THICKNESS_FLOOR,
    PENETRATE_TYPE_COUNT, PenetrationDepthTable, REV_END_EPS, RIFLE_COLLATERAL_SCALE,
    SURF_NOPENETRATE, SURF_TYPE_FLESH, SURFACE_TYPE_NAMES, bg_advance_trace, depth_surface_type,
    split_pen_key,
};
pub use placement::{
    DUAL_WIELD_VIEW_MODEL_OFFSET_LEFT_SCALE, GUN_DAMAGE_ADS_HALF, GUN_DAMAGE_DEFLECT_MS,
    GUN_DAMAGE_OVERLAY_MIX, GUN_DAMAGE_RETURN_MS, PLACEMENT_ASSEMBLE_STEP_COUNT,
    PMF_LADDER as PMF_LADDER_PLACEMENT, StanceTransitionFadeGlobals, VIEWHEIGHT_TARGET_CROUCH,
    VIEWHEIGHT_TARGET_PRONE, WEAPON_BOB_AMP_DUCKED, WEAPON_BOB_AMP_PRONE, WEAPON_BOB_AMP_SPRINTING,
    WEAPON_BOB_AMP_STANDING, WEAPON_BOB_AMPLITUDE_BASE, WEAPON_BOB_AMPLITUDE_ROLL, WEAPON_BOB_LAG,
    WEAPON_BOB_MAX, WEAPON_BOB_UP_PHASE, WEAPON_IDLE_AMOUNT_DEFAULT, WEAPON_IDLE_FACTOR_LERP,
    WEAPON_IDLE_PITCH_FREQ, WEAPON_IDLE_ROLL_FREQ, WEAPON_IDLE_SIN_SCALE,
    WEAPON_IDLE_TIME_MS_SCALE, WEAPON_IDLE_YAW_FREQ, WeaponBobInputs, WeaponBobState,
    WeaponBobWaveformInputs, WeaponIdleInputs, WeaponMovementKinematics, WeaponMovementOfsInputs,
    WeaponPlacementAssembleStep, WeaponPlacementContribution, WeaponPlacementPsInputs,
    WeaponPlacementState, WeaponStanceStaticOfsInputs, base_stance_movement_angles,
    bg_apply_idle_sway_scale, bg_calculate_weapon_movement_bob,
    bg_calculate_weapon_movement_bob_waveform, bg_calculate_weapon_movement_targets,
    bg_stance_movement_lerp, bg_weapon_damage_kick_angles, bg_weapon_idle_amount_speed,
    bg_weapon_stance_static_ofs, dual_wield_view_model_origin_add,
    placement_movement_channel_scale, stance_transition_fade, weapon_bob_add_to_angles,
    weapon_bob_ads_attenuation, weapon_bob_apply_ads_attenuation, weapon_bob_rotate_origin,
    weapon_bob_set_waveform, weapon_placement_apply_angles, weapon_placement_apply_origin,
    weapon_placement_assemble, weapon_placement_jump_land_ofs,
};
pub use player_anim_type::{PLAYER_ANIM_TYPE_COUNT, PLAYER_ANIM_TYPE_NAMES};
pub use pm_weapon::{
    BURST_COOLDOWN_DEFAULT_MS, BUTTON_ATTACK, BUTTON_RELOAD, BUTTON_THROW,
    CHECK_FIRING_AMMO_DRY_FIRE_MS, CapturedCombatInput, MissingCombatFacts, PERK_FASTRELOAD,
    PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT, WeaponCmd, WeaponCombatFacts, WeaponHandState,
    WeaponTickEvent, perk_fastreload_eligible, pm_get_weapon_fire_button, pm_weapon_hands,
    pm_weapon_ordinary, pm_weapon_time_adjust, spawn_clip_stock, spawn_weapon_hand,
};
pub use reload::{
    ReloadDelayedOutcome, pm_reload_clip, pm_weapon_allow_reload, pm_weapon_arm_reload_add_delay,
    pm_weapon_process_input_wants_reload, pm_weapon_reload_delayed_action,
    reload_weaponstate_may_credit,
};
pub use spread::{
    AIM_SPREAD_AIR_DECAY, AIM_SPREAD_AIR_VIEWCHANGE, AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT,
    AIM_SPREAD_SCALE_MAX, AIM_SPREAD_TURN_SCALE, AimSpreadMotion, AimSpreadState,
    PERK_BULLETACCURACY, PERK_WEAP_SPREAD_MULTIPLIER_DEFAULT, PM_TYPE_NORMAL_LINKED, SHORT2ANGLE,
    SpreadCone, SpreadOverrideState, VIEWHEIGHT_CROUCH_SEAM, VIEWHEIGHT_PRONE,
    VIEWHEIGHT_PRONE_SPAN, VIEWHEIGHT_STAND_SPAN, WeaponAimSpreadDecayFacts, WeaponSpreadFacts,
    bg_get_spread_for_weapon, fire_weapon_spread_degrees, perk_weap_spread_multiplier,
    pm_add_aim_spread_fire, pm_adjust_aim_spread_scale,
};
pub use sprint::{PMF_SPRINTING, pm_weapon_advance_sprint, pm_weapon_check_for_sprint};
pub use sway::{
    SWAY_FRAME_HZ, SWAY_SHELLSHOCK_SMOOTH_PEAK, SwayContribution, SwaySpringState, TRACK_SNAP_EPS,
    WeaponSwayParams, angle_delta, angle_normalize_180, bg_calculate_weapon_movement_sway,
    clamp_abs, lerp_sway_params, sway_contribution, sway_shellshock_landing_scale, track, track_a,
};
pub use view_bob::{
    BG_VIEW_KICK_MAX, BG_VIEW_KICK_MIN, BG_VIEW_KICK_SCALE, EFLAGS_TURRET_VEHICLE, LAND_DEFLECT_MS,
    LAND_END_MS, LAND_RETURN_MS, LAND_VIEW_DIP_FALL_IN, LAND_VIEW_DIP_MAX,
    PERK_LIGHTWEIGHT_VIEW_BOB_BIT, PERK_LIGHTWEIGHT_VIEW_BOB_SCALE, VIEW_BOB_AMP_DUCKED,
    VIEW_BOB_AMP_DUCKED_ADS, VIEW_BOB_AMP_PRONE, VIEW_BOB_AMP_SPRINTING, VIEW_BOB_AMP_STANDING,
    VIEW_BOB_AMP_STANDING_ADS, VIEW_BOB_MAX, VIEW_DAMAGE_DEFLECT_MS, VIEW_DAMAGE_RETURN_MS,
    VIEW_DAMAGE_UNDIRECTED, VIEW_ORG_BOB_Z_MIN_OFS, VIEWWEAPON_LAND_SCALE, ViewAngleBob,
    ViewAngleBobInputs, ViewDamageFeedback, ViewOrgBob, ViewOrgBobInputs, bg_calc_view_bob_pitch,
    bg_calc_view_bob_roll, bg_crash_land_fall_height, bg_crash_land_view_dip,
    bg_land_origin_weight, bg_land_origin_z, bg_should_apply_view_org_bob, bg_view_angle_bob,
    bg_view_bob_cycle, bg_view_damage_angles, bg_view_kick_amplitude, bg_view_org_bob,
    bg_viewweapon_land_origin_z, cg_damage_feedback_kick,
};
pub use viewmodel::bg_get_viewmodel_weapon_index;
pub use viewweapon::{
    viewweapon_composed_world_forward, viewweapon_iron_ads_saves_composed_axis,
    viewweapon_save_gun_pitch_yaw, viewweapon_save_offset_movement, viewweapon_view_to_world_delta,
};
pub use weap_anim::{
    WEAP_ANIM_RESTART_BIT, pm_continue_weapon_anim, pm_set_fps_fire_anim, pm_set_rechamber_anim,
    pm_set_weap_anim, pm_start_weapon_anim, pm_weapon_idle_weap_anim, weap_anim_event,
};
pub use weap_anim_rate::{
    ACTION_GOAL_TIME_SECS, ACTIVE_GOAL_WEIGHT, ANIM_RATE_TABLE, AnimRateOffsets,
    IDLE_INTERRUPT_GOAL_TIME_SECS, INACTIVE_GOAL_WEIGHT, WEAP_ANIM_EVENT_MASK,
    known_complete_rate_timer_offset, known_rate_timer_offset, playback_rate,
    slot_for_weap_anim_event, slot_uses_native_rate,
};
pub use weapon_change::{
    PMF_CHANGE_BLOCK, PMF_LADDER as PMF_LADDER_WEAPON, check_for_change_admits,
    finish_putaway_to_cmd, finish_putaway_while_holstered, pm_begin_weapon_change,
    pm_weapon_check_for_change, traversal_forces_holster,
};
pub use weaponcomplete::WeaponCompleteDef;
pub use weapondef::WeaponDef;
pub use weaponstate::{
    FireType, WeaponDecodeError, WeaponState, viewmodel_rocket_should_be_attached,
};
