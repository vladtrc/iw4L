use crate::zone::decoder::T5ZoneImageExt;
use asset_transport::open_zone_shared;
use std::collections::HashMap;
use std::path::Path;

use crate::zone::decoder::T5ZoneMemory;
use asset_audio::{
    CapturedAlias, CapturedSndCurve, CapturedSound, LoadedSoundPcm, MSS_PCM, SoundCatalog,
};
use asset_core::{FamilyId, ZoneOwner};
use ext_t5_fastfile::size as sz;
use ext_t5_fastfile::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZonePtr, ZoneStream,
    load_asset_at_observed, load_zone,
};

#[derive(Default)]
pub struct T5SoundCapture {
    catalog: SoundCatalog,
    last_loaded_name: Option<String>,
    file_to_loaded: HashMap<(u8, u32), String>,
    file_to_streamed: HashMap<(u8, u32), (String, String)>,
}

impl T5SoundCapture {
    pub fn for_zone(path: &Path) -> Self {
        let mut capture = Self::default();
        capture
            .catalog
            .set_capture_zone(ZoneOwner::from_zone_path(path));
        capture.catalog.set_capture_game(FamilyId::new("t5"));
        capture
    }

    pub fn finish(mut self) -> SoundCatalog {
        self.catalog.resolve_loaded_edges();
        self.catalog.publish();
        self.catalog
    }
}

struct T5SoundSink {
    capture: T5SoundCapture,
    walked: usize,
    stopped_at: Option<(usize, &'static str)>,
}

impl AssetSink for T5SoundSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.stopped_at = Some((index, ty.name()));
        load_asset_at_observed(s, ty, slot, &mut self.capture)?;
        self.walked += 1;
        self.stopped_at = None;
        Ok(())
    }
}

impl AssetLinkSink for T5SoundCapture {
    fn capture_snd_curves(
        &mut self,
        s: &ZoneStream<'_>,
        rows: Ptr,
        count: usize,
    ) -> ext_t5_fastfile::Result<()> {
        for i in 0..count {
            let row = rows.at(i * 100);
            let mut knots = Vec::with_capacity(8);
            for n in 0..8 {
                knots.push((s.f32_at(row, 36 + n * 8)?, s.f32_at(row, 40 + n * 8)?));
            }
            let name = format!("t5/curve/{i}");
            self.catalog.ingest_curve(CapturedSndCurve { name, knots });
        }
        Ok(())
    }

    fn capture_snd_groups(
        &mut self,
        s: &ZoneStream<'_>,
        rows: Ptr,
        count: usize,
    ) -> ext_t5_fastfile::Result<()> {
        let mut groups = Vec::with_capacity(count);
        for i in 0..count {
            let row = rows.at(i * 80);
            groups.push(asset_audio::MixerGroup {
                parent: s.i32_at(row, 68)?,
                attenuation: f32::from(s.u16_at(row, 78)?) / 65535.0,
            });
        }
        self.catalog
            .ingest_mixer_groups(FamilyId::new("t5"), groups);
        Ok(())
    }

    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> ext_t5_fastfile::Result<()> {
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_t5_fastfile::Result<()> {
        self.catalog.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> ext_t5_fastfile::Result<()> {
        let name = name_at(s, header, sz::LOADED_SOUND_NAME_OFF).unwrap_or(String::new());
        let asset = header.at(sz::LOADED_SOUND_ASSET_OFF);
        let t5_format = s.i32_at(asset, sz::SND_ASSET_FORMAT_OFF).unwrap_or(-1);
        let rate = s.u32_at(asset, sz::SND_ASSET_FRAME_RATE_OFF).unwrap_or(0);
        let channels = s
            .u32_at(asset, sz::SND_ASSET_CHANNEL_COUNT_OFF)
            .unwrap_or(0) as i32;
        let samples = s.u32_at(asset, sz::SND_ASSET_FRAME_COUNT_OFF).unwrap_or(0);
        let block_size = s.u32_at(asset, sz::SND_ASSET_BLOCK_SIZE_OFF).unwrap_or(0);
        let pcm_bytes = if data_len == 0 {
            Vec::new()
        } else {
            s.slice_at(pcm, 0, data_len)
                .map(|b| b.to_vec())
                .unwrap_or(Vec::new())
        };
        if name.is_empty() {
            self.catalog.capture_gaps += 1;
            self.last_loaded_name = None;
            return Ok(());
        }
        self.last_loaded_name = Some(name.clone());
        if pcm_bytes.is_empty() {
            return Ok(());
        }

        let (format, bits) = if t5_format == sz::SND_ASSET_FORMAT_PCMS16 {
            (MSS_PCM, 16)
        } else {
            (t5_format, 16)
        };
        let seek_table = if t5_format == sz::SND_ASSET_FORMAT_WMA {
            read_seek_table(s, asset)
        } else {
            Vec::new()
        };
        self.catalog.ingest_loaded(LoadedSoundPcm {
            name,
            game: FamilyId::new("t5"),
            format,
            rate,
            bits,
            channels,
            samples,
            block_size,
            pcm: pcm_bytes.into(),
            zone: self.catalog.capture_zone_for_ingest(),
            seek_table,
            external_media: None,
        });
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> ext_t5_fastfile::Result<()> {
        if let Some(name) = self.last_loaded_name.clone() {
            self.file_to_loaded.insert(file_key(file), name);
        } else {
            self.catalog.capture_gaps += 1;
        }
        Ok(())
    }

    fn bind_streamed_sound_file(
        &mut self,
        file: Ptr,
        dir: &str,
        name: &str,
    ) -> ext_t5_fastfile::Result<()> {
        self.file_to_streamed
            .insert(file_key(file), (dir.to_owned(), name.to_owned()));
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        let name = name_at(s, list, sz::SND_ALIAS_LIST_NAME_OFF).unwrap_or(String::new());
        let Some(arr) = head else {
            if count > 0 {
                self.catalog.capture_gaps += 1;
            }
            return Ok(());
        };
        let mut aliases = Vec::with_capacity(count);
        for i in 0..count {
            let row = arr.at(i * sz::SND_ALIAS);
            aliases.push(self.capture_alias_row(s, row));
        }
        if name.is_empty() {
            self.catalog.capture_gaps += 1;
            return Ok(());
        }
        self.catalog.ingest_sound(CapturedSound {
            name,
            aliases,
            game: FamilyId::new("t5"),
            zone: self.catalog.capture_zone_for_ingest(),
        });
        Ok(())
    }
}

impl T5SoundCapture {
    fn capture_alias_row(&self, s: &ZoneStream<'_>, row: Ptr) -> CapturedAlias {
        let alias_name = name_at(s, row, sz::SND_ALIAS_NAME_OFF).unwrap_or(String::new());
        let (
            loaded_name,
            streamed,
            file_type,
            file_exists,
            file_name,
            file_u,
            file_u_ptr,
            file_u_deref,
        ) = match s.ptr_at(row, sz::SND_ALIAS_SOUND_FILE_OFF).ok() {
            Some(ZonePtr::Offset(p)) => {
                let file = s.resolve_alias(p);
                let key = file_key(file);
                let loaded = self.file_to_loaded.get(&key).cloned();
                let streamed = self.file_to_streamed.get(&key).cloned();
                let inspected = inspect_sound_file(s, file);
                let u = s.ptr_at(file, sz::SOUND_FILE_UNION_OFF).ok();
                let file_u_deref = match u {
                    Some(ZonePtr::Offset(q)) => offset_deref_kind(s, q),
                    _ => None,
                };
                let loaded = if file_u_deref == Some("following")
                    || file_u_deref == Some("insert")
                    || matches!(u, Some(ZonePtr::Following) | Some(ZonePtr::Insert))
                {
                    loaded
                } else {
                    loaded.or(inspected.loaded_name.clone())
                };
                let streamed = streamed.or(inspected.streamed);
                let file_name = loaded.clone().or(inspected.file_name.clone()).or_else(|| {
                    streamed.as_ref().map(|(dir, name)| {
                        if dir.is_empty() {
                            name.clone()
                        } else {
                            format!("{dir}/{name}")
                        }
                    })
                });
                (
                    loaded,
                    streamed,
                    inspected.file_type,
                    inspected.file_exists,
                    file_name,
                    zone_ptr_kind(u),
                    zone_ptr_label(u),
                    file_u_deref,
                )
            }
            Some(ZonePtr::Null) => (None, None, None, None, None, Some("null"), None, None),
            other => (
                None,
                None,
                Some(0),
                None,
                None,
                zone_ptr_kind(other),
                zone_ptr_label(other),
                None,
            ),
        };
        let sound_file = s.ptr_at(row, sz::SND_ALIAS_SOUND_FILE_OFF).ok();
        let loaded = capture_loaded_edge(
            sound_file,
            file_type,
            file_u,
            file_u_deref,
            loaded_name.as_deref(),
        );
        let vol_min = u16_unit(s, row, sz::SND_ALIAS_VOL_MIN_OFF, 65535.0);
        let vol_max = u16_unit(s, row, sz::SND_ALIAS_VOL_MAX_OFF, 65535.0);
        let pitch_min = u16_unit(s, row, sz::SND_ALIAS_PITCH_MIN_OFF, 32767.0);
        let pitch_max = u16_unit(s, row, sz::SND_ALIAS_PITCH_MAX_OFF, 32767.0);
        let dist_min = s
            .u16_at(row, sz::SND_ALIAS_DIST_MIN_OFF)
            .map(|v| v as f32)
            .unwrap_or(0.0);
        let dist_max = s
            .u16_at(row, sz::SND_ALIAS_DIST_MAX_OFF)
            .map(|v| v as f32)
            .unwrap_or(0.0);
        let envelop_min = s
            .u16_at(row, sz::SND_ALIAS_ENVELOP_MIN_OFF)
            .map(|v| v as f32)
            .unwrap_or(0.0);
        let envelop_max = s
            .u16_at(row, sz::SND_ALIAS_ENVELOP_MAX_OFF)
            .map(|v| v as f32)
            .unwrap_or(0.0);
        let envelop_percentage = u16_unit(s, row, sz::SND_ALIAS_ENVELOP_PERCENTAGE_OFF, 65535.0);
        let probability = match s.u8_at(row, sz::SND_ALIAS_PROBABILITY_OFF).ok() {
            Some(0) | None => 1.0,
            Some(raw) => raw as f32 / 255.0,
        };
        CapturedAlias {
            alias_name,
            subtitle: optional_name(s, row, sz::SND_ALIAS_SUBTITLE_OFF),
            secondary: optional_name(s, row, sz::SND_ALIAS_SECONDARY_OFF),
            chain: None,
            mixer_group: None,
            loaded_name,
            loaded,
            loaded_binding_origin: asset_audio::LoadedBindingOrigin::Unresolved,
            streamed: streamed.map(normalize_streamed),
            file_type,
            file_exists,
            file_name,
            file_u,
            file_u_ptr,
            file_u_deref,
            sequence: 0,
            vol_min,
            vol_max,
            vol_mod_index: None,
            pitch_min,
            pitch_max,
            dist_min,
            dist_max,
            velocity_min: 0.0,
            flags: s.u32_at(row, sz::SND_ALIAS_FLAGS_OFF).ok(),
            looping: s
                .u32_at(row, sz::SND_ALIAS_FLAGS_OFF)
                .ok()
                .map(|flags| flags & 1 != 0),
            channel_id: None,
            slave_percentage: 0.0,
            probability,
            lfe_percentage: 0.0,
            center_percentage: 0.0,
            start_delay: s
                .u16_at(row, sz::SND_ALIAS_START_DELAY_OFF)
                .map(i32::from)
                .unwrap_or(0),
            volume_falloff: None,

            distance_curve_names: s
                .u8_at(row, 76)
                .ok()
                .zip(s.u8_at(row, 78).ok())
                .map(|(dry, near)| [format!("t5/curve/{dry}"), format!("t5/curve/{near}")]),
            near_falloff: None,
            voice_priority: Some(asset_audio::VoicePriority {
                thresholds: [
                    s.u8_at(row, sz::SND_ALIAS_PRIORITY_MIN_THRESHOLD_OFF)
                        .unwrap_or(0),
                    s.u8_at(row, sz::SND_ALIAS_PRIORITY_MAX_THRESHOLD_OFF)
                        .unwrap_or(0),
                ],
                values: [
                    s.u8_at(row, sz::SND_ALIAS_PRIORITY_MIN_OFF).unwrap_or(0),
                    s.u8_at(row, sz::SND_ALIAS_PRIORITY_MAX_OFF).unwrap_or(0),
                ],
                distance_max: s
                    .u16_at(row, sz::SND_ALIAS_PRIORITY_DISTANCE_OFF)
                    .map(f32::from)
                    .unwrap_or(0.0),
            }),
            envelop_min,
            envelop_max,
            envelop_percentage,
            speaker_map: None,
            stereo_speaker_gains: None,
            surround_pan: None,
            limit_count: s.u8_at(row, sz::SND_ALIAS_LIMIT_COUNT_OFF).ok(),
            entity_limit_count: s.u8_at(row, sz::SND_ALIAS_ENTITY_LIMIT_COUNT_OFF).ok(),
        }
    }
}

struct InspectedSoundFile {
    file_type: Option<u8>,
    file_exists: Option<u8>,
    file_name: Option<String>,
    loaded_name: Option<String>,
    streamed: Option<(String, String)>,
}

fn inspect_sound_file(s: &ZoneStream<'_>, file: Ptr) -> InspectedSoundFile {
    let file_type = s.u8_at(file, sz::SOUND_FILE_TYPE_OFF).ok();
    let file_exists = s.u8_at(file, sz::SOUND_FILE_EXISTS_OFF).ok();
    match file_type {
        Some(1) => {
            let loaded_name = match s.ptr_at(file, sz::SOUND_FILE_UNION_OFF).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let target = s.resolve_alias(p);
                    match s.u32_at(target, 0).ok().map(ZonePtr::decode) {
                        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => None,
                        _ => name_at(s, target, sz::LOADED_SOUND_NAME_OFF),
                    }
                }
                _ => None,
            };
            InspectedSoundFile {
                file_type,
                file_exists,
                file_name: loaded_name.clone(),
                loaded_name,
                streamed: None,
            }
        }
        Some(2) | Some(3) => {
            let name = name_at(s, file, sz::SOUND_FILE_UNION_OFF)
                .or_else(|| match s.ptr_at(file, sz::SOUND_FILE_UNION_OFF).ok() {
                    Some(ZonePtr::Offset(p)) => {
                        name_at(s, s.resolve_alias(p), sz::STREAMED_SOUND_FILENAME_OFF)
                    }
                    _ => None,
                })
                .unwrap_or(String::new());
            let streamed = (!name.is_empty()).then(|| (String::new(), name.clone()));
            InspectedSoundFile {
                file_type,
                file_exists,
                loaded_name: None,
                streamed,
                file_name: (!name.is_empty()).then_some(name),
            }
        }
        _ => InspectedSoundFile {
            file_type,
            file_exists,
            loaded_name: None,
            streamed: None,
            file_name: None,
        },
    }
}

fn capture_loaded_edge(
    sound_file: Option<ZonePtr>,
    file_type: Option<u8>,
    file_u: Option<&str>,
    file_u_deref: Option<&str>,
    loaded_name: Option<&str>,
) -> asset_audio::LoadedSoundEdge {
    use asset_core::{AssetEdge, AssetEdgeReason};
    match sound_file {
        None | Some(ZonePtr::Null) => AssetEdge::Absent,
        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => {
            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
        }
        Some(ZonePtr::Offset(_)) => match file_type {
            Some(2) | Some(3) => AssetEdge::Absent,
            Some(1) => {
                if loaded_name.is_some_and(|n| {
                    n.rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(n)
                        .eq_ignore_ascii_case("null.wav")
                }) {
                    return AssetEdge::Absent;
                }
                match file_u {
                    Some("null") | None => AssetEdge::Absent,
                    Some("following") | Some("insert") => {
                        if loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        }
                    }
                    Some("offset") => {
                        let stub =
                            file_u_deref == Some("following") || file_u_deref == Some("insert");
                        if stub && !loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        }
                    }
                    _ => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                }
            }
            _ => AssetEdge::Absent,
        },
    }
}

fn name_at(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(q) => s
            .cstr(s.resolve_alias(q))
            .ok()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn optional_name(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    name_at(s, parent, field)
}

fn u16_unit(s: &ZoneStream<'_>, row: Ptr, off: usize, denom: f32) -> f32 {
    s.u16_at(row, off).map(|v| v as f32 / denom).unwrap_or(0.0)
}

fn read_seek_table(s: &ZoneStream<'_>, asset: Ptr) -> Vec<u32> {
    let count = s
        .u32_at(asset, sz::SND_ASSET_SEEK_TABLE_COUNT_OFF)
        .unwrap_or(0) as usize;
    if count == 0 {
        return Vec::new();
    }
    let Ok(ZonePtr::Offset(p)) = s.ptr_at(asset, sz::SND_ASSET_SEEK_TABLE_OFF) else {
        return Vec::new();
    };
    let ptr = s.resolve_alias(p);
    let Ok(bytes) = s.slice_at(ptr, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn file_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

fn zone_ptr_kind(p: Option<ZonePtr>) -> Option<&'static str> {
    Some(match p? {
        ZonePtr::Null => "null",
        ZonePtr::Offset(_) => "offset",
        ZonePtr::Following => "following",
        ZonePtr::Insert => "insert",
    })
}

fn zone_ptr_label(p: Option<ZonePtr>) -> Option<String> {
    match p? {
        ZonePtr::Offset(q) => Some(format!("{}:{:#x}", q.block, q.offset)),
        _ => None,
    }
}

fn offset_deref_kind(s: &ZoneStream<'_>, q: Ptr) -> Option<&'static str> {
    match s.u32_at(s.resolve_alias(q), 0).ok().map(ZonePtr::decode) {
        Some(ZonePtr::Following) => Some("following"),
        Some(ZonePtr::Insert) => Some("insert"),
        Some(ZonePtr::Null) => Some("null"),
        Some(ZonePtr::Offset(_)) => Some("offset"),
        None => None,
    }
}

pub fn load_sound_catalog_t5(path: &Path) -> Result<SoundCatalog, String> {
    let image = open_zone_shared(path).map_err(|e| e.to_string())?;
    if image.game != FamilyId::new("t5") {
        return Err(format!(
            "load_sound_catalog_t5: {} is {:?}, not T5",
            path.display(),
            image.game
        ));
    }
    let header = image.t5_header().map_err(|e| e.to_string())?;
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = T5SoundSink {
        capture: T5SoundCapture::for_zone(path),
        walked: 0,
        stopped_at: None,
    };
    if let Err(e) = load_zone(&mut stream, &mut sink) {
        if let Some((idx, name)) = sink.stopped_at {
            diag::info!(
                Zone,
                "t5 sound catalog: stopped at asset {idx} ({name}) after {} walked: {e}",
                sink.walked
            );
        }
    }
    Ok(sink.capture.finish())
}

fn normalize_streamed((dir, name): (String, String)) -> (String, String) {
    if !dir.is_empty() {
        return (dir, name);
    }
    let path = name.replace('\\', "/");
    let relative = path.strip_prefix("sound/").or_else(|| {
        let (_, rest) = path.split_once('/')?;
        rest.strip_prefix("sound/")
    });
    if let Some((directory, file)) = relative.and_then(|relative| relative.rsplit_once('/')) {
        return (directory.to_owned(), file.to_owned());
    }
    (dir, name)
}

impl asset_audio::NativeSoundCapture for T5SoundCapture {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn catalog_mut(&mut self) -> Option<&mut SoundCatalog> {
        Some(&mut self.catalog)
    }
    fn finish(self: Box<Self>) -> Result<SoundCatalog, String> {
        Ok((*self).finish())
    }
}
