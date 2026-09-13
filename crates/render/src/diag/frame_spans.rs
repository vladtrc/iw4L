use bevy::app::{RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy::prelude::*;

fn begin_postupdate() {
    perf::Span::FramesPostupdateMs.begin();
}

fn end_postupdate() {
    perf::Span::FramesPostupdateMs.end();
}

pub(crate) fn register_postupdate_span(app: &mut App) {
    app.add_systems(
        PostUpdate,
        begin_postupdate.before(bevy::transform::TransformSystems::Propagate),
    )
    .add_systems(Last, end_postupdate);
}

pub(crate) fn register_preupdate_span(app: &mut App) {
    app.add_systems(First, begin_preupdate).add_systems(
        RunFixedMainLoop,
        end_preupdate.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
    );
}

fn begin_preupdate() {
    perf::Span::FramesPreupdateMs.begin();
}

fn end_preupdate() {
    perf::Span::FramesPreupdateMs.end();
}
