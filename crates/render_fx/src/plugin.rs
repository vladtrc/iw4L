use bevy::prelude::*;
use frame::ScopeApp;

pub struct RenderFxPlugin;

impl Plugin for RenderFxPlugin {
    fn build(&self, app: &mut App) {
        app.scoped::<crate::FxCodeMeshPlan>(frame::MatchScope::Live)
            .scoped::<crate::FxParticleCloudPlan>(frame::MatchScope::Live)
            .scoped::<crate::GfxMarkMeshPlan>(frame::MatchScope::Live)
            .scoped::<crate::FxJournalCursor>(frame::Timeline::Live)
            .scoped::<crate::CombatFxDump>(frame::MatchScope::Live)
            .scoped::<crate::PreparedFxElemInfos>(frame::MatchScope::Live)
            .scoped::<crate::FxCameraOrigin>(frame::MatchScope::Live)
            .scoped::<crate::HostFxSystem>(frame::Timeline::Live)
            .init_resource::<crate::FxMarkDvars>()
            .init_resource::<crate::LaserDvars>()
            .scoped::<crate::HostFxDlights>(frame::Timeline::Live)
            .scoped::<crate::HostFxPostLights>(frame::Timeline::Live)
            .scoped::<crate::FxWorldColorImages>(frame::MatchScope::Live)
            .scoped::<crate::FxDumpRequest>(frame::MatchScope::Live)
            .scoped::<crate::PresentedVehicleFx>(frame::MatchScope::Live)
            .scoped::<crate::TracerDrawGate>(frame::Timeline::Live)
            .scoped::<crate::TracerWorld>(frame::Timeline::Live)
            .scoped::<crate::EntityMarks>(frame::Timeline::Live)
            .scoped::<crate::FxModelDrawPlan>(frame::MatchScope::Live);
        crate::system::register_fx_orchestration(app);
    }
}
