//! `load_jobs.csv`: what each piece of the load was waiting on.
//!
//! A stage row says a stage held the pool for two seconds. It does not say
//! whether those two seconds were a worker doing the work, the job sitting in
//! the executor's queue behind sixteen others, or the job being *ready* and
//! nobody having handed it to the pool yet. Those three have different fixes,
//! and the third one is invisible to every timer that starts when a worker
//! picks the job up.
//!
//! So a job carries seven stamps, not two:
//!
//! ```text
//! discovered_at      the producer learned the work exists
//! plan_ready_at      the work is fully described and could be handed over
//! enqueued_at        it was handed to the pool
//! started_at         a worker picked it up
//! decode_started_at  it was allowed to spend memory and began the work
//! finished_at        the worker put the result down
//! joined_at          the consumer took the result
//! ```
//!
//! `plan_ready_at → enqueued_at` is the gap this file was written for: work
//! that was ready and idle. `enqueued_at → started_at` is queue delay, and
//! `started_at → finished_at` is service time. Reading them as one number is
//! how a scheduling bug reads as a slow decoder.
//!
//! `started_at → decode_started_at` is the fifth: a worker that has the job and
//! is waiting on a memory ceiling before it may begin. Folding that into
//! service time is how a throttle reads as a slow decoder, which is the same
//! mistake one level down, so the decode timer starts when the permission does
//! and not when the worker does.
//!
//! `finished_at → joined_at` is not a wait either. It is how long a finished
//! result sat before its consumer came for it — which is what holds memory —
//! and it is named `completed_to_join` because it was being read as the
//! consumer blocking on work that had not been done.
//!
//! Bounded and preallocated, like [`perf::frames`](../../perf/src/frames.rs):
//! when the table fills, rows are dropped and counted, and the count travels
//! with the snapshot.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Rows kept in the default (per-class) mode.
const CLASS_CAPACITY: usize = 4_096;
/// Rows kept when `IW4L_BENCH_JOBS=assets` asks for one row per asset.
const ASSET_CAPACITY: usize = 131_072;

const ENV: &str = "IW4L_BENCH_JOBS";

/// What kind of work a row is. A free-form string would let two producers
/// spell the same class differently and split its rows in the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Opening and inflating a FastFile.
    ZoneOpen,
    /// Walking a zone's assets into a catalog.
    ZoneWalk,
    /// Building the list of images a catalog wants decoded.
    ImagePlan,
    /// Decoding one plan's images.
    ImageDecode,
    /// One image, in `IW4L_BENCH_JOBS=assets` mode.
    Image,
    /// Preparing the match's audio clips.
    AudioPrep,
    /// One clip, in `IW4L_BENCH_JOBS=assets` mode.
    AudioClip,
}

impl JobKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ZoneOpen => "zone_open",
            Self::ZoneWalk => "zone_walk",
            Self::ImagePlan => "image_plan",
            Self::ImageDecode => "image_decode",
            Self::Image => "image",
            Self::AudioPrep => "audio_prep",
            Self::AudioClip => "audio_clip",
        }
    }

    /// Whether this class is only recorded in per-asset mode.
    const fn per_asset(self) -> bool {
        matches!(self, Self::Image | Self::AudioClip)
    }
}

/// Where a prepared result came from, when the job went through a cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheResult {
    Hit,
    Miss,
    Stored,
    Failed,
}

impl CacheResult {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Stored => "stored",
            Self::Failed => "failed",
        }
    }
}

/// One job. Every timestamp after `discovered_at` is optional, because a job
/// that was never enqueued is exactly the finding this table exists to make
/// visible — and an absent stamp must not read as a zero.
#[derive(Clone, Debug)]
pub struct JobRow {
    pub id: u64,
    /// Jobs this one waited on, by id. Real edges, not "whatever else was
    /// running": the report subtracts dependency wait from these, and a
    /// guessed edge would turn into an invented number.
    pub deps: Vec<u64>,
    pub kind: JobKind,
    pub namespace: Option<&'static str>,
    pub canonical: Option<String>,
    pub discovered_at: Instant,
    pub plan_ready_at: Option<Instant>,
    pub enqueued_at: Option<Instant>,
    pub started_at: Option<Instant>,
    /// When the job was allowed to start spending memory. Between `started_at`
    /// and this the worker held the job and was waiting on the ceiling.
    pub decode_started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
    pub joined_at: Option<Instant>,
    /// Bytes outstanding against the ceiling when this job was let through, and
    /// whether it had to wait at all. A job that never waited leaves `waited`
    /// false and the ceiling is not what held the load up.
    pub outstanding_at_decode: Option<u64>,
    pub waited_for_budget: bool,
    pub source_bytes: Option<u64>,
    /// Bytes this job prepared itself, whether or not the merge kept them.
    /// This is what the work cost; `retained_bytes` is what it bought.
    pub prepared_bytes: Option<u64>,
    /// Bytes the job served out of a payload another job had already prepared.
    /// Nothing was decoded for these and nothing was allocated twice; they are
    /// here so that `prepared + reused` — what the job *served* — is a sum a
    /// reader can take rather than a number that silently includes both.
    pub reused_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    /// Bytes the merged catalog kept, and bytes it threw away. The pair is the
    /// whole point of the merge-before-decode question.
    pub retained_bytes: Option<u64>,
    pub discarded_bytes: Option<u64>,
    /// Process RSS when the job finished. Whole-process, so it is not this
    /// job's ownership and the report may not sum these.
    pub rss_at_finish: Option<u64>,
    pub items: Option<u64>,
    pub cache_result: Option<CacheResult>,
    pub discard_reason: Option<String>,
}

/// The table, and what it could not keep.
pub struct Jobs {
    pub rows: Vec<JobRow>,
    pub dropped: u64,
    pub per_asset: bool,
    pub capacity: usize,
}

static ARMED: AtomicBool = AtomicBool::new(false);
static PER_ASSET: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static TABLE: Mutex<Vec<JobRow>> = Mutex::new(Vec::new());

/// Arm the recorder. `on` is whether the bench is running at all; the mode
/// comes from `IW4L_BENCH_JOBS`, where `assets` adds one row per decoded image
/// and prepared clip and anything else keeps the per-class rows only.
pub fn arm(on: bool) {
    if !on {
        return;
    }
    let per_asset = std::env::var(ENV).is_ok_and(|value| {
        value.eq_ignore_ascii_case("assets") || value.eq_ignore_ascii_case("asset")
    });
    let capacity = if per_asset {
        ASSET_CAPACITY
    } else {
        CLASS_CAPACITY
    };
    let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    table.reserve_exact(capacity);
    PER_ASSET.store(per_asset, Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
}

#[inline]
pub fn enabled() -> bool {
    ARMED.load(Ordering::Relaxed)
}

/// Whether per-asset rows are being kept.
#[inline]
pub fn per_asset() -> bool {
    PER_ASSET.load(Ordering::Relaxed)
}

/// Open a job. The returned handle is inert when the recorder is off, so a
/// call site costs one relaxed load and nothing else.
pub fn open(kind: JobKind) -> Job {
    if !enabled() || (kind.per_asset() && !per_asset()) {
        return Job::INERT;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let row = JobRow {
        id,
        deps: Vec::new(),
        kind,
        namespace: None,
        canonical: None,
        discovered_at: Instant::now(),
        plan_ready_at: None,
        enqueued_at: None,
        started_at: None,
        decode_started_at: None,
        finished_at: None,
        joined_at: None,
        outstanding_at_decode: None,
        waited_for_budget: false,
        source_bytes: None,
        prepared_bytes: None,
        reused_bytes: None,
        output_bytes: None,
        retained_bytes: None,
        discarded_bytes: None,
        rss_at_finish: None,
        items: None,
        cache_result: None,
        discard_reason: None,
    };
    let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    if table.len() == table.capacity() {
        drop(table);
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return Job::INERT;
    }
    let at = table.len();
    table.push(row);
    Job { id, at }
}

/// A handle to one row. `Copy`, so it can be carried into a task and used
/// again by the consumer that joins the result without any ceremony.
#[derive(Clone, Copy, Debug)]
pub struct Job {
    id: u64,
    /// Where the row sits in the table. Rows are only ever appended, so this
    /// stays valid — and a handle that had to search for its own id would
    /// make per-asset mode quadratic in the number of assets.
    at: usize,
}

impl Job {
    /// A handle that records nothing: the recorder is off, this class is not
    /// kept in this mode, or the table was full.
    pub const INERT: Self = Self {
        id: 0,
        at: usize::MAX,
    };

    /// The id this job has in `load_jobs.csv`, or `None` when nothing is
    /// being recorded.
    pub fn id(self) -> Option<u64> {
        (self.id != 0).then_some(self.id)
    }

    fn edit(self, change: impl FnOnce(&mut JobRow)) {
        if self.at == usize::MAX {
            return;
        }
        let mut table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(row) = table.get_mut(self.at) {
            change(row);
        }
    }

    pub fn depends_on(self, other: Job) -> Self {
        if let Some(id) = other.id() {
            self.edit(|row| row.deps.push(id));
        }
        self
    }

    pub fn namespace(self, namespace: &'static str) -> Self {
        self.edit(|row| row.namespace = Some(namespace));
        self
    }

    pub fn canonical(self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.edit(|row| row.canonical = Some(id));
        self
    }

    pub fn plan_ready(self) -> Self {
        let at = Instant::now();
        self.edit(|row| {
            row.plan_ready_at.get_or_insert(at);
        });
        self
    }

    pub fn enqueued(self) -> Self {
        let at = Instant::now();
        self.edit(|row| {
            row.enqueued_at.get_or_insert(at);
        });
        self
    }

    pub fn started(self) -> Self {
        let at = Instant::now();
        self.edit(|row| {
            row.started_at.get_or_insert(at);
        });
        self
    }

    /// The worker was let past the memory ceiling and the work itself begins
    /// now. `outstanding` is what was already outstanding against the ceiling
    /// at that moment, and `waited` whether this job was ever held by it.
    pub fn decode_started(self, outstanding: u64, waited: bool) -> Self {
        let at = Instant::now();
        self.edit(|row| {
            row.decode_started_at.get_or_insert(at);
            row.outstanding_at_decode = Some(outstanding);
            row.waited_for_budget = waited;
        });
        self
    }

    /// The worker put the result down. Samples RSS here rather than at join,
    /// because what a decode cost the process is a fact about the moment it
    /// stopped holding its own buffers.
    pub fn finished(self) -> Self {
        let at = Instant::now();
        let rss = crate::progress::process_resident_bytes();
        self.edit(|row| {
            row.finished_at.get_or_insert(at);
            row.rss_at_finish = rss;
        });
        self
    }

    pub fn joined(self) -> Self {
        let at = Instant::now();
        self.edit(|row| {
            row.joined_at.get_or_insert(at);
        });
        self
    }

    pub fn bytes(self, source: Option<u64>, output: Option<u64>) -> Self {
        self.edit(|row| {
            row.source_bytes = source;
            row.output_bytes = output;
        });
        self
    }

    /// What the job prepared itself and what it took from another job's work,
    /// kept or not. Recorded by the worker rather than derived at join, so
    /// `prepared + reused = retained + discarded` is a check on the merge's
    /// accounting and not a definition of it.
    ///
    /// The two are separate because only the first is work. Adding them into
    /// one "produced" column reports a plan against bytes it never decoded.
    pub fn prepared(self, newly: u64, reused: u64) -> Self {
        self.edit(|row| {
            row.prepared_bytes = Some(newly);
            row.reused_bytes = Some(reused);
        });
        self
    }

    pub fn items(self, items: u64) -> Self {
        self.edit(|row| row.items = Some(items));
        self
    }

    pub fn cache(self, result: CacheResult) -> Self {
        self.edit(|row| row.cache_result = Some(result));
        self
    }

    /// What the merge kept and what it threw away, and why any of it was
    /// thrown away. A job with `discarded_bytes` and no reason is a hole in
    /// the instrumentation, and the report prints it as one.
    pub fn merged(self, retained: u64, discarded: u64, reason: Option<String>) -> Self {
        self.edit(|row| {
            row.retained_bytes = Some(retained);
            row.discarded_bytes = Some(discarded);
            row.discard_reason = reason;
        });
        self
    }
}

/// Every row, in the order the jobs were opened.
pub fn snapshot() -> Jobs {
    let table = TABLE.lock().unwrap_or_else(|poison| poison.into_inner());
    Jobs {
        rows: table.clone(),
        dropped: DROPPED.load(Ordering::Relaxed),
        per_asset: per_asset(),
        capacity: table.capacity(),
    }
}
