use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::render_core::{QUANTUM, RenderShared, SAMPLE_RATE};

const CAPACITY: usize = 24_000;

pub(crate) struct StreamRing {
    samples: Box<[AtomicU32]>,
    written: AtomicU64,
    read: AtomicU64,
}

impl StreamRing {
    fn new() -> Self {
        Self {
            samples: (0..CAPACITY * 2).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicU64::new(0),
            read: AtomicU64::new(0),
        }
    }

    pub(crate) fn mix(&self, output: &mut [[f32; 2]; QUANTUM]) {
        let read = self.read.load(Ordering::Relaxed);
        let ready = (self.written.load(Ordering::Acquire) - read) as usize;
        let n = ready.min(QUANTUM);
        for (i, frame) in output[..n].iter_mut().enumerate() {
            let at = ((read as usize + i) % CAPACITY) * 2;
            frame[0] += f32::from_bits(self.samples[at].load(Ordering::Relaxed));
            frame[1] += f32::from_bits(self.samples[at + 1].load(Ordering::Relaxed));
        }
        self.read.store(read + n as u64, Ordering::Release);
    }
}

pub struct PcmStream {
    ring: Arc<StreamRing>,
    slot: Arc<Mutex<Option<Arc<StreamRing>>>>,
}

impl PcmStream {
    pub(crate) fn attach(shared: &RenderShared) -> Self {
        let ring = Arc::new(StreamRing::new());
        *shared.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(ring.clone());
        Self {
            ring,
            slot: shared.stream.clone(),
        }
    }

    pub fn push(&self, stereo: &[f32]) -> usize {
        let written = self.ring.written.load(Ordering::Relaxed);
        let n = (stereo.len() / 2).min(self.free_frames());
        for (i, frame) in stereo.chunks_exact(2).take(n).enumerate() {
            let at = ((written as usize + i) % CAPACITY) * 2;
            self.ring.samples[at].store(frame[0].to_bits(), Ordering::Relaxed);
            self.ring.samples[at + 1].store(frame[1].to_bits(), Ordering::Relaxed);
        }
        self.ring
            .written
            .store(written + n as u64, Ordering::Release);
        n
    }

    pub fn free_frames(&self) -> usize {
        let used =
            self.ring.written.load(Ordering::Relaxed) - self.ring.read.load(Ordering::Acquire);
        CAPACITY - used as usize
    }

    pub fn played_frames(&self) -> u64 {
        self.ring.read.load(Ordering::Acquire)
    }

    pub fn clock(&self) -> PcmClock {
        PcmClock(self.ring.clone())
    }
}

#[derive(Clone)]
pub struct PcmClock(Arc<StreamRing>);

impl PcmClock {
    pub fn played_frames(&self) -> u64 {
        self.0.read.load(Ordering::Acquire)
    }

    pub fn played_seconds(&self) -> f64 {
        self.played_frames() as f64 / f64::from(SAMPLE_RATE)
    }
}

impl Drop for PcmStream {
    fn drop(&mut self) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref().is_some_and(|r| Arc::ptr_eq(r, &self.ring)) {
            *slot = None;
        }
    }
}
