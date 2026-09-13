use weapon_iw4::{
    SwayContribution, SwaySpringState, WeaponSwayParams, bg_calculate_weapon_movement_sway,
    lerp_sway_params, sway_contribution,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewSwayState {
    springs: SwaySpringState,
    prev_view_angles: Option<[f32; 3]>,
    last: SwayContribution,
}

impl ViewSwayState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn springs(&self) -> SwaySpringState {
        self.springs
    }

    pub fn advance(
        &mut self,
        hip: WeaponSwayParams,
        ads: WeaponSwayParams,
        view_angles: [f32; 3],
        weapon_pos_frac: f32,
        aim_down_sight: bool,
        overlay_active: bool,
        landing_scale: f32,
        dt_secs: f32,
    ) {
        if !dt_secs.is_finite() || dt_secs <= 0.0 {
            return;
        }
        if overlay_active && weapon_pos_frac > 0.0 {
            return;
        }
        let Some(prev) = self.prev_view_angles.replace(view_angles) else {
            self.last = sway_contribution(self.springs);
            return;
        };
        let params = if aim_down_sight {
            lerp_sway_params(hip, ads, weapon_pos_frac)
        } else {
            hip
        };
        bg_calculate_weapon_movement_sway(
            &mut self.springs,
            view_angles,
            prev,
            params,
            landing_scale,
            dt_secs,
        );
        self.last = sway_contribution(self.springs);
    }
}
