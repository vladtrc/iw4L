use std::collections::{HashMap, HashSet};
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

#[derive(Clone, Debug)]
pub(crate) enum ClipError {
    Decode,
    Read,
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

#[derive(Resource)]
pub struct ClipStore {
    bank: Arc<SoundCatalog>,
    iwd: Option<Arc<NamespaceSoundIwd>>,
    tx: Sender<ClipKey>,
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
        let (tx, rx) = channel::<ClipKey>();
        let rx = Arc::new(Mutex::new(rx));
        let outcomes = Arc::new(Mutex::new(HashMap::new()));
        for slot in 0..workers {
            let rx = Arc::clone(&rx);
            let bank = Arc::clone(&bank);
            let iwd = iwd.clone();
            let outcomes = Arc::clone(&outcomes);

            let spawned = std::thread::Builder::new()
                .name(format!("clip-prep-{slot}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let guard = rx.lock().unwrap_or_else(|poison| poison.into_inner());
                            guard.recv()
                        };
                        let Ok(key) = job else {
                            return;
                        };
                        let result = prepare_clip_now(&bank, iwd.as_deref(), &key);
                        let mut guard =
                            outcomes.lock().unwrap_or_else(|poison| poison.into_inner());
                        guard.insert(key, result);
                    }
                });
            if let Err(e) = spawned {
                diag::warn!(Audio, "audio: clip prep worker {slot} not started ({e})");
            }
        }
        Self {
            bank,
            iwd,
            tx,
            queued: HashSet::new(),
            outcomes,
            workers,
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

    pub fn late_prepares(&self) -> u32 {
        self.late_prepares
    }

    fn note_late(&mut self, key: &ClipKey) {
        if !self.match_live {
            return;
        }
        self.late_prepares = self.late_prepares.saturating_add(1);
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
        if self.ready(&key).is_some() {
            return false;
        }
        if !needs_worker(&self.bank, &key) {
            self.note_late(&key);
            let result = prepare_clip_now(&self.bank, self.iwd.as_deref(), &key);
            let mut guard = self
                .outcomes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            guard.insert(key, result);
            return false;
        }
        if !self.queued.insert(key.clone()) {
            return false;
        }
        self.note_late(&key);
        if self.tx.send(key.clone()).is_err() {
            self.queued.remove(&key);
            return false;
        }
        true
    }

    pub(crate) fn request_off_frame(&mut self, key: ClipKey) -> bool {
        if self.ready(&key).is_some() {
            return false;
        }
        if !self.queued.insert(key.clone()) {
            return false;
        }
        self.note_late(&key);
        if self.tx.send(key.clone()).is_err() {
            self.queued.remove(&key);
            return false;
        }
        true
    }

    pub(crate) fn ready(&self, key: &ClipKey) -> Option<Result<PcmAudio, ClipError>> {
        if let ClipKey::Loaded(index) = key
            && let Some(sound) = self.bank.pcm_at(*index)
        {
            if let Some(pcm) = PcmAudio::from_loaded(sound) {
                return Some(Ok(pcm));
            }
            if sound.t5_xwma_error().is_some() {
                return Some(Err(ClipError::Decode));
            }
        }
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

    pub fn cancel_alias(&mut self, alias: &str) {
        self.entries.retain(|e| e.alias != alias);
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

fn needs_worker(bank: &SoundCatalog, key: &ClipKey) -> bool {
    match key {
        ClipKey::Streamed { .. } => true,
        ClipKey::Loaded(index) => bank
            .pcm_at(*index)
            .is_some_and(|sound| sound.is_t5_xwma() || sound.t5_adpcm_bytes().is_some()),
    }
}

pub(crate) fn prepare_clip_now(
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    key: &ClipKey,
) -> Result<PreparedPcm, ClipError> {
    match key {
        ClipKey::Loaded(index) => {
            let sound = bank.pcm_at(*index).ok_or(ClipError::Decode)?;
            prepare_loaded(sound)
        }
        ClipKey::Streamed { ns, dir, name } => {
            prepare_streamed(iwd, *ns, dir, name)?.ok_or(ClipError::Decode)
        }
    }
}

fn prepare_loaded(sound: &assets::LoadedSoundPcm) -> Result<PreparedPcm, ClipError> {
    if let Some(samples) = sound.prepared_samples() {
        let channels = u16::try_from(sound.channels().max(1)).map_err(|_| ClipError::Decode)?;
        return Ok(PreparedPcm {
            samples,
            channels,
            sample_rate: sound.rate.max(1),
        });
    }
    if let Some(bytes) = sound.t5_adpcm_bytes() {
        let channels = u16::try_from(sound.channels().max(1)).map_err(|_| ClipError::Decode)?;
        let pcm = crate::pcm::t5_stream::decode_adpcm(
            bytes,
            sound.samples,
            sound.rate,
            u32::from(channels),
        )
        .ok_or(ClipError::Decode)?;
        sound.set_playback_samples(Arc::clone(pcm.samples()));
        return Ok(PreparedPcm {
            samples: Arc::clone(pcm.samples()),
            channels: pcm.channel_count(),
            sample_rate: pcm.rate(),
        });
    }
    if sound.is_t5_xwma() {
        sound.prepare_t5_xwma().map_err(|_| ClipError::Decode)?;
    }
    let samples = sound.samples_f32().ok_or(ClipError::Decode)?;
    let channels = u16::try_from(sound.channels().max(1)).map_err(|_| ClipError::Decode)?;
    Ok(PreparedPcm {
        samples,
        channels,
        sample_rate: sound.rate.max(1),
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
