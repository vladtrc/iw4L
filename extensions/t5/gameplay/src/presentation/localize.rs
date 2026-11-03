use crate::zone::decoder::{T5ZoneImageExt, T5ZoneMemory};
use asset_game::LocalizeCatalog;
use asset_transport::open_zone;
use std::path::Path;
pub fn load_localize_catalog_t5(path: &Path) -> Result<LocalizeCatalog, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.t5_header().map_err(|e| e.to_string())?;
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = T5LocalizeSink {
        sound: asset_audio::ZoneSoundCapture::claim_common(path, crate::ID, "localize"),
        ..Default::default()
    };
    let walk = ext_t5_fastfile::load_zone(&mut stream, &mut sink);
    if let Some(sound) = sink.sound.take() {
        sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    }
    if let Err(e) = walk {
        diag::info!(
            Zone,
            "localize t5: {} stopped ({} strings): {e}",
            path.display(),
            sink.catalog.len()
        );
    }
    Ok(sink.catalog)
}

#[derive(Default)]
struct T5LocalizeSink {
    catalog: LocalizeCatalog,
    walked: usize,
    sound: Option<asset_audio::ZoneSoundCapture>,
}

impl ext_t5_fastfile::AssetLinkSink for T5LocalizeSink {
    fn loaded(
        &mut self,
        _s: &ext_t5_fastfile::ZoneStream<'_>,
        _ty: ext_t5_fastfile::AssetType,
        _slot: ext_t5_fastfile::Ptr,
        _insert_slot: Option<ext_t5_fastfile::Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        Ok(())
    }

    fn alias(
        &mut self,
        _ty: ext_t5_fastfile::AssetType,
        _slot: ext_t5_fastfile::Ptr,
        _target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        Ok(())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> ext_t5_fastfile::Result<()> {
        self.catalog.insert_bytes(name, value);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        Ok(())
    }

    crate::sound::forward_t5_sound!();
}

impl ext_t5_fastfile::AssetSink for T5LocalizeSink {
    fn set_script_strings(&mut self, _strings: ext_t5_fastfile::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ext_t5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        ext_t5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        Ok(())
    }
}

pub const POLICY: asset_game::LocalizationFamily = asset_game::LocalizationFamily {
    load: load_localize_catalog_t5,
    plan,
    priority: 1,
};
fn plan(
    root: &asset_transport::GamesRoot,
    _zone_ff: &Path,
    _language: Option<&str>,
) -> Result<asset_game::LocalizationSources, String> {
    let anchor = asset_transport::find_zone_file_version(
        root,
        "common_mp",
        ext_t5_fastfile::ZONE_VERSION_PC,
    )?;
    let files = crate::zone::localized::find_t5_localized_zones(&anchor.path, _language)?;
    Ok(asset_game::LocalizationSources {
        files,
        gaps: Vec::new(),
    })
}
