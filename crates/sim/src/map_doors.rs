use crate::ClientId;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoorSwitch {
    pub cmodel: u32,
    pub origin: [f32; 3],
    pub half: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoorLeaf {
    pub cmodel: u32,
    pub model_angles: [f32; 3],
    pub model: u32,
    pub brush: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapDoors {
    pub switches: [DoorSwitch; 2],
    pub leaves: [DoorLeaf; 2],
    pub sound_origin: [f32; 3],
    pub startup_at: Option<u32>,
    pub started_at: Option<u32>,
    pub open: bool,
    pub completed: bool,
    pub alarm_count: u8,
    pub activations: u32,

    pub hints: Vec<ClientId>,

    pub held: Vec<ClientId>,
}

impl MapDoors {
    pub fn unavailable(&self, now: u32) -> bool {
        self.started_at
            .is_some_and(|at| gamemode_iw4::radiation_unavailable(now.saturating_sub(at)))
    }
}
