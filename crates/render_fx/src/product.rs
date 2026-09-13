use bevy::prelude::*;
use fx::{FxGenerateVertsOut, GfxMarkMeshCensus};
use fx_iw4::FxPostLight;

use crate::present::FxVertsGaps;
use crate::tracer::TracerWorld;

#[derive(Debug)]
pub enum FxUnavailableCause {
    CatalogAbsent,
}

pub struct FxGeneratedFrame {
    pub out: FxGenerateVertsOut,
    pub verts_gaps: FxVertsGaps,
    pub mark_mesh: Option<GfxMarkMeshCensus>,
    pub post_lights: Vec<FxPostLight>,
    pub tracers: TracerWorld,
    pub cam_tf: Transform,
    pub frustum_planes: Vec<[f32; 4]>,
    pub clip_from_world: Option<[f32; 16]>,
    pub tan_half_fov: Option<(f32, f32)>,
    pub world_present: bool,
}

pub enum FxFrameOutcome {
    Generated(FxGeneratedFrame),
    Unavailable(FxUnavailableCause),
}

pub fn clear_fx_owned_plans(
    plan: &mut crate::FxCodeMeshPlan,
    spark: &mut crate::FxParticleCloudPlan,
    marks: &mut crate::GfxMarkMeshPlan,
) {
    plan.clear();
    spark.clear_draws();
    marks.clear();
}

pub fn publish_empty_fx_owned_plans(
    plan: &mut crate::FxCodeMeshPlan,
    spark: &mut crate::FxParticleCloudPlan,
) {
    plan.bump();
    plan.publish_share();
    spark.bump();
    spark.publish_share();
}
