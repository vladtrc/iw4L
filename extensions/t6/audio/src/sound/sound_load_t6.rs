use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::zone::sab::{SoundAssetBank, snd_hash_name};
use asset_audio::{CapturedAlias, CapturedSound, LoadedSoundPcm, SoundCatalog};
use asset_core::{AssetEdge, AssetEdgeReason, FamilyId, ZoneOwner};
use ext_t6_fastfile::{AssetType, Ptr, ZoneLoad};

const SND_BANK_ALIAS_COUNT: usize = 4;
const SND_BANK_ALIAS: usize = 8;
const SND_ALIAS_LIST: u32 = 20;
const SND_ALIAS_LIST_ID: usize = 4;
const SND_ALIAS_LIST_HEAD: usize = 8;
const SND_ALIAS_LIST_COUNT: usize = 12;
const SND_ALIAS: u32 = 96;
const SND_ALIAS_SECONDARY: usize = 12;
const SND_ALIAS_FLAGS0: usize = 24;

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn decode_ptr(raw: u32) -> Option<Ptr> {
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    Some(Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    })
}

pub fn t6_sound_banks(zone: &Path) -> (Vec<SoundAssetBank>, Vec<String>) {
    match zone.parent().and_then(Path::parent).and_then(Path::parent) {
        Some(root) => crate::zone::sab::open_sound_asset_banks(&root.join("sound")),
        None => (
            Vec::new(),
            vec![format!("{}: no install root", zone.display())],
        ),
    }
}

#[derive(Default)]
struct SoundGlobals {
    curves: Vec<asset_audio::CapturedSndCurve>,
    pans: Vec<(String, [f32; 6])>,
    groups: Vec<asset_audio::MixerGroup>,
}

fn capture_globals(loads: &[&ZoneLoad]) -> SoundGlobals {
    let mut globals = SoundGlobals::default();
    for load in loads {
        for asset in load
            .assets
            .iter()
            .filter(|a| a.ty == AssetType::SndDriverGlobals)
        {
            if asset.header.len() < 28 {
                continue;
            }
            let rows = |count_at, pointer_at, size: u32| -> Vec<&[u8]> {
                let Some(base) = decode_ptr(le32(&asset.header, pointer_at)) else {
                    return Vec::new();
                };
                (0..le32(&asset.header, count_at))
                    .map_while(|i| load.blocks.bytes(base.at(i * size), size as usize).ok())
                    .collect()
            };
            let name = |b: &[u8]| {
                String::from_utf8_lossy(&b[..b[..32].iter().position(|&b| b == 0).unwrap_or(32)])
                    .into_owned()
            };
            let float = |b: &[u8], at| f32::from_bits(le32(b, at));
            if globals.groups.is_empty() {
                globals.groups = rows(4, 8, 80)
                    .iter()
                    .map(|b| asset_audio::MixerGroup {
                        parent: le32(b, 68) as i32,
                        attenuation: f32::from(le16(b, 78)) / 65535.0,
                    })
                    .collect();
            }
            if globals.curves.is_empty() {
                globals.curves = rows(12, 16, 100)
                    .iter()
                    .map(|b| asset_audio::CapturedSndCurve {
                        name: name(b),
                        knots: (0..8)
                            .map(|i| (float(b, 36 + i * 8), float(b, 40 + i * 8)))
                            .collect(),
                    })
                    .collect();
            }
            if globals.pans.is_empty() {
                globals.pans = rows(20, 24, 60)
                    .iter()
                    .map(|b| (name(b), std::array::from_fn(|i| float(b, 36 + i * 4))))
                    .collect();
            }
        }
    }
    globals
}

fn sound_rawfile(name: &str) -> bool {
    name.starts_with("rumble/")
        || name == "soundaliases/channels.def"
        || name.ends_with("/soundaliases/channels.def")
        || ((name.starts_with("maps/createfx/") || name.starts_with("maps/mp/"))
            && name.ends_with("_fx.gsc"))
}

pub fn capture_t6_sounds<'n>(
    zone: &Path,
    loads: &[&ZoneLoad],
    banks: &[SoundAssetBank],
    names: impl IntoIterator<Item = &'n str>,
) -> (SoundCatalog, Vec<String>, Vec<String>) {
    let game = FamilyId::new("t6");
    let globals = capture_globals(loads);
    let mut report = Vec::new();
    let mut lists: HashMap<u32, (&ZoneLoad, Ptr, u32)> = HashMap::new();
    for &load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let (Some(count), Some(array)) = (
                bank.header
                    .get(SND_BANK_ALIAS_COUNT..SND_BANK_ALIAS_COUNT + 4),
                bank.header
                    .get(SND_BANK_ALIAS..SND_BANK_ALIAS + 4)
                    .and_then(|b| decode_ptr(le32(b, 0))),
            ) else {
                continue;
            };
            for i in 0..le32(count, 0) {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                if let Some(head) = decode_ptr(le32(list, SND_ALIAS_LIST_HEAD)) {
                    lists.entry(le32(list, SND_ALIAS_LIST_ID)).or_insert((
                        load,
                        head,
                        le32(list, SND_ALIAS_LIST_COUNT),
                    ));
                }
            }
        }
    }

    let mut catalog = SoundCatalog::default();
    catalog.set_capture_zone(ZoneOwner::from_zone_path(zone));
    catalog.set_capture_game(game);
    let mut ignored_rawfiles = 0usize;
    for &load in loads.iter().rev() {
        for asset in load.assets.iter().filter(|a| a.ty == AssetType::RawFile) {
            let rawfile = || -> Result<Option<_>, &str> {
                let header = asset.header.get(..12).ok_or("truncated header")?;
                let name = decode_ptr(le32(header, 0))
                    .and_then(|p| load.blocks.cstr(p).ok())
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .ok_or("invalid name")?
                    .replace('\\', "/")
                    .to_ascii_lowercase();
                if !sound_rawfile(&name) {
                    return Ok(None);
                }
                let len = le32(header, 4) as usize;
                let data = if len == 0 {
                    &[]
                } else {
                    decode_ptr(le32(header, 8))
                        .and_then(|p| load.blocks.bytes(p, len).ok())
                        .ok_or("invalid data span")?
                };
                Ok(Some((name, data)))
            };
            match rawfile() {
                Ok(Some((name, data))) => catalog.ingest_rawfile(&name, data, false),
                Ok(None) => ignored_rawfiles += 1,
                Err(error) => report.push(format!("t6 sound rawfile: {error}")),
            }
        }
    }
    let (rawfile_count, rawfile_bytes) = catalog
        .rawfiles_in(FamilyId::new("t6"))
        .fold((0usize, 0usize), |(count, bytes), (_, data)| {
            (count + 1, bytes + data.len())
        });
    report.push(format!(
        "t6 audio rawfiles: n={rawfile_count} bytes={rawfile_bytes} ignored={ignored_rawfiles}"
    ));
    if !globals.groups.is_empty() {
        catalog.ingest_mixer_groups(FamilyId::new("t6"), globals.groups.clone());
    }
    let mut loaded: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut filled = Vec::new();
    let mut queue: Vec<String> = names.into_iter().map(str::to_owned).collect();
    queue.reverse();
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        let name = name.as_str();
        let Some(&(load, head, count)) = lists.get(&snd_hash_name(name)) else {
            report.push(format!("t6 sound {name}: no alias"));
            continue;
        };
        let mut aliases = Vec::with_capacity(count as usize);
        for k in 0..count {
            let Ok(row) = load
                .blocks
                .bytes(head.at(k * SND_ALIAS), SND_ALIAS as usize)
            else {
                continue;
            };
            let asset = le32(row, 16);
            loaded
                .entry(asset)
                .or_insert_with(|| load_asset(&mut catalog, banks, asset, name, &mut report));
            let loaded_name = format!("t6/{asset:08x}");
            let secondary = decode_ptr(le32(row, SND_ALIAS_SECONDARY))
                .and_then(|p| load.blocks.cstr(p).ok())
                .and_then(|b| std::str::from_utf8(b).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if let Some(secondary) = &secondary
                && seen.insert(secondary.clone())
            {
                queue.push(secondary.clone());
            }
            let flags = le32(row, SND_ALIAS_FLAGS0);
            let curves = le32(row, SND_ALIAS_FLAGS0 + 4);
            let pan = globals.pans.get(usize::from(row[92]));
            aliases.push(CapturedAlias {
                secondary,
                loaded_name: Some(loaded_name.clone()),
                loaded: AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                file_type: Some(1),
                file_name: Some(loaded_name),
                vol_min: f32::from(le16(row, 60)) / 65535.0,
                vol_max: f32::from(le16(row, 62)) / 65535.0,
                pitch_min: f32::from(le16(row, 64)) / 32767.0,
                pitch_max: f32::from(le16(row, 66)) / 32767.0,
                dist_min: f32::from(le16(row, 68)),
                dist_max: f32::from(le16(row, 70)),
                start_delay: i32::from(le16(row, 54)),
                flags: Some(flags),
                looping: Some(flags & 1 != 0),
                volume_falloff: globals.curves.get(((curves >> 2) & 0x3f) as usize).cloned(),
                near_falloff: globals
                    .curves
                    .get(((curves >> 14) & 0x3f) as usize)
                    .cloned(),
                speaker_map: Some(
                    pan.map_or_else(|| format!("t6/pan/{}", row[92]), |(name, _)| name.clone()),
                ),
                surround_pan: pan.map(|(_, gains)| *gains),
                limit_count: Some(row[93]),
                entity_limit_count: Some(row[94]),
                voice_priority: Some(asset_audio::VoicePriority {
                    thresholds: [row[86], row[87]],
                    values: [row[90], row[91]],
                    distance_max: f32::from(le16(row, 70)),
                }),
                probability: f32::from(row[88]) / 255.0,
                ..Default::default()
            });
        }
        if aliases.is_empty() {
            continue;
        }
        catalog.ingest_sound(CapturedSound {
            name: name.to_owned(),
            aliases,
            game: FamilyId::new("t6"),
            zone: catalog.capture_zone_for_ingest(),
        });
        filled.push(name.to_owned());
    }
    catalog.resolve_loaded_edges();
    catalog.publish();
    (catalog, filled, report)
}

fn load_asset(
    catalog: &mut SoundCatalog,
    banks: &[SoundAssetBank],
    id: u32,
    alias: &str,
    report: &mut Vec<String>,
) -> Option<String> {
    let Some((bank, entry)) = banks
        .iter()
        .find_map(|bank| bank.entry(id).map(|entry| (bank, entry)))
    else {
        report.push(format!("t6 sound {alias}: asset {id:08x} in no bank"));
        return None;
    };
    let codec = match entry.format {
        crate::zone::sab::SAB_FORMAT_PCMS16 => asset_audio::ExternalAudioCodec::PcmS16,
        crate::zone::sab::SAB_FORMAT_FLAC => asset_audio::ExternalAudioCodec::Flac,
        format => {
            report.push(format!(
                "t6 sound {alias}: asset {id:08x} has unsupported SAB codec {format}"
            ));
            asset_audio::ExternalAudioCodec::Unsupported(u32::from(format))
        }
    };
    let mut sound = LoadedSoundPcm::captured(
        FamilyId::new("t6"),
        format!("t6/{id:08x}"),
        i32::from(entry.format),
        entry.frame_rate().unwrap_or(0),
        i32::from(entry.channels),
        Vec::new(),
        Vec::new(),
    );
    sound.samples = entry.frame_count;
    sound.zone = catalog.capture_zone_for_ingest();
    sound.external_media = Some(asset_audio::ExternalAudioSource {
        path: bank.path().to_owned(),
        offset: u64::from(entry.offset),
        size: u64::from(entry.size),
        frames: entry.frame_count,
        rate: entry.frame_rate().unwrap_or(0),
        channels: u16::from(entry.channels),
        codec,
    });
    let name = sound.name.clone();
    catalog.ingest_loaded(sound);
    Some(name)
}

pub fn t6_sound_names(loads: &[&ZoneLoad]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let Some(count) = bank.header.get(4..8).map(|b| le32(b, 0)) else {
                continue;
            };
            let Some(array) = bank.header.get(8..12).and_then(|b| decode_ptr(le32(b, 0))) else {
                continue;
            };
            for i in 0..count {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                let Some(name) = decode_ptr(le32(list, 0))
                    .and_then(|p| load.blocks.cstr(p).ok())
                    .and_then(|b| std::str::from_utf8(b).ok())
                else {
                    continue;
                };
                if snd_hash_name(name) == le32(list, 4) {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    names.into_iter().collect()
}
