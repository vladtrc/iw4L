use bevy::prelude::Resource;
use render_frame::skinning::SkinBatchLimits;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default)]
struct SupportState {
    device_generation: u64,
    limits: Option<SkinBatchLimits>,
}

#[derive(Resource, Clone)]
pub struct SkinSupport {
    state: Arc<Mutex<SupportState>>,
    pub resident_budget: u64,
}

impl Default for SkinSupport {
    fn default() -> Self {
        let resident_budget = std::env::var("IW4L_SKIN_RESIDENT_BYTES")
            .map_or(Ok(1024 * 1024 * 1024), |text| text.parse::<u64>())
            .ok()
            .filter(|bytes| (1024 * 1024..=4 * 1024 * 1024 * 1024).contains(bytes))
            .unwrap_or(0);
        Self {
            state: Arc::new(Mutex::new(SupportState::default())),
            resident_budget,
        }
    }
}

impl SkinSupport {
    pub fn report(&self, limits: SkinBatchLimits, device_generation: u64) {
        let mut state = self.state.lock().expect("skin support poisoned");
        state.limits = Some(limits);
        state.device_generation = device_generation;
    }

    pub fn device_generation(&self) -> u64 {
        self.state
            .lock()
            .expect("skin support poisoned")
            .device_generation
    }

    pub fn limits(&self) -> Option<SkinBatchLimits> {
        self.state.lock().expect("skin support poisoned").limits
    }
}
