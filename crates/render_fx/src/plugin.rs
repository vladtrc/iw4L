use bevy::prelude::*;

pub struct RenderFxPlugin;

impl Plugin for RenderFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::FxCodeMeshPlan>()
            .init_resource::<crate::FxParticleCloudPlan>()
            .init_resource::<crate::GfxMarkMeshPlan>()
            .init_resource::<crate::FxJournalCursor>()
            .init_resource::<crate::CombatFxDump>()
            .init_resource::<crate::PreparedFxCatalog>()
            .init_resource::<crate::PreparedFxModels>()
            .init_resource::<crate::PreparedFxElemInfos>()
            .init_resource::<crate::FxCameraOrigin>()
            .init_resource::<crate::PreparedImpactFx>()
            .init_resource::<crate::HostFxSystem>()
            .init_resource::<crate::FxMarkDvars>()
            .init_resource::<crate::LaserDvars>()
            .init_resource::<crate::HostFxDlights>()
            .init_resource::<crate::HostFxPostLights>()
            .init_resource::<crate::FxWorldColorImages>()
            .init_resource::<crate::FxDumpRequest>()
            .init_resource::<crate::PresentedVehicleFx>()
            .init_resource::<crate::PreparedTracers>()
            .init_resource::<crate::TracerDrawGate>()
            .init_resource::<crate::TracerWorld>()
            .init_resource::<crate::EntityMarks>()
            .init_resource::<crate::FxModelDrawPlan>();
        crate::system::register_fx_orchestration(app);
    }
}
