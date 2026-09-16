//! Section two's second half: the frames themselves, not their histogram.
//!
//! Everything above this reads [`perf::stats`], which keeps distributions and
//! throws the rows away. Three questions need the rows:
//!
//! * **did it settle?** The first seconds after a map appears are pipeline
//!   compilation, streaming and a cold cache; folding them into one average
//!   with the steady state reports a number no part of the run ever saw.
//! * **which frames were the worst, and what was different about them?** A p99
//!   names a height, not a frame. This names the frames, what was flagged in
//!   them, and which span was largest in each — a candidate, not a verdict.
//! * **how much of the frame did nothing claim?** Wall minus the *union* of
//!   the top-level spans is the time the recorder could not attribute to
//!   anything, and it is the honest measure of how far the instrumentation
//!   reaches. A union rather than a sum, because two schedules running at once
//!   on two threads cover the wall once between them — summing them is what
//!   used to drive the remainder negative and have it clamped to zero.

use perf::frames::{FrameRow, flag};
use perf::{Span, stats};

use crate::bench::table::{Align, Table, ms};

/// Where the steady state is taken to begin, after the first playable frame.
const SETTLE_NS: u64 = 3_000_000_000;

/// Frames named individually in the worst-frames table.
const WORST: usize = 20;

pub(crate) fn render(out: &mut Vec<String>) {
    let frames = perf::frames::snapshot();
    if frames.rows.is_empty() {
        out.push(String::new());
        out.push(
            "  per-frame rows: MISS — the recorder kept none (IW4L_BENCH_FRAMES=0, or the run closed no frame)."
                .to_owned(),
        );
        return;
    }
    let live: Vec<&FrameRow> = frames.rows.iter().filter(|row| row.phase == 1).collect();
    out.push(String::new());
    out.push(format!(
        "  per-frame rows: {} kept ({} live, {} loading){}",
        frames.rows.len(),
        live.len(),
        frames.rows.len() - live.len(),
        if frames.dropped == 0 {
            String::new()
        } else {
            format!(
                ", {} dropped after the table's {} filled — every number below is the kept prefix, not the run",
                frames.dropped, frames.capacity
            )
        },
    ));
    if live.is_empty() {
        out.push("  no live frame was closed, so there is no steady state to read.".to_owned());
        return;
    }
    settle(&live, out);
    worst(&live, out);
    unclassified(&live, out);
    overlap(&live, out);
    nesting(&live, out);
}

/// How much of the frame the render thread and a main schedule were both
/// inside.
///
/// Read off the intervals, not off which frame a render stage says it came
/// from: a stage that names its origin frame says where its work belongs, not
/// that the work ran inside that frame's wall. Serialised rendering leaves
/// this at zero whatever else is true, so it is the line that says whether a
/// pipelined run actually overlapped anything — and it is not a saving on its
/// own, because both halves are competing for the same cores.
fn overlap(live: &[&FrameRow], out: &mut Vec<String>) {
    let render: u64 = live
        .iter()
        .map(|row| row.spans_ns[Span::RenderRenderThreadMs as usize])
        .sum();
    if render == 0 {
        return;
    }
    let both: u64 = live
        .iter()
        .map(|row| {
            row.main_covered_ns
                .saturating_add(row.spans_ns[Span::RenderRenderThreadMs as usize])
                .saturating_sub(row.covered_ns)
        })
        .sum();
    let frames = live.len() as u64;
    let overlapped = live
        .iter()
        .filter(|row| {
            row.main_covered_ns
                .saturating_add(row.spans_ns[Span::RenderRenderThreadMs as usize])
                > row.covered_ns
        })
        .count();
    out.push(String::new());
    out.push(format!(
        "  main/render overlap: {} per frame on average, in {overlapped} of {frames} frames — the part of the wall a main schedule and the render thread were both inside. The render thread itself is {} per frame.",
        ms(both / frames),
        ms(render / frames),
    ));
    out.push(
        "  measured from the intervals, not from the frame a render stage names as its origin: the second says where the work belongs, not that it ran inside that frame's wall. Serialised rendering reads zero here whatever else is true."
            .to_owned(),
    );
    presented_age(out);
}

/// How old the state on the screen was, as far as the process can see it.
///
/// The other half of the pipelining pair. Overlap is what it buys; this is
/// what it costs: the render world presents a frame it extracted while the
/// main world was somewhere earlier, and the image that reaches the surface
/// was built from that older state. Read at the render graph's Finish set,
/// the last point in the schedule that still belongs to this image: bevy
/// presents after the whole graph schedule returns, so the present itself is
/// outside this measurement and so is everything after it — the compositor,
/// the queue behind it and the panel. It is not input-to-photon, and
/// `desired_maximum_frame_latency` is a hint the backend may clamp, so how
/// many frames the GPU is really allowed in flight is not in here either.
fn presented_age(out: &mut Vec<String>) {
    let stats = perf::stats::counter_snapshot(perf::Phase::Live);
    let find = |counter: perf::Counter| {
        stats
            .iter()
            .find(|row| row.counter == counter)
            .filter(|row| row.samples > 0)
    };
    let (Some(age), Some(behind)) = (
        find(perf::Counter::RenderPresentedStateAgeMs),
        find(perf::Counter::RenderPresentedFramesBehind),
    ) else {
        return;
    };
    out.push(format!(
        "  presented state age: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms from extract to the end of the render graph, {:.2} main frames behind on average (p99 {:.0}), over {} presented frames.",
        age.p50,
        age.p99,
        age.max,
        behind.avg(),
        behind.p99,
        age.samples,
    ));
    out.push(
        "  it ends where the render graph does, one step before bevy presents: the present call itself, the compositor and the display are all outside it, so this is the age of the state the graph finished with and never an input-to-photon latency."
            .to_owned(),
    );
}

/// The first seconds against the rest. Exact, not histogram midpoints: these
/// are the rows.
fn settle(live: &[&FrameRow], out: &mut Vec<String>) {
    let origin = live.first().map_or(0, |row| row.start_ns);
    let cut = origin.saturating_add(SETTLE_NS);
    let (first, steady): (Vec<&&FrameRow>, Vec<&&FrameRow>) =
        live.iter().partition(|row| row.start_ns < cut);
    out.push(format!(
        "  settling — the first {:.0}s of gameplay against everything after it:",
        SETTLE_NS as f64 / 1e9
    ));
    let mut table = Table::new(
        4,
        &[
            ("window", Align::Left),
            ("frames", Align::Right),
            ("mean", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
        ],
    );
    for (label, rows) in [("first seconds", &first), ("steady state", &steady)] {
        let mut walls: Vec<u64> = rows.iter().map(|row| row.wall_ns).collect();
        if walls.is_empty() {
            table.row([
                label.to_owned(),
                "0".to_owned(),
                "MISS".to_owned(),
                "MISS".to_owned(),
                "MISS".to_owned(),
                "MISS".to_owned(),
                "MISS".to_owned(),
            ]);
            continue;
        }
        walls.sort_unstable();
        let sum: u64 = walls.iter().sum();
        table.row([
            label.to_owned(),
            walls.len().to_string(),
            ms(sum / walls.len() as u64),
            ms(quantile(&walls, 0.50)),
            ms(quantile(&walls, 0.95)),
            ms(quantile(&walls, 0.99)),
            ms(*walls.last().unwrap_or(&0)),
        ]);
    }
    table.render(out);
    out.push(
        "  these percentiles are exact — they are read off the sorted frames, not off a histogram."
            .to_owned(),
    );
}

/// The `q`th quantile of a sorted slice, by nearest rank.
fn quantile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((sorted.len() as f64) * q).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// The worst frames, each with what was flagged and its largest span.
///
/// The largest span is a *candidate*: it is the biggest thing the recorder saw
/// inside that frame, which is not the same as the cause, and on a frame whose
/// cost is outside every span it will name something small and innocent. The
/// `unclassified` column is what says which of those two happened.
fn worst(live: &[&FrameRow], out: &mut Vec<String>) {
    let mut ranked: Vec<&&FrameRow> = live.iter().collect();
    ranked.sort_by_key(|row| std::cmp::Reverse(row.wall_ns));
    out.push(String::new());
    out.push(format!("  worst {WORST} frames:"));
    let mut table = Table::new(
        4,
        &[
            ("frame", Align::Right),
            ("at", Align::Right),
            ("wall", Align::Right),
            ("unclassified", Align::Right),
            ("carried in", Align::Right),
            ("largest span", Align::Left),
            ("of it", Align::Right),
            ("state", Align::Left),
            ("draws", Align::Right),
        ],
    );
    let origin = live.first().map_or(0, |row| row.start_ns);
    for row in ranked.into_iter().take(WORST) {
        let largest = Span::ALL
            .into_iter()
            .filter(|span| *span != Span::FramesWallFrameMs)
            .max_by_key(|span| row.spans_ns[*span as usize]);
        let (name, span_ns) = match largest {
            Some(span) => (span.name(), row.spans_ns[span as usize]),
            None => ("—", 0),
        };
        table.row([
            row.index.to_string(),
            format!("{:.2}s", (row.start_ns.saturating_sub(origin)) as f64 / 1e9),
            ms(row.wall_ns),
            ms(row.wall_ns.saturating_sub(row.covered_ns)),
            if row.carried_spans == 0 {
                "—".to_owned()
            } else {
                format!("{} in {}", ms(row.carried_in_ns), row.carried_spans)
            },
            name.to_owned(),
            ms(span_ns),
            state(row),
            counter_cell(row, perf::Counter::CounterDraws),
        ]);
    }
    table.render(out);
    out.push(
        "  `largest span` is the biggest span the recorder saw inside that frame. It is a candidate and not a cause — on a frame whose cost is outside every span it names something small, and `unclassified` is what tells those apart."
            .to_owned(),
    );
    out.push(
        "  every span column is the part of the span that ran inside this frame's wall, so none of them can be larger than the frame. `carried in` is what those spans had already run before it opened: a background task that finished here and started three frames ago shows its tail in the span column and the rest of itself in this one."
            .to_owned(),
    );
}

fn counter_cell(row: &FrameRow, counter: perf::Counter) -> String {
    if row.counter_samples[counter as usize] == 0 {
        return "MISS".to_owned();
    }
    row.counters[counter as usize].to_string()
}

fn state(row: &FrameRow) -> String {
    let names: Vec<&str> = flag::ALL
        .iter()
        .filter(|(bit, _)| row.flags & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if names.is_empty() {
        "—".to_owned()
    } else {
        names.join("|")
    }
}

/// Wall minus the union of the root spans: time inside the frame that no span
/// was open for.
fn unclassified(live: &[&FrameRow], out: &mut Vec<String>) {
    let roots: Vec<Span> = stats::snapshot(perf::Phase::Live)
        .into_iter()
        .filter(|row| row.span.coverage_root())
        .map(|row| row.span)
        .collect();
    let total: u64 = live
        .iter()
        .map(|row| row.wall_ns.saturating_sub(row.covered_ns))
        .sum();
    let carried: u64 = live.iter().map(|row| row.carried_in_ns).sum();
    let truncated = live.iter().filter(|row| row.coverage_overflow).count();
    out.push(String::new());
    out.push(format!(
        "  unclassified: {} per frame on average — frame wall minus the union of the intervals the frame's declared top-level spans were open for ({}).",
        ms(total / live.len() as u64),
        if roots.is_empty() {
            "none took a sample".to_owned()
        } else {
            roots
                .iter()
                .map(|span| span.name())
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    out.push(
        "  a union, so two roots running at once on two threads cover the wall once between them. The remainder cannot go negative and is never clamped."
            .to_owned(),
    );
    if carried > 0 {
        out.push(format!(
            "  {} of span time closed inside these frames but ran before they opened, and is in none of the numbers above — it belongs to the frames it actually ran in. The span histograms carry each span's whole elapsed time.",
            ms(carried / live.len() as u64),
        ));
    }
    if truncated > 0 {
        out.push(format!(
            "  {truncated} of {} frames had more root intervals than the coverage set holds, so their unclassified time is an upper bound.",
            live.len()
        ));
    }
}

/// Rows where a span outlasted the span the recorder says contains it.
///
/// With per-frame intersection and per-thread nesting this should be empty. It
/// is printed rather than asserted because a non-zero count is the finding: it
/// says the parent column is describing something other than containment, and
/// every self-time number derived from it is suspect.
fn nesting(live: &[&FrameRow], out: &mut Vec<String>) {
    let parents: Vec<(Span, Span)> = stats::snapshot(perf::Phase::Live)
        .into_iter()
        .filter(|row| !row.parent_varies)
        .filter_map(|row| row.parent.map(|parent| (row.span, parent)))
        .collect();
    if parents.is_empty() {
        return;
    }
    let mut rows = 0usize;
    let mut worst: Option<(u64, Span, Span, u64)> = None;
    for row in live {
        let mut broke = false;
        for (child, parent) in &parents {
            let (inside, outside) = (
                row.spans_ns[*child as usize],
                row.spans_ns[*parent as usize],
            );
            if inside > outside {
                broke = true;
                let over = inside - outside;
                if worst.is_none_or(|(known, ..)| over > known) {
                    worst = Some((over, *child, *parent, row.index));
                }
            }
        }
        if broke {
            rows += 1;
        }
    }
    out.push(String::new());
    if rows == 0 {
        out.push(format!(
            "  nesting: no frame of {} has a span longer than the span that contains it.",
            live.len()
        ));
        return;
    }
    let detail = worst.map_or_else(String::new, |(over, child, parent, frame)| {
        format!(
            " Worst is {} over {} inside {} on frame {frame}.",
            ms(over),
            child.name(),
            parent.name(),
        )
    });
    out.push(format!(
        "  nesting: {rows} of {} frames have a span longer than the span the recorder calls its parent, so that parent is not containing it.{detail}",
        live.len()
    ));
}
