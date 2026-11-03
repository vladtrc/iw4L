use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBodyFacts {
    pub body_resolved: bool,

    pub fire_time_ms: i32,

    pub impact_type: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,
    pub first_raise_time_ms: i32,

    pub fire_delay_ms: i32,

    pub hold_fire_time_ms: i32,

    pub weap_type: i32,

    pub player_anim_type: i32,

    pub weap_class: i32,

    pub offhand_class: i32,

    pub shots_per_fire: i32,

    pub ammo_index: i32,

    pub clip_index: i32,

    pub ammo_counter_clip: i32,

    pub low_ammo_warning_threshold: f32,

    pub hip_spread_stand_min: f32,

    pub hip_spread_ducked_min: f32,

    pub hip_spread_prone_min: f32,

    pub hip_spread_stand_max: f32,

    pub hip_spread_ducked_max: f32,

    pub hip_spread_prone_max: f32,

    pub hip_spread_decay_rate: f32,

    pub hip_spread_fire_add: f32,

    pub hip_spread_turn_add: f32,

    pub hip_spread_move_add: f32,

    pub hip_spread_ducked_decay: f32,

    pub hip_spread_prone_decay: f32,

    pub i_reticle_side_size: i32,

    pub i_reticle_min_ofs: i32,

    pub hip_reticle_side_pos: f32,

    pub ads_aim_pitch: f32,

    pub ads_crosshair_in_frac: f32,

    pub ads_crosshair_out_frac: f32,

    pub ads_spread: f32,

    pub can_hold_breath: bool,

    pub aim_down_sight: bool,

    pub thermal_scope: bool,

    pub silenced: bool,

    pub ads_zoom_fov: f32,
    pub scope_zoom: weapon_iw4::ScopeZoom,

    pub ads_dof: Option<[f32; 2]>,

    pub ads_zoom_in_frac: f32,

    pub ads_zoom_out_frac: f32,

    pub no_ads_when_mag_empty: bool,

    pub inherits_perks: bool,

    pub ads_reload_trans_time_ms: i32,
    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub melee_damage: i32,

    pub overlay_reticle: i32,

    pub overlay_interface: i32,

    pub ads_overlay_width: f32,

    pub ads_overlay_height: f32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub knife_model: u32,

    pub use_as_melee: bool,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo: Option<bool>,

    pub offhand_hold_is_cancelable: Option<bool>,

    pub move_speed_scale: f32,

    pub ads_move_speed_scale: f32,

    pub sprint_duration_scale: f32,

    pub ducked_ofs: [f32; 3],

    pub prone_ofs: [f32; 3],

    pub night_vision_wear_time: i32,

    pub ads_bob_factor: f32,

    pub ads_view_bob_mult: f32,

    pub movement: WeaponMovementOfsInputs,

    pub idle: WeaponIdleInputs,

    pub clip_size: i32,
    pub penetrate_type: i32,

    pub penetrate_multiplier: f32,

    pub motion_tracker: bool,

    pub rifle_bullet: bool,
    pub ricochet_chance: f32,
    pub explosive_bullet: bool,
    pub inventory_type: i32,
    pub fire_type: i32,
    pub max_ammo: i32,
    pub damage: i32,
    pub rechamber_time_ms: i32,

    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,
    pub reload_time_ms: i32,

    pub reload_show_rocket_time_ms: i32,
    pub reload_empty_time_ms: i32,
    pub reload_add_time_ms: i32,

    pub reload_empty_add_time_ms: i32,
    pub reload_start_time_ms: i32,

    pub reload_start_add_time_ms: i32,
    pub reload_end_time_ms: i32,

    pub dual_mag: Option<weapon_iw4::DualMagTimes>,

    pub kill_icon_ratio: i32,

    pub flip_kill_icon: bool,

    pub reload_ammo_add: i32,

    pub reload_start_add: i32,

    pub no_partial_reload: bool,

    pub bolt_action: bool,

    pub segmented_reload: bool,

    pub sprint_raise_time_ms: i32,

    pub sprint_loop_time_ms: i32,

    pub sprint_drop_time_ms: i32,
    pub stunned_start_time_ms: i32,
    pub stunned_end_time_ms: i32,
    pub fuse_time_ms: i32,

    pub auto_aim_range: f32,

    pub aim_assist_range: f32,

    pub aim_assist_range_ads: f32,

    pub cook_off_hold: bool,

    pub clip_only: bool,

    pub has_detonator: bool,
    pub detonate_delay_ms: i32,
    pub detonate_time_ms: i32,
    pub projectile_rotates: bool,
    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub damage_cone_angle: f32,
    pub missile_guidance: i32,
    pub ignition_delay_ms: i32,
    pub require_lock_to_fire: bool,
    pub stickiness: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,
    pub projectile_speed_relative_up: i32,
    pub refuses_pickup: bool,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
    pub location_damage_mult: Option<[f32; 20]>,
    pub start_ammo: i32,

    pub ammo_count_clip_relative: bool,
    pub min_damage: i32,
    pub min_player_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,
    pub flame_damage_interval_ms: i32,

    pub kick: WeaponKickFacts,

    pub sway: WeaponSwayFacts,

    pub dual_wield_view_model_offset: f32,

    pub no_dual_wield: bool,
    pub dual_wield: bool,
    pub fire_melees: bool,
}

impl WeaponBodyFacts {
    pub fn start_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(
            self.start_ammo,
            self.clip_size,
            self.ammo_count_clip_relative,
        )
    }

    pub fn max_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(self.max_ammo, self.clip_size, self.ammo_count_clip_relative)
    }
}

fn leftover_clip_relative_rounds(count: i32, clip_size: i32, clip_relative: bool) -> i32 {
    if clip_relative && clip_size > 0 && count > 0 {
        count.saturating_mul(clip_size)
    } else {
        count
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponKickFacts {
    pub f_ads_view_kick_center_speed: f32,

    pub f_hip_view_kick_center_speed: f32,
    pub gun_max_pitch: f32,
    pub gun_max_yaw: f32,
    pub ads_gun_kick_reduced_kick_bullets: i32,
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
    pub hip_gun_kick_reduced_kick_bullets: i32,
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

impl WeaponKickFacts {
    pub(crate) fn from_capture(c: fastfile_iw4::WeaponKickCapture) -> Self {
        Self {
            f_ads_view_kick_center_speed: c.f_ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: c.f_hip_view_kick_center_speed,
            gun_max_pitch: c.gun_max_pitch,
            gun_max_yaw: c.gun_max_yaw,
            ads_gun_kick_reduced_kick_bullets: c.ads_gun_kick_reduced_kick_bullets,
            ads_gun_kick_reduced_kick_percent: c.ads_gun_kick_reduced_kick_percent,
            ads_gun_kick_pitch_min: c.ads_gun_kick_pitch_min,
            ads_gun_kick_pitch_max: c.ads_gun_kick_pitch_max,
            ads_gun_kick_yaw_min: c.ads_gun_kick_yaw_min,
            ads_gun_kick_yaw_max: c.ads_gun_kick_yaw_max,
            ads_gun_kick_accel: c.ads_gun_kick_accel,
            ads_gun_kick_speed_max: c.ads_gun_kick_speed_max,
            ads_gun_kick_speed_decay: c.ads_gun_kick_speed_decay,
            ads_gun_kick_static_decay: c.ads_gun_kick_static_decay,
            ads_view_kick_pitch_min: c.ads_view_kick_pitch_min,
            ads_view_kick_pitch_max: c.ads_view_kick_pitch_max,
            ads_view_kick_yaw_min: c.ads_view_kick_yaw_min,
            ads_view_kick_yaw_max: c.ads_view_kick_yaw_max,
            hip_gun_kick_reduced_kick_bullets: c.hip_gun_kick_reduced_kick_bullets,
            hip_gun_kick_reduced_kick_percent: c.hip_gun_kick_reduced_kick_percent,
            hip_gun_kick_pitch_min: c.hip_gun_kick_pitch_min,
            hip_gun_kick_pitch_max: c.hip_gun_kick_pitch_max,
            hip_gun_kick_yaw_min: c.hip_gun_kick_yaw_min,
            hip_gun_kick_yaw_max: c.hip_gun_kick_yaw_max,
            hip_gun_kick_accel: c.hip_gun_kick_accel,
            hip_gun_kick_speed_max: c.hip_gun_kick_speed_max,
            hip_gun_kick_speed_decay: c.hip_gun_kick_speed_decay,
            hip_gun_kick_static_decay: c.hip_gun_kick_static_decay,
            hip_view_kick_pitch_min: c.hip_view_kick_pitch_min,
            hip_view_kick_pitch_max: c.hip_view_kick_pitch_max,
            hip_view_kick_yaw_min: c.hip_view_kick_yaw_min,
            hip_view_kick_yaw_max: c.hip_view_kick_yaw_max,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayFacts {
    pub sway_max_angle: f32,
    pub sway_lerp_speed: f32,
    pub sway_pitch_scale: f32,
    pub sway_yaw_scale: f32,
    pub sway_horiz_scale: f32,
    pub sway_vert_scale: f32,

    pub sway_shell_shock_scale: f32,
    pub ads_sway_max_angle: f32,
    pub ads_sway_lerp_speed: f32,
    pub ads_sway_pitch_scale: f32,
    pub ads_sway_yaw_scale: f32,
    pub ads_sway_horiz_scale: f32,
    pub ads_sway_vert_scale: f32,
}

impl WeaponSwayFacts {
    pub(crate) fn from_capture(c: fastfile_iw4::WeaponSwayCapture) -> Self {
        Self {
            sway_max_angle: c.sway_max_angle,
            sway_lerp_speed: c.sway_lerp_speed,
            sway_pitch_scale: c.sway_pitch_scale,
            sway_yaw_scale: c.sway_yaw_scale,
            sway_horiz_scale: c.sway_horiz_scale,
            sway_vert_scale: c.sway_vert_scale,
            sway_shell_shock_scale: c.sway_shell_shock_scale,
            ads_sway_max_angle: c.ads_sway_max_angle,
            ads_sway_lerp_speed: c.ads_sway_lerp_speed,
            ads_sway_pitch_scale: c.ads_sway_pitch_scale,
            ads_sway_yaw_scale: c.ads_sway_yaw_scale,
            ads_sway_horiz_scale: c.ads_sway_horiz_scale,
            ads_sway_vert_scale: c.ads_sway_vert_scale,
        }
    }

    pub fn hip_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.sway_max_angle,
            pitch_scale: self.sway_pitch_scale,
            yaw_scale: self.sway_yaw_scale,
            horiz_scale: self.sway_horiz_scale,
            vert_scale: self.sway_vert_scale,
        }
    }

    pub fn ads_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.ads_sway_max_angle,
            pitch_scale: self.ads_sway_pitch_scale,
            yaw_scale: self.ads_sway_yaw_scale,
            horiz_scale: self.ads_sway_horiz_scale,
            vert_scale: self.ads_sway_vert_scale,
        }
    }
}

impl WeaponBodyFacts {
    pub fn apply_hip_spread(&mut self, block: [f32; 12]) {
        self.hip_spread_stand_min = block[0];
        self.hip_spread_ducked_min = block[1];
        self.hip_spread_prone_min = block[2];
        self.hip_spread_stand_max = block[3];
        self.hip_spread_ducked_max = block[4];
        self.hip_spread_prone_max = block[5];
        self.hip_spread_decay_rate = block[6];
        self.hip_spread_fire_add = block[7];
        self.hip_spread_turn_add = block[8];
        self.hip_spread_move_add = block[9];
        self.hip_spread_ducked_decay = block[10];
        self.hip_spread_prone_decay = block[11];
    }
}
