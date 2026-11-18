use super::{XwmaDecodeError, XwmaPcmSink};
use asset_audio::*;

pub struct T5AudioDecoder;
impl AudioDecoder for T5AudioDecoder {
    fn encoding(&self, sound: &LoadedSoundPcm) -> Option<LoadedAudioEncoding> {
        match sound.format() {
            MSS_PCM => Some(LoadedAudioEncoding::Pcm),
            6 => Some(LoadedAudioEncoding::Container),
            super::T5_WMA => Some(LoadedAudioEncoding::Native),
            _ => None,
        }
    }
    fn prepare<'a>(
        &self,
        sound: &'a LoadedSoundPcm,
        sink: &mut dyn AudioPcmSink,
    ) -> Result<AudioDecodePlan<'a>, NativeAudioError> {
        match sound.format() {
            MSS_PCM => Ok(AudioDecodePlan::Pcm),
            6 => adpcm(
                sound.encoded_bytes(),
                sound.samples,
                sound.rate,
                sound.channels() as u32,
                sink,
            )
            .map(AudioDecodePlan::Container),
            super::T5_WMA => {
                super::decode_t5_xwma(
                    sound.encoded_bytes(),
                    &sound.seek_table,
                    sound.channels() as u32,
                    sound.rate,
                    &mut WmaSink(sink),
                )
                .map_err(native_error)?;
                Ok(AudioDecodePlan::Decoded)
            }
            format => Err(NativeAudioError::UnsupportedFormat(format)),
        }
    }
    fn prepare_streamed<'a>(
        &self,
        bytes: &'a [u8],
        sink: &mut dyn AudioPcmSink,
    ) -> Result<AudioContainer<'a>, NativeAudioError> {
        if bytes.starts_with(b"RIFF") {
            return Ok(AudioContainer::detected(bytes));
        }
        let field = |offset: usize| -> Option<u32> {
            Some(u32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        let (data, frames, rate, channels) = (|| {
            if field(0)? != 1 || field(28)? != 6 {
                return None;
            }
            let header = field(16)? as usize;
            if header < 56 {
                return None;
            }
            let size = field(48)? as usize;
            Some((
                bytes.get(header..header.checked_add(size)?)?,
                field(4)?,
                field(8)?,
                field(12)?,
            ))
        })()
        .ok_or_else(|| NativeAudioError::Decode("invalid T5 stream header".into()))?;
        adpcm(data, frames, rate, channels, sink)
    }
}
fn native_error(error: XwmaDecodeError) -> NativeAudioError {
    match error {
        XwmaDecodeError::MemoryLimit => NativeAudioError::MemoryLimit,
        XwmaDecodeError::CacheRead => NativeAudioError::Read,
        error => NativeAudioError::Decode(error.to_string()),
    }
}
struct WmaSink<'a>(&'a mut dyn AudioPcmSink);
impl XwmaPcmSink for WmaSink<'_> {
    type Workspace = Box<dyn Send>;
    fn reserve_workspace(&mut self, bytes: usize) -> Result<Self::Workspace, XwmaDecodeError> {
        self.0
            .reserve_workspace(bytes)
            .map_err(|_| XwmaDecodeError::MemoryLimit)
    }
    fn extend(&mut self, samples: &[i16]) -> Result<(), XwmaDecodeError> {
        self.0
            .extend(samples)
            .map_err(|_| XwmaDecodeError::MemoryLimit)
    }
    fn sample_count(&self) -> usize {
        self.0.sample_count()
    }
    fn visit_samples(
        &self,
        visitor: &mut dyn FnMut(&[i16]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.0.visit_samples(visitor)
    }
}
fn adpcm<'a>(
    data: &[u8],
    frames: u32,
    rate: u32,
    channels: u32,
    sink: &mut dyn AudioPcmSink,
) -> Result<AudioContainer<'a>, NativeAudioError> {
    let reservation = sink.reserve_workspace(
        data.len()
            .checked_add(90)
            .ok_or(NativeAudioError::MemoryLimit)?,
    )?;
    let wav = (|| {
        let size = data.len();
        if !(1..=2).contains(&channels) || rate == 0 || size == 0 {
            return None;
        }
        let align = 262 * channels;
        if !size.is_multiple_of(align as usize)
            || frames == 0
            || u64::from(frames) > (size as u64 / u64::from(align)) * 512
        {
            return None;
        }
        let mut wav = Vec::with_capacity(size + 90);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(82_u32.checked_add(size.try_into().ok()?)?).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&50_u32.to_le_bytes());
        for value in [2_u16, channels as u16] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate.checked_mul(align)? / 512).to_le_bytes());
        for value in [align as u16, 4, 32, 512, 7] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        for (a, b) in [
            (256_i16, 0_i16),
            (512, -256),
            (0, 0),
            (192, 64),
            (240, 0),
            (460, -208),
            (392, -232),
        ] {
            wav.extend_from_slice(&a.to_le_bytes());
            wav.extend_from_slice(&b.to_le_bytes());
        }
        wav.extend_from_slice(b"fact");
        wav.extend_from_slice(&4_u32.to_le_bytes());
        wav.extend_from_slice(&frames.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(size as u32).to_le_bytes());
        wav.extend_from_slice(data);
        Some(wav)
    })()
    .ok_or_else(|| NativeAudioError::Decode("invalid T5 ADPCM geometry".into()))?;
    Ok(AudioContainer {
        bytes: std::borrow::Cow::Owned(wav),
        frames: Some(frames),
        channels: Some(channels as u16),
        workspace: Some(reservation),
    })
}
