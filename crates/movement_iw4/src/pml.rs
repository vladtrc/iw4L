#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pml {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub frametime: f32,
    pub msec: i32,
    pub walking: u32,
    pub ground_plane: u32,
    pub almost_ground_plane: u32,
    pub ground_trace: [u32; 11],
    pub previous_origin: [f32; 3],
    pub previous_velocity: [f32; 3],
    pub holdrand: i32,
}
