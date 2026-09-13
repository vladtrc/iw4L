use super::{AssetLinkSink, always_array, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{MapEntsGeometry, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

fn linked_name<'a>(s: &'a ZoneStream<'_>, p: crate::zone::Ptr) -> &'a str {
    match s.ptr_at(p, 0).ok() {
        Some(ZonePtr::Offset(np)) => s.cstr(s.resolve_alias(np)).ok().unwrap_or(""),
        _ => "",
    }
}

pub(super) fn load_ddl(s: &mut ZoneStream<'_>) -> Result<()> {
    let root = s.alloc_load(4, 8)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, root, 0)?;
    let mut slot = root.at(4);
    while let Some(def) = always_array(s, slot, 4, 28)? {
        let count = s.u32_at(def, 12)? as usize;
        if let Some(structs) = always_array(s, def.at(8), 4, count * 16)? {
            for i in 0..count {
                let structure = structs.at(i * 16);
                follow_name(s, structure, 0)?;
                let members = s.u32_at(structure, 8)? as usize;
                if let Some(array) = always_array(s, structure.at(12), 4, members * 48)? {
                    for j in 0..members {
                        follow_name(s, array.at(j * 48), 0)?;
                    }
                }
            }
        }
        let count = s.u32_at(def, 20)? as usize;
        if let Some(enums) = always_array(s, def.at(16), 4, count * 12)? {
            for i in 0..count {
                let enumeration = enums.at(i * 12);
                follow_name(s, enumeration, 0)?;
                let members = s.u32_at(enumeration, 4)? as usize;
                if let Some(array) = always_array(s, enumeration.at(8), 4, members * 4)? {
                    for j in 0..members {
                        follow_name(s, array.at(j * 4), 0)?;
                    }
                }
            }
        }
        slot = def.at(24);
    }
    s.pop()
}

pub(super) fn load_localize(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::LOCALIZE_ENTRY)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let value = s.follow_string(p, 0)?;
    let name = s.follow_string(p, 4)?;
    if let (Some(name), Some(value)) = (name, value) {
        let name = s.cstr(name)?;
        let value = s.cstr_bytes(value)?;
        links.capture_localize(name, value)?;
    }
    s.pop()
}

pub(super) fn load_raw_file(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::RAW_FILE)?;
    let len = s.i32_at(p, 4)?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let bytes = len.saturating_add(1);
    let data_ptr = if s.begin_body(p.at(8))? {
        let body = s.alloc_load(1, bytes)?;
        s.fixup_slot(p.at(8), body)?;
        Some(body)
    } else {
        match s.ptr_at(p, 8)? {
            ZonePtr::Offset(q) => Some(q),
            _ => None,
        }
    };
    if let Some(px) = data_ptr {
        let raw = s.slice_at(px, 0, bytes).unwrap_or(&[]);
        let name = linked_name(s, p);
        if !name.is_empty() {
            links.capture_raw_file(name, raw, false)?;
        }
    }
    s.pop()
}

pub(super) fn load_phys_preset(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::PHYS_PRESET)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    follow_name(s, p, 28)?;
    s.pop()
}

pub(super) fn load_string_table(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::STRING_TABLE)?;
    let columns = s.i32_at(p, 4)?.max(0) as usize;
    let rows = s.i32_at(p, 8)?.max(0) as usize;
    let cells = columns.saturating_mul(rows);
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(arr) = always_array(
        s,
        p.at(sz::STRING_TABLE_VALUES_OFF),
        4,
        sz::STRING_TABLE_CELL * cells,
    )? {
        for i in 0..cells {
            s.follow_string(arr.at(i * sz::STRING_TABLE_CELL), 0)?;
        }
    }
    let _ = always_array(s, p.at(sz::STRING_TABLE_CELL_INDEX_OFF), 2, 2 * cells)?;
    links.capture_string_table(s, p)?;
    s.pop()
}

pub(super) fn load_font(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::FONT)?;
    let glyph_count = s.i32_at(p, sz::FONT_GLYPH_COUNT_OFF)?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    asset_ptr_at(s, links, AssetType::Material, p.at(sz::FONT_MATERIAL_OFF))?;
    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        p.at(sz::FONT_GLOW_MATERIAL_OFF),
    )?;
    if s.begin_body(p.at(sz::FONT_GLYPHS_OFF))? {
        s.alloc_load(4, sz::FONT_GLYPH * glyph_count)?;
    }
    s.pop()
}

pub(super) fn load_map_ents(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::MAP_ENTS)?;
    let num_chars = s.i32_at(p, 8)?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let entity_string = always_array(s, p.at(4), 1, num_chars)?;
    s.pop()?;
    s.record_map_ents(MapEntsGeometry {
        entity_string,
        entity_chars: num_chars,
    });
    Ok(())
}
