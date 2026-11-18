use asset_core::FamilyId;
use asset_transport::{LocalizedZones, ZoneFile};
use std::path::Path;

pub fn find_t5_localized_zones(
    anchor: &Path,
    language: Option<&str>,
) -> Result<Vec<ZoneFile>, String> {
    let zones = LocalizedZones::find(anchor, language, FamilyId::new("t5"))?;
    Ok(["code_post_gfx_mp", "common_mp", "ui_mp"]
        .into_iter()
        .filter_map(|stem| zones.zone(stem))
        .collect())
}

pub fn find_t5_localized_zone(
    anchor: &Path,
    language: Option<&str>,
    stem: &str,
) -> Result<Option<ZoneFile>, String> {
    Ok(LocalizedZones::find(anchor, language, FamilyId::new("t5"))?.zone(stem))
}
