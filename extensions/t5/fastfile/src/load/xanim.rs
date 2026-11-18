use super::{AssetLinkSink, always_array, follow_name};
use crate::size as sz;
use crate::zone::{Ptr, Result, XAnimPartsGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_xanim_parts(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::XANIM_PARTS)?;
    let data_byte = s.u16_at(p, 0x4)? as usize;
    let data_short = s.u16_at(p, 0x6)? as usize;
    let data_int = s.u16_at(p, 0x8)? as usize;
    let random_data_byte = s.u16_at(p, 0xa)? as usize;
    let random_data_int = s.u16_at(p, 0xc)? as usize;
    let numframes = s.u16_at(p, 0xe)?;
    let b_loop = s.u8_at(p, 0x10)?;
    let mut bone_count = [0u8; 10];
    for (i, b) in bone_count.iter_mut().enumerate() {
        *b = s.u8_at(p, 0x18 + i)?;
    }
    let bone_all = bone_count[9] as usize;
    let notify_count = s.u8_at(p, 0x22)? as usize;
    let random_data_short = s.u32_at(p, 0x28)? as usize;
    let index_count = s.u32_at(p, 0x2c)? as usize;
    let framerate = s.f32_at(p, 0x30)?;
    let frequency = s.f32_at(p, 0x34)?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let names = always_array(s, p.at(sz::XANIM_NAMES_OFF), 2, 2 * bone_all)?;

    let notify = always_array(
        s,
        p.at(sz::XANIM_NOTIFY_OFF),
        4,
        sz::XANIM_NOTIFY_INFO * notify_count,
    )?;

    if let Some(delta) = always_array(s, p.at(sz::XANIM_DELTA_PART_OFF), 4, sz::XANIM_DELTA_PART)? {
        load_delta_part(s, delta, numframes as usize)?;
    }

    let data_byte_ptr = always_array(s, p.at(sz::XANIM_DATA_BYTE_OFF), 1, data_byte)?;
    let data_short_ptr = always_array(s, p.at(sz::XANIM_DATA_SHORT_OFF), 2, 2 * data_short)?;
    let data_int_ptr = always_array(s, p.at(sz::XANIM_DATA_INT_OFF), 4, 4 * data_int)?;
    let random_data_short_ptr = always_array(
        s,
        p.at(sz::XANIM_RANDOM_DATA_SHORT_OFF),
        2,
        2 * random_data_short,
    )?;
    let random_data_byte_ptr =
        always_array(s, p.at(sz::XANIM_RANDOM_DATA_BYTE_OFF), 1, random_data_byte)?;
    let random_data_int_ptr = always_array(
        s,
        p.at(sz::XANIM_RANDOM_DATA_INT_OFF),
        4,
        4 * random_data_int,
    )?;

    let indices = load_indices(
        s,
        p.at(sz::XANIM_INDICES_OFF),
        numframes as usize,
        index_count,
    )?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    links.capture_xanim(
        s,
        XAnimPartsGeometry {
            name,
            numframes,

            flags: if b_loop != 0 { 1 } else { 0 },
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
            indices_are_bytes: numframes < 256,
        },
    )?;
    s.pop()
}

fn load_indices(
    s: &mut ZoneStream<'_>,
    slot: Ptr,
    numframes: usize,
    index_count: usize,
) -> Result<Option<Ptr>> {
    if numframes >= 0x100 {
        always_array(s, slot, 2, 2 * index_count)
    } else {
        always_array(s, slot, 1, index_count)
    }
}

fn load_delta_part(s: &mut ZoneStream<'_>, p: Ptr, numframes: usize) -> Result<()> {
    load_part_trans(s, p.at(0), numframes)?;
    load_delta_quat(s, p.at(4), numframes)?;
    Ok(())
}

fn indices_bytes(size: usize, numframes: usize) -> usize {
    let n = size + 1;
    if numframes >= 0x100 { 2 * n } else { n }
}

fn load_part_trans(s: &mut ZoneStream<'_>, slot: Ptr, numframes: usize) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }

    s.align_pos(4)?;
    let peek = s.peek(4);
    if peek.len() < 4 {
        return Err(crate::zone::ZoneError::Truncated {
            at: s.cursor(),
            needed: 4,
            len: s.len(),
        });
    }
    let size = u16::from_le_bytes([peek[0], peek[1]]) as usize;
    let small_trans = peek[2];
    let total = if size == 0 {
        4 + 12
    } else {
        4 + 28 + indices_bytes(size, numframes)
    };
    let body = s.alloc_load(1, total)?;
    s.fixup_slot(slot, body)?;
    if size != 0 {
        load_dynamic_frames(s, body.at(28), size, small_trans)?;
    }
    Ok(())
}

fn load_delta_quat(s: &mut ZoneStream<'_>, slot: Ptr, numframes: usize) -> Result<()> {
    if !super::always_alloc(s, slot)? {
        return Ok(());
    }
    s.align_pos(4)?;
    let peek = s.peek(4);
    if peek.len() < 4 {
        return Err(crate::zone::ZoneError::Truncated {
            at: s.cursor(),
            needed: 4,
            len: s.len(),
        });
    }
    let size = u16::from_le_bytes([peek[0], peek[1]]) as usize;
    let total = if size == 0 {
        4 + 4
    } else {
        4 + 4 + indices_bytes(size, numframes)
    };
    let body = s.alloc_load(1, total)?;
    s.fixup_slot(slot, body)?;
    if size != 0 {
        let _ = always_array(s, body.at(4), 4, 4 * (size + 1))?;
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
