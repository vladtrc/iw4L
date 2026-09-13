use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct ClsRealtime {
    realtime: i32,

    frametime: i32,

    started: bool,

    frac_ms: f32,
}

impl ClsRealtime {
    pub fn realtime(&self) -> i32 {
        self.realtime
    }

    pub fn frametime(&self) -> i32 {
        self.frametime
    }

    pub fn frametime_secs(&self) -> f32 {
        self.frametime as f32 / 1000.0
    }

    pub fn frac_ms(&self) -> f32 {
        self.frac_ms
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn advance_listen(&mut self, delta_secs: f32) {
        if !self.started {
            self.started = true;
            self.frametime = 0;
            self.frac_ms = 0.0;
            return;
        }
        self.frac_ms += delta_secs.max(0.0) * 1000.0;
        let dt_ms = self.frac_ms as i32;
        self.frac_ms -= dt_ms as f32;
        self.frametime = dt_ms;
        self.realtime = self.realtime.wrapping_add(dt_ms);
    }
}
