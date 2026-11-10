use asset_core::FamilyId;
use asset_transport::{LocalizedZones, ZoneFile};
use std::path::Path;

pub fn find_t6_localized_zones(
    anchor: &Path,
    language: Option<&str>,
) -> Result<Vec<ZoneFile>, String> {
    let zones = LocalizedZones::find(anchor, language, FamilyId::new("t6"))?;
    Ok(["patch_mp", "ui_mp", "code_post_gfx_mp"]
        .into_iter()
        .filter_map(|stem| zones.zone(stem))
        .collect())
}
