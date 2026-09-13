extern crate self as render_frontend;

pub mod adapters;
pub mod assemble;
pub mod prepare;

pub use adapters::RenderAdaptersPlugin;
pub use assemble::RenderAssemblePlugin;
pub use prepare::RenderPreparePlugin;

pub use assemble::drawsurf::*;
pub use assemble::pack::{
    HOST_XMODEL_RIGID_TESS_INFO_PACKED_ARM, PackDraw, PackKind, pack_spot_shadow_frontend,
    pack_sun_shadow_frontend,
};
pub use prepare::gfx_scene::{
    AddBModelArgs, AddBModelPose, AddDObjArgs, AddDObjPose, GfxScene, GfxSceneBrush, GfxSceneDobj,
    GfxSceneModel, SCENE_BRUSH_CAP, SCENE_DOBJ_CAP, SCENE_GFX_ENT_CAP, SCENE_INDEX_EMPTY,
    SCENE_MODEL_CAP, SCENE_VIEWMODEL_ENTNUM, SCENE_VIEWMODEL_FX_FLAGS, SCENE_VIEWMODEL_LEFT_ENTNUM,
    SceneAddError, SceneAddKind, SceneEntSkinnedSurfs, pack_scene_info, scene_info_entnum,
};
pub use prepare::scene::world::*;
pub use prepare::sun::{add_bsp_sun_shadow_partition, pack_sun_shadow_caster_lists};
pub use prepare::worker_cmds::{
    AddWorkerCmd, BoundEntWorkerCmd, CellFrustumWorkerCmd, DpvsEntWorkerCmd,
    SkinCachedStaticModelCmd, SpotShadowEntWorkerCmd, WORKER_CMD_BOUND_ENT, WORKER_CMD_BUSY,
    WORKER_CMD_CELL_DYN_BRUSH, WORKER_CMD_CELL_DYN_MODEL, WORKER_CMD_CELL_SCENE_ENT,
    WORKER_CMD_COUNT, WORKER_CMD_DPVS_ENT, WORKER_CMD_SMODELCACHE, WORKER_CMD_SPOT_SHADOW_ENT,
    WorkerCmdBusy, WorkerCmdBusyInput, WorkerCmdError, WorkerCmdQueues, enqueue_cell_frustum_cmds,
    worker_cmd_dispatch_skips_device_lost,
};
