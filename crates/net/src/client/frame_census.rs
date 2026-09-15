use std::fmt::Write as _;
use std::time::Instant;

use bevy::prelude::*;

use crate::schedule::ClientSet;
use frame::{CLIENT_TOC, ClientEdge, client_set_name};

#[derive(Resource, Debug, Clone, Default)]
pub struct ClientPhaseCensus {
    pub n: [Option<u64>; CLIENT_TOC.len()],

    pub bytes: [Option<u64>; CLIENT_TOC.len()],
    seam_n: Option<u64>,
    seam_bytes: Option<u64>,
}

impl ClientPhaseCensus {
    pub fn alloc_n(&self) -> Option<String> {
        format_toc_u64(&self.n)
    }

    pub fn alloc_bytes(&self) -> Option<String> {
        format_toc_u64(&self.bytes)
    }

    pub fn alloc_sum_n(&self) -> Option<u64> {
        sum_named(&self.n)
    }

    pub fn alloc_sum_bytes(&self) -> Option<u64> {
        sum_named(&self.bytes)
    }
}

fn format_toc_u64(values: &[Option<u64>; CLIENT_TOC.len()]) -> Option<String> {
    let mut out = String::new();
    for (index, phase) in CLIENT_TOC.iter().enumerate() {
        let Some(n) = values[index] else { continue };
        if !out.is_empty() {
            out.push(',');
        }
        let _ = write!(out, "{}:{n}", client_set_name(phase));
    }
    (!out.is_empty()).then_some(out)
}

fn sum_named(values: &[Option<u64>; CLIENT_TOC.len()]) -> Option<u64> {
    let mut sum = 0u64;
    let mut any = false;
    for slot in values {
        let Some(n) = slot else { continue };
        sum += *n;
        any = true;
    }
    any.then_some(sum)
}

fn stamp_client_edge<const EDGE: u8>(mut census: ResMut<ClientPhaseCensus>) {
    let alloc = diag::process_allocations();
    if EDGE > 0 {
        let slot = EDGE as usize - 1;
        client_toc_span(slot).end();
        if let Some(n0) = census.seam_n {
            census.n[slot] = Some(alloc.process_allocations.saturating_sub(n0));
        }
        if let Some(b0) = census.seam_bytes {
            census.bytes[slot] = Some(alloc.process_allocation_bytes.saturating_sub(b0));
        }
    }
    if EDGE == 0 {
        perf::Span::FramesUpdateMs.begin();
    }
    if (EDGE as usize) < CLIENT_TOC.len() {
        client_toc_span(EDGE as usize).begin();
    }
    if EDGE as usize == CLIENT_TOC.len() {
        perf::Span::FramesUpdateMs.end();
    }
    census.seam_n = Some(alloc.process_allocations);
    census.seam_bytes = Some(alloc.process_allocation_bytes);
}

fn client_toc_span(slot: usize) -> perf::Span {
    match slot {
        0 => perf::Span::TocLoad,
        1 => perf::Span::TocReceive,
        2 => perf::Span::TocReconcile,
        3 => perf::Span::TocInput,
        4 => perf::Span::FramesPredictMs,
        5 => perf::Span::TocSend,
        6 => perf::Span::FramesPresentMs,
        7 => perf::Span::TocUi,
        8 => perf::Span::FramesEffectsMs,
        9 => perf::Span::FramesDiagMs,
        _ => unreachable!("CLIENT_TOC slot {slot}"),
    }
}

fn register_phase_census(app: &mut App) {
    app.init_resource::<ClientPhaseCensus>();
    macro_rules! seams {
        ($($edge:literal),* $(,)?) => {$(
            app.add_systems(
                Update,
                stamp_client_edge::<$edge>.in_set(ClientEdge($edge)),
            );
        )*};
    }
    seams!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
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
    emit(perf::Counter::PresentPublishMs, census.publish_presented_ms);
    emit(perf::Counter::HudSurfacesScheduleMs, census.hud_surfaces_ms);
    emit(perf::Counter::HudTessBodyMs, census.hud_tess_body_ms);
    if let Some(jobs) = census.hud_tess_jobs {
        perf::Counter::HudTessJobs.emit(f64::from(jobs));
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
        perf::Counter::HudStageMaxScheduleMs.emit(f64::from(ms));
        perf::Counter::HudStageMaxScheduleAt.emit(at as f64);
    }
    emit(perf::Counter::UiHudSetupMs, census.hud_root_setup_ms);
    emit(
        perf::Counter::UiApplyDeferredMs,
        census.present_apply_deferred_ms,
    );
    emit(perf::Counter::UiHudVisibilityMs, census.hud_visibility_ms);
}

pub fn register_update_phase_census(app: &mut App) {
    register_phase_census(app);
    app.init_resource::<UpdatePhaseCensus>().add_systems(
        Update,
        (
            begin_update_census.before(ClientSet::Load),
            begin_present_census.in_set(ClientSet::Send),
            // After both phases it measures — `Ui` runs after `Present`, and
            // `Diag` after `Ui` — and before the next frame's reset.
            publish_present_census.in_set(ClientSet::Diag),
        ),
    );
}
