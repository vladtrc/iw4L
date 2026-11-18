use asset_core::FamilyId;
use asset_transport::{ZoneImage, ZoneOpenError};
use std::path::Path;
pub fn decode_zone(bytes: &[u8]) -> Result<ZoneImage, ZoneOpenError> {
    let image = parse_t6_zone_image(bytes).map_err(native_error)?;
    Ok(ZoneImage {
        game: FamilyId::new("t6"),
        version: image.header.version,
        bytes: image.bytes,
        iw4_wire_format: None,
    })
}

pub struct T6ZoneImage {
    pub header: ext_t6_fastfile::FileHeader,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum T6ZoneError {
    Io(std::io::Error),
    Header(ext_t6_fastfile::FileHeaderError),
    ChunkOverrun(usize),
    Inflate { chunk: usize, error: String },
}

impl std::fmt::Display for T6ZoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "reading zone: {e}"),
            Self::Header(e) => write!(f, "bad T6 fastfile header: {e:?}"),
            Self::ChunkOverrun(at) => write!(f, "T6 chunk at {at:#x} runs past end of file"),
            Self::Inflate { chunk, error } => {
                write!(f, "T6 chunk {chunk}: inflate failed: {error}")
            }
        }
    }
}

impl std::error::Error for T6ZoneError {}

pub fn open_t6_zone(path: impl AsRef<Path>) -> Result<T6ZoneImage, T6ZoneError> {
    let bytes = std::fs::read(path).map_err(T6ZoneError::Io)?;
    parse_t6_zone_image(&bytes)
}

pub fn parse_t6_zone_image(bytes: &[u8]) -> Result<T6ZoneImage, T6ZoneError> {
    let header = ext_t6_fastfile::parse_file_header(bytes).map_err(T6ZoneError::Header)?;
    let mut cipher = ext_t6_fastfile::ZoneCipher::new(header.name());
    let mut out = Vec::with_capacity(bytes.len() * 3);
    let mut plain = Vec::new();
    // Each chunk inflates into `scratch` first: `decompress_vec` initializes
    // all spare capacity of the vector it writes to, and the image's spare
    // capacity is hundreds of megabytes.
    let mut scratch = Vec::new();
    for chunk in ext_t6_fastfile::Chunks::new(bytes) {
        let chunk = chunk.map_err(T6ZoneError::ChunkOverrun)?;
        plain.clear();
        plain.extend_from_slice(chunk.bytes);
        cipher.decrypt(chunk.index, &mut plain);
        scratch.clear();
        inflate_raw_into(&plain, &mut scratch).map_err(|error| T6ZoneError::Inflate {
            chunk: chunk.index,
            error,
        })?;
        out.extend_from_slice(&scratch);
    }
    Ok(T6ZoneImage { header, bytes: out })
}

fn inflate_raw_into(input: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let mut decoder = flate2::Decompress::new(false);
    loop {
        out.reserve((input.len() * 4).max(64 * 1024));
        let before = (decoder.total_in(), decoder.total_out());
        let status = decoder
            .decompress_vec(
                &input[decoder.total_in() as usize..],
                out,
                flate2::FlushDecompress::Finish,
            )
            .map_err(|e| e.to_string())?;
        if status == flate2::Status::StreamEnd {
            return Ok(());
        }
        if before == (decoder.total_in(), decoder.total_out()) {
            return Err("truncated deflate stream".into());
        }
    }
}

fn native_error(error: impl std::fmt::Debug) -> ZoneOpenError {
    ZoneOpenError::Family {
        family: FamilyId::new("t6"),
        message: format!("{error:?}"),
    }
}
