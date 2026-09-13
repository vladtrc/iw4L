use weapon_iw4::{
    FireRecoilImpulse, FireRecoilPsScales, GunKickRange, GunKickSpring, ViewKickRange,
    bg_weapon_fire_recoil, cg_kick_angles, fire_recoil_gun_range, fire_recoil_view_range,
    gun_recoil_single_angle, kick_angles_center_speed, lerp_gun_kick_spring,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KickParams {
    pub f_ads_view_kick_center_speed: f32,
    pub f_hip_view_kick_center_speed: f32,
    pub gun_max_pitch: f32,
    pub gun_max_yaw: f32,
    pub ads_gun_kick_reduced_kick_percent: f32,
    pub ads_gun_kick_pitch_min: f32,
    pub ads_gun_kick_pitch_max: f32,
    pub ads_gun_kick_yaw_min: f32,
    pub ads_gun_kick_yaw_max: f32,
    pub ads_gun_kick_accel: f32,
    pub ads_gun_kick_speed_max: f32,
    pub ads_gun_kick_speed_decay: f32,
    pub ads_gun_kick_static_decay: f32,
    pub ads_view_kick_pitch_min: f32,
    pub ads_view_kick_pitch_max: f32,
    pub ads_view_kick_yaw_min: f32,
    pub ads_view_kick_yaw_max: f32,
    pub hip_gun_kick_reduced_kick_percent: f32,
    pub hip_gun_kick_pitch_min: f32,
    pub hip_gun_kick_pitch_max: f32,
    pub hip_gun_kick_yaw_min: f32,
    pub hip_gun_kick_yaw_max: f32,
    pub hip_gun_kick_accel: f32,
    pub hip_gun_kick_speed_max: f32,
    pub hip_gun_kick_speed_decay: f32,
    pub hip_gun_kick_static_decay: f32,
    pub hip_view_kick_pitch_min: f32,
    pub hip_view_kick_pitch_max: f32,
    pub hip_view_kick_yaw_min: f32,
    pub hip_view_kick_yaw_max: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewKickState {
    pub kick_avel: [f32; 3],

    pub kick_angles: [f32; 3],

    pub gun_speed: [f32; 2],

    pub gun_angles: [f32; 2],

    rng: u32,
}

impl Default for ViewKickState {
    fn default() -> Self {
        Self {
            kick_avel: [0.0; 3],
            kick_angles: [0.0; 3],
            gun_speed: [0.0; 2],
            gun_angles: [0.0; 2],
            rng: 0xA341_316C,
        }
    }
}

impl ViewKickState {
    pub fn reset(&mut self) {
        let rng = self.rng;
        *self = Self::default();
        self.rng = rng;
    }

    pub fn seed_fire(
        &mut self,
        kick: &KickParams,
        weapon_pos_frac: f32,
        weap_flags: u32,
        recoil_scale: i32,
        reduce_window_active: bool,
    ) {
        self.seed_fire_scaled(
            kick,
            weapon_pos_frac,
            FireRecoilPsScales {
                reduce_window_active,
                reduced_percent: 0.0,
                weap_flags,
                recoil_scale,
            },
        );
    }

    pub fn seed_fire_scaled(
        &mut self,
        kick: &KickParams,
        weapon_pos_frac: f32,
        ps: FireRecoilPsScales,
    ) {
        let view = fire_recoil_view_range(
            weapon_pos_frac,
            ViewKickRange {
                pitch_min: kick.hip_view_kick_pitch_min,
                pitch_max: kick.hip_view_kick_pitch_max,
                yaw_min: kick.hip_view_kick_yaw_min,
                yaw_max: kick.hip_view_kick_yaw_max,
            },
            ViewKickRange {
                pitch_min: kick.ads_view_kick_pitch_min,
                pitch_max: kick.ads_view_kick_pitch_max,
                yaw_min: kick.ads_view_kick_yaw_min,
                yaw_max: kick.ads_view_kick_yaw_max,
            },
        );
        let gun = fire_recoil_gun_range(
            weapon_pos_frac,
            GunKickRange {
                pitch_min: kick.hip_gun_kick_pitch_min,
                pitch_max: kick.hip_gun_kick_pitch_max,
                yaw_min: kick.hip_gun_kick_yaw_min,
                yaw_max: kick.hip_gun_kick_yaw_max,
            },
            GunKickRange {
                pitch_min: kick.ads_gun_kick_pitch_min,
                pitch_max: kick.ads_gun_kick_pitch_max,
                yaw_min: kick.ads_gun_kick_yaw_min,
                yaw_max: kick.ads_gun_kick_yaw_max,
            },
        );
        let mut ps = ps;
        if ps.reduce_window_active {
            ps.reduced_percent = if weapon_pos_frac == 1.0 {
                kick.ads_gun_kick_reduced_kick_percent
            } else {
                kick.hip_gun_kick_reduced_kick_percent
            };
        }
        let unit01 = [self.unit01(), self.unit01(), self.unit01(), self.unit01()];
        let FireRecoilImpulse {
            kick_avel,
            gun_speed_delta,
        } = bg_weapon_fire_recoil(view, gun, ps, unit01);
        self.kick_avel = kick_avel;
        self.gun_speed[0] += gun_speed_delta[0];
        self.gun_speed[1] += gun_speed_delta[1];
    }

    pub fn advance(
        &mut self,
        kick: &KickParams,
        weapon_index: i32,
        weapon_pos_frac: f32,
        frametime_ms: i32,
    ) {
        if frametime_ms <= 0 {
            return;
        }
        let center = kick_angles_center_speed(
            weapon_index,
            weapon_pos_frac,
            kick.f_hip_view_kick_center_speed,
            kick.f_ads_view_kick_center_speed,
        );
        cg_kick_angles(
            &mut self.kick_angles,
            &mut self.kick_avel,
            frametime_ms,
            center,
        );

        let spring = lerp_gun_kick_spring(
            GunKickSpring {
                accel: kick.hip_gun_kick_accel,
                speed_max: kick.hip_gun_kick_speed_max,
                speed_decay: kick.hip_gun_kick_speed_decay,
                static_decay: kick.hip_gun_kick_static_decay,
            },
            GunKickSpring {
                accel: kick.ads_gun_kick_accel,
                speed_max: kick.ads_gun_kick_speed_max,
                speed_decay: kick.ads_gun_kick_speed_decay,
                static_decay: kick.ads_gun_kick_static_decay,
            },
            weapon_pos_frac,
        );
        let mut t = frametime_ms;
        while t > 0 {
            let step_ms = if t < 5 { t } else { 5 };
            let dt = step_ms as f32 * 0.001;
            gun_recoil_single_angle(
                &mut self.gun_angles[0],
                &mut self.gun_speed[0],
                dt,
                kick.gun_max_pitch,
                spring,
            );
            gun_recoil_single_angle(
                &mut self.gun_angles[1],
                &mut self.gun_speed[1],
                dt,
                kick.gun_max_yaw,
                spring,
            );
            t -= 5;
        }
    }

    fn unit01(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = if x == 0 { 1 } else { x };
        (x as f32) / (u32::MAX as f32)
    }
}

#[inline]
pub fn add_kick_to_viewangles(viewangles: [f32; 3], kick_angles: [f32; 3]) -> [f32; 3] {
    [
        viewangles[0] + kick_angles[0],
        viewangles[1] + kick_angles[1],
        viewangles[2] + kick_angles[2],
    ]
}
