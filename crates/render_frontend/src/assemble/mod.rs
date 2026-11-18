use bevy::prelude::*;
use frame::ScopeApp;

pub mod drawsurf;
pub mod pack;

#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StaticSunAndFx;

pub(crate) fn prepare_static_sun_and_fx(world: &mut World) {
    let _prepare = perf::span!("static_sun_fx", "iw4l.render");
    world.run_schedule(StaticSunAndFx);
}

#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DrawLaneRebuild;

fn rebuild_draw_lanes(world: &mut World) {
    world.run_schedule(DrawLaneRebuild);
}

pub struct RenderAssemblePlugin;

impl Plugin for RenderAssemblePlugin {
    fn build(&self, app: &mut App) {
        app.scoped::<drawsurf::StaticSunCasters>(frame::MatchScope::Live)
            .add_systems(StaticSunAndFx, drawsurf::bake_static_sun_shadow_casters)
            .edit_schedule(StaticSunAndFx, |schedule| {
                schedule.set_executor(bevy::ecs::schedule::MultiThreadedExecutor::new());
            })
            .add_systems(
                Update,
                prepare_static_sun_and_fx
                    .in_set(frame::InMatch)
                    .in_set(net::ClientSet::Present)
                    .after(crate::adapters::fx::system::tick_fx_remaining_update)
                    .after(crate::prepare::scene::smodel_geom_cache::skin_cached_static_model_cmd)
                    .after(drawsurf::update_command_context_code_sources)
                    .after(crate::prepare::scene::cull::apply_dpvs_cull)
                    .after(crate::prepare::scene::smodel_lighting::update_smodel_lighting),
            );

        app.add_systems(
            Update,
            crate::assemble::drawsurf::tess::world::build_world_draw_gpu_plan
                .in_set(frame::InMatch)
                .after(crate::prepare::scene::spawn::spawn_world)
                .before(crate::prepare::scene::spawn::spawn_world_finish)
                .in_set(net::ClientSet::Present),
        );
        app.add_systems(
            Update,
            crate::assemble::drawsurf::tess::smodel::build_smodel_gpu_plan
                .in_set(frame::InMatch)
                .after(crate::prepare::scene::spawn::spawn_world_finish)
                .before(frame::WorkerCmdSet::WorldVisibility)
                .in_set(net::ClientSet::Present),
        );
        app.add_systems(
            Update,
            crate::assemble::drawsurf::tess::sky::build_sky_model_draw_plan
                .after(crate::prepare::scene::spawn::spawn_world_finish)
                .before(rebuild_draw_lanes)
                .in_set(net::ClientSet::Present),
        );

        app.add_systems(
            Update,
            crate::assemble::drawsurf::tess::xmodel::apply_resolved_xmodel_lighting
                .in_set(frame::InMatch)
                .after(crate::prepare::scene::model_lighting_cache::update_dirty_model_lighting)
                .in_set(net::ClientSet::Present),
        );

        app.add_systems(
            Update,
            crate::assemble::drawsurf::update_command_context_code_sources
                .after(crate::prepare::scene::cull::apply_dpvs_cull)
                // Brush membership extends the BSP masks. Publish the complete
                // view to the GPU only after those owners have been admitted.
                .after(crate::adapters::anim::dyn_ent_brush::exec_cell_dyn_brush_cmds)
                .after(crate::adapters::anim::dyn_ent_brush::exec_cell_scene_ent_cmds)
                .in_set(frame::InMatch)
                .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
                .in_set(net::ClientSet::Present),
        );
        app.add_systems(
            DrawLaneRebuild,
            (
                crate::assemble::drawsurf::rebuild_xmodel_draw_lane,
                crate::assemble::drawsurf::rebuild_fx_draw_lane,
                crate::assemble::drawsurf::rebuild_static_draw_lane,
            ),
        )
        .edit_schedule(DrawLaneRebuild, |schedule| {
            schedule.set_executor(bevy::ecs::schedule::MultiThreadedExecutor::new());
        })
        .add_systems(
            Update,
            rebuild_draw_lanes
                .in_set(frame::InMatch)
                .after(frame::ScreenEffectsPublished)
                .after(crate::adapters::anim::fpv_present::FpvPlacementSet)
                .after(crate::adapters::anim::fpv_present::FpvGeometrySet)
                .after(frame::ModelLightingSeated)
                .after(render_anim::occupancy::remote_body::finish_body_draw_plan)
                .after(crate::assemble::drawsurf::tess::xmodel::apply_resolved_xmodel_lighting)
                .after(crate::assemble::drawsurf::tess::xmodel::apply_resolved_fx_model_lighting)
                .after(crate::adapters::anim::script_model::ScriptModelDrawSet)
                .after(crate::assemble::prepare_static_sun_and_fx)
                .after(crate::assemble::drawsurf::tess::glass::apply_glass_model_lighting)
                .after(crate::prepare::scene::cull::apply_dpvs_cull)
                .after(crate::prepare::scene::smodel_lighting::update_smodel_lighting)
                .after(crate::prepare::scene::smodel_geom_cache::skin_cached_static_model_cmd)
                .after(crate::assemble::drawsurf::ingest_drawsurf_list)
                .after(crate::assemble::drawsurf::update_command_context_code_sources)
                .in_set(net::ClientSet::Present),
        );

        app.add_systems(
            Update,
            (
                crate::assemble::drawsurf::open_frame_products
                    .in_set(frame::InMatch)
                    .after(crate::assemble::prepare_static_sun_and_fx)
                    .after(crate::assemble::drawsurf::update_command_context_code_sources),
                crate::assemble::drawsurf::bake_sun_shadow_casters
                    .in_set(frame::InMatch)
                    .after(prepare_static_sun_and_fx)
                    .after(crate::assemble::drawsurf::open_frame_products)
                    .after(rebuild_draw_lanes)
                    .after(crate::prepare::scene::smodel_geom_cache::skin_cached_static_model_cmd)
                    .after(crate::prepare::scene::cull::apply_dpvs_cull)
                    .after(crate::prepare::scene::smodel_lighting::update_smodel_lighting),
                crate::assemble::drawsurf::bake_spot_shadow_casters
                    .in_set(frame::InMatch)
                    .after(crate::assemble::drawsurf::open_frame_products)
                    .after(rebuild_draw_lanes)
                    .after(crate::prepare::scene::gfx_scene::snapshot_spot_shadow_occupancy)
                    .after(crate::prepare::scene::cull::apply_dpvs_cull)
                    .after(rebuild_draw_lanes),
                crate::assemble::drawsurf::execute_sun_product
                    .in_set(frame::InMatch)
                    .after(crate::assemble::drawsurf::bake_sun_shadow_casters)
                    .after(rebuild_draw_lanes),
                crate::assemble::drawsurf::execute_spot_product
                    .in_set(frame::InMatch)
                    .after(crate::assemble::drawsurf::bake_spot_shadow_casters)
                    .after(rebuild_draw_lanes),
                crate::assemble::drawsurf::execute_camera_products
                    .in_set(frame::InMatch)
                    .after(crate::assemble::drawsurf::bake_sun_shadow_casters)
                    .after(crate::assemble::drawsurf::bake_spot_shadow_casters)
                    .after(rebuild_draw_lanes)
                    .after(rebuild_draw_lanes)
                    .after(rebuild_draw_lanes),
                crate::assemble::drawsurf::publish_frame_products
                    .in_set(frame::InMatch)
                    .after(crate::assemble::drawsurf::execute_camera_products)
                    .after(crate::assemble::drawsurf::execute_sun_product)
                    .after(crate::assemble::drawsurf::execute_spot_product),
            )
                .in_set(net::ClientSet::Present),
        );
        app.add_systems(
            PostUpdate,
            crate::assemble::drawsurf::log_probe_index_census
                .in_set(frame::InMatch)
                .after(bevy::transform::TransformSystems::Propagate),
        );
    }
}
