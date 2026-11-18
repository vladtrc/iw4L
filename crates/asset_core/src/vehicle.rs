/// Prepared vehicle motion and wheel contact inputs, in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMotion {
    pub kind: u32,
    pub physics: Option<VehiclePhysics>,
    pub acceleration: f32,
    pub wheel_count: u8,
    pub physics_enabled: bool,
    pub suspension_travel: [f32; 2],
}

impl VehicleMotion {
    pub fn valid(self) -> bool {
        self.kind <= 5
            && self.physics.is_none_or(VehiclePhysics::valid)
            && matches!(self.wheel_count, 0 | 4 | 6)
            && self.acceleration.is_finite()
            && self.acceleration >= 0.0
            && self
                .suspension_travel
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
    }
}

/// Authored chassis material and impact inputs. Speeds are world units/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehiclePhysics {
    pub mass: f32,
    pub bounce: f32,
    pub friction: f32,
    pub impulse_scale: f32,
    pub max_velocity: f32,
    pub collision_momentum: f32,
    pub collision_launch_scale: f32,
    pub wrecked_mass_scale: f32,
    pub wrecked_friction: f32,
    pub jolt_threshold: f32,
}

impl VehiclePhysics {
    pub fn valid(self) -> bool {
        self.mass > 0.0
            && self.max_velocity > 0.0
            && self.wrecked_mass_scale > 0.0
            && [
                self.mass,
                self.bounce,
                self.friction,
                self.impulse_scale,
                self.max_velocity,
                self.collision_momentum,
                self.collision_launch_scale,
                self.wrecked_mass_scale,
                self.wrecked_friction,
                self.jolt_threshold,
            ]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
    }
}

/// Authored engine aliases and transition durations, in milliseconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VehicleEngineSounds {
    pub idle_low: Option<String>,
    pub idle_high: Option<String>,
    pub engine_low: Option<String>,
    pub engine_high: Option<String>,
    pub blend_speed: f32,
    pub startup: Option<String>,
    pub shutdown: Option<String>,
    pub idle: Option<String>,
    pub sustain: Option<String>,
    pub ramp_up: Option<String>,
    pub ramp_down: Option<String>,
    pub startup_ms: i32,
    pub ramp_up_ms: i32,
    pub ramp_down_ms: i32,
}
