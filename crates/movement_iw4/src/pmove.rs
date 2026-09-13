#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pmove {
    pub ps: u32,
    pub cmd: [u8; 0x28],
    pub oldcmd: [u8; 0x28],
    pub tracemask: u32,
    pub numtouch: i32,
    pub touchents: [i32; 32],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub xyspeed: f32,
    pub mantle_started: u8,
    pub mantle_end_pos: [f32; 3],
    pub mantle_duration: i32,
    pub handler: u8,
}
