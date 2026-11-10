use asset_game::*;
use std::collections::BTreeMap;
pub const POLICY: WeaponTableFamily = WeaponTableFamily {
    supplemental: None,
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
    _known: &BTreeMap<String, AttachmentChoice>,
) -> Option<WeaponStatsRow> {
    let group = table.cell(row, 2);
    let base = table.cell(row, 4);
    if !group.starts_with("weapon_") || base.is_empty() || base == "weapon_null" {
        return None;
    }
    let offered = table
        .cell(row, 8)
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
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
