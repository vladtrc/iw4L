use asset_iw4::size as sz;

use super::{AssetLinkSink, follow_name};
use crate::zone::{Result, XAnimPartsGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_xanim(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::XANIM_PARTS, 136))?;

    let data_byte_count = s.u16_at(p, s.layout(4, 8))? as usize;
    let data_short_count = s.u16_at(p, s.layout(6, 10))? as usize;
    let data_int_count = s.u16_at(p, s.layout(8, 12))? as usize;
    let random_data_byte_count = s.u16_at(p, s.layout(10, 14))? as usize;
    let random_data_int_count = s.u16_at(p, s.layout(12, 16))? as usize;
    let numframes = s.u16_at(p, s.layout(14, 18))?;
    let flags = s.u8_at(p, s.layout(16, 20))?;
    let mut bone_count = [0u8; 10];
    for (i, b) in bone_count.iter_mut().enumerate() {
        *b = s.u8_at(p, s.layout(17, 21) + i)?;
    }
    let notify_count = s.u8_at(p, s.layout(27, 31))? as usize;
    let random_data_short_count = s.u32_at(p, s.layout(32, 36))? as usize;
    let index_count = s.u32_at(p, s.layout(36, 40))? as usize;
    let framerate = s.f32_at(p, s.layout(40, 44))?;
    let frequency = s.f32_at(p, s.layout(44, 48))?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let names = s.plain_array(p, s.layout(48, 56), 2, 2, bone_count[9] as usize)?;

    let mut notify = None;
    if s.begin_body(p.at(s.layout(80, 120)))? {
        notify = Some(s.alloc_load(4, sz::XANIM_NOTIFY_INFO * notify_count)?);
    }

    let mut delta_trans = crate::XAnimDeltaTransGeometry::default();
    if s.begin_body(p.at(s.layout(84, 128)))? {
        delta_trans = load_delta_part(s, numframes)?;
    }

    let data_byte = s.plain_array(p, s.layout(52, 64), 1, 1, data_byte_count)?;
    let data_short = s.plain_array(p, s.layout(56, 72), 2, 2, data_short_count)?;
    let data_int = s.plain_array(p, s.layout(60, 80), 4, 4, data_int_count)?;
    let random_data_short = s.plain_array(p, s.layout(64, 88), 2, 2, random_data_short_count)?;
    let random_data_byte = s.plain_array(p, s.layout(68, 96), 1, 1, random_data_byte_count)?;
    let random_data_int = s.plain_array(p, s.layout(72, 104), 4, 4, random_data_int_count)?;

    let indices = if numframes < 256 {
        s.plain_array(p, s.layout(76, 112), 1, 1, index_count)?
    } else {
        s.plain_array(p, s.layout(76, 112), 2, 2, index_count)?
    };

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };

    let geometry = XAnimPartsGeometry {
        name,
        numframes,
        flags,
        bone_count,
        notify_count,
        framerate,
        frequency,
        names,
        notify,
        data_byte,
        data_byte_count,
        data_short,
        data_short_count,
        data_int,
        data_int_count,
        random_data_short,
        random_data_short_count,
        random_data_byte,
        random_data_byte_count,
        random_data_int,
        random_data_int_count,
        indices,
        index_count,
        indices_are_bytes: numframes < 256,
        delta_trans,
    };
    links.capture_xanim(s, geometry)?;

    s.pop()
}

fn load_delta_part(
    s: &mut ZoneStream<'_>,
    numframes: u16,
) -> Result<crate::XAnimDeltaTransGeometry> {
    let p = s.alloc_load(4, s.layout(sz::XANIM_DELTA_PART, 24))?;
    let trans = if s.begin_body(p.at(0))? {
        load_part_trans(s, numframes)?
    } else {
        crate::XAnimDeltaTransGeometry::default()
    };
    if s.begin_body(p.at(s.layout(4, 8)))? {
        load_delta_quat2(s, numframes)?;
    }
    if s.begin_body(p.at(s.layout(8, 16)))? {
        load_delta_quat(s, numframes)?;
    }
    Ok(trans)
}

fn load_part_trans(
    s: &mut ZoneStream<'_>,
    numframes: u16,
) -> Result<crate::XAnimDeltaTransGeometry> {
    let head = s.alloc_load(4, s.layout(4, 8))?;
    let size = s.u16_at(head, 0)?;
    let small_trans = s.u8_at(head, 2)?;
    let indices_are_bytes = numframes < 256;

    if size == 0 {
        let constant = s.alloc_load(4, 12)?;
        return Ok(crate::XAnimDeltaTransGeometry {
            size,
            small: small_trans,
            constant: Some(constant),
            mins_step: None,
            frames: None,
            indices: None,
            indices_are_bytes,
        });
    }

    let n = size as usize + 1;
    let fr = s.alloc_load(4, s.layout(28, 32))?;
    let indices = load_dynamic_indices(s, numframes, n)?;

    let frames = if s.begin_body(fr.at(24))? {
        let packed = if small_trans != 0 {
            s.alloc_load(1, 3 * n)?
        } else {
            s.alloc_load(4, 6 * n)?
        };
        Some(packed)
    } else {
        None
    };
    Ok(crate::XAnimDeltaTransGeometry {
        size,
        small: small_trans,
        constant: None,
        mins_step: Some(fr),
        frames,
        indices: Some(indices),
        indices_are_bytes,
    })
}

fn load_delta_quat2(s: &mut ZoneStream<'_>, numframes: u16) -> Result<()> {
    let head = s.alloc_load(4, s.layout(4, 8))?;
    let size = s.u16_at(head, 0)?;
    if size == 0 {
        s.alloc_load(2, 4)?;
        return Ok(());
    }
    let n = size as usize + 1;
    let fr = s.alloc_load(4, s.layout(4, 8))?;
    load_dynamic_indices(s, numframes, n)?;
    if s.begin_body(fr.at(0))? {
        s.alloc_load(4, 4 * n)?;
    }
    Ok(())
}

fn load_delta_quat(s: &mut ZoneStream<'_>, numframes: u16) -> Result<()> {
    let head = s.alloc_load(4, s.layout(4, 8))?;
    let size = s.u16_at(head, 0)?;
    if size == 0 {
        s.alloc_load(2, 8)?;
        return Ok(());
    }
    let n = size as usize + 1;
    let fr = s.alloc_load(4, s.layout(4, 8))?;
    load_dynamic_indices(s, numframes, n)?;
    if s.begin_body(fr.at(0))? {
        s.alloc_load(4, 8 * n)?;
    }
    Ok(())
}

fn load_dynamic_indices(s: &mut ZoneStream<'_>, numframes: u16, n: usize) -> Result<crate::Ptr> {
    if numframes < 256 {
        s.alloc_load(1, n)
    } else {
        s.alloc_load(2, 2 * n)
    }
}
