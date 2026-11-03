use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use asset_audio::SoundCatalog;
use asset_core::FamilyId;
use assets::NamespaceSoundIwd;
use bevy::prelude::*;

use crate::media::PcmBuffer;
use crate::media_queue::{ClipJob, MediaJobQueue, MediaPriority, QueueRefusal};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClipKey {
    revision: u64,
    locator: ClipLocator,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ClipLocator {
    Loaded(usize),
    Streamed {
        ns: FamilyId,
        dir: String,
        name: String,
    },
}

impl ClipKey {
    fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn is_loaded(&self) -> bool {
        matches!(&self.locator, ClipLocator::Loaded(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipPath {
    /// Linear PCM read in place from the zone.
    Pcm,
    Container,
    Native,
    External,
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
        Self::Container,
        Self::Native,
        Self::External,
        Self::Streamed,
        Self::Unresolved,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Pcm => "pcm",
            Self::Container => "container",
            Self::Native => "native",
            Self::External => "external",
            Self::Streamed => "streamed",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ClipError {
    Decode,
    UnsupportedCodec(asset_audio::ExternalAudioCodec),
    MetadataMismatch,
    ForeignOwner,
    Native(asset_audio::NativeAudioError),
    InvalidPcm(crate::media::PcmError),
    Read,
    QueueClosed,
    RequestLimit,
}

impl From<crate::pcm::DecodeError> for ClipError {
    fn from(error: crate::pcm::DecodeError) -> Self {
        match error {
            crate::pcm::DecodeError::Decode => Self::Decode,
            crate::pcm::DecodeError::Native(error) => Self::Native(error),
            crate::pcm::DecodeError::Read => Self::Read,
            crate::pcm::DecodeError::UnsupportedCodec(codec) => Self::UnsupportedCodec(codec),
            crate::pcm::DecodeError::MetadataMismatch => Self::MetadataMismatch,
            crate::pcm::DecodeError::Pcm(error) => Self::InvalidPcm(error),
        }
    }
}

pub(crate) enum MediaRequest {
    Existing,
    Resident,
    Submitted,
    Deferred,
    Refused,
}

// Bump when PCM decode, layout or trim rules change.
const PCM_CONVERSION_REVISION: u32 = 2;

#[derive(Clone, PartialEq, Eq, Hash)]
struct LoadedClipKey {
    family: FamilyId,
    content: [u8; 32],
    conversion_revision: u32,
    format: i32,
    rate: u32,
    bits: i32,
    channels: i32,
    samples: u32,
    block_size: u32,
    seek_table: Vec<u32>,
}

fn resident_clip_key(bank: &SoundCatalog, key: &ClipKey) -> Option<(LoadedClipKey, bool)> {
    if key.revision() != bank.revision() {
        return None;
    }
    let ClipLocator::Loaded(index) = &key.locator else {
        return None;
    };
    let sound = bank.pcm_at(*index)?;
    // External media has a locator, not a verified payload hash. Never reuse it
    // under the empty encoded-content hash shared by descriptor-only entries.
    if sound.external_media.is_some() {
        return None;
    }
    let common = asset_audio::audio_family(sound.game())
        .is_some_and(|family| family.common_zones.contains(&sound.zone.as_str()));
    Some((
        LoadedClipKey {
            family: sound.game(),
            content: sound.encoded_content_id(),
            conversion_revision: PCM_CONVERSION_REVISION,
            format: sound.format(),
            rate: sound.rate,
            bits: sound.bits(),
            channels: sound.channels(),
            samples: sound.samples,
            block_size: sound.block_size,
            seek_table: sound.seek_table.clone(),
        },
        common,
    ))
}

struct PreparedClip {
    pcm: PcmBuffer,
    common: bool,
}

#[derive(Default)]
struct PreparedClipCacheInner {
    profile_id: u64,
    bank: std::sync::Weak<SoundCatalog>,
    prepared: HashMap<LoadedClipKey, PreparedClip>,
}

#[derive(Clone, Default)]
struct PreparedClipCache(Arc<Mutex<PreparedClipCacheInner>>);

#[derive(Resource, Clone, Default)]
pub(crate) struct ResidentClipCache {
    prepared: PreparedClipCache,
}

impl ResidentClipCache {
    fn use_profile(&self, profile_id: u64) {
        self.prepared.use_profile(profile_id);
    }
    fn use_bank(&self, bank: &Arc<SoundCatalog>) {
        self.prepared.use_bank(bank);
    }
}

impl PreparedClipCache {
    fn use_profile(&self, profile_id: u64) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if inner.profile_id != profile_id {
            inner.prepared.clear();
            inner.profile_id = profile_id;
        }
    }

    fn use_bank(&self, bank: &Arc<SoundCatalog>) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if std::sync::Weak::ptr_eq(&inner.bank, &Arc::downgrade(bank)) {
            return;
        }
        let before_bytes = inner
            .prepared
            .values()
            .map(|entry| entry.pcm.resident_bytes())
            .sum::<usize>();
        let before_count = inner.prepared.len();
        let current: HashSet<_> = (0..bank.loaded_sounds().len())
            .filter_map(|index| {
                resident_clip_key(
                    bank,
                    &ClipKey {
                        revision: bank.revision(),
                        locator: ClipLocator::Loaded(index),
                    },
                )
                .map(|(key, _)| key)
            })
            .collect();
        inner
            .prepared
            .retain(|key, entry| entry.common && current.contains(key));
        inner.bank = Arc::downgrade(bank);
        if crate::diagnostics::enabled() {
            crate::diagnostics::emit(format!(
                "audio cache: bank={} before={before_count}/{before_bytes}B after={}/{}B idle={}B",
                bank.revision(),
                inner.prepared.len(),
                inner
                    .prepared
                    .values()
                    .map(|entry| entry.pcm.resident_bytes())
                    .sum::<usize>(),
                inner
                    .prepared
                    .values()
                    .filter(|entry| !entry.pcm.shared())
                    .map(|entry| entry.pcm.resident_bytes())
                    .sum::<usize>()
            ));
        }
    }

    fn evict_idle(&self) -> usize {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut freed = 0;
        inner.prepared.retain(|_, entry| {
            if entry.pcm.shared() {
                return true;
            }
            freed += entry.pcm.resident_bytes();
            false
        });
        EVICTED_BYTES.fetch_add(freed as u64, Ordering::Relaxed);
        if crate::diagnostics::enabled() {
            crate::diagnostics::emit(format!(
                "audio cache pressure: freed={freed}B remaining={}/{}B",
                inner.prepared.len(),
                inner
                    .prepared
                    .values()
                    .map(|entry| entry.pcm.resident_bytes())
                    .sum::<usize>()
            ));
        }
        freed
    }

    fn ready(&self, profile_id: u64, key: &LoadedClipKey, common: bool) -> Option<PcmBuffer> {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if profile_id == 0 || inner.profile_id != profile_id {
            return None;
        }
        let entry = inner.prepared.get_mut(key)?;
        entry.common |= common;
        Some(entry.pcm.clone())
    }

    fn remember(&self, profile_id: u64, key: LoadedClipKey, common: bool, pcm: PcmBuffer) {
        let mut inner = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if profile_id != 0 && inner.profile_id == profile_id {
            inner
                .prepared
                .entry(key)
                .and_modify(|entry| entry.common |= common)
                .or_insert(PreparedClip { pcm, common });
        }
    }
}

pub(crate) const MEDIA_REQUEST_LIMIT: usize = 4096;

struct ClipWorkers {
    handles: Vec<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    queue: Arc<MediaJobQueue>,
}

impl Drop for ClipWorkers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.queue.close();
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
    service: MediaService,
}

#[derive(Clone)]
pub(crate) struct MediaService(Arc<MediaServiceInner>);

struct MediaServiceInner {
    bank: Arc<SoundCatalog>,
    queue: Arc<MediaJobQueue>,
    requests: Arc<Mutex<MediaRequests>>,
    outcomes: Arc<Mutex<Outcomes>>,
    workers: Option<ClipWorkers>,
    retirement: Sender<ClipWorkers>,
    clip_cache: Option<PreparedClipCache>,
    common_profile_id: u64,
}

impl Drop for MediaServiceInner {
    fn drop(&mut self) {
        if let Some(workers) = self.workers.take() {
            workers.stop.store(true, Ordering::Release);
            self.queue.close();
            if let Err(error) = self.retirement.send(workers) {
                let mut workers = error.0;
                // Joining here would stall control; the closed queue wakes stopped workers.
                workers.handles.clear();
            }
        }
    }
}

type Outcomes = HashMap<ClipKey, (Result<PcmBuffer, ClipError>, u64)>;

static USE_TICK: AtomicU64 = AtomicU64::new(0);

fn evict_idle_streamed(requests: &Mutex<MediaRequests>, outcomes: &Mutex<Outcomes>) -> usize {
    let mut requests = requests.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut outcomes = outcomes.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut idle: Vec<(u64, ClipKey, usize)> = outcomes
        .iter()
        .filter_map(|(key, (result, used))| match (&key.locator, result) {
            (ClipLocator::Streamed { .. }, Ok(pcm)) if !pcm.shared() => {
                Some((*used, key.clone(), pcm.resident_bytes()))
            }
            _ => None,
        })
        .collect();
    idle.sort_unstable_by_key(|(used, ..)| *used);
    let goal = crate::pcm_memory().limit_bytes / 4;
    let mut freed = 0;
    for (_, key, bytes) in idle {
        if freed >= goal {
            break;
        }
        outcomes.remove(&key);
        requests.queued.remove(&key);
        freed += bytes;
    }
    EVICTED_BYTES.fetch_add(freed as u64, Ordering::Relaxed);
    freed
}

#[derive(Default)]
struct MediaRequests {
    queued: HashSet<ClipKey>,
    reused_clips: usize,
    reused_bytes: u64,
    match_live: bool,
    late_prepares: u32,
}

impl ClipStore {
    pub fn start(bank: Arc<SoundCatalog>, iwd: Option<Arc<NamespaceSoundIwd>>) -> Self {
        Self {
            service: MediaService::start_with_common(bank, iwd, 0, None),
        }
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
        let prepared = clip_cache.as_ref().map(|cache| cache.prepared.clone());
        Self {
            service: MediaService::start_with_common(bank, iwd, common_profile_id, prepared),
        }
    }

    pub(crate) fn service(&self) -> MediaService {
        self.service.clone()
    }
    pub(crate) fn release_idle_streamed(&self) -> usize {
        evict_idle_streamed(&self.service.0.requests, &self.service.0.outcomes)
    }
    pub fn workers(&self) -> usize {
        self.service.workers()
    }
    pub fn reused_resident(&self) -> (usize, u64) {
        self.service.reused_resident()
    }
    pub fn arm_match_live(&mut self) {
        self.service.arm_match_live();
    }
    pub fn late_prepares(&self) -> u32 {
        self.service.late_prepares()
    }
    pub(crate) fn prefetch(&mut self, key: ClipKey) -> MediaRequest {
        self.service.submit_request(key, MediaPriority::Prewarm)
    }
    pub(crate) fn ready(&self, key: &ClipKey) -> Option<Result<PcmBuffer, ClipError>> {
        self.service.ready(key)
    }
}

impl MediaService {
    fn start_with_common(
        bank: Arc<SoundCatalog>,
        iwd: Option<Arc<NamespaceSoundIwd>>,
        common_profile_id: u64,
        clip_cache: Option<PreparedClipCache>,
    ) -> Self {
        if let Some(cache) = &clip_cache {
            cache.use_profile(common_profile_id);
            cache.use_bank(&bank);
        }
        let (retirement, retired_workers) = channel::<ClipWorkers>();
        if let Err(error) = std::thread::Builder::new()
            .name("audio-media-retire".into())
            .spawn(move || {
                if let Ok(workers) = retired_workers.recv() {
                    drop(workers);
                }
            })
        {
            diag::warn!(
                Audio,
                "audio: media retirement thread not started ({error})"
            );
        }
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(2, 4))
            .unwrap_or(2);
        let queue = Arc::new(MediaJobQueue::new());
        let outcomes = Arc::new(Mutex::new(Outcomes::new()));
        let requests = Arc::new(Mutex::new(MediaRequests::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::with_capacity(workers);
        let mut urgent_started = false;
        let mut general_started = false;
        for slot in 0..workers {
            let queue_worker = Arc::clone(&queue);
            let bank = Arc::clone(&bank);
            let iwd = iwd.clone();
            let outcomes = Arc::clone(&outcomes);
            let requests = Arc::clone(&requests);
            let clip_cache = clip_cache.clone();
            let stop_worker = Arc::clone(&stop);

            let spawned = std::thread::Builder::new()
                .name(format!("clip-prep-{slot}"))
                .spawn(move || {
                    assets::session_load::use_process_cpus();
                    crate::diagnostics::thread(&format!("clip-prep-{slot}"));
                    loop {
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        let Some(job) = queue_worker.pop(slot == 0) else {
                            return;
                        };
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        QUEUE_WAIT_NS.fetch_add(job.queued_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        let prepare_at = Instant::now();
                        let (path, mut result) = prepare_clip_now(&bank, iwd.as_deref(), &job.key);
                        note_prepared(path, prepare_at, result.as_ref());
                        if matches!(result, Err(ClipError::InvalidPcm(crate::media::PcmError::MemoryLimit)))
                            && !stop_worker.load(Ordering::Acquire)
                        {
                            let cached = clip_cache.as_ref().map_or(0, PreparedClipCache::evict_idle);
                            let streamed = evict_idle_streamed(&requests, &outcomes);
                            if cached != 0 || streamed != 0 {
                                let retry_at = Instant::now();
                                let (path, retry) = prepare_clip_now(&bank, iwd.as_deref(), &job.key);
                                note_prepared(path, retry_at, retry.as_ref());
                                result = retry;
                            }
                        }
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        if let Some(cache) = &clip_cache
                            && let Ok(pcm) = &result
                            && let Some((key, common)) = resident_clip_key(&bank, &job.key)
                        {
                            cache.remember(common_profile_id, key, common, pcm.clone());
                        }
                        if crate::diagnostics::enabled() {
                            let result_detail = match &result {
                                Ok(pcm) => format!("ready frames={} rate={}", pcm.len() / usize::from(pcm.channels()), pcm.rate()),
                                Err(error) => format!("failed {error:?}"),
                            };
                            crate::diagnostics::emit(format!("audio diag: media key={:?} queued_to_publish_ms={:.3} prepare_ms={:.3} {result_detail}", job.key, job.queued_at.elapsed().as_secs_f64()*1000.0, prepare_at.elapsed().as_secs_f64()*1000.0));
                        }
                        outcomes.lock().unwrap_or_else(|poison| poison.into_inner())
                            .insert(job.key, (result, USE_TICK.fetch_add(1, Ordering::Relaxed)));
                    }
                });
            match spawned {
                Ok(handle) => {
                    urgent_started |= slot == 0;
                    general_started |= slot != 0;
                    handles.push(handle);
                }
                Err(e) => diag::warn!(Audio, "audio: clip prep worker {slot} not started ({e})"),
            }
        }
        if !urgent_started || !general_started {
            stop.store(true, Ordering::Release);
            queue.close();
        }
        WORKERS.fetch_add(handles.len() as u64, Ordering::Relaxed);
        Self(Arc::new(MediaServiceInner {
            bank,
            queue: Arc::clone(&queue),
            requests,
            outcomes,
            workers: Some(ClipWorkers {
                handles,
                stop,
                queue,
            }),
            retirement,
            clip_cache,
            common_profile_id,
        }))
    }

    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn bank_revision(&self) -> u64 {
        self.0.bank.revision()
    }

    fn workers(&self) -> usize {
        self.0
            .workers
            .as_ref()
            .map_or(0, |workers| workers.handles.len())
    }

    fn reused_resident(&self) -> (usize, u64) {
        let requests = self
            .0
            .requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (requests.reused_clips, requests.reused_bytes)
    }

    fn arm_match_live(&self) {
        self.0
            .requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .match_live = true;
    }

    fn late_prepares(&self) -> u32 {
        self.0
            .requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .late_prepares
    }

    pub(crate) fn request(&self, key: ClipKey) -> bool {
        matches!(
            self.submit_request(key, MediaPriority::Urgent),
            MediaRequest::Submitted
        )
    }

    fn submit_request(&self, key: ClipKey, priority: MediaPriority) -> MediaRequest {
        if key.revision() != self.bank_revision() {
            return MediaRequest::Refused;
        }
        REQUESTS.fetch_add(1, Ordering::Relaxed);
        let lock_at = Instant::now();
        let mut requests = self
            .0
            .requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _ = crate::diagnostics::slow_stage("media_request_lock", lock_at);
        if requests.queued.contains(&key) {
            if priority == MediaPriority::Urgent {
                self.0.queue.promote(&key);
            }
            return MediaRequest::Existing;
        }
        if requests.queued.len() == MEDIA_REQUEST_LIMIT {
            REQUEST_LIMIT.fetch_add(1, Ordering::Relaxed);
            return MediaRequest::Refused;
        }
        requests.queued.insert(key.clone());
        if let Some(pcm) = self.0.clip_cache.as_ref().and_then(|cache| {
            resident_clip_key(&self.0.bank, &key)
                .and_then(|(source, common)| cache.ready(self.0.common_profile_id, &source, common))
        }) {
            requests.reused_clips += 1;
            requests.reused_bytes += pcm.resident_bytes() as u64;
            self.0
                .outcomes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(key, (Ok(pcm), USE_TICK.fetch_add(1, Ordering::Relaxed)));
            return MediaRequest::Resident;
        }
        if requests.match_live && key.is_loaded() {
            requests.late_prepares = requests.late_prepares.saturating_add(1);
            LATE.fetch_add(1, Ordering::Relaxed);
        }
        let job = ClipJob {
            key: key.clone(),
            queued_at: Instant::now(),
        };
        match self.0.queue.push(job, priority) {
            Ok(()) => {
                QUEUED.fetch_add(1, Ordering::Relaxed);
                MediaRequest::Submitted
            }
            Err(error) => {
                let reason = match error {
                    QueueRefusal::Full => {
                        requests.queued.remove(&key);
                        QUEUE_DEFERRED.fetch_add(1, Ordering::Relaxed);
                        return MediaRequest::Deferred;
                    }
                    QueueRefusal::Closed => ClipError::QueueClosed,
                };
                self.0
                    .outcomes
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .insert(key, (Err(reason), 0));
                MediaRequest::Refused
            }
        }
    }

    pub(crate) fn ready(&self, key: &ClipKey) -> Option<Result<PcmBuffer, ClipError>> {
        if key.revision() != self.bank_revision() {
            return Some(Err(ClipError::ForeignOwner));
        }
        let lock_at = Instant::now();
        let at_limit = {
            let requests = self
                .0
                .requests
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            !requests.queued.contains(key) && requests.queued.len() == MEDIA_REQUEST_LIMIT
        };
        let lock_at = crate::diagnostics::slow_stage("media_ready_requests_lock", lock_at);
        if at_limit {
            return Some(Err(ClipError::RequestLimit));
        }
        let mut outcomes = self
            .0
            .outcomes
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _ = crate::diagnostics::slow_stage("media_ready_outcomes_lock", lock_at);
        let (result, used) = outcomes.get_mut(key)?;
        *used = USE_TICK.fetch_add(1, Ordering::Relaxed);
        Some(result.clone())
    }
}

pub(crate) struct CueFeedbackEntry {
    pub handle: crate::cue::CueHandle,
}

#[derive(Resource, Default)]
pub(crate) struct CueFeedback {
    pub cues: Vec<CueFeedbackEntry>,
    pub dropped: u64,
}

impl CueFeedback {
    pub(crate) fn push(&mut self, handle: crate::cue::CueHandle) {
        if self.cues.len() == crate::runtime::LOGICAL_INSTANCES {
            self.cues.remove(0);
            self.dropped = self.dropped.saturating_add(1);
        }
        self.cues.push(CueFeedbackEntry { handle });
    }
}

pub(crate) fn clip_keys_for_alias(bank: &SoundCatalog, ns: FamilyId, alias: &str) -> Vec<ClipKey> {
    let mut seen_alias = HashSet::new();
    let mut seen_key = HashSet::new();
    let mut out = Vec::new();
    collect_clip_keys(bank, ns, alias, 0, &mut seen_alias, &mut seen_key, &mut out);
    out
}

fn collect_clip_keys(
    bank: &SoundCatalog,
    ns: FamilyId,
    alias: &str,
    depth: u8,
    seen_alias: &mut HashSet<String>,
    seen_key: &mut HashSet<ClipKey>,
    out: &mut Vec<ClipKey>,
) {
    if depth > asset_audio::MAX_SECONDARY_DEPTH || !seen_alias.insert(alias.to_owned()) {
        return;
    }
    let Some(index) = bank.index_in(ns, alias) else {
        return;
    };
    let Some(sound) = bank.sound_at(index) else {
        return;
    };
    for vi in 0..sound.aliases.len() {
        let Ok(bound) = bank.bind_published_alias(index, vi) else {
            continue;
        };
        if let Ok(Some(key)) = clip_key_for_sound(bound) {
            if seen_key.insert(key.clone()) {
                out.push(key);
            }
        }
        if let Some(secondary) = bank
            .playback_policy(index, vi)
            .and_then(|policy| policy.composition.secondary.as_ref())
        {
            collect_clip_keys(
                bank,
                ns,
                &secondary.alias,
                depth + 1,
                seen_alias,
                seen_key,
                out,
            );
        }
    }
}

pub(crate) fn clip_key_for_sound(
    bound: asset_audio::BoundSound<'_>,
) -> Result<Option<ClipKey>, &'static str> {
    let revision = bound.revision();
    Ok(bound.media()?.map(|media| ClipKey {
        revision,
        locator: match media {
            asset_audio::BoundSoundMedia::Loaded { index, .. } => ClipLocator::Loaded(index),
            asset_audio::BoundSoundMedia::Streamed {
                namespace,
                dir,
                name,
            } => ClipLocator::Streamed {
                ns: namespace,
                dir,
                name,
            },
        },
    }))
}

static WORKERS: AtomicU64 = AtomicU64::new(0);
static REQUESTS: AtomicU64 = AtomicU64::new(0);
static QUEUE_DEFERRED: AtomicU64 = AtomicU64::new(0);
static REQUEST_LIMIT: AtomicU64 = AtomicU64::new(0);
static QUEUED: AtomicU64 = AtomicU64::new(0);
static QUEUE_WAIT_NS: AtomicU64 = AtomicU64::new(0);
static LATE: AtomicU64 = AtomicU64::new(0);
static EVICTED_BYTES: AtomicU64 = AtomicU64::new(0);
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
    pub queue_deferred: u64,
    pub request_limit: u64,
    pub queue_wait_ms: f64,
    pub late: u64,
    pub evicted_bytes: u64,
    pub paths: Vec<(ClipPath, ClipPathCost)>,
}

pub fn clip_prep_cost() -> ClipPrepCost {
    ClipPrepCost {
        workers: WORKERS.load(Ordering::Relaxed),
        requests: REQUESTS.load(Ordering::Relaxed),
        queued: QUEUED.load(Ordering::Relaxed),
        queue_deferred: QUEUE_DEFERRED.load(Ordering::Relaxed),
        request_limit: REQUEST_LIMIT.load(Ordering::Relaxed),
        queue_wait_ms: QUEUE_WAIT_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        late: LATE.load(Ordering::Relaxed),
        evicted_bytes: EVICTED_BYTES.load(Ordering::Relaxed),
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

fn note_prepared(path: ClipPath, prepare_at: Instant, result: Result<&PcmBuffer, &ClipError>) {
    note_wall(path, prepare_at.elapsed());
    note_outcome(path, result);
}

fn note_wall(path: ClipPath, wall: std::time::Duration) {
    WALL_NS[path as usize].fetch_add(wall.as_nanos() as u64, Ordering::Relaxed);
}

fn note_outcome(path: ClipPath, result: Result<&PcmBuffer, &ClipError>) {
    let slot = path as usize;
    match result {
        Ok(prepared) => {
            PREPARED[slot].fetch_add(1, Ordering::Relaxed);
            SAMPLE_BYTES[slot].fetch_add(prepared.resident_bytes() as u64, Ordering::Relaxed);
        }
        Err(error) => {
            if let ClipError::Native(error) = error {
                diag::warn!(Audio, "audio: {error}");
            }
            FAILED[slot].fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn prepare_clip_now(
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    key: &ClipKey,
) -> (ClipPath, Result<PcmBuffer, ClipError>) {
    if key.revision() != bank.revision() {
        return (ClipPath::Unresolved, Err(ClipError::ForeignOwner));
    }
    match &key.locator {
        ClipLocator::Loaded(index) => {
            let Some(sound) = bank.pcm_at(*index) else {
                return (ClipPath::Unresolved, Err(ClipError::Decode));
            };
            let Some(path) = loaded_path(sound) else {
                return (ClipPath::Unresolved, Err(ClipError::Decode));
            };
            (path, prepare_loaded(sound))
        }
        ClipLocator::Streamed { ns, dir, name, .. } => {
            (ClipPath::Streamed, prepare_streamed(iwd, *ns, dir, name))
        }
    }
}

/// Which decoder `prepare_loaded` will reach for, decided the same way it
/// decides — the two read the same fields in the same order, so a clip cannot
/// be counted under one path and decoded by another.
fn loaded_path(sound: &asset_audio::LoadedSoundPcm) -> Option<ClipPath> {
    if sound.external_media.is_some() {
        return Some(ClipPath::External);
    }
    Some(
        match asset_audio::audio_family(sound.game())?
            .decoder
            .encoding(sound)?
        {
            asset_audio::LoadedAudioEncoding::Pcm => ClipPath::Pcm,
            asset_audio::LoadedAudioEncoding::Container => ClipPath::Container,
            asset_audio::LoadedAudioEncoding::Native => ClipPath::Native,
        },
    )
}
fn prepare_loaded(sound: &asset_audio::LoadedSoundPcm) -> Result<PcmBuffer, ClipError> {
    crate::pcm::native_decode::prepare_loaded(sound).map_err(ClipError::from)
}

fn prepare_streamed(
    iwd: Option<&NamespaceSoundIwd>,
    ns: FamilyId,
    dir: &str,
    name: &str,
) -> Result<PcmBuffer, ClipError> {
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
    crate::pcm::native_decode::prepare_streamed(ns, &bytes).map_err(ClipError::from)
}
