use asset_core::FamilyId;
use asset_transport::zone::inflate_zone;
use asset_transport::{ZoneImage, ZoneOpenError};
pub fn decode_zone(bytes: &[u8]) -> Result<ZoneImage, ZoneOpenError> {
    let header = ext_t5_fastfile::parse_file_header(bytes).map_err(native_error)?;
    let body = &bytes[ext_t5_fastfile::FILE_PREAMBLE_LEN..];

    let _ = header.signing;
    let image = inflate_zone(body)?;

    if image.len() < ext_t5_fastfile::XFILE_HEADER_LEN {
        return Err(ZoneOpenError::Truncated);
    }

    Ok(ZoneImage {
        game: FamilyId::new("t5"),
        version: header.version,
        bytes: image,
        iw4_wire_format: None,
    })
}

pub struct T5ZoneMemory {
    blocks: [Vec<u8>; ext_t5_fastfile::MAX_XFILE_COUNT],
    insert_map: Vec<u8>,
}

impl T5ZoneMemory {
    pub fn for_header(header: &ext_t5_fastfile::ZoneHeader) -> T5ZoneMemory {
        let blocks = std::array::from_fn(|i| vec![0u8; t5_block_capacity(header, i)]);
        let insert_map = vec![0u8; ext_t5_fastfile::ZoneStream::insert_map_len(header)];
        T5ZoneMemory { blocks, insert_map }
    }

    pub fn total_bytes(&self) -> usize {
        self.blocks.iter().map(|b| b.len()).sum()
    }

    pub fn stream<'a>(
        &'a mut self,
        image: &'a [u8],
    ) -> Result<ext_t5_fastfile::ZoneStream<'a>, ext_t5_fastfile::ZoneError> {
        let [b0, b1, b2, b3, b4, b5, b6] = &mut self.blocks;
        let blocks: [&mut [u8]; ext_t5_fastfile::MAX_XFILE_COUNT] = [
            b0.as_mut_slice(),
            b1.as_mut_slice(),
            b2.as_mut_slice(),
            b3.as_mut_slice(),
            b4.as_mut_slice(),
            b5.as_mut_slice(),
            b6.as_mut_slice(),
        ];
        ext_t5_fastfile::ZoneStream::new(image, blocks, &mut self.insert_map)
    }
}

fn t5_block_capacity(header: &ext_t5_fastfile::ZoneHeader, block: usize) -> usize {
    let logical = header.block_size[block] as usize;
    if logical == 0 || block != ext_t5_fastfile::XFILE_BLOCK_RUNTIME {
        return logical;
    }

    logical.saturating_add(0x0f).next_multiple_of(0x1000)
}

pub trait T5ZoneImageExt {
    fn t5_header(&self) -> Result<ext_t5_fastfile::ZoneHeader, ZoneOpenError>;
}
impl T5ZoneImageExt for ZoneImage {
    fn t5_header(&self) -> Result<ext_t5_fastfile::ZoneHeader, ZoneOpenError> {
        if self.game != FamilyId::new("t5") {
            return Err(ZoneOpenError::WrongGame {
                expected: FamilyId::new("t5"),
                got: self.game,
            });
        }
        ext_t5_fastfile::parse_zone_header(&self.bytes).map_err(|_| ZoneOpenError::Truncated)
    }
}

fn native_error(error: impl std::fmt::Debug) -> ZoneOpenError {
    ZoneOpenError::Family {
        family: FamilyId::new("t5"),
        message: format!("{error:?}"),
    }
}
