//! The loading table, projected from the load's facts.
//!
//! Everything here is presentation: which rows exist, in what order, and what
//! their cells say. Nothing in this file may change what the load does.

use assets::{LoadSnapshot, StageId, StageOutcome, StageSnapshot};
use bevy::prelude::Color;

/// What a row is doing, once its stages have been folded together.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowState {
    Running,
    Done,
    Skipped,
    Cancelled,
    Abandoned,
    Failed,
}

impl RowState {
    fn suffix(self) -> &'static str {
        match self {
            Self::Running | Self::Done => "",
            Self::Skipped => " (skipped)",
            Self::Cancelled => " (canceled)",
            Self::Abandoned => " (interrupted)",
            Self::Failed => " (failed)",
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::Running => RUNNING,
            Self::Done => DONE,
            Self::Skipped => SKIPPED,
            Self::Cancelled | Self::Abandoned => CANCELLED,
            Self::Failed => FAILED,
        }
    }
}

const RUNNING: Color = Color::srgb(1.0, 0.72, 0.28);
const DONE: Color = Color::srgb(0.62, 0.92, 0.68);
const SKIPPED: Color = Color::srgb(0.52, 0.56, 0.62);
const CANCELLED: Color = Color::srgb(0.86, 0.72, 0.42);
const FAILED: Color = Color::srgb(1.0, 0.35, 0.25);

pub(crate) struct LoadRow {
    pub id: StageId,
    pub name: String,
    /// Bytes where the stage can weigh itself, its own units where it cannot.
    pub value: String,
    pub time: String,
    pub state: RowState,
}

/// `running` is in the order the stages opened and `ended` in the order they
/// finished, so both blocks only ever gain a line.
pub(crate) struct LoadTable {
    pub running: Vec<LoadRow>,
    pub ended: Vec<LoadRow>,
}

/// The name a person reads. The only place that turns a [`StageId`] into
/// English, and it never carries what the other two columns say.
fn stage_name(id: StageId) -> &'static str {
    match id {
        StageId::MapAssets => "Map assets",
        StageId::CommonAssets => "Common assets",
        StageId::Localization => "Localization",
        StageId::Images => "Images",
        StageId::Preview => "Loadscreen",
        StageId::Install => "Match install",
        StageId::WorldImages => "World images",
        StageId::Programs => "Programs",
        StageId::ProgramMerge => "Program merge",
        StageId::Shaders => "Shaders",
        StageId::GpuTextures => "GPU textures",
        StageId::Pipelines => "Pipelines",
        StageId::RenderFrames => "Render frames",
        StageId::Audio => "Audio",
        StageId::Navigation => "Navigation",
        StageId::Admission => "Admission",
    }
}

/// What a stage counts, where the number alone would not say.
fn count_unit(id: StageId) -> &'static str {
    match id {
        StageId::Navigation => " edges",
        StageId::RenderFrames => " frames",
        _ => "",
    }
}

pub(crate) fn format_elapsed(value: std::time::Duration) -> String {
    if value < std::time::Duration::from_secs(1) {
        format!("{} ms", value.as_millis())
    } else {
        format!("{:.1} s", value.as_secs_f64())
    }
}

fn format_bytes(bytes: u64) -> String {
    const STEP: f64 = 1024.0;
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= STEP && unit + 1 < UNITS.len() {
        value /= STEP;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn format_count(count: u64) -> String {
    const STEP: f64 = 1000.0;
    const UNITS: [&str; 4] = ["", " k", " M", " G"];
    let mut value = count as f64;
    let mut unit = 0;
    while value >= STEP && unit + 1 < UNITS.len() {
        value /= STEP;
        unit += 1;
    }
    if unit == 0 {
        count.to_string()
    } else if value < 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

/// Bytes where the stage weighs itself, its own units otherwise. A stage with
/// neither has nothing to say here and [`project`] leaves it out.
fn fold_value(id: StageId, slots: &[&StageSnapshot]) -> Option<String> {
    let weighed: Vec<u64> = slots.iter().filter_map(|slot| slot.bytes).collect();
    if !weighed.is_empty() {
        return Some(format_bytes(weighed.iter().sum()));
    }
    // Counts only add up where the unit is the same, so a group sums the slots
    // of one stage and nothing else.
    let counted: Vec<_> = slots.iter().filter_map(|slot| slot.count).collect();
    if counted.is_empty() {
        return None;
    }
    let completed: u64 = counted.iter().map(|count| count.completed).sum();
    Some(format!("{}{}", format_count(completed), count_unit(id)))
}

fn fold_state(slots: &[&StageSnapshot]) -> RowState {
    if slots.iter().any(|slot| slot.running()) {
        return RowState::Running;
    }
    let outcomes: Vec<StageOutcome> = slots.iter().filter_map(|slot| slot.outcome()).collect();
    if outcomes.contains(&StageOutcome::Failed) {
        RowState::Failed
    } else if outcomes.contains(&StageOutcome::Abandoned) {
        RowState::Abandoned
    } else if outcomes.contains(&StageOutcome::Cancelled) {
        RowState::Cancelled
    } else if outcomes
        .iter()
        .all(|outcome| *outcome == StageOutcome::Skipped)
    {
        RowState::Skipped
    } else {
        RowState::Done
    }
}

/// From the first start to the last end. Parallel stages overlap, so this is
/// never the sum of their spans.
fn fold_elapsed(slots: &[&StageSnapshot], now: std::time::Instant) -> Option<std::time::Duration> {
    let started = slots.iter().filter_map(|slot| slot.started_at).min()?;
    let running = slots.iter().any(|slot| slot.running());
    let ended = slots
        .iter()
        .filter_map(|slot| slot.end.as_ref().map(|end| end.at))
        .max();
    let until = match ended {
        Some(at) if !running => at,
        _ => now,
    };
    Some(until.saturating_duration_since(started))
}

/// A row per stage the load has opened and can say something about; a stage
/// nobody has reached yet, or one with no unit of its own, has no row.
pub(crate) fn project(snapshot: &LoadSnapshot) -> LoadTable {
    let mut opened: Vec<StageId> = Vec::new();
    for slot in &snapshot.stages {
        if !opened.contains(&slot.key.id) {
            opened.push(slot.key.id);
        }
    }

    let mut running: Vec<(std::time::Instant, LoadRow)> = Vec::new();
    let mut ended: Vec<(std::time::Instant, LoadRow)> = Vec::new();
    for id in opened {
        let slots: Vec<&StageSnapshot> = snapshot
            .stages
            .iter()
            .filter(|slot| slot.key.id == id)
            .collect();
        let Some(value) = fold_value(id, &slots) else {
            continue;
        };
        let Some(elapsed) = fold_elapsed(&slots, snapshot.now) else {
            continue;
        };
        let state = fold_state(&slots);
        let row = LoadRow {
            id,
            name: format!("{}{}", stage_name(id), state.suffix()),
            value,
            time: format_elapsed(elapsed),
            state,
        };
        if matches!(state, RowState::Running) {
            // Placed by when the group first opened, not by its latest scope:
            // another `images/…` starting must not move the row.
            let at = slots.iter().filter_map(|slot| slot.started_at).min();
            running.push((at.unwrap_or(snapshot.requested_at), row));
        } else {
            let at = slots
                .iter()
                .filter_map(|slot| slot.end.as_ref().map(|end| end.at))
                .max();
            ended.push((at.unwrap_or(snapshot.requested_at), row));
        }
    }

    // Stable, so stages that opened or ended in the same instant keep the order
    // the load registered them in.
    running.sort_by_key(|(at, _)| *at);
    ended.sort_by_key(|(at, _)| *at);
    LoadTable {
        running: running.into_iter().map(|(_, row)| row).collect(),
        ended: ended.into_iter().map(|(_, row)| row).collect(),
    }
}

pub(crate) fn activity_line(snapshot: &LoadSnapshot) -> String {
    let active = snapshot.stages.iter().filter(|slot| slot.running()).count();
    match active {
        0 => "Loading".to_owned(),
        1 => "Loading · 1 active stage".to_owned(),
        n => format!("Loading · {n} active stages"),
    }
}

/// Wall time from the request to now, or to the last stage that ended once
/// nothing is open. Not the sum of the column under it: stages overlap.
pub(crate) fn total_elapsed(snapshot: &LoadSnapshot) -> std::time::Duration {
    let running = snapshot.stages.iter().any(|slot| slot.running());
    let ended = snapshot
        .stages
        .iter()
        .filter_map(|slot| slot.end.as_ref().map(|end| end.at))
        .max();
    let until = match ended {
        Some(at) if !running => at,
        _ => snapshot.now,
    };
    until.saturating_duration_since(snapshot.requested_at)
}
