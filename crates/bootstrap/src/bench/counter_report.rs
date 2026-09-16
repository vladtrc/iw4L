//! Section three: the counters, which until now only ever reached a trace.
//!
//! `Counter::emit` writes a Perfetto counter track, and the bench report is a
//! different tool over the same run — so every render-stage timing, every GPU
//! pass and every draw census was invisible to `make bench` unless someone
//! opened the `.pftrace`. The recorder now keeps them, and this is where they
//! are read back.
//!
//! Three rules the tables here exist to keep:
//!
//! * a counter that took no sample prints MISS, never `0.00`. A pass the GPU
//!   could not time and a pass that cost nothing are not the same fact;
//! * a sample is not a frame. `submit_prepare` fires once per submit and
//!   `fx_update` twice per frame, so `avg` is the cost of one *call* and
//!   `per frame` is the cost of the calls one frame made. The two columns are
//!   printed side by side because comparing the wrong one to a frame budget is
//!   the easiest mistake this report can invite;
//! * GPU passes are not added up. They overlap on the device and the driver
//!   resolves them some frames after the CPU work that queued them, so the
//!   measured `gpu_frame` interval is printed as its own row rather than as a
//!   total of the rows above it.

use perf::{Counter, CounterStats, Origin, Phase, SpanStats, Unit};

use crate::bench::table::{Align, Table};

/// CPU stage timings, in the order the work happens rather than by size: a
/// reader is looking for where a stage grew, and the sequence is the context.
const CPU_MS: [Counter; 7] = [
    Counter::RenderSubmitPrepareMs,
    Counter::RenderSubmitGatherMs,
    Counter::RenderSubmitArenaMs,
    Counter::RenderSubmitRecordMs,
    Counter::RenderSubmitSunMs,
    Counter::RenderGraphRenderMs,
    Counter::RenderGraphPresentMs,
];

/// GPU pass timings. `gpu_frame` is last and is the measured interval, not the
/// sum of the passes before it.
const GPU_MS: [Counter; 6] = [
    Counter::RenderGpuColourMs,
    Counter::RenderGpuSunMs,
    Counter::RenderGpuSpotMs,
    Counter::RenderGpuFloatzMs,
    Counter::RenderGpuPostfxMs,
    Counter::RenderGpuFrameMs,
];

/// Inside `Present` and `Ui`, whose span self-time is most of what the frame
/// report cannot name. Two phases in one table because they are adjacent and a
/// reader chasing the frame reads them together; the `present_`/`ui_` prefix
/// says which is which.
const PHASE_MS: [Counter; 5] = [
    Counter::PresentPublishMs,
    Counter::HudTessBodyMs,
    Counter::UiHudSetupMs,
    Counter::UiApplyDeferredMs,
    Counter::UiHudVisibilityMs,
];

/// Gaps *between* systems, kept in their own table so nothing adds them to the
/// bodies above.
///
/// `.chain()` fixes the order of the HUD systems and promises nothing about
/// what the executor runs in the gaps between them. A wide gap therefore says
/// the schedule put something there — it is not evidence that the HUD system
/// on either side of it was slow, and reading it as one is how an earlier
/// iteration set out to rewrite a HUD whose bodies were short.
const SCHEDULE_MS: [Counter; 3] = [
    Counter::HudSurfacesScheduleMs,
    Counter::HudStageMaxScheduleMs,
    Counter::RenderGraphSubmitIntervalMs,
];

/// Per-frame work: what the CPU asked the GPU to do, and what it allocated
/// doing it.
const WORK: [Counter; 14] = [
    Counter::HudTessJobs,
    Counter::RenderGraphSubmitPendingN,
    Counter::CounterOverlayConstWrites,
    Counter::CounterDraws,
    Counter::CounterDipsColour,
    Counter::CounterDipsSun,
    Counter::CounterSubmittedBatches,
    Counter::CounterMultiDraws,
    Counter::CounterMultiDrawCommands,
    Counter::CounterBindGroup0,
    Counter::CounterBindGroup1,
    Counter::CounterCmdState,
    Counter::CounterFxElemLive,
    Counter::CounterProcessAllocations,
];

pub(crate) fn render(wall: &SpanStats, out: &mut Vec<String>) {
    let stats = perf::stats::counter_snapshot(Phase::Live);
    let frames = wall.count;
    out.push("[3/3] COUNTERS".to_owned());
    if stats.is_empty() {
        out.push(
            "  MISS — no counter took a sample. Nothing emitted one on this workload, or the census that feeds them was compiled out."
                .to_owned(),
        );
        return;
    }
    out.push(format!(
        "  {} frames in the window. `n` is samples, not frames: `n/frame` says how often the counter fired, `avg` is one firing, `per frame` is what one frame paid.",
        frames
    ));

    out.push(String::new());
    out.push("  render stages, CPU (ms)".to_owned());
    ms_table(&stats, &CPU_MS, wall, frames, Share::Frame, out);
    out.push(
        "    Stage timings from different points of the render schedule, in the order the work happens. Some of them run inside others, so this column is not a budget and does not add up to a frame."
            .to_owned(),
    );

    age_table(&stats, frames, out);

    out.push(String::new());
    out.push("  inside Present and Ui, CPU (ms)".to_owned());
    ms_table(&stats, &PHASE_MS, wall, frames, Share::Frame, out);
    out.push(
        "    Bodies: each one was taken inside the functions it names, so it is what that work cost. `hud_tess_body` is the nine HUD tess flush systems' own bodies summed over the frame, and `hud_tess_jobs` in the work table below is how many tess jobs they applied."
            .to_owned(),
    );

    out.push(String::new());
    out.push("  schedule gaps, wall (ms)".to_owned());
    ms_table(&stats, &SCHEDULE_MS, wall, frames, Share::Frame, out);
    hud_stage(&stats, out);
    out.push(
        "    `graph_submit_schedule_interval` is the wall from the end of the render graph's Render set to the start of its Finish set. Inside it are two of bevy's own exclusive systems — `submit_pending_command_buffers`, which finishes every pending encoder and then calls `Queue::submit`, and `handle_uncovered_swap_chains` — and whatever else the executor put there. It is not a `Queue::submit` body; `graph_submit_pending` in the work table is how many buffers and unfinished encoders it was handed."
            .to_owned(),
    );
    out.push(
        "    and the count does not say where the time went: it sums finished buffers and unfinished encoders, and says nothing about how many commands or resources each carries, what `finish` costs, what locks or callbacks the driver serviced, or whether the thread was descheduled. The span that would answer it is bevy's own `queue_submit`, around `RenderQueue::submit` and after the buffers have been taken; this capture does not contain it. A build with the `bevy-trace` feature records it, and `scheduling.bevy_tracing` in the manifest says whether this run was one — a run where it reads false cannot have measured a submit body whatever this interval shows."
            .to_owned(),
    );

    out.push(String::new());
    out.push(
        "  GPU passes (ms) — measured on the device, resolved some frames after the CPU work that queued them. They overlap, so they are not summed; `gpu_frame` is the measured interval."
            .to_owned(),
    );
    ms_table(&stats, &GPU_MS, wall, frames, Share::Wall, out);
    out.push(
        "    `%wall` is device-busy time against the CPU wall clock of the same window, not a share of any one frame: the GPU runs ahead of and behind the CPU that queued it."
            .to_owned(),
    );

    out.push(String::new());
    out.push("  work per frame".to_owned());
    count_table(&stats, &WORK, frames, out);
    if find(&stats, Counter::CounterProcessAllocations).is_none() {
        out.push(
            "    process_allocations is MISS because the counting allocator is off; set IW4L_COUNTING_ALLOC to turn it on. It is not zero — nobody counted."
                .to_owned(),
        );
    }

    rejected(&stats, out);
    untabled(&stats, out);
    never_sampled(out);
}

/// How old the state on the surface was, in its own table because every column
/// the other tables carry would be a lie here.
///
/// `presented_state_age` is when the work happened, not how long anything
/// took, so it has no `per frame` and no `%frame`: an age divided by a frame
/// is not a share of one, and among the render stages it reads as one — which
/// is the input-to-photon reading the frame section forbids.
/// `presented_frames_behind` is the same quantity counted in frames, so it
/// belongs beside it rather than in the work table, where its `total` would be
/// a sum of the number one.
///
/// Silent when the counter never fired at all, which is a run that presented
/// nothing rather than a run that presented in line with the main world: a
/// serialised run still has an age here, it just answers zero frames behind.
/// A MISS row would read as a measurement that came back empty rather than as
/// a question the run never asked.
fn age_table(stats: &[CounterStats], frames: u64, out: &mut Vec<String>) {
    let Some(age) = find(stats, Counter::RenderPresentedStateAgeMs) else {
        return;
    };
    out.push(String::new());
    out.push("  presented state age (ms) — when, not how long".to_owned());
    let mut table = Table::new(
        4,
        &[
            ("counter", Align::Left),
            ("n", Align::Right),
            ("n/frame", Align::Right),
            ("avg", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
        ],
    );
    debug_assert_eq!(age.unit, Unit::Milliseconds);
    table.row([
        age.counter.name().to_owned(),
        age.samples.to_string(),
        opt2(age.per_frame_samples(frames)),
        num2(age.avg()),
        num2(age.p50),
        num2(age.p95),
        num2(age.p99),
        num2(age.max),
    ]);
    table.render(out);
    out.push(
        "    the wall from this frame's extract — the main world stalled, so nothing moved across it — to the render graph's Finish set. It is not input-to-photon: the compositor, the queue behind it and the panel are all outside this process, and `desired_maximum_frame_latency` is a hint the backend may clamp, so how many frames the GPU is really allowed in flight is not in here either."
            .to_owned(),
    );
    match find(stats, Counter::RenderPresentedFramesBehind) {
        Some(behind) => out.push(format!(
            "    `presented_frames_behind` on the same frames: p50 {:.0}, max {:.0}. It is the age in frames rather than in milliseconds — zero is a render world running in line with the main one, and one or more is a pipelined render world presenting an older state. Read it as the arm the run took, not as a distribution: with pipelining on it is the same number nearly every frame.",
            behind.p50, behind.max,
        )),
        None => out.push(
            "    `presented_frames_behind` MISS — nobody counted, which is not the same as zero frames behind."
                .to_owned(),
        ),
    }
}

/// Which HUD slice was the largest, not only how large. An index is not a
/// duration and has no business in a millisecond table, so it gets a sentence:
/// the reader who has just seen a HUD stage take a third of the frame should
/// not then have to bisect the chain to find out which one.
fn hud_stage(stats: &[CounterStats], out: &mut Vec<String>) {
    out.push(
        "    These are wall intervals between two systems, not the cost of either. Whatever the executor ran in the gap is in them; compare them against `hud_tess_body` above rather than adding the two, and do not subtract either from a span."
            .to_owned(),
    );
    let Some(at) = find(stats, Counter::HudStageMaxScheduleAt) else {
        return;
    };
    out.push(format!(
        "    `hud_stage_max_schedule_interval` is the largest of the nine gaps inside `hud_surfaces_schedule_interval`, not a phase beside it. It was slice {:.0} on the median frame (min {:.0}, max {:.0}) — the wall between `hud_stage_close::<{:.0}>` and the close before it, in crates/hud/src/plugin.rs.",
        at.p50,
        at.min,
        at.max,
        at.p50,
    ));
}

/// Counters that took a sample and are in none of the three tables above. The
/// tables are a curated set — the frame's cost and the frame's work — while
/// `summary.json` carries every counter the recorder held. Naming the rest is
/// what stops the report reading as the whole list when it is a selection.
fn untabled(stats: &[CounterStats], out: &mut Vec<String>) {
    let tabled: Vec<Counter> = CPU_MS
        .into_iter()
        .chain([
            Counter::RenderPresentedStateAgeMs,
            Counter::RenderPresentedFramesBehind,
        ])
        .chain(PHASE_MS)
        .chain(SCHEDULE_MS)
        .chain(GPU_MS)
        .chain(WORK)
        .chain([Counter::HudStageMaxScheduleAt])
        .collect();
    let rest: Vec<&str> = stats
        .iter()
        .filter(|row| !tabled.contains(&row.counter))
        .map(|row| row.counter.name())
        .collect();
    if rest.is_empty() {
        return;
    }
    out.push(String::new());
    out.push(format!(
        "  sampled but not tabled above ({}), in summary.json with the rest: {}",
        rest.len(),
        rest.join(", ")
    ));
}

/// What the last column of a millisecond table divides by, and what it is
/// therefore allowed to be called. A CPU stage is part of the frame it was
/// measured in; a GPU pass is device time against the same wall clock and is
/// not a share of any one frame, so it gets its own name rather than borrowing
/// one that would read as a frame budget.
#[derive(Clone, Copy)]
enum Share {
    Frame,
    Wall,
}

impl Share {
    const fn header(self) -> &'static str {
        match self {
            Self::Frame => "%frame",
            Self::Wall => "%wall",
        }
    }
}

fn ms_table(
    stats: &[CounterStats],
    wanted: &[Counter],
    wall: &SpanStats,
    frames: u64,
    share: Share,
    out: &mut Vec<String>,
) {
    let wall_ms = wall.sum_ns as f64 / 1e6;
    let mut table = Table::new(
        4,
        &[
            ("counter", Align::Left),
            ("n", Align::Right),
            ("n/frame", Align::Right),
            ("avg", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
            ("per frame", Align::Right),
            (share.header(), Align::Right),
            ("where", Align::Left),
        ],
    );
    for counter in wanted {
        let Some(row) = find(stats, *counter) else {
            table.miss(counter.name());
            continue;
        };
        debug_assert_eq!(row.unit, Unit::Milliseconds);
        let per_frame = row.per_frame(frames);
        table.row([
            counter.name().to_owned(),
            row.samples.to_string(),
            opt2(row.per_frame_samples(frames)),
            num2(row.avg()),
            num2(row.p50),
            num2(row.p95),
            num2(row.p99),
            num2(row.max),
            opt2(per_frame),
            per_frame.map_or_else(
                || "—".to_owned(),
                |per_frame| {
                    if wall_ms <= 0.0 {
                        "—".to_owned()
                    } else {
                        format!("{:.1}", per_frame * 100.0 * frames as f64 / wall_ms)
                    }
                },
            ),
            match row.origin {
                Origin::Cpu => "cpu",
                Origin::Gpu => "gpu",
            }
            .to_owned(),
        ]);
    }
    table.render(out);
}

fn count_table(stats: &[CounterStats], wanted: &[Counter], frames: u64, out: &mut Vec<String>) {
    let mut table = Table::new(
        4,
        &[
            ("counter", Align::Left),
            ("n", Align::Right),
            ("n/frame", Align::Right),
            ("avg", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
            ("per frame", Align::Right),
            ("total", Align::Right),
        ],
    );
    for counter in wanted {
        let Some(row) = find(stats, *counter) else {
            table.miss(counter.name());
            continue;
        };
        debug_assert_eq!(row.unit, Unit::Count);
        table.row([
            counter.name().to_owned(),
            row.samples.to_string(),
            opt2(row.per_frame_samples(frames)),
            num2(row.avg()),
            num0(row.p50),
            num0(row.p95),
            num0(row.p99),
            num0(row.max),
            opt2(row.per_frame(frames)),
            num0(row.sum),
        ]);
    }
    table.render(out);
}

/// Samples the recorder refused: not finite, or negative. A counter is a
/// non-negative quantity, so one of these is a bug at the emitter and the
/// report says so rather than folding it into an average.
fn rejected(stats: &[CounterStats], out: &mut Vec<String>) {
    let bad: Vec<&CounterStats> = stats.iter().filter(|row| row.rejected > 0).collect();
    if bad.is_empty() {
        return;
    }
    out.push(String::new());
    for row in bad {
        out.push(format!(
            "  {}: {} sample(s) were NaN, infinite or negative and are in none of the numbers above.",
            row.counter.name(),
            row.rejected
        ));
    }
}

/// Counters this build declares that took no sample in either phase. Printing
/// the names is the difference between "this workload never did that" and
/// "nothing emits this any more", which the reader can tell apart and the
/// report cannot.
fn never_sampled(out: &mut Vec<String>) {
    let silent = perf::stats::counters_never_sampled();
    if silent.is_empty() {
        return;
    }
    let names: Vec<&str> = silent.iter().map(|counter| counter.name()).collect();
    out.push(String::new());
    out.push(format!(
        "  declared but never sampled on this run ({}): {}",
        silent.len(),
        names.join(", ")
    ));
}

fn find(stats: &[CounterStats], counter: Counter) -> Option<&CounterStats> {
    stats.iter().find(|row| row.counter == counter)
}

fn num2(value: f64) -> String {
    format!("{value:.2}")
}

fn num0(value: f64) -> String {
    format!("{value:.0}")
}

fn opt2(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}"))
}
