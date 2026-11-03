use asset_game::LocalizeCatalog;
use std::path::Path;
pub fn load_localize_catalog_t6(path: &Path) -> Result<LocalizeCatalog, String> {
    let image = crate::zone::decoder::open_t6_zone(path).map_err(|e| format!("{e:?}"))?;
    let schema = ext_t6_fastfile::schema::parse().map_err(|e| format!("T6 load plan: {e:?}"))?;
    let (load, walked) = ext_t6_fastfile::load_zone(&schema, &image.bytes, |_, _| true);
    let text = |at: usize, header: &[u8]| {
        let raw = u32::from_le_bytes(header.get(at..at + 4)?.try_into().ok()?);
        let p = (raw != 0 && raw < 0xFFFF_FFFE).then(|| ext_t6_fastfile::Ptr {
            block: ((raw - 1) >> 29) as u8,
            offset: (raw - 1) & 0x1FFF_FFFF,
        })?;
        load.blocks.cstr(p).ok()
    };
    let mut catalog = LocalizeCatalog::default();
    for asset in &load.assets {
        if asset.ty != ext_t6_fastfile::AssetType::LocalizeEntry {
            continue;
        }
        if let (Some(value), Some(name)) = (text(0, &asset.header), text(4, &asset.header)) {
            catalog.insert_bytes(&String::from_utf8_lossy(name), value);
        }
    }
    if let Err(e) = walked {
        diag::info!(
            Zone,
            "localize t6: {} stopped ({} strings): {e:?}",
            path.display(),
            catalog.len()
        );
    }
    Ok(catalog)
}

pub const POLICY: asset_game::LocalizationFamily = asset_game::LocalizationFamily {
    load: load_localize_catalog_t6,
    plan,
    priority: 3,
};
fn plan(
    root: &asset_transport::GamesRoot,
    _zone_ff: &Path,
    _language: Option<&str>,
) -> Result<asset_game::LocalizationSources, String> {
    let anchor = asset_transport::find_zone_file_version(
        root,
        "common_mp",
        ext_t6_fastfile::ZONE_VERSION_PC,
    )?;
    let files = crate::zone::localized::find_t6_localized_zones(&anchor.path, _language)?;
    Ok(asset_game::LocalizationSources {
        files,
        gaps: Vec::new(),
    })
}
