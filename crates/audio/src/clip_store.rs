use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use assets::{AssetNamespace, NamespaceSoundIwd, SoundCatalog};
use bevy::prelude::*;

use crate::pcm::{PcmAudio, decode_audio_bytes};
use crate::start::SoundClass;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ClipKey {
    Loaded(usize),
    Streamed {
        ns: AssetNamespace,
        dir: String,
        name: String,
    },
}

/// A queued clip and the instant it was queued, so a worker can say how long
/// the job sat before anyone picked it up. The wait is the queue's, not the
/// clip's: it is what the store owed and could not pay yet.
struct ClipJob {
    key: ClipKey,
    queued_at: Instant,
}

/// Which decoder a clip went through. Every clip takes exactly one of these,
/// and they cost wildly different things — an external process per XWMA clip,
/// a loop over bytes for everything else — so the report separates them rather
/// than averaging one number over all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipPath {
    /// Linear PCM straight out of the zone, converted to float and nothing else.
    Pcm,
    /// T5 ADPCM, decoded in process.
    Adpcm,
    /// T5 XWMA, decoded by an external `ffmpeg` or read from the artifact cache.
    Xwma,
    /// A clip read out of an IWD rather than out of the zone.
    Streamed,
    /// No decoder claims the key: the bank holds nothing at that index, or it
    /// holds a clip in a format none of the four above reads. Counted here so
    /// it cannot hide inside another path's failures.
    Unresolved,
}

impl ClipPath {
    pub const COUNT: usize = Self::Unresolved as usize + 1;

    pub const ALL: [Self; Self::COUNT] = [
        Self::Pcm,
        Self::Adpcm,
        Self::Xwma,
        Self::Streamed,
        Self::Unresolved,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Pcm => "pcm",
            Self::Adpcm => "t5 adpcm",
            Self::Xwma => "t5 xwma",
            Self::Streamed => "streamed",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ClipError {
    Decode,
    Read,
    QueueClosed,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedPcm {
    samples: Arc<[f32]>,
    channels: u16,
    sample_rate: u32,
}

#[derive(Clone)]
struct LoadedClipKey {
    encoded: Arc<[u8]>,
    format: i32,
    rate: u32,
    bits: i32,
    channels: i32,
    samples: u32,
    block_size: u32,
    seek_table: Vec<u32>,
}

impl PartialEq for LoadedClipKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.encoded, &other.encoded)
            && self.format == other.format
            && self.rate == other.rate
            && self.bits == other.bits
            && self.channels == other.channels
            && self.samples == other.samples
            && self.block_size == other.block_size
            && self.seek_table == other.seek_table
    }
}

impl Eq for LoadedClipKey {}

impl std::hash::Hash for LoadedClipKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.encoded) as *const u8 as usize).hash(state);
        self.format.hash(state);
        self.rate.hash(state);
        self.bits.hash(state);
        self.channels.hash(state);
        self.samples.hash(state);
        self.block_size.hash(state);
        self.seek_table.hash(state);
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum ResidentClipKey {
    Common(LoadedClipKey),
    Map(LoadedClipKey),
    Streamed(ClipKey),
}

impl ResidentClipKey {
    fn common(&self) -> bool {
        matches!(self, Self::Common(_))
    }
}

fn resident_clip_key(bank: &SoundCatalog, key: &ClipKey) -> Option<ResidentClipKey> {
    let ClipKey::Loaded(index) = key else {
        return Some(ResidentClipKey::Streamed(key.clone()));
    };
    let sound = bank.pcm_at(*index)?;
    let common = matches!(
        sound.zone.as_str(),
        "code_post_gfx_mp"
            | "localized_code_post_gfx_mp"
            | "patch_mp"
            | "common_mp"
            | "localized_common_mp"
    );
    let loaded = LoadedClipKey {
        encoded: sound.encoded_arc(),
        format: sound.format(),
        rate: sound.rate,
        bits: sound.bits(),
        channels: sound.channels(),
        samples: sound.samples,
        block_size: sound.block_size,
        seek_table: sound.seek_table.clone(),
    };
    Some(if common {
        ResidentClipKey::Common(loaded)
    } else {
        ResidentClipKey::Map(loaded)
    })
}

#[derive(Default)]
struct ResidentClipCacheInner {
    profile_id: u64,
    bank: std::sync::Weak<SoundCatalog>,
    prepared: HashMap<ResidentClipKey, PreparedPcm>,
    dry_handles: HashMap<ResidentClipKey, Handle<PcmAudio>>,
}

#[derive(Resource, Clone, Default)]
pub(crate) struct ResidentClipCache(Arc<Mutex<ResidentClipCacheInner>>);

impl ResidentClipCache {
    fn use_profile(&self, profile_id: u64) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if inner.profile_id != profile_id {
            inner.prepared.clear();
            inner.dry_handles.clear();
            inner.profile_id = profile_id;
        }
    }

    fn use_bank(&self, bank: &Arc<SoundCatalog>) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if std::sync::Weak::ptr_eq(&inner.bank, &Arc::downgrade(bank)) {
            return;
        }
        inner.prepared.retain(|key, _| key.common());
        inner.dry_handles.retain(|key, _| key.common());
        inner.bank = Arc::downgrade(bank);
    }

    fn ready(&self, profile_id: u64, key: &ResidentClipKey) -> Option<PreparedPcm> {
        let inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        (profile_id != 0 && inner.profile_id == profile_id)
            .then(|| inner.prepared.get(key).cloned())
            .flatten()
    }

    fn remember(&self, profile_id: u64, key: ResidentClipKey, pcm: PreparedPcm) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if profile_id != 0 && inner.profile_id == profile_id {
            inner.prepared.entry(key).or_insert(pcm);
        }
    }

    fn dry_handle(
        &self,
        profile_id: u64,
        key: ResidentClipKey,
        assets: &mut Assets<PcmAudio>,
        pcm: &PcmAudio,
    ) -> Option<(Handle<PcmAudio>, bool)> {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if profile_id == 0 || inner.profile_id != profile_id {
            return None;
        }
        if let Some(handle) = inner.dry_handles.get(&key)
            && assets.get(handle.id()).is_some()
        {
            return Some((handle.clone(), true));
        }
        let handle = assets.add(pcm.clone());
        inner.dry_handles.insert(key, handle.clone());
        Some((handle, false))
    }
}

impl PreparedPcm {
    pub(crate) fn into_audio(self) -> Option<PcmAudio> {
        PcmAudio::from_prepared(self.samples, self.channels, self.sample_rate)
    }
}

/// How many queued jobs a worker takes at once. It bounds how long the store
/// can hold a clip's outcome back — every job in a batch is published when the
/// last of them is prepared — and it is the XWMA decoder's batch, which is the
/// only decoder here that gains anything from the grouping.
pub const PREP_BATCH: usize = 64;

/// Kept after the sender so the channel closes before these workers are joined.
struct ClipWorkers {
    handles: Vec<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl Drop for ClipWorkers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let mut joined = 0;
        for handle in self.handles.drain(..) {
            if handle.join().is_err() {
                diag::warn!(Audio, "audio: clip prep worker panicked during retirement");
            }
            joined += 1;
        }
        if joined != 0 {
            diag::info!(Audio, "audio: clip prep workers joined={joined}");
        }
    }
}

#[derive(Resource)]
pub struct ClipStore {
    bank: Arc<SoundCatalog>,
    tx: Sender<ClipJob>,
    queued: HashSet<ClipKey>,
    outcomes: Arc<Mutex<HashMap<ClipKey, Result<PreparedPcm, ClipError>>>>,
    workers: ClipWorkers,
    clip_cache: Option<ResidentClipCache>,
    common_profile_id: u64,
    reused_clips: usize,
    reused_bytes: u64,

    match_live: bool,
    late_prepares: u32,
}

impl ClipStore {
    pub fn start(bank: Arc<SoundCatalog>, iwd: Option<Arc<NamespaceSoundIwd>>) -> Self {
        Self::start_with_common(bank, iwd, 0, None)
    }

    pub(crate) fn start_with_common(
        bank: Arc<SoundCatalog>,
        iwd: Option<Arc<NamespaceSoundIwd>>,
        common_profile_id: u64,
        clip_cache: Option<ResidentClipCache>,
    ) -> Self {
        if let Some(cache) = &clip_cache {
            cache.use_profile(common_profile_id);
            cache.use_bank(&bank);
        }
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(1, 4))
            .unwrap_or(1);
        let (tx, rx) = channel::<ClipJob>();
        let rx = Arc::new(Mutex::new(rx));
        let outcomes = Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::with_capacity(workers);
        for slot in 0..workers {
            let rx = Arc::clone(&rx);
            let bank = Arc::clone(&bank);
            let iwd = iwd.clone();
            let outcomes = Arc::clone(&outcomes);
            let clip_cache = clip_cache.clone();
            let stop_worker = Arc::clone(&stop);

            let spawned = std::thread::Builder::new()
                .name(format!("clip-prep-{slot}"))
                .spawn(move || {
                    loop {
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        // A match queues every clip it needs at once, and one
                        // of the decoders below is an external process whose
                        // startup costs more than the decode. So a worker takes
                        // what is already waiting rather than one job at a time:
                        // the queue is where the batch comes from. A clip asked
                        // for on its own — a late prepare, a reload — still
                        // arrives alone and is prepared alone.
                        let jobs = {
                            let guard = rx.lock().unwrap_or_else(|poison| poison.into_inner());
                            let Ok(first) = guard.recv() else {
                                return;
                            };
                            let mut jobs = vec![first];
                            while jobs.len() < PREP_BATCH {
                                let Ok(next) = guard.try_recv() else { break };
                                jobs.push(next);
                            }
                            jobs
                        };
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        for job in &jobs {
                            QUEUE_WAIT_NS.fetch_add(
                                job.queued_at.elapsed().as_nanos() as u64,
                                Ordering::Relaxed,
                            );
                        }
                        let prepared = prepare_jobs(&bank, iwd.as_deref(), &jobs);
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        if let Some(cache) = &clip_cache {
                            for (job, result) in jobs.iter().zip(&prepared) {
                                if let Ok(pcm) = result
                                    && let Some(key) = resident_clip_key(&bank, &job.key)
                                {
                                    cache.remember(common_profile_id, key, pcm.clone());
                                }
                            }
                        }
                        let mut guard =
                            outcomes.lock().unwrap_or_else(|poison| poison.into_inner());
                        for (job, result) in jobs.into_iter().zip(prepared) {
                            guard.insert(job.key, result);
                        }
                    }
                });
            match spawned {
                Ok(handle) => handles.push(handle),
                Err(e) => diag::warn!(Audio, "audio: clip prep worker {slot} not started ({e})"),
            }
        }
        WORKERS.fetch_add(handles.len() as u64, Ordering::Relaxed);
        Self {
            bank,
            tx,
            queued: HashSet::new(),
            outcomes,
            workers: ClipWorkers { handles, stop },
            clip_cache,
            common_profile_id,
            reused_clips: 0,
            reused_bytes: 0,
            match_live: false,
            late_prepares: 0,
        }
    }

    pub fn workers(&self) -> usize {
        self.workers.handles.len()
    }

    pub fn reused_resident(&self) -> (usize, u64) {
        (self.reused_clips, self.reused_bytes)
    }

    pub(crate) fn resident_dry_handle(
        &self,
        clip: &ClipKey,
        assets: &mut Assets<PcmAudio>,
        pcm: &PcmAudio,
    ) -> Option<(Handle<PcmAudio>, bool)> {
        let cache = self.clip_cache.as_ref()?;
        let key = resident_clip_key(&self.bank, clip)?;
        cache.dry_handle(self.common_profile_id, key, assets, pcm)
    }

    pub fn arm_match_live(&mut self) {
        self.match_live = true;
    }

    /// This store's count. A store lives as long as one installed sound bank,
    /// so a map change starts a new one — which is the scope the console asks
    /// about. The process total is in [`clip_prep_cost`], for the exit hook
    /// that has no store to ask.
    pub fn late_prepares(&self) -> u32 {
        self.late_prepares
    }

    fn note_late(&mut self, key: &ClipKey) {
        if !self.match_live {
            return;
        }
        self.late_prepares = self.late_prepares.saturating_add(1);
        LATE.fetch_add(1, Ordering::Relaxed);
        diag::warn!(Audio, "audio: clip prepare after AudioReady ({key:?})");
    }

    pub fn request_alias(&mut self, ns: AssetNamespace, alias: &str) -> usize {
        let mut queued = 0;
        for key in clip_keys_for_alias(&self.bank, ns, alias) {
            if self.request(key) {
                queued += 1;
            }
        }
        queued
    }

    pub(crate) fn request(&mut self, key: ClipKey) -> bool {
        REQUESTS.fetch_add(1, Ordering::Relaxed);
        if self.ready(&key).is_some() {
            return false;
        }
        if let Some(pcm) = self.clip_cache.as_ref().and_then(|cache| {
            resident_clip_key(&self.bank, &key)
                .and_then(|source| cache.ready(self.common_profile_id, &source))
        }) {
            self.reused_clips += 1;
            self.reused_bytes += (pcm.samples.len() * size_of::<f32>()) as u64;
            self.outcomes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(key, Ok(pcm));
            return false;
        }
        if !self.queued.insert(key.clone()) {
            return false;
        }
        self.note_late(&key);
        QUEUED.fetch_add(1, Ordering::Relaxed);
        let job = ClipJob {
            key: key.clone(),
            queued_at: Instant::now(),
        };
        if self.tx.send(job).is_err() {
            self.outcomes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(key, Err(ClipError::QueueClosed));
            return false;
        }
        true
    }

    pub(crate) fn ready(&self, key: &ClipKey) -> Option<Result<PcmAudio, ClipError>> {
        let guard = self
            .outcomes
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match guard.get(key) {
            Some(Ok(prepared)) => prepared
                .clone()
                .into_audio()
                .map(Ok)
                .or(Some(Err(ClipError::Decode))),
            Some(Err(err)) => Some(Err(err.clone())),
            None => None,
        }
    }
}

pub(crate) struct PendingOneshot {
    pub namespace: AssetNamespace,
    pub alias: String,
    /// The alias's row in the bank it was bound against, when the caller had
    /// one; the start resumes on that row rather than on a name lookup.
    pub bound: Option<usize>,
    pub variant: usize,
    pub volume: f32,
    pub pitch: f32,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
    pub clip: ClipKey,
    pub layer: Option<String>,
    pub class: SoundClass,
    pub epoch: u64,
    pub deadline: Instant,
}

#[derive(Resource, Default)]
pub(crate) struct PendingStarts {
    pub entries: Vec<PendingOneshot>,
}

impl PendingStarts {
    pub fn push_oneshot(&mut self, pending: PendingOneshot) {
        self.entries.push(pending);
    }

    pub fn cancel_alias(
        &mut self,
        namespace: AssetNamespace,
        alias: &str,
        snd_ent: Option<u32>,
        epoch: u64,
    ) {
        self.entries.retain(|e| {
            !(e.namespace == namespace
                && e.alias == alias
                && e.snd_ent == snd_ent
                && e.epoch == epoch
                && e.class.scope() == crate::backend::AudioScope::Match)
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

pub(crate) fn deadline_for(class: SoundClass) -> Instant {
    Instant::now() + class.oneshot_wait()
}

pub(crate) fn clip_keys_for_alias(
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
) -> Vec<ClipKey> {
    let mut seen_alias = HashSet::new();
    let mut seen_key = HashSet::new();
    let mut out = Vec::new();
    collect_clip_keys(bank, ns, alias, 0, &mut seen_alias, &mut seen_key, &mut out);
    out
}

fn collect_clip_keys(
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
    depth: u8,
    seen_alias: &mut HashSet<String>,
    seen_key: &mut HashSet<ClipKey>,
    out: &mut Vec<ClipKey>,
) {
    if depth > 10 || !seen_alias.insert(alias.to_owned()) {
        return;
    }
    let Some(sound) = bank.sound_in(ns, alias) else {
        return;
    };
    for (vi, row) in sound.aliases.iter().enumerate() {
        if let Some(idx) = row.loaded.bound_index() {
            let key = ClipKey::Loaded(idx);
            if seen_key.insert(key.clone()) {
                out.push(key);
            }
        } else if let Some((sns, dir, name)) = bank.streamed_for_variant(ns, alias, vi) {
            let key = ClipKey::Streamed { ns: sns, dir, name };
            if seen_key.insert(key.clone()) {
                out.push(key);
            }
        }
        if let Some(sec) = row.secondary.as_deref()
            && !sec.is_empty()
        {
            collect_clip_keys(bank, ns, sec, depth + 1, seen_alias, seen_key, out);
        }
    }
}

pub(crate) fn clip_key_for_variant(
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
    bound: Option<usize>,
    variant: usize,
    loaded_name: Option<&str>,
    loaded_ns: Option<AssetNamespace>,
) -> Option<ClipKey> {
    if let (Some(name), Some(loaded_ns)) = (loaded_name, loaded_ns)
        && let Some(idx) = bank.loaded_index_in(loaded_ns, name)
    {
        return Some(ClipKey::Loaded(idx));
    }
    let sound = match bound {
        Some(index) => bank.sound_at(index),
        None => bank.sound_in(ns, alias),
    };
    if let Some(idx) = sound
        .and_then(|s| s.aliases.get(variant))
        .and_then(|row| row.loaded.bound_index())
    {
        return Some(ClipKey::Loaded(idx));
    }
    match bound {
        Some(index) => bank.streamed_for_variant_at(index, variant),
        None => bank.streamed_for_variant(ns, alias, variant),
    }
    .map(|(sns, dir, name)| ClipKey::Streamed { ns: sns, dir, name })
}

static WORKERS: AtomicU64 = AtomicU64::new(0);
static REQUESTS: AtomicU64 = AtomicU64::new(0);
static QUEUED: AtomicU64 = AtomicU64::new(0);
static QUEUE_WAIT_NS: AtomicU64 = AtomicU64::new(0);
static LATE: AtomicU64 = AtomicU64::new(0);
static PREPARED: [AtomicU64; ClipPath::COUNT] = [const { AtomicU64::new(0) }; ClipPath::COUNT];
static FAILED: [AtomicU64; ClipPath::COUNT] = [const { AtomicU64::new(0) }; ClipPath::COUNT];
static WALL_NS: [AtomicU64; ClipPath::COUNT] = [const { AtomicU64::new(0) }; ClipPath::COUNT];
static SAMPLE_BYTES: [AtomicU64; ClipPath::COUNT] = [const { AtomicU64::new(0) }; ClipPath::COUNT];

/// One decoder's share of the clip preparation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipPathCost {
    pub prepared: u64,
    pub failed: u64,
    /// Worker time inside this decoder, summed over the prep threads.
    pub wall_ms: f64,
    /// Resident `f32` samples this decoder produced. The store keeps them for
    /// the life of the match, so this is memory and not throughput.
    pub sample_bytes: u64,
}

/// What preparing the match's clips cost this process.
///
/// `requests` counts asks and `queued` counts jobs: the walk reaches one clip
/// from several aliases and several weapons, and the difference between the
/// two is what the store's own dedupe already saves. Every duration is summed
/// over the prep workers, which run several at a time, so none of them is a
/// stretch of the load.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipPrepCost {
    /// Prep threads that actually started, across every store this process
    /// opened. A match teardown closes one store and a reload opens another.
    pub workers: u64,
    pub requests: u64,
    pub queued: u64,
    pub queue_wait_ms: f64,
    pub late: u64,
    pub paths: Vec<(ClipPath, ClipPathCost)>,
}

pub fn clip_prep_cost() -> ClipPrepCost {
    ClipPrepCost {
        workers: WORKERS.load(Ordering::Relaxed),
        requests: REQUESTS.load(Ordering::Relaxed),
        queued: QUEUED.load(Ordering::Relaxed),
        queue_wait_ms: QUEUE_WAIT_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        late: LATE.load(Ordering::Relaxed),
        paths: ClipPath::ALL
            .into_iter()
            .map(|path| {
                let slot = path as usize;
                (
                    path,
                    ClipPathCost {
                        prepared: PREPARED[slot].load(Ordering::Relaxed),
                        failed: FAILED[slot].load(Ordering::Relaxed),
                        wall_ms: WALL_NS[slot].load(Ordering::Relaxed) as f64 / 1.0e6,
                        sample_bytes: SAMPLE_BYTES[slot].load(Ordering::Relaxed),
                    },
                )
            })
            .collect(),
    }
}

fn note_prepared(path: ClipPath, prepare_at: Instant, result: Result<&PreparedPcm, &ClipError>) {
    note_wall(path, prepare_at.elapsed());
    note_outcome(path, result);
}

/// Worker time this decoder spent, whether it spent it on one clip or on the
/// sixty-four it was handed together. A batched decoder has no per-clip time to
/// report — what it has is a total and a count, and the report divides them.
fn note_wall(path: ClipPath, wall: std::time::Duration) {
    WALL_NS[path as usize].fetch_add(wall.as_nanos() as u64, Ordering::Relaxed);
}

fn note_outcome(path: ClipPath, result: Result<&PreparedPcm, &ClipError>) {
    let slot = path as usize;
    match result {
        Ok(prepared) => {
            PREPARED[slot].fetch_add(1, Ordering::Relaxed);
            SAMPLE_BYTES[slot].fetch_add(
                (prepared.samples.len() * size_of::<f32>()) as u64,
                Ordering::Relaxed,
            );
        }
        Err(_) => {
            FAILED[slot].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// One worker's drained jobs, prepared together.
///
/// The XWMA clips among them go to their decoder in one call, because that
/// decoder leaves the process: one `ffmpeg` for the batch instead of one per
/// clip. Every other path is a loop over bytes in this process and is prepared
/// one clip at a time, exactly as before.
fn prepare_jobs(
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    jobs: &[ClipJob],
) -> Vec<Result<PreparedPcm, ClipError>> {
    let mut out: Vec<Option<Result<PreparedPcm, ClipError>>> = vec![None; jobs.len()];

    let asked: Vec<(usize, &assets::LoadedSoundPcm)> = jobs
        .iter()
        .enumerate()
        .filter_map(|(i, job)| match job.key {
            ClipKey::Loaded(index) => {
                let sound = bank.pcm_at(index)?;
                (loaded_path(sound) == Some(ClipPath::Xwma)).then_some((i, sound))
            }
            ClipKey::Streamed { .. } => None,
        })
        .collect();
    if !asked.is_empty() {
        let clips: Vec<assets::XwmaClip<'_>> = asked
            .iter()
            .map(|(_, sound)| assets::XwmaClip {
                packets: sound.encoded_bytes(),
                seek_table: &sound.seek_table,
                channels: sound.channels().max(0) as u32,
                rate: sound.rate,
            })
            .collect();
        let decode_at = Instant::now();
        let decoded = assets::decode_t5_xwma_batch(&clips);
        note_wall(ClipPath::Xwma, decode_at.elapsed());
        for ((i, sound), pcm) in asked.into_iter().zip(decoded) {
            let result = pcm
                .map_err(|_| ClipError::Decode)
                .and_then(|bytes| pcm_from_bytes(16, &bytes, sound.channels(), sound.rate));
            note_outcome(ClipPath::Xwma, result.as_ref());
            out[i] = Some(result);
        }
    }

    for (i, job) in jobs.iter().enumerate() {
        if out[i].is_some() {
            continue;
        }
        let prepare_at = Instant::now();
        let (path, result) = prepare_clip_now(bank, iwd, &job.key);
        note_prepared(path, prepare_at, result.as_ref());
        out[i] = Some(result);
    }
    out.into_iter()
        .map(|result| result.unwrap_or(Err(ClipError::Decode)))
        .collect()
}

fn prepare_clip_now(
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    key: &ClipKey,
) -> (ClipPath, Result<PreparedPcm, ClipError>) {
    match key {
        ClipKey::Loaded(index) => {
            let Some(sound) = bank.pcm_at(*index) else {
                return (ClipPath::Unresolved, Err(ClipError::Decode));
            };
            let Some(path) = loaded_path(sound) else {
                return (ClipPath::Unresolved, Err(ClipError::Decode));
            };
            (path, prepare_loaded(sound))
        }
        ClipKey::Streamed { ns, dir, name } => (
            ClipPath::Streamed,
            prepare_streamed(iwd, *ns, dir, name).and_then(|pcm| pcm.ok_or(ClipError::Decode)),
        ),
    }
}

/// Which decoder `prepare_loaded` will reach for, decided the same way it
/// decides — the two read the same fields in the same order, so a clip cannot
/// be counted under one path and decoded by another.
fn loaded_path(sound: &assets::LoadedSoundPcm) -> Option<ClipPath> {
    if sound.t5_adpcm_bytes().is_some() {
        Some(ClipPath::Adpcm)
    } else if sound.is_t5_xwma() {
        Some(ClipPath::Xwma)
    } else if sound.format() == assets::MSS_PCM {
        Some(ClipPath::Pcm)
    } else {
        None
    }
}

fn prepare_loaded(sound: &assets::LoadedSoundPcm) -> Result<PreparedPcm, ClipError> {
    if let Some(bytes) = sound.t5_adpcm_bytes() {
        let channels = u16::try_from(sound.channels().max(1)).map_err(|_| ClipError::Decode)?;
        let pcm = crate::pcm::t5_stream::decode_adpcm(
            bytes,
            sound.samples,
            sound.rate,
            u32::from(channels),
        )
        .ok_or(ClipError::Decode)?;
        return Ok(PreparedPcm {
            samples: Arc::clone(pcm.samples()),
            channels: pcm.channel_count(),
            sample_rate: pcm.rate(),
        });
    }
    let decoded;
    let (bits, bytes) = if sound.is_t5_xwma() {
        decoded = assets::decode_t5_xwma(
            sound.encoded_bytes(),
            &sound.seek_table,
            sound.channels().max(0) as u32,
            sound.rate,
        )
        .map_err(|_| ClipError::Decode)?;
        (16, decoded.as_slice())
    } else if sound.format() == 1 {
        (sound.bits(), sound.encoded_bytes())
    } else {
        return Err(ClipError::Decode);
    };
    pcm_from_bytes(bits, bytes, sound.channels(), sound.rate)
}

/// Encoded samples as the mixer wants them: interleaved `f32`, a whole number
/// of frames, and never empty — a clip with no samples is a failed decode and
/// not a silent clip.
fn pcm_from_bytes(
    bits: i32,
    bytes: &[u8],
    channels: i32,
    rate: u32,
) -> Result<PreparedPcm, ClipError> {
    let mut samples: Vec<f32> = match bits {
        8 => bytes.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        16 => bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect(),
        _ => return Err(ClipError::Decode),
    };
    let lanes = channels.max(1) as usize;
    samples.truncate(samples.len() / lanes * lanes);
    if samples.is_empty() {
        return Err(ClipError::Decode);
    }
    let channels = u16::try_from(channels.max(1)).map_err(|_| ClipError::Decode)?;
    Ok(PreparedPcm {
        samples: samples.into(),
        channels,
        sample_rate: rate.max(1),
    })
}

fn prepare_streamed(
    iwd: Option<&NamespaceSoundIwd>,
    ns: AssetNamespace,
    dir: &str,
    name: &str,
) -> Result<Option<PreparedPcm>, ClipError> {
    let iwd = iwd.ok_or(ClipError::Read)?;
    let rel = format!("{dir}/{name}");
    let bytes: Vec<u8> = match iwd.read_sound(ns, &rel) {
        Some(Ok(bytes)) => bytes,
        Some(Err(e)) => {
            diag::warn!(Audio, "audio: IWD read `{rel}` failed: {e}");
            return Err(ClipError::Read);
        }
        None => {
            diag::warn!(
                Audio,
                "audio: streamed IWD miss `{}:{rel}` (typed gap)",
                ns.as_str()
            );
            return Err(ClipError::Read);
        }
    };
    let pcm = if ns == AssetNamespace::T5 && !bytes.starts_with(b"RIFF") {
        crate::pcm::t5_stream::decode(&bytes)
    } else {
        decode_audio_bytes(&bytes)
    };
    let Some(pcm) = pcm else {
        diag::warn!(
            Audio,
            "audio: failed to decode streamed `{}:{rel}`",
            ns.as_str()
        );
        return Err(ClipError::Decode);
    };
    Ok(Some(PreparedPcm {
        samples: Arc::clone(pcm.samples()),
        channels: pcm.channel_count(),
        sample_rate: pcm.rate(),
    }))
}
