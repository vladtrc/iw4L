#![no_std]
#![forbid(unsafe_code)]

mod adjust_mover;
mod centity;
mod client_state;
mod corpse_info;
mod entity_state;
mod events;
mod g_corpse_info;
mod g_link;
mod g_origin;
mod glass;
mod missile_land;
mod player_angles;
mod script_mover;
mod trajectory;

pub use adjust_mover::{
    ET_GENERAL, ET_ITEM, ET_MISSILE, ET_PLANE, ET_PLAYER, ET_PLAYER_CORPSE, ET_PRIMARY_LIGHT,
    ET_SCRIPTMOVER, adjust_position_for_mover_evaluates, cg_adjust_position_for_mover,
    mover_num_in_adjust_range,
};
pub use centity::Centity;
pub use client_state::{
    CLIENT_STATE_NAME_LEN, ClientState, TEAM_ALLIES, TEAM_AXIS, TEAM_FREE, TEAM_SPECTATOR,
    cg_get_team_name, client_state_name, client_state_name_bytes,
    client_state_team_from_sessionteam, pack_client_state_name,
};
pub use corpse_info::{CORPSE_INFO_LIVE_INFO_AT, CORPSE_INFO_TREE_REWRITE_AT, CorpseInfo};
pub use entity_state::EntityState;
pub use events::{
    ET_EVENTS, EVENT_RING_LEN, EVENT_SEQUENCE_MASK, EVENT_SEQUENCE_WRAP_WINDOW, EntityEventAction,
    EntityEventFact, EntityEventKind, LOCAL_SOUND_ENTITY, SequencedEntityEvent,
    UnsupportedEntityEvent, add_entity_event, bg_bullet_hit_event, bg_is_left_hand_fire_event,
    bg_is_weapon_fire_last_shot_event, cg_entity_event_action, cg_packet_entity_uses_event_ring,
    cg_predicted_weapon_fire_event, consume_entity_events,
};
pub use g_corpse_info::{
    CORPSE_INFO_LEGS_ANIM_AT, CORPSE_INFO_TORSO_ANIM_AT, CORPSE_INFO_TORSO_PITCH_AT,
    CORPSE_INFO_WAIST_PITCH_AT, CORPSE_INFO_WALK_DWORDS, CorpseInfoPlayerAnimCopy,
    g_corpse_info_copy_player_anims, g_corpse_info_entnum_matched, g_corpse_info_slot_for_entnum,
};
pub use g_link::{SvLinkBounds, sv_link_entity_needs_rotated_radius, sv_link_entity_world_bounds};
pub use g_origin::{
    DObjAnimMat, PARENT_LINK_AXIS_IDENTITY, g_dobj_anim_mat_axis, g_parent_link_apply_local,
    g_parent_link_pose, g_parent_link_world_from_tag, g_set_angle, g_set_origin,
};
pub use glass::{
    CG_GLASS_PIECE_LIMIT, CgGlassApplyAction, CgGlassPiece, GGlassPiece, GLASS_BLAST_DAMAGE_SCALE,
    GLASS_BLAST_RADIUS_CAP, GLASS_COLLAPSE_LONG_BASE_MS, GLASS_COLLAPSE_LONG_RANGE_MS,
    GLASS_COLLAPSE_LONG_THRESHOLD, GLASS_COLLAPSE_SHORT_BASE_MS, GLASS_COLLAPSE_SHORT_RANGE_MS,
    GLASS_COLLAPSE_SHORT_THRESHOLD, GLASS_DAMAGE_ADD_CAP, GLASS_DAMAGE_INVALID,
    GLASS_DAMAGE_TO_DESTROY, GLASS_DAMAGE_TO_WEAKEN, GLASS_DECODE_SHATTER_SCALE,
    GLASS_ENCODE_SHATTER_BIAS, GLASS_ENCODE_SHATTER_SCALE, GLASS_FRACTURE_PROFILE_VERSION,
    GLASS_IMPACT_DIR_NONE, GLASS_MELEE_DAMAGE, GLASS_PROJECTILE_PANE_HOPS, GlassBreakRecord,
    GlassCause, GlassPaneBasis, GlassPieceState, GlassShatterSeed, GlassStateChange,
    MISSILE_GLASS_SHATTER_VEL, cg_glass_apply_state, cg_glass_is_solid, cg_glass_read_change,
    cg_glass_update, glass_add_damage, glass_apply_damage, glass_blast_cone_keeps,
    glass_blast_integer_damage, glass_collapse_due, glass_collapse_piece,
    glass_decode_shatter_coord, glass_encode_shatter_coord, glass_is_solid,
    glass_is_solid_threshold, glass_packed_dir_to_vec, glass_shatter_impact_from_seed,
    glass_shatter_seed_from_hit, glass_should_notify_destroyed, glass_state_from_damage,
    glass_state_from_damage_thresholds, glass_vec_to_packed_dir, glass_weakened_collapse_time_cs,
};
pub use missile_land::{
    GRENADE_APOS_PITCH_OFS, GRENADE_BLADE_SPIN_PITCH, GRENADE_SPIN_PITCH_MAX,
    GRENADE_SPIN_PITCH_MIN, GRENADE_SPIN_ROLL_MAX, GRENADE_SPIN_ROLL_MIN, MISSILE_NODRAW_BASE_MS,
    MISSILE_NODRAW_MAX_MS, MISSILE_NODRAW_MIN_MS, MISSILE_NODRAW_SPEED_DIV,
    MISSILE_NODRAW_SPEED_SCALE, MissileLandAnglesIn, MissileLandAnglesOut, cg_missile_nodraw,
    g_fire_grenade_no_draw_ms, g_fire_missile_apos, g_init_grenade_apos, g_init_grenade_pos,
    missile_land_angles,
};
pub use player_angles::{
    BG_LEG_YAW_TOLERANCE, BG_SWING_SPEED, LEGS_YAW_CLAMP, PLAYER_MOVE_FACTOR_ON_TORSO,
    PlayerAngleDvars, PlayerAngleInput, PlayerAngleOutput, SwingState, TORSO_YAW_CLAMP,
    cg_player_angles, cg_swing_angles, legs_offset_deg,
};
pub use script_mover::{
    CG_SCRIPT_MOVER_NODRAW, SCRIPT_MOVER_BMODEL_SOLID, cg_script_mover_add_bmodel,
};
pub use trajectory::{
    TR_GRAVITY, TR_INTERPOLATE, TR_LINEAR, TR_LINEAR_STOP, TR_STATIONARY, Trajectory,
    bg_evaluate_trajectory, bg_evaluate_trajectory_delta, truncated_tr_delta,
};
