use sim::Snapshot;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::transport::frame::FrameParts;

#[derive(Debug)]
struct SentFrame {
    sequence: u32,
    snapshot: Arc<Snapshot>,
    parts: FrameParts,
}

#[derive(Debug)]
pub struct AckedBaselineTable {
    sent: VecDeque<SentFrame>,

    highest_acked: Option<u32>,
    max_retained: usize,
}

impl Default for AckedBaselineTable {
    fn default() -> Self {
        Self::new(32)
    }
}

pub(super) fn sequence_after(sequence: u32, previous: u32) -> bool {
    sequence != previous && sequence.wrapping_sub(previous) < 0x8000_0000
}

impl AckedBaselineTable {
    pub fn new(max_retained: usize) -> Self {
        Self {
            sent: VecDeque::new(),
            highest_acked: None,
            max_retained: max_retained.max(2),
        }
    }

    pub fn remember(&mut self, snapshot_seq: u32, snapshot: Arc<Snapshot>, parts: FrameParts) {
        assert_ne!(snapshot_seq, 0, "zero denotes an unbased snapshot");
        let frame = SentFrame {
            sequence: snapshot_seq,
            snapshot,
            parts,
        };
        if let Some(previous) = self
            .sent
            .iter_mut()
            .find(|row| row.sequence == snapshot_seq)
        {
            *previous = frame;
        } else {
            self.sent.push_back(frame);
        }
        self.trim();
    }

    pub fn ack(&mut self, snapshot_seq: u32) -> Option<sim::Tick> {
        let tick = self
            .sent
            .iter()
            .find(|row| row.sequence == snapshot_seq)?
            .snapshot
            .tick;
        self.highest_acked = Some(match self.highest_acked {
            Some(prev) if !sequence_after(snapshot_seq, prev) => prev,
            Some(_) => snapshot_seq,
            None => snapshot_seq,
        });

        if let Some(acked) = self.highest_acked {
            self.sent
                .retain(|frame| frame.sequence == acked || sequence_after(frame.sequence, acked));
        }
        Some(tick)
    }

    pub fn baseline_seq_for_encode(&self) -> u32 {
        match self.highest_acked {
            Some(seq) if self.sent.iter().any(|frame| frame.sequence == seq) => seq,
            _ => 0,
        }
    }

    pub fn clear_missing_ack(&mut self) -> bool {
        match self.highest_acked {
            Some(seq) if !self.sent.iter().any(|frame| frame.sequence == seq) => {
                self.highest_acked = None;
                true
            }
            _ => false,
        }
    }

    fn is_pinned(&self, seq: u32) -> bool {
        if self.highest_acked == Some(seq) {
            return true;
        }
        let newest = self.sent.back().map(|frame| frame.sequence);
        if newest == Some(seq) {
            return true;
        }
        self.highest_acked.is_none() && self.sent.front().map(|frame| frame.sequence) == Some(seq)
    }

    fn trim(&mut self) {
        while self.sent.len() > self.max_retained {
            let victim = self
                .sent
                .iter()
                .position(|frame| !self.is_pinned(frame.sequence));
            match victim {
                Some(index) => {
                    self.sent.remove(index);
                }
                None => break,
            }
        }
    }

    pub fn highest_acked(&self) -> Option<u32> {
        self.highest_acked
    }

    pub fn baseline_for_encode(&self) -> Option<&Snapshot> {
        let acked = self.highest_acked?;
        self.sent
            .iter()
            .find(|frame| frame.sequence == acked)
            .map(|sent| sent.snapshot.as_ref())
    }

    pub fn baseline_parts_for_encode(&self) -> Option<&FrameParts> {
        let acked = self.highest_acked?;
        self.sent
            .iter()
            .find(|frame| frame.sequence == acked)
            .map(|sent| &sent.parts)
    }

    pub fn may_encode_against(&self, baseline_seq: u32) -> bool {
        if baseline_seq == 0 {
            return true;
        }
        self.highest_acked == Some(baseline_seq)
            && self.sent.iter().any(|frame| frame.sequence == baseline_seq)
    }
}
