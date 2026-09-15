use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
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

#[derive(Resource)]
pub struct ClipStore {
    bank: Arc<SoundCatalog>,
    tx: Sender<ClipJob>,
    queued: HashSet<ClipKey>,
    outcomes: Arc<Mutex<HashMap<ClipKey, Result<PreparedPcm, ClipError>>>>,
    workers: usize,

    match_live: bool,
    late_prepares: u32,
}

impl ClipStore {
    pub fn start(bank: Arc<SoundCatalog>, iwd: Option<Arc<NamespaceSoundIwd>>) -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(1, 4))
            .unwrap_or(1);
        let (tx, rx) = channel::<ClipJob>();
        let rx = Arc::new(Mutex::new(rx));
        let outcomes = Arc::new(Mutex::new(HashMap::new()));
        let mut started_workers = 0;
        for slot in 0..workers {
            let rx = Arc::clone(&rx);
            let bank = Arc::clone(&bank);
            let iwd = iwd.clone();
            let outcomes = Arc::clone(&outcomes);

            let spawned = std::thread::Builder::new()
                .name(format!("clip-prep-{slot}"))
                .spawn(move || {
                    loop {
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
                        for job in &jobs {
                            QUEUE_WAIT_NS.fetch_add(
                                job.queued_at.elapsed().as_nanos() as u64,
                                Ordering::Relaxed,
                            );
                        }
                        let prepared = prepare_jobs(&bank, iwd.as_deref(), &jobs);
                        let mut guard =
                            outcomes.lock().unwrap_or_else(|poison| poison.into_inner());
                        for (job, result) in jobs.into_iter().zip(prepared) {
                            guard.insert(job.key, result);
                        }
                    }
                });
            match spawned {
                Ok(_) => started_workers += 1,
                Err(e) => diag::warn!(Audio, "audio: clip prep worker {slot} not started ({e})"),
            }
        }
        WORKERS.fetch_add(started_workers as u64, Ordering::Relaxed);
        Self {
            bank,
            tx,
            queued: HashSet::new(),
            outcomes,
            workers: started_workers,
            match_live: false,
            late_prepares: 0,
        }
    }

    pub fn workers(&self) -> usize {
        self.workers
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
    variant: usize,
    loaded_name: Option<&str>,
    loaded_ns: Option<AssetNamespace>,
) -> Option<ClipKey> {
    if let (Some(name), Some(loaded_ns)) = (loaded_name, loaded_ns)
        && let Some(idx) = bank.loaded_index_in(loaded_ns, name)
    {
        return Some(ClipKey::Loaded(idx));
    }
    if let Some(idx) = bank
        .sound_in(ns, alias)
        .and_then(|s| s.aliases.get(variant))
        .and_then(|row| row.loaded.bound_index())
    {
        return Some(ClipKey::Loaded(idx));
    }
    bank.streamed_for_variant(ns, alias, variant)
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

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{
        AssetEdge, AssetEdgeReason, CapturedAlias, CapturedSound, LoadedSoundPcm, MSS_PCM, ZoneGame,
    };

    const FIRE_BYTES: [u8; 6] = [0x00, 0x80, 0x00, 0x00, 0xff, 0x7f];
    const TAIL_BYTES: [u8; 4] = [0x00, 0x40, 0x00, 0xc0];

    fn loaded(
        name: &str,
        game: ZoneGame,
        format: i32,
        channels: i32,
        bytes: &[u8],
    ) -> LoadedSoundPcm {
        let mut pcm =
            LoadedSoundPcm::captured(name, format, 48000, channels, bytes.to_vec(), Vec::new());
        pcm.game = game;
        pcm
    }

    fn row(loaded_name: &str, secondary: Option<&str>) -> CapturedAlias {
        CapturedAlias {
            loaded_name: Some(loaded_name.to_owned()),
            secondary: secondary.map(str::to_owned),
            file_type: Some(1),
            // What the walk leaves behind: a name and no row yet. Binding it is
            // `resolve_loaded_edges`' job, and this test asks it to do it.
            loaded: AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            ..CapturedAlias::default()
        }
    }

    fn sound(name: &str, rows: Vec<CapturedAlias>) -> CapturedSound {
        CapturedSound {
            name: name.to_owned(),
            aliases: rows,
            ..CapturedSound::default()
        }
    }

    /// A host zone and a donor zone that both call a sound `weapon_fire` and
    /// both ship a clip called `weap_fire`, absorbed the way a match load
    /// absorbs common_mp into the map.
    fn two_namespace_bank() -> SoundCatalog {
        let mut host = SoundCatalog::default();
        host.set_capture_game(ZoneGame::Iw4);
        host.ingest_loaded(loaded("weap_fire", ZoneGame::Iw4, MSS_PCM, 1, &FIRE_BYTES));
        host.ingest_loaded(loaded("weap_tail", ZoneGame::Iw4, MSS_PCM, 2, &TAIL_BYTES));
        host.ingest_loaded(loaded("weap_broken", ZoneGame::Iw4, 99, 1, &FIRE_BYTES));
        host.ingest_sound(sound(
            "weapon_fire",
            vec![
                row("weap_fire", Some("weapon_tail")),
                // Same clip again: a second variant of one alias must not
                // queue the same decode twice.
                row("weap_fire", None),
                row("weap_broken", None),
            ],
        ));
        host.ingest_sound(sound("weapon_tail", vec![row("weap_tail", None)]));

        let mut donor = SoundCatalog::default();
        donor.set_capture_game(ZoneGame::T5);
        donor.ingest_loaded(loaded("weap_fire", ZoneGame::T5, MSS_PCM, 1, &TAIL_BYTES));
        donor.ingest_sound(sound("weapon_fire", vec![row("weap_fire", None)]));

        host.absorb(donor);
        host
    }

    fn settle(store: &ClipStore, key: &ClipKey) -> Result<PcmAudio, ClipError> {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(outcome) = store.ready(key) {
                return outcome;
            }
            assert!(Instant::now() < deadline, "clip {key:?} never settled");
            std::thread::yield_now();
        }
    }

    /// One alias, carried end to end: captured by two zones, absorbed into one
    /// bank, bound to its rows, walked into clip keys, decoded by the prep
    /// workers, and finally cancelled. Every stage has to agree on which clip
    /// `iw4:weapon_fire` means — the namespace, the variant and the alias chain
    /// are the same identity in all of them, and a donor zone that ships a
    /// same-named clip may not be reachable through the host's alias.
    #[test]
    fn one_alias_keeps_its_identity_from_capture_through_decode_to_cancel() {
        let bank = two_namespace_bank();

        // The two zones' clips live side by side, each reachable only in its
        // own namespace.
        assert_eq!(
            bank.loaded_index_in(AssetNamespace::Iw4, "weap_fire"),
            Some(0)
        );
        assert_eq!(
            bank.loaded_index_in(AssetNamespace::T5, "weap_fire"),
            Some(3)
        );

        // `resolve_loaded_edges` bound every alias row that named a clip.
        let census = bank.loaded_edge_census();
        assert_eq!((census.n, census.bound, census.unresolved), (5, 5, 0));

        // The alias walk: variant order, the secondary chain inlined after the
        // row that named it, and no key twice.
        let keys = clip_keys_for_alias(&bank, AssetNamespace::Iw4, "weapon_fire");
        assert_eq!(
            keys,
            vec![ClipKey::Loaded(0), ClipKey::Loaded(1), ClipKey::Loaded(2)]
        );
        assert_eq!(
            clip_keys_for_alias(&bank, AssetNamespace::T5, "weapon_fire"),
            vec![ClipKey::Loaded(3)]
        );

        let bank = Arc::new(bank);
        let mut store = ClipStore::start(Arc::clone(&bank), None);
        assert!(store.workers() >= 1);

        // Requesting the alias queues each distinct clip exactly once.
        assert_eq!(store.request_alias(AssetNamespace::Iw4, "weapon_fire"), 3);
        assert_eq!(store.request_alias(AssetNamespace::Iw4, "weapon_fire"), 0);

        // 16-bit LE PCM, converted to float and nothing else.
        let fire = settle(&store, &keys[0]).expect("fire decodes");
        assert_eq!(&**fire.samples(), &[-1.0, 0.0, 32767.0 / 32768.0]);
        assert_eq!((fire.channel_count(), fire.rate()), (1, 48000));
        let tail = settle(&store, &keys[1]).expect("tail decodes");
        assert_eq!(&**tail.samples(), &[0.5, -0.5]);
        assert_eq!(tail.channel_count(), 2);

        // A clip the workers could not decode fails once and stays failed: it
        // is neither retried nor left pending for a caller to wait on forever.
        assert!(matches!(settle(&store, &keys[2]), Err(ClipError::Decode)));
        assert!(!store.request(keys[2].clone()));
        assert_eq!(store.request_alias(AssetNamespace::Iw4, "weapon_fire"), 0);
        assert_eq!(store.outcomes.lock().unwrap().len(), 3);

        // Decoding is the store's; the shared bank still holds exactly the
        // bytes the zone shipped.
        assert_eq!(bank.pcm_at(0).unwrap().encoded_bytes(), &FIRE_BYTES);
        assert_eq!(bank.pcm_at(3).unwrap().encoded_bytes(), &TAIL_BYTES);

        // A queue whose receiver is gone is the same kind of terminal: the
        // request fails, records why, and does not leave the key pending.
        let (tx, rx) = channel();
        drop(rx);
        let mut closed = ClipStore {
            bank: Arc::clone(&bank),
            tx,
            queued: HashSet::new(),
            outcomes: Arc::default(),
            workers: 0,
            match_live: false,
            late_prepares: 0,
        };
        assert!(!closed.request(keys[0].clone()));
        assert!(matches!(
            closed.ready(&keys[0]),
            Some(Err(ClipError::QueueClosed))
        ));
        assert!(!closed.request(keys[0].clone()));
        assert_eq!(closed.outcomes.lock().unwrap().len(), 1);

        // Stop is scoped by the same identity the walk used: namespace, sound
        // entity and match epoch. Everything outside that scope survives.
        let pending = |namespace, snd_ent, epoch, clip: &ClipKey| PendingOneshot {
            namespace,
            alias: "weapon_fire".into(),
            variant: 0,
            volume: 0.5,
            pitch: 1.1,
            origin_inches: None,
            snd_ent,
            clip: clip.clone(),
            layer: None,
            class: SoundClass::World,
            epoch,
            deadline: deadline_for(SoundClass::World),
        };
        let mut starts = PendingStarts::default();
        for entry in [
            pending(AssetNamespace::Iw4, Some(1), 3, &keys[0]),
            pending(AssetNamespace::Iw4, Some(2), 3, &keys[0]),
            pending(AssetNamespace::T5, Some(1), 3, &ClipKey::Loaded(3)),
            pending(AssetNamespace::Iw4, Some(1), 4, &keys[0]),
        ] {
            starts.push_oneshot(entry);
        }
        starts.cancel_alias(AssetNamespace::Iw4, "weapon_fire", Some(1), 3);
        assert_eq!(
            starts
                .entries
                .iter()
                .map(|e| (e.namespace, e.snd_ent, e.epoch))
                .collect::<Vec<_>>(),
            vec![
                (AssetNamespace::Iw4, Some(2), 3),
                (AssetNamespace::T5, Some(1), 3),
                (AssetNamespace::Iw4, Some(1), 4),
            ]
        );
    }
}
