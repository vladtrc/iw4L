use crate::{
    AttachmentChoice, CacAuthoredCategory, CacOffhandBucket, CapturedStringTable, FamilySlot,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentTableRole {
    Offered,
    Service,
    Cosmetic,
}
#[derive(Clone, Debug)]
pub struct WeaponAttachmentRow {
    pub choice: AttachmentChoice,
    pub role: AttachmentTableRole,
    pub partners: Option<Vec<String>>,
}
#[derive(Clone, Debug)]
pub struct WeaponStatsRow {
    pub base: String,
    pub item_group: String,
    pub caption_key: String,
    pub image: String,
    pub authored_slot: Option<FamilySlot>,
    pub offered: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentOrder {
    ByName,
    Authored,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentCompatibility {
    ForbiddenPairs,
    MutualPartners,
}
#[derive(Default, Debug)]
pub struct WeaponSupplementalTables {
    pub tables: Vec<CapturedStringTable>,
    pub report: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct WeaponTableFamily {
    pub supplemental: Option<fn(&asset_transport::GamesRoot) -> WeaponSupplementalTables>,
    pub attachment: fn(&CapturedStringTable, i32) -> Option<WeaponAttachmentRow>,
    pub stats: fn(
        &CapturedStringTable,
        i32,
        &BTreeMap<String, AttachmentChoice>,
    ) -> Option<WeaponStatsRow>,
    pub slot:
        fn(&WeaponStatsRow, Option<CacAuthoredCategory>, Option<CacOffhandBucket>) -> FamilySlot,
    pub order: AttachmentOrder,
    pub point_rank: fn(&str) -> u8,
    pub compatibility: AttachmentCompatibility,
}

pub fn offhand_family_slot(offhand: Option<CacOffhandBucket>) -> FamilySlot {
    match offhand {
        Some(CacOffhandBucket::Lethal) => FamilySlot::Lethal,
        Some(CacOffhandBucket::Tactical) => FamilySlot::Tactical,
        None => FamilySlot::Other,
    }
}

pub fn reference_weapon_attachment_row(
    table: &CapturedStringTable,
    row: i32,
) -> Option<WeaponAttachmentRow> {
    let name = table.cell(row, 4).to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let group = table.cell(row, 2);
    Some(WeaponAttachmentRow {
        choice: AttachmentChoice {
            name,
            point: group.to_owned(),
            caption_key: table.cell(row, 3).to_owned(),
            icon: table.cell(row, 6).to_owned(),
            desc_key: table.cell(row, 7).to_owned(),
            order: table.cell(row, 9).parse().unwrap_or(row as u32),
        },
        role: if group == "none" {
            AttachmentTableRole::Service
        } else {
            AttachmentTableRole::Offered
        },
        partners: None,
    })
}
pub fn reference_weapon_stats_row(
    table: &CapturedStringTable,
    row: i32,
    _: &BTreeMap<String, AttachmentChoice>,
) -> Option<WeaponStatsRow> {
    let group = table.cell(row, 2);
    let base = table.cell(row, 4);
    if !group.starts_with("weapon_") || base.is_empty() || base == "weapon_null" {
        return None;
    }
    Some(WeaponStatsRow {
        base: base.into(),
        item_group: group.into(),
        caption_key: table.cell(row, 3).into(),
        image: table.cell(row, 6).into(),
        authored_slot: None,
        offered: (11..=21)
            .map(|col| table.cell(row, col).to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect(),
    })
}
fn reference_slot(
    _: &WeaponStatsRow,
    category: Option<CacAuthoredCategory>,
    offhand: Option<CacOffhandBucket>,
) -> FamilySlot {
    use CacAuthoredCategory as C;
    match category {
        Some(C::Pistol | C::MachinePistol | C::Projectile | C::Special | C::Shotgun) => {
            FamilySlot::Secondary
        }
        Some(_) => FamilySlot::Primary,
        None => offhand_family_slot(offhand),
    }
}
pub const REFERENCE_WEAPON_TABLES: WeaponTableFamily = WeaponTableFamily {
    supplemental: None,
    attachment: reference_weapon_attachment_row,
    stats: reference_weapon_stats_row,
    slot: reference_slot,
    order: AttachmentOrder::ByName,
    point_rank: |_| 0,
    compatibility: AttachmentCompatibility::ForbiddenPairs,
};

pub fn black_ops_point_rank(point: &str) -> u8 {
    match point {
        "top" => 1,
        "bottom" => 2,
        "trigger" => 3,
        "muzzle" => 4,
        _ => 0,
    }
}
pub fn black_ops_slot(
    row: &WeaponStatsRow,
    _: Option<CacAuthoredCategory>,
    offhand: Option<CacOffhandBucket>,
) -> FamilySlot {
    row.authored_slot
        .unwrap_or_else(|| offhand_family_slot(offhand))
}
pub fn black_ops_attachment(table: &CapturedStringTable, row: i32) -> Option<WeaponAttachmentRow> {
    let name = table.cell(row, 4).to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let kind = table.cell(row, 2);
    let role = if kind == "weaponoption" {
        AttachmentTableRole::Cosmetic
    } else if kind != "attachment" || name == "none" {
        AttachmentTableRole::Service
    } else {
        AttachmentTableRole::Offered
    };
    Some(WeaponAttachmentRow {
        choice: AttachmentChoice {
            name,
            point: table.cell(row, 1).into(),
            caption_key: table.cell(row, 3).into(),
            icon: table.cell(row, 6).into(),
            desc_key: table.cell(row, 7).into(),
            order: table.cell(row, 9).parse().unwrap_or(row as u32),
        },
        role,
        partners: Some(
            table
                .cell(row, 11)
                .split_ascii_whitespace()
                .map(str::to_ascii_lowercase)
                .collect(),
        ),
    })
}
