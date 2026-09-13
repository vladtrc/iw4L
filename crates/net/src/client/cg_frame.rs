use bevy::prelude::*;

use crate::ServerTime;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CgFrameClock {
    time: i32,

    old_time: i32,

    frametime: i32,

    started: bool,

    frac_ms: f32,

    last_server: Option<i32>,
    reset_n: u32,
    fast_n: u32,
    nudge_n: u32,

    last_adjust: u8,

    extrapolated: bool,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CgameActive(pub bool);

impl CgameActive {
    pub fn get(self) -> bool {
        self.0
    }

    pub fn from_first_snapshot(world_spawned: bool, has_snap: bool) -> bool {
        world_spawned && has_snap
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CgameJoinCensus {
    pub level_ms: Option<i32>,

    pub tick: Option<u32>,

    pub bevy_elapsed_ms: Option<i32>,
}

impl CgameJoinCensus {
    pub fn latch(&mut self, level_ms: i32, tick: u32, bevy_elapsed_ms: i32) {
        if self.level_ms.is_some() {
            return;
        }
        self.level_ms = Some(level_ms);
        self.tick = Some(tick);
        self.bevy_elapsed_ms = Some(bevy_elapsed_ms);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl CgFrameClock {
    pub fn time(&self) -> i32 {
        self.time
    }

    pub fn old_time(&self) -> i32 {
        self.old_time
    }

    pub fn frametime(&self) -> i32 {
        self.frametime
    }

    pub fn frametime_secs(&self) -> f32 {
        self.frametime as f32 / 1000.0
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub const ADJUST_TIME_DELTA_RESET_MS: i32 = 500;

    pub const ADJUST_TIME_DELTA_FAST_MS: i32 = 100;

    pub fn last_adjust(&self) -> Option<&'static str> {
        match self.last_adjust {
            1 => Some("reset"),
            2 => Some("fast"),
            3 => Some("nudge"),
            _ => None,
        }
    }

    pub fn extrapolated(&self) -> bool {
        self.extrapolated
    }

    pub fn frac_ms(&self) -> f32 {
        self.frac_ms
    }

    pub fn assign_server_time(&mut self, server_time: i32) {
        self.old_time = self.time;
        self.time = server_time;
        let ft = self.time.wrapping_sub(self.old_time);
        if ft < 0 {
            self.frametime = 0;
            self.old_time = self.time;
        } else {
            self.frametime = ft;
        }
        self.started = true;
    }

    pub fn advance_listen(
        &mut self,
        delta_secs: f32,
        active: bool,
        server_time: Option<ServerTime>,
    ) {
        self.advance(delta_secs, active, server_time, false);
    }

    pub fn advance_remote(
        &mut self,
        delta_secs: f32,
        active: bool,
        server_time: Option<ServerTime>,
    ) {
        self.advance(delta_secs, active, server_time, true);
    }

    fn advance(
        &mut self,
        delta_secs: f32,
        active: bool,
        server_time: Option<ServerTime>,
        monotonic: bool,
    ) {
        if !active {
            return;
        }
        self.last_adjust = 0;
        if !self.started {
            let ms = server_time.map(ServerTime::ms).unwrap_or(0);
            self.time = ms;
            self.old_time = ms;
            self.frametime = 0;
            self.frac_ms = 0.0;
            self.last_server = Some(ms);
            self.started = true;
            return;
        }
        self.frac_ms += delta_secs.max(0.0) * 1000.0;
        let dt_ms = self.frac_ms as i32;
        self.frac_ms -= dt_ms as f32;
        let mut next = self.time.wrapping_add(dt_ms);
        self.extrapolated = self.last_server.is_some_and(|s| next >= s.wrapping_sub(8));
        if let Some(st) = server_time {
            let server = st.ms();
            if self.last_server != Some(server) {
                self.last_server = Some(server);
                let skew = server.wrapping_sub(next);
                let mag = skew.unsigned_abs();
                if mag > Self::ADJUST_TIME_DELTA_RESET_MS as u32 {
                    self.frac_ms = 0.0;
                    self.reset_n = self.reset_n.saturating_add(1);
                    self.last_adjust = 1;
                    self.assign_server_time(if monotonic {
                        server.max(self.time)
                    } else {
                        server
                    });
                    return;
                } else if mag > Self::ADJUST_TIME_DELTA_FAST_MS as u32 {
                    next = next.wrapping_add(skew / 2);
                    self.fast_n = self.fast_n.saturating_add(1);
                    self.last_adjust = 2;
                } else if mag > 0 {
                    next = next.wrapping_add(skew.signum());
                    self.nudge_n = self.nudge_n.saturating_add(1);
                    self.last_adjust = 3;
                }
            }
        }

        self.assign_server_time(if monotonic { next.max(self.time) } else { next });
    }
}
