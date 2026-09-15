use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static RSS_PEAK: AtomicU64 = AtomicU64::new(0);

const MAX_VISIBLE_LINES: usize = 8;

const MORE_RUNNING_ID: u64 = u64::MAX;
const MORE_FINISHED_ID: u64 = u64::MAX - 1;

#[derive(Clone, Default)]
pub struct LoadProgress(Arc<Inner>);

#[derive(Default)]
struct Inner {
    lanes: Mutex<Vec<Lane>>,
    measurement: Mutex<LoadMeasurement>,
    next_id: AtomicU64,
    canceled: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct LoadMeasurement {
    zone_open_started: Option<Instant>,
    rss_at_open: Option<u64>,
    rss_hwm_at_open: Option<u64>,
    zone_image_bytes: Option<u64>,
    first_frame_ms: Option<f32>,
    rss_peak_bytes: Option<u64>,
}

pub fn process_resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let bytes = proc_status_bytes("VmRSS:")?;
        RSS_PEAK.fetch_max(bytes, Ordering::Relaxed);
        Some(bytes)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub fn peak_resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        return proc_status_bytes("VmHWM:");
    }
    #[cfg(not(target_os = "linux"))]
    {
        let n = RSS_PEAK.load(Ordering::Relaxed);
        (n > 0).then_some(n)
    }
}

#[cfg(target_os = "linux")]
fn proc_status_bytes(key: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kib = status.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })?;
    kib.checked_mul(1024)
}

fn format_caption(note: bool, label: &str, done: u64, total: u64) -> String {
    if note {
        return label.to_owned();
    }
    match (total, done) {
        (0, 0) => label.to_owned(),
        (0, done) => format!("{label} {done}"),
        (total, done) => format!("{label} {done}/{total}"),
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[derive(Clone, Copy, Debug, Default)]
struct MemSample {
    rss: Option<u64>,
    heap: Option<u64>,
}

impl MemSample {
    fn now() -> Self {
        Self {
            rss: process_resident_bytes(),
            heap: diag::process_live_heap_bytes(),
        }
    }

    fn suffix(started: Self, ended: Self) -> String {
        fn one(name: &str, start: Option<u64>, end: Option<u64>) -> String {
            match (start, end) {
                (Some(start), Some(end)) => {
                    format!(
                        " {name}={:+.0}MiB→{:.0}MiB",
                        mib(end) - mib(start),
                        mib(end)
                    )
                }
                _ => String::new(),
            }
        }
        format!(
            "{}{}",
            one("rss", started.rss, ended.rss),
            one("heap", started.heap, ended.heap)
        )
    }
}

struct Lane {
    id: u64,
    label: String,

    done: u64,
    total: u64,
    started: Instant,

    started_mem: MemSample,

    ended_mem: Option<MemSample>,

    elapsed: Option<Duration>,

    note: bool,

    frozen_caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadLaneView {
    pub id: u64,
    pub label: String,
    pub done: u64,
    pub total: u64,

    pub elapsed: Option<Duration>,
    pub note: bool,

    pub overflow: Option<LoadOverflow>,
    frozen_caption: Option<String>,

    started: Option<Instant>,
}

/// One stage of a load, as the bench report reads it: when it opened relative
/// to the first stage, how long it held, and what it cost in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadLaneTiming {
    pub label: String,
    pub at: Duration,
    pub elapsed: Duration,
    pub done: u64,
    pub total: u64,
    pub running: bool,
    pub rss_delta: Option<i64>,
    pub heap_delta: Option<i64>,
}

impl LoadLaneTiming {
    pub fn end(&self) -> Duration {
        self.at + self.elapsed
    }

    /// The `rss=+NMiB heap=+NMiB` tail, empty when the platform reports neither.
    pub fn mem_suffix(&self) -> String {
        fn one(name: &str, delta: Option<i64>) -> String {
            match delta {
                Some(delta) => format!(" {name}={:+.0}MiB", delta as f64 / (1024.0 * 1024.0)),
                None => String::new(),
            }
        }
        format!(
            "{}{}",
            one("rss", self.rss_delta),
            one("heap", self.heap_delta)
        )
    }
}

fn delta(started: Option<u64>, ended: Option<u64>) -> Option<i64> {
    let started = i64::try_from(started?).ok()?;
    let ended = i64::try_from(ended?).ok()?;
    Some(ended - started)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOverflow {
    Running(usize),
    Finished(usize),
}

impl LoadLaneView {
    fn from_lane(lane: &Lane) -> Self {
        Self {
            id: lane.id,
            label: lane.label.clone(),
            done: lane.done,
            total: lane.total,
            elapsed: lane.elapsed,
            note: lane.note,
            overflow: None,
            frozen_caption: lane.frozen_caption.clone(),
            started: Some(lane.started),
        }
    }

    fn more_running(n: usize) -> Self {
        Self {
            id: MORE_RUNNING_ID,
            label: format!("... +{n} more"),
            done: 0,
            total: 0,
            elapsed: None,
            note: false,
            overflow: Some(LoadOverflow::Running(n)),
            frozen_caption: None,
            started: None,
        }
    }

    fn more_finished(n: usize) -> Self {
        Self {
            id: MORE_FINISHED_ID,
            label: format!("... +{n} more"),
            done: 0,
            total: 0,
            elapsed: None,
            note: false,
            overflow: Some(LoadOverflow::Finished(n)),
            frozen_caption: None,
            started: None,
        }
    }

    pub fn finished(&self) -> bool {
        match self.overflow {
            Some(LoadOverflow::Finished(_)) => true,
            Some(LoadOverflow::Running(_)) => false,
            None => self.elapsed.is_some(),
        }
    }

    pub fn in_progress(&self) -> bool {
        self.overflow.is_none() && !self.note && self.elapsed.is_none()
    }

    pub fn caption(&self) -> String {
        if let Some(frozen) = &self.frozen_caption {
            return frozen.clone();
        }
        format_caption(self.note, &self.label, self.done, self.total)
    }

    pub fn elapsed_ms_label(&self) -> Option<String> {
        if self.overflow.is_some() || self.note {
            return None;
        }
        let elapsed = match self.elapsed {
            Some(done) => done,
            None => self.started?.elapsed(),
        };
        Some(format!("{:.0}ms", elapsed.as_secs_f32() * 1000.0))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OverlayFit {
    running_shown: usize,
    finished_shown: usize,
    running_hidden: usize,
    finished_hidden: usize,
}

fn fit_overlay(running: usize, finished: usize, max: usize) -> OverlayFit {
    if running >= max {
        return OverlayFit {
            running_shown: running,
            finished_shown: 0,
            running_hidden: 0,
            finished_hidden: finished,
        };
    }
    if running.saturating_add(finished) <= max {
        return OverlayFit {
            running_shown: running,
            finished_shown: finished,
            running_hidden: 0,
            finished_hidden: 0,
        };
    }
    let rest = max - running;
    let finished_shown = rest.saturating_sub(1);
    OverlayFit {
        running_shown: running,
        finished_shown,
        running_hidden: 0,
        finished_hidden: finished.saturating_sub(finished_shown),
    }
}

impl LoadProgress {
    pub fn cancel(&self) {
        self.0
            .canceled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_canceled(&self) -> bool {
        self.0.canceled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn begin_zone_open(&self) {
        let rss = process_resident_bytes();
        let hwm = peak_resident_bytes();
        if let Ok(mut measurement) = self.0.measurement.lock()
            && measurement.zone_open_started.is_none()
        {
            measurement.zone_open_started = Some(Instant::now());
            measurement.rss_at_open = rss;
            measurement.rss_hwm_at_open = hwm;
        }
    }

    pub fn record_zone_image_bytes(&self, bytes: usize) {
        if let Ok(mut measurement) = self.0.measurement.lock() {
            measurement.zone_image_bytes = u64::try_from(bytes).ok();
        }
    }

    pub fn record_first_frame(&self, presented_at: Instant) -> bool {
        let end_hwm = peak_resident_bytes();
        let Ok(mut measurement) = self.0.measurement.lock() else {
            return false;
        };
        if measurement.first_frame_ms.is_some() {
            return false;
        }
        let Some(started) = measurement.zone_open_started else {
            return false;
        };
        measurement.first_frame_ms = Some(
            presented_at
                .saturating_duration_since(started)
                .as_secs_f32()
                * 1000.0,
        );
        measurement.rss_peak_bytes = match (
            measurement.rss_at_open,
            measurement.rss_hwm_at_open,
            end_hwm,
        ) {
            (Some(rss), Some(start_hwm), Some(end_hwm))
                if end_hwm > start_hwm || rss == start_hwm =>
            {
                Some(end_hwm)
            }
            _ => None,
        };
        true
    }

    pub fn zone_image_bytes(&self) -> Option<u64> {
        self.0
            .measurement
            .lock()
            .ok()
            .and_then(|measurement| measurement.zone_image_bytes)
    }

    pub fn first_frame_ms(&self) -> Option<f32> {
        self.0
            .measurement
            .lock()
            .ok()
            .and_then(|measurement| measurement.first_frame_ms)
    }

    pub fn load_rss_peak_bytes(&self) -> Option<u64> {
        self.0
            .measurement
            .lock()
            .ok()
            .and_then(|measurement| measurement.rss_peak_bytes)
    }

    pub fn rss_at_open_bytes(&self) -> Option<u64> {
        self.0
            .measurement
            .lock()
            .ok()
            .and_then(|measurement| measurement.rss_at_open)
    }

    pub fn stage(&self, label: impl Into<String>) -> LoadStage {
        let id = self.push(label.into(), false);
        LoadStage {
            progress: self.clone(),
            id,
        }
    }

    pub fn note(&self, message: impl Into<String>) {
        self.push(message.into(), true);
    }

    fn push(&self, label: String, note: bool) -> u64 {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut lanes) = self.0.lanes.lock() {
            lanes.push(Lane {
                id,
                label,
                done: 0,
                total: 0,
                started: Instant::now(),
                started_mem: MemSample::now(),
                ended_mem: None,
                elapsed: None,
                note,
                frozen_caption: None,
            });
        }
        id
    }

    pub fn snapshot_lanes(&self) -> Vec<LoadLaneView> {
        let Ok(lanes) = self.0.lanes.lock() else {
            return Vec::new();
        };
        let mut running: Vec<&Lane> = lanes
            .iter()
            .filter(|lane| lane.elapsed.is_none() && !lane.note)
            .collect();
        running.sort_by_key(|lane| std::cmp::Reverse(lane.id));
        let mut finished: Vec<&Lane> = lanes
            .iter()
            .filter(|lane| lane.elapsed.is_some() || lane.note)
            .collect();
        finished.sort_by_key(|lane| std::cmp::Reverse(lane.id));
        let fit = fit_overlay(running.len(), finished.len(), MAX_VISIBLE_LINES);
        let mut views = Vec::with_capacity(MAX_VISIBLE_LINES);
        if fit.running_hidden > 0 {
            views.push(LoadLaneView::more_running(fit.running_hidden));
        }
        views.extend(
            running
                .into_iter()
                .take(fit.running_shown)
                .map(LoadLaneView::from_lane),
        );
        views.extend(
            finished
                .into_iter()
                .take(fit.finished_shown)
                .map(LoadLaneView::from_lane),
        );
        if fit.finished_hidden > 0 {
            views.push(LoadLaneView::more_finished(fit.finished_hidden));
        }
        views
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.snapshot_lanes()
            .into_iter()
            .map(|lane| match lane.elapsed_ms_label() {
                Some(ms) => format!("{} {ms}", lane.caption()),
                None => lane.caption(),
            })
            .collect()
    }

    /// Every stage this load opened, in the order it opened them, with the
    /// window it occupied. Stages run in parallel on the load pool, so the
    /// offsets are what says which of them overlapped and which one nobody
    /// else was covering; `timing_report` is this, flattened to lines, and the
    /// bench report is this, analysed.
    pub fn lane_timings(&self) -> Vec<LoadLaneTiming> {
        let Ok(lanes) = self.0.lanes.lock() else {
            return Vec::new();
        };
        let Some(origin) = lanes
            .iter()
            .filter(|lane| !lane.note)
            .map(|lane| lane.started)
            .min()
        else {
            return Vec::new();
        };
        lanes
            .iter()
            .filter(|lane| !lane.note)
            .map(|lane| {
                let ended_mem = lane.ended_mem.unwrap_or_else(MemSample::now);
                LoadLaneTiming {
                    label: lane.label.clone(),
                    at: lane.started.saturating_duration_since(origin),
                    elapsed: lane.elapsed.unwrap_or_else(|| lane.started.elapsed()),
                    done: lane.done,
                    total: lane.total,
                    running: lane.elapsed.is_none(),
                    rss_delta: delta(lane.started_mem.rss, ended_mem.rss),
                    heap_delta: delta(lane.started_mem.heap, ended_mem.heap),
                }
            })
            .collect()
    }

    pub fn timing_report(&self) -> Vec<String> {
        let lanes = self.lane_timings();
        let mut total = Duration::ZERO;
        let mut rows = Vec::with_capacity(lanes.len() + 1);
        for lane in &lanes {
            total += lane.elapsed;
            let items = match lane.total {
                0 if lane.done == 0 => String::new(),
                0 => format!(" items={}", lane.done),
                whole => format!(" items={}/{whole}", lane.done),
            };
            rows.push(format!(
                "load stage: {} at=+{:.0}ms {:.1}ms{items}{}{}",
                lane.label,
                lane.at.as_secs_f32() * 1000.0,
                lane.elapsed.as_secs_f32() * 1000.0,
                lane.mem_suffix(),
                if lane.running { " (still running)" } else { "" },
            ));
        }
        rows.push(format!(
            "load stages: {} stages, {:.1}ms of stage wall time (parallel stages overlap){}",
            rows.len(),
            total.as_secs_f32() * 1000.0,
            peak_resident_bytes().map_or(String::new(), |peak| {
                format!(" rss_peak={:.0}MiB", mib(peak))
            }),
        ));
        rows
    }

    fn with_lane(&self, id: u64, edit: impl FnOnce(&mut Lane)) {
        if let Ok(mut lanes) = self.0.lanes.lock()
            && let Some(lane) = lanes.iter_mut().find(|lane| lane.id == id)
        {
            edit(lane);
        }
    }
}

pub struct LoadStage {
    progress: LoadProgress,
    id: u64,
}

impl LoadStage {
    pub fn total(&self, total: u64) {
        self.progress.with_lane(self.id, |lane| lane.total = total);
    }

    pub fn advance(&self, delta: u64) {
        self.progress.with_lane(self.id, |lane| lane.done += delta);
    }

    pub fn set_done(&self, done: u64) {
        self.progress.with_lane(self.id, |lane| lane.done = done);
    }

    pub fn is_canceled(&self) -> bool {
        self.progress.is_canceled()
    }
}

impl Drop for LoadStage {
    fn drop(&mut self) {
        let mut finished = None;
        let ended_mem = MemSample::now();
        self.progress.with_lane(self.id, |lane| {
            let elapsed = lane.started.elapsed();
            lane.frozen_caption = Some(format_caption(
                lane.note,
                &lane.label,
                lane.done,
                lane.total,
            ));
            lane.elapsed = Some(elapsed);
            lane.ended_mem = Some(ended_mem);
            finished = Some((
                lane.label.clone(),
                elapsed,
                lane.done,
                lane.total,
                lane.started_mem,
            ));
        });
        if let Some((label, elapsed, done, total, started_mem)) = finished {
            let items = match total {
                0 if done == 0 => String::new(),
                0 => format!(" items={done}"),
                total => format!(" items={done}/{total}"),
            };
            diag::info!(
                World,
                "load stage: {label} {:.1}ms{items}{}",
                elapsed.as_secs_f32() * 1000.0,
                MemSample::suffix(started_mem, ended_mem),
            );
        }
    }
}
