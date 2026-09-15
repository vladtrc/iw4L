//! Section two: where a gameplay frame goes.
//!
//! The tree is not a table someone wrote down: a span's parent is whichever
//! span was open when it began, recorded by the same instrumentation that
//! timed it. Move a system between sets and this report follows; the only
//! declared relation is that `wall` is the frame clock, which encloses no
//! scope because it is opened in one frame and closed in the next.

use perf::{Phase, Span, SpanStats};

use crate::bench::table::{Align, Table, ms, percent};

/// A leaf worth naming: either it is expensive on its own, or it is a big share
/// of a frame that nothing else explains.
const HOT_AVG_NS: u64 = 500_000;
const HOT_SHARE: f64 = 5.0;

pub(crate) fn render(out: &mut Vec<String>) {
    let stats = perf::stats::snapshot(Phase::Live);
    out.push("[2/2] FRAME TIME".to_owned());
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
    hot(&stats, &wall, out);
    anomalies(out);
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
            ("avg", Align::Right),
            ("p50", Align::Right),
            ("p95", Align::Right),
            ("p99", Align::Right),
            ("max", Align::Right),
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
    table.row([
        format!("{}{}{varies}", "  ".repeat(depth), row.span.name()),
        row.count.to_string(),
        ms(row.avg_ns()),
        ms(row.p50_ns),
        ms(row.p95_ns),
        ms(row.p99_ns),
        ms(row.max_ns),
        ms(row.sum_ns),
        percent(row.sum_ns, wall.sum_ns),
        ms(self_ns),
    ]);
}

fn hot(stats: &[SpanStats], wall: &SpanStats, out: &mut Vec<String>) {
    let mut leaves: Vec<&SpanStats> = stats
        .iter()
        .filter(|row| row.span != Span::FramesWallFrameMs)
        .filter(|row| !stats.iter().any(|child| child.parent == Some(row.span)))
        .filter(|row| {
            row.avg_ns() >= HOT_AVG_NS
                || row.sum_ns as f64 * 100.0 / wall.sum_ns.max(1) as f64 >= HOT_SHARE
        })
        .collect();
    leaves.sort_by_key(|row| std::cmp::Reverse(row.sum_ns));
    if leaves.is_empty() {
        out.push(format!(
            "  hot leaves: none — no span with nothing under it reached {} ms average or {HOT_SHARE:.0}% of the frame.",
            ms(HOT_AVG_NS)
        ));
        return;
    }
    out.push(format!(
        "  hot leaves (nothing nested inside them; ≥{} ms average or ≥{HOT_SHARE:.0}% of the frame)",
        ms(HOT_AVG_NS)
    ));
    let mut table = Table::new(
        4,
        &[
            ("span", Align::Left),
            ("avg", Align::Right),
            ("p95", Align::Right),
            ("%frame", Align::Right),
            ("under", Align::Left),
        ],
    );
    for leaf in leaves {
        table.row([
            leaf.span.name().to_owned(),
            ms(leaf.avg_ns()),
            ms(leaf.p95_ns),
            percent(leaf.sum_ns, wall.sum_ns),
            leaf.parent
                .map_or_else(|| "—".to_owned(), |parent| parent.name().to_owned()),
        ]);
    }
    table.render(out);
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
