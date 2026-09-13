pub mod adopt;
pub mod anim_script_gap;
pub mod barrel_policy;
pub mod bullet;
pub mod bullet_collision;
mod carrier;
pub mod combat;
pub mod content;
mod corpse;
mod damage;
mod entity_run;
mod equipment;
mod frame;
mod gentity;
pub mod hudelem;
pub mod identities;
pub mod input;
mod item;
mod mantle_xanim;
pub mod match_state;
mod missile;
pub mod player_anim_script;
pub mod rules;
mod score;
mod smodel_grid;
mod snapshot;
mod sound_alias_cs;
pub mod spawn;
mod step;
pub mod t5_destructible;
mod use_object;
mod vehicle_glass;
mod voice;
mod world;
pub mod world_objects;

pub use adopt::{ADOPT_GAP_COUNT, ADOPT_GAPS, AdoptGap, AdoptReport};
pub use anim_script_gap::PlayerAnimScriptGap;
pub use barrel_policy::{
    EXPLODABLE_BARREL_BURN_DRAIN, EXPLODABLE_BARREL_BURN_DRAIN_INTERVAL_MS,
    EXPLODABLE_BARREL_BURN_LOOP_FX, EXPLODABLE_BARREL_BURN_LOOP_INTERVAL_MS,
    EXPLODABLE_BARREL_BURN_START_FX, EXPLODABLE_BARREL_DEATH_FX, EXPLODABLE_BARREL_DEATH_SOUND,
    EXPLODABLE_BARREL_DESTROYED_STATE, EXPLODABLE_BARREL_EXPLODE_DAMAGE,
    EXPLODABLE_BARREL_EXPLODE_RANGE, EXPLODABLE_BARREL_HEALTH, EXPLODABLE_BARREL_HUSK,
};
pub use bullet_collision::{
    AuthorityDObjCollision, AuthorityDObjCollisionBone, AuthorityDObjState, AuthorityModelOwner,
    BulletHitKind, BulletPath, BulletTraceQuery, BulletTraceSegment, COLLISION_COVERAGE,
    COLLISION_HISTORY_TICKS, CONTENTS_SOLID, ColliderId, CollisionCoverageRow, CollisionHistory,
    CoverageSupport, CurrentAuthorityReason, EntityCollisionCapabilities, EntityCollisionEpoch,
    EntityCollisionFrame, EntityCollisionHistory, EntityCollisionSample, EntityCollisionTraceGeom,
    HistoryClampReason, HistoryFrame, HistoryPhase, HistoryRefusalReason, HistorySample,
    HistorySampleVerdict, HitVolumeKind, LAGCOMP_MAX_REWIND_TICKS, LagcompQuery,
    LinkedBrushCollisionBrush, MASK_BULLET_WORLD, MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS,
    PLAYER_MINS, PlayerCollisionPose, ScriptModelPlayAnim, ShotSampleProvenance, ShotSampleQuality,
    TraceInvalidReason, TraceOutcome, bullet_trace, bullet_trace_segments,
    bullet_trace_segments_with_entity_models, bullet_trace_with_entity_models,
    dobj_contents_match_mask, lagcomp_rewind_ticks,
};
pub use carrier::{SimWorld, StepReason, step};
pub use clipmap_iw4::{ClipCmodel, ClipLeaf, ClipNode};
pub use combat::{
    AcceptedShot, Emission, PlayerCollisionRepresentation, ShotCollisionGeometry,
    ShotCollisionVerdict, TracePhaseOutput, spread_pellet_direction,
};
pub use content::{
    CONTENT_DIGEST_SCHEME, ContentComponents, content_components_v2, content_digest_v0,
    content_digest_v1, content_digest_v2,
};
pub use corpse::{PlayerCorpsePool, PlayerCorpseSlot, level_time_ms};
pub use damage::{DamageAttempt, DamageOutcome, DamageRefusal, DeathCommit};
pub use equipment::{
    EquipmentRuntimeFacts, ProjectileHitGeometry, ProjectileImpact, ProjectileState,
    projectile_birth_ms,
};
pub use gamemode_iw4::{
    ANIMATED_MODEL_TARGETNAME, FAN_BLADE_AXIS_DOT, FAN_BLADE_FAST_SPEED_MAX,
    FAN_BLADE_FAST_SPEED_MIN, FAN_BLADE_ROTATE_FAST_TARGETNAME, FAN_BLADE_ROTATE_TARGETNAME,
    FAN_BLADE_ROTATE_TIME, FAN_BLADE_SLOW_SPEED_MAX, FAN_BLADE_SLOW_SPEED_MIN,
    FanBladeRotateChannel, TOY_CEILING_FAN_IDLE_MPANIM, TOY_CEILING_FAN_TYPE,
    TOY_WALL_FAN_IDLE_MPANIM, TOY_WALL_FAN_TYPE, animprop_machine, fan_blade_dots, fan_blade_right,
    fan_blade_rotate_channel, fan_blade_rotate_delta, fan_blade_speed_bounds,
    mp_clip_for_animated_model, mp_clip_for_toy_fan,
};
pub use gentity::{
    EntityAllocError, EntityKernel, EntityKernelOccupiedSnapshot, EntityKernelSlotSnapshot,
    EntityKernelSnapshot, EntityKernelSnapshotError, EntityRef, EntityRefError, EntityRelations,
    EntityRunKind, EntitySlotView, GENTITY_RESERVED_COUNT, GENTITY_REUSE_QUARANTINE_MS,
    GENTITY_TEMP_EVENT_LIFETIME_MS, KERNEL_PHASE_ORDER, KernelPhase, ScriptMoverGentity,
    ThinkEnter, UsePress, apos_from_entity_state, begin_script_mover_rotate_velocity,
    collect_use_presses, gentity_spawn_base, init_item_state, init_missile_state,
    init_script_mover_state, pos_from_entity_state, rotate_velocity_apos,
};
pub use hudelem::{
    GameHudElemSlot, HUDELEM_UPDATE_ARCHIVAL, HUDELEM_UPDATE_BOTH, HUDELEM_UPDATE_CURRENT,
    ensure_damage_feedback_slot, ensure_score_popup_slot, hud_elem_update_client,
    hud_level_time_ms, pulse_damage_feedback, pulse_score_popup, rebase_hud_archival,
    tick_score_popup_slots,
};
pub use identities::{
    ActionSequence, DamageSource, EventSequence, LifeSequence, MatchPhase, MatchRng, PelletId,
    ProjectileId, RNG_DOMAIN_SCHEME, RngDomain, ScriptModelId, ShotId,
};
pub use input::{ActionRequestId, ClassId, ClientAction, TickInput, action_request_id};
pub use mantle_xanim::MantleXAnimBind;
pub use match_state::{
    CLASS_CATALOG_BLING, CLASS_CATALOG_COLD_BLOODED, CLASS_CATALOG_DANGER_CLOSE,
    CLASS_CATALOG_LIGHTWEIGHT, CLASS_CATALOG_MARATHON, CLASS_CATALOG_NINJA,
    CLASS_CATALOG_SCAVENGER, CLASS_CATALOG_SCRAMBLER, CLASS_CATALOG_SLEIGHT_OF_HAND,
    CLASS_CATALOG_STEADY_AIM, CLASS_CATALOG_STOPPING_POWER, ClassDef, ClassRejectReason,
    ClientLifecycle, ClientSnapshotMeta, DroppedItemAmmo, EntityEventPayload, EntityEventRecord,
    EventAudience, EventRecord, GiveRejectReason, HealthRegenCensus, ItemPickupRecord, KillcamHud,
    LoadoutSpec, MatchEndReason, PelletFxRecord, RngDebugMeta, SIM_EVENT_ROSTER, SimEvent,
    SimEventRow, SnapshotMeta, UNRELIABLE_SIM_EVENT_COUNT, class_catalog_has,
    class_catalog_radar_jam_e_flags, perk_bits_from_class_catalog, sim_event_is_reliable,
};
pub use player_anim_script::{
    AnimConditions, AnimScriptCommand, AnimScriptCondition, AnimScriptItem, PlayerAnimScript,
    anim_conditions_from_pmove, pmove_anim_weapon_ids,
};
pub use rules::{FFA, FreeForAllRules};
pub use score::{MATCH_TICK_MS, bootstrap_score_defaults};
pub use snapshot::{
    AreaEntityLinkSnapshot, AreaEntityWorldSnapshot, AreaEntityWorldSnapshotError,
    AreaSectorSnapshot, Snapshot,
};
pub use sound_alias_cs::{
    CS_SOUNDALIASES_SLOTS, EffectNameCs, EffectNameCsOccupied, HudMaterialCs,
    HudMaterialCsOccupied, REQUIRED_HUD_MATERIALS, SoundAliasCs, SoundAliasCsOccupied,
    name_in_occupied,
};
pub use spawn::{
    AuthoredSpawnPoint, DomFlagDescriptor, HostGameModeSelection, MatchBootstrap, SPAWN_BAD_DIST,
    SPAWN_IDEAL_DIST, SpawnAttemptReport, SpawnDecision, SpawnReject, host_game_mode_kind,
    pick_ffa_spawn, spawn_candidate_indices, spawn_candidate_indices_for,
};
pub use step::{apply_explodable_barrel_death_presentation, phase_materialize_entity_dobjs};
pub use use_object::{
    DomFlagInstallError, MapUseBindError, UseCancelReason, UseHoldSession, UseObject,
    UseObjectEvent, UseObjectInstall, UseTriggerKind, bind_map_use_object,
    trigger_radius_world_aabb, world_aabb_from_r_box,
};
pub use weapon_iw4::{
    BulletPenFacts, CapturedCombatInput, FireType, MissingCombatFacts, PERK_FASTRELOAD,
    PenetrationDepthTable, WeaponCombatFacts,
};
pub use world::{
    ClientId, DamageFeedbackCue, HitvolDumpRow, PendingPlayerCardEvent, PendingPlayerCardKind,
    PlayerKitCollision, SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh, SimStaticModel, Tick,
    blank_player_state,
};
pub use world_objects::{
    DestructibleApplyReport, DestructibleDamageIntent, DestructibleExplodeEvent,
    DestructibleStateIndex, GLASS_DAMAGE_TO_DESTROY, GLASS_DAMAGE_TO_WEAKEN, GLASS_MELEE_DAMAGE,
    GlassPaneBasis, GlassPieceId, GlassPieceSnapshot, GlassPieceState, GlassShatterSeed,
    ToyDestructibleKind, VehicleDestructibleKind, VehicleDumpRow, VehicleFxPulse,
    VehicleSoundPulse, WorldObjectSnapshot, WorldObjectState,
};
pub use xmodel_runtime::{
    AnimClip, BoneCollision, DObjCompositionDescriptor, DObjModelDescriptor, DObjPoseRequest,
    DObjSemanticState, HidePartBits, ModelPoseSrc, PartBits, RetainedModelCapability, Rotation,
    Track, Translation, XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind, XAnimNodeState,
    XAnimSemanticNode, XAnimSemanticNodeKind, XAnimTreeDefinition, XAnimTreeSnapshot,
};

mod map_doors;
pub use map_doors::{DoorLeaf, DoorSwitch, MapDoors};

mod objectives;
pub use objectives::{BombSite, ObjectiveHull, ObjectiveMatch, ObjectiveView};

pub use world::{SimContent, SimContentBuilder};
