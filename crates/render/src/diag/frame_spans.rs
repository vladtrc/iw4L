//! The frame clock and the schedule spans it encloses.
//!
//! Every span here opens and closes inside one turn of the main loop, and the
//! clock is the boundary they are measured against: it closes the row for the
//! frame that just ran and opens the next one at the top of `First`, before
//! anything else this frame does. That ordering is the whole point: a clock
//! that closes from inside `Update` cuts the row with `Update` still open, and
//! the schedule reports a fraction of itself in its own column while the rest
//! lands in the next row as carried-in time.

use bevy::app::{RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy::prelude::*;

/// The frame clock, and with it the one allocation figure that is per frame
/// rather than per system: the process-wide count since the previous bookend.
/// It is emitted where the clock closes so the count and the frame it belongs
/// to are the same interval.
fn bookend_wall_frame(mut wall_open: Local<bool>, mut allocations: Local<Option<u64>>) {
    if *wall_open {
        perf::Span::FramesWallFrameMs.end();
    }
    // Only when the allocator is actually counting: otherwise the total is a
    // constant zero and the delta would publish "no allocations this frame",
    // which is a different claim from "nobody counted".
    if perf::stats::enabled() && diag::counting_enabled() {
        let total = diag::process_allocations().process_allocations;
        if let Some(previous) = *allocations {
            perf::Counter::CounterProcessAllocations.emit(total.saturating_sub(previous) as f64);
        }
        *allocations = Some(total);
    }
    perf::Span::FramesWallFrameMs.begin();
    *wall_open = true;
}

fn begin_postupdate() {
    perf::Span::FramesPostupdateMs.begin();
}

fn end_postupdate() {
    perf::Span::FramesPostupdateMs.end();
}

/// The clock and the three schedule spans, registered together because their
/// order relative to one another is what the measurement is.
pub(crate) fn register_frame_spans(app: &mut App) {
    app.add_systems(First, (bookend_wall_frame, begin_preupdate).chain())
        .add_systems(
            RunFixedMainLoop,
            end_preupdate.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        )
        .add_systems(
            PostUpdate,
            begin_postupdate.before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(Last, end_postupdate);
}

fn begin_preupdate() {
    perf::Span::FramesPreupdateMs.begin();
}

fn end_preupdate() {
    perf::Span::FramesPreupdateMs.end();
}
