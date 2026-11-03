//! Delayed GPU aperture visibility. Missing, stale or invalid feedback admits.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct PortalOcclusionHistory(Arc<Mutex<History>>);

#[derive(Debug, Default)]
struct History {
    generation: u64,
    epoch: u64,
    frame: u64,
    eye: [f32; 3],
    forward: [f32; 3],
    viewport: [f32; 4],
    projection: [f32; 2],
    entries: HashMap<Vec<u32>, Entry>,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    frame: u64,
    zeroes: u8,
}

#[derive(Clone, Debug)]
pub struct PortalOcclusionFrame {
    pub generation: u64,
    history: PortalOcclusionHistory,
    epoch: u64,
    pub frame: u64,
    hidden: HashMap<Vec<u32>, bool>,
}

impl PartialEq for PortalOcclusionFrame {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.frame == other.frame
            && Arc::ptr_eq(&self.history.0, &other.history.0)
    }
}

impl PortalOcclusionHistory {
    pub fn begin(
        &self,
        generation: u64,
        eye: [f32; 3],
        forward: [f32; 3],
        projection: [f32; 2],
        viewport: [f32; 4],
    ) -> PortalOcclusionFrame {
        let mut h = self.0.lock().expect("portal feedback");
        let distance: f32 = eye.iter().zip(h.eye).map(|(a, b)| (a - b).powi(2)).sum();
        let facing: f32 = forward.iter().zip(h.forward).map(|(a, b)| a * b).sum();
        // Compare against the epoch anchor, not the preceding frame: slow
        // continuous motion must eventually invalidate old visibility too.
        if h.generation != generation
            || h.viewport != viewport
            || h.projection != projection
            || distance > 64.0 * 64.0
            || facing < 0.965926
        {
            h.epoch += 1;
            h.generation = generation;
            h.eye = eye;
            h.forward = forward;
            h.viewport = viewport;
            h.projection = projection;
            h.entries.clear();
        }
        h.frame += 1;
        let frame = h.frame;
        h.entries.retain(|_, e| frame.saturating_sub(e.frame) <= 6);
        PortalOcclusionFrame {
            generation: h.generation,
            history: self.clone(),
            epoch: h.epoch,
            frame,
            hidden: h
                .entries
                .iter()
                .map(|(k, e)| (k.clone(), e.zeroes >= 2))
                .collect(),
        }
    }
}

impl PortalOcclusionFrame {
    pub fn hidden(&self, key: &[u32]) -> bool {
        self.hidden.get(key).copied().unwrap_or(false)
    }

    pub fn publish(&self, key: Vec<u32>, samples: u64) {
        let mut h = self.history.0.lock().expect("portal feedback");
        if h.epoch != self.epoch || h.frame.saturating_sub(self.frame) > 6 {
            return;
        }
        if h.entries.get(&key).is_some_and(|e| e.frame >= self.frame) {
            return;
        }
        let zeroes = if samples > 0 {
            0
        } else {
            h.entries
                .get(&key)
                .map_or(1, |e| e.zeroes.saturating_add(1))
        };
        h.entries.insert(
            key,
            Entry {
                frame: self.frame,
                zeroes,
            },
        );
    }
}
