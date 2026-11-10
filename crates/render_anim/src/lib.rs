pub mod anim;
pub mod draw;
mod draw_build;
pub mod geometry;
pub mod lighting;
pub mod occupancy;
mod plugin;

pub use anim::dobj_pose::{
    HostDObjPoseFrame, PosedPlayer, PosedPlayerFrame, PosedPlayerHead, ScriptModelBoltDemand,
    ScriptModelDObjFrame,
};
pub use anim::fpv_host::{
    FpvBoltTargets, FpvHeldLife, FpvHeldSettled, FpvPoseGenerator, FpvPoseKind, FpvPoseProduct,
    FpvPosedFrame, FpvPresentCursor, PendingFpvNotetracks, PendingFpvSpawn, PendingFpvSpawnRequest,
};
pub use anim::fpv_prepared::{
    FpvRigSet, FpvWeaponSlot, FpvWeaponTable, FpvWeaponView, PreparedFpv, prepare_fpv_compositions,
};
pub use anim::model_materials::{PreparedModelMaterials, prepare_model_materials};
pub use anim::viewmodel_controller::{
    AdvanceResult, EventResult, ViewmodelController, ViewmodelEvent, WeaponState,
};
pub use anim::*;
pub use draw::*;
pub use draw_build::*;
pub use lighting::{
    fpv_dobj_lighting_box_half, fpv_dobj_skel_radii, lighting_box_half,
    script_model_lighting_box_half, viewmodel_lighting_origin,
};
pub use occupancy::{
    DynEntCellBits, DynEntPhysClip, DynEntPhysWorld, FpvGeometrySet, FpvPlacementRoot,
    FpvPlacementSet, GunOffset, PendingViewHurt, RemoteFxBolts, RemotePlayer, RenderFocus,
    ScriptModelDrawSet, ScriptModelSkinSet, SessionViewmodel, occupy_fpv_scene, spawn_pending_fpv,
    stamp_fpv_placement_matrix, sync_camera_from_presented, tick_fpv_viewmodel,
};
pub use plugin::RenderAnimPlugin;
