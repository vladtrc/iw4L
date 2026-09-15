//! Section two: where a gameplay frame goes.
//!
//! The tree is not a table someone wrote down: a span's parent is whichever
//! span was open when it began, recorded by the same instrumentation that
//! timed it. Move a system between sets and this report follows; the only
//! declared relation is that `wall` is the frame clock, which encloses no
//! scope because it is opened in one frame and closed in the next.

use perf::{Phase, Span, SpanStats};

use crate::bench::table::{Align, Table, ms, percent};

/// A span worth naming: either its own time is large, or it is a big share of
/// a frame that nothing else explains.
const HOT_SELF_NS: u64 = 500_000;
const HOT_SHARE: f64 = 5.0;

/// How many rows the self-time ranking prints before it stops.
const TOP_SELF: usize = 12;

pub(crate) fn render(out: &mut Vec<String>) {
    let stats = perf::stats::snapshot(Phase::Live);
    out.push("[2/3] FRAME TIME".to_owned());
    let Some(wall) = stats
        .iter()
        .find(|row| row.span == Span::FramesWallFrameMs)
        .copied()
    else {
        out.push(
            "  MISS — no gameplay frame was clocked. The run never reached a playable map, or it quit at the first one."
                .to_owned(),
        );
        return;
    };
    header(&wall, out);
    out.push(String::new());
    tree(&stats, &wall, out);
    out.push(String::new());
    top_self(&stats, &wall, out);
    silent_spans(out);
    anomalies(out);
    out.push(String::new());
    crate::bench::counter_report::render(&wall, out);
}

fn header(wall: &SpanStats, out: &mut Vec<String>) {
    let seconds = wall.sum_ns as f64 / 1e9;
    let fps = |ns: u64| {
        if ns == 0 {
            "—".to_owned()
        } else {
            format!("{:.0}", 1e9 / ns as f64)
        }
    };
    out.push(format!(
        "  {} frames over {seconds:.2}s of gameplay, {} fps average",
        wall.count,
        fps(wall.avg_ns()),
    ));
    out.push(format!(
        "  frame ms   avg {}  p50 {}  p95 {}  p99 {}  max {}  min {}",
        ms(wall.avg_ns()),
        ms(wall.p50_ns),
        ms(wall.p95_ns),
        ms(wall.p99_ns),
        ms(wall.max_ns),
        ms(wall.min_ns),
    ));
    out.push(format!(
        "  same as fps  avg {}  p50 {}  p95 {}  p99 {}  min {}   (a slow frame is a low fps: the columns line up with the row above)",
        fps(wall.avg_ns()),
        fps(wall.p50_ns),
        fps(wall.p95_ns),
        fps(wall.p99_ns),
        fps(wall.max_ns),
    ));
    out.push(format!(
        "  percentiles are histogram midpoints, exact to ±{:.1}%; counts, sums and extremes are exact.",
        perf::stats::PRECISION * 100.0
    ));
}

fn tree(stats: &[SpanStats], wall: &SpanStats, out: &mut Vec<String>) {
    let mut table = Table::new(
        2,
        &[
            ("span", Align::Left),
            ("n", Align::Right),
            ("n/frame", Align::Right),
            ("avg", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
            ("per frame", Align::Right),
            ("total", Align::Right),
            ("%frame", Align::Right),
            ("self", Align::Right),
        ],
    );
    let mut placed: Vec<Span> = Vec::with_capacity(stats.len());
    push(&mut table, wall, wall, 0, self_ns(stats, wall, None));
    walk(stats, wall, None, 1, &mut table, &mut placed);

    // A span whose recorded parent is not itself reachable from the root — a
    // parent that took no sample in this window, or two spans that raced and
    // each recorded the other. Dropping them silently would hide numbers that
    // were measured, so they are shown, under their own heading, with the
    // parent each of them recorded.
    let mut orphans: Vec<&SpanStats> = stats
        .iter()
        .filter(|row| row.span != Span::FramesWallFrameMs && !placed.contains(&row.span))
        .collect();
    table.render(out);
    out.push(
        "  `self` is the span's own time: its total minus the totals of the spans that nested inside it."
            .to_owned(),
    );
    out.push(
        "  `avg` is one call; `per frame` is what one frame paid for all of them. A span that runs twice a frame has an `avg` half its `per frame`, and only `per frame` belongs next to a frame budget."
            .to_owned(),
    );
    if orphans.is_empty() {
        return;
    }
    orphans.sort_by_key(|row| std::cmp::Reverse(row.sum_ns));
    out.push(String::new());
    out.push(
        "  not placed in the tree — the parent each recorded is not itself reachable from the frame, so their nesting is unknown:"
            .to_owned(),
    );
    let mut table = Table::new(
        2,
        &[
            ("span", Align::Left),
            ("n", Align::Right),
            ("avg", Align::Right),
            ("p95", Align::Right),
            ("total", Align::Right),
            ("%frame", Align::Right),
            ("recorded under", Align::Left),
        ],
    );
    for row in orphans {
        table.row([
            format!(
                "{}{}",
                row.span.name(),
                if row.parent_varies { " *" } else { "" }
            ),
            row.count.to_string(),
            ms(row.avg_ns()),
            ms(row.p95_ns),
            ms(row.sum_ns),
            percent(row.sum_ns, wall.sum_ns),
            row.parent
                .map_or_else(|| "—".to_owned(), |parent| parent.name().to_owned()),
        ]);
    }
    table.render(out);
}

/// Children of `parent`, heaviest first, then their children. `wall` is the
/// root of the printed tree but the parent of nothing, so the spans with no
/// recorded parent hang directly under it. `placed` is both the record of what
/// has been printed and the guard that keeps a parent cycle from recursing
/// forever.
fn walk(
    stats: &[SpanStats],
    wall: &SpanStats,
    parent: Option<Span>,
    depth: usize,
    table: &mut Table,
    placed: &mut Vec<Span>,
) {
    let mut children: Vec<&SpanStats> = stats
        .iter()
        .filter(|row| row.span != Span::FramesWallFrameMs && row.parent == parent)
        .filter(|row| !placed.contains(&row.span))
        .collect();
    children.sort_by_key(|row| std::cmp::Reverse(row.sum_ns));
    for child in children {
        placed.push(child.span);
        push(
            table,
            child,
            wall,
            depth,
            self_ns(stats, child, Some(child.span)),
        );
        walk(stats, wall, Some(child.span), depth + 1, table, placed);
    }
}

/// A span's total minus what its children accounted for. For `wall` the
/// children are the spans that recorded no parent at all — the roots of each
/// schedule — which is what makes the remainder "the frame outside our spans".
fn self_ns(stats: &[SpanStats], row: &SpanStats, parent: Option<Span>) -> u64 {
    let children: u64 = stats
        .iter()
        .filter(|candidate| candidate.span != Span::FramesWallFrameMs && candidate.parent == parent)
        .map(|candidate| candidate.sum_ns)
        .sum();
    row.sum_ns.saturating_sub(children)
}

fn push(table: &mut Table, row: &SpanStats, wall: &SpanStats, depth: usize, self_ns: u64) {
    let varies = if row.parent_varies { " *" } else { "" };
    let frames = wall.count;
    table.row([
        format!("{}{}{varies}", "  ".repeat(depth), row.span.name()),
        row.count.to_string(),
        per_frame_calls(row.count, frames),
        ms(row.avg_ns()),
        ms(row.p50_ns),
        ms(row.p95_ns),
        ms(row.p99_ns),
        ms(row.max_ns),
        per_frame_ms(row.sum_ns, frames),
        ms(row.sum_ns),
        percent(row.sum_ns, wall.sum_ns),
        ms(self_ns),
    ]);
}

/// How many times this span ran per frame of the window. `wall` itself is one
/// per frame by construction; anything else is a measured ratio, and it is the
/// number that makes `avg` comparable to `per frame`.
fn per_frame_calls(count: u64, frames: u64) -> String {
    if frames == 0 {
        return "—".to_owned();
    }
    format!("{:.2}", count as f64 / frames as f64)
}

/// A span's total spread over the frames of the window: what one frame paid for
/// this span, however many calls that took.
fn per_frame_ms(sum_ns: u64, frames: u64) -> String {
    if frames == 0 {
        return "—".to_owned();
    }
    format!("{:.2}", sum_ns as f64 / frames as f64 / 1e6)
}

/// The spans ranked by their *own* time, parents included.
///
/// The previous version of this section ranked leaves only — a span with
/// nothing nested inside it — which is the one ranking that cannot see the
/// case it matters for. A parent whose children are cheap and whose own body
/// is expensive has all of that time in `self` and none of it in any leaf, so
/// it was absent from the summary while being the largest single cost in the
/// frame. Self time is defined for every span, so every span is ranked.
fn top_self(stats: &[SpanStats], wall: &SpanStats, out: &mut Vec<String>) {
    let frames = wall.count;
    let mut ranked: Vec<(u64, &SpanStats)> = stats
        .iter()
        .filter(|row| row.span != Span::FramesWallFrameMs)
        .map(|row| (self_ns(stats, row, Some(row.span)), row))
        .filter(|(self_ns, _)| {
            *self_ns >= HOT_SELF_NS.saturating_mul(frames.max(1))
                || *self_ns as f64 * 100.0 / wall.sum_ns.max(1) as f64 >= HOT_SHARE
        })
        .collect();
    if ranked.is_empty() {
        out.push(format!(
            "  top self: none — no span's own time reached {} ms per frame or {HOT_SHARE:.0}% of the frame.",
            ms(HOT_SELF_NS)
        ));
        return;
    }
    ranked.sort_by_key(|(self_ns, _)| std::cmp::Reverse(*self_ns));
    out.push(format!(
        "  top self — every span, parents included, by its own time (≥{} ms per frame or ≥{HOT_SHARE:.0}% of the frame)",
        ms(HOT_SELF_NS)
    ));
    let mut table = Table::new(
        4,
        &[
            ("span", Align::Left),
            ("kind", Align::Left),
            ("self/frame", Align::Right),
            ("self total", Align::Right),
            ("%frame", Align::Right),
            ("n/frame", Align::Right),
            ("under", Align::Left),
        ],
    );
    for (own, row) in ranked.into_iter().take(TOP_SELF) {
        let has_children = stats.iter().any(|child| child.parent == Some(row.span));
        table.row([
            row.span.name().to_owned(),
            if has_children { "parent" } else { "leaf" }.to_owned(),
            per_frame_ms(own, frames),
            ms(own),
            percent(own, wall.sum_ns),
            per_frame_calls(row.count, frames),
            row.parent
                .map_or_else(|| "—".to_owned(), |parent| parent.name().to_owned()),
        ]);
    }
    table.render(out);
    out.push(
        "  `self` here is the same quantity as the tree's last column: the span's total minus what its own children accounted for. It is time the recorder could not attribute to a child — not proven useful work, and on a thread that waits it can be the wait."
            .to_owned(),
    );
}

/// Spans this build declares that opened in neither phase. The tree prints
/// what was sampled, so a span whose producer was deleted or compiled out is
/// absent from it in exactly the same way as a span the workload never
/// reached. Naming them is what tells those two apart.
fn silent_spans(out: &mut Vec<String>) {
    let silent = perf::stats::spans_never_sampled();
    if silent.is_empty() {
        return;
    }
    let names: Vec<&str> = silent.iter().map(|span| span.name()).collect();
    out.push(String::new());
    out.push(format!(
        "  declared but never opened on this run ({}): {}",
        silent.len(),
        names.join(", ")
    ));
}

fn anomalies(out: &mut Vec<String>) {
    let anomalies = perf::stats::anomalies();
    if anomalies.unmatched_end == 0 && anomalies.reopened == 0 {
        return;
    }
    out.push(String::new());
    out.push(format!(
        "  recorder: {} span ends closed nothing, {} spans were opened while already open — those instances are missing from the numbers above.",
        anomalies.unmatched_end, anomalies.reopened
    ));
}
