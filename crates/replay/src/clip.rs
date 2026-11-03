use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use net::AUTHORITY_MS;
use sim::Snapshot;

use crate::file::{demo_stem, sanitize_demo_name};
use crate::{ReplayError, demo_path};

pub const CLIP_MS: u32 = 45_000;

pub const CLIP_TICK_MS: u32 = 50;

pub const CLIP_TICKS: usize = (CLIP_MS / CLIP_TICK_MS) as usize;

pub const CLIP_LATEST: &str = "LATEST";

pub const CLIP_DEMO_FILE: &str = "clip.iw4ldemo";
pub const CLIP_DUMP_FILE: &str = "dump.txt";
pub const CLIP_MANIFEST_FILE: &str = "manifest.toml";

const _: () = assert!(CLIP_TICKS == 900);
const _: () = assert!(CLIP_TICK_MS as i32 == AUTHORITY_MS);

#[derive(Clone, Debug)]
struct ClipTick {
    tick: sim::Tick,
    raw_len: usize,
    compressed: Box<[u8]>,
}

#[derive(Resource, Debug)]
pub struct ClipRing {
    ticks: VecDeque<ClipTick>,
    bytes: usize,
    max_ticks: usize,
}

impl Default for ClipRing {
    fn default() -> Self {
        Self::with_capacity(CLIP_TICKS)
    }
}

impl ClipRing {
    pub fn with_capacity(max_ticks: usize) -> Self {
        Self {
            ticks: VecDeque::new(),
            bytes: 0,
            max_ticks: max_ticks.max(1),
        }
    }

    pub fn push(&mut self, snapshot: &Snapshot) -> Result<(), ReplayError> {
        self.push_received(snapshot, None)
    }

    pub fn push_frame(
        &mut self,
        snapshot: &Snapshot,
        frame: &net::Frame,
    ) -> Result<(), ReplayError> {
        self.push_received(snapshot, Some(frame))
    }

    fn push_received(
        &mut self,
        snapshot: &Snapshot,
        received: Option<&net::Frame>,
    ) -> Result<(), ReplayError> {
        if self
            .ticks
            .back()
            .is_some_and(|last| snapshot.tick <= last.tick)
        {
            return Ok(());
        }
        // Each tick has its own baseline, so eviction never breaks later ticks.
        // Retain the promised time window; entity count must not shorten it.
        let frame =
            crate::session::recording_frame(&mut net::SnapshotEncoder::new(), snapshot, received);
        let raw = frame.to_bytes();
        let compressed = zstd::bulk::compress(&raw, 1)?.into_boxed_slice();
        self.bytes += compressed.len();
        self.ticks.push_back(ClipTick {
            tick: snapshot.tick,
            raw_len: raw.len(),
            compressed,
        });
        while self.ticks.len() > self.max_ticks || self.duration_ms() > CLIP_MS {
            let oldest = self.ticks.pop_front().expect("nonempty clip ring");
            self.bytes -= oldest.compressed.len();
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.ticks.clear();
        self.bytes = 0;
    }
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn duration_ms(&self) -> u32 {
        match (self.ticks.front(), self.ticks.back()) {
            (Some(first), Some(last)) => last
                .tick
                .0
                .saturating_sub(first.tick.0)
                .saturating_add(1)
                .saturating_mul(CLIP_TICK_MS),
            _ => 0,
        }
    }

    /// Bytes of compressed tick payloads currently retained.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn frames(&self) -> impl Iterator<Item = Result<net::Frame, ReplayError>> {
        self.ticks.iter().map(|tick| {
            let raw = zstd::bulk::decompress(&tick.compressed, tick.raw_len)?;
            Ok(net::Frame::decode(
                &mut net::WireReader::new(&raw),
                &mut net::WorldObjectSyncDecoder::default(),
            )?)
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<Snapshot, ReplayError>> {
        self.frames().map(|frame| {
            let frame = frame?;
            let mut snapshot = net::SnapshotDecoder::new().decode(&frame.snapshot_delta)?;
            snapshot.meta = frame.snapshot_meta;
            Ok(snapshot)
        })
    }
}

pub fn clips_dir(artifacts_root: &Path) -> PathBuf {
    artifacts_root.join("clips")
}

pub fn clip_dir(artifacts_root: &Path, id: &str) -> PathBuf {
    clips_dir(artifacts_root).join(id)
}

pub fn clip_demo_path(artifacts_root: &Path, id: &str) -> PathBuf {
    clip_dir(artifacts_root, id).join(CLIP_DEMO_FILE)
}

pub fn create_clip_dir(artifacts_root: &Path, id: &str) -> Result<PathBuf, ReplayError> {
    let dir = clip_dir(artifacts_root, id);
    std::fs::create_dir_all(clips_dir(artifacts_root))?;
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

pub fn rewrite_latest_symlink(artifacts_root: &Path, id: &str) -> Result<(), ReplayError> {
    let clips = clips_dir(artifacts_root);
    std::fs::create_dir_all(&clips)?;
    let latest = clips.join(CLIP_LATEST);
    remove_latest_pointer(&latest)?;
    create_latest_pointer(id, &latest)?;
    Ok(())
}

#[cfg(unix)]
fn remove_latest_pointer(latest: &Path) -> Result<(), std::io::Error> {
    match std::fs::remove_file(latest) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn remove_latest_pointer(latest: &Path) -> Result<(), std::io::Error> {
    match std::fs::remove_dir(latest) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_latest_pointer(id: &str, latest: &Path) -> Result<(), std::io::Error> {
    std::os::unix::fs::symlink(id, latest)
}

#[cfg(windows)]
fn create_latest_pointer(id: &str, latest: &Path) -> Result<(), std::io::Error> {
    std::os::windows::fs::symlink_dir(id, latest)
}

pub fn clip_manifest(
    id: &str,
    ticks: u64,
    duration_ms: u32,
    zone: &str,
    captured_unix_ns: u128,
) -> String {
    format!(
        "id = \"{id}\"\n\
         ticks = {ticks}\n\
         duration_ms = {duration_ms}\n\
         zone = {zone:?}\n\
         captured_unix_ns = {captured_unix_ns}\n\
         demo = \"{CLIP_DEMO_FILE}\"\n\
         dump = \"{CLIP_DUMP_FILE}\"\n"
    )
}

pub fn existing_clip_demo(artifacts_root: &Path, requested: &str) -> Option<PathBuf> {
    let name = sanitize_demo_name(demo_stem(requested)).unwrap_or_else(|| requested.to_owned());
    let id = name.to_ascii_uppercase();
    let path = clip_demo_path(artifacts_root, &id);
    path.exists().then_some(path)
}

pub fn resolve_playback_path(artifacts_root: &Path, requested: &str) -> PathBuf {
    let name = sanitize_demo_name(demo_stem(requested)).unwrap_or_else(|| requested.to_owned());
    let demo = demo_path(artifacts_root, &name);
    if demo.exists() {
        return demo;
    }
    existing_clip_demo(artifacts_root, &name).unwrap_or(demo)
}
