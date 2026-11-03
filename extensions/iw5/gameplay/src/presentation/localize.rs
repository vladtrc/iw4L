use crate::zone::decoder::{Iw5ZoneImageExt, Iw5ZoneMemory};
use asset_game::LocalizeCatalog;
use asset_transport::open_zone;
use std::path::Path;
pub fn load_localize_catalog_iw5(path: &Path) -> Result<LocalizeCatalog, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.iw5_header().map_err(|e| e.to_string())?;
    let mut memory = Iw5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = Iw5LocalizeSink {
        sound: asset_audio::ZoneSoundCapture::claim_common(path, crate::ID, "localize"),
        ..Default::default()
    };
    let walk = ext_iw5_fastfile::load_zone(&mut stream, &mut sink);
    if let Some(sound) = sink.sound.take() {
        sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    }
    if let Err(e) = walk {
        diag::info!(
            Zone,
            "localize iw5: {} stopped ({} strings): {e}",
            path.display(),
            sink.catalog.len()
        );
    }
    Ok(sink.catalog)
}

#[derive(Default)]
struct Iw5LocalizeSink {
    catalog: LocalizeCatalog,
    walked: usize,
    sound: Option<asset_audio::ZoneSoundCapture>,
}

impl ext_iw5_fastfile::AssetLinkSink for Iw5LocalizeSink {
    fn loaded(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result = ext_iw5_fastfile::AssetLinkSink::loaded(capture, s, ty, slot, insert_slot);
            sound.guard(result);
        }
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result = ext_iw5_fastfile::AssetLinkSink::alias(capture, ty, slot, target);
            sound.guard(result);
        }
        Ok(())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> ext_iw5_fastfile::Result<()> {
        self.catalog.insert_bytes(name, value);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        Ok(())
    }

    crate::sound::forward_iw5_sound!();
}

impl ext_iw5_fastfile::AssetSink for Iw5LocalizeSink {
    fn set_script_strings(&mut self, _strings: ext_iw5_fastfile::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ext_iw5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        ext_iw5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        Ok(())
    }
}

pub const POLICY: asset_game::LocalizationFamily = asset_game::LocalizationFamily {
    load: load_localize_catalog_iw5,
    plan,
    priority: 2,
};
fn plan(
    root: &asset_transport::GamesRoot,
    _zone_ff: &Path,
    _language: Option<&str>,
) -> Result<asset_game::LocalizationSources, String> {
    let mut plan = asset_game::LocalizationSources::default();
    for name in [
        "localized_code_post_gfx_mp",
        "localized_common_mp",
        "localized_ui_mp",
    ] {
        match asset_transport::find_zone_file_version(root, name, ext_iw5_fastfile::ZONE_VERSION_PC)
        {
            Ok(found) => plan.files.push(found),
            Err(error) => plan.gaps.push(format!(
                "localize: {name} missing ({error}) — its keys will report as gaps"
            )),
        }
    }
    Ok(plan)
}
