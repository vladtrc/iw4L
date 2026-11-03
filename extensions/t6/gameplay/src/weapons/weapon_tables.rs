use asset_game::*;
use std::collections::BTreeMap;
pub const POLICY: WeaponTableFamily = WeaponTableFamily {
    supplemental: Some(super::weapon_table_sources::capture),
    attachment,
    stats,
    slot,
    order: AttachmentOrder::Authored,
    point_rank,
    compatibility: AttachmentCompatibility::MutualPartners,
};
fn point_rank(point: &str) -> u8 {
    match point {
        "top" => 1,
        "bottom" => 2,
        "trigger" => 3,
        "muzzle" => 4,
        _ => 0,
    }
}
fn slot(
    row: &WeaponStatsRow,
    _: Option<CacAuthoredCategory>,
    offhand: Option<CacOffhandBucket>,
) -> FamilySlot {
    row.authored_slot
        .unwrap_or_else(|| offhand_family_slot(offhand))
}
fn attachment(table: &CapturedStringTable, row: i32) -> Option<WeaponAttachmentRow> {
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
fn stats(
    table: &CapturedStringTable,
    row: i32,
    known: &BTreeMap<String, AttachmentChoice>,
) -> Option<WeaponStatsRow> {
    let group = table.cell(row, 2);
    let base = table.cell(row, 4);
    if !group.starts_with("weapon_") || base.is_empty() || base == "weapon_null" {
        return None;
    }
    if table.cell(row, 12) == "-1" {
        return None;
    }
    let offered = table
        .cell(row, 8)
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .map(|name| match name.rsplit_once('_') {
            Some((attachment, _)) if !known.contains_key(&name) => attachment.to_owned(),
            _ => name,
        })
        .collect();
    Some(WeaponStatsRow {
        base: base.into(),
        item_group: group.into(),
        caption_key: table.cell(row, 3).into(),
        image: table.cell(row, 6).into(),
        authored_slot: match table.cell(row, 13) {
            "primary" => Some(FamilySlot::Primary),
            "secondary" => Some(FamilySlot::Secondary),
            _ => None,
        },
        offered,
    })
}
