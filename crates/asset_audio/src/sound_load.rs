use std::path::Path;

use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at_observed,
    load_zone,
};

use crate::discover::{GamesRoot, find_runtime_zone, find_zone_file, find_zone_file_version};
use crate::sound_catalog::SoundCatalog;
use crate::zone::{ZoneMemory, open_zone_shared};
use crate::{AssetNamespace, ZoneGame};

pub fn namespace_for_zone(games: &GamesRoot, zone: &str) -> AssetNamespace {
    find_zone_file(games, zone)
        .ok()
        .and_then(|found| crate::zone_game_for_path(&found.path))
        .map(AssetNamespace::from_zone_game)
        .unwrap_or(AssetNamespace::Iw4)
}

struct SoundSink {
    catalog: SoundCatalog,
    walked: usize,
    stopped_at: Option<(usize, &'static str)>,
}

impl AssetSink for SoundSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        self.stopped_at = Some((index, ty.name()));
        load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        self.stopped_at = None;
        Ok(())
    }
}

impl AssetLinkSink for SoundSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.loaded(s, ty, slot, insert_slot)
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.alias(ty, slot, target)
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.capture_loaded_sound(s, header, pcm, data_len)
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.bind_last_loaded_to_sound_file(file)
    }

    fn bind_streamed_sound_file(
        &mut self,
        file: Ptr,
        dir: &str,
        name: &str,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.bind_streamed_sound_file(file, dir, name)
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.capture_sound(s, list, count, head)
    }

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.capture_snd_curve(s, header)
    }
}

pub fn load_sound_catalog(path: &Path) -> Result<SoundCatalog, String> {
    let image = open_zone_shared(path).map_err(|e| e.to_string())?;
    let header = image.header().map_err(|e| e.to_string())?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = SoundSink {
        catalog: SoundCatalog::default(),
        walked: 0,
        stopped_at: None,
    };
    sink.catalog
        .set_capture_zone(crate::ZoneOwner::from_zone_path(path));
    sink.catalog.set_capture_game(crate::ZoneGame::Iw4);
    load_zone(&mut stream, &mut sink).map_err(|e| {
        format!(
            "sound catalog failed after {} assets at {:?}: {e}",
            sink.walked, sink.stopped_at
        )
    })?;
    sink.catalog.resolve_curve_knots();
    sink.catalog.resolve_ent_channels();
    sink.catalog.publish();
    Ok(sink.catalog)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundZoneGap {
    pub namespace: AssetNamespace,

    pub zone: String,
    pub reason: String,
}

#[derive(Default)]
pub struct LoadedSoundBank {
    pub catalog: SoundCatalog,
    pub gaps: Vec<SoundZoneGap>,
}

fn namespace_of_game(game: ZoneGame) -> AssetNamespace {
    AssetNamespace::from_zone_game(game)
}

fn load_sound_catalog_in_lane(path: &Path) -> Result<(AssetNamespace, SoundCatalog), String> {
    let game = crate::zone_game_for_path(path)
        .ok_or_else(|| format!("{} is not a readable IWff envelope", path.display()))?;
    let catalog = match game {
        ZoneGame::Iw4 => load_sound_catalog(path)?,
        ZoneGame::Iw5 => crate::sound_load_iw5::load_sound_catalog_iw5(path)?,
        ZoneGame::T5 => crate::sound_load_t5::load_sound_catalog_t5(path)?,
    };
    Ok((namespace_of_game(game), catalog))
}

pub fn load_mp_sound_bank(games: &GamesRoot, zone: &str) -> Result<LoadedSoundBank, String> {
    let zone_ff = find_zone_file(games, zone)?;
    let mut bank = LoadedSoundBank::default();

    const STARTUP: [&str; 3] = ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"];
    for name in STARTUP {
        match find_runtime_zone(games, &zone_ff.path, name) {
            Ok(found) => match load_sound_catalog(&found.path) {
                Ok(extra) => {
                    diag::info!(
                        Zone,
                        "sound bank: {name} — {} curves {} rawfiles {} aliases",
                        extra.curves.len(),
                        extra.rawfiles.len(),
                        extra.sounds.len()
                    );
                    bank.catalog.absorb_unresolved(extra);
                }
                Err(e) => bank.gap(AssetNamespace::Iw4, name, e),
            },
            Err(e) => diag::info!(Zone, "sound bank: startup {name} missing ({e})"),
        }
    }

    match find_runtime_zone(games, &zone_ff.path, "common_mp") {
        Ok(found) => match load_sound_catalog(&found.path) {
            Ok(extra) => {
                diag::info!(
                    Zone,
                    "sound bank: runtime common_mp — {} aliases {} loaded",
                    extra.sounds.len(),
                    extra.loaded.len()
                );
                bank.catalog.absorb_unresolved(extra);
            }
            Err(e) => bank.gap(AssetNamespace::Iw4, "common_mp", e),
        },
        Err(e) => bank.gap(AssetNamespace::Iw4, "common_mp", e),
    }
    match find_runtime_zone(games, &zone_ff.path, "localized_common_mp") {
        Ok(found) => match load_sound_catalog(&found.path) {
            Ok(extra) => bank.catalog.absorb_unresolved(extra),
            Err(e) => bank.gap(AssetNamespace::Iw4, "localized_common_mp", e),
        },
        Err(e) => {
            bank.gap(AssetNamespace::Iw4, "localized_common_mp", e);
            diag::warn!(
                Zone,
                "sound bank: localized_common_mp missing — player SFX will gap"
            );
        }
    }

    match load_sound_catalog_in_lane(&zone_ff.path) {
        Ok((ns, map_cat)) => {
            diag::info!(
                Zone,
                "sound bank: map zone {zone} ({}) — {} rawfiles, {} aliases",
                ns.as_str(),
                map_cat.rawfiles.len(),
                map_cat.sounds.len()
            );
            match ns {
                AssetNamespace::Iw4 => bank.catalog.absorb_unresolved(map_cat),

                _ => bank.catalog.absorb_missing_aliases_unresolved(map_cat),
            }
        }
        Err(e) => {
            let ns = crate::zone_game_for_path(&zone_ff.path)
                .map_or(AssetNamespace::Iw4, namespace_of_game);
            bank.gap(ns, zone, e);
        }
    }

    absorb_leftover_iw5_sounds(games, &mut bank);
    absorb_leftover_t5_sounds(games, &mut bank);

    bank.catalog.finalize();
    for gap in &bank.gaps {
        diag::warn!(
            Zone,
            "sound bank gap: {} `{}` — {}",
            gap.namespace.as_str(),
            gap.zone,
            gap.reason
        );
    }
    Ok(bank)
}

impl LoadedSoundBank {
    fn gap(&mut self, namespace: AssetNamespace, zone: &str, reason: String) {
        self.gaps.push(SoundZoneGap {
            namespace,
            zone: zone.to_owned(),
            reason,
        });
    }

    pub fn gap_lines(&self) -> Vec<String> {
        self.gaps
            .iter()
            .map(|g| {
                format!(
                    "sound bank gap: {} `{}` — {}",
                    g.namespace.as_str(),
                    g.zone,
                    g.reason
                )
            })
            .collect()
    }
}

fn absorb_leftover_iw5_sounds(games: &GamesRoot, bank: &mut LoadedSoundBank) {
    absorb_leftover_donor(
        games,
        bank,
        AssetNamespace::Iw5,
        fastfile_iw5::ZONE_VERSION_PC,
        crate::sound_load_iw5::load_sound_catalog_iw5,
    );
}

fn absorb_leftover_t5_sounds(games: &GamesRoot, bank: &mut LoadedSoundBank) {
    if let Ok(zone) =
        find_zone_file_version(games, "code_post_gfx_mp", fastfile_t5::ZONE_VERSION_PC)
    {
        match crate::sound_load_t5::load_sound_catalog_t5(&zone.path) {
            Ok(globals) => bank.catalog.absorb_missing_aliases_unresolved(globals),
            Err(error) => bank.gap(AssetNamespace::T5, "code_post_gfx_mp", error),
        }
    }
    absorb_leftover_donor(
        games,
        bank,
        AssetNamespace::T5,
        fastfile_t5::ZONE_VERSION_PC,
        crate::sound_load_t5::load_sound_catalog_t5,
    );
}

fn absorb_leftover_donor(
    games: &GamesRoot,
    bank: &mut LoadedSoundBank,
    namespace: AssetNamespace,
    version: u32,
    walk: fn(&Path) -> Result<SoundCatalog, String>,
) {
    for stem in ["common_mp", "localized_common_mp"] {
        let Ok(donor) = find_zone_file_version(games, stem, version) else {
            continue;
        };
        match walk(&donor.path) {
            Ok(extra) => {
                diag::info!(
                    Zone,
                    "sound bank: leftover {} {stem} — {} aliases {} loaded",
                    namespace.as_str(),
                    extra.sounds.len(),
                    extra.loaded.len()
                );
                bank.catalog.absorb_missing_aliases_unresolved(extra);
            }
            Err(e) => bank.gap(namespace, stem, e),
        }
    }
}
