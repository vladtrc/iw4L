use bevy::prelude::*;
use bevy::render::{Extract, ExtractSchedule, RenderApp};

use super::colour_submit::{
    ColourSubmitScratch, ExactColourBindingCache, ExactPipelineKickCache, ExactShadowBindingCache,
    ShadowSubmitScratch,
};
use super::exact_pipeline::ExactPipelineRegistry;
use super::iw_tess::SavedScreenGpu;
use super::shellshock_screen::TemporalScreenGpu;
use super::{InstalledRenderWorld, PublishedRenderFrame};

pub(crate) fn register(app: &mut App) {
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<InstalledRenderWorld>()
            .init_resource::<PublishedRenderFrame>()
            .init_resource::<render_scene::SkinFailure>()
            .init_resource::<render_scene::WorldPresentAck>()
            .add_systems(
                ExtractSchedule,
                apply_scope_transitions.in_set(frame::ScopeSet::Install),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MatchPipelineState<'w> {
    pipelines: ResMut<'w, ExactPipelineRegistry>,
    colour_scratch: ResMut<'w, ColourSubmitScratch>,
    shadow_scratch: ResMut<'w, ShadowSubmitScratch>,
    colour_bindings: ResMut<'w, ExactColourBindingCache>,
    shadow_bindings: ResMut<'w, ExactShadowBindingCache>,
    kicked: ResMut<'w, ExactPipelineKickCache>,
    warmup: ResMut<'w, crate::WorldPipelineWarmup>,
    working: ResMut<'w, crate::ColourWorkingSet>,
}

impl MatchPipelineState<'_> {
    fn clear(&mut self) {
        *self.pipelines = Default::default();
        *self.colour_scratch = Default::default();
        *self.shadow_scratch = Default::default();
        *self.colour_bindings = Default::default();
        *self.shadow_bindings = Default::default();
        *self.kicked = Default::default();
        *self.warmup = Default::default();
        *self.working = Default::default();
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct RenderWorldPublication<'w> {
    skin_failure: ResMut<'w, render_scene::SkinFailure>,
    present_ack: ResMut<'w, render_scene::WorldPresentAck>,
    world: ResMut<'w, InstalledRenderWorld>,
    published: ResMut<'w, PublishedRenderFrame>,
}

impl RenderWorldPublication<'_> {
    fn clear(&mut self) {
        *self.skin_failure = Default::default();
        *self.present_ack = Default::default();
        *self.world = Default::default();
        *self.published = Default::default();
    }
}

fn apply_scope_transitions(
    mut matches: Extract<MessageReader<StateTransitionEvent<frame::MatchScope>>>,
    mut timelines: Extract<MessageReader<StateTransitionEvent<frame::Timeline>>>,
    mut pipelines: MatchPipelineState,
    mut publication: RenderWorldPublication,
    mut temporal: ResMut<TemporalScreenGpu>,
    mut saved: ResMut<SavedScreenGpu>,
) {
    let mut match_boundary = false;
    for transition in matches.read() {
        if transition.entered != transition.exited || transition.allow_same_state_transitions {
            match_boundary |= transition.entered == Some(frame::MatchScope::Live)
                || transition.exited == Some(frame::MatchScope::Live);
        }
    }
    let mut timeline_boundary = false;
    for transition in timelines.read() {
        timeline_boundary |=
            transition.entered != transition.exited || transition.allow_same_state_transitions;
    }
    if match_boundary {
        pipelines.clear();
        publication.clear();
    }
    if match_boundary || timeline_boundary {
        *temporal = Default::default();
        *saved = Default::default();
    }
}
