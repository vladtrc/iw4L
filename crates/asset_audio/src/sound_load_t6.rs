//! T6 aliases, read from the `SndBank` assets of a finished zone load and the
//! sound asset banks in the install's `sound/` directory.
//!
//! Only the aliases asked for are read — a T6 bank holds thousands. They land
//! in a catalog captured as IW4: T6 weapons resolve their content in the IW4
//! namespace (see `AssetNamespace::content`), and T6 alias names do not
//! collide with IW4's.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::sound_catalog::{CapturedAlias, CapturedSound, LoadedSoundPcm, MSS_PCM, SoundCatalog};
use crate::{ZoneGame, ZoneOwner};
use asset_transport::{SAB_FORMAT_FLAC, SAB_FORMAT_PCMS16, SoundAssetBank, snd_hash_name};
use fastfile_t6::{AssetType, Ptr, ZoneLoad};

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

/// The sound asset banks of the T6 install a zone belongs to
/// (`<root>/zone/<dir>/x.ff` → `<root>/sound`).
pub fn t6_sound_banks(zone: &Path) -> (Vec<SoundAssetBank>, Vec<String>) {
    match zone.parent().and_then(Path::parent).and_then(Path::parent) {
        Some(root) => asset_transport::open_sound_asset_banks(&root.join("sound")),
        None => (
            Vec::new(),
            vec![format!("{}: no install root", zone.display())],
        ),
    }
}

/// `names` as a catalog, reading aliases from the `SndBank`s of `loads` (the
/// first that defines a name wins) and following each alias's secondary.
/// Returns it with the names it could fill and a report line per gap.
pub fn capture_t6_sounds<'n>(
    zone: &Path,
    loads: &[&ZoneLoad],
    banks: &[SoundAssetBank],
    names: impl IntoIterator<Item = &'n str>,
) -> (SoundCatalog, Vec<String>, Vec<String>) {
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
    catalog.set_capture_game(ZoneGame::Iw4);
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
            let loaded_name = loaded
                .entry(asset)
                .or_insert_with(|| load_asset(&mut catalog, banks, asset, name, &mut report))
                .clone();
            let Some(loaded_name) = loaded_name else {
                continue;
            };
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
            aliases.push(CapturedAlias {
                alias_name: name.to_owned(),
                secondary,
                loaded_name: Some(loaded_name.clone()),
                file_type: Some(1),
                file_name: Some(loaded_name),
                vol_min: f32::from(le16(row, 60)) / 65535.0,
                vol_max: f32::from(le16(row, 62)) / 65535.0,
                pitch_min: f32::from(le16(row, 64)) / 32767.0,
                pitch_max: f32::from(le16(row, 66)) / 32767.0,
                dist_min: f32::from(le16(row, 68)),
                dist_max: f32::from(le16(row, 70)),
                start_delay: i32::from(le16(row, 54)),
                // `flags0` bit 0; the rest of T6's flag words is not IW4's.
                looping: Some(le32(row, SND_ALIAS_FLAGS0) & 1 != 0),
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
            ..Default::default()
        });
        filled.push(name.to_owned());
    }
    catalog.resolve_loaded_edges();
    catalog.publish();
    (catalog, filled, report)
}

/// Asset `id` out of whichever bank holds it, as 16-bit PCM.
fn load_asset(
    catalog: &mut SoundCatalog,
    banks: &[SoundAssetBank],
    id: u32,
    alias: &str,
    report: &mut Vec<String>,
) -> Option<String> {
    let (entry, bytes) = match banks.iter().find_map(|bank| bank.read(id)) {
        Some(Ok(read)) => read,
        Some(Err(error)) => {
            report.push(format!("t6 sound {alias}: asset {id:08x}: {error}"));
            return None;
        }
        None => {
            report.push(format!("t6 sound {alias}: asset {id:08x} in no bank"));
            return None;
        }
    };
    let (pcm, samples) = match entry.format {
        SAB_FORMAT_PCMS16 => (bytes, entry.frame_count),
        SAB_FORMAT_FLAC => match decode_flac(bytes, entry.channels) {
            Ok(decoded) => decoded,
            Err(error) => {
                report.push(format!("t6 sound {alias}: asset {id:08x} flac: {error}"));
                return None;
            }
        },
        format => {
            report.push(format!(
                "t6 sound {alias}: asset {id:08x} format {format} is not read"
            ));
            return None;
        }
    };
    let rate = entry.frame_rate()?;
    let name = format!("t6/{id:08x}");
    catalog.ingest_loaded(LoadedSoundPcm {
        name: name.clone(),
        game: ZoneGame::Iw4,
        format: MSS_PCM,
        rate,
        bits: 16,
        channels: i32::from(entry.channels),
        samples,
        pcm: pcm.into(),
        zone: catalog.capture_zone_for_ingest(),
        ..Default::default()
    });
    Some(name)
}

/// A whole FLAC stream as interleaved 16-bit PCM and its frame count.
fn decode_flac(bytes: Vec<u8>, channels: u8) -> Result<(Vec<u8>, u32), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    let mut format = symphonia::default::get_probe()
        .format(
            Hint::new().with_extension("flac"),
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?
        .format;
    let track = format.default_track().ok_or("no track")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut pcm = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(|e| e.to_string())?;
        if decoded.spec().channels.count() != usize::from(channels) {
            return Err(format!(
                "{} channels, bank says {channels}",
                decoded.spec().channels.count()
            ));
        }
        let mut samples = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        samples.copy_interleaved_ref(decoded);
        pcm.extend(samples.samples().iter().flat_map(|s| s.to_le_bytes()));
    }
    let frames = pcm.len() / 2 / usize::from(channels.max(1));
    Ok((pcm, frames as u32))
}
