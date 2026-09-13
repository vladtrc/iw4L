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

#[derive(Resource)]
pub struct ClipStore {
    bank: Arc<SoundCatalog>,
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
            match spawned {
                Ok(_) => started_workers += 1,
                Err(e) => diag::warn!(Audio, "audio: clip prep worker {slot} not started ({e})"),
            }
        }
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
        if !self.queued.insert(key.clone()) {
            return false;
        }
        self.note_late(&key);
        if self.tx.send(key.clone()).is_err() {
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

fn prepare_clip_now(
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
    let mut samples: Vec<f32> = match bits {
        8 => bytes.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        16 => bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect(),
        _ => return Err(ClipError::Decode),
    };
    let channels = sound.channels().max(1) as usize;
    samples.truncate(samples.len() / channels * channels);
    if samples.is_empty() {
        return Err(ClipError::Decode);
    }
    let samples = samples.into();
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
