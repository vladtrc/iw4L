use crate::LoadedSoundPcm;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub enum NativeAudioError {
    Decode(String),
    MemoryLimit,
    Read,
    UnsupportedFormat(i32),
}

impl std::fmt::Display for NativeAudioError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(error) => output.write_str(error),
            Self::MemoryLimit => output.write_str("audio decode memory limit"),
            Self::Read => output.write_str("audio decode read failed"),
            Self::UnsupportedFormat(format) => {
                write!(output, "unsupported native audio format {format}")
            }
        }
    }
}
impl std::error::Error for NativeAudioError {}

/// Reservations stay live until the native decoder or container conversion releases them.
pub trait AudioPcmSink {
    fn reserve_workspace(&mut self, bytes: usize) -> Result<Box<dyn Send>, NativeAudioError>;
    fn extend(&mut self, samples: &[i16]) -> Result<(), NativeAudioError>;
    fn sample_count(&self) -> usize;
    fn visit_samples(
        &self,
        visitor: &mut dyn FnMut(&[i16]) -> std::io::Result<()>,
    ) -> std::io::Result<()>;
}

pub struct AudioContainer<'a> {
    pub bytes: Cow<'a, [u8]>,
    pub frames: Option<u32>,
    pub channels: Option<u16>,
    pub workspace: Option<Box<dyn Send>>,
}
impl<'a> AudioContainer<'a> {
    pub fn detected(bytes: &'a [u8]) -> Self {
        Self {
            bytes: Cow::Borrowed(bytes),
            frames: None,
            channels: None,
            workspace: None,
        }
    }
}
#[derive(Clone, Copy)]
pub enum LoadedAudioEncoding {
    Pcm,
    Container,
    Native,
}
pub enum AudioDecodePlan<'a> {
    Pcm,
    Container(AudioContainer<'a>),
    Decoded,
}

pub trait AudioDecoder: Send + Sync {
    fn encoding(&self, sound: &LoadedSoundPcm) -> Option<LoadedAudioEncoding>;
    fn prepare<'a>(
        &self,
        sound: &'a LoadedSoundPcm,
        sink: &mut dyn AudioPcmSink,
    ) -> Result<AudioDecodePlan<'a>, NativeAudioError>;
    fn prepare_streamed<'a>(
        &self,
        bytes: &'a [u8],
        _: &mut dyn AudioPcmSink,
    ) -> Result<AudioContainer<'a>, NativeAudioError> {
        Ok(AudioContainer::detected(bytes))
    }
}
pub struct PcmAudioDecoder;
impl AudioDecoder for PcmAudioDecoder {
    fn encoding(&self, sound: &LoadedSoundPcm) -> Option<LoadedAudioEncoding> {
        (sound.format() == crate::MSS_PCM).then_some(LoadedAudioEncoding::Pcm)
    }
    fn prepare<'a>(
        &self,
        sound: &'a LoadedSoundPcm,
        _: &mut dyn AudioPcmSink,
    ) -> Result<AudioDecodePlan<'a>, NativeAudioError> {
        if sound.format() == crate::MSS_PCM {
            Ok(AudioDecodePlan::Pcm)
        } else {
            Err(NativeAudioError::UnsupportedFormat(sound.format()))
        }
    }
}
