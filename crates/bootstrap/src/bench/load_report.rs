//! Section one: where the time between the command and a playable map went.
//!
//! Two views of the same window, because one alone lies. The **waterfall** is
//! the serial story — request, walk, install, spawn, first drawn frame — and it
//! is what a player waits through. The **stages** are the parallel story: the
//! walk runs on the load pool, so a stage's own duration says nothing about
//! whether removing it would shorten anything. What answers that is the time a
//! stage held the load *alone*, which is what `exclusive` below measures.

use std::time::{Duration, Instant};

use assets::LoadLaneTiming;

use crate::bench::milestones::Milestones;
use crate::bench::table::{Align, Table, bytes, items, mib, ms, secs};

/// Stages listed by name before the report rolls the rest into one line.
const TOP_STAGES: usize = 16;

pub(crate) fn render(bench: &Milestones, command_start: Option<Instant>, out: &mut Vec<String>) {
    out.push("[1/2] MAP LOAD".to_owned());
    waterfall(bench, command_start, out);
    out.push(String::new());
    stages(&bench.lanes, out);
    out.push(String::new());
    picture(bench, out);
}

fn waterfall(bench: &Milestones, command_start: Option<Instant>, out: &mut Vec<String>) {
    let origin = command_start.unwrap_or(bench.t0);
    let source = if command_start.is_some() {
        "the shell command"
    } else {
        "process start"
    };
    match bench.playable() {
        Some(playable) => out.push(format!(
            "  command → playable: {} (from {source})",
            secs(playable.saturating_duration_since(origin))
        )),
        None => out.push(format!(
            "  command → playable: MISS — the map never became playable (from {source})"
        )),
    }
    out.push(String::new());

    let mut table = Table::new(
        2,
        &[
            ("milestone", Align::Left),
            ("at", Align::Right),
            ("+step", Align::Right),
            ("what happened in the step", Align::Left),
        ],
    );
    let mut steps: Vec<(&str, Option<Instant>, &str)> = vec![
        (
            "process start",
            Some(bench.t0),
            "argument parsing, window, plugins",
        ),
        ("load requested", bench.request, "zone discovery"),
        ("match installed", bench.installed, "the load-pool walk"),
        (
            "world spawned",
            bench.spawned,
            "scene conversion, GPU upload",
        ),
        (
            "loading screen down",
            bench.overlay_down,
            "overlay torn down",
        ),
        ("in game", bench.ingame, "session handed over"),
        (
            "first drawn frame",
            bench.rendered_spawn,
            "pipelines compiled",
        ),
        ("ambient booted", bench.ambient, "map ambience started"),
    ];
    // In the order they actually happened, not the order they are listed: two
    // of these race, and a "+step" measured against a later milestone would
    // read as a zero rather than as the overlap it is. A milestone that was
    // never reached sorts to the end, where it reads as a gap in the story
    // rather than as its beginning.
    steps.sort_by_key(|(_, at, _)| (at.is_none(), *at));
    let mut previous: Option<Instant> = command_start;
    for (name, at, note) in steps {
        let Some(at) = at else {
            table.row([
                name.to_owned(),
                "MISS".to_owned(),
                "—".to_owned(),
                note.to_owned(),
            ]);
            continue;
        };
        let step = previous.map(|previous| at.saturating_duration_since(previous));
        table.row([
            name.to_owned(),
            secs(at.saturating_duration_since(origin)),
            step.map_or_else(|| "—".to_owned(), secs),
            note.to_owned(),
        ]);
        previous = Some(at);
    }
    table.render(out);
}

fn stages(lanes: &[LoadLaneTiming], out: &mut Vec<String>) {
    if lanes.is_empty() {
        out.push("  load stages: MISS — the walk opened no stage on this run".to_owned());
        return;
    }
    let window = lanes
        .iter()
        .map(LoadLaneTiming::end)
        .max()
        .unwrap_or(Duration::ZERO);
    let held: Duration = lanes.iter().map(|lane| lane.elapsed).sum();
    let covered = union(lanes);
    let exclusive = exclusive(lanes);

    let parallel = if covered.is_zero() {
        "—".to_owned()
    } else {
        format!("{:.1}x", held.as_secs_f64() / covered.as_secs_f64())
    };
    out.push(format!(
        "  load stages: {} stages holding {} across a {} window — {parallel} parallel, {} covered by no stage",
        lanes.len(),
        secs(held),
        secs(window),
        secs(window.saturating_sub(covered)),
    ));
    out.push(
        "  `exclusive` is the time a stage was the only one running: the part of the load it, alone, lengthened."
            .to_owned(),
    );

    let mut ordered: Vec<(usize, &LoadLaneTiming)> = lanes.iter().enumerate().collect();
    ordered.sort_by_key(|(index, lane)| (std::cmp::Reverse(lane.elapsed), *index));

    let mut table = Table::new(
        2,
        &[
            ("stage", Align::Left),
            ("at", Align::Right),
            ("held", Align::Right),
            ("exclusive", Align::Right),
            ("items", Align::Right),
            ("memory", Align::Left),
        ],
    );
    for (index, lane) in ordered.iter().take(TOP_STAGES) {
        table.row([
            format!(
                "{}{}",
                lane.label,
                if lane.running { " (running)" } else { "" }
            ),
            secs(lane.at),
            secs(lane.elapsed),
            secs(exclusive[*index]),
            items(lane.done, lane.total),
            lane.mem_suffix().trim().to_owned(),
        ]);
    }
    table.render(out);
    if ordered.len() > TOP_STAGES {
        let rest: Duration = ordered
            .iter()
            .skip(TOP_STAGES)
            .map(|(_, lane)| lane.elapsed)
            .sum();
        out.push(format!(
            "  … and {} shorter stages holding {}",
            ordered.len() - TOP_STAGES,
            secs(rest)
        ));
    }
}

fn picture(bench: &Milestones, out: &mut Vec<String>) {
    let mut lines = Vec::new();
    if let Some(bytes) = bench.progress.zone_image_bytes() {
        lines.push(format!("zone image {}", mib(bytes)));
    }
    match (
        bench.progress.rss_at_open_bytes(),
        assets::peak_resident_bytes(),
    ) {
        (Some(open), Some(peak)) => lines.push(format!("rss {} → {}", mib(open), mib(peak))),
        (_, Some(peak)) => lines.push(format!("rss peak {}", mib(peak))),
        _ => {}
    }
    if !lines.is_empty() {
        out.push(format!("  memory: {}", lines.join(", ")));
    }

    let frames = perf::stats::snapshot(perf::Phase::Load)
        .into_iter()
        .find(|stats| stats.span == perf::Span::FramesWallFrameMs);
    match frames {
        Some(frames) => out.push(format!(
            "  frames drawn while loading: {} at {} ms average (the loading screen)",
            frames.count,
            ms(frames.avg_ns())
        )),
        None => out.push("  frames drawn while loading: none reached the frame clock".to_owned()),
    }

    match (bench.screenshot_bytes(), bench.shot) {
        (Some(size), _) => out.push(format!(
            "  screenshot: {} ({}, {} world batches, g0 {})",
            bench.screenshot.display(),
            bytes(size),
            bench
                .batches_at_shot
                .map_or_else(|| "MISS".to_owned(), |n| n.to_string()),
            bench
                .g0_at_shot
                .map_or_else(|| "MISS".to_owned(), |n| n.to_string()),
        )),
        _ => out.push(format!(
            "  screenshot: MISS — nothing was written to {}",
            bench.screenshot.display()
        )),
    }
}

/// Total time during which at least one stage was running.
fn union(lanes: &[LoadLaneTiming]) -> Duration {
    let mut windows: Vec<(Duration, Duration)> =
        lanes.iter().map(|lane| (lane.at, lane.end())).collect();
    windows.sort();
    let mut covered = Duration::ZERO;
    let mut open: Option<(Duration, Duration)> = None;
    for (start, end) in windows {
        match open {
            Some((from, until)) if start <= until => open = Some((from, until.max(end))),
            Some((from, until)) => {
                covered += until.saturating_sub(from);
                open = Some((start, end));
            }
            None => open = Some((start, end)),
        }
    }
    if let Some((from, until)) = open {
        covered += until.saturating_sub(from);
    }
    covered
}

/// Per stage, the time it was the only stage running. A sweep over the edges:
/// between two consecutive edges the set of running stages does not change, so
/// a gap with exactly one stage open belongs to that stage.
fn exclusive(lanes: &[LoadLaneTiming]) -> Vec<Duration> {
    let mut edges: Vec<(Duration, isize, usize)> = Vec::with_capacity(lanes.len() * 2);
    for (index, lane) in lanes.iter().enumerate() {
        edges.push((lane.at, 1, index));
        edges.push((lane.end(), -1, index));
    }
    // Closing edges first at the same instant: a stage that ends where another
    // begins was never alone with it.
    edges.sort_by_key(|(at, delta, index)| (*at, -*delta, *index));

    let mut out = vec![Duration::ZERO; lanes.len()];
    let mut open: Vec<usize> = Vec::new();
    let mut previous = edges
        .first()
        .map(|(at, _, _)| *at)
        .unwrap_or(Duration::ZERO);
    for (at, delta, index) in edges {
        if at > previous && open.len() == 1 {
            out[open[0]] += at - previous;
        }
        previous = at;
        if delta > 0 {
            open.push(index);
        } else if let Some(position) = open.iter().position(|open| *open == index) {
            open.remove(position);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(at_ms: u64, elapsed_ms: u64) -> LoadLaneTiming {
        LoadLaneTiming {
            label: format!("stage at {at_ms}"),
            at: Duration::from_millis(at_ms),
            elapsed: Duration::from_millis(elapsed_ms),
            done: 0,
            total: 0,
            running: false,
            rss_delta: None,
            heap_delta: None,
        }
    }

    #[test]
    fn union_merges_overlapping_stages_and_keeps_the_gap_out() {
        let lanes = [lane(0, 100), lane(50, 100), lane(400, 100)];
        assert_eq!(union(&lanes), Duration::from_millis(250));
    }

    #[test]
    fn exclusive_credits_only_the_stage_nobody_overlapped() {
        // 0..100 alone, 100..150 shared, 150..200 alone on the second stage.
        let lanes = [lane(0, 150), lane(100, 100)];
        let exclusive = exclusive(&lanes);
        assert_eq!(exclusive[0], Duration::from_millis(100));
        assert_eq!(exclusive[1], Duration::from_millis(50));
    }

    #[test]
    fn a_stage_that_shared_its_whole_window_is_credited_nothing() {
        let lanes = [lane(0, 200), lane(50, 50)];
        assert_eq!(exclusive(&lanes)[1], Duration::ZERO);
    }
}
