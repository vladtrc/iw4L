use crate::client::cg_frame::CgFrameClock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictedError {
    error: [f32; 3],

    error_time_ms: i32,

    time_ms: i32,

    old_time_ms: i32,

    last_miss_len: f32,

    reset_count: u32,

    new_entity_count: u32,
}

impl Default for PredictedError {
    fn default() -> Self {
        Self {
            error: [0.0; 3],
            error_time_ms: 0,
            time_ms: 0,
            old_time_ms: 0,
            last_miss_len: 0.0,
            reset_count: 0,
            new_entity_count: 0,
        }
    }
}

impl PredictedError {
    pub const ERROR_DECAY_MS: f32 = 100.0;

    pub const SNAP_BELOW: f32 = 0.1;

    pub fn is_active(&self) -> bool {
        length_sq(self.error) > 1e-9
    }

    pub fn error(&self) -> [f32; 3] {
        self.error
    }

    pub fn error_time_ms(&self) -> i32 {
        self.error_time_ms
    }

    pub fn last_miss_len(&self) -> f32 {
        self.last_miss_len
    }

    pub fn reset_count(&self) -> u32 {
        self.reset_count
    }

    pub fn new_entity_count(&self) -> u32 {
        self.new_entity_count
    }

    pub fn sync_frame(&mut self, clock: &CgFrameClock) {
        self.time_ms = clock.time();
        self.old_time_ms = clock.old_time();
    }

    pub fn begin(&mut self, pre_view: [f32; 3], post_authoritative: [f32; 3]) {
        let delta = [
            pre_view[0] - post_authoritative[0],
            pre_view[1] - post_authoritative[1],
            pre_view[2] - post_authoritative[2],
        ];
        let len = math_iw4::vec3_length(delta);
        if len <= Self::SNAP_BELOW {
            return;
        }
        self.last_miss_len = len;
        if Self::ERROR_DECAY_MS == 0.0 {
            self.error = [0.0; 3];
        } else {
            let f = decay_scale(Self::ERROR_DECAY_MS, self.time_ms, self.error_time_ms);
            self.error = scale(self.error, f);
        }
        self.error = add(delta, self.error);
        self.error_time_ms = self.old_time_ms;
    }

    pub fn reset(&mut self) {
        self.error = [0.0; 3];
        self.error_time_ms = 0;
        self.reset_count = self.reset_count.saturating_add(1);
    }

    pub fn reset_new_entity(&mut self) {
        self.reset();
        self.new_entity_count = self.new_entity_count.saturating_add(1);
    }

    pub fn note_teleport_delta(&mut self, pre_view: [f32; 3], post_authoritative: [f32; 3]) {
        let delta = [
            pre_view[0] - post_authoritative[0],
            pre_view[1] - post_authoritative[1],
            pre_view[2] - post_authoritative[2],
        ];
        self.last_miss_len = math_iw4::vec3_length(delta);
    }

    pub fn offset(&self) -> [f32; 3] {
        self.offset_for_pm_type(0)
    }

    pub fn offset_for_pm_type(&self, pm_type: i32) -> [f32; 3] {
        if pm_type == playerstate_iw4::PM_TYPE_NORMAL_LINKED
            || pm_type == playerstate_iw4::PM_TYPE_DEAD_LINKED
        {
            return [0.0; 3];
        }
        if Self::ERROR_DECAY_MS <= 0.0 {
            return [0.0; 3];
        }
        let f = decay_scale(Self::ERROR_DECAY_MS, self.time_ms, self.error_time_ms);
        if !(0.0 < f && f < 1.0) {
            return [0.0; 3];
        }
        scale(self.error, f)
    }
}

fn decay_scale(decay_ms: f32, time_ms: i32, error_time_ms: i32) -> f32 {
    let t = time_ms.wrapping_sub(error_time_ms) as f32;
    ((decay_ms - t) / decay_ms).max(0.0)
}

fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn length_sq(v: [f32; 3]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}
