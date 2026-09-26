use weapon_iw4::{HITLOC_COUNT, HITLOC_NAMES, LOCATION_DAMAGE_IDENTITY, location_damage_is_valid};

const LOCHIT_MAGIC: &str = "LOCDMGTABLE";
const LOCHIT_RAWFILE: &str = "info/mp_lochit_dmgtable";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LochitTableError {
    MissingMagic,
    OddPair,
    BadNumber,
    UnknownKey,
    NotUtf8,
}

pub fn parse_lochit_info_string(text: &str) -> Result<[f32; HITLOC_COUNT], LochitTableError> {
    let text = text.trim();
    let rest = text
        .strip_prefix(LOCHIT_MAGIC)
        .ok_or(LochitTableError::MissingMagic)?;
    let mut table = LOCATION_DAMAGE_IDENTITY;
    let mut parts = rest.split('\\').filter(|part| !part.is_empty());
    loop {
        let Some(key) = parts.next() else {
            break;
        };
        let Some(value) = parts.next() else {
            return Err(LochitTableError::OddPair);
        };
        let parsed: f32 = value.parse().map_err(|_| LochitTableError::BadNumber)?;
        let Some(slot) = HITLOC_NAMES.iter().position(|name| *name == key) else {
            return Err(LochitTableError::UnknownKey);
        };
        table[slot] = parsed;
    }
    if !location_damage_is_valid(&table) {
        return Err(LochitTableError::BadNumber);
    }
    Ok(table)
}

pub fn parse_lochit_rawfile(data: &[u8]) -> Result<[f32; HITLOC_COUNT], LochitTableError> {
    let text = core::str::from_utf8(data).map_err(|_| LochitTableError::NotUtf8)?;
    parse_lochit_info_string(text)
}

pub fn capture_lochit_table(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Option<[f32; HITLOC_COUNT]> {
    if name != LOCHIT_RAWFILE {
        return None;
    }
    let mut bytes = if zlib_compressed {
        asset_transport::inflate_zlib(data).ok()?
    } else {
        data.to_vec()
    };
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    parse_lochit_rawfile(&bytes).ok()
}
