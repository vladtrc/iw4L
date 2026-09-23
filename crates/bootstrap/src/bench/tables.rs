//! `frames.csv` and `load_jobs.csv`: the rows the reports were summarised from.
//!
//! A histogram cannot be un-summarised. `report.txt` says p99 was 34.6 ms and
//! nothing in it can say which frame that was, what else was true of it, or
//! whether the twenty worst frames were one hitch or twenty. These two files
//! are the rows, written once at exit from tables that were filled with
//! relaxed atomics and no formatting while the run was going.
//!
//! Both are CSV with a header and no quoting rules to learn: every field is a
//! number, a bare token or an empty cell. An empty cell is MISS — the recorder
//! had no answer — and is never a zero.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use assets::load_jobs::{JobRow, Jobs};
use perf::frames::{FrameRow, Frames, flag};
use perf::{Counter, Span};

/// `frames.csv`, one row per frame the recorder kept.
pub(crate) fn write_frames(path: &Path, frames: &Frames) -> Result<(), String> {
    let mut out = String::with_capacity(256 * (frames.rows.len() + 1));
    let mut header = String::from(
        "frame,phase,start_ns,end_ns,wall_ns,covered_ns,main_covered_ns,\
         carried_in_ns,carried_spans,\
         coverage_overflow,main_thread,render_thread,replay_tick,flags",
    );
    for span in Span::ALL {
        if span == Span::FramesWallFrameMs {
            continue;
        }
        let _ = write!(header, ",{}_ns", span.name());
    }
    for counter in Counter::ALL {
        let _ = write!(header, ",{}", perf::frames::counter_column(counter));
    }
    out.push_str(&header);
    out.push('\n');
    for row in &frames.rows {
        write_frame(&mut out, row);
    }
    std::fs::write(path, out).map_err(|error| format!("write {}: {error}", path.display()))
}

fn write_frame(out: &mut String, row: &FrameRow) {
    let _ = write!(
        out,
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        row.index,
        if row.phase == 0 { "load" } else { "live" },
        row.start_ns,
        row.end_ns,
        row.wall_ns,
        // The union of the root spans clipped to the wall, so `wall_ns -
        // covered_ns` is the unattributed remainder and never negative.
        row.covered_ns,
        // The same union without the render thread: `main_covered_ns +
        // render_thread_ns - covered_ns` is how much of this frame the two
        // ran at the same time.
        row.main_covered_ns,
        row.carried_in_ns,
        row.carried_spans,
        u8::from(row.coverage_overflow),
        row.main_thread,
        // Slot zero is "no render_thread span closed in this frame", which is
        // a different fact from "the same thread as main".
        cell(u64::from(row.render_thread), row.render_thread != 0),
        row.replay_tick
            .map_or_else(String::new, |tick| tick.to_string()),
        flags(row.flags),
    );
    // Each span column is the span's overlap with this frame's wall, not its
    // elapsed time: a decode that ran across four frames is in four rows, each
    // with its own part of it, and never larger than the frame it is in.
    for span in Span::ALL {
        if span == Span::FramesWallFrameMs {
            continue;
        }
        let _ = write!(out, ",{}", row.spans_ns[span as usize]);
    }
    for counter in Counter::ALL {
        // No sample is not a zero: a counter the frame never emitted leaves an
        // empty cell, so a reader averaging the column does not average in
        // frames that never reported.
        let _ = write!(
            out,
            ",{}",
            cell(
                row.counters[counter as usize],
                row.counter_samples[counter as usize] > 0
            )
        );
    }
    out.push('\n');
}

fn cell(value: u64, present: bool) -> String {
    if present {
        value.to_string()
    } else {
        String::new()
    }
}

/// The set bits, `|`-joined, so the column stays greppable without a legend.
fn flags(bits: u32) -> String {
    let names: Vec<&str> = flag::ALL
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    names.join("|")
}

pub(crate) fn write_framebuffers(
    path: &Path,
    passes: &[gpu_probe::FramebufferPass],
) -> Result<(), String> {
    let mut out = String::from("pass,key,width,height,attachments,creations\n");
    for pass in passes {
        for row in &pass.keys {
            let _ = writeln!(
                out,
                "{},{:016x},{},{},{},{}",
                pass.label, row.key, row.extent.0, row.extent.1, row.attachments, row.creations,
            );
        }
        if pass.overflow_keys > 0 {
            let _ = writeln!(
                out,
                "{},overflow:{},,,,{}",
                pass.label, pass.overflow_keys, pass.overflow_creations,
            );
        }
    }
    std::fs::write(path, out).map_err(|error| format!("write {}: {error}", path.display()))
}

pub(crate) fn write_encoders(
    path: &Path,
    shapes: &[gpu_probe::EncoderShape],
) -> Result<(), String> {
    let mut out = String::from("retires,passes\n");
    for shape in shapes {
        let _ = writeln!(out, "{},{}", shape.retires, shape.signature);
    }
    std::fs::write(path, out).map_err(|error| format!("write {}: {error}", path.display()))
}

/// `load_jobs.csv`, one row per load job.
///
/// Times are milliseconds from `origin` — the same zero the load waterfall in
/// `report.txt` counts from, so a row here can be laid against a milestone
/// there without converting anything.
pub(crate) fn write_jobs(path: &Path, jobs: &Jobs, origin: Instant) -> Result<(), String> {
    let mut out = String::with_capacity(160 * (jobs.rows.len() + 1));
    out.push_str(
        "job_id,deps,kind,namespace,canonical_id,\
         discovered_ms,plan_ready_ms,enqueued_ms,started_ms,decode_started_ms,\
         finished_ms,joined_ms,\
         ready_to_enqueue_ms,queue_delay_ms,budget_wait_ms,decode_ms,service_ms,\
         completed_to_join_ms,budget_waited,outstanding_at_decode,\
         items,source_bytes,prepared_bytes,reused_bytes,output_bytes,retained_bytes,discarded_bytes,\
         rss_at_finish,cache_result,discard_reason\n",
    );
    for row in &jobs.rows {
        write_job(&mut out, row, origin);
    }
    std::fs::write(path, out).map_err(|error| format!("write {}: {error}", path.display()))
}

fn write_job(out: &mut String, row: &JobRow, origin: Instant) {
    let at = |stamp: Option<Instant>| -> String {
        stamp.map_or_else(String::new, |stamp| {
            format!(
                "{:.3}",
                stamp.saturating_duration_since(origin).as_secs_f64() * 1000.0
            )
        })
    };
    // Each of these is a different fix. Ready-but-not-enqueued is a scheduling
    // bug, queue delay is a pool that is too small or too busy, budget wait is
    // a memory ceiling holding a worker that already has the job, and decode is
    // the work itself. `service_ms` is still the whole of it — budget wait and
    // decode add up to it — so an older reading of the column stays true.
    let gap = |from: Option<Instant>, to: Option<Instant>| -> String {
        match (from, to) {
            (Some(from), Some(to)) => format!(
                "{:.3}",
                to.saturating_duration_since(from).as_secs_f64() * 1000.0
            ),
            _ => String::new(),
        }
    };
    let bytes = |value: Option<u64>| value.map_or_else(String::new, |value| value.to_string());
    let _ = writeln!(
        out,
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        row.id,
        row.deps
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(" "),
        row.kind.name(),
        row.namespace.unwrap_or(""),
        token(row.canonical.as_deref()),
        at(Some(row.discovered_at)),
        at(row.plan_ready_at),
        at(row.enqueued_at),
        at(row.started_at),
        at(row.decode_started_at),
        at(row.finished_at),
        at(row.joined_at),
        gap(row.plan_ready_at, row.enqueued_at),
        gap(row.enqueued_at, row.started_at),
        gap(row.started_at, row.decode_started_at),
        gap(row.decode_started_at, row.finished_at),
        gap(row.started_at, row.finished_at),
        gap(row.finished_at, row.joined_at),
        // A job that never recorded the stamp leaves the cell empty rather than
        // claiming it did not wait.
        cell(
            u64::from(row.waited_for_budget),
            row.decode_started_at.is_some()
        ),
        bytes(row.outstanding_at_decode),
        bytes(row.items),
        bytes(row.source_bytes),
        bytes(row.prepared_bytes),
        bytes(row.reused_bytes),
        bytes(row.output_bytes),
        bytes(row.retained_bytes),
        bytes(row.discarded_bytes),
        bytes(row.rss_at_finish),
        row.cache_result.map_or("", |result| result.name()),
        token(row.discard_reason.as_deref()),
    );
}

/// A free-text cell with the separators taken out. No quoting: a comma or a
/// newline inside a label would split the row, and a label is never worth a
/// parser.
fn token(value: Option<&str>) -> String {
    value.map_or_else(String::new, |value| value.replace([',', '\n', '\r'], " "))
}
