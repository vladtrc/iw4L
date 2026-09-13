#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponCompleteDef {
    pub hide_tags: u32,
    pub sz_xanims: u32,
    pub ads_zoom_fov: f32,
    pub i_clip_size: i32,
    pub impact_type: i32,
    pub i_fire_time: i32,
    pub dpad_icon_ratio: i32,
    pub penetrate_multiplier: f32,
    pub f_ads_view_kick_center_speed: f32,
    pub f_hip_view_kick_center_speed: f32,
    pub i_alt_raise_time: i32,
    pub kill_icon: u32,
    pub dpad_icon: u32,
    pub i_first_raise_time: i32,
}
