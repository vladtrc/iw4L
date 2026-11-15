#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathNode {
    pub kind: &'static str,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub radius: f32,
    pub spawnflags: i32,
    pub keys: Vec<(&'static str, String)>,
    pub links: Vec<(u16, f32)>,
}

pub const IW4_NODE_TYPES: [&str; 20] = [
    "BAD NODE",
    "Path",
    "Cover Stand",
    "Cover Crouch",
    "Cover Crouch Window",
    "Cover Prone",
    "Cover Right",
    "Cover Left",
    "Ambush",
    "Exposed",
    "Conceal Stand",
    "Conceal Crouch",
    "Conceal Prone",
    "Door",
    "Door Interior",
    "Scripted",
    "Begin",
    "End",
    "Turret",
    "Guard",
];
