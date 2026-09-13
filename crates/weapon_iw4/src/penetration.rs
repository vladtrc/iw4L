pub const PENETRATE_TYPE_COUNT: usize = 4;

pub const PEN_SURF_TYPE_COUNT: usize = 31;

pub const SURF_NOPENETRATE: u32 = 0x100;

pub const CONTENTS_GLASS: u32 = 0x10;

pub const SURF_TYPE_FLESH: u32 = 7;

pub const MAX_PENETRATE_STEPS: usize = 5;

pub const MAX_EXTENDED_STEPS: usize = 12;

pub const ADVANCE_TRACE_FWD: f32 = 0.135;

pub const ADVANCE_TRACE_REV: f32 = 0.01;

pub const REV_END_EPS: f32 = 2.0;

pub const ADVANCE_DOT_MIN: f32 = 0.125;

pub const RIFLE_COLLATERAL_SCALE: f32 = 0.5;

pub const PEN_THICKNESS_FLOOR: f32 = 1.0;

pub const SURFACE_TYPE_NAMES: [&str; PEN_SURF_TYPE_COUNT] = [
    "default",
    "bark",
    "brick",
    "carpet",
    "cloth",
    "concrete",
    "dirt",
    "flesh",
    "foliage",
    "glass",
    "grass",
    "gravel",
    "ice",
    "metal",
    "mud",
    "paper",
    "plaster",
    "rock",
    "sand",
    "snow",
    "water",
    "wood",
    "asphalt",
    "ceramic",
    "plastic",
    "rubber",
    "cushion",
    "fruit",
    "paintedmetal",
    "riotshield",
    "slush",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletPenFacts {
    pub penetrate_type: i32,

    pub penetrate_multiplier: f32,

    pub rifle_bullet: bool,
}

impl Default for BulletPenFacts {
    fn default() -> Self {
        Self {
            penetrate_type: 0,
            penetrate_multiplier: 1.0,
            rifle_bullet: false,
        }
    }
}

impl BulletPenFacts {
    pub const fn pen_gate(self, penetration_enabled: bool) -> bool {
        penetration_enabled && self.penetrate_type != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenetrationDepthTable {
    depths: [[f32; PEN_SURF_TYPE_COUNT]; PENETRATE_TYPE_COUNT],
}

impl Default for PenetrationDepthTable {
    fn default() -> Self {
        Self::empty()
    }
}

impl PenetrationDepthTable {
    pub const fn empty() -> Self {
        Self {
            depths: [[0.0; PEN_SURF_TYPE_COUNT]; PENETRATE_TYPE_COUNT],
        }
    }

    pub const fn from_depths(depths: [[f32; PEN_SURF_TYPE_COUNT]; PENETRATE_TYPE_COUNT]) -> Self {
        Self { depths }
    }

    pub fn set_depth(&mut self, penetrate_type: usize, surface_type: usize, value: f32) {
        if penetrate_type < PENETRATE_TYPE_COUNT && surface_type < PEN_SURF_TYPE_COUNT {
            self.depths[penetrate_type][surface_type] = value;
        }
    }

    pub fn depth(self, penetrate_type: i32, surface_type: u32) -> f32 {
        let pt = penetrate_type as usize;
        let st = surface_type as usize;
        if pt == 0 || pt >= PENETRATE_TYPE_COUNT || st >= PEN_SURF_TYPE_COUNT || st == 0 {
            return 0.0;
        }
        self.depths[pt][st]
    }
}

pub fn depth_surface_type(surface_flags: u32, last_surface_type: u32) -> u32 {
    if surface_flags & SURF_NOPENETRATE != 0 {
        return 0;
    }
    let mut surf = (surface_flags >> 20) & 0x1f;
    if surf == 0 && last_surface_type != 0 {
        surf = last_surface_type;
    }
    surf
}

pub fn bg_advance_trace(
    hit_pos: [f32; 3],
    normal: [f32; 3],
    dir: [f32; 3],
    hit_is_world: bool,
    dist: f32,
) -> Option<[f32; 3]> {
    if hit_is_world && dist > 0.0 {
        let dot = -(normal[0] * dir[0] + normal[1] * dir[1] + normal[2] * dir[2]);
        if dot < ADVANCE_DOT_MIN {
            return None;
        }
        let offset = dist / dot;
        Some(vec3_mad(hit_pos, offset, dir))
    } else {
        Some(hit_pos)
    }
}

fn vec3_mad(a: [f32; 3], s: f32, b: [f32; 3]) -> [f32; 3] {
    [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]]
}

pub fn split_pen_key(key: &str) -> Option<(usize, usize)> {
    let (prefix, surf_name) = key.split_once('_')?;
    let ptype = match prefix {
        "small" => 1usize,
        "medium" => 2,
        "large" => 3,
        _ => return None,
    };
    let surf = SURFACE_TYPE_NAMES.iter().position(|&n| n == surf_name)?;
    Some((ptype, surf))
}
