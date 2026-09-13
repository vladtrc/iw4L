use std::collections::VecDeque;

use bevy::prelude::Resource;
use playerstate_iw4::PlayerState;
use sim::{Snapshot, Tick};

use crate::authority::inbox::AUTHORITY_MS;

pub const ARCHIVE_TICK_MS: i32 = AUTHORITY_MS;

pub const ARCHIVE_CACHED_SNAPSHOT_CLIENTS: usize = 0x144;

pub const ARCHIVE_MAX_TICKS: usize = 0x4b0;

pub const ARCHIVE_BYTE_BUDGET: usize = 32 * 1024 * 1024;

pub const ARCHIVE_LOOKUP_WINDOW_TICKS: usize = ARCHIVE_MAX_TICKS;

pub const fn archive_attainable_ticks(_clients: usize) -> usize {
    ARCHIVE_MAX_TICKS
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedFrame {
    pub tick: Tick,
    pub snapshot: Snapshot,
}

impl ArchivedFrame {
    pub fn records(&self) -> usize {
        self.snapshot.players.len()
    }

    pub fn bytes(&self) -> usize {
        archived_snapshot_bytes(&self.snapshot)
    }
}

fn archived_snapshot_bytes(snapshot: &Snapshot) -> usize {
    const SNAPSHOT_BASE: usize = 64;
    let hud_elems: usize = snapshot
        .meta
        .clients
        .iter()
        .map(|(_, meta)| meta.hud_archival.len() + meta.hud_current.len())
        .sum();
    SNAPSHOT_BASE
        + snapshot.players.len() * core::mem::size_of::<PlayerState>()
        + snapshot.projectiles.len() * 64
        + snapshot.meta.entities.len() * core::mem::size_of::<entity_iw4::EntityState>()
        + hud_elems * core::mem::size_of::<hud_iw4::HudElem>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveLookup {
    pub attained_ms: i32,

    pub tick: Option<Tick>,
}

impl ArchiveLookup {
    pub fn nothing_to_show(&self) -> bool {
        self.tick.is_none()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveMetrics {
    pub archived: u64,

    pub evicted: u64,

    pub peak_ticks: usize,

    pub peak_records: usize,

    pub peak_bytes: usize,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FrameArchive {
    frames: VecDeque<ArchivedFrame>,
    records: usize,
    bytes: usize,
    metrics: ArchiveMetrics,
}

impl FrameArchive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, frame: ArchivedFrame) -> bool {
        if let Some(newest) = self.frames.back()
            && frame.tick.0 <= newest.tick.0
        {
            return false;
        }
        self.records += frame.records();
        self.bytes += frame.bytes();
        self.frames.push_back(frame);
        self.metrics.archived = self.metrics.archived.saturating_add(1);
        self.evict();
        self.metrics.peak_ticks = self.metrics.peak_ticks.max(self.frames.len());
        self.metrics.peak_records = self.metrics.peak_records.max(self.records);
        self.metrics.peak_bytes = self.metrics.peak_bytes.max(self.bytes);
        true
    }

    pub fn push_snapshot(&mut self, snapshot: &Snapshot) -> bool {
        self.push(ArchivedFrame {
            tick: snapshot.tick,
            snapshot: snapshot.clone(),
        })
    }

    fn evict(&mut self) {
        while self.frames.len() > 1
            && (self.frames.len() > ARCHIVE_MAX_TICKS || self.bytes > ARCHIVE_BYTE_BUDGET)
        {
            let Some(oldest) = self.frames.pop_front() else {
                break;
            };
            self.records -= oldest.records();
            self.bytes -= oldest.bytes();
            self.metrics.evicted = self.metrics.evicted.saturating_add(1);
        }
    }

    pub fn records(&self) -> usize {
        self.records
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn metrics(&self) -> ArchiveMetrics {
        self.metrics
    }

    pub fn oldest(&self) -> Option<&ArchivedFrame> {
        self.frames.front()
    }

    pub fn newest(&self) -> Option<&ArchivedFrame> {
        self.frames.back()
    }

    pub fn frame(&self, tick: Tick) -> Option<&ArchivedFrame> {
        self.frames.iter().rev().find(|f| f.tick == tick)
    }

    fn frame_count(&self) -> Option<i64> {
        self.frames.back().map(|f| i64::from(f.tick.0) + 1)
    }

    pub fn duration_ms(&self) -> i32 {
        let (Some(count), Some(oldest)) = (self.frame_count(), self.frames.front()) else {
            return 0;
        };
        ((count - i64::from(oldest.tick.0)) * i64::from(ARCHIVE_TICK_MS)) as i32
    }

    pub fn lookup(&self, requested_ms: i32) -> ArchiveLookup {
        if requested_ms < 1 {
            return ArchiveLookup {
                attained_ms: requested_ms,
                tick: None,
            };
        }
        let (Some(count), Some(oldest)) = (self.frame_count(), self.frames.front()) else {
            return ArchiveLookup {
                attained_ms: 0,
                tick: None,
            };
        };
        let back = i64::from(requested_ms) / i64::from(ARCHIVE_TICK_MS);
        let index = (count - back).max(i64::from(oldest.tick.0));
        match self.frames.iter().find(|f| i64::from(f.tick.0) >= index) {
            Some(found) => ArchiveLookup {
                attained_ms: ((count - i64::from(found.tick.0)) * i64::from(ARCHIVE_TICK_MS))
                    as i32,
                tick: Some(found.tick),
            },

            None => ArchiveLookup {
                attained_ms: 0,
                tick: None,
            },
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.records = 0;
        self.bytes = 0;
    }
}
