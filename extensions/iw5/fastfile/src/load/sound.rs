use super::{AssetLinkSink, asset_ptr_at, follow_name, load_asset_at_observed};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::wire::Iw5WireFormat;
use crate::zone::{Ptr, Result, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

const SAT_LOADED: u8 = 1;

const SPEAKER_LEVELS_X64: usize = 8;

pub(super) fn load_sound(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::SND_ALIAS_LIST, 24))?;
    let count = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let mut head = None;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let alias = s.layout(sz::SND_ALIAS, 152);
        let arr = s.alloc_load(4, alias * count)?;
        head = Some(arr);
        for i in 0..count {
            load_snd_alias(s, links, arr.at(i * alias))?;
        }
    }
    links.capture_sound(s, p, count, head)?;
    s.pop()
}

fn load_snd_alias(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    s.walk_stage = "snd_alias.names";
    follow_name(s, p, sz::SND_ALIAS_ALIAS_NAME_OFF)?;
    follow_name(s, p, s.layout(sz::SND_ALIAS_SUBTITLE_OFF, 8))?;
    follow_name(s, p, s.layout(sz::SND_ALIAS_SECONDARY_OFF, 16))?;
    follow_name(s, p, s.layout(sz::SND_ALIAS_CHAIN_OFF, 24))?;
    follow_name(s, p, s.layout(sz::SND_ALIAS_MIXER_GROUP_OFF, 32))?;

    s.walk_stage = "snd_alias.sound_file";
    let sound_file_off = s.layout(sz::SND_ALIAS_SOUND_FILE_OFF, 40);
    if s.begin_body(p.at(sound_file_off))? {
        let file = load_sound_file(s, links)?;
        s.fixup_slot(p.at(sound_file_off), file)?;
    }

    s.walk_stage = "snd_alias.curve";
    asset_ptr_at(
        s,
        links,
        AssetType::SoundCurve,
        p.at(s.layout(sz::SND_ALIAS_VOLUME_FALLOFF_CURVE_OFF, 120)),
    )?;
    s.walk_stage = "snd_alias.speaker_map";

    if s.begin_body(p.at(s.layout(sz::SND_ALIAS_SPEAKER_MAP_OFF, 144)))? {
        let sm = s.alloc_load(4, s.layout(sz::SPEAKER_MAP, 64))?;
        s.follow_string(sm, s.layout(4, 8))?;
        if s.wire_format() == Iw5WireFormat::X64 {
            for i in 0..4 {
                let map = sm.at(16 + i * 12);
                let speakers = s.u32_at(map, 0)? as usize;
                s.plain_array(map, 4, 4, SPEAKER_LEVELS_X64, speakers)?;
            }
        }
    }
    Ok(())
}

fn load_sound_file(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<Ptr> {
    let p = s.alloc_load(4, s.layout(sz::SOUND_FILE, 24))?;
    let dir_off = s.layout(4, 8);
    let name_off = s.layout(8, 16);
    if s.u8_at(p, 0)? == SAT_LOADED {
        if load_asset_at_observed(s, AssetType::LoadedSound, p.at(dir_off), links)? {
            links.bind_last_loaded_to_sound_file(p)?;
        }
    } else {
        follow_name(s, p, dir_off)?;
        follow_name(s, p, name_off)?;
        let dir = cstr_at(s, p, dir_off).unwrap_or("");
        let name = cstr_at(s, p, name_off).unwrap_or("");
        links.bind_streamed_sound_file(p, dir, name)?;
    }
    Ok(p)
}

fn cstr_at<'a>(s: &'a ZoneStream<'_>, parent: Ptr, field: usize) -> Option<&'a str> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(q) => s.cstr(s.resolve_alias(q)).ok(),
        ZonePtr::Null => Some(""),
        _ => None,
    }
}

pub(super) fn load_loaded_sound(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    s.walk_stage = "loaded_sound";
    let p = s.alloc_load(4, s.layout(sz::LOADED_SOUND, 64))?;

    let sound_off = s.layout(4, 8);
    let data_len = s.u32_at(p, sound_off + s.layout(8, 24))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    s.push(XFILE_BLOCK_TEMP)?;
    let pcm_ptr = s.plain_array(p, sound_off + s.layout(36, 48), 1, 1, data_len)?;
    s.walk_stage = "loaded_sound.pcm";
    links.capture_loaded_sound(s, p, pcm_ptr.unwrap_or(p), pcm_ptr.map_or(0, |_| data_len))?;
    s.pop()?;

    s.pop()
}

pub(super) fn load_snd_curve(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::SND_CURVE, 144))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    links.capture_snd_curve(s, p)?;
    s.pop()
}
