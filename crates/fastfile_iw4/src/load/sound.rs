use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at_linked, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

const SAT_LOADED: u8 = 1;

pub(super) fn load_sound(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::SND_ALIAS_LIST, 24))?;
    let count = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    s.record_sound_list_name(name);

    let head = if s.begin_body(p.at(s.layout(4, 8)))? {
        let arr = s.alloc_load(4, s.layout(sz::SND_ALIAS, 136) * count)?;
        s.fixup_slot(p.at(s.layout(4, 8)), arr)?;
        for i in 0..count {
            load_snd_alias(s, links, arr.at(i * s.layout(sz::SND_ALIAS, 136)))?;
        }
        Some(arr)
    } else {
        match s.ptr_at(p, s.layout(4, 8))? {
            ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
            _ => None,
        }
    };

    links.capture_sound(s, p, count, head)?;
    s.pop()
}

fn load_snd_alias(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    follow_name(s, p, s.layout(4, 8))?;
    follow_name(s, p, s.layout(8, 16))?;
    follow_name(s, p, s.layout(12, 24))?;
    follow_name(s, p, s.layout(16, 32))?;

    if s.begin_body(p.at(s.layout(20, 40)))? {
        let file = load_sound_file(s, links)?;
        s.fixup_slot(p.at(s.layout(20, 40)), file)?;
    }

    asset_ptr_at_linked(s, links, AssetType::SoundCurve, p.at(s.layout(80, 104)))?;

    if s.begin_body(p.at(s.layout(96, 128)))? {
        let sm = s.alloc_load(4, s.layout(sz::SPEAKER_MAP, 80))?;
        s.fixup_slot(p.at(s.layout(96, 128)), sm)?;
        s.follow_string(sm, s.layout(4, 8))?;
        if s.wire_format() == crate::Iw4WireFormat::X64 {
            for i in 0..4 {
                let row = sm.at(16 + i * 16);
                let count = s.u8_at(row, 0)? as usize;
                s.plain_array(row, 8, 4, 8, count)?;
            }
        }
    }
    Ok(())
}

fn load_sound_file(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<Ptr> {
    let p = s.alloc_load(4, s.layout(sz::SOUND_FILE, 24))?;
    if s.u8_at(p, 0)? == SAT_LOADED {
        if asset_ptr_at_linked(s, links, AssetType::LoadedSound, p.at(s.layout(4, 8)))? {
            links.bind_last_loaded_to_sound_file(p)?;
        }
    } else {
        follow_name(s, p, s.layout(4, 8))?;
        follow_name(s, p, s.layout(8, 16))?;
        let dir = cstr_at(s, p, s.layout(4, 8)).unwrap_or("");
        let name = cstr_at(s, p, s.layout(8, 16)).unwrap_or("");
        links.bind_streamed_sound_file(p, dir, name)?;
    }
    Ok(p)
}

fn cstr_at<'a>(s: &'a ZoneStream<'_>, parent: Ptr, field: usize) -> Option<&'a str> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).ok(),
        ZonePtr::Null => Some(""),
        _ => None,
    }
}

pub(super) fn load_loaded_sound(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::LOADED_SOUND, 64))?;
    let data_len = s.u32_at(p, s.layout(12, 32))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    s.push(XFILE_BLOCK_TEMP)?;
    let pcm_ptr = s.plain_array(p, s.layout(40, 56), 1, 1, data_len)?;
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
