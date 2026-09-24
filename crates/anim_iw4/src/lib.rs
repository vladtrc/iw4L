#![no_std]
#![forbid(unsafe_code)]

mod channel;
mod compose;
mod dobj;
mod dobj_bounds;
mod goal_weight;
mod hide;
mod part_bits;
mod player_anim;
mod quat;
mod sample;
mod skel;
mod xanim;
mod xanim_time;
mod xbone;

pub use channel::{full_quat, half_quat, quantized_trans, quat16};
pub use compose::{Local, compose_rotation, compose_translation};
pub use dobj::DObj;
pub use dobj_bounds::{
    DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT, DOBJ_RADIUS_PARENT_ROOT, dobj_compute_bounds_radius,
};
pub use goal_weight::{
    XANIM_CLIENT_ANIM_BLEND_FLOOR_NEW_MS, XANIM_CLIENT_ANIM_BLEND_FLOOR_NO_DURATION_MS,
    XANIM_CLIENT_ANIM_BLEND_FLOOR_OLD_DURATION_MS, XANIM_CLIENT_ANIM_CLEAR_BLEND_MS,
    XANIM_CLIENT_ANIM_RATE_CAP, XANIM_CLIENT_ANIM_RATE_LADDER_CAP,
    XANIM_CLIENT_ANIM_RATE_LERP_MAX_SPEED, XANIM_CLIENT_ANIM_RATE_LERP_MIN_SPEED,
    XANIM_CLIENT_ANIM_RATE_LERP_SPAN, XANIM_CLIENT_ANIM_RATE_MIN, XANIM_CLIENT_ANIM_RATE_SHORT_CAP,
    XANIM_CLIENT_ANIM_RATE_ZERO_EPS, XANIM_GOAL_WEIGHT_SNAP_EPS,
    XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO, xanim_client_anim_blend_ms,
    xanim_client_anim_playback_rate, xanim_goal_time_from_blend_ms, xanim_sanitize_goal_weight,
    xanim_vec3_distance,
};
pub use hide::{dobj_surface_hidden, hide_part_bit, or_shift_part_bits, set_hide_part_bit};
pub use part_bits::{PartBits, xmodel_no_scale_bit};
pub use player_anim::{
    ANIM_BODY_PART_NAMES, ANIM_COND_AKIMBO, ANIM_COND_CROUCHING, ANIM_COND_DAMAGETYPE,
    ANIM_COND_FIRING, ANIM_COND_HITDIRECTION, ANIM_COND_HITLOCATION, ANIM_COND_IS_BITFLAGS,
    ANIM_COND_MOUNTED, ANIM_COND_MOVETYPE, ANIM_COND_NAMES, ANIM_COND_PERK,
    ANIM_COND_PLAYERANIMTYPE, ANIM_COND_PLAYERANIMTYPEPRIMARY, ANIM_COND_STRAFING,
    ANIM_COND_WEAPON_POSITION, ANIM_COND_WEAPONCLASS, ANIM_DAMAGE_EXPLOSION_NEAR_DIST_SQ,
    ANIM_DAMAGETYPE_NAMES, ANIM_ET_DEATH, ANIM_ET_FIREWEAPON, ANIM_ET_NAMES, ANIM_ET_RELOAD,
    ANIM_HITDIR_FRONT_BACK_DOT_SQ, ANIM_HITDIRECTION_NAMES, ANIM_HITLOCATION_NAMES, ANIM_MT_IDLE,
    ANIM_MT_IDLECR, ANIM_MT_IDLELASTSTAND, ANIM_MT_IDLEPRONE, ANIM_MT_NAMES, ANIM_PARSE_MODES,
    ANIM_PERK_NAMES, ANIM_PLAYERANIMTYPE_NAMES, ANIM_STATE_NAMES, ANIM_STRAFING_NAMES,
    ANIM_WEAPON_POSITION_NAMES, ANIM_WEAPONCLASS_NAMES, ANIMFLAG_ADDITIVE, ANIMFLAG_COMPLETE,
    ANIMFLAG_LOOPSYNC, ANIMFLAG_NONLOOPSYNC, ANIMTREE_PROPERTY_NAMES, PLAYER_ANIM_INDEX_MASK,
    PLAYER_ANIM_RAW_MASK, PLAYER_ANIM_RESTART_TOGGLE, PlayerAnimValue, anim_cond_evaluable,
    anim_cond_null_value_defaults_to_one, anim_cond_value_names, anim_script_hit_direction,
    anim_script_hit_location, anim_weapon_position_from_pm_flags, bg_random,
};
pub use quat::{
    QUAT_IDENTITY, Quat, VEC3_ZERO, Vec3, normalize, quat_add_weighted, quat_dot, quat_mul,
    quat_neg, vec3_add_scaled, xanim_apply_additive,
};
pub use sample::{FrameKind, sample_quat, sample_vec3, span, time_to_frame};
pub use skel::{DuplicatePart, Skel};
pub use xanim::XAnimParts;
pub use xanim_time::{
    XANIM_NONLOOP_END_PARK, XANIM_WEIGHT_FLOOR, XANIM_WEIGHT_FLOOR_SCALE,
    xanim_advance_goal_weight, xanim_advance_leaf_time,
};
pub use xbone::XBoneInfo;
