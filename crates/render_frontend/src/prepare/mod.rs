use bevy::prelude::*;
use frame::ScopeApp;
use frame::{ModelLightingSeated, WorkerCmdSet};
use net::ClientSet;

pub mod gfx_scene;
pub mod scene;
pub mod sun;
pub mod worker_cmds;

use crate::adapters::anim::view_kick::sync_camera_from_presented;
use crate::assemble::drawsurf::{
    ColourDrawMethod, DrawSurfList, MaterialFrameInputs, MaterialGeneration, RenderFrameProducts,
    StaticDrawLane,
};
use crate::prepare::scene::camera::fly_camera;
use crate::prepare::scene::cull::{
    DpvsFrameStats, apply_dpvs_cull, log_dpvs_stats_once, log_script_model_gaps_once,
};
use crate::prepare::scene::smodel_lighting::update_smodel_lighting;
use crate::prepare::scene::spawn::{
    WorldSpawnJob, register_world_gpu_ready, spawn_world, spawn_world_finish,
};
use crate::prepare::scene::view_parms::stamp_prepared_scene_view;
use crate::prepare::scene::world::WorldScene;

pub struct RenderPreparePlugin;

impl Plugin for RenderPreparePlugin {
    fn build(&self, app: &mut App) {
        super::assemble::drawsurf::dof::register(app);
        super::assemble::drawsurf::film_vision_view::register(app);
        app

            .scoped::<WorldSpawnJob>(frame::MatchScope::Live)
            .scoped::<DpvsFrameStats>(frame::MatchScope::Live)
            .scoped::<DrawSurfList>(frame::MatchScope::Live)
            .scoped::<StaticDrawLane>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::XModelDrawLane>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::FxDrawLane>(frame::MatchScope::Live)
            .scoped::<RenderFrameProducts>(frame::MatchScope::Live)
            .init_resource::<ColourDrawMethod>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::SmcEnableDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::PretessDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::LodRampDvar>()
            .init_resource::<crate::prepare::scene::smodel_geom_cache::LodRampSkinnedDvar>()
            .scoped::<crate::prepare::scene::smodel_geom_cache::FrontendWorkerCmds>(frame::MatchScope::Live)
            .configure_sets(
                Update,
                (
                    crate::prepare::scene::gfx_scene::GfxSceneClear
                        .after(spawn_world)
                        .before(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                    crate::prepare::scene::gfx_scene::GfxSceneAdd
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(WorkerCmdSet::WorldVisibility)
                        .before(WorkerCmdSet::DynamicModels)
                        .before(WorkerCmdSet::ModelPose),
                )
                    .in_set(ClientSet::Present),
            )
            .scoped::<crate::assemble::drawsurf::MapSunEffects>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SunEffectsFrameInput>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::sun_effects::SunEffectsCameraTrack>(frame::Timeline::Live)
            .scoped::<crate::assemble::drawsurf::MapPrimaryLightTypes>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::MapPrimaryLights>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::DrawMethodDfog>(frame::MatchScope::Live)
            .init_resource::<crate::assemble::drawsurf::fog::FogDvars>()
            .scoped::<crate::assemble::drawsurf::SunShadowMapPresent>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SunShadowUnmatchedLights>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SpotShadowMapLights>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SunShadowCasterPlan>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SpotShadowCasterPlan>(frame::MatchScope::Live)
            .scoped::<MaterialGeneration>(frame::MatchScope::Live)
            .scoped::<MaterialFrameInputs>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::FrameAssemblyInputs>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::CameraProducts>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::DistortionSettings>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SunProduct>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::SpotProduct>(frame::MatchScope::Live)
            .scoped::<crate::assemble::drawsurf::fog::ScriptFogPresentation>(frame::MatchScope::Live)





            .scoped::<crate::assemble::drawsurf::FxModelDrawPlan>(frame::MatchScope::Live)

            .scoped::<crate::assemble::drawsurf::XModelDrawPlan>(frame::MatchScope::Live)
            .init_resource::<frame::ClassSelectHandoff>()
            .add_systems(
                Update,
                (
                    spawn_world.in_set(frame::InMatch),
                    spawn_world_finish.in_set(frame::InMatch).after(spawn_world),
                    render_anim::prepare_fpv_compositions.in_set(frame::InMatch).after(spawn_world_finish),
                    render_anim::prepare_model_materials.in_set(frame::InMatch).after(spawn_world_finish),
                    crate::assemble::drawsurf::tess::glass::apply_cg_glass_tess.in_set(frame::InMatch)
                        .after(spawn_world)
                        .after(crate::adapters::fx::system::tick_fx_non_dependent_update),
                    crate::assemble::drawsurf::fog::sync_script_fog.in_set(frame::InMatch)
                        .after(spawn_world_finish)
                        .before(stamp_prepared_scene_view),
                    fly_camera,
                    stamp_prepared_scene_view
                        .after(fly_camera)
                        .after(sync_camera_from_presented)
                        .after(crate::adapters::anim::fpv_present::spawn_pending_fpv)
                        .after(crate::adapters::anim::fpv_present::tick_fpv_viewmodel),
                    crate::assemble::drawsurf::publish_sun_effects_frame.in_set(frame::InMatch)
                        .after(stamp_prepared_scene_view),
                    crate::assemble::drawsurf::ingest_drawsurf_list.in_set(frame::InMatch)
                        .after(WorkerCmdSet::WorldVisibility)
                        .after(WorkerCmdSet::DynamicModels)
                        .after(crate::adapters::anim::dyn_ent_brush::exec_cell_dyn_brush_cmds),
                    update_smodel_lighting.in_set(frame::InMatch)
                        .after(WorkerCmdSet::WorldVisibility)
                        .after(WorkerCmdSet::ModelPose)
                        .after(crate::adapters::anim::script_model::ScriptModelSkinSet),
                    crate::prepare::scene::smodel_geom_cache::cache_visible_smodel_surfaces.in_set(frame::InMatch)
                        .after(update_smodel_lighting),
                    crate::prepare::scene::gfx_scene::snapshot_spot_shadow_occupancy.in_set(frame::InMatch)
                        .after(crate::prepare::scene::gfx_scene::GfxSceneAdd)
                        .after(WorkerCmdSet::ModelPose),
                    crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame.in_set(frame::InMatch)
                        .after(spawn_world),
                    crate::prepare::scene::model_lighting_cache::enqueue_fpv_model_lighting.in_set(frame::InMatch)
                        .after(render_anim::spawn_pending_fpv)
                        .after(render_anim::tick_fpv_viewmodel)
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(render_anim::ScriptModelDrawSet)
                        .before(WorkerCmdSet::DynamicModels),
                    crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting.in_set(frame::InMatch)
                        .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                        .after(crate::assemble::drawsurf::tess::glass::apply_cg_glass_tess)
                        .after(WorkerCmdSet::ModelPose)
                        .after(crate::adapters::anim::script_model::ScriptModelSkinSet)
                        .after(WorkerCmdSet::DynamicModels)
                        .after(crate::prepare::scene::model_lighting_cache::update_dirty_model_lighting),
                    log_dpvs_stats_once.in_set(frame::InMatch).after(WorkerCmdSet::WorldVisibility),
                    log_script_model_gaps_once,
                )
                    .in_set(ClientSet::Present),
            )

            .add_systems(
                Update,
                crate::prepare::scene::model_lighting_cache::update_dirty_model_lighting.in_set(frame::InMatch)
                    .after(crate::prepare::scene::model_lighting_cache::begin_dyn_model_lighting_frame)
                    .after(WorkerCmdSet::ModelPose)
                    .after(WorkerCmdSet::DynamicModels)
                    .after(crate::adapters::anim::script_model::ScriptModelSkinSet)
                    .after(crate::prepare::scene::model_lighting_cache::enqueue_fpv_model_lighting)
                    .in_set(ModelLightingSeated)
                    .before(crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting),
            )
            .add_systems(
                Update,
                (
                    crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting.in_set(frame::InMatch)
                        .after(crate::assemble::drawsurf::tess::glass::enqueue_glass_model_lighting)
                        .before(crate::assemble::prepare_static_sun_and_fx),
                    crate::assemble::drawsurf::tess::glass::apply_glass_model_lighting.in_set(frame::InMatch)
                        .after(crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting),
                    crate::prepare::scene::model_lighting_cache::update_fx_dyn_lighting.in_set(frame::InMatch)
                        .after(crate::assemble::prepare_static_sun_and_fx)
                        .after(crate::prepare::scene::model_lighting_cache::update_glass_dyn_lighting),
                    crate::assemble::drawsurf::tess::xmodel::apply_resolved_fx_model_lighting.in_set(frame::InMatch)
                        .after(crate::prepare::scene::model_lighting_cache::update_fx_dyn_lighting),
                )
                    .in_set(ClientSet::Present),
            )
            .add_systems(
                Update,
                (
                    crate::prepare::scene::smodel_geom_cache::skin_cached_static_model_cmd.in_set(frame::InMatch)
                        .in_set(net::ClientSet::Present)
                        .after(crate::prepare::scene::smodel_geom_cache::cache_visible_smodel_surfaces),
                    crate::prepare::scene::gfx_scene::clear_host_gfx_scene.in_set(frame::InMatch)
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneClear),
                    crate::prepare::scene::gfx_scene::occupy_script_brush_scene.in_set(frame::InMatch)
                        .after(spawn_world)
                        .after(crate::adapters::anim::script_model::ScriptModelDrawSet)
                        .after(crate::prepare::scene::gfx_scene::apply_anim_dobj_scene_submissions)
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                    crate::prepare::scene::gfx_scene::apply_anim_dobj_scene_submissions.in_set(frame::InMatch)
                        .after(crate::adapters::anim::scene_submission::AnimSceneSubmit)
                        .in_set(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                ),
            )
            .add_systems(
                Update,
                apply_dpvs_cull.in_set(frame::InMatch)
                    .after(fly_camera)
                    .after(stamp_prepared_scene_view)
                    .after(crate::prepare::scene::sun_stage::update_active_sun_stage)
                    .in_set(WorkerCmdSet::WorldVisibility),
            )
            .add_systems(
                Update,
                crate::assemble::drawsurf::publish_sun_shadow_frame.in_set(frame::InMatch)
                    .after(stamp_prepared_scene_view)
                    .after(crate::prepare::scene::sun_stage::update_active_sun_stage)
                    .before(crate::adapters::anim::script_model::ScriptModelSkinSet)
                    .before(crate::assemble::drawsurf::update_command_context_code_sources),
            )
            .add_systems(
                Update,
                crate::prepare::scene::sun_stage::update_active_sun_stage.in_set(frame::InMatch)
                    .after(stamp_prepared_scene_view)
                    .before(crate::assemble::drawsurf::update_command_context_code_sources)
                    .in_set(ClientSet::Present),
            )
            ;

        app.add_systems(
            OnEnter(frame::MatchScope::Live),
            (publish_dyn_atpoint_lookup).in_set(frame::ScopeSet::Derive),
        );
        register_world_gpu_ready(app);
        crate::prepare::scene::cell_frustum_cmds::register_cell_frustum_cmds(app);
    }
}

fn publish_dyn_atpoint_lookup(
    scene: Res<WorldScene>,
    mut lookup: ResMut<render_scene::DynAtPointLookup>,
    mut cells: ResMut<render_scene::WorldDpvsCells>,
) {
    // Static tables are complete when the install transaction publishes this message.
    scene.publish_dyn_atpoint(&mut lookup);
    scene.publish_dpvs_cells(&mut cells);
}
