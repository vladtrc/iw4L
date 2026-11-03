use bevy::prelude::*;
use frame::{ScopeApp, configure_render_sets};
use net::ClientSet;

use crate::diag::capture::{CaptureQueue, capture_frame, report_unwritten_captures};
use crate::diag::frame_spans::register_frame_spans;
use crate::diag::render_frame_diag::{register_render_frame_diag, sample_render_frame_diag};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        configure_render_sets(app);
        app.add_plugins((
            render_anim::RenderAnimPlugin,
            render_fx::RenderFxPlugin,
            render_scene::RenderScenePlugin,
            render_frontend::RenderPreparePlugin,
            render_frontend::RenderAdaptersPlugin,
            render_frontend::RenderAssemblePlugin,
            render_gpu::RenderGpuPlugin,
        ))
        .scoped::<render_gpu::InstalledRenderWorld>(frame::MatchScope::Live)
        .scoped::<render_gpu::PublishedRenderFrame>(frame::MatchScope::Live)
        .add_systems(
            PostUpdate,
            (
                stamp_gpu_submit_ready,
                crate::extract::seal_render_frame.in_set(frame::InMatch),
            )
                .chain()
                .after(frame::RenderSet::FrontendAssemble),
        )
        .add_systems(
            Update,
            (
                publish_overhead_posed_players.in_set(frame::InMatch),
                publish_overhead_posed_models.in_set(frame::InMatch),
            )
                .after(frame::WorkerCmdSet::ModelPose)
                .in_set(hud::OverheadPosedPlayerFramePublished)
                .in_set(ClientSet::Present),
        );

        crate::diag::acceptance::register_acceptance_systems(app);
        register_render_frame_diag(app);
        crate::resolution::register(app);
        app.add_systems(Update, sample_render_frame_diag.in_set(ClientSet::Diag));
        register_frame_spans(app);
        app.init_resource::<CaptureQueue>().add_systems(
            Update,
            (capture_frame, report_unwritten_captures)
                .chain()
                .in_set(ClientSet::Diag),
        );
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app.add_systems(
                bevy::render::ExtractSchedule,
                (
                    extract_gpu_load_progress,
                    crate::extract::extract_exact_colour,
                    crate::extract::extract_image_handles,
                    crate::extract::extract_postfx,
                    crate::extract::extract_model_lighting_tiles,
                )
                    .after(frame::ScopeSet::Install),
            );
            if render_gpu::geometry_diagnostic_enabled() {
                render_app.add_systems(
                    bevy::render::ExtractSchedule,
                    crate::extract::extract_geometry,
                );
            }
        }
    }
}

fn extract_gpu_load_progress(
    mut main_world: ResMut<bevy::render::MainWorld>,
    pipelines: Res<bevy::render::render_resource::PipelineCache>,
    working_set: Res<render_gpu::ColourWorkingSet>,
    warmup: Res<render_gpu::WorldPipelineWarmup>,
    skin_failure: Res<render_scene::SkinFailure>,
    present_ack: Res<render_scene::WorldPresentAck>,
) {
    use render_frontend::prepare::scene::world_gpu::{GpuLoadProgress, OverlayWarmup};

    *main_world.resource_mut::<render_gpu::ColourWorkingSet>() = *working_set;
    let Some(mut progress) = main_world.get_resource_mut::<GpuLoadProgress>() else {
        return;
    };
    progress.skin_failure = skin_failure.reason;
    progress.presented_at = present_ack.at;
    progress.waiting_n = pipelines.waiting_pipelines().count() as u32;
    progress.pipeline_n = pipelines.pipelines().count() as u32;
    progress.warmup = OverlayWarmup {
        initialized: warmup.initialized,
        total: warmup.total,
        ready: warmup.ready,
    };
    progress.working_hits = working_set.hits;
    progress.working_not_ready = working_set.pipeline_not_ready;
    let pending = std::mem::take(&mut progress.pending_shaders);
    progress.pending_shaders = waiting_pipeline_shaders(&pipelines, pending);
}

fn waiting_pipeline_shaders(
    pipelines: &bevy::render::render_resource::PipelineCache,
    mut out: Vec<bevy::asset::AssetId<bevy::shader::Shader>>,
) -> Vec<bevy::asset::AssetId<bevy::shader::Shader>> {
    use bevy::render::render_resource::PipelineDescriptor;

    out.clear();
    let waiting: std::collections::HashSet<_> = pipelines.waiting_pipelines().collect();
    if waiting.is_empty() {
        return out;
    }
    for (index, pipeline) in pipelines.pipelines().enumerate() {
        if !waiting.contains(&index) {
            continue;
        }
        if let PipelineDescriptor::RenderPipelineDescriptor(desc) = &pipeline.descriptor {
            out.push(desc.vertex.shader.id());
            out.extend(desc.fragment.iter().map(|fragment| fragment.shader.id()));
        }
    }
    out
}

fn stamp_gpu_submit_ready(
    demand: Option<Res<render_frontend::prepare::scene::world_gpu::GpuSubmitDemand>>,
    mut ready: ResMut<render_gpu::GpuSubmitReady>,
) {
    let Some(demand) = demand.as_ref() else {
        *ready = render_gpu::GpuSubmitReady::default();
        return;
    };
    *ready = render_gpu::GpuSubmitReady {
        warm_pipelines: demand.warm_pipelines,
        overlay_gpu_wait: demand.overlay_gpu_wait,
        pipeline_world_materials: demand.pipeline_world_materials.clone(),
        pipeline_smodel_materials: demand.pipeline_smodel_materials.clone(),
        pipeline_demand_revision: demand.pipeline_demand_revision,
    };
}

fn publish_overhead_posed_players(
    source: Res<render_anim::PosedPlayerFrame>,
    slots: Res<net::CEntitySlots>,
    mut destination: Query<&mut hud::OverheadNameState>,
) {
    for mut state in &mut destination {
        state.head = None;
    }
    for player in source.iter() {
        let Some(entity) = slots.entity_for_number(player.entnum) else {
            continue;
        };
        let Ok(mut state) = destination.get_mut(entity) else {
            continue;
        };
        state.head = Some(match player.head {
            render_anim::PosedPlayerHead::Exact(world) => {
                hud::OverheadPosedHead::ExactWorld(world.to_array())
            }
            render_anim::PosedPlayerHead::NoDObjOrHead => hud::OverheadPosedHead::NoDObjOrHead,
        });
    }
}

fn publish_overhead_posed_models(
    source: Res<render_anim::ScriptModelDObjFrame>,
    mut destination: Option<ResMut<hud::OverheadPosedModelFrame>>,
) {
    if let Some(destination) = destination.as_mut() {
        destination.replace(source.iter());
    }
}
