//! Delayed GPU aperture visibility. Missing, stale or invalid feedback admits.
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};
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
    up: [f32; 3],
    viewport: [f32; 4],
    projection: Mat4,
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
    epoch: Option<u64>,
    pub frame: u64,
    hidden: HashSet<Vec<u32>>,
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
        up: [f32; 3],
        projection: Mat4,
        viewport: [f32; 4],
    ) -> PortalOcclusionFrame {
        let forward = Vec3::from_array(forward).try_normalize();
        let up = Vec3::from_array(up).try_normalize();
        let determinant = projection.determinant();
        let valid = eye.iter().chain(&viewport).all(|v| v.is_finite())
            && viewport[0] >= 0.0
            && viewport[1] >= 0.0
            && viewport[2] > 0.0
            && viewport[3] > 0.0
            && projection.is_finite()
            && determinant.is_finite()
            && determinant != 0.0
            && forward.zip(up).is_some_and(|(f, u)| f.dot(u).abs() < 0.001);
        let forward = forward.unwrap_or(Vec3::ZERO);
        let up = up.unwrap_or(Vec3::ZERO);
        let mut h = self.0.lock().expect("portal feedback");
        let distance = (Vec3::from_array(eye) - Vec3::from_array(h.eye)).length_squared();
        let previous_forward = Vec3::from_array(h.forward);
        let previous_up = Vec3::from_array(h.up);
        // trace(R) = 1 + 2*cos(angle): include roll as well as heading.
        let orientation = forward.dot(previous_forward)
            + up.dot(previous_up)
            + forward.cross(up).dot(previous_forward.cross(previous_up));
        // Compare against the epoch anchor so accumulated slow motion expires it.
        if !valid
            || h.generation != generation
            || h.viewport != viewport
            || h.projection != projection
            || distance > 64.0 * 64.0
            || orientation < 1.0 + 2.0 * 0.965926
        {
            h.epoch += 1;
            h.generation = generation;
            if valid {
                h.eye = eye;
                h.forward = forward.to_array();
                h.up = up.to_array();
                h.viewport = viewport;
                h.projection = projection;
            }
            h.entries.clear();
        }
        h.frame += 1;
        let frame = h.frame;
        h.entries.retain(|_, e| frame.saturating_sub(e.frame) <= 6);
        PortalOcclusionFrame {
            generation: h.generation,
            history: self.clone(),
            epoch: valid.then_some(h.epoch),
            frame,
            hidden: h
                .entries
                .iter()
                .filter(|(_, e)| e.zeroes >= 2)
                .map(|(k, _)| k.clone())
                .collect(),
        }
    }
}

impl PortalOcclusionFrame {
    pub fn hidden(&self, key: &[u32]) -> bool {
        self.hidden.contains(key)
    }

    pub fn publish(&self, key: Vec<u32>, samples: u64) {
        let Some(epoch) = self.epoch else {
            return;
        };
        let mut h = self.history.0.lock().expect("portal feedback");
        if h.epoch != epoch || self.frame > h.frame || h.frame - self.frame > 6 {
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
