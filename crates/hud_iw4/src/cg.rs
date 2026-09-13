#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cg {
    pub predicted_player_state: [u8; 0x311c],
    pub frametime: i32,
    pub time: i32,
    pub kick_angles: [f32; 3],
    pub blood_overlay_intensity: f32,
    pub view_damage: [u8; 0x330],
    pub pain_vision_on: u8,
    pub thermal_vision_enabled: u8,
}
