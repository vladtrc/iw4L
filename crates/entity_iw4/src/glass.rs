#[path = "glass_packed_dir.rs"]
mod glass_packed_dir;

pub const GLASS_DAMAGE_TO_WEAKEN: u16 = 25;

pub const GLASS_DAMAGE_TO_DESTROY: u16 = 100;

pub const GLASS_MELEE_DAMAGE: u16 = 1000;

pub const GLASS_DAMAGE_INVALID: u16 = 0xffff;

pub const GLASS_DAMAGE_ADD_CAP: u16 = 0xfffe;

pub const GLASS_SHATTER_SEED_LIFETIME_MS: i32 = 1000;

pub const GLASS_IMPACT_DIR_NONE: u8 = 0xff;

pub const GLASS_ENCODE_SHATTER_SCALE: f32 = 0.123046875;

pub const GLASS_ENCODE_SHATTER_BIAS: f32 = 31.5;

const GLASS_ENCODE_SHATTER_ROUND: f32 = 0.5;

const GLASS_ENCODE_SHATTER_CLAMP: f32 = 64.0;

pub const GLASS_DECODE_SHATTER_SCALE: f32 =
    f64::from_le_bytes([0x00, 0x00, 0x00, 0x20, 0x04, 0x41, 0x20, 0x40]) as f32;

pub const CG_GLASS_PIECE_LIMIT: usize = 0x400;

pub const GLASS_COLLAPSE_SHORT_THRESHOLD: f32 = 0.30000001192092896;

pub const GLASS_COLLAPSE_LONG_THRESHOLD: f32 = 0.699999988079071;

pub const GLASS_COLLAPSE_SHORT_RANGE_MS: f32 = -57_000.0;

pub const GLASS_COLLAPSE_LONG_RANGE_MS: f32 = -540_000.0;

pub const GLASS_COLLAPSE_SHORT_BASE_MS: i32 = 3_000;

pub const GLASS_COLLAPSE_LONG_BASE_MS: i32 = 60_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum GlassPieceState {
    #[default]
    Intact = 0,
    Weakened = 1,
    Shattered = 2,
    Deleted = 3,
}

impl GlassPieceState {
    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Intact),
            1 => Some(Self::Weakened),
            2 => Some(Self::Shattered),
            3 => Some(Self::Deleted),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassShatterSeed {
    pub impact_dir: u8,
    pub impact_pos: [u8; 2],
}

impl GlassShatterSeed {
    pub const fn new(impact_dir: u8, impact_pos: [u8; 2]) -> Option<Self> {
        if impact_dir == GLASS_IMPACT_DIR_NONE || impact_pos[0] > 0x3f || impact_pos[1] > 0x3f {
            return None;
        }
        Some(Self {
            impact_dir,
            impact_pos,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct CgGlassPiece {
    pub applied: u8,
    pub pending: u8,
    pub impact_dir: u8,
    pub impact_pos: [u8; 2],
}

const _: [(); 5] = [(); core::mem::size_of::<CgGlassPiece>()];
const _: [(); 0] = [(); core::mem::offset_of!(CgGlassPiece, applied)];
const _: [(); 1] = [(); core::mem::offset_of!(CgGlassPiece, pending)];
const _: [(); 2] = [(); core::mem::offset_of!(CgGlassPiece, impact_dir)];
const _: [(); 3] = [(); core::mem::offset_of!(CgGlassPiece, impact_pos)];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CgGlassApplyAction {
    None,
    Weaken,

    Shatter {
        hit: [f32; 3],
        dir: [f32; 3],
        weakened_first: bool,
    },
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassPaneBasis {
    pub origin: [f32; 3],
    pub axis_s: [f32; 3],
    pub axis_t: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct GGlassPiece {
    pub damage: u16,

    pub collapse_time: u16,

    pub last_state_change_time: i32,

    pub impact_dir: u8,

    pub impact_pos: [u8; 2],

    _gap_0b: u8,
}

impl Default for GGlassPiece {
    fn default() -> Self {
        Self {
            damage: 0,
            collapse_time: 0,
            last_state_change_time: 0,
            impact_dir: 0,
            impact_pos: [0; 2],
            _gap_0b: 0,
        }
    }
}

impl GGlassPiece {
    pub fn state(self) -> GlassPieceState {
        glass_state_from_damage(self.damage)
    }

    pub fn shatter_seed(self) -> Option<GlassShatterSeed> {
        if self.state() != GlassPieceState::Shattered {
            return None;
        }
        GlassShatterSeed::new(self.impact_dir, self.impact_pos)
    }
}

const _: [(); 0x0c] = [(); core::mem::size_of::<GGlassPiece>()];
const _: [(); 0x00] = [(); core::mem::offset_of!(GGlassPiece, damage)];
const _: [(); 0x02] = [(); core::mem::offset_of!(GGlassPiece, collapse_time)];
const _: [(); 0x04] = [(); core::mem::offset_of!(GGlassPiece, last_state_change_time)];
const _: [(); 0x08] = [(); core::mem::offset_of!(GGlassPiece, impact_dir)];
const _: [(); 0x09] = [(); core::mem::offset_of!(GGlassPiece, impact_pos)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassStateChange {
    pub previous: GlassPieceState,
    pub current: GlassPieceState,
}

pub fn glass_apply_damage(
    piece: &mut GGlassPiece,
    added: u32,
    at_time_ms: i32,
    weakened_collapse_time_cs: Option<u16>,
    shatter_seed: Option<GlassShatterSeed>,
) -> Option<GlassStateChange> {
    let previous = piece.state();
    if previous == GlassPieceState::Deleted || added == 0 {
        return None;
    }
    piece.damage = glass_add_damage(piece.damage, added);
    let current = piece.state();
    if current == previous {
        return None;
    }

    piece.last_state_change_time = at_time_ms;
    if current == GlassPieceState::Weakened {
        if let Some(collapse_time) = weakened_collapse_time_cs {
            piece.collapse_time = collapse_time;
        }
    } else if current == GlassPieceState::Shattered {
        match shatter_seed {
            Some(seed) => {
                piece.impact_dir = seed.impact_dir;
                piece.impact_pos = seed.impact_pos;
            }
            None => {
                piece.impact_dir = GLASS_IMPACT_DIR_NONE;
                piece.impact_pos = [0; 2];
            }
        }
    }

    Some(GlassStateChange { previous, current })
}

pub fn glass_shatter_seed_is_fresh(last_state_change_time: i32, at_time_ms: i32) -> bool {
    last_state_change_time.saturating_add(GLASS_SHATTER_SEED_LIFETIME_MS) >= at_time_ms
}

pub fn glass_state_from_damage(damage: u16) -> GlassPieceState {
    glass_state_from_damage_thresholds(damage, GLASS_DAMAGE_TO_WEAKEN, GLASS_DAMAGE_TO_DESTROY)
}

pub fn glass_state_from_damage_thresholds(
    damage: u16,
    weaken: u16,
    destroy: u16,
) -> GlassPieceState {
    if damage == GLASS_DAMAGE_INVALID {
        return GlassPieceState::Deleted;
    }
    if damage >= destroy {
        return GlassPieceState::Shattered;
    }
    if damage >= weaken {
        return GlassPieceState::Weakened;
    }
    GlassPieceState::Intact
}

pub fn glass_add_damage(current: u16, added: u32) -> u16 {
    let sum = u32::from(current).saturating_add(added);
    if sum < u32::from(GLASS_DAMAGE_INVALID) {
        sum as u16
    } else {
        GLASS_DAMAGE_ADD_CAP
    }
}

pub fn glass_is_solid(damage: u16) -> bool {
    glass_is_solid_threshold(damage, GLASS_DAMAGE_TO_DESTROY)
}

pub fn glass_is_solid_threshold(damage: u16, destroy: u16) -> bool {
    if damage == GLASS_DAMAGE_INVALID {
        return false;
    }
    damage < destroy
}

pub fn glass_encode_shatter_coord(uv: f32) -> u8 {
    let quantized = libm::floorf(
        uv * GLASS_ENCODE_SHATTER_SCALE + GLASS_ENCODE_SHATTER_BIAS + GLASS_ENCODE_SHATTER_ROUND,
    );
    if quantized >= GLASS_ENCODE_SHATTER_CLAMP {
        0x3f
    } else if quantized <= 0.0 {
        0
    } else {
        quantized as u8
    }
}

pub fn glass_decode_shatter_coord(quantized: u8) -> f32 {
    (quantized as f32 - GLASS_ENCODE_SHATTER_BIAS) * GLASS_DECODE_SHATTER_SCALE
}

pub fn glass_packed_dir_to_vec(index: u8) -> [f32; 3] {
    glass_packed_dir::GLASS_PACKED_DIR_TABLE
        .get(usize::from(index))
        .copied()
        .unwrap_or([0.0, 0.0, 0.0])
}

pub fn glass_shatter_impact_from_seed(
    pane: GlassPaneBasis,
    impact_dir: u8,
    impact_pos: [u8; 2],
) -> ([f32; 3], [f32; 3]) {
    if impact_dir == GLASS_IMPACT_DIR_NONE {
        return (pane.origin, [0.0, 0.0, 0.0]);
    }
    let dir = glass_packed_dir_to_vec(impact_dir);
    let u = glass_decode_shatter_coord(impact_pos[0]);
    let v = glass_decode_shatter_coord(impact_pos[1]);
    let hit = [
        pane.origin[0] + u * pane.axis_s[0] + v * pane.axis_t[0],
        pane.origin[1] + u * pane.axis_s[1] + v * pane.axis_t[1],
        pane.origin[2] + u * pane.axis_s[2] + v * pane.axis_t[2],
    ];
    (hit, dir)
}

pub fn cg_glass_read_change(
    row: &mut CgGlassPiece,
    pending: GlassPieceState,
    seed: Option<GlassShatterSeed>,
) {
    row.pending = pending.as_u8();
    if pending == GlassPieceState::Shattered {
        match seed {
            Some(seed) => {
                row.impact_dir = seed.impact_dir;
                row.impact_pos = seed.impact_pos;
            }
            None => {
                row.impact_dir = GLASS_IMPACT_DIR_NONE;
                row.impact_pos = [0, 0];
            }
        }
    }
}

pub fn cg_glass_is_solid(row: CgGlassPiece) -> bool {
    row.applied < GlassPieceState::Shattered.as_u8()
}

pub fn cg_glass_apply_state(
    row: &mut CgGlassPiece,
    pane: Option<GlassPaneBasis>,
) -> CgGlassApplyAction {
    if i32::from(row.applied) >= i32::from(row.pending) {
        return CgGlassApplyAction::None;
    }
    if row.pending == GlassPieceState::Deleted.as_u8() {
        row.applied = GlassPieceState::Deleted.as_u8();
        return CgGlassApplyAction::Delete;
    }
    let weakened_first = row.pending > 0 && row.applied == 0;
    if weakened_first && row.pending == GlassPieceState::Weakened.as_u8() {
        row.applied = GlassPieceState::Weakened.as_u8();
        return CgGlassApplyAction::Weaken;
    }
    if row.pending > GlassPieceState::Weakened.as_u8() {
        let (hit, dir) = match pane {
            Some(pane) => glass_shatter_impact_from_seed(pane, row.impact_dir, row.impact_pos),
            None => ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        };
        row.applied = row.pending;
        return CgGlassApplyAction::Shatter {
            hit,
            dir,
            weakened_first,
        };
    }
    row.applied = row.pending;
    CgGlassApplyAction::None
}

pub fn cg_glass_update(
    rows: &mut [CgGlassPiece],
    pane_at: impl Fn(usize) -> Option<GlassPaneBasis>,
) {
    for (i, row) in rows.iter_mut().enumerate() {
        if row.applied < row.pending {
            let _ = cg_glass_apply_state(row, pane_at(i));
        }
    }
}

pub fn glass_vec_to_packed_dir(dir: [f32; 3]) -> u8 {
    let mut best_dot = f32::NEG_INFINITY;
    let mut best = 0u8;
    for (i, row) in glass_packed_dir::GLASS_PACKED_DIR_TABLE.iter().enumerate() {
        let dot = dir[2] * row[2] + row[0] * dir[0] + row[1] * dir[1];
        if best_dot < dot {
            best_dot = dot;
            best = i as u8;
        }
    }
    best
}

pub fn glass_shatter_seed_from_hit(
    pane: GlassPaneBasis,
    hit: [f32; 3],
    dir: [f32; 3],
) -> Option<GlassShatterSeed> {
    let len2 = dir[2] * dir[2] + dir[0] * dir[0] + dir[1] * dir[1];
    if len2 <= 0.0 {
        return None;
    }
    let delta = [
        hit[0] - pane.origin[0],
        hit[1] - pane.origin[1],
        hit[2] - pane.origin[2],
    ];
    let u = delta[0] * pane.axis_s[0] + delta[1] * pane.axis_s[1] + delta[2] * pane.axis_s[2];
    let v = delta[0] * pane.axis_t[0] + delta[1] * pane.axis_t[1] + delta[2] * pane.axis_t[2];
    GlassShatterSeed::new(
        glass_vec_to_packed_dir(dir),
        [glass_encode_shatter_coord(u), glass_encode_shatter_coord(v)],
    )
}

fn ftol_round(x: f32) -> i32 {
    if x >= 0.0 {
        (x + 0.5) as i32
    } else {
        -((-x + 0.5) as i32)
    }
}

pub fn glass_weakened_collapse_time_cs(next_random: &mut impl FnMut() -> f32) -> u16 {
    let first = next_random();
    let ms = if first < GLASS_COLLAPSE_SHORT_THRESHOLD {
        let mag = next_random();
        GLASS_COLLAPSE_SHORT_BASE_MS - ftol_round(mag * GLASS_COLLAPSE_SHORT_RANGE_MS)
    } else if first < GLASS_COLLAPSE_LONG_THRESHOLD {
        let mag = next_random();
        GLASS_COLLAPSE_LONG_BASE_MS - ftol_round(mag * GLASS_COLLAPSE_LONG_RANGE_MS)
    } else {
        return 0;
    };
    (ms / 100) as u16
}

pub fn glass_collapse_due(piece: GGlassPiece, level_time_ms: i32) -> bool {
    if piece.collapse_time == 0 {
        return false;
    }
    if piece.damage == GLASS_DAMAGE_INVALID || piece.damage >= GLASS_DAMAGE_TO_DESTROY {
        return false;
    }
    let deadline = i32::from(piece.collapse_time)
        .saturating_mul(100)
        .saturating_add(piece.last_state_change_time);
    deadline < level_time_ms
}

pub fn glass_collapse_piece(piece: &mut GGlassPiece, at_time_ms: i32) -> Option<GlassStateChange> {
    if !glass_collapse_due(*piece, at_time_ms) {
        return None;
    }
    let previous = piece.state();
    piece.damage = GLASS_DAMAGE_TO_DESTROY;
    let current = piece.state();
    if current == previous {
        return None;
    }
    piece.last_state_change_time = at_time_ms;
    piece.impact_dir = GLASS_IMPACT_DIR_NONE;
    piece.impact_pos = [0; 2];
    Some(GlassStateChange { previous, current })
}

pub fn glass_should_notify_destroyed(change: GlassStateChange) -> bool {
    (change.previous as u8) < 2 && (change.current as u8) > 1
}
