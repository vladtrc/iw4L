use asset_core::FamilyId;
use asset_transport::zone::{dechunk_authed, inflate_zone};
use asset_transport::{ZoneImage, ZoneOpenError};
use ext_iw5_fastfile::Signing as Iw5Signing;
use std::borrow::Cow;
pub fn decode_zone(bytes: &[u8]) -> Result<ZoneImage, ZoneOpenError> {
    let header = ext_iw5_fastfile::parse_file_header(bytes).map_err(native_error)?;
    let body = &bytes[ext_iw5_fastfile::FILE_PREAMBLE_LEN..];
    let compressed: Cow<'_, [u8]> = match header.signing {
        Iw5Signing::Unsigned => Cow::Borrowed(body),
        Iw5Signing::Signed => Cow::Owned(dechunk_authed(body)?),
    };

    let image = inflate_zone(&compressed)?;

    if image.len() < ext_iw5_fastfile::XFILE_HEADER_LEN {
        return Err(ZoneOpenError::Truncated);
    }

    Ok(ZoneImage {
        game: FamilyId::new("iw5"),
        version: header.version,
        bytes: image,
        iw4_wire_format: None,
    })
}

pub struct Iw5ZoneMemory {
    blocks: [Vec<u8>; ext_iw5_fastfile::MAX_XFILE_COUNT],
    insert_map: Vec<u8>,
}

impl Iw5ZoneMemory {
    pub fn for_header(header: &ext_iw5_fastfile::ZoneHeader) -> Iw5ZoneMemory {
        let blocks = std::array::from_fn(|i| vec![0u8; header.block_size[i] as usize]);
        let insert_map = vec![0u8; ext_iw5_fastfile::ZoneStream::insert_map_len(header)];
        Iw5ZoneMemory { blocks, insert_map }
    }

    pub fn total_bytes(&self) -> usize {
        self.blocks.iter().map(|b| b.len()).sum()
    }

    pub fn stream<'a>(
        &'a mut self,
        image: &'a [u8],
    ) -> Result<ext_iw5_fastfile::ZoneStream<'a>, ext_iw5_fastfile::ZoneError> {
        let [b0, b1, b2, b3, b4, b5, b6, b7, b8] = &mut self.blocks;
        let blocks: [&mut [u8]; ext_iw5_fastfile::MAX_XFILE_COUNT] = [
            b0.as_mut_slice(),
            b1.as_mut_slice(),
            b2.as_mut_slice(),
            b3.as_mut_slice(),
            b4.as_mut_slice(),
            b5.as_mut_slice(),
            b6.as_mut_slice(),
            b7.as_mut_slice(),
            b8.as_mut_slice(),
        ];
        ext_iw5_fastfile::ZoneStream::new(image, blocks, &mut self.insert_map)
    }
}

pub trait Iw5ZoneImageExt {
    fn iw5_header(&self) -> Result<ext_iw5_fastfile::ZoneHeader, ZoneOpenError>;
}
impl Iw5ZoneImageExt for ZoneImage {
    fn iw5_header(&self) -> Result<ext_iw5_fastfile::ZoneHeader, ZoneOpenError> {
        if self.game != FamilyId::new("iw5") {
            return Err(ZoneOpenError::WrongGame {
                expected: FamilyId::new("iw5"),
                got: self.game,
            });
        }
        ext_iw5_fastfile::parse_zone_header(&self.bytes).map_err(|_| ZoneOpenError::Truncated)
    }
}

fn native_error(error: impl std::fmt::Debug) -> ZoneOpenError {
    ZoneOpenError::Family {
        family: FamilyId::new("iw5"),
        message: format!("{error:?}"),
    }
}
