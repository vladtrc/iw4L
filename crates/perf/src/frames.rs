//! `frames.csv`: one row per frame, not a histogram of them.
//!
//! [`stats`](crate::stats) answers "where did the run go" and cannot answer
//! "which frame was the 65 ms one, and what was different about it" — a
//! histogram does not keep the rows it was built from. This does: a fixed,
//! preallocated table filled at frame end, dumped once after the measurement.
//!
//! What it costs in the hot path is one relaxed `fetch_add` per span end and
//! per counter sample, and one mutex lock per *frame* — never per span. There
//! is no formatting, no allocation and no disk write while the run is going;
//! `snapshot` hands the rows out at exit and whoever asked for them writes the
//! file.
//!
//! The table is bounded on purpose. When it fills, rows are dropped and
//! counted, and the count travels with the snapshot: a truncated measurement
//! says so rather than quietly reporting the first minute as the whole run.
//!
//! GPU counters are not this frame's GPU time. The driver resolves a timestamp
//! several frames after the CPU work that queued it, so they are recorded here
//! under `delivered_*`: what arrived during this CPU frame, which is a fact
//! about delivery and not about this frame's GPU cost.
//!
//! A span is charged to a frame by *intersection*, not by where it happened to
//! end. A load task that ran for 36 ms across four frames and closed in this
//! one used to book all 36 ms here, which is how a 47 ms frame could report a
//! 36 ms child inside an 11 ms parent. What lands in `spans_ns` now is the part
//! of the span that overlapped this frame's wall; the part that ran before it
//! is summed into `carried_in_ns` rather than dropped, and the span's full
//! elapsed time is still in the histogram, where it belongs.
//!
//! `covered_ns` is the *union* of the root spans' intervals clipped to the
//! frame, so two schedules running at once on two threads cover the wall once
//! between them instead of twice each. `wall_ns - covered_ns` is therefore a
//! real remainder and cannot go negative, which `wall - sum(roots)` could.
//!
//! Which spans are roots is declared by [`Span::coverage_root`], not read off
//! a per-thread stack at the moment each closed: a schedule span opens in one
//! system and closes in another, and an executor that runs those two on
//! different workers left the span root on neither thread and its whole
//! interval outside the union.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::vocabulary_types::{Counter, Origin, Span, Unit};

/// Rows kept unless `IW4L_BENCH_FRAMES` says otherwise. Ten minutes at 60 fps,
/// which is longer than any bench run so far and about 17 MiB of table.
const DEFAULT_CAPACITY: usize = 36_000;

const ENV_CAPACITY: &str = "IW4L_BENCH_FRAMES";

/// What was true of the frame beyond its timings. Sticky bits are set by
/// whoever knows — the loading overlay, the capture queue — and read out when
/// the frame closes.
pub mod flag {
    /// The loading overlay was up for at least part of the frame.
    pub const LOADING: u32 = 1 << 0;
    /// The match had not started: warmup, or a demo still seeking.
    pub const WARMUP: u32 = 1 << 1;
    /// A screenshot readback was queued or serviced in this frame.
    pub const SCREENSHOT: u32 = 1 << 2;
    /// A first-person weapon was raised, swapped or dropped.
    pub const WEAPON: u32 = 1 << 3;
    /// Depth of field was on.
    pub const DOF: u32 = 1 << 4;
    /// A teardown ran: despawn, zone drop, cache eviction.
    pub const CLEANUP: u32 = 1 << 5;
    /// The row was closed at exit rather than by the frame clock. Its wall is
    /// as much of the frame as the process lived through and is not a frame
    /// time; its span columns are what that much of the frame ran.
    pub const PARTIAL: u32 = 1 << 6;

    /// Every flag with the name the report and the CSV use for it.
    pub const ALL: [(u32, &str); 7] = [
        (LOADING, "loading"),
        (WARMUP, "warmup"),
        (SCREENSHOT, "screenshot"),
        (WEAPON, "weapon"),
        (DOF, "dof"),
        (CLEANUP, "cleanup"),
        (PARTIAL, "partial"),
    ];
}

/// One frame, as the recorder saw it. Fixed size: the arrays are indexed by
/// `Span as usize` and `Counter as usize`, so a row allocates nothing.
#[derive(Clone)]
pub struct FrameRow {
    /// Frames closed since the recorder was armed, from zero. Not a Bevy tick.
    pub index: u64,
    /// Start and end of the frame clock, nanoseconds since the recorder's zero.
    pub start_ns: u64,
    pub end_ns: u64,
    /// `end_ns - start_ns`, kept as measured rather than recomputed.
    pub wall_ns: u64,
    /// 0 while the map was loading, 1 once it was playable.
    pub phase: u8,
    /// The [`flag`] bits that were set during the frame.
    pub flags: u32,
    /// Recorder thread slots — see [`thread_names`]. `main` is the thread that
    /// closed the frame clock, `render` the one that closed `render_thread`.
    pub main_thread: u32,
    pub render_thread: u32,
    /// The replay tick this frame presented, or `None` when nothing set one.
    pub replay_tick: Option<u64>,
    /// Each span's overlap with this frame's wall, nanoseconds. Not the
    /// span's elapsed time: a span that began in an earlier frame contributes
    /// only the part that ran inside this one.
    pub spans_ns: [u64; Span::COUNT],
    /// The union of the root spans' intervals, clipped to the wall. Counted
    /// once where they overlap, so this is never more than `wall_ns`.
    pub covered_ns: u64,
    /// The same union with the render thread left out: what the main world's
    /// schedules covered on their own.
    ///
    /// `main_covered_ns + spans_ns[render_thread] - covered_ns` is the time
    /// the render thread and a main schedule were both inside this frame —
    /// real overlap, read off the intervals, and not inferred from which frame
    /// a render stage says it originated in. Serialised rendering leaves it at
    /// zero; it is what a pipelining A/B is read on.
    pub main_covered_ns: u64,
    /// Span time that ran before this frame's wall began, in spans that ended
    /// inside it. It is not in `spans_ns` and not in `covered_ns`; it is what
    /// this frame inherited from the ones before it.
    pub carried_in_ns: u64,
    /// How many spans contributed to `carried_in_ns`.
    pub carried_spans: u32,
    /// The coverage set ran out of room, so `covered_ns` is a lower bound and
    /// the remainder an upper one.
    pub coverage_overflow: bool,

    /// Each counter summed over the frame, in the counter's own unit —
    /// nanoseconds for [`Unit::Milliseconds`], the count itself otherwise.
    pub counters: [u64; Counter::COUNT],
    /// How many samples each counter took, so a MISS is not a zero.
    pub counter_samples: [u32; Counter::COUNT],
}

/// The rows, and what the recorder could not keep.
pub struct Frames {
    pub rows: Vec<FrameRow>,
    /// Frames closed after the table filled. They are in no row here.
    pub dropped: u64,
    /// Whether the table ever filled. `dropped == 0` and `overflowed` are the
    /// same answer today; the flag stays because a future eviction policy
    /// would keep rows *and* have overflowed.
    pub overflowed: bool,
    pub capacity: usize,
}

static ARMED: AtomicBool = AtomicBool::new(false);
static CAPACITY: AtomicU32 = AtomicU32::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static OVERFLOWED: AtomicBool = AtomicBool::new(false);
static CLOSED: AtomicU64 = AtomicU64::new(0);

static SPAN_NS: [AtomicU64; Span::COUNT] = [const { AtomicU64::new(0) }; Span::COUNT];
static COUNTER_SUM: [AtomicU64; Counter::COUNT] = [const { AtomicU64::new(0) }; Counter::COUNT];
static COUNTER_N: [AtomicU32; Counter::COUNT] = [const { AtomicU32::new(0) }; Counter::COUNT];
static FLAGS: AtomicU32 = AtomicU32::new(0);
static STATE: AtomicU32 = AtomicU32::new(0);
static REPLAY_TICK: AtomicU64 = AtomicU64::new(u64::MAX);
static RENDER_THREAD: AtomicU32 = AtomicU32::new(0);

/// When the open frame's wall began. Everything charged to this frame is
/// clipped to start no earlier than this.
static FRAME_OPEN_AT: AtomicU64 = AtomicU64::new(0);
static CARRIED_NS: AtomicU64 = AtomicU64::new(0);
static CARRIED_SPANS: AtomicU32 = AtomicU32::new(0);

/// The intervals the root spans covered, merged as they arrive.
static COVER: Mutex<Coverage> = Mutex::new(Coverage::EMPTY);

/// The frame's coverage, twice: every root, and the main world's roots alone.
///
/// One lock for the pair. The difference between the two answers whether the
/// render thread ran beside a main schedule or after it, which is the whole
/// question a pipelining A/B asks.
struct Coverage {
    all: Cover,
    main: Cover,
}

impl Coverage {
    const EMPTY: Self = Self {
        all: Cover::EMPTY,
        main: Cover::EMPTY,
    };
}

/// Root span intervals kept per frame. A frame has one root per schedule per
/// thread — four or five in practice — so this is generous, and running out
/// sets a flag rather than reporting a wrong union.
const COVER_SLOTS: usize = 64;

/// A union of half-open intervals, kept sorted and merged.
///
/// Summing the roots instead double-counts every nanosecond two threads were
/// both inside one, which is how `wall - sum(roots)` went negative. Merging
/// costs a memmove of at most [`COVER_SLOTS`] pairs, once per root span.
struct Cover {
    spans: [(u64, u64); COVER_SLOTS],
    n: usize,
    overflow: bool,
}

impl Cover {
    const EMPTY: Self = Self {
        spans: [(0, 0); COVER_SLOTS],
        n: 0,
        overflow: false,
    };

    fn add(&mut self, start: u64, end: u64) {
        if end <= start {
            return;
        }
        // Absorb every interval this one touches, then insert what is left.
        // `insert` counts the intervals that end before it, which is where the
        // merged one belongs once the rest have been compacted down.
        let (mut start, mut end) = (start, end);
        let mut write = 0;
        let mut insert = 0;
        for read in 0..self.n {
            let (low, high) = self.spans[read];
            if high < start {
                self.spans[write] = (low, high);
                write += 1;
                insert = write;
            } else if low > end {
                self.spans[write] = (low, high);
                write += 1;
            } else {
                start = start.min(low);
                end = end.max(high);
            }
        }
        self.n = write;
        if self.n == COVER_SLOTS {
            self.overflow = true;
            return;
        }
        self.spans.copy_within(insert..self.n, insert + 1);
        self.spans[insert] = (start, end);
        self.n += 1;
    }

    /// How much of `[from, to)` the union covers, and reset for the next frame.
    fn drain(&mut self, from: u64, to: u64) -> (u64, bool) {
        let covered = self.spans[..self.n]
            .iter()
            .map(|(low, high)| (*high).min(to).saturating_sub((*low).max(from)))
            .sum();
        let overflow = self.overflow;
        self.n = 0;
        self.overflow = false;
        (covered, overflow)
    }
}

static TABLE: Mutex<Vec<FrameRow>> = Mutex::new(Vec::new());

/// Thread slot names, indexed by slot. Slot 0 is "not recorded".
static THREADS: Mutex<Vec<String>> = Mutex::new(Vec::new());

thread_local! {
    static SLOT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// This thread's recorder slot, minted on first use. Not an OS thread id: it
/// answers "was this the same thread as that one", which is the question the
/// report actually asks, and it costs a thread-local read rather than a call
/// into the kernel on every frame.
fn slot() -> u32 {
    SLOT.with(|cell| {
        let known = cell.get();
        if known != 0 {
            return known;
        }
        let name = std::thread::current()
            .name()
            .map_or_else(|| "<unnamed>".to_owned(), str::to_owned);
        let mut threads = THREADS.lock().unwrap_or_else(|poison| poison.into_inner());
        threads.push(name);
        let minted = u32::try_from(threads.len()).unwrap_or(u32::MAX);
        cell.set(minted);
        minted
    })
}

/// The name of each recorder thread slot, in slot order starting at one.
pub fn thread_names() -> Vec<String> {
    THREADS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Size the table and arm the recorder. Called by [`stats::arm`] once the
/// environment has been read; `on` is whether the bench recorder is running at
/// all, because a per-frame row is worthless without the spans that fill it.
///
/// `IW4L_BENCH_FRAMES=0` turns the table off and leaves the rest of the bench
/// alone — the escape hatch for measuring the recorder's own cost.
pub(crate) fn arm(on: bool) {
    let capacity = match std::env::var(ENV_CAPACITY) {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(rows) => rows,
            Err(_) => {
                eprintln!("bench: {ENV_CAPACITY} is not a row count; using {DEFAULT_CAPACITY}");
                DEFAULT_CAPACITY
            }
        },
        Err(_) => DEFAULT_CAPACITY,
    };
    if !on || capacity == 0 {
        return;
    }
    let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    table.reserve_exact(capacity);
    CAPACITY.store(
        u32::try_from(capacity).unwrap_or(u32::MAX),
        Ordering::Relaxed,
    );
    ARMED.store(true, Ordering::Relaxed);
}

/// Whether a per-frame row is being kept. Off leaves every function here at
/// one relaxed load.
#[inline]
pub fn enabled() -> bool {
    ARMED.load(Ordering::Relaxed)
}

/// The open frame's wall began now. Called by the frame clock's `begin`, so
/// that a span closing inside this frame knows which part of itself is this
/// frame's and which part the previous frames already ran.
#[inline]
pub(crate) fn open(at_ns: u64) {
    if enabled() {
        FRAME_OPEN_AT.store(at_ns, Ordering::Relaxed);
    }
}

/// Charge one closed span to the open frame.
///
/// `start_ns`/`end_ns` are the span's own interval; what is booked is its
/// intersection with the frame, and the part that ran before the frame opened
/// goes to `carried_in_ns`. `root` — [`Span::coverage_root`], declared and not
/// observed — also puts the clipped interval into the coverage union.
#[inline]
pub(crate) fn add_span(span: Span, start_ns: u64, end_ns: u64, root: bool) {
    if !enabled() {
        return;
    }
    let opened = FRAME_OPEN_AT.load(Ordering::Relaxed);
    let inside_from = start_ns.max(opened);
    let inside = end_ns.saturating_sub(inside_from);
    SPAN_NS[span as usize].fetch_add(inside, Ordering::Relaxed);
    let carried = inside_from.saturating_sub(start_ns);
    if carried > 0 {
        CARRIED_NS.fetch_add(carried, Ordering::Relaxed);
        CARRIED_SPANS.fetch_add(1, Ordering::Relaxed);
    }
    if root && inside > 0 {
        let mut cover = COVER.lock().unwrap_or_else(|poison| poison.into_inner());
        cover.all.add(inside_from, end_ns);
        if span != Span::RenderRenderThreadMs {
            cover.main.add(inside_from, end_ns);
        }
    }
    if span == Span::RenderRenderThreadMs {
        RENDER_THREAD.store(slot(), Ordering::Relaxed);
    }
}

#[inline]
pub(crate) fn add_counter(counter: Counter, stored: u64) {
    if !enabled() {
        return;
    }
    COUNTER_SUM[counter as usize].fetch_add(stored, Ordering::Relaxed);
    COUNTER_N[counter as usize].fetch_add(1, Ordering::Relaxed);
}

/// The index the frame that is open now will carry once it closes.
///
/// Work that runs on another thread and is only reported back a frame or two
/// later reads this when it *starts* and hands it back with the result, so the
/// value lands on the row whose wall it was inside. Without it the render
/// stage timings sit one row late, and a spike reads against the frame after
/// the one that paid for it.
#[inline]
pub fn open_index() -> u64 {
    CLOSED.load(Ordering::Relaxed)
}

/// Charge a counter to the frame that carried `index`, not to the open one.
///
/// Returns whether the row was found: a frame the table dropped, or one closed
/// before the recorder was armed, cannot be charged and the sample is left out
/// rather than moved to a row it did not happen in.
pub(crate) fn add_counter_at(index: u64, counter: Counter, stored: u64) -> bool {
    if !enabled() {
        return false;
    }
    let closed = CLOSED.load(Ordering::Relaxed);
    if index == closed {
        // Still open: the accumulators are this frame's, which is where an
        // in-frame sample goes anyway.
        add_counter(counter, stored);
        return true;
    }
    if index > closed {
        return false;
    }
    let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    // Rows are pushed in index order with no gaps, so the index *is* the slot
    // until the table fills; the check is what makes that an assumption the
    // code can survive being wrong about.
    let at = usize::try_from(index).ok().filter(|at| {
        table
            .get(*at)
            .is_some_and(|row: &FrameRow| row.index == index)
    });
    let at = match at {
        Some(at) => at,
        None => match table.binary_search_by_key(&index, |row| row.index) {
            Ok(at) => at,
            Err(_) => return false,
        },
    };
    let row = &mut table[at];
    row.counters[counter as usize] += stored;
    row.counter_samples[counter as usize] += 1;
    true
}

/// Raise a [`flag`] for the frame that is open now. Cleared when it closes.
#[inline]
pub fn mark(flag: u32) {
    if enabled() {
        FLAGS.fetch_or(flag, Ordering::Relaxed);
    }
}

/// Hold a [`flag`] until it is lowered again — for a condition that spans
/// frames, like the loading overlay, where a per-frame `mark` would miss every
/// frame that did not happen to touch the code that knows.
#[inline]
pub fn set_state(flag: u32, on: bool) {
    if !enabled() {
        return;
    }
    if on {
        STATE.fetch_or(flag, Ordering::Relaxed);
    } else {
        STATE.fetch_and(!flag, Ordering::Relaxed);
    }
}

/// The replay tick this frame is presenting. Left alone, the column is MISS
/// rather than zero.
#[inline]
pub fn set_replay_tick(tick: u64) {
    if enabled() {
        REPLAY_TICK.store(tick, Ordering::Relaxed);
    }
}

/// Close the frame that was open: drain the accumulators into a row. Called
/// from the frame clock's end, which is also what gives the row its wall.
pub(crate) fn close(start_ns: u64, end_ns: u64, phase: u8) {
    if !enabled() {
        return;
    }
    let mut spans_ns = [0u64; Span::COUNT];
    for span in Span::ALL {
        spans_ns[span as usize] = SPAN_NS[span as usize].swap(0, Ordering::Relaxed);
    }
    let mut counters = [0u64; Counter::COUNT];
    let mut counter_samples = [0u32; Counter::COUNT];
    for counter in Counter::ALL {
        counters[counter as usize] = COUNTER_SUM[counter as usize].swap(0, Ordering::Relaxed);
        counter_samples[counter as usize] = COUNTER_N[counter as usize].swap(0, Ordering::Relaxed);
    }
    let tick = REPLAY_TICK.load(Ordering::Relaxed);
    let (covered_ns, main_covered_ns, coverage_overflow) = {
        let mut cover = COVER.lock().unwrap_or_else(|poison| poison.into_inner());
        let (covered_ns, all_overflow) = cover.all.drain(start_ns, end_ns);
        let (main_covered_ns, main_overflow) = cover.main.drain(start_ns, end_ns);
        (covered_ns, main_covered_ns, all_overflow || main_overflow)
    };
    // The next frame's wall starts where this one ended, so a span that closes
    // between the two is charged to the frame it actually ran in.
    FRAME_OPEN_AT.store(end_ns, Ordering::Relaxed);
    let row = FrameRow {
        index: CLOSED.fetch_add(1, Ordering::Relaxed),
        start_ns,
        end_ns,
        wall_ns: end_ns.saturating_sub(start_ns),
        phase,
        flags: FLAGS.swap(0, Ordering::Relaxed) | STATE.load(Ordering::Relaxed),
        main_thread: slot(),
        render_thread: RENDER_THREAD.swap(0, Ordering::Relaxed),
        replay_tick: (tick != u64::MAX).then_some(tick),
        spans_ns,
        covered_ns: covered_ns.min(end_ns.saturating_sub(start_ns)),
        main_covered_ns: main_covered_ns.min(end_ns.saturating_sub(start_ns)),
        carried_in_ns: CARRIED_NS.swap(0, Ordering::Relaxed),
        carried_spans: CARRIED_SPANS.swap(0, Ordering::Relaxed),
        coverage_overflow,
        counters,
        counter_samples,
    };

    let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    if table.len() == table.capacity() {
        drop(table);
        OVERFLOWED.store(true, Ordering::Relaxed);
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    table.push(row);
}

/// Every row kept, and what was not. Cheap to call once; it clones the table.
pub fn snapshot() -> Frames {
    let table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    Frames {
        rows: table.clone(),
        dropped: DROPPED.load(Ordering::Relaxed),
        overflowed: OVERFLOWED.load(Ordering::Relaxed),
        capacity: CAPACITY.load(Ordering::Relaxed) as usize,
    }
}

/// The column name a counter takes in `frames.csv`. A GPU counter is a
/// delivery, not this frame's GPU time, and the header says so rather than
/// leaving the reader to know it.
pub fn counter_column(counter: Counter) -> String {
    let unit = match counter.unit() {
        Unit::Milliseconds => "_ns",
        Unit::Count => "",
    };
    match counter.origin() {
        Origin::Gpu => format!("delivered_{}{unit}", counter.name()),
        Origin::Cpu => format!("{}{unit}", counter.name()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Counter, Cover};

    /// The recorder is process-global, so the one test that arms it and closes
    /// frames holds this while it does.
    static RECORDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn covered(intervals: &[(u64, u64)], from: u64, to: u64) -> (u64, bool) {
        let mut cover = Cover::EMPTY;
        for (start, end) in intervals {
            cover.add(*start, *end);
        }
        cover.drain(from, to)
    }

    #[test]
    fn two_threads_inside_the_same_millisecond_cover_it_once() {
        // The bug this replaces: summing the roots reported 20 ms of work in a
        // 10 ms frame and clamped the remainder to zero.
        let (ns, overflow) = covered(&[(0, 10), (0, 10)], 0, 10);
        assert_eq!(ns, 10);
        assert!(!overflow);
    }

    #[test]
    fn a_gap_between_roots_stays_uncovered() {
        let (ns, _) = covered(&[(0, 3), (7, 10)], 0, 10);
        assert_eq!(ns, 6);
    }

    #[test]
    fn intervals_merge_whatever_order_they_close_in() {
        let ordered = covered(&[(0, 4), (3, 6), (6, 9)], 0, 10).0;
        let shuffled = covered(&[(6, 9), (0, 4), (3, 6)], 0, 10).0;
        assert_eq!(ordered, 9);
        assert_eq!(ordered, shuffled);
    }

    #[test]
    fn coverage_never_exceeds_the_window_it_is_read_over() {
        let (ns, _) = covered(&[(0, 1_000)], 100, 200);
        assert_eq!(ns, 100);
    }

    #[test]
    fn a_counter_reported_late_lands_on_the_frame_it_was_measured_in() {
        // The render stage timings are published by the render app and read by
        // the main world a frame later, so a sample that does not name the
        // frame it was measured in lands on the row after the one that paid it.
        let _guard = RECORDER.lock().unwrap_or_else(|poison| poison.into_inner());
        super::arm(true);
        let first = super::open_index();
        super::close(0, 1_000, 1);
        super::close(1_000, 2_000, 1);

        assert!(super::add_counter_at(
            first,
            Counter::RenderGraphSubmitIntervalMs,
            75
        ));
        let rows = super::snapshot().rows;
        let row = |index: u64| {
            rows.iter()
                .find(|row| row.index == index)
                .expect("the row this test closed")
                .counters[Counter::RenderGraphSubmitIntervalMs as usize]
        };
        assert_eq!(row(first), 75, "the sample missed the frame it ran in");
        assert_eq!(row(first + 1), 0, "the sample landed a frame late");

        // A frame the table never kept is left out rather than charged to
        // whatever is open now.
        assert!(!super::add_counter_at(
            u64::MAX - 1,
            Counter::RenderGraphSubmitIntervalMs,
            5
        ));
    }

    #[test]
    fn running_out_of_slots_says_so_instead_of_reporting_a_short_union() {
        let disjoint: Vec<(u64, u64)> = (0..super::COVER_SLOTS as u64 + 4)
            .map(|n| (n * 10, n * 10 + 1))
            .collect();
        let (ns, overflow) = covered(&disjoint, 0, u64::MAX);
        assert!(overflow, "the set filled and did not say so");
        assert_eq!(ns, super::COVER_SLOTS as u64);
    }
}
