use bevy::prelude::*;

use crate::{
    DynAtPointLookup, HostGfxScene, ModelLightingRequests, RLockPvs, RSubwindowDvar,
    RZnearDepthhackDvar, RZnearDvar, ResolvedModelLightingTable, SceneEntSkinInputs,
    SceneEntSurfaceCache, SimCamera, SmEnableDvar, SmSunEnableDvar, SpotShadowEntityOriginTrack,
    SpotShadowSceneOccupancy,
};

pub struct RenderScenePlugin;

impl Plugin for RenderScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HostGfxScene>()
            .init_resource::<DynAtPointLookup>()
            .init_resource::<crate::WorldDpvsCells>()
            .init_resource::<crate::PublishedCellVis>()
            .init_resource::<crate::TessMaterials>()
            .init_resource::<crate::WorldPresentFacts>()
            .init_resource::<SpotShadowSceneOccupancy>()
            .init_resource::<SpotShadowEntityOriginTrack>()
            .init_resource::<SceneEntSkinInputs>()
            .init_resource::<SceneEntSurfaceCache>()
            .init_resource::<ModelLightingRequests>()
            .init_resource::<ResolvedModelLightingTable>()
            .init_resource::<SimCamera>()
            .init_resource::<crate::PreparedSceneView>()
            .init_resource::<crate::LodRampSkinnedDvar>()
            .init_resource::<RZnearDvar>()
            .init_resource::<RZnearDepthhackDvar>()
            .init_resource::<RSubwindowDvar>()
            .init_resource::<RLockPvs>()
            .init_resource::<SmEnableDvar>()
            .init_resource::<SmSunEnableDvar>();
    }
}
