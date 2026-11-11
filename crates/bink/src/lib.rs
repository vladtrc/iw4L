mod audio;
mod bits;
mod tables;
mod video;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Unsupported(String),
    Format(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "bink: {e}"),
            Self::Unsupported(what) => write!(f, "bink: unsupported {what}"),
            Self::Format(what) => write!(f, "bink: corrupt stream ({what})"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioTrack {
    pub rate: u32,
    pub channels: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub revision: u8,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frames: u32,
    pub audio: Option<AudioTrack>,
}

const FLAG_ALPHA: u32 = 0x0010_0000;
const AUD_STEREO: u16 = 0x2000;
const AUD_DCT: u16 = 0x1000;

pub struct Bink {
    file: File,
    header: Header,
    index: Vec<(u64, u32)>,
    next: usize,
    video: video::Video,
    audio: Option<audio::Audio>,
    tracks: usize,
    packet: Vec<u8>,
}

impl Bink {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let mut h = [0u8; 44];
        file.read_exact(&mut h)?;
        let word = |i: usize| u32::from_le_bytes(h[i * 4..i * 4 + 4].try_into().unwrap());
        let revision = h[3];
        if &h[..3] != b"BIK" || !(b'f'..=b'i').contains(&revision) {
            return Err(Error::Unsupported(format!(
                "container {:?}",
                String::from_utf8_lossy(&h[..4])
            )));
        }
        let file_size = u64::from(word(1)) + 8;
        let frames = word(2);
        let (width, height) = (word(5), word(6));
        let (fps_num, fps_den) = (word(7), word(8));
        let flags = word(9);
        let tracks = word(10) as usize;
        if frames == 0 || frames > 1_000_000 || fps_num == 0 || fps_den == 0 || tracks > 256 {
            return Err(Error::Format("header"));
        }
        if !(1..=7680).contains(&width) || !(1..=4800).contains(&height) {
            return Err(Error::Format("dimensions"));
        }

        let mut audio_track = None;
        if tracks > 0 {
            let mut t = vec![0u8; tracks * 12];
            file.read_exact(&mut t)?;
            let rate = u16::from_le_bytes([t[tracks * 4], t[tracks * 4 + 1]]);
            let aflags = u16::from_le_bytes([t[tracks * 4 + 2], t[tracks * 4 + 3]]);
            if rate > 0 {
                let channels = if aflags & AUD_STEREO != 0 { 2 } else { 1 };
                audio_track = Some((
                    AudioTrack {
                        rate: u32::from(rate),
                        channels,
                    },
                    aflags & AUD_DCT != 0,
                ));
            }
        }

        let mut raw = vec![0u8; frames as usize * 4];
        file.read_exact(&mut raw)?;
        let offsets: Vec<u64> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| u64::from(u32::from_le_bytes(c) & !1))
            .chain(std::iter::once(file_size))
            .collect();
        let mut index = Vec::with_capacity(frames as usize);
        for w in offsets.windows(2) {
            if w[1] <= w[0] {
                return Err(Error::Format("frame index"));
            }
            index.push((w[0], (w[1] - w[0]) as u32));
        }

        let header = Header {
            revision,
            width,
            height,
            fps_num,
            fps_den,
            frames,
            audio: audio_track.map(|(a, _)| a),
        };
        Ok(Self {
            file,
            video: video::Video::new(
                width as usize,
                height as usize,
                revision,
                flags & FLAG_ALPHA != 0,
            ),
            audio: audio_track.map(|(a, dct)| audio::Audio::new(a.rate, a.channels, dct)),
            header,
            index,
            next: 0,
            tracks,
            packet: Vec::new(),
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn next_frame(&mut self, audio: &mut Vec<f32>) -> Result<bool, Error> {
        let Some(&(offset, size)) = self.index.get(self.next) else {
            return Ok(false);
        };
        self.next += 1;
        self.packet.resize(size as usize, 0);
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut self.packet)?;

        let mut at = 0usize;
        for track in 0..self.tracks {
            let len = self
                .packet
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
                .ok_or(Error::Format("audio size"))?;
            let data = self
                .packet
                .get(at + 4..at + 4 + len)
                .ok_or(Error::Format("audio packet"))?;
            if track == 0
                && len >= 4
                && let Some(dec) = &mut self.audio
            {
                dec.decode(data, audio)?;
            }
            at += 4 + len;
        }
        self.video.decode(&self.packet[at..])?;
        Ok(true)
    }

    pub fn rgba(&self, out: &mut [u8]) {
        let (w, h) = (self.header.width as usize, self.header.height as usize);
        video::to_rgba(&self.video.cur, w, h, out);
    }
}
