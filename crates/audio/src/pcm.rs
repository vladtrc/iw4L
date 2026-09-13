#[path = "t5_stream.rs"]
pub(crate) mod t5_stream;

use std::sync::atomic::{AtomicU32, Ordering};
use std::{num::NonZero, sync::Arc, time::Duration};

use bevy::{
    audio::{ChannelCount, Decodable, PlaybackSettings, SampleRate, Source, Volume},
    prelude::*,
};

#[derive(Clone, Debug)]
pub struct LivePan {
    left: Arc<AtomicU32>,
    right: Arc<AtomicU32>,
}

impl LivePan {
    pub fn unity() -> Self {
        Self::new(1.0, 1.0)
    }

    pub fn new(left: f32, right: f32) -> Self {
        Self {
            left: Arc::new(AtomicU32::new(left.to_bits())),
            right: Arc::new(AtomicU32::new(right.to_bits())),
        }
    }

    pub fn set(&self, left: f32, right: f32) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> (f32, f32) {
        (
            f32::from_bits(self.left.load(Ordering::Relaxed)),
            f32::from_bits(self.right.load(Ordering::Relaxed)),
        )
    }
}

#[derive(Asset, TypePath, Clone)]
pub struct PcmAudio {
    samples: Arc<[f32]>,
    channels: u16,
    sample_rate: u32,
    live_pan: Option<LivePan>,
}

#[derive(Asset, TypePath, Clone)]
pub struct LoopingPcmAudio(PcmAudio);

#[derive(Bundle)]
pub struct LoopingPcmPlayback {
    player: AudioPlayer<LoopingPcmAudio>,
    settings: PlaybackSettings,
}

impl LoopingPcmPlayback {
    pub fn new(handle: Handle<LoopingPcmAudio>, volume: Volume) -> Self {
        Self {
            player: AudioPlayer(handle),
            settings: PlaybackSettings::ONCE.with_volume(volume),
        }
    }
}

impl PcmAudio {
    pub(crate) fn from_prepared(
        samples: Arc<[f32]>,
        channels: u16,
        sample_rate: u32,
    ) -> Option<Self> {
        if samples.is_empty() || channels == 0 || sample_rate == 0 {
            return None;
        }
        Some(Self {
            samples,
            channels,
            sample_rate,
            live_pan: None,
        })
    }

    pub(crate) fn samples(&self) -> &Arc<[f32]> {
        &self.samples
    }

    pub(crate) fn channel_count(&self) -> u16 {
        self.channels
    }

    pub(crate) fn rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn with_live_pan(&self) -> Self {
        Self {
            samples: Arc::clone(&self.samples),
            channels: self.channels,
            sample_rate: self.sample_rate,
            live_pan: Some(LivePan::unity()),
        }
    }

    pub fn live_pan(&self) -> Option<&LivePan> {
        self.live_pan.as_ref()
    }

    pub fn into_looping(self) -> LoopingPcmAudio {
        LoopingPcmAudio(self)
    }

    fn decoder_with_looping(&self, looping: bool) -> PcmDecoder {
        PcmDecoder {
            samples: Arc::clone(&self.samples),
            pos: 0,
            src_channels: self.channels,
            sample_rate: self.sample_rate,
            live: self.live_pan.clone(),
            pending_right: None,
            looping,
        }
    }
}

impl LoopingPcmAudio {
    pub fn live_pan(&self) -> Option<&LivePan> {
        self.0.live_pan()
    }
}

pub struct PcmDecoder {
    samples: Arc<[f32]>,

    pos: usize,
    src_channels: u16,
    sample_rate: u32,
    live: Option<LivePan>,
    pending_right: Option<f32>,
    looping: bool,
}

impl PcmDecoder {
    fn remaining_out(&self) -> usize {
        if self.live.is_some() {
            let extra = usize::from(self.pending_right.is_some());
            let ch = self.src_channels.max(1) as usize;
            let frames_left = self.samples.len().saturating_sub(self.pos) / ch;
            extra + frames_left * 2
        } else {
            self.samples.len().saturating_sub(self.pos)
        }
    }

    fn rewind_for_loop(&mut self) -> bool {
        if self.pos < self.samples.len() {
            return true;
        }
        if !self.looping || self.samples.is_empty() {
            return false;
        }
        self.pos = 0;
        true
    }
}

impl Iterator for PcmDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.pending_right.take() {
            return Some(right);
        }
        if !self.rewind_for_loop() {
            return None;
        }
        let Some(live) = &self.live else {
            let sample = self.samples[self.pos];
            self.pos += 1;
            return Some(sample);
        };
        let (gain_l, gain_r) = live.get();
        let ch = self.src_channels.max(1) as usize;
        let left = *self.samples.get(self.pos)?;
        let right = self.samples.get(self.pos + 1).copied().unwrap_or(left);
        self.pos += ch;
        self.pending_right = Some(right * gain_r);
        Some(left * gain_l)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.looping {
            return (0, None);
        }
        let remaining = self.remaining_out();
        (remaining, Some(remaining))
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        (!self.looping).then(|| self.remaining_out())
    }

    fn channels(&self) -> ChannelCount {
        let n = if self.live.is_some() {
            2
        } else {
            self.src_channels.max(1)
        };
        NonZero::new(n).unwrap_or(NonZero::new(1).unwrap())
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.sample_rate).unwrap_or(NonZero::new(22_050).unwrap())
    }

    fn total_duration(&self) -> Option<Duration> {
        if self.looping {
            return None;
        }
        let frames = self.samples.len() as u64 / self.src_channels.max(1) as u64;
        Some(Duration::from_secs_f64(
            frames as f64 / self.sample_rate.max(1) as f64,
        ))
    }
}

impl Decodable for PcmAudio {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> Self::Decoder {
        self.decoder_with_looping(false)
    }
}

impl Decodable for LoopingPcmAudio {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> Self::Decoder {
        self.0.decoder_with_looping(true)
    }
}

pub fn decode_audio_bytes(bytes: &[u8]) -> Option<PcmAudio> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut samples: Vec<f32> = Vec::new();
    let mut channels: u16 = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::ResetRequired) => continue,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(_) => continue,
        };
        channels = channels.max(decoded.spec().channels.count() as u16);
        let mut interleaved = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        interleaved.copy_interleaved_ref(decoded);
        samples.extend_from_slice(interleaved.samples());
    }
    if samples.is_empty() || channels == 0 {
        return None;
    }
    Some(PcmAudio {
        samples: samples.into(),
        channels,
        sample_rate,
        live_pan: None,
    })
}
