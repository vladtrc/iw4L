use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize};

use crate::render_core::AudioScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceKey {
    pub scope: AudioScope,
    pub object: u64,
    pub slot: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRenderGroup {
    MapEmitter,
}

pub(crate) struct SourceCueRequest {
    pub bank: Arc<asset_audio::SoundCatalog>,
    pub namespace: asset_core::FamilyId,
    pub alias: String,
    pub emitter: Option<u32>,
    pub scope: AudioScope,
    pub group: Option<SourceRenderGroup>,
}

pub(crate) struct SourceCue {
    pub bank: Arc<asset_audio::SoundCatalog>,
    pub namespace: asset_core::FamilyId,
    pub alias: String,
    pub emitter: Option<u32>,
    pub media: crate::clip_store::MediaService,
    pub mix: Option<crate::cue_execution::CueMix>,
    pub lease: crate::cue_execution::CueLease,
    pub group: Option<SourceRenderGroup>,
}

#[derive(Clone)]
pub(crate) struct DesiredSource {
    pub key: SourceKey,
    pub version: u64,
    pub cue: Arc<SourceCue>,
    pub origin_inches: Option<[f32; 3]>,
    pub start_frame: u64,
    pub gain: f32,
    pub rate: f32,
    pub audible: bool,
}

pub(crate) struct SourceScene {
    pub revision: u64,
    pub sources: Vec<DesiredSource>,
    pub asserted: Vec<DesiredSource>,
}

pub(crate) const SOURCE_HISTORY: usize = 4096;

struct SourceVersion {
    version: u64,
    active: Option<DesiredSource>,
}

pub(crate) struct SourcePublisher {
    revision: u64,
    versions: HashMap<SourceKey, SourceVersion>,
}

impl SourcePublisher {
    pub fn new() -> Self {
        Self {
            revision: 0,
            versions: HashMap::with_capacity(SOURCE_HISTORY),
        }
    }

    pub fn reconcile(
        &mut self,
        mut sources: Vec<DesiredSource>,
        cancelled: bool,
    ) -> (SourceScene, u64) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("source revision exhausted");
        sources.sort_unstable_by_key(|source| (source.key, std::cmp::Reverse(source.version)));
        sources.dedup_by_key(|source| source.key);
        let mut desired = Vec::with_capacity(sources.len().min(crate::runtime::LOGICAL_INSTANCES));
        let mut present = HashSet::with_capacity(SOURCE_HISTORY);
        if !cancelled {
            present.extend(self.versions.keys().copied().filter(|key| {
                sources
                    .binary_search_by_key(key, |source| source.key)
                    .is_ok()
            }));
        }
        let mut rejected = 0u64;
        for mut source in sources {
            if cancelled {
                rejected += 1;
                continue;
            }
            let executable = desired.len() < crate::runtime::LOGICAL_INSTANCES;
            if !executable && !self.versions.contains_key(&source.key) {
                rejected += 1;
                continue;
            }
            if let Some(current) = self.versions.get_mut(&source.key) {
                if source.version <= current.version {
                    let Some(active) = &current.active else {
                        rejected += 1;
                        continue;
                    };
                    if source.version < current.version {
                        source = active.clone();
                        rejected += 1;
                    } else {
                        source.cue = active.cue.clone();
                        source.start_frame = active.start_frame;
                    }
                }
                current.version = source.version;
                current.active = Some(source.clone());
            } else {
                if self.versions.len() == SOURCE_HISTORY {
                    rejected += 1;
                    continue;
                }
                self.versions.insert(
                    source.key,
                    SourceVersion {
                        version: source.version,
                        active: Some(source.clone()),
                    },
                );
            }
            present.insert(source.key);
            if executable {
                desired.push(source);
            } else {
                rejected += 1;
            }
        }
        for (key, current) in &mut self.versions {
            if !present.contains(key) {
                current.active = None;
            }
        }
        let mut asserted: Vec<_> = self
            .versions
            .values()
            .filter_map(|current| current.active.clone())
            .collect();
        asserted.sort_unstable_by_key(|source| source.key);
        (
            SourceScene {
                revision: self.revision,
                sources: desired,
                asserted,
            },
            rejected,
        )
    }

    pub fn stop_source(&mut self, key: SourceKey) {
        self.versions.remove(&key);
    }

    pub fn stop_scope(&mut self, scope: AudioScope) {
        self.versions.retain(|key, _| key.scope != scope);
    }

    pub fn cancel(&mut self) {
        for current in self.versions.values_mut() {
            current.active = None;
        }
    }
}

pub(crate) struct SourceActivity {
    pub revision: AtomicU64,
    pub pending_layers: AtomicUsize,
    pub active: AtomicUsize,
    pub rendered: AtomicUsize,
    pub virtualized: AtomicUsize,
    pub dropped: AtomicU64,
}

impl SourceActivity {
    pub fn new() -> Self {
        Self {
            revision: AtomicU64::new(0),
            pending_layers: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            rendered: AtomicUsize::new(0),
            virtualized: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
        }
    }
}
