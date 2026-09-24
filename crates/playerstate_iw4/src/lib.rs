#![no_std]
#![forbid(unsafe_code)]

mod archive;
mod chrome;
mod playerstate;
mod seat;
mod third_person;
mod usercmd;

pub use archive::{REMAPPED_PS_TIMERS, RemappedTimer, remapped_timer};
pub use chrome::{
    KILLCAM_DEFAULT_LERP_MS, KILLCAM_ENTER_STEPS, KILLCAM_EXIT_STEPS, KILLCAM_TURRET_LERP_MS,
    KillCamMode, KillcamEnterStep, KillcamExitStep, killcam_lerp_deadline_ms,
    mode0_from_kill_cam_entity, third_person_in_killcam,
};
pub use playerstate::{
    AnimPair, ENTITYNUM_NONE, PERK_COLDBLOODED, PERK_HEARTBREAKER, PERK_PISTOLDEATH, PERK_QUIETER,
    PlayerState, bg_get_viewmodel_weapon_index, eflags, mantle_flags, other_flags, pm_flags,
    weap_flags,
};
pub use seat::{
    HITSCAN_KILL_CAM_ENTITY, SeatFocus, apply_killcam_seat, rebase_archived_timers,
    seat_matches_archived_except_exceptions,
};
pub use third_person::{
    CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, CG_CAMERA_PULLBACK_Z_BIAS,
    CG_THIRD_PERSON_FOCUS_Z, CG_THIRD_PERSON_PITCH_CLAMP, CG_THIRD_PERSON_PITCH_SCALE,
    CG_THIRD_PERSON_RANGE_DEFAULT, CgIsThirdPersonViewInputs, GENTITY_SPAWN_BASE,
    LINK_FLAGS_FORCE_THIRD_PERSON, MAX_CLIENT_CORPSES, OffsetThirdPersonViewInputs,
    PLAYER_CORPSE_ENTITY_BASE, PM_TYPE_DEAD, PM_TYPE_DEAD_LINKED, PM_TYPE_INTERMISSION,
    PM_TYPE_LAST_STAND, PM_TYPE_NORMAL_LINKED, PM_TYPE_SPECTATOR, ThirdPersonView,
    cg_is_third_person_view, look_at_killer_yaw, offset_third_person_view,
    pull_back_camera_through_brush, viewweapon_frame_runs,
};
pub use usercmd::{UserCmd, buttons};
