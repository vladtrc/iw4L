#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathNode {
    pub kind: u32,
    pub spawnflags: u16,
    pub targetname: Option<String>,
    pub script_linkname: Option<String>,
    pub script_noteworthy: Option<String>,
    pub target: Option<String>,
    pub animscript: Option<String>,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub forward: [f32; 2],
    pub radius: f32,
    pub min_use_dist_sq: f32,
    pub overlap_nodes: [i16; 2],
    pub links: Vec<PathLink>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathLink {
    pub distance: f32,
    pub node: u16,
    pub disconnect_count: u8,
    pub negotiation: bool,
    pub flags: u8,
    pub bad_place_count: [u8; 3],
}

impl PathNode {
    pub fn type_name(&self) -> Option<&'static str> {
        const NAMES: [&str; 20] = [
            "Error",
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
        NAMES.get(self.kind as usize).copied()
    }
}
