use bevy::prelude::*;
use frame::ScopeApp;

use crate::{
    DynAtPointLookup, HostGfxScene, ModelLightingRequests, RLockPvs, RSubwindowDvar,
    RZnearDepthhackDvar, RZnearDvar, ResolvedModelLightingTable, SceneEntSkinInputs,
    SceneEntSurfaceCache, SimCamera, SmEnableDvar, SmSunEnableDvar, SpotShadowEntityOriginTrack,
    SpotShadowSceneOccupancy,
};

pub struct RenderScenePlugin;

impl Plugin for RenderScenePlugin {
    fn build(&self, app: &mut App) {
        app.scoped::<HostGfxScene>(frame::MatchScope::Live)
            .scoped::<crate::PreparedSunShadowFrame>(frame::MatchScope::Live)
            .scoped::<crate::ModelLightingAtlasTileWrites>(frame::MatchScope::Live)
            .scoped::<DynAtPointLookup>(frame::MatchScope::Live)
            .scoped::<crate::WorldDpvsCells>(frame::MatchScope::Live)
            .scoped::<crate::PublishedCellVis>(frame::MatchScope::Live)
            .scoped::<crate::TessMaterials>(frame::MatchScope::Live)
            .scoped::<crate::WorldPresentFacts>(frame::MatchScope::Live)
            .scoped::<SpotShadowSceneOccupancy>(frame::MatchScope::Live)
            .scoped::<SpotShadowEntityOriginTrack>(frame::MatchScope::Live)
            .scoped::<SceneEntSkinInputs>(frame::MatchScope::Live)
            .scoped::<SceneEntSurfaceCache>(frame::MatchScope::Live)
            .scoped::<ModelLightingRequests>(frame::MatchScope::Live)
            .scoped::<ResolvedModelLightingTable>(frame::MatchScope::Live)
            .init_resource::<SimCamera>()
            .init_resource::<crate::PreparedSceneView>()
            .init_resource::<crate::LodRampSkinnedDvar>()
            .init_resource::<RZnearDvar>()
            .init_resource::<RZnearDepthhackDvar>()
            .init_resource::<RSubwindowDvar>()
            .init_resource::<RLockPvs>()
            .init_resource::<crate::PortalRenderControls>()
            .init_resource::<SmEnableDvar>()
            .init_resource::<SmSunEnableDvar>();
    }
}
