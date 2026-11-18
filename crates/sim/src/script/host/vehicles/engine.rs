use asset_core::VehicleEngineSounds;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Initial,
    Off,
    Starting(i32),
    Idle,
    Rising(i32),
    Sustain,
    Falling(i32),
}

#[derive(Clone, Debug)]
pub(super) struct Engine {
    sounds: VehicleEngineSounds,
    enabled: bool,
    phase: Phase,
    speed_fraction: f32,
}

impl Engine {
    pub(super) fn new(sounds: VehicleEngineSounds) -> Self {
        Self {
            sounds,
            enabled: true,
            phase: Phase::Initial,
            speed_fraction: 0.0,
        }
    }

    pub(super) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(super) fn advance(&mut self, now: i32, speed: f32, previous_speed: f32) {
        self.speed_fraction =
            if self.sounds.blend_speed.is_finite() && self.sounds.blend_speed > 0.0 {
                (speed.abs() / self.sounds.blend_speed).clamp(0.0, 1.0)
            } else if speed > 0.0 {
                1.0
            } else {
                0.0
            };
        if !self.enabled {
            if self.phase != Phase::Initial {
                self.phase = Phase::Off;
            }
            return;
        }
        let powered = speed > 0.0 && speed >= previous_speed;
        self.phase = match self.phase {
            Phase::Initial | Phase::Off => {
                if self.sounds.startup.is_some() {
                    Phase::Starting(now.saturating_add(self.sounds.startup_ms))
                } else {
                    Phase::Idle
                }
            }
            Phase::Starting(end) if now >= end => Phase::Idle,
            Phase::Idle if powered => {
                if self.sounds.ramp_up.is_some() {
                    Phase::Rising(now.saturating_add(self.sounds.ramp_up_ms))
                } else {
                    Phase::Sustain
                }
            }
            Phase::Rising(_) | Phase::Sustain if !powered => {
                if self.sounds.ramp_down.is_some() {
                    Phase::Falling(now.saturating_add(self.sounds.ramp_down_ms))
                } else {
                    Phase::Idle
                }
            }
            Phase::Rising(end) if now >= end => Phase::Sustain,
            Phase::Falling(end) if now >= end => Phase::Idle,
            phase => phase,
        };
    }

    pub(super) fn voices(&self) -> Vec<(u8, &str, f32)> {
        let mut voices = Vec::new();
        if let Some(alias) = self.alias() {
            voices.push((0, alias, 1.0));
        }
        if matches!(
            self.phase,
            Phase::Idle | Phase::Rising(_) | Phase::Sustain | Phase::Falling(_)
        ) {
            // Independent audio model: crossfade each authored low/high pair by path speed.
            let t = self.speed_fraction;
            for (slot, alias, gain) in [
                (1, self.sounds.idle_low.as_deref(), 1.0 - t),
                (2, self.sounds.idle_high.as_deref(), t),
                (3, self.sounds.engine_low.as_deref(), 1.0 - t),
                (4, self.sounds.engine_high.as_deref(), t),
            ] {
                if let Some(alias) = alias {
                    voices.push((slot, alias, gain));
                }
            }
        }
        voices
    }

    pub(super) fn alias(&self) -> Option<&str> {
        match self.phase {
            Phase::Initial => None,
            Phase::Off => self.sounds.shutdown.as_deref(),
            Phase::Starting(_) => self.sounds.startup.as_deref(),
            Phase::Idle => self.sounds.idle.as_deref(),
            Phase::Rising(_) => self.sounds.ramp_up.as_deref(),
            Phase::Sustain => self.sounds.sustain.as_deref(),
            Phase::Falling(_) => self.sounds.ramp_down.as_deref(),
        }
    }
}

fn set_enabled(
    world: &mut bevy_ecs::world::World,
    receiver: &crate::script::Value,
    enabled: bool,
) -> Result<crate::script::Value, String> {
    super::heli(world, receiver)?
        .engine
        .as_mut()
        .ok_or("vehicle engine sound inputs are unavailable")?
        .set_enabled(enabled);
    Ok(crate::script::Value::Undefined)
}

pub(super) fn register(registry: &mut crate::script::NativeRegistry) {
    registry.register(
        crate::script::Namespace::Method,
        "vehicle_turnengineoff",
        |world, receiver, _| set_enabled(world, receiver, false),
    );
    registry.register(
        crate::script::Namespace::Method,
        "vehicle_turnengineon",
        |world, receiver, _| set_enabled(world, receiver, true),
    );
}
