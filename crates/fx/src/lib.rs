mod def;
mod draw;
mod elem;
mod glass;
mod lifetime;
mod marks;
mod motion;
mod play;
mod sort;
mod spark;
mod spark_fountain;
mod spawn;
mod system;
mod trail;
mod update;

pub use def::{FxEffectDefInfo, FxElemDefInfo};
pub use draw::{
    FxCloudInstance, FxDrawElemContext, FxDrawResolver, FxDrawTrailContext,
    FxDrawTrailSampleContext, FxElemLightInstance, FxFountainInstance, FxGenerateVertsOut,
    FxModelInstance, FxSparkDrawQuery, FxSpriteInstance, FxTrailDrawDef, FxTrailMeshInstance,
    FxTrailSampleVisual, generate_verts, generate_verts_with_trails,
};
pub use elem::FxElemSlot;
pub use glass::{
    FxGlassInitTables, FxGlassSystemHost, GlassBreakPresentation, GlassImpact,
    GlassPresentationEvent, GlassWorldContact, GlassWorldTrace,
};
pub use lifetime::{
    FxMsec, LE_MOVING_TRACER, LE_TR_LINEAR, LOCAL_ENTITY_POOL_CAPACITY, LOCAL_ENTITY_SIZE,
    LocalEntityPool, LocalEntitySlot, local_entity_is_live, set_presentation_clock,
    tracer_travel_msec,
};
pub use marks::{
    FxMarksSystemHost, GfxMarkMesh, GfxMarkMeshSurf, MarkImpactRequest, MarkImpactResult,
    MarkReceiverEnable, MarkTraceRecord,
};
pub use motion::{FxElemCollision, FxElemMotion, evaluate_elem_physics_motion};
pub use play::{
    FxPlayPose, FxPlayRequest, PlayResult, axis_from_hit_normal, axis_from_impact_velocity,
    play_at_origin, play_bolted, play_oriented, spawn_impact_or_death_effect, spawn_oriented,
};
pub use spark::{FxSparkCloudInstance, FxSparkFillVisual};
pub use spawn::{sort_effect_elems, spawn_looping_partial};
pub use system::{
    FX_CATALOG_INDEX_NONE, FxBoltOrientation, FxBoltTarget, FxEffectSlot, FxPackedLightingSrc,
    FxResolvedBoltPose, FxSpawnProducts, FxSystemHost, PendingDecalSpawn, PendingRunnerSpawn,
    PendingSoundSpawn, PendingTrailImpact, SpawnFail,
};
pub use trail::{
    FxTrailCollideHit, FxTrailElemSlot, FxTrailSlot, FxTrailUpdateHooks, alloc_trail,
    alloc_trail_elem, update_effect_trails, update_trail,
};
pub use update::{
    FxChildKind, FxChildSpawnRequest, FxElemMotionQuery, FxElemMotionResult, FxElemTraceHit,
    FxEmitQuery, FxImpactSpawn, FxSparkFillQuery, FxUpdateHooks, PendingCollide, update,
};
