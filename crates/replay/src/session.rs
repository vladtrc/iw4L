use std::path::{Path, PathBuf};

use net::{Frame, SnapshotDecoder, SnapshotEncoder, frame_from_tick};
use sim::Snapshot;

use crate::file::{
    MatchRecordIdentity, ReaderCheckpoint, RecordReader, RecordWriter, ReplayError, demo_path,
    sanitize_demo_name,
};

pub(crate) fn recording_frame(
    encoder: &mut SnapshotEncoder,
    snapshot: &Snapshot,
    received: Option<&Frame>,
) -> Frame {
    let packed = frame_from_tick(encoder, snapshot);
    match received {
        Some(received) => Frame {
            acks: received.acks.clone(),
            reliable: received.reliable.clone(),
            svc_sounds: received.svc_sounds.clone(),
            svc_scores: received.svc_scores.clone(),
            svc_card_slots: received.svc_card_slots.clone(),
            svc_open_menus: received.svc_open_menus.clone(),
            svc_hud_splashes: received.svc_hud_splashes.clone(),
            svc_game_notifies: received.svc_game_notifies.clone(),
            ..packed
        },
        None => packed,
    }
}

pub fn auto_demo_name(index: u32) -> String {
    format!("demo{index:04}")
}

pub fn next_free_demo_name(artifacts_root: &Path) -> String {
    for index in 0..10_000u32 {
        let name = auto_demo_name(index);
        if !demo_path(artifacts_root, &name).exists() {
            return name;
        }
    }

    format!(
        "demo{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

#[derive(Debug)]
pub struct Recording {
    writer: RecordWriter,
    encoder: SnapshotEncoder,
    name: String,
}

impl Recording {
    pub fn start(artifacts_root: &Path, requested: &str) -> Result<Self, ReplayError> {
        Self::start_with_identity(artifacts_root, requested, MatchRecordIdentity::default())
    }

    pub fn start_with_identity(
        artifacts_root: &Path,
        requested: &str,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        let name =
            sanitize_demo_name(requested).unwrap_or_else(|| next_free_demo_name(artifacts_root));
        let path = demo_path(artifacts_root, &name);
        let writer = RecordWriter::create_with_identity(&path, identity)?;
        Ok(Self {
            writer,
            encoder: SnapshotEncoder::new(),
            name,
        })
    }

    pub fn start_at_path(
        path: PathBuf,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        let name = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("clip")
            .to_owned();
        let writer = RecordWriter::create_with_identity(&path, identity)?;
        Ok(Self {
            writer,
            encoder: SnapshotEncoder::new(),
            name,
        })
    }

    pub fn record(&mut self, snapshot: &Snapshot) -> Result<(), ReplayError> {
        self.record_received(snapshot, None)
    }

    pub fn record_frame(&mut self, snapshot: &Snapshot, frame: &Frame) -> Result<(), ReplayError> {
        self.record_received(snapshot, Some(frame))
    }

    fn record_received(
        &mut self,
        snapshot: &Snapshot,
        received: Option<&Frame>,
    ) -> Result<(), ReplayError> {
        let frame = recording_frame(&mut self.encoder, snapshot, received);
        self.writer.write_frame(&frame)?;
        Ok(())
    }

    pub fn record_clip_ring(&mut self, ring: &crate::clip::ClipRing) -> Result<u64, ReplayError> {
        for frame in ring.frames() {
            let frame = frame?;
            let mut snapshot = SnapshotDecoder::new().decode(&frame.snapshot_delta)?;
            snapshot.meta = frame.snapshot_meta.clone();
            self.record_frame(&snapshot, &frame)?;
        }
        Ok(ring.len() as u64)
    }

    pub fn ticks(&self) -> u64 {
        self.writer.frames()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        self.writer.path()
    }

    pub fn stop(self) -> Result<(u64, PathBuf), ReplayError> {
        self.writer.finish()
    }
}

#[derive(Debug)]
pub struct Playback {
    reader: RecordReader,
    decoder: SnapshotDecoder,
    ticks: u64,
}

impl Playback {
    pub fn open(artifacts_root: &Path, requested: &str) -> Result<Self, ReplayError> {
        Self::open_path(&crate::clip::resolve_playback_path(
            artifacts_root,
            requested,
        ))
    }

    pub fn open_path(path: &Path) -> Result<Self, ReplayError> {
        let reader = RecordReader::open(path)?;
        Ok(Self {
            reader,
            decoder: SnapshotDecoder::new(),
            ticks: 0,
        })
    }

    pub fn next_tick(&mut self) -> Result<Option<PlayedTick>, ReplayError> {
        let Some(frame) = self.reader.read_frame()? else {
            return Ok(None);
        };
        let mut snapshot = self.decoder.decode(&frame.snapshot_delta)?;
        snapshot.meta = frame.snapshot_meta.clone();
        self.ticks += 1;
        Ok(Some(PlayedTick { snapshot, frame }))
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn path(&self) -> &Path {
        self.reader.path()
    }

    pub fn position_before(&mut self, frame_index: u64) -> Result<(), ReplayError> {
        self.seek(frame_index, &SeekIndex::default())
    }

    pub fn keyframe(&self) -> Keyframe {
        Keyframe {
            ticks: self.ticks,
            reader: self.reader.checkpoint(),
            decoder: self.decoder.clone(),
        }
    }

    pub fn seek(&mut self, frame_index: u64, index: &SeekIndex) -> Result<(), ReplayError> {
        let keyframe = index.at_or_before(frame_index);
        let resume_here = self.ticks <= frame_index
            && keyframe.is_none_or(|keyframe| keyframe.ticks <= self.ticks);
        if !resume_here {
            match keyframe {
                Some(keyframe) => {
                    self.reader.restore(&keyframe.reader)?;
                    self.decoder = keyframe.decoder.clone();
                    self.ticks = keyframe.ticks;
                }
                None => *self = Self::open_path(self.reader.path())?,
            }
        }
        while self.ticks < frame_index {
            if self.next_tick()?.is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "seek is beyond the recording",
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn identity(&self) -> MatchRecordIdentity {
        self.reader.identity()
    }
}

#[derive(Clone, Debug)]
pub struct Keyframe {
    ticks: u64,
    reader: ReaderCheckpoint,
    decoder: SnapshotDecoder,
}

impl Keyframe {
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
}

#[derive(Clone, Debug, Default)]
pub struct SeekIndex {
    keyframes: Vec<Keyframe>,
}

pub const SEEK_KEYFRAME_TICKS: u64 = 100;

impl SeekIndex {
    pub fn note(&mut self, playback: &Playback) {
        let ticks = playback.ticks();
        if ticks > 0
            && ticks.is_multiple_of(SEEK_KEYFRAME_TICKS)
            && self.keyframes.last().is_none_or(|last| last.ticks < ticks)
        {
            self.keyframes.push(playback.keyframe());
        }
    }

    pub fn at_or_before(&self, frame_index: u64) -> Option<&Keyframe> {
        let after = self
            .keyframes
            .partition_point(|keyframe| keyframe.ticks <= frame_index);
        after.checked_sub(1).map(|at| &self.keyframes[at])
    }

    pub fn len(&self) -> usize {
        self.keyframes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayedTick {
    pub snapshot: Snapshot,

    pub frame: Frame,
}
