#![no_std]
#![forbid(unsafe_code)]

mod adjust_mover;
mod centity;
mod client_state;
pub mod corpse_info;
mod entity_state;
mod events;
mod glass;
mod link;
mod missile_land;
mod origin;
mod player_angles;
mod script_mover;
mod trajectory;

pub use adjust_mover::{
    ET_GENERAL, ET_ITEM, ET_MISSILE, ET_PLANE, ET_PLAYER, ET_PLAYER_CORPSE, ET_PRIMARY_LIGHT,
    ET_SCRIPTMOVER, adjust_position_for_mover, adjust_position_for_mover_evaluates,
    mover_num_in_adjust_range,
};
pub use centity::Centity;
pub use client_state::{
    CLIENT_STATE_NAME_LEN, ClientState, TEAM_ALLIES, TEAM_AXIS, TEAM_FREE, TEAM_SPECTATOR,
    client_state_name, client_state_name_bytes, client_state_team_from_sessionteam, get_team_name,
    pack_client_state_name,
};
pub use corpse_info::CorpseInfoPlayerAnimCopy;
pub use entity_state::EntityState;
pub use events::{
    ET_EVENTS, EVENT_RING_LEN, EVENT_SEQUENCE_MASK, EVENT_SEQUENCE_WRAP_WINDOW, EntityEventAction,
    EntityEventFact, EntityEventKind, LOCAL_SOUND_ENTITY, SequencedEntityEvent,
    UnsupportedEntityEvent, add_entity_event, bullet_hit_event, consume_entity_events,
    entity_event_action, is_left_hand_fire_event, is_weapon_fire_last_shot_event,
    packet_entity_uses_event_ring, predicted_weapon_fire_event,
};
pub use glass::{
    CG_GLASS_PIECE_LIMIT, ClientGlassPiece, GLASS_BLAST_DAMAGE_SCALE, GLASS_BLAST_RADIUS_CAP,
    GLASS_COLLAPSE_LONG_BASE_MS, GLASS_COLLAPSE_LONG_RANGE_MS, GLASS_COLLAPSE_LONG_THRESHOLD,
    GLASS_COLLAPSE_SHORT_BASE_MS, GLASS_COLLAPSE_SHORT_RANGE_MS, GLASS_COLLAPSE_SHORT_THRESHOLD,
    GLASS_DAMAGE_ADD_CAP, GLASS_DAMAGE_INVALID, GLASS_DAMAGE_TO_DESTROY, GLASS_DAMAGE_TO_WEAKEN,
    GLASS_DECODE_SHATTER_SCALE, GLASS_ENCODE_SHATTER_BIAS, GLASS_ENCODE_SHATTER_SCALE,
    GLASS_FRACTURE_PROFILE_VERSION, GLASS_IMPACT_DIR_NONE, GLASS_MELEE_DAMAGE,
    GLASS_PROJECTILE_PANE_HOPS, GlassApplyAction, GlassBreakRecord, GlassCause, GlassPaneBasis,
    GlassPieceState, GlassShatterSeed, GlassStateChange, MISSILE_GLASS_SHATTER_VEL,
    ServerGlassPiece, glass_add_damage, glass_apply_damage, glass_apply_state,
    glass_blast_cone_keeps, glass_blast_integer_damage, glass_collapse_due, glass_collapse_piece,
    glass_decode_shatter_coord, glass_encode_shatter_coord, glass_is_solid,
    glass_is_solid_threshold, glass_packed_dir_to_vec, glass_read_change,
    glass_shatter_impact_from_seed, glass_shatter_seed_from_hit, glass_should_notify_destroyed,
    glass_state_from_damage, glass_state_from_damage_thresholds, glass_update,
    glass_vec_to_packed_dir, glass_weakened_collapse_time_cs,
};
pub use link::{LinkBounds, link_entity_needs_rotated_radius, link_entity_world_bounds};
pub use missile_land::{
    GRENADE_APOS_PITCH_OFS, GRENADE_BLADE_SPIN_PITCH, GRENADE_SPIN_PITCH_MAX,
    GRENADE_SPIN_PITCH_MIN, GRENADE_SPIN_ROLL_MAX, GRENADE_SPIN_ROLL_MIN, MISSILE_NODRAW_BASE_MS,
    MISSILE_NODRAW_MAX_MS, MISSILE_NODRAW_MIN_MS, MISSILE_NODRAW_SPEED_DIV,
    MISSILE_NODRAW_SPEED_SCALE, MissileLandAnglesIn, MissileLandAnglesOut, fire_grenade_no_draw_ms,
    fire_missile_apos, init_grenade_apos, init_grenade_pos, missile_land_angles, missile_nodraw,
};
pub use origin::{
    DObjAnimMat, PARENT_LINK_AXIS_IDENTITY, parent_link_apply_local, parent_link_pose,
    parent_link_world_from_tag, set_angle, set_origin,
};
pub use player_angles::{
    BG_LEG_YAW_TOLERANCE, BG_SWING_SPEED, LEGS_YAW_CLAMP, PLAYER_MOVE_FACTOR_ON_TORSO,
    PlayerAngleDvars, PlayerAngleInput, PlayerAngleOutput, SwingState, TORSO_YAW_CLAMP,
    legs_offset_deg, player_angles, swing_angles,
};
pub use script_mover::{
    CG_SCRIPT_MOVER_NODRAW, SCRIPT_MOVER_BMODEL_SOLID, SP_HERO_ONLY_LIGHTING,
    SP_SCRIPT_MODEL_AIM_ASSIST, script_mover_add_bmodel,
};
pub use trajectory::{
    TR_GRAVITY, TR_INTERPOLATE, TR_LINEAR, TR_LINEAR_STOP, TR_STATIONARY, Trajectory,
    evaluate_trajectory, evaluate_trajectory_delta, truncated_tr_delta,
};
