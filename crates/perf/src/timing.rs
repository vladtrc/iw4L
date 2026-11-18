//! Small, always-on history for clip diagnostics. Independent of Perfetto recording.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(45);
// Bounds memory even with an uncapped frame rate; dumps expose cap eviction.
const MAX_SAMPLES: usize = 65_536;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Frame,
    Simulation,
}
struct Sample {
    end: Instant,
    kind: Kind,
    frame: u64,
    tick: Option<u32>,
    ms: f64,
}
#[derive(Clone, Copy)]
pub(crate) struct FrameBoundary {
    instant: Instant,
    #[cfg(all(unix, feature = "native"))]
    pub(crate) trace: Option<perfetto_sdk::track_event::TrackEventTimestamp>,
}

impl FrameBoundary {
    pub(crate) fn now() -> Self {
        Self {
            instant: Instant::now(),
            #[cfg(all(unix, feature = "native"))]
            trace: crate::session::frame_timestamp(),
        }
    }

    fn elapsed_since(self, previous: Self) -> Duration {
        #[cfg(all(unix, feature = "native"))]
        if let (Some(now), Some(before)) = (self.trace, previous.trace)
            && now.clock_id() == before.clock_id()
            && let Some(ns) = now.timestamp().checked_sub(before.timestamp())
        {
            return Duration::from_nanos(ns);
        }
        self.instant.duration_since(previous.instant)
    }
}

#[derive(Default)]
struct History {
    frame_start: Option<FrameBoundary>,
    samples: VecDeque<Sample>,
    last_capacity_eviction: Option<Instant>,
}
static HISTORY: Mutex<Option<History>> = Mutex::new(None);
impl History {
    fn prune(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|s| now.duration_since(s.end) > WINDOW)
        {
            self.samples.pop_front();
        }
    }
    fn push(&mut self, sample: Sample) {
        self.prune(sample.end);
        if self.samples.len() == MAX_SAMPLES {
            self.last_capacity_eviction = Some(sample.end);
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
}
pub(crate) fn advance_frame(frame: u64) -> FrameBoundary {
    let mut guard = HISTORY.lock().unwrap_or_else(|p| p.into_inner());
    let history = guard.get_or_insert_with(History::default);
    let now = FrameBoundary::now();
    if let Some(previous) = history.frame_start.replace(now) {
        history.push(Sample {
            end: now.instant,
            kind: Kind::Frame,
            frame,
            tick: None,
            ms: now.elapsed_since(previous).as_secs_f64() * 1000.,
        });
    }
    now
}
/// Elapsed time inside the synchronous simulation core, including preemption.
/// This is not OS thread CPU time, render time or the fixed tick interval.
pub struct SimulationTimer {
    started: Instant,
    tick: u32,
}
impl SimulationTimer {
    pub fn start(tick: u32) -> Self {
        Self {
            started: Instant::now(),
            tick,
        }
    }
}
impl Drop for SimulationTimer {
    fn drop(&mut self) {
        let mut guard = HISTORY.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let ms = now.duration_since(self.started).as_secs_f64() * 1000.;
        guard.get_or_insert_with(History::default).push(Sample {
            end: now,
            kind: Kind::Simulation,
            frame: crate::frames::open_index(),
            tick: Some(self.tick),
            ms,
        });
    }
}
fn statistics(samples: &[Sample], kind: Kind) -> serde_json::Value {
    let mut values: Vec<_> = samples
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| s.ms)
        .collect();
    if values.is_empty() {
        return serde_json::json!({"samples":0});
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values.sort_by(f64::total_cmp);
    // Nearest-rank percentiles: retain actual long frames rather than interpolating.
    let percentile = |p: usize| values[(values.len() * p).div_ceil(100).saturating_sub(1)];
    let mut result = serde_json::json!({"samples":values.len(),"mean_ms":mean,"p50_ms":percentile(50),"p95_ms":percentile(95),"p99_ms":percentile(99),"max_ms":values.last()});
    if kind == Kind::Frame && mean > 0. {
        result["mean_fps"] = serde_json::json!(1000. / mean);
    }
    result
}
pub fn snapshot() -> serde_json::Value {
    let (samples, truncated, now) = {
        let mut guard = HISTORY.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let history = guard.get_or_insert_with(History::default);
        history.prune(now);
        let truncated = history
            .last_capacity_eviction
            .is_some_and(|at| now.duration_since(at) <= WINDOW);
        let samples: Vec<_> = history
            .samples
            .iter()
            .map(|s| Sample {
                end: s.end,
                kind: s.kind,
                frame: s.frame,
                tick: s.tick,
                ms: s.ms,
            })
            .collect();
        (samples, truncated, now)
    };
    serde_json::json!({
        "window_wall_seconds": WINDOW.as_secs(),
        "capacity_truncated":truncated,
        "frame_wall":statistics(&samples, Kind::Frame),
        "simulation_core":statistics(&samples, Kind::Simulation),
        "notes":"frame_wall measures main-loop First-to-First cadence, including waits; simulation_core measures synchronous sim::run_frame elapsed time, including preemption. Recording uses the same sampled frame boundary for history and wall spans. Neither measures OS thread CPU time or GPU execution. Ages are relative to this capture; ticks can restart after a map reload.",
        "series_columns":["end_age_ms","frame_index","simulation_tick","frame_wall_ms","simulation_core_ms"],
        "series":samples.iter().map(|s| serde_json::json!([
            now.duration_since(s.end).as_secs_f64()*1000.,s.frame,s.tick,
            if s.kind==Kind::Frame {Some(s.ms)} else {None},
            if s.kind==Kind::Simulation {Some(s.ms)} else {None}
        ])).collect::<Vec<_>>()
    })
}
