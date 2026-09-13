#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Centity {
    pub e_type: u8,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub pose_origin: [f32; 3],
    pub current_state: [u8; 0x70],
    pub next_state: [u8; 0x7c],
    pub client_num: i32,
    pub other_entity_num: i16,
    pub current_valid: u8,
    pub previous_event_sequence: i32,
}
