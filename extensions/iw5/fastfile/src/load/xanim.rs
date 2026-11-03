use super::{AssetLinkSink, always_array, follow_name};
use crate::size as sz;
use crate::zone::{Ptr, Result, XAnimPartsGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_xanim_parts(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::XANIM_PARTS, 136))?;
    let counts = s.layout(0x4, 0x8);
    let data_byte = s.u16_at(p, counts)? as usize;
    let data_short = s.u16_at(p, counts + 2)? as usize;
    let data_int = s.u16_at(p, counts + 4)? as usize;
    let random_data_byte = s.u16_at(p, counts + 6)? as usize;
    let random_data_int = s.u16_at(p, counts + 8)? as usize;
    let numframes = s.u16_at(p, counts + 10)?;
    let flags = s.u8_at(p, counts + 12)?;
    let mut bone_count = [0u8; 10];
    for (i, b) in bone_count.iter_mut().enumerate() {
        *b = s.u8_at(p, counts + 13 + i)?;
    }
    let notify_count = s.u8_at(p, s.layout(sz::XANIM_NOTIFY_COUNT_OFF, 31))? as usize;
    let random_data_short = s.u32_at(p, s.layout(0x20, 36))? as usize;
    let index_count = s.u32_at(p, s.layout(0x24, 40))? as usize;
    let framerate = s.f32_at(p, s.layout(0x28, 44))?;
    let frequency = s.f32_at(p, s.layout(0x2c, 48))?;
    let bone_all = bone_count[9] as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let names = always_array(s, p.at(s.layout(sz::XANIM_NAMES_OFF, 56)), 2, 2 * bone_all)?;
    let notify = always_array(
        s,
        p.at(s.layout(sz::XANIM_NOTIFY_OFF, 120)),
        4,
        sz::XANIM_NOTIFY_INFO * notify_count,
    )?;

    let delta_part = always_array(
        s,
        p.at(s.layout(sz::XANIM_DELTA_PART_OFF, 128)),
        4,
        s.layout(sz::XANIM_DELTA_PART, 24),
    )?;
    if let Some(delta) = delta_part {
        load_delta_part(s, delta, numframes as usize)?;
    }

    let data_byte_ptr = always_array(s, p.at(s.layout(sz::XANIM_DATA_BYTE_OFF, 64)), 1, data_byte)?;
    let data_short_ptr = always_array(
        s,
        p.at(s.layout(sz::XANIM_DATA_SHORT_OFF, 72)),
        2,
        2 * data_short,
    )?;
    let data_int_ptr = always_array(
        s,
        p.at(s.layout(sz::XANIM_DATA_INT_OFF, 80)),
        4,
        4 * data_int,
    )?;
    let random_data_short_ptr = always_array(
        s,
        p.at(s.layout(sz::XANIM_RANDOM_DATA_SHORT_OFF, 88)),
        2,
        2 * random_data_short,
    )?;
    let random_data_byte_ptr = always_array(
        s,
        p.at(s.layout(sz::XANIM_RANDOM_DATA_BYTE_OFF, 96)),
        1,
        random_data_byte,
    )?;
    let random_data_int_ptr = always_array(
        s,
        p.at(s.layout(sz::XANIM_RANDOM_DATA_INT_OFF, 104)),
        4,
        4 * random_data_int,
    )?;
    load_indices(
        s,
        p.at(s.layout(sz::XANIM_INDICES_OFF, 112)),
        numframes as usize,
        index_count,
    )?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    let indices = match s.ptr_at(p, s.layout(sz::XANIM_INDICES_OFF, 112))? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    links.capture_xanim(
        s,
        XAnimPartsGeometry {
            name,
            numframes,
            flags,
            bone_count,
            notify_count,
            framerate,
            frequency,
            names,
            notify,
            data_byte: data_byte_ptr,
            data_byte_count: data_byte,
            data_short: data_short_ptr,
            data_short_count: data_short,
            data_int: data_int_ptr,
            data_int_count: data_int,
            random_data_short: random_data_short_ptr,
            random_data_short_count: random_data_short,
            random_data_byte: random_data_byte_ptr,
            random_data_byte_count: random_data_byte,
            random_data_int: random_data_int_ptr,
            random_data_int_count: random_data_int,
            indices,
            index_count,
            indices_are_bytes: numframes < 0x100,
        },
    )?;
    s.pop()
}

fn load_indices(
    s: &mut ZoneStream<'_>,
    slot: Ptr,
    numframes: usize,
    index_count: usize,
) -> Result<()> {
    if numframes >= 0x100 {
        always_array(s, slot, 2, 2 * index_count)?;
    } else {
        always_array(s, slot, 1, index_count)?;
    }
    Ok(())
}

fn load_delta_part(s: &mut ZoneStream<'_>, p: Ptr, numframes: usize) -> Result<()> {
    load_part_trans(s, p.at(0), numframes)?;
    load_delta_quat2(s, p.at(s.layout(4, 8)), numframes)?;
    load_delta_quat(s, p.at(s.layout(8, 16)), numframes)?;
    Ok(())
}

fn load_part_trans(s: &mut ZoneStream<'_>, slot: Ptr, numframes: usize) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let head = s.alloc_load(4, s.layout(4, 8))?;
    s.fixup_slot(slot, head)?;
    let size = s.u16_at(head, 0)? as usize;
    let small_trans = s.u8_at(head, 2)?;
    if size != 0 {
        let frames = s.alloc_load(4, s.layout(28, 32))?;
        load_dynamic_indices(s, size, numframes)?;
        load_dynamic_frames(s, frames.at(24), size, small_trans)?;
    } else {
        s.alloc_load(4, 12)?;
    }
    Ok(())
}

fn load_delta_quat2(s: &mut ZoneStream<'_>, slot: Ptr, numframes: usize) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let head = s.alloc_load(4, s.layout(4, 8))?;
    s.fixup_slot(slot, head)?;
    let size = s.u16_at(head, 0)? as usize;
    if size != 0 {
        let data = s.alloc_load(4, s.layout(4, 8))?;
        load_dynamic_indices(s, size, numframes)?;
        always_array(s, data.at(0), 4, 4 * (size + 1))?;
    } else {
        s.alloc_load(4, 4)?;
    }
    Ok(())
}

fn load_delta_quat(s: &mut ZoneStream<'_>, slot: Ptr, numframes: usize) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    let head = s.alloc_load(4, s.layout(4, 8))?;
    s.fixup_slot(slot, head)?;
    let size = s.u16_at(head, 0)? as usize;
    if size != 0 {
        let data = s.alloc_load(4, s.layout(4, 8))?;
        load_dynamic_indices(s, size, numframes)?;
        always_array(s, data.at(0), 4, 8 * (size + 1))?;
    } else {
        s.alloc_load(4, 8)?;
    }
    Ok(())
}

fn load_dynamic_indices(s: &mut ZoneStream<'_>, size: usize, numframes: usize) -> Result<()> {
    let count = size + 1;
    if numframes >= 0x100 {
        s.alloc_load(2, 2 * count)?;
    } else {
        s.alloc_load(1, count)?;
    }
    Ok(())
}

fn load_dynamic_frames(
    s: &mut ZoneStream<'_>,
    slot: Ptr,
    size: usize,
    small_trans: u8,
) -> Result<()> {
    let count = if size != 0 { size + 1 } else { 0 };
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(p) => {
            s.note_offset(p);
            Ok(())
        }
        ZonePtr::Following | ZonePtr::Insert => {
            if small_trans != 0 {
                let body = s.alloc_load(1, 3 * count)?;
                s.fixup_slot(slot, body)?;
            } else {
                let body = s.alloc_load(4, 6 * count)?;
                s.fixup_slot(slot, body)?;
            }
            Ok(())
        }
    }
}
