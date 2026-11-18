use asset_game::*;
pub const POLICY: WeaponTableFamily = WeaponTableFamily {
    supplemental: None,
    attachment: reference_weapon_attachment_row,
    stats: reference_weapon_stats_row,
    slot,
    order: AttachmentOrder::ByName,
    point_rank: |_| 0,
    compatibility: AttachmentCompatibility::ForbiddenPairs,
};
fn slot(
    _: &WeaponStatsRow,
    category: Option<CacAuthoredCategory>,
    offhand: Option<CacOffhandBucket>,
) -> FamilySlot {
    use CacAuthoredCategory as C;
    match category {
        Some(C::Pistol | C::MachinePistol | C::Projectile | C::Special) => FamilySlot::Secondary,
        Some(_) => FamilySlot::Primary,
        None => offhand_family_slot(offhand),
    }
}
