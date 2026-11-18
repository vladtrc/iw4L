use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalAudioCodec {
    PcmS16,
    Flac,
    Unsupported(u32),
}

/// A validated native bank entry lowered to a byte range and standard audio geometry.
#[derive(Clone, Debug)]
pub struct ExternalAudioSource {
    pub path: PathBuf,
    pub offset: u64,
    pub size: u64,
    pub frames: u32,
    pub rate: u32,
    pub channels: u16,
    pub codec: ExternalAudioCodec,
}
