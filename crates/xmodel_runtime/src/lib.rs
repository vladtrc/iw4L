mod anim_state;
mod destructible;
mod dobj;
mod dobj_runtime;
mod player_body;
mod retained;
mod semantic;
mod xanim_clip;
mod xanim_tree;

pub use dobj::{
    AIM_PITCH_CLAMP_RAD, AnimInstance, Attach, DObj, DObjBoneOrientation, DObjBoneOrientationError,
    DObjError, HidePartBits, ModelPoseSrc, PLAYER_CONTROLLER_TAGS, PlayerControllerInput,
    PlayerControllerResult, TP_HEAD_ATTACH_TAG, TP_WEAPON_ATTACH_TAGS, apply_aim_pitches,
    apply_legs_yaw, apply_player_controller, apply_standing_player_controller, bone_orientation,
    build_body_head_weapon_dobj, build_body_weapon_dobj, pitch_spine_bone,
    player_controller_tag_origin, set_angles, set_control_tag_angles, set_local_tag,
    tp_head_attach_tag, tp_weapon_attach_tag, yaw_bone,
};
pub use dobj_runtime::{DObjAnimRuntime, DObjReuseKey, model_token, reuse_matches};
pub use retained::{
    BoneCollision, CollSurfCollision, CollTri, CollisionBone, DObjPoseRequest, MaterializeError,
    ModelMovementBrush, RetainedModelCapability, collision_bone_from_local_box,
    collision_dobj_with_controller, collision_models, collision_models_with_controller, pose_dobj,
    pose_dobj_with_controller,
};
pub use semantic::{
    DObjCompositionDescriptor, DObjModelDescriptor, DObjSemanticState, SemanticResolveError,
    XAnimSemanticNode, XAnimSemanticNodeKind, XAnimTreeSnapshot,
};
pub use xanim_clip::{
    AnimClip, ClipError, ClipNotify, FrameIndices, Keyed, RawDeltaQuat, RawDeltaTrans,
    RawXAnimParts, Rotation, SampledTrack, Track, Translation,
};
pub use xanim_tree::{
    XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind, XAnimNodeState, XAnimTreeDefinition,
    XAnimTreeError, XAnimTreeRuntime,
};

pub use anim_state::{AnimStateClip, AnimStateTable};
pub use destructible::{T5DestructibleDef, T5DestructiblePiece, T5DestructibleStage};

pub use player_body::{
    ClientAnimSample, PlayerAnimProperties, PlayerBodyBranches, apply_player_anim_goals,
    apply_player_anim_rates, overlay_legs_clip,
};
