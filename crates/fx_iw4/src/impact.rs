#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ImpactType {
    None = 0,
    BulletSmall = 1,
    BulletLarge = 2,
    BulletAp = 3,
    BulletExplode = 4,
    Shotgun = 5,
    ShotgunExplode = 6,
    GrenadeBounce = 7,
    GrenadeExplode = 8,
    RocketExplode = 9,
    ProjectileDud = 10,
}

impl ImpactType {
    #[inline]
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => Self::None,
            1 => Self::BulletSmall,
            2 => Self::BulletLarge,
            3 => Self::BulletAp,
            4 => Self::BulletExplode,
            5 => Self::Shotgun,
            6 => Self::ShotgunExplode,
            7 => Self::GrenadeBounce,
            8 => Self::GrenadeExplode,
            9 => Self::RocketExplode,
            10 => Self::ProjectileDud,
            _ => return None,
        })
    }
}

pub const FX_IMPACT_NONFLESH_COUNT: usize = 31;

pub const FX_IMPACT_FLESH_COUNT: usize = 4;

pub const FX_IMPACT_ENTRY_SIZE: usize = 140;

pub const FX_IMPACT_TABLE_ROWS: usize = 15;

pub const FX_SURF_TYPE_FLESH: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxImpactEntry {
    pub nonflesh: [u32; 31],
    pub flesh: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxImpactTable {
    pub entries: u32,
}

pub const FX_IMPACT_EXIT_SURFACE_FLAG: u32 = 1 << 2;

#[inline]
pub fn fx_impact_table_row(impact_type: i32, exit: bool) -> Option<usize> {
    match ImpactType::from_i32(impact_type)? {
        ImpactType::None => None,
        ImpactType::BulletSmall => Some(if exit { 1 } else { 0 }),
        ImpactType::BulletLarge => Some(if exit { 3 } else { 2 }),
        ImpactType::BulletAp => Some(if exit { 9 } else { 8 }),
        ImpactType::BulletExplode => Some(4),
        ImpactType::Shotgun => Some(if exit { 6 } else { 5 }),
        ImpactType::ShotgunExplode => Some(7),
        ImpactType::GrenadeBounce => Some(10),
        ImpactType::GrenadeExplode => Some(12),
        ImpactType::RocketExplode => Some(13),
        ImpactType::ProjectileDud => Some(14),
    }
}

#[inline]
pub fn fx_flesh_effect_index(hit_flags: u32) -> usize {
    let mut idx = (hit_flags & 1) * 2;
    if hit_flags & 2 != 0 {
        idx |= 1;
    }
    idx as usize
}

#[inline]
pub const fn fx_flesh_hit_flags(head: bool, fatal: bool) -> u32 {
    (head as u32) | ((fatal as u32) << 1)
}

#[inline]
pub const fn fx_surface_type_index(surface_flags: u32) -> usize {
    ((surface_flags >> 20) & 0x1f) as usize
}

#[inline]
pub fn fx_impact_entry_cell_offset(
    surface_type: usize,
    flesh_slot: Option<usize>,
) -> Option<usize> {
    if let Some(slot) = flesh_slot {
        if slot >= FX_IMPACT_FLESH_COUNT {
            return None;
        }
        return Some(0x7c + slot * 4);
    }
    if surface_type >= FX_IMPACT_NONFLESH_COUNT {
        return None;
    }
    Some(surface_type * 4)
}
