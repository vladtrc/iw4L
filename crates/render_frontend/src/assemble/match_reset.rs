use bevy::prelude::*;
use frame::{MatchTornDown, SessionSwapApplied};
use net::ClientSet;

use super::drawsurf::{
    CgGlassTable, DrawSurfList, FxCodeMeshPlan, FxModelDrawPlan, FxParticleCloudPlan,
    GfxGlassMeshPlan, GfxMarkMeshPlan,
};

pub(crate) fn reset_world_draw_plans_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    mut commands: Commands,
    mut dfog: ResMut<super::drawsurf::DrawMethodDfog>,
    mut draw_list: ResMut<DrawSurfList>,
    mut glass: ResMut<GfxGlassMeshPlan>,
    mut cg_glass: ResMut<CgGlassTable>,
    mut fx_code: ResMut<FxCodeMeshPlan>,
    mut spark: ResMut<FxParticleCloudPlan>,
    mut marks: ResMut<GfxMarkMeshPlan>,
    mut fx_models: ResMut<FxModelDrawPlan>,
) {
    if torn.read().count() == 0 {
        return;
    }
    commands.remove_resource::<super::drawsurf::MapFrameFog>();
    dfog.0 = false;
    *draw_list = DrawSurfList::default();
    *glass = GfxGlassMeshPlan::default();
    cg_glass.reset();
    *fx_code = FxCodeMeshPlan::default();
    *spark = FxParticleCloudPlan::default();
    *marks = GfxMarkMeshPlan::default();
    *fx_models = FxModelDrawPlan::default();
}

pub(crate) fn register_match_reset_systems(app: &mut App) {
    app.add_systems(
        Update,
        reset_world_draw_plans_on_teardown
            .after(SessionSwapApplied)
            .after(crate::prepare::scene::spawn::reset_world_spawn_on_teardown)
            .before(crate::prepare::scene::spawn::shutdown_world_on_teardown)
            .in_set(ClientSet::Load),
    );
}
