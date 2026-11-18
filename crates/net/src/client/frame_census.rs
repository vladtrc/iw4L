use std::time::Instant;

use bevy::prelude::*;

use crate::schedule::ClientSet;
use frame::ClientEdge;

fn stamp_client_edge<const EDGE: u8>() {
    if EDGE > 2 {
        client_toc_span(EDGE as usize - 1).end();
    }
    if EDGE == 2 {
        perf::span!("Update", "iw4l.sim", begin);
    }
    if (EDGE as usize) < 10 {
        client_toc_span(EDGE as usize).begin();
    }
    if EDGE as usize == 10 {
        perf::span!("Update", "iw4l.sim", end);
    }
}

fn stamp_preupdate_edge<const EDGE: u8>() {
    if EDGE == 0 {
        perf::span!("PreUpdate", "iw4l.sim", begin);
    }
    if EDGE > 0 {
        client_toc_span(EDGE as usize - 1).end();
    }
    if EDGE < 2 {
        client_toc_span(EDGE as usize).begin();
    }
    if EDGE == 2 {
        perf::span!("PreUpdate", "iw4l.sim", end);
    }
}

fn client_toc_span(slot: usize) -> perf::Span {
    match slot {
        0 => perf::span!("Load", "iw4l.sim", handle),
        1 => perf::span!("Receive", "iw4l.sim", handle),
        2 => perf::span!("Reconcile", "iw4l.sim", handle),
        3 => perf::span!("Input", "iw4l.sim", handle),
        4 => perf::span!("Predict", "iw4l.sim", handle),
        5 => perf::span!("Send", "iw4l.sim", handle),
        6 => perf::span!("Present", "iw4l.sim", handle),
        7 => perf::span!("Ui", "iw4l.sim", handle),
        8 => perf::span!("Effects", "iw4l.sim", handle),
        9 => perf::span!("Diag", "iw4l.sim", handle),
        _ => unreachable!("client phase slot {slot}"),
    }
}

fn register_phase_census(app: &mut App) {
    macro_rules! seams {
        ($($edge:literal),* $(,)?) => {$(
            app.add_systems(
                Update,
                stamp_client_edge::<$edge>.in_set(ClientEdge($edge)),
            );
        )*};
    }
    seams!(2, 3, 4, 5, 6, 7, 8, 9, 10);
    app.add_systems(
        PreUpdate,
        (
            stamp_preupdate_edge::<0>.in_set(ClientEdge(0)),
            stamp_preupdate_edge::<1>.in_set(ClientEdge(1)),
            stamp_preupdate_edge::<2>.in_set(ClientEdge(2)),
            begin_update_census.before(ClientSet::Load),
        ),
    );
}

pub const HUD_STAGE_N: usize = 9;

#[derive(Resource, Debug, Default, Clone)]
pub struct UpdatePhaseCensus {
    pub present_started: Option<Instant>,

    pub hud_root_setup_ms: Option<f32>,

    pub present_apply_deferred_ms: Option<f32>,

    pub hud_visibility_ms: Option<f32>,

    pub hud_surfaces_ms: Option<f32>,

    pub hud_stage_ms: [Option<f32>; HUD_STAGE_N],

    /// Time spent inside the bodies of the HUD tess flush systems, summed over
    /// the frame, and how many tess jobs those bodies applied.
    ///
    /// This is what the HUD actually cost. `hud_stage_ms` and
    /// `hud_surfaces_ms` are intervals *between* systems, which is a different
    /// quantity: the executor is free to run anything it likes in a gap
    /// between two systems that only asked to be ordered, so a gap that is
    /// wide says the schedule put something there, not that the HUD was slow.
    pub hud_tess_body_ms: Option<f32>,
    pub hud_tess_jobs: Option<u32>,

    pub publish_presented_ms: Option<f32>,
}

fn begin_update_census(mut census: ResMut<UpdatePhaseCensus>) {
    *census = UpdatePhaseCensus::default();
}

fn begin_present_census(mut census: ResMut<UpdatePhaseCensus>) {
    census.present_started = Some(Instant::now());
}

/// The `Present` and `Ui` phases, broken into the systems that actually run in
/// them.
///
/// `Present` was 4.43 ms of self time in the bench report — time inside the
/// span that no child span accounted for — and "add one more span around the
/// whole thing" would have moved the number without naming anything. These are
/// the stamps the HUD and the client already take; they were sitting in a
/// resource nobody read. Emitting them here puts them in the counter report
/// and in `frames.csv`, where the remainder after subtracting them is a real
/// measurement of how much of `Present` is still unaccounted for.
///
/// Which phase each stamp belongs to is read off the schedule, not off the
/// field name: `LifeFrontPublished` is configured inside `ClientSet::Present`,
/// so the HUD surface stamps are `present_*`. The HUD root chain is in
/// `ClientSet::Ui`, so those three are `ui_*` — `present_apply_deferred_ms` is
/// misnamed in the census and the counter does not repeat the mistake.
///
/// The two `*_schedule_interval` counters are gaps between systems, not the
/// cost of a system's body: `.chain()` orders the HUD systems but does not
/// make them one indivisible block, so the executor can and does run other
/// work between two of them. They are named for what they measure so nothing
/// adds them to the body timings beside them, and `hud_tess_body` is the one
/// that says what the HUD itself spent.
///
/// `hud_stage_max_schedule_interval` is the largest of the nine gaps inside
/// `hud_surfaces_schedule_interval`, not a phase beside it: adding it to the
/// others would count that time twice.
fn publish_present_census(census: Res<UpdatePhaseCensus>) {
    if !perf::recording() {
        return;
    }
    let emit = |counter: perf::Counter, value: Option<f32>| {
        if let Some(ms) = value {
            counter.emit(f64::from(ms));
        }
    };
    emit(
        perf::counter!("present_publish", "iw4l.render", handle),
        census.publish_presented_ms,
    );
    emit(
        perf::counter!("hud_surfaces_schedule_interval", "iw4l.frame", handle),
        census.hud_surfaces_ms,
    );
    emit(
        perf::counter!("hud_tess_body", "iw4l.frame", handle),
        census.hud_tess_body_ms,
    );
    if let Some(jobs) = census.hud_tess_jobs {
        perf::counter!("hud_tess_jobs", "iw4l.frame", f64::from(jobs));
    }
    // Which of the nine, not only how big: a maximum with no index says a HUD
    // stage is most of the frame and leaves the next reader to bisect for it.
    let worst = census
        .hud_stage_ms
        .iter()
        .enumerate()
        .filter_map(|(at, ms)| ms.map(|ms| (at, ms)))
        .fold(None::<(usize, f32)>, |best, (at, ms)| match best {
            Some((_, best_ms)) if best_ms >= ms => best,
            _ => Some((at, ms)),
        });
    if let Some((at, ms)) = worst {
        perf::counter!(
            "hud_stage_max_schedule_interval",
            "iw4l.frame",
            f64::from(ms)
        );
        perf::counter!(
            "hud_stage_max_schedule_interval_at",
            "iw4l.frame",
            at as f64
        );
    }
    emit(
        perf::counter!("ui_hud_setup", "iw4l.render", handle),
        census.hud_root_setup_ms,
    );
    emit(
        perf::counter!("ui_apply_deferred", "iw4l.render", handle),
        census.present_apply_deferred_ms,
    );
    emit(
        perf::counter!("ui_hud_visibility", "iw4l.render", handle),
        census.hud_visibility_ms,
    );
}

pub fn register_update_phase_census(app: &mut App) {
    register_phase_census(app);
    app.init_resource::<UpdatePhaseCensus>().add_systems(
        Update,
        (
            begin_present_census.in_set(ClientSet::Send),
            // After both phases it measures — `Ui` runs after `Present`, and
            // `Diag` after `Ui` — and before the next frame's reset.
            publish_present_census.in_set(ClientSet::Diag),
        ),
    );
}
