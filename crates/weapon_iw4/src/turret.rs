/// Authored turret defaults. Angles/spreads are degrees, durations seconds and
/// range world units. Arc order is left, right, top, bottom; convergence pitch, yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretSettings {
    pub arcs: [f32; 4],
    pub convergence_seconds: [f32; 2],
    pub suppression_seconds: f32,
    pub ai_spread_degrees: f32,
    pub player_spread_degrees: f32,
    pub max_range: f32,
    pub player_position_dist: f32,
    pub aim_column_degrees: f32,
    pub barrel_spin: Option<BarrelSpinSettings>,
}

impl TurretSettings {
    pub fn finite(self) -> bool {
        self.arcs
            .into_iter()
            .chain(self.convergence_seconds)
            .chain([
                self.suppression_seconds,
                self.ai_spread_degrees,
                self.player_spread_degrees,
                self.max_range,
                self.aim_column_degrees,
                self.player_position_dist,
            ])
            .all(f32::is_finite)
            && self.barrel_spin.is_none_or(|spin| {
                [spin.turns_per_second, spin.up_seconds, spin.down_seconds]
                    .into_iter()
                    .all(f32::is_finite)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarrelSpinSettings {
    pub turns_per_second: f32,
    pub up_seconds: f32,
    pub down_seconds: f32,
}
