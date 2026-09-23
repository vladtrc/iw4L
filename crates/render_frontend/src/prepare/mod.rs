use bevy::prelude::*;
use frame::{HasWorld, ModelLightingSeated, SessionSwapApplied, WorkerCmdSet};
use net::ClientSet;

pub mod gfx_scene;
pub mod scene;
pub mod sun;
pub mod worker_cmds;

use crate::adapters::anim::view_kick::sync_camera_from_presented;
use crate::assemble::drawsurf::{
    ColourDrawMethod, DrawSurfList, FpvDrawPlan, MaterialFrameInputs, MaterialGeneration,
    RenderFrameProducts, StaticDrawLane,
};
use crate::prepare::scene::camera::fly_camera;
use crate::prepare::scene::cull::{
    DpvsFrameStats, apply_dpvs_cull, log_dpvs_stats_once, log_script_model_gaps_once,
};
use crate::prepare::scene::smodel_lighting::update_smodel_lighting;
use crate::prepare::scene::spawn::{
    WorldSpawnJob, arm_world_spawn_on_install, despawn_fly_cameras_on_teardown,
    despawn_world_entities_on_teardown, register_world_gpu_ready, reset_world_spawn_on_teardown,
    shutdown_world_on_teardown, spawn_world, spawn_world_finish,
};
use crate::prepare::scene::view_parms::stamp_prepared_scene_view;
use crate::prepare::scene::world::WorldScene;

pub struct RenderPreparePlugin;

impl Plugin for RenderPreparePlugin {
    fn build(&self, app: &mut App) {
        super::assemble::drawsurf::dof::register(app);
        super::assemble::drawsurf::film_vision_view::register(app);
        app.init_resource::<HasWorld>()
            .init_resource::<WorldScene>()
            .init_resource::<WorldSpawnJob>()
            .init_resource::<DpvsFrameStats>()
            .init_resource::<DrawSurfList>()
            .init_resource::<StaticDrawLane>()
            .init_resource::<crate::assemble::drawsurf::XModelDrawLane>()
            .init_resource::<crate::assemble::drawsurf::FxDrawLane>()
            .init_resource::<RenderFrameProducts>()
            .init_resource::<ColourDrawMethod>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::SmcEnableDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::PretessDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::LodRampDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::LodRampSkinnedDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::FrontendWorkerCmds>()
            .configure_sets(
                Update,
                (
                    crate::prepare::scene::gfx_scene::GfxSceneClear
                        .after(spawn_world)
                        .before(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                    crate::prepare::scene::gfx_scene::GfxSceneAdd
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(WorkerCmdSet::CellStatic)
                        .before(WorkerCmdSet::CellDynModel)
                        .before(WorkerCmdSet::SkinModel),
                )
                    .in_set(ClientSet::Present),
            )
            .init_resource::<crate::assemble::drawsurf::MapSunEffects>()
            .init_resource::<crate::assemble::drawsurf::SunEffectsFrameInput>()
            .init_resource::<crate::assemble::drawsurf::MapPrimaryLightTypes>()
            .init_resource::<crate::assemble::drawsurf::MapPrimaryLights>()
            .init_resource::<crate::assemble::drawsurf::DrawMethodDfog>()
            .init_resource::<crate::assemble::drawsurf::fog::FogDvars>()
            .init_resource::<crate::assemble::drawsurf::SunShadowMapPresent>()
            .init_resource::<crate::assemble::drawsurf::SpotShadowMapLights>()
            .init_resource::<crate::assemble::drawsurf::SunShadowCasterPlan>()
            .init_resource::<crate::assemble::drawsurf::SpotShadowCasterPlan>()
            .init_resource::<MaterialGeneration>()
            .init_resource::<MaterialFrameInputs>()
            .init_resource::<crate::assemble::drawsurf::FrameAssemblyInputs>()
            .init_resource::<crate::assemble::drawsurf::CameraProducts>()
            .init_resource::<crate::assemble::drawsurf::DistortionSettings>()
            .init_resource::<crate::assemble::drawsurf::SunProduct>()
            .init_resource::<crate::assemble::drawsurf::SpotProduct>()
            .init_resource::<FpvDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::RemoteBodyDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::ScriptModelDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::MissileDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::ItemDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::FxModelDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::DynEntDrawPlan>()
            .init_resource::<crate::assemble::drawsurf::XModelDrawPlan>()
            .init_resource::<frame::ClassSelectHandoff>()
            .add_systems(
                Update,
                (
                    arm_world_spawn_on_install.before(spawn_world),
                    spawn_world,
                    spawn_world_finish.after(spawn_world),
                    crate::assemble::drawsurf::tess::glass::apply_cg_glass_tess
                        .after(spawn_world)
                        .after(WorkerCmdSet::FxNonDependent),
                    fly_camera,
                    stamp_prepared_scene_view
                        .after(fly_camera)
                        .after(sync_camera_from_presented)
                        .after(crate::adapters::anim::fpv_present::spawn_pending_fpv)
                        .after(crate::adapters::anim::fpv_present::tick_fpv_viewmodel),
                    crate::assemble::drawsurf::publish_sun_effects_frame
                        .after(stamp_prepared_scene_view),
                    crate::assemble::drawsurf::ingest_drawsurf_list
                        .after(WorkerCmdSet::CellStatic)
                        .after(WorkerCmdSet::CellDynModel)
                        .after(WorkerCmdSet::CellDynBrush),
                    update_smodel_lighting
                        .after(WorkerCmdSet::CellStatic)
                        .after(WorkerCmdSet::SkinModel)
                        .after(crate::adapters::anim::script_model::ScriptModelSkinSet),
                    crate::prepare::scene::smodel_geom_cache::cache_visible_smodel_surfaces
                        .after(update_smodel_lighting),
                    crate::prepare::scene::gfx_scene::snapshot_spot_shadow_occupancy
                        .after(crate::prepare::scene::gfx_scene::GfxSceneAdd)
                        .after(WorkerCmdSet::SkinModel),
                    crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame
                        .after(spawn_world),
                    crate::prepare::scene::model_lighting_cache::enqueue_fpv_model_lighting
                        .after(render_anim::spawn_pending_fpv)
                        .after(render_anim::tick_fpv_viewmodel)
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(render_anim::ScriptModelDrawSet)
                        .before(WorkerCmdSet::CellDynModel),
                    crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(crate::assemble::drawsurf::tess::glass::apply_cg_glass_tess)
                        .after(WorkerCmdSet::SkinModel)
                        .after(crate::adapters::anim::script_model::ScriptModelSkinSet)
                        .after(WorkerCmdSet::CellDynModel)
                        .after(crate::prepare::scene::model_lighting_cache::update_dirty_model_lighting),
                    log_dpvs_stats_once.after(WorkerCmdSet::CellStatic),
                    log_script_model_gaps_once,
                )
                    .in_set(ClientSet::Present),
            )

            .add_systems(
                Update,
                crate::prepare::scene::model_lighting_cache::update_dirty_model_lighting
                    .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                    .after(WorkerCmdSet::SkinModel)
                    .after(WorkerCmdSet::CellDynModel)
                    .after(crate::adapters::anim::script_model::ScriptModelSkinSet)
                    .after(crate::prepare::scene::model_lighting_cache::enqueue_fpv_model_lighting)
                    .in_set(ModelLightingSeated)
                    .before(crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting),
            )
            .add_systems(
                Update,
                (
                    crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting
                        .after(crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting)
                        .before(WorkerCmdSet::FxVerts),
                    crate::assemble::drawsurf::tess::glass::apply_glass_model_lighting
                        .after(crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting),
                    crate::prepare::scene::model_lighting_cache::update_fx_dyn_lighting
                        .after(WorkerCmdSet::FxVerts)
                        .after(crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting),
                    crate::assemble::drawsurf::tess::xmodel::apply_resolved_fx_model_lighting
                        .after(crate::prepare::scene::model_lighting_cache::update_fx_dyn_lighting),
                )
                    .in_set(ClientSet::Present),
            )
            .add_systems(
                Update,
                (
                    crate::prepare::scene::smodel_geom_cache::skin_cached_static_model_cmd
                        .in_set(WorkerCmdSet::SmodelCache)
                        .after(crate::prepare::scene::smodel_geom_cache::cache_visible_smodel_surfaces),
                    crate::prepare::scene::gfx_scene::clear_host_gfx_scene
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneClear),
                    crate::prepare::scene::gfx_scene::occupy_script_brush_scene
                        .after(spawn_world)
                        .after(crate::adapters::anim::script_model::ScriptModelDrawSet)
                        .after(crate::prepare::scene::gfx_scene::apply_anim_dobj_scene_submissions)
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                    crate::prepare::scene::gfx_scene::apply_anim_dobj_scene_submissions
                        .after(crate::adapters::anim::scene_submission::AnimSceneSubmit)
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                ),
            )
            .add_systems(
                Update,
                apply_dpvs_cull
                    .after(fly_camera)
                    .after(stamp_prepared_scene_view)
                    .in_set(WorkerCmdSet::CellStatic),
            )
            .init_resource::<frame::Retiring>()
            .add_systems(
                Update,
                (
                    despawn_world_entities_on_teardown,
                    reset_world_spawn_on_teardown,
                    shutdown_world_on_teardown,
                    despawn_fly_cameras_on_teardown,
                )
                    .chain()
                    .after(SessionSwapApplied)
                    .in_set(ClientSet::Load),
            );

        app.add_systems(
            Update,
            publish_dyn_atpoint_lookup
                .after(SessionSwapApplied)
                .after(shutdown_world_on_teardown)
                .in_set(ClientSet::Load),
        );
        register_world_gpu_ready(app);
        crate::prepare::scene::cell_frustum_cmds::register_cell_frustum_cmds(app);
    }
}

fn publish_dyn_atpoint_lookup(
    mut installed: MessageReader<frame::MatchInstalled>,
    scene: Res<WorldScene>,
    mut lookup: ResMut<render_scene::DynAtPointLookup>,
    mut cells: ResMut<render_scene::WorldDpvsCells>,
) {
    // Static tables are complete when the install transaction publishes this message.
    if installed.read().count() != 0 {
        scene.publish_dyn_atpoint(&mut lookup);
        scene.publish_dpvs_cells(&mut cells);
    }
}
