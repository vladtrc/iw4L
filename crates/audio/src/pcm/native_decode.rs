use super::DecodeError;
use crate::decode_budget::DecodeSamples;
use crate::media::{PcmBuffer, PcmError};
use asset_audio::{AudioContainer, AudioDecodePlan, FamilyId, LoadedSoundPcm, NativeAudioError};

fn native_error(error: NativeAudioError) -> DecodeError {
    match error {
        NativeAudioError::MemoryLimit => DecodeError::Pcm(PcmError::MemoryLimit),
        NativeAudioError::Read => DecodeError::Read,
        error => DecodeError::Native(error),
    }
}
fn prepare_container(container: AudioContainer<'_>) -> Result<PcmBuffer, DecodeError> {
    let pcm = super::decode_audio_bytes(&container.bytes)?;
    if container
        .channels
        .is_some_and(|channels| channels != pcm.channels())
    {
        return Err(DecodeError::MetadataMismatch);
    }
    match container.frames {
        Some(frames) => pcm.prefix_frames(frames as usize).map_err(DecodeError::Pcm),
        None => Ok(pcm),
    }
}
pub(crate) fn prepare_loaded(sound: &LoadedSoundPcm) -> Result<PcmBuffer, DecodeError> {
    if let Some(source) = &sound.external_media {
        return super::external_media::prepare(source);
    }
    let channels = u16::try_from(sound.channels())
        .map_err(|_| DecodeError::Pcm(PcmError::UnsupportedChannels))?;
    PcmBuffer::validate_geometry(channels, sound.rate).map_err(DecodeError::Pcm)?;
    let decoder = asset_audio::audio_family(sound.game())
        .ok_or(DecodeError::Decode)?
        .decoder;
    let mut samples = DecodeSamples::new().map_err(DecodeError::Pcm)?;
    match decoder.prepare(sound, &mut samples).map_err(native_error)? {
        AudioDecodePlan::Pcm => {
            PcmBuffer::from_zone(sound.encoded_shared(), sound.bits(), channels, sound.rate)
                .map_err(|error| match error {
                    PcmError::Empty => DecodeError::Decode,
                    error => DecodeError::Pcm(error),
                })
        }
        AudioDecodePlan::Container(container) => prepare_container(container),
        AudioDecodePlan::Decoded => samples
            .into_pcm(channels, sound.rate)
            .map_err(DecodeError::Pcm),
    }
}
pub(crate) fn prepare_streamed(family: FamilyId, bytes: &[u8]) -> Result<PcmBuffer, DecodeError> {
    let decoder = asset_audio::audio_family(family)
        .ok_or(DecodeError::Decode)?
        .decoder;
    let mut scratch = DecodeSamples::new().map_err(DecodeError::Pcm)?;
    prepare_container(
        decoder
            .prepare_streamed(bytes, &mut scratch)
            .map_err(native_error)?,
    )
}
