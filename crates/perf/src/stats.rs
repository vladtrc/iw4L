//! In-process span statistics: what `make bench` reads.
//!
//! Perfetto answers "what happened in this one frame"; this answers "where did
//! the time go over the whole run", without a trace file and without Trace
//! Processor. It is the same instrumentation — every [`Span`] begin/end already
//! in the tree feeds both — so there is no second set of call sites to keep in
//! step.
//!
//! Off unless `IW4L_BENCH` is set, and off it costs one relaxed load per
//! begin/end. On, it costs two `Instant::now()` and a handful of relaxed atomic
//! adds per span instance: no allocation, no lock, nothing to flush.
//!
//! Durations land in a log-scale histogram, [`SUB`] buckets per octave, so a
//! percentile is exact to within [`PRECISION`] of its own value. Counts, sums,
//! minima and maxima are exact.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use crate::vocabulary_types::Span;

const ENV: &str = "IW4L_BENCH";

/// Sub-buckets per octave, as a shift. Four bits is sixteen buckets per octave.
const SUB_BITS: u32 = 4;
const SUB: u64 = 1 << SUB_BITS;
/// One row of `SUB` buckets for values below `SUB`, then one row per octave up
/// to the largest `u64` low edge that does not overflow.
const BUCKETS: usize = 61 * SUB as usize;

/// Worst-case relative error of a percentile read out of the histogram: half a
/// bucket width over that bucket's low edge.
pub const PRECISION: f64 = 0.5 / SUB as f64;

/// Which half of the run a sample belongs to. The two are independent reports:
/// [`Phase::Load`] is everything up to the first playable frame, [`Phase::Live`]
/// the frames after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Load,
    Live,
}

impl Phase {
    const COUNT: usize = 2;

    const fn index(self) -> usize {
        match self {
            Self::Load => 0,
            Self::Live => 1,
        }
    }
}

const NO_PARENT: u8 = u8::MAX;

struct Histogram {
    count: AtomicU64,
    sum_ns: AtomicU64,
    min_ns: AtomicU64,
    max_ns: AtomicU64,
    buckets: [AtomicU32; BUCKETS],
}

impl Histogram {
    #[allow(clippy::declare_interior_mutable_const)]
    const ZERO: Self = Self {
        count: AtomicU64::new(0),
        sum_ns: AtomicU64::new(0),
        min_ns: AtomicU64::new(u64::MAX),
        max_ns: AtomicU64::new(0),
        buckets: [const { AtomicU32::new(0) }; BUCKETS],
    };

    #[inline]
    fn record(&self, ns: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_ns.fetch_add(ns, Ordering::Relaxed);
        self.min_ns.fetch_min(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
        self.buckets[bucket_of(ns)].fetch_add(1, Ordering::Relaxed);
    }
}

static ARMED: AtomicBool = AtomicBool::new(false);
static PHASE: AtomicU8 = AtomicU8::new(0);
static PARENT: [AtomicU8; Span::COUNT] = [const { AtomicU8::new(NO_PARENT) }; Span::COUNT];
static PARENT_VARIES: [AtomicBool; Span::COUNT] = [const { AtomicBool::new(false) }; Span::COUNT];
static UNMATCHED_END: AtomicU64 = AtomicU64::new(0);
static REOPENED: AtomicU64 = AtomicU64::new(0);
static HISTOGRAMS: [[Histogram; Span::COUNT]; Phase::COUNT] =
    [const { [const { Histogram::ZERO }; Span::COUNT] }; Phase::COUNT];

/// When each span opened, as nanoseconds since [`BASE`] plus one; zero is
/// closed. Global rather than thread-local on purpose: Bevy runs `FixedUpdate`,
/// `PreUpdate` and `PostUpdate` on the task pool, so a span's begin and its end
/// are not promised the same thread — which is also why the Perfetto side gives
/// every span its own named track. One slot per span is enough because a span
/// is a single scope: it is never open twice at once.
static OPEN_AT: [AtomicU64; Span::COUNT] = [const { AtomicU64::new(0) }; Span::COUNT];

/// The zero of the recorder's clock, fixed by [`arm`] before the first span.
static BASE: OnceLock<Instant> = OnceLock::new();

/// Read `IW4L_BENCH` once, before the first span. Until this is called nothing
/// is recorded.
pub fn arm() {
    let _ = BASE.set(Instant::now());
    ARMED.store(env_enabled(), Ordering::Relaxed);
}

/// Nanoseconds since the recorder's zero. Wraps in 584 years.
#[inline]
fn now_ns() -> u64 {
    BASE.get().map_or(0, |base| {
        u64::try_from(base.elapsed().as_nanos()).unwrap_or(u64::MAX)
    })
}

/// Whether this process was asked to collect bench statistics.
#[inline]
pub fn enabled() -> bool {
    ARMED.load(Ordering::Relaxed)
}

fn env_enabled() -> bool {
    match std::env::var(ENV) {
        Ok(value)
            if value.is_empty()
                || value == "0"
                || value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("off") =>
        {
            false
        }
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Everything recorded from here on belongs to [`Phase::Live`]. Called once the
/// map is on screen; before it, the samples are the cost of getting there.
pub fn mark_live() {
    PHASE.store(Phase::Live.index() as u8, Ordering::Relaxed);
}

#[inline]
pub(crate) fn begin(span: Span) {
    if !enabled() {
        return;
    }
    if span != Span::FramesWallFrameMs {
        remember_parent(span, innermost_open());
    }
    if OPEN_AT[span as usize].swap(now_ns() + 1, Ordering::Relaxed) != 0 {
        REOPENED.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn end(span: Span) {
    if !enabled() {
        return;
    }
    let now = now_ns();
    let started = OPEN_AT[span as usize].swap(0, Ordering::Relaxed);
    if started == 0 {
        UNMATCHED_END.fetch_add(1, Ordering::Relaxed);
        return;
    }
    record(span, now.saturating_sub(started - 1));
}

/// The open span that began most recently — the one this span is nesting
/// inside. `wall` is excluded: it is the frame clock, opened in one frame and
/// closed in the next, so it encloses no scope and is the root of the printed
/// tree instead of a parent in it.
fn innermost_open() -> Option<Span> {
    let mut innermost: Option<(u64, Span)> = None;
    for span in Span::ALL {
        if span == Span::FramesWallFrameMs {
            continue;
        }
        let at = OPEN_AT[span as usize].load(Ordering::Relaxed);
        if at != 0 && innermost.is_none_or(|(latest, _)| at > latest) {
            innermost = Some((at, span));
        }
    }
    innermost.map(|(_, span)| span)
}

fn remember_parent(span: Span, parent: Option<Span>) {
    let seen = parent.map_or(NO_PARENT, |parent| parent as u8);
    let slot = &PARENT[span as usize];
    match slot.compare_exchange(NO_PARENT, seen, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => {}
        Err(known) if known == seen => {}
        Err(_) => PARENT_VARIES[span as usize].store(true, Ordering::Relaxed),
    }
}

#[inline]
fn record(span: Span, ns: u64) {
    let phase = (PHASE.load(Ordering::Relaxed) as usize).min(Phase::COUNT - 1);
    HISTOGRAMS[phase][span as usize].record(ns);
}

/// Values below [`SUB`] are their own bucket; above it, `SUB` buckets cover
/// each octave.
fn bucket_of(ns: u64) -> usize {
    if ns < SUB {
        return ns as usize;
    }
    let octave = u64::from(63 - ns.leading_zeros()) - u64::from(SUB_BITS);
    let sub = (ns >> octave) & (SUB - 1);
    ((octave + 1) * SUB + sub) as usize
}

fn bucket_low_ns(index: usize) -> u64 {
    let index = index as u64;
    if index < SUB {
        return index;
    }
    let octave = u32::try_from(index / SUB - 1).unwrap_or(u32::MAX);
    let step = 1u64.checked_shl(octave).unwrap_or(u64::MAX);
    (SUB | (index % SUB)).saturating_mul(step)
}

/// What one span cost over one phase. Times are nanoseconds; the percentiles
/// are bucket midpoints, exact to [`PRECISION`].
#[derive(Clone, Copy, Debug)]
pub struct SpanStats {
    pub span: Span,
    pub parent: Option<Span>,
    pub parent_varies: bool,
    pub count: u64,
    pub sum_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
}

impl SpanStats {
    pub fn avg_ns(&self) -> u64 {
        self.sum_ns.checked_div(self.count).unwrap_or(0)
    }
}

/// Every span that produced at least one sample in `phase`, in declaration
/// order. An empty result means the phase was never reached.
pub fn snapshot(phase: Phase) -> Vec<SpanStats> {
    let histograms = &HISTOGRAMS[phase.index()];
    Span::ALL
        .into_iter()
        .filter_map(|span| {
            let histogram = &histograms[span as usize];
            let count = histogram.count.load(Ordering::Relaxed);
            if count == 0 {
                return None;
            }
            let quantiles = quantiles(histogram, count, &[0.50, 0.95, 0.99]);
            Some(SpanStats {
                span,
                parent: parent_of(span),
                parent_varies: PARENT_VARIES[span as usize].load(Ordering::Relaxed),
                count,
                sum_ns: histogram.sum_ns.load(Ordering::Relaxed),
                min_ns: histogram.min_ns.load(Ordering::Relaxed),
                max_ns: histogram.max_ns.load(Ordering::Relaxed),
                p50_ns: quantiles[0],
                p95_ns: quantiles[1],
                p99_ns: quantiles[2],
            })
        })
        .collect()
}

fn parent_of(span: Span) -> Option<Span> {
    let parent = PARENT[span as usize].load(Ordering::Relaxed);
    if parent == NO_PARENT {
        return None;
    }
    Span::ALL
        .into_iter()
        .find(|candidate| *candidate as u8 == parent)
}

fn quantiles(histogram: &Histogram, count: u64, wanted: &[f64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(wanted.len());
    let mut cursor = 0usize;
    // Samples in the buckets *before* `cursor`, so a later quantile can resume
    // from here without counting the bucket it stopped on twice.
    let mut seen = 0u64;
    for quantile in wanted {
        let rank = ((count as f64 * quantile).ceil() as u64).clamp(1, count);
        while cursor < BUCKETS {
            let n = u64::from(histogram.buckets[cursor].load(Ordering::Relaxed));
            if seen + n >= rank {
                break;
            }
            seen += n;
            cursor += 1;
        }
        let index = cursor.min(BUCKETS - 1);
        let low = bucket_low_ns(index);
        let high = bucket_low_ns(index + 1).max(low);
        out.push(low + (high - low) / 2);
    }
    out
}

/// Bookkeeping the report prints, so a reader can tell a real number from one
/// the recorder could not close.
#[derive(Clone, Copy, Debug, Default)]
pub struct Anomalies {
    pub unmatched_end: u64,
    pub reopened: u64,
}

pub fn anomalies() -> Anomalies {
    Anomalies {
        unmatched_end: UNMATCHED_END.load(Ordering::Relaxed),
        reopened: REOPENED.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_lands_in_a_bucket_that_contains_it() {
        for ns in [
            0,
            1,
            15,
            16,
            17,
            31,
            32,
            1_000,
            1_000_000,
            16 * 1_000_000,
            u64::MAX / 2,
        ] {
            let index = bucket_of(ns);
            assert!(index < BUCKETS, "{ns} landed outside the histogram");
            let low = bucket_low_ns(index);
            let high = bucket_low_ns(index + 1);
            assert!(low <= ns, "{ns} is below its bucket's low edge {low}");
            assert!(
                ns < high,
                "{ns} is at or above its bucket's high edge {high}"
            );
        }
    }

    #[test]
    fn bucket_low_edges_never_go_backwards() {
        let mut previous = 0;
        for index in 1..BUCKETS {
            let low = bucket_low_ns(index);
            assert!(low > previous, "bucket {index} low {low} <= {previous}");
            previous = low;
        }
    }

    #[test]
    fn percentiles_read_back_the_values_that_were_recorded() {
        let histogram = Histogram::ZERO;
        for ms in 1..=100u64 {
            histogram.record(ms * 1_000_000);
        }
        let read = quantiles(&histogram, 100, &[0.50, 0.95, 0.99]);
        for (got, want) in read.into_iter().zip([50u64, 95, 99]) {
            let want = want * 1_000_000;
            let error = (got as f64 - want as f64).abs() / want as f64;
            assert!(error <= PRECISION, "read {got} for {want}: off by {error}");
        }
    }

    #[test]
    fn sum_count_and_extremes_stay_exact() {
        let histogram = Histogram::ZERO;
        for ns in [7u64, 1_234_567, 42, 999_999_999] {
            histogram.record(ns);
        }
        assert_eq!(histogram.count.load(Ordering::Relaxed), 4);
        assert_eq!(
            histogram.sum_ns.load(Ordering::Relaxed),
            7 + 1_234_567 + 42 + 999_999_999
        );
        assert_eq!(histogram.min_ns.load(Ordering::Relaxed), 7);
        assert_eq!(histogram.max_ns.load(Ordering::Relaxed), 999_999_999);
    }
}
