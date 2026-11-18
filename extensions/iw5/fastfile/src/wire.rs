use crate::asset_type::AssetType;
use crate::zone::{MAX_XFILE_COUNT, Ptr, XFILE_BLOCK_VIRTUAL, XFILE_HEADER_LEN, parse_zone_header};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iw5WireFormat {
    X86,
    X64,
}

impl Iw5WireFormat {
    pub const fn pointer_bytes(self) -> usize {
        match self {
            Self::X86 => 4,
            Self::X64 => 8,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirePointer {
    Null,
    Following,
    Insert,
    Offset(Ptr),
}

impl WirePointer {
    pub fn decode(raw: u64, format: Iw5WireFormat) -> Option<Self> {
        let following = match format {
            Iw5WireFormat::X86 => u64::from(u32::MAX),
            Iw5WireFormat::X64 => u64::MAX,
        };
        if raw == 0 {
            Some(Self::Null)
        } else if raw == following {
            Some(Self::Following)
        } else if raw == following - 1 {
            Some(Self::Insert)
        } else {
            let packed = u32::try_from(raw.checked_sub(1)?).ok()?;
            let block = (packed >> 28) as u8;
            ((block as usize) < MAX_XFILE_COUNT).then_some(Self::Offset(Ptr {
                block,
                offset: packed & 0x0fff_ffff,
            }))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireTableError {
    Truncated { at: usize, needed: usize },
    InvalidCountPointer { at: usize },
    InvalidPointer { at: usize, raw: u64 },
    BlockBounds { at: usize },
    UnknownAssetType { at: usize, raw: u32 },
    UnterminatedString { at: usize },
    TrailingEmptyData { at: usize },
}

impl core::fmt::Display for WireTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated { at, needed } => {
                write!(f, "truncated at {at}: needed {needed} bytes")
            }
            Self::InvalidCountPointer { at } => write!(f, "count/pointer disagree at {at}"),
            Self::InvalidPointer { at, raw } => write!(f, "undecodable pointer {raw:#x} at {at}"),
            Self::BlockBounds { at } => write!(f, "table overruns its block at {at}"),
            Self::UnknownAssetType { at, raw } => {
                write!(f, "unknown asset pool id {raw:#x} at {at}")
            }
            Self::UnterminatedString { at } => write!(f, "unterminated string at {at}"),
            Self::TrailingEmptyData { at } => write!(f, "empty table with payload left at {at}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WireAssetTable<'a> {
    format: Iw5WireFormat,
    pub string_count: usize,
    pub asset_count: usize,
    pub bodies_offset: usize,
    entries: &'a [u8],
}

impl WireAssetTable<'_> {
    pub fn format(&self) -> Iw5WireFormat {
        self.format
    }

    pub fn kinds(&self) -> impl Iterator<Item = AssetType> + '_ {
        self.entries
            .chunks_exact(2 * self.format.pointer_bytes())
            .map(|entry| {
                AssetType::from_u32(u32::from_le_bytes(entry[..4].try_into().unwrap())).unwrap()
            })
    }
}

fn bytes(data: &[u8], at: usize, needed: usize) -> Result<&[u8], WireTableError> {
    data.get(at..at.saturating_add(needed))
        .ok_or(WireTableError::Truncated { at, needed })
}

fn count(data: &[u8], at: usize) -> Result<usize, WireTableError> {
    Ok(u32::from_le_bytes(bytes(data, at, 4)?.try_into().unwrap()) as usize)
}

fn pointer(data: &[u8], at: usize, format: Iw5WireFormat) -> Result<WirePointer, WireTableError> {
    let mut raw = [0; 8];
    raw[..format.pointer_bytes()].copy_from_slice(bytes(data, at, format.pointer_bytes())?);
    let raw = u64::from_le_bytes(raw);
    WirePointer::decode(raw, format).ok_or(WireTableError::InvalidPointer { at, raw })
}

pub fn read_wire_asset_table(
    data: &[u8],
    format: Iw5WireFormat,
) -> Result<WireAssetTable<'_>, WireTableError> {
    let header = parse_zone_header(data).map_err(|_| WireTableError::Truncated {
        at: 0,
        needed: XFILE_HEADER_LEN,
    })?;
    let width = format.pointer_bytes();
    let head = XFILE_HEADER_LEN;
    let string_count = count(data, head)?;
    let asset_count = count(data, head + 2 * width)?;
    for (at, count) in [
        (head + width, string_count),
        (head + 3 * width, asset_count),
    ] {
        match (count, pointer(data, at, format)?) {
            (0, WirePointer::Null | WirePointer::Following) => {}
            (1.., WirePointer::Following) => {}
            _ => return Err(WireTableError::InvalidCountPointer { at }),
        }
    }
    let virtual_size = header.block_size[XFILE_BLOCK_VIRTUAL] as usize;
    let mut cursor = head + 4 * width;
    let strings_size = string_count
        .checked_mul(width)
        .ok_or(WireTableError::BlockBounds { at: cursor })?;
    if strings_size > virtual_size {
        return Err(WireTableError::BlockBounds { at: cursor });
    }
    bytes(data, cursor, strings_size)?;
    let slots = cursor;
    cursor += strings_size;
    let mut virtual_end = strings_size;
    for i in 0..string_count {
        let at = slots + i * width;
        match pointer(data, at, format)? {
            WirePointer::Null => {}
            WirePointer::Following => {
                let length = data[cursor..]
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or(WireTableError::UnterminatedString { at: cursor })?
                    + 1;
                virtual_end = virtual_end
                    .checked_add(length)
                    .ok_or(WireTableError::BlockBounds { at: cursor })?;
                if virtual_end > virtual_size {
                    return Err(WireTableError::BlockBounds { at: cursor });
                }
                cursor += length;
            }
            WirePointer::Offset(p)
                if p.block as usize == XFILE_BLOCK_VIRTUAL
                    && p.offset as usize >= strings_size
                    && (p.offset as usize) < virtual_end => {}
            _ => return Err(WireTableError::InvalidCountPointer { at }),
        }
    }

    if asset_count > 0 {
        virtual_end = virtual_end
            .checked_add(3)
            .ok_or(WireTableError::BlockBounds { at: cursor })?
            & !3;
    }
    let entry_bytes = asset_count
        .checked_mul(2 * width)
        .ok_or(WireTableError::BlockBounds { at: cursor })?;
    if virtual_end
        .checked_add(entry_bytes)
        .is_none_or(|end| end > virtual_size)
    {
        return Err(WireTableError::BlockBounds { at: cursor });
    }
    let entries = bytes(data, cursor, entry_bytes)?;
    for i in 0..asset_count {
        let at = cursor + i * 2 * width;
        let raw = count(data, at)? as u32;
        if AssetType::from_u32(raw).is_none() {
            return Err(WireTableError::UnknownAssetType { at, raw });
        }

        if raw == AssetType::SndDriverGlobals as u32 {
            continue;
        }
        if let WirePointer::Offset(p) = pointer(data, at + width, format)?
            && p.offset >= header.block_size[p.block as usize]
        {
            return Err(WireTableError::BlockBounds { at: at + width });
        }
    }
    cursor += entry_bytes;

    if asset_count == 0 && cursor != data.len() {
        return Err(WireTableError::TrailingEmptyData { at: cursor });
    }
    Ok(WireAssetTable {
        format,
        string_count,
        asset_count,
        bodies_offset: cursor,
        entries,
    })
}
