pub const MAGIC_SIGNED: &[u8; 8] = b"IWff0100";

pub const MAGIC_UNSIGNED: &[u8; 8] = b"IWffu100";

pub const ZONE_VERSION_PC: u32 = 0x1D9;

pub const FILE_PREAMBLE_LEN: usize = 8 + 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signing {
    Signed,
    Unsigned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub signing: Signing,
    pub version: u32,
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
    Ok(FileHeader { signing, version })
}
