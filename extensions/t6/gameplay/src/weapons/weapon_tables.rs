use asset_game::*;
use std::collections::BTreeMap;
pub const POLICY: WeaponTableFamily = WeaponTableFamily {
    supplemental: Some(super::weapon_table_sources::capture),
    attachment: black_ops_attachment,
    stats,
    slot: black_ops_slot,
    order: AttachmentOrder::Authored,
    point_rank: black_ops_point_rank,
    compatibility: AttachmentCompatibility::MutualPartners,
};
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
