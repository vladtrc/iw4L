#![no_std]
#![forbid(unsafe_code)]

mod accelerate;
mod ads_frac;
mod ads_intent;
mod air;
mod check_prone;
mod cmdscale;
mod collision;
mod correct_solid;
mod crash;
mod dmgtimer;
mod drop_timers;
mod events;
mod footstep;
mod friction;
mod ground;
mod integrate;
mod is_in_air;
mod jump;
mod ladder;
mod mantle;
mod melee_charge;
mod pml;
mod pmove;
mod single;
mod slide;
mod snap;
mod sprint;
mod stance;
mod viewangles;
mod walk;

pub use accelerate::pm_accelerate;
pub use ads_frac::{AdsFracContext, pm_update_ads_frac};
pub use ads_intent::{
    AdsIntentContext, AdsIntentResult, BUTTON_ADS, PMF_ADS_INTENT, pm_update_ads_intent,
};
pub use air::{AirMoveContext, pm_air_move};
pub use check_prone::{PRONE_CHECK_HEIGHT, PRONE_FEET_DIST, bg_check_prone, player_prone_allowed};
pub use cmdscale::{CmdScaleWalkContext, pm_cmd_scale_walk};
pub use collision::CollisionBackend;
pub use correct_solid::{BG_CORRECT_SOLID_DELTAS, CorrectSolidOutcome, pm_correct_solid};
pub use crash::{crash_land_fall_height, pm_crash_land};
pub use dmgtimer::{
    ANIM_MT_FLINCH_FORWARD, PLAYER_DMGTIMER_FLINCH_TIME_MS, PLAYER_DMGTIMER_MAX_TIME,
    PLAYER_DMGTIMER_MIN_SCALE, PLAYER_DMGTIMER_STUMBLE_TIME_MS, PLAYER_DMGTIMER_TIME_PER_POINT,
    pm_damage_scale_walk, pm_damage_window_open, pm_update_damage_timer,
    pm_walk_move_drop_damage_timer,
};
pub use drop_timers::pm_drop_timers;
pub use events::{
    SequencedPlayerEvent, add_predictable_event, consume_player_events, pm_add_event,
};
pub use footstep::{
    LADDER_SURFACE_FLAGS, LADDER_SURFACE_TYPE, SURFACE_TYPE_NAMES, bob_cycle_wrapped,
    pm_footstep_event, pm_footstep_event_type, pm_footsteps_anim_move_type, pm_footsteps_bob_cycle,
    pm_get_bob_max_speed, pm_ladder_footsteps, pm_should_make_footsteps, surface_type_index,
    surface_type_name, surface_type_to_name,
};
pub use friction::pm_friction;
pub use ground::complete_ground_trace;
pub use integrate::pm_predict_integrate;
pub use is_in_air::pm_is_in_air;
pub use jump::{
    JumpAnimation, JumpCheckContext, JumpCheckResult, JumpLaunchContext, jump_check,
    jump_check_gate, jump_clear_state, jump_get_step_height, jump_stance_allows,
    pm_ground_surface_type, pm_jump_event, pm_jump_push_off_ladder, pm_jump_start,
};
pub use ladder::{
    CheckLadderContext, LADDER_ATTRACT_SPEED, LADDER_JUMP_BLOCK_MS, LADDER_TRACE_DIST_AIR,
    LADDER_TRACE_DIST_WALK, LadderAttachBackend, LadderMoveContext, LadderTraceHit, PMF_LADDER,
    PMF_LADDER_FALL, SURF_LADDER, pm_check_ladder_move, pm_clear_ladder_flag,
    pm_ladder_attract_velocity, pm_ladder_move, pm_set_ladder_flag,
};
pub use mantle::{
    CONTENTS_MANTLE, CreateAnimsMantleRootDelta, FlatMantleAnimLength, MANTLE_CHECK_RADIUS_DEFAULT,
    MANTLE_CHECK_RANGE_DEFAULT, MANTLE_CLEARANCE_MAXS_Z, MANTLE_FORWARD_DIST, MANTLE_FRONT_MAXS_Z,
    MANTLE_HALF_WIDTH, MANTLE_LEDGE_FLOOR_Z, MANTLE_LEDGE_HEIGHTS, MANTLE_OVER_FORWARD,
    MANTLE_PLAYER_RADIUS, MANTLE_VIEW_YAWCAP_DEFAULT, MANTLE_XANIM_NAMES, MANTLE_XANIM_NAMES_FR,
    MANTLE_XANIM_TREE_SIZE, MantleCapViewContext, MantleCapsuleTrace, MantleCheckContext,
    MantleFindLedgeContext, MantleFrontProbeCast, MantleLedgeBackend, MantleLedgeProbe,
    MantleLedgeProbeLog, MantleMoveContext, MantleResults, MantleRootDelta, MantleXAnimLength,
    PMF_MANTLE, SURF_MANTLE_ON_OR_OVER, SURF_MANTLE_OVER, ZeroMantleRootDelta, mantle_active_xanim,
    mantle_calc_end_pos, mantle_calc_path, mantle_cap_view, mantle_check, mantle_clear_hint,
    mantle_create_anims_end_delta, mantle_duration, mantle_enter, mantle_find_ledge,
    mantle_find_ledge_recording, mantle_find_transition, mantle_front_probe_accept,
    mantle_front_probe_along, mantle_front_probe_cast, mantle_height_landing_probe,
    mantle_is_weapon_inactive, mantle_move, mantle_over_length, mantle_sample_root_track,
    mantle_start_allowed, mantle_start_clearance, mantle_trans_over_anim, mantle_trans_up_anim,
    mantle_up_length, vector_angle_multiply,
};
pub use melee_charge::{
    MeleeChargeWeaponDelays, PLAYER_MELEE_RANGE_DEFAULT as MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
    pm_calc_melee_charge_time, pm_melee_charge_clear, pm_melee_charge_move,
};
pub use pml::Pml;
pub use pmove::Pmove;
pub use single::{
    GroundTraceInput, MoveBounds, PmoveResult, PmoveSingle, PmoveSingleContext, pm_move,
};
pub(crate) use slide::pm_project_velocity;
pub use slide::{pm_slide_move, pm_step_slide_move};
pub use snap::{pm_end_tick_velocity, snap_vector};
pub use sprint::{
    PERK_MARATHON, PMF_SPRINTING, SprintContext, SprintResult, bg_get_max_sprint_time,
    pm_end_sprint, pm_sprint_ending_buttons, pm_sprint_start_interfering_buttons, pm_update_sprint,
    sprint_forward_below_minimum, sprint_recharge_penalty_ms, sprint_time_remaining,
};
pub use stance::{
    CROUCH_MAXS_Z, PMF_CROUCH, PMF_PRONE, PRONE_MAXS_Z, STAND_MAXS_Z, StanceChange, StanceSurface,
    pm_sync_stance_tail, pm_update_stance_flags, pm_update_stance_target, pm_update_view_height,
    stance_speed_scale, stance_surface_type, view_height, view_height_lerp_duration,
};
pub use viewangles::{ANGLE2SHORT, SHORT2ANGLE, ViewAngleClamp, pm_update_view_angles};
pub use walk::{WalkMoveContext, pm_walk_move};
