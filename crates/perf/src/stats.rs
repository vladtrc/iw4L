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
//!
//! What a span cost and which frame it cost it in are two different questions.
//! The histogram here answers the first with the span's whole elapsed time. The
//! per-frame table answers the second, and takes only the part of the span that
//! overlapped the frame — see [`frames`](crate::frames).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use crate::vocabulary_types::{Counter, Origin, Span, Unit};

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

/// Counters get the same histogram as spans, one per phase. A [`Unit::Count`]
/// sample is stored as the count itself and a [`Unit::Milliseconds`] one as
/// nanoseconds, so both keep the histogram's precision without a second scale.
static COUNTERS: [[Histogram; Counter::COUNT]; Phase::COUNT] =
    [const { [const { Histogram::ZERO }; Counter::COUNT] }; Phase::COUNT];

/// Samples an emitter handed us that the histogram cannot hold: NaN, infinity
/// or a negative value. Counted rather than clamped, because a counter that
/// goes negative is a bug in the emitter and silently recording a zero for it
/// would hide that behind a plausible number.
static COUNTER_REJECTED: [AtomicU64; Counter::COUNT] =
    [const { AtomicU64::new(0) }; Counter::COUNT];

/// Samples that named the frame they were measured in and found no row for it.
/// Counted rather than charged to whatever frame is open, which would put a
/// render stage's cost on the row after the one that paid it.
static COUNTER_UNATTRIBUTED: [AtomicU64; Counter::COUNT] =
    [const { AtomicU64::new(0) }; Counter::COUNT];

/// When each span opened, as nanoseconds since [`BASE`] plus one; zero is
/// closed. Global rather than thread-local on purpose: Bevy runs `FixedUpdate`,
/// `PreUpdate` and `PostUpdate` on the task pool, so a span's begin and its end
/// are not promised the same thread — which is also why the Perfetto side gives
/// every span its own named track. One slot per span is enough because a span
/// is a single scope: it is never open twice at once. Nesting is *not* read out
/// of this table; see [`STACK`].
static OPEN_AT: [AtomicU64; Span::COUNT] = [const { AtomicU64::new(0) }; Span::COUNT];

/// Spans open on *this* thread, innermost last.
///
/// Nesting is a property of a call stack, and a call stack belongs to a thread.
/// Reading the innermost span out of the global table instead made the render
/// thread's spans the parents of whatever the main thread opened next, which is
/// how a schedule-level span acquired a child it never called.
///
/// What this stack is *not* is where the frame's coverage roots come from. A
/// schedule span opens in one system and closes in another, and the executor
/// is free to run those on two different workers: the close then finds nothing
/// to pop here and the open is left behind for the rest of the process,
/// adopting every span that thread opens afterwards. [`Span::coverage_root`]
/// declares the roots instead, and [`prune`] drops what a migration left here.
///
/// Sixteen is deeper than the tree gets; overflowing it loses the parent name
/// for that one span rather than corrupting the stack.
const DEPTH: usize = 16;

thread_local! {
    static STACK: std::cell::RefCell<Vec<Span>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Spans that closed on a thread where they were not the innermost open span.
static MISNESTED: AtomicU64 = AtomicU64::new(0);

/// Spans whose `end` ran on a thread their `begin` never touched. The executor
/// moved the two systems apart; the span's own timing is unaffected, and what
/// it costs is the parent this thread could have named for it.
static MIGRATED: AtomicU64 = AtomicU64::new(0);

/// The zero of the recorder's clock, fixed by [`arm`] before the first span.
static BASE: OnceLock<Instant> = OnceLock::new();

/// Read `IW4L_BENCH` once, before the first span. Until this is called nothing
/// is recorded.
pub fn arm() {
    let _ = BASE.set(Instant::now());
    let on = env_enabled();
    ARMED.store(on, Ordering::Relaxed);
    crate::frames::arm(on);
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
    crate::switch::on(ENV)
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
    let at = now_ns();
    if span == Span::FramesWallFrameMs {
        // The frame clock is the row's wall, not a scope inside it: it takes no
        // parent and it is what every other span is clipped against.
        crate::frames::open(at);
    } else {
        remember_parent(span, innermost_open());
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.len() < DEPTH {
                stack.push(span);
            }
        });
    }
    if OPEN_AT[span as usize].swap(at + 1, Ordering::Relaxed) != 0 {
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
    let started = started - 1;
    let ns = now.saturating_sub(started);
    // The histogram keeps the span's whole elapsed time — that is what the span
    // cost. The frame row keeps only the part that ran inside the frame.
    record(span, ns);
    // The frame clock is the row, not a span inside it, and it closes last:
    // draining the accumulators before the frame's own spans had ended would
    // charge them to the frame after.
    if span == Span::FramesWallFrameMs {
        crate::frames::close(started, now, phase_index() as u8);
    } else {
        pop(span);
        crate::frames::add_span(span, started, now, span.coverage_root());
    }
}

/// Close the frame clock if it is still open, so the last frame of the run is
/// a row like every other one.
///
/// The clock closes and reopens at the top of `First`, which means the frame a
/// run exits from has never been closed: its spans sit in the accumulators and
/// reach no row. Called once, from whoever is about to read the table.
/// `partial` is raised on the row, because the frame was cut at exit and its
/// wall is not a frame time anyone can compare.
pub fn close_open_frame() {
    if !enabled() || OPEN_AT[Span::FramesWallFrameMs as usize].load(Ordering::Relaxed) == 0 {
        return;
    }
    crate::frames::mark(crate::frames::flag::PARTIAL);
    Span::FramesWallFrameMs.end();
}

/// Take `span` off this thread's stack, so the spans opened after it are not
/// left claiming it as their parent.
///
/// A span that is not on top closed out of order, which the Perfetto side would
/// show as a crossed pair. It is counted and removed wherever it sits rather
/// than left behind to become a false parent for the rest of the frame.
///
/// A span that is not on this thread's stack at all opened somewhere else — a
/// schedule whose `begin` and `end` the executor ran on two workers — or
/// deeper than [`DEPTH`]. That is counted too, and costs nothing but the
/// parent this thread could have named: what the frame's coverage is built
/// from is declared by [`Span::coverage_root`], not read out of here.
#[inline]
fn pop(span: Span) {
    STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        prune(&mut stack);
        let Some(at) = stack.iter().rposition(|open| *open == span) else {
            MIGRATED.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if at + 1 != stack.len() {
            MISNESTED.fetch_add(1, Ordering::Relaxed);
        }
        stack.remove(at);
    });
}

/// Drop spans this thread opened and another one closed.
///
/// [`OPEN_AT`] is the process-wide answer to "is this span open at all", and a
/// zero there means somebody closed it. Without this the entry stays on the
/// thread that opened it for the rest of the run and becomes the parent of
/// every span that thread opens next — which is how `PreUpdate`, opened in
/// `First` and closed in `RunFixedMainLoop`, comes to contain `Update`.
#[inline]
fn prune(stack: &mut Vec<Span>) {
    stack.retain(|span| OPEN_AT[*span as usize].load(Ordering::Relaxed) != 0);
}

/// The span this one is opening inside: the innermost still open *on this
/// thread*. `None` is a root — the top of a schedule, or work on a thread whose
/// enclosing span belongs to another one.
#[inline]
fn innermost_open() -> Option<Span> {
    STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        prune(&mut stack);
        stack.last().copied()
    })
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

/// One counter sample, in whatever unit [`Counter::unit`] declares. Called
/// from `Counter::emit` before the Perfetto category gate, so the bench report
/// sees the value whether or not a trace is being written.
#[inline]
pub(crate) fn count(counter: Counter, value: f64) {
    let Some(stored) = store(counter, value) else {
        return;
    };
    crate::frames::add_counter(counter, stored);
}

/// Record a counter against the frame it was measured in rather than the one
/// that happened to be open when it was reported.
pub(crate) fn count_at(counter: Counter, value: f64, frame: u64) {
    let Some(stored) = store(counter, value) else {
        return;
    };
    if !crate::frames::add_counter_at(frame, counter, stored) {
        COUNTER_UNATTRIBUTED[counter as usize].fetch_add(1, Ordering::Relaxed);
    }
}

/// Into the histogram, and out with what the frame row stores. `None` when the
/// value was rejected or nothing is recording.
fn store(counter: Counter, value: f64) -> Option<u64> {
    if !enabled() {
        return None;
    }
    let phase = phase_index();
    if !value.is_finite() || value < 0.0 {
        COUNTER_REJECTED[counter as usize].fetch_add(1, Ordering::Relaxed);
        return None;
    }
    let stored = match counter.unit() {
        Unit::Milliseconds => value * 1e6,
        Unit::Count => value,
    };
    let stored = stored as u64;
    COUNTERS[phase][counter as usize].record(stored);
    Some(stored)
}

#[inline]
fn phase_index() -> usize {
    (PHASE.load(Ordering::Relaxed) as usize).min(Phase::COUNT - 1)
}

#[inline]
fn record(span: Span, ns: u64) {
    HISTOGRAMS[phase_index()][span as usize].record(ns);
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

/// What one counter recorded over one phase. `sum`, `min`, `max` and the
/// percentiles are in the counter's own [`Unit`]; nothing here is comparable
/// across units, which is why the unit travels with the row.
#[derive(Clone, Copy, Debug)]
pub struct CounterStats {
    pub counter: Counter,
    pub unit: Unit,
    pub origin: Origin,
    /// Samples the emitter produced. Not the frame count: a counter emitted
    /// twice a frame has twice as many, and one the GPU could not resolve has
    /// fewer.
    pub samples: u64,
    /// Samples refused as not finite or negative. They are in no other number
    /// on this row.
    pub rejected: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

impl CounterStats {
    /// The mean of one sample — the cost or size of a single emission. For a
    /// counter emitted more than once a frame this is *not* the per-frame
    /// figure; divide `sum` by the frame count for that.
    pub fn avg(&self) -> f64 {
        if self.samples == 0 {
            return 0.0;
        }
        self.sum / self.samples as f64
    }

    /// How many samples this counter produced per frame of the phase.
    pub fn per_frame_samples(&self, frames: u64) -> Option<f64> {
        (frames > 0).then(|| self.samples as f64 / frames as f64)
    }

    /// The counter's total spread over the frames of the phase. This is the
    /// number to compare against a frame budget; `avg` is not.
    pub fn per_frame(&self, frames: u64) -> Option<f64> {
        (frames > 0).then(|| self.sum / frames as f64)
    }
}

/// Every counter that took at least one sample in `phase`, in declaration
/// order. A counter that was never emitted is absent rather than zero: the
/// report has to be able to say MISS.
pub fn counter_snapshot(phase: Phase) -> Vec<CounterStats> {
    let histograms = &COUNTERS[phase.index()];
    Counter::ALL
        .into_iter()
        .filter_map(|counter| {
            let histogram = &histograms[counter as usize];
            let samples = histogram.count.load(Ordering::Relaxed);
            let rejected = COUNTER_REJECTED[counter as usize].load(Ordering::Relaxed);
            if samples == 0 && rejected == 0 {
                return None;
            }
            let quantiles = quantiles(histogram, samples.max(1), &[0.50, 0.95, 0.99]);
            let scale = |stored: u64| match counter.unit() {
                Unit::Milliseconds => stored as f64 / 1e6,
                Unit::Count => stored as f64,
            };
            Some(CounterStats {
                counter,
                unit: counter.unit(),
                origin: counter.origin(),
                samples,
                rejected,
                sum: scale(histogram.sum_ns.load(Ordering::Relaxed)),
                min: if samples == 0 {
                    0.0
                } else {
                    scale(histogram.min_ns.load(Ordering::Relaxed))
                },
                max: scale(histogram.max_ns.load(Ordering::Relaxed)),
                p50: scale(quantiles[0]),
                p95: scale(quantiles[1]),
                p99: scale(quantiles[2]),
            })
        })
        .collect()
}

/// Counters this build declares but that took no sample at all in either
/// phase. A name here is either instrumentation that never ran on this
/// workload or a counter nothing emits any more; the report prints them so a
/// reader does not mistake an absent row for a zero.
pub fn counters_never_sampled() -> Vec<Counter> {
    Counter::ALL
        .into_iter()
        .filter(|counter| {
            COUNTERS
                .iter()
                .all(|phase| phase[*counter as usize].count.load(Ordering::Relaxed) == 0)
                && COUNTER_REJECTED[*counter as usize].load(Ordering::Relaxed) == 0
        })
        .collect()
}

/// Spans this build declares that opened in neither phase. The tree can only
/// print what took a sample, so without this a span whose producer was deleted
/// or compiled out looks exactly like a span the workload never reached — and
/// the first is a hole in the instrumentation while the second is a fact about
/// the run.
pub fn spans_never_sampled() -> Vec<Span> {
    Span::ALL
        .into_iter()
        .filter(|span| {
            HISTOGRAMS
                .iter()
                .all(|phase| phase[*span as usize].count.load(Ordering::Relaxed) == 0)
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
    /// Spans that closed while something they did not enclose was still open
    /// under them on the same thread.
    pub misnested: u64,
    /// Spans whose `begin` and `end` ran on different threads, so no thread
    /// could name the span they opened inside. Their own timings stand; the
    /// frame's coverage does not depend on them, because the roots are
    /// declared.
    pub migrated: u64,
    /// Counter samples that named the frame they ran in and found no row for
    /// it — the frame was dropped, or closed before the recorder was armed.
    /// They are in the histograms and in no row.
    pub unattributed_counters: u64,
}

pub fn anomalies() -> Anomalies {
    Anomalies {
        unmatched_end: UNMATCHED_END.load(Ordering::Relaxed),
        reopened: REOPENED.load(Ordering::Relaxed),
        misnested: MISNESTED.load(Ordering::Relaxed),
        migrated: MIGRATED.load(Ordering::Relaxed),
        unattributed_counters: COUNTER_UNATTRIBUTED
            .iter()
            .map(|n| n.load(Ordering::Relaxed))
            .sum(),
    }
}
