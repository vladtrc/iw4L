mod destructible;
mod dobj;
mod dobj_runtime;
mod retained;
mod semantic;
mod xanim_clip;
mod xanim_tree;

pub use anim_iw4::{Local, PartBits};
pub use dobj::{
    AIM_PITCH_CLAMP_RAD, AnimInstance, Attach, DObj, DObjBoneOrientation, DObjBoneOrientationError,
    DObjError, HidePartBits, ModelPoseSrc, PLAYER_CONTROLLER_TAGS, PlayerControllerInput,
    PlayerControllerResult, TP_HEAD_ATTACH_TAG, TP_WEAPON_ATTACH_TAGS, apply_aim_pitches,
    apply_legs_yaw, apply_player_controller, apply_standing_player_controller,
    build_body_head_weapon_dobj, build_body_weapon_dobj, dobj_bone_orientation, dobj_set_angles,
    dobj_set_control_tag_angles, dobj_set_local_tag, pitch_spine_bone,
    player_controller_tag_origin, tp_head_attach_tag, tp_weapon_attach_tag, yaw_bone,
};
pub use dobj_runtime::{DObjAnimRuntime, DObjReuseKey, dobj_model_token, dobj_reuse_matches};
pub use retained::{
    BoneCollision, CollSurfCollision, CollTri, CollisionBone, DObjPoseRequest, MaterializeError,
    RetainedModelCapability, collision_dobj_with_controller, collision_models,
    collision_models_with_controller, pose_dobj, pose_dobj_with_controller,
};
pub use semantic::{
    DObjCompositionDescriptor, DObjModelDescriptor, DObjSemanticState, SemanticResolveError,
    XAnimSemanticNode, XAnimSemanticNodeKind, XAnimTreeSnapshot,
};
pub use xanim_clip::{
    AnimClip, ClipError, ClipNotify, FrameIndices, Keyed, RawDeltaTrans, RawXAnimParts, Rotation,
    SampledTrack, Track, Translation,
};
pub use xanim_tree::{
    XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind, XAnimNodeState, XAnimTreeDefinition,
    XAnimTreeError, XAnimTreeRuntime,
};

pub use destructible::{T5DestructibleDef, T5DestructiblePiece, T5DestructibleStage};
