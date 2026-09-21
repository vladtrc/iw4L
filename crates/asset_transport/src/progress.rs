//! What a map load has done so far, as facts rather than as text.
//!
//! One process per load request. A stage is a logical unit of that process: it
//! starts, it may count the work it has processed, and its owner publishes how
//! it ended. Nothing here formats a line, orders a table or decides how many
//! rows fit — a surface reads [`LoadProgress::snapshot`] and projects it.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static RSS_PEAK: AtomicU64 = AtomicU64::new(0);

/// `total` is not known yet: the producer is still discovering the plan.
const UNKNOWN_TOTAL: u64 = u64::MAX;

/// Declaration order is the order a table shows them in, so a stage that
/// finishes early does not move and a stage that never runs still holds its
/// place as a pending row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StageId {
    /// The map zone: opening it, walking it, and building what it described.
    MapAssets,
    /// `common_mp`, the startup zones and the donor namespaces.
    CommonAssets,
    /// The localized string zones beside the map.
    Localization,
    /// Decoding material images out of the archives.
    Images,
    /// The loadscreen picture behind the overlay.
    Preview,
    /// Moving the prepared match into the live world on the main thread.
    Install,
    /// Handing world images to `Assets<Image>`. Not the same as GPU-ready.
    WorldImages,
    /// Compiling material programs on the worker pool.
    Programs,
    /// Absorbing the compiled outcomes on the main thread.
    ProgramMerge,
    /// Admitting the absorbed ports as shaders.
    Shaders,
    /// Images actually resident on the GPU. A gauge, not a monotonic count.
    GpuTextures,
    /// Render pipelines warmed for the spawned world. Also a gauge.
    Pipelines,
    /// Consecutive frames drawn with nothing left compiling.
    RenderFrames,
    /// Converting the clips this match's aliases resolve to.
    Audio,
    /// Baking the bot navigation graph.
    Navigation,
    /// Waiting for the session to allow this client in.
    Admission,
}

impl StageId {
    pub const ALL: [Self; 16] = [
        Self::MapAssets,
        Self::CommonAssets,
        Self::Localization,
        Self::Images,
        Self::Preview,
        Self::Install,
        Self::WorldImages,
        Self::Programs,
        Self::ProgramMerge,
        Self::Shaders,
        Self::GpuTextures,
        Self::Pipelines,
        Self::RenderFrames,
        Self::Audio,
        Self::Navigation,
        Self::Admission,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MapAssets => "map_assets",
            Self::CommonAssets => "common_assets",
            Self::Localization => "localization",
            Self::Images => "images",
            Self::Preview => "preview",
            Self::Install => "install",
            Self::WorldImages => "world_images",
            Self::Programs => "programs",
            Self::ProgramMerge => "program_merge",
            Self::Shaders => "shaders",
            Self::GpuTextures => "gpu_textures",
            Self::Pipelines => "pipelines",
            Self::RenderFrames => "render_frames",
            Self::Audio => "audio",
            Self::Navigation => "navigation",
            Self::Admission => "admission",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The same kind of work runs in several passes — the map zone, `common_mp`, a
/// donor namespace — and each pass takes its own slot instead of overwriting
/// the one before it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StageScope {
    Whole,
    Named(Arc<str>),
}

impl StageScope {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Whole => "",
            Self::Named(name) => name,
        }
    }
}

impl From<&str> for StageScope {
    fn from(name: &str) -> Self {
        Self::Named(Arc::from(name))
    }
}

impl From<String> for StageScope {
    fn from(name: String) -> Self {
        Self::Named(Arc::from(name.as_str()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StageKey {
    pub id: StageId,
    pub scope: StageScope,
}

impl StageKey {
    pub fn new(id: StageId, scope: impl Into<StageScope>) -> Self {
        Self {
            id,
            scope: scope.into(),
        }
    }

    pub fn whole(id: StageId) -> Self {
        Self {
            id,
            scope: StageScope::Whole,
        }
    }

    pub fn label(&self) -> String {
        match &self.scope {
            StageScope::Whole => self.id.as_str().to_owned(),
            StageScope::Named(name) => format!("{}/{name}", self.id.as_str()),
        }
    }
}

/// How a stage ended, published by whoever owns the operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StageOutcome {
    /// The owner confirmed the result is ready at its own boundary.
    Done,
    /// The branch did not run at all.
    Skipped,
    /// The load was retargeted or aborted before this stage could finish.
    Cancelled,
    /// The handle went out of scope without anyone publishing a result. Not a
    /// success: it is a stage whose owner forgot to say how it went.
    Abandoned,
    Failed,
}

impl StageOutcome {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Done)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
            Self::Abandoned => "abandoned",
            Self::Failed => "failed",
        }
    }
}

/// Logical units this stage has processed, and how many there are in total.
///
/// `completed` counts units the producer has already handled — a skip and a
/// refusal count as processed just as a success does. Which of them it was is
/// the producer's own report, not this number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkCount {
    pub completed: u64,
    /// `None` while the producer is still discovering the plan. `Some(0)` is a
    /// known and empty one, which is not the same thing.
    pub total: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageEnd {
    pub at: Instant,
    pub outcome: StageOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageSnapshot {
    pub key: StageKey,
    pub started_at: Option<Instant>,
    /// `None` when no useful unit of work is defined for this stage.
    pub count: Option<WorkCount>,
    /// Bytes this stage has brought in, where the producer can weigh its work
    /// at all.
    pub bytes: Option<u64>,
    pub end: Option<StageEnd>,
    pub rss_delta: Option<i64>,
    pub heap_delta: Option<i64>,
}

impl StageSnapshot {
    pub fn pending(&self) -> bool {
        self.started_at.is_none() && self.end.is_none()
    }

    pub fn running(&self) -> bool {
        self.started_at.is_some() && self.end.is_none()
    }

    pub fn outcome(&self) -> Option<StageOutcome> {
        self.end.map(|end| end.outcome)
    }

    /// How long the stage has been alive, including whatever it waited on.
    pub fn elapsed(&self, now: Instant) -> Option<Duration> {
        let started = self.started_at?;
        Some(match self.end {
            Some(end) => end.at.saturating_duration_since(started),
            None => now.saturating_duration_since(started),
        })
    }
}

/// One consistent read of a whole load. `now` is sampled once, so every
/// running stage in it is measured against the same instant.
#[derive(Clone, Debug)]
pub struct LoadSnapshot {
    pub request_id: u64,
    pub requested_at: Instant,
    pub now: Instant,
    pub canceled: bool,
    pub stages: Vec<StageSnapshot>,
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
        proc_status_bytes("VmHWM:")
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

fn delta(started: Option<u64>, ended: Option<u64>) -> Option<i64> {
    let started = i64::try_from(started?).ok()?;
    let ended = i64::try_from(ended?).ok()?;
    Some(ended - started)
}

/// `at` is measured from the first stage that started, not from the request.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadLaneTiming {
    pub label: String,
    pub at: Duration,
    pub elapsed: Duration,
    pub done: u64,
    pub total: u64,
    pub running: bool,
    pub outcome: Option<StageOutcome>,
    pub rss_delta: Option<i64>,
    pub heap_delta: Option<i64>,
}

impl LoadLaneTiming {
    pub fn end(&self) -> Duration {
        self.at + self.elapsed
    }

    pub fn outcome_suffix(&self) -> &'static str {
        if self.running {
            return " (running)";
        }
        match self.outcome {
            None | Some(StageOutcome::Done) => "",
            Some(StageOutcome::Skipped) => " (skipped)",
            Some(StageOutcome::Cancelled) => " (canceled)",
            Some(StageOutcome::Abandoned) => " (interrupted)",
            Some(StageOutcome::Failed) => " (failed)",
        }
    }

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

/// The slot a handle writes into. Registered once at `begin`, so publishing
/// progress is a store into an already-owned allocation: no name lookup, no
/// scan of the other stages, no shared lock on the hot path.
struct StageSlot {
    key: StageKey,
    completed: AtomicU64,
    total: AtomicU64,
    /// Whether any unit of count is defined for this stage at all.
    counted: AtomicBool,
    bytes: AtomicU64,
    /// Whether the producer weighs its work in bytes at all.
    weighed: AtomicBool,
    life: Mutex<StageLife>,
}

struct StageLife {
    started_at: Option<Instant>,
    end: Option<StageEnd>,
    /// Taken under the same lock that records `end`, so a terminal read shows
    /// the outcome, the final count and the end time as one fact.
    final_count: Option<WorkCount>,
    final_bytes: Option<u64>,
    started_mem: MemSample,
    ended_mem: Option<MemSample>,
}

impl StageSlot {
    fn count(&self, life: &StageLife) -> Option<WorkCount> {
        if let Some(count) = life.final_count {
            return Some(count);
        }
        if !self.counted.load(Ordering::Relaxed) {
            return None;
        }
        Some(WorkCount {
            completed: self.completed.load(Ordering::Relaxed),
            total: match self.total.load(Ordering::Relaxed) {
                UNKNOWN_TOTAL => None,
                total => Some(total),
            },
        })
    }

    fn bytes(&self, life: &StageLife) -> Option<u64> {
        if let Some(bytes) = life.final_bytes {
            return Some(bytes);
        }
        if !self.weighed.load(Ordering::Relaxed) {
            return None;
        }
        Some(self.bytes.load(Ordering::Relaxed))
    }

    fn snapshot(&self) -> StageSnapshot {
        let life = self
            .life
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let ended_mem = life.ended_mem;
        StageSnapshot {
            key: self.key.clone(),
            started_at: life.started_at,
            count: self.count(&life),
            bytes: self.bytes(&life),
            end: life.end,
            rss_delta: ended_mem.and_then(|ended| delta(life.started_mem.rss, ended.rss)),
            heap_delta: ended_mem.and_then(|ended| delta(life.started_mem.heap, ended.heap)),
        }
    }
}

#[derive(Clone)]
pub struct LoadProgress(Arc<Inner>);

impl Default for LoadProgress {
    fn default() -> Self {
        Self::new(0)
    }
}

struct Inner {
    request_id: u64,
    requested_at: Instant,
    slots: Mutex<Vec<Arc<StageSlot>>>,
    measurement: Mutex<LoadMeasurement>,
    canceled: Arc<AtomicBool>,
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

impl LoadProgress {
    /// A load request's own process. A new request takes a new one rather than
    /// clearing this one: tasks from the map before it still hold handles into
    /// their own slots and must not be able to write into the next map's rows.
    pub fn new(request_id: u64) -> Self {
        Self(Arc::new(Inner {
            request_id,
            requested_at: Instant::now(),
            slots: Mutex::new(Vec::new()),
            measurement: Mutex::new(LoadMeasurement::default()),
            canceled: Arc::new(AtomicBool::new(false)),
        }))
    }

    pub fn request_id(&self) -> u64 {
        self.0.request_id
    }

    /// The one origin the whole process is measured from. Milestones are
    /// offsets from it; a stage's own elapsed is not.
    pub fn requested_at(&self) -> Instant {
        self.0.requested_at
    }

    pub fn cancel(&self) {
        self.0.canceled.store(true, Ordering::Relaxed);
    }

    pub fn is_canceled(&self) -> bool {
        self.0.canceled.load(Ordering::Relaxed)
    }

    /// Open a stage. `total` is the plan size when the producer already knows
    /// it, and `None` when it does not — either because the plan is still being
    /// discovered or because this stage has no unit worth counting.
    pub fn begin(&self, id: StageId, total: Option<u64>) -> StageHandle {
        self.begin_keyed(StageKey::whole(id), total)
    }

    pub fn begin_scoped(
        &self,
        id: StageId,
        scope: impl Into<StageScope>,
        total: Option<u64>,
    ) -> StageHandle {
        self.begin_keyed(StageKey::new(id, scope), total)
    }

    pub fn begin_keyed(&self, key: StageKey, total: Option<u64>) -> StageHandle {
        let slot = Arc::new(StageSlot {
            key,
            completed: AtomicU64::new(0),
            total: AtomicU64::new(total.unwrap_or(UNKNOWN_TOTAL)),
            counted: AtomicBool::new(total.is_some()),
            bytes: AtomicU64::new(0),
            weighed: AtomicBool::new(false),
            life: Mutex::new(StageLife {
                started_at: Some(Instant::now()),
                end: None,
                final_count: None,
                final_bytes: None,
                started_mem: MemSample::now(),
                ended_mem: None,
            }),
        });
        self.register(Arc::clone(&slot));
        StageHandle {
            slot,
            canceled: Arc::clone(&self.0.canceled),
            published: false,
        }
    }

    pub fn record_skipped(&self, id: StageId) {
        self.record_skipped_keyed(StageKey::whole(id));
    }

    pub fn record_skipped_scoped(&self, id: StageId, scope: impl Into<StageScope>) {
        self.record_skipped_keyed(StageKey::new(id, scope));
    }

    pub fn record_skipped_keyed(&self, key: StageKey) {
        let slot = Arc::new(StageSlot {
            key,
            completed: AtomicU64::new(0),
            total: AtomicU64::new(UNKNOWN_TOTAL),
            counted: AtomicBool::new(false),
            bytes: AtomicU64::new(0),
            weighed: AtomicBool::new(false),
            life: Mutex::new(StageLife {
                started_at: None,
                end: Some(StageEnd {
                    at: Instant::now(),
                    outcome: StageOutcome::Skipped,
                }),
                final_count: None,
                final_bytes: None,
                started_mem: MemSample::default(),
                ended_mem: None,
            }),
        });
        self.register(slot);
    }

    fn register(&self, slot: Arc<StageSlot>) {
        if let Ok(mut slots) = self.0.slots.lock() {
            slots.push(slot);
        }
    }

    /// Every stage this load has opened, in the order it opened them, read
    /// against a single `now`.
    pub fn snapshot(&self) -> LoadSnapshot {
        let stages = match self.0.slots.lock() {
            Ok(slots) => slots.iter().map(|slot| slot.snapshot()).collect(),
            Err(_) => Vec::new(),
        };
        LoadSnapshot {
            request_id: self.0.request_id,
            requested_at: self.0.requested_at,
            now: Instant::now(),
            canceled: self.is_canceled(),
            stages,
        }
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

    /// Every stage with the window it occupied. Stages run in parallel on the
    /// load pool, so the offsets are what says which of them overlapped.
    pub fn lane_timings(&self) -> Vec<LoadLaneTiming> {
        let snapshot = self.snapshot();
        let origin = snapshot
            .stages
            .iter()
            .filter_map(|stage| stage.started_at)
            .min()
            .unwrap_or(snapshot.requested_at);
        snapshot
            .stages
            .iter()
            .filter(|stage| stage.started_at.is_some())
            .map(|stage| {
                let count = stage.count.unwrap_or(WorkCount {
                    completed: 0,
                    total: Some(0),
                });
                LoadLaneTiming {
                    label: stage.key.label(),
                    at: stage
                        .started_at
                        .map(|at| at.saturating_duration_since(origin))
                        .unwrap_or(Duration::ZERO),
                    elapsed: stage.elapsed(snapshot.now).unwrap_or(Duration::ZERO),
                    done: count.completed,
                    total: count.total.unwrap_or(0),
                    running: stage.running(),
                    outcome: stage.outcome(),
                    rss_delta: stage.rss_delta,
                    heap_delta: stage.heap_delta,
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
            let outcome = match lane.outcome {
                Some(StageOutcome::Done) | None => String::new(),
                Some(outcome) => format!(" {}", outcome.as_str()),
            };
            rows.push(format!(
                "load stage: {} at=+{:.0}ms {:.1}ms{items}{}{outcome}{}",
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
}

/// A producer's write end of one stage.
///
/// It owns its slot outright, so `advance` is an atomic add and nothing else —
/// no formatting, no logging, no RSS read, no directory walk. Publishing the
/// result is the owner's explicit call: dropping the handle without one is
/// recorded as [`StageOutcome::Abandoned`], never as success.
pub struct StageHandle {
    slot: Arc<StageSlot>,
    canceled: Arc<AtomicBool>,
    published: bool,
}

impl StageHandle {
    pub fn key(&self) -> &StageKey {
        &self.slot.key
    }

    /// The plan is closed and this is its size. `0` is a known empty plan.
    pub fn set_total(&self, total: u64) {
        self.slot.total.store(total, Ordering::Relaxed);
        self.slot.counted.store(true, Ordering::Relaxed);
    }

    /// The plan is still open: what has been done is known, the whole is not.
    pub fn clear_total(&self) {
        self.slot.total.store(UNKNOWN_TOTAL, Ordering::Relaxed);
        self.slot.counted.store(true, Ordering::Relaxed);
    }

    /// Units this producer has finished handling. Called after the work, never
    /// before it.
    pub fn advance(&self, delta: u64) {
        self.slot.completed.fetch_add(delta, Ordering::Relaxed);
        self.slot.counted.store(true, Ordering::Relaxed);
    }

    /// The producer already has the number — a worker atomic, a gauge of what
    /// is currently ready — and sets it outright. A gauge may fall.
    pub fn set_completed(&self, completed: u64) {
        self.slot.completed.store(completed, Ordering::Relaxed);
        self.slot.counted.store(true, Ordering::Relaxed);
    }

    /// Bytes this producer has just brought in — decoded texels, a zone image
    /// read, samples made resident. Independent of the count: a stage may
    /// publish both.
    pub fn add_bytes(&self, delta: u64) {
        self.slot.bytes.fetch_add(delta, Ordering::Relaxed);
        self.slot.weighed.store(true, Ordering::Relaxed);
    }

    pub fn set_bytes(&self, bytes: u64) {
        self.slot.bytes.store(bytes, Ordering::Relaxed);
        self.slot.weighed.store(true, Ordering::Relaxed);
    }

    pub fn completed(&self) -> u64 {
        self.slot.completed.load(Ordering::Relaxed)
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::Relaxed)
    }

    /// Publish how this stage ended. Reaching `completed == total` is not this:
    /// the owner says so when the result is actually ready at its own boundary.
    pub fn finish(mut self, outcome: StageOutcome) {
        self.publish(outcome);
    }

    pub fn done(self) {
        self.finish(StageOutcome::Done);
    }

    pub fn fail(self) {
        self.finish(StageOutcome::Failed);
    }

    pub fn cancel(self) {
        self.finish(StageOutcome::Cancelled);
    }

    pub fn skip(self) {
        self.finish(StageOutcome::Skipped);
    }

    pub fn finish_from<T, E>(self, result: &Result<T, E>) {
        self.finish(match result {
            Ok(_) => StageOutcome::Done,
            Err(_) => StageOutcome::Failed,
        });
    }

    fn publish(&mut self, outcome: StageOutcome) {
        if self.published {
            return;
        }
        self.published = true;
        let at = Instant::now();
        let ended_mem = MemSample::now();
        let (count, elapsed, started_mem) = {
            let mut life = self
                .slot
                .life
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if life.end.is_some() {
                return;
            }
            let count = self.slot.count(&life);
            let bytes = self.slot.bytes(&life);
            life.final_count = count;
            life.final_bytes = bytes;
            life.ended_mem = Some(ended_mem);
            life.end = Some(StageEnd { at, outcome });
            let elapsed = life
                .started_at
                .map(|started| at.saturating_duration_since(started))
                .unwrap_or(Duration::ZERO);
            (count, elapsed, life.started_mem)
        };
        let items = match count {
            None => String::new(),
            Some(WorkCount {
                completed,
                total: None,
            }) => format!(" items={completed}/?"),
            Some(WorkCount {
                completed,
                total: Some(total),
            }) => format!(" items={completed}/{total}"),
        };
        diag::info!(
            World,
            "load stage: {} {} {:.1}ms{items}{}",
            self.slot.key.label(),
            outcome.as_str(),
            elapsed.as_secs_f32() * 1000.0,
            MemSample::suffix(started_mem, ended_mem),
        );
    }
}

impl Drop for StageHandle {
    fn drop(&mut self) {
        // A handle that unwinds out of a canceled load was interrupted, not
        // forgotten — the owner never reached its publish because the load was
        // retargeted under it.
        self.publish(if self.is_canceled() {
            StageOutcome::Cancelled
        } else {
            StageOutcome::Abandoned
        });
    }
}
