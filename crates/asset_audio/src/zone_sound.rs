use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::{FamilyId, NativeSoundCapture, SoundCatalog};

pub struct ZoneSoundCapture {
    path: PathBuf,
    walk: &'static str,
    capture: Box<dyn NativeSoundCapture>,
    accept_partial: bool,
    stopped: Option<String>,
    claim: Option<Claim>,
}
impl ZoneSoundCapture {
    fn new(path: &Path, family: FamilyId, walk: &'static str, claim: Option<Claim>) -> Self {
        let policy = crate::audio_family(family).expect("registered sound capture family");
        Self {
            path: path.to_owned(),
            walk,
            capture: (policy.capture)(path, family),
            accept_partial: policy.accept_partial_walk,
            stopped: None,
            claim,
        }
    }
    pub fn for_map(path: &Path, family: FamilyId, walk: &'static str) -> Self {
        Self::new(path, family, walk, None)
    }
    pub fn claim_common(path: &Path, family: FamilyId, walk: &'static str) -> Option<Self> {
        let name = path.file_stem()?.to_str()?;
        let policy = crate::audio_family(family)?;
        if !policy
            .common_zones
            .iter()
            .any(|zone| name.eq_ignore_ascii_case(zone))
        {
            return None;
        }
        Some(Self::new(path, family, walk, Some(Claim::take(path)?)))
    }
    pub fn finish(self, walk: Result<(), String>) -> Result<SoundCatalog, String> {
        let reason = self.stopped.or_else(|| walk.err());
        if let Some(reason) = reason {
            if !self.accept_partial {
                return Err(format!("sound capture stopped: {reason}"));
            }
            diag::info!(
                Zone,
                "sound source: `{}` kept what the {} walk read before it stopped: {reason}",
                self.path.display(),
                self.walk
            );
        }
        self.capture.finish()
    }
    pub fn deposit(mut self, walk: Result<(), String>) {
        let claim = self.claim.take();
        let path = self.path.clone();
        let walk_name = self.walk;
        let result = self.finish(walk);
        match &result {
            Ok(catalog) => diag::info!(
                Zone,
                "sound source: `{}` captured in the {walk_name} walk — {} aliases {} loaded",
                path.display(),
                catalog.sounds().len(),
                catalog.loaded_sounds().len()
            ),
            Err(error) => diag::warn!(
                Zone,
                "sound source: `{}` failed in the {walk_name} walk: {error}",
                path.display()
            ),
        }
        if let Some(claim) = claim {
            claim.fill(result.map(Arc::new), walk_name);
        }
    }
    pub fn native_mut<T: NativeSoundCapture>(&mut self) -> Option<&mut T> {
        if self.stopped.is_some() {
            return None;
        }
        self.capture.as_any_mut().downcast_mut()
    }
    pub fn iw4(&mut self) -> Option<&mut SoundCatalog> {
        self.native_mut()
    }
    pub fn guard<E: std::fmt::Display>(&mut self, result: Result<(), E>) {
        if let Err(error) = result
            && self.stopped.is_none()
        {
            self.stopped = Some(error.to_string());
        }
    }
    pub fn raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        if self.stopped.is_none()
            && let Some(catalog) = self.capture.catalog_mut()
        {
            catalog.ingest_rawfile(name, data, zlib_compressed);
        }
    }
    pub fn iw4_loaded(
        &mut self,
        s: &fastfile_iw4::ZoneStream<'_>,
        ty: fastfile_iw4::AssetType,
        slot: fastfile_iw4::Ptr,
        insert_slot: Option<fastfile_iw4::Ptr>,
    ) {
        if let Some(catalog) = self.iw4() {
            let result = fastfile_iw4::AssetLinkSink::loaded(catalog, s, ty, slot, insert_slot);
            self.guard(result);
        }
    }
    pub fn iw4_alias(
        &mut self,
        ty: fastfile_iw4::AssetType,
        slot: fastfile_iw4::Ptr,
        target: fastfile_iw4::Ptr,
    ) {
        if let Some(catalog) = self.iw4() {
            let result = fastfile_iw4::AssetLinkSink::alias(catalog, ty, slot, target);
            self.guard(result);
        }
    }
}

type Stored = Result<Arc<SoundCatalog>, String>;

enum Slot {
    Capturing,
    Ready { catalog: Stored, walk: &'static str },
}

struct Store {
    slots: Mutex<HashMap<PathBuf, Slot>>,
    changed: Condvar,
}

fn store() -> &'static Store {
    static STORE: std::sync::OnceLock<Store> = std::sync::OnceLock::new();
    STORE.get_or_init(|| Store {
        slots: Mutex::new(HashMap::new()),
        changed: Condvar::new(),
    })
}

fn store_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

struct Claim {
    key: PathBuf,
    filled: bool,
}

impl Claim {
    fn take(path: &Path) -> Option<Self> {
        let key = store_key(path);
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        if slots.contains_key(&key) {
            return None;
        }
        slots.insert(key.clone(), Slot::Capturing);
        Some(Self { key, filled: false })
    }

    fn fill(mut self, catalog: Stored, walk: &'static str) {
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.insert(self.key.clone(), Slot::Ready { catalog, walk });
        self.filled = true;
        store.changed.notify_all();
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.filled {
            return;
        }
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(slots.get(&self.key), Some(Slot::Capturing)) {
            slots.remove(&self.key);
        }
        store.changed.notify_all();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneSoundOrigin {
    Shared(&'static str),
    AudioOnly,
}

pub fn ensure_zone_sound(path: &Path) -> (Stored, ZoneSoundOrigin) {
    let key = store_key(path);
    let store = store();
    let started = Instant::now();
    let mut reported = false;
    let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match slots.get(&key) {
            Some(Slot::Ready { catalog, walk }) => {
                return (catalog.clone(), ZoneSoundOrigin::Shared(walk));
            }
            Some(Slot::Capturing) => {
                if !reported && started.elapsed() >= Duration::from_secs(10) {
                    reported = true;
                    diag::warn!(
                        Zone,
                        "sound source: still waiting on the walk capturing `{}`",
                        path.display()
                    );
                }
                slots = store
                    .changed
                    .wait_timeout(slots, Duration::from_millis(250))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            None => break,
        }
    }
    slots.insert(key.clone(), Slot::Capturing);
    drop(slots);
    let claim = Claim { key, filled: false };
    let walked = crate::sound_load::walk_zone_sound(path).map(Arc::new);
    claim.fill(walked.clone(), "audio-only");
    (walked, ZoneSoundOrigin::AudioOnly)
}

#[macro_export]
macro_rules! forward_iw4_sound {
    () => {
        fn capture_loaded_sound(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            header: fastfile_iw4::Ptr,
            pcm: fastfile_iw4::Ptr,
            data_len: usize,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result = fastfile_iw4::AssetLinkSink::capture_loaded_sound(
                    catalog, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: fastfile_iw4::Ptr,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::bind_last_loaded_to_sound_file(catalog, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            list: fastfile_iw4::Ptr,
            count: usize,
            head: Option<fastfile_iw4::Ptr>,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::capture_sound(catalog, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_curve(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            header: fastfile_iw4::Ptr,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result = fastfile_iw4::AssetLinkSink::capture_snd_curve(catalog, s, header);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: fastfile_iw4::Ptr,
            dir: &str,
            name: &str,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::bind_streamed_sound_file(catalog, file, dir, name);
                sound.guard(result);
            }
            Ok(())
        }
    };
}
