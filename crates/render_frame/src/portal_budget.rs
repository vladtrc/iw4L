//! Wall-frame pressure changes portal limits slowly, with separate recovery
//! thresholds. Loading and offline fixed-clock capture cannot lower quality.
use crate::portal_plan::PortalRenderSettings;

#[derive(Default)]
pub struct PortalBudget {
    generation: u64,
    age: f64,
    fps: f64,
    pressure: f64,
    recovery: f64,
    level: u8,
    pub previous_views: usize,
}

impl PortalBudget {
    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn update(
        &mut self,
        generation: u64,
        dt: f64,
        real_time: bool,
        requested: PortalRenderSettings,
    ) -> PortalRenderSettings {
        if self.generation != generation || !real_time {
            *self = Self {
                generation,
                ..Default::default()
            };
        }
        if real_time && dt.is_finite() && dt > 0.0 && dt <= 0.25 {
            self.age += dt;
            // Average frame rate over wall time. Averaging durations with dt
            // weights gives a single long frame a second, unintended weight.
            self.fps = if self.fps == 0.0 {
                1.0 / dt
            } else {
                self.fps + (1.0 / dt - self.fps) * (1.0 - (-dt).exp())
            };
            if self.age >= 2.0 {
                let demand = self.previous_views >= 3 || self.level > 0;
                let target = if demand && self.fps < 15.0 {
                    2
                } else if demand && self.fps < 30.0 {
                    1
                } else {
                    0
                };
                if target > self.level {
                    self.recovery = 0.0;
                    self.pressure += dt;
                    if self.pressure >= 0.75 {
                        self.level = target;
                        self.pressure = 0.0;
                    }
                } else {
                    self.pressure = 0.0;
                    let good = match self.level {
                        2 => self.fps > 25.0,
                        1 => self.fps > 45.0,
                        _ => false,
                    };
                    self.recovery = if good { self.recovery + dt } else { 0.0 };
                    if self.recovery >= if self.level == 2 { 2.0 } else { 3.0 } {
                        self.level = self.level.saturating_sub(1);
                        self.recovery = 0.0;
                    }
                }
            }
        }
        let (views, depth) = match self.level {
            2 => (2, 1),
            1 => (4, 2),
            _ => (8, 3),
        };
        PortalRenderSettings {
            max_views: requested.max_views.min(views),
            depth: requested.depth.min(depth),
            ..requested
        }
    }
}
