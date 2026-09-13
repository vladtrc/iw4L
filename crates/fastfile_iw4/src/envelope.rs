pub const MAGIC_SIGNED: &[u8; 8] = b"IWff0100";

pub const MAGIC_UNSIGNED: &[u8; 8] = b"IWffu100";

pub const MAGIC_AUTH_HEADER: &[u8; 8] = b"IWffs100";

pub const ZONE_VERSION_PC: u32 = 0x114;

pub const FILE_PREAMBLE_LEN: usize = 8 + 4 + 1 + 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signing {
    Signed,
    Unsigned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub signing: Signing,
    pub version: u32,

    pub flag: u8,

    pub stamp: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileHeaderError {
    TooShort { len: usize },
    BadMagic { got: [u8; 8] },
    BadVersion { got: u32 },
}

pub fn parse_file_header(bytes: &[u8]) -> Result<FileHeader, FileHeaderError> {
    if bytes.len() < FILE_PREAMBLE_LEN {
        return Err(FileHeaderError::TooShort { len: bytes.len() });
    }
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&bytes[0..8]);
    let signing = if &magic == MAGIC_SIGNED {
        Signing::Signed
    } else if &magic == MAGIC_UNSIGNED {
        Signing::Unsigned
    } else {
        return Err(FileHeaderError::BadMagic { got: magic });
    };
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != ZONE_VERSION_PC {
        return Err(FileHeaderError::BadVersion { got: version });
    }
    Ok(FileHeader {
        signing,
        version,
        flag: bytes[12],
        stamp: [
            u32::from_le_bytes(bytes[13..17].try_into().unwrap()),
            u32::from_le_bytes(bytes[17..21].try_into().unwrap()),
        ],
    })
}
