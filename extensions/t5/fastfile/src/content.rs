use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneError, ZonePtr, ZoneStream};

pub const XASSET_LIST_LEN: usize = 16;
pub const XASSET_ENTRY_LEN: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptStrings {
    array: Option<Ptr>,
    count: usize,
}

impl ScriptStrings {
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn array(&self) -> Option<Ptr> {
        self.array
    }

    pub fn get<'s>(&self, s: &'s ZoneStream<'_>, i: u16) -> Option<&'s str> {
        let arr = self.array?;
        if i as usize >= self.count {
            return None;
        }
        match s.ptr_at(arr, i as usize * 4).ok()? {
            ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).ok(),
            ZonePtr::Null => Some(""),
            _ => None,
        }
    }
}

pub trait AssetSink {
    fn set_script_strings(&mut self, strings: ScriptStrings);

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AssetTable {
    pub strings: ScriptStrings,
    array: Option<Ptr>,
    count: usize,
}

impl AssetTable {
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn array(&self) -> Option<Ptr> {
        self.array
    }

    pub fn kind(&self, s: &ZoneStream<'_>, i: usize) -> Result<AssetType> {
        let arr = self.array.ok_or(ZoneError::NoBlockPushed)?;
        if i >= self.count {
            return Err(ZoneError::BadOffset {
                block: XFILE_BLOCK_VIRTUAL,
                offset: i,
                size: self.count,
            });
        }
        let raw = s.u32_at(arr, i * XASSET_ENTRY_LEN)?;
        AssetType::from_u32(raw).ok_or(ZoneError::UnknownAssetType(raw))
    }

    pub fn slot(&self, i: usize) -> Option<Ptr> {
        let arr = self.array?;
        (i < self.count).then(|| arr.at(i * XASSET_ENTRY_LEN + 4))
    }
}

pub fn open_asset_table(s: &mut ZoneStream<'_>) -> Result<AssetTable> {
    let head = s.read_raw(XASSET_LIST_LEN)?;
    let rd = |o: usize| u32::from_le_bytes([head[o], head[o + 1], head[o + 2], head[o + 3]]);
    let string_count = rd(0) as usize;
    let strings_ptr = ZonePtr::decode(rd(4));
    let asset_count = rd(8) as usize;
    let assets_ptr = ZonePtr::decode(rd(12));

    s.push(XFILE_BLOCK_VIRTUAL)?;

    let strings = load_script_strings(s, strings_ptr, string_count)?;

    let array = if assets_ptr.is_null() {
        None
    } else {
        Some(s.alloc_load(4, XASSET_ENTRY_LEN * asset_count)?)
    };

    Ok(AssetTable {
        strings,
        array,
        count: if array.is_some() { asset_count } else { 0 },
    })
}

pub fn load_zone(s: &mut ZoneStream<'_>, sink: &mut dyn AssetSink) -> Result<AssetTable> {
    let table = open_asset_table(s)?;
    sink.set_script_strings(table.strings);

    for i in 0..table.count() {
        let ty = table.kind(s, i)?;
        let slot = table.slot(i).ok_or(ZoneError::NoBlockPushed)?;
        sink.load_asset(s, i, ty, slot)?;
    }

    s.pop()?;
    Ok(table)
}

fn load_script_strings(s: &mut ZoneStream<'_>, p: ZonePtr, count: usize) -> Result<ScriptStrings> {
    if p.is_null() || count == 0 {
        return Ok(ScriptStrings::default());
    }

    let arr = s.alloc_load(4, 4 * count)?;
    for i in 0..count {
        let resolved = match s.ptr_at(arr, i * 4)? {
            ZonePtr::Null => None,
            ZonePtr::Offset(q) => {
                s.note_offset(q);
                Some(s.resolve_alias(q))
            }
            _ => {
                s.begin_body(arr.at(i * 4))?;
                Some(s.load_string()?)
            }
        };
        let encoded = match resolved {
            Some(q) => ZonePtr::encode_offset(q),
            None => 0,
        };
        s.write_u32_at(arr, i * 4, encoded)?;
    }

    Ok(ScriptStrings {
        array: Some(arr),
        count,
    })
}
