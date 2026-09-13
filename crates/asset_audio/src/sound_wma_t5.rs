use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::process::{Command, Stdio};

pub const T5_WMA: i32 = 7;

const WMAUDIO2: u16 = 353;
const STEREO_BLOCK_ALIGN: u16 = 4096;
const MONO_BLOCK_ALIGN: u16 = 2230;
const STDERR_CAP: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XwmaDecodeError {
    Mux,
    FfmpegMissing,
    Spawn(String),
    Pipe(&'static str),
    Wait,
    Process { status: Option<i32>, stderr: String },
    EmptyPcm,
}

impl XwmaDecodeError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::FfmpegMissing | Self::Spawn(_) | Self::Pipe(_) | Self::Wait
        )
    }
}

impl fmt::Display for XwmaDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mux => f.write_str("xwma mux"),
            Self::FfmpegMissing => f.write_str("ffmpeg missing"),
            Self::Spawn(e) => write!(f, "ffmpeg spawn: {e}"),
            Self::Pipe(side) => write!(f, "ffmpeg {side}"),
            Self::Wait => f.write_str("ffmpeg wait"),
            Self::Process { status, stderr } => {
                write!(f, "ffmpeg status={status:?}")?;
                if !stderr.is_empty() {
                    write!(f, " stderr={stderr}")?;
                }
                Ok(())
            }
            Self::EmptyPcm => f.write_str("ffmpeg empty pcm"),
        }
    }
}

pub fn wma_block_align(channels: u32) -> u16 {
    if channels == 1 {
        MONO_BLOCK_ALIGN
    } else {
        STEREO_BLOCK_ALIGN
    }
}

pub fn mux_t5_xwma(
    packets: &[u8],
    seek_table: &[u32],
    channels: u32,
    rate: u32,
) -> Option<Vec<u8>> {
    if packets.is_empty() || seek_table.is_empty() || channels == 0 || rate == 0 {
        return None;
    }
    let block_align = wma_block_align(channels) as usize;
    let audio_bytes = seek_table.len().checked_mul(block_align)?;
    if packets.len() < audio_bytes {
        return None;
    }
    let data = &packets[..audio_bytes];
    let fmt_size: u32 = 18;
    let dpds_size: u32 = (seek_table.len() * 4) as u32;
    let data_size: u32 = audio_bytes as u32;

    let riff_size: u32 = 4 + (8 + fmt_size) + (8 + dpds_size) + (8 + data_size);
    let mut out = Vec::with_capacity(8 + riff_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"XWMA");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_size.to_le_bytes());
    out.extend_from_slice(&WMAUDIO2.to_le_bytes());
    out.extend_from_slice(&(channels as u16).to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(6000u32.saturating_mul(channels)).to_le_bytes());
    out.extend_from_slice(&(block_align as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"dpds");
    out.extend_from_slice(&dpds_size.to_le_bytes());
    for n in seek_table {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(data);
    Some(out)
}

fn clip_stderr(bytes: &[u8]) -> String {
    let slice = if bytes.len() > STDERR_CAP {
        &bytes[..STDERR_CAP]
    } else {
        bytes
    };
    String::from_utf8_lossy(slice).into_owned()
}

pub fn decode_t5_xwma(
    packets: &[u8],
    seek_table: &[u32],
    channels: u32,
    rate: u32,
) -> Result<Vec<u8>, XwmaDecodeError> {
    let xwma = mux_t5_xwma(packets, seek_table, channels, rate).ok_or(XwmaDecodeError::Mux)?;
    let ch = channels.max(1).to_string();
    let ar = rate.max(1).to_string();
    let mut child = match Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "xwma",
            "-i",
            "pipe:0",
            "-f",
            "s16le",
            "-ac",
            &ch,
            "-ar",
            &ar,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(XwmaDecodeError::FfmpegMissing);
        }
        Err(e) => return Err(XwmaDecodeError::Spawn(e.to_string())),
    };
    let mut stdin = child.stdin.take().ok_or(XwmaDecodeError::Pipe("stdin"))?;
    let mut stdout = child.stdout.take().ok_or(XwmaDecodeError::Pipe("stdout"))?;
    let mut stderr = child.stderr.take().ok_or(XwmaDecodeError::Pipe("stderr"))?;

    let write = std::thread::spawn(move || stdin.write_all(&xwma));
    let mut pcm = Vec::new();
    stdout
        .read_to_end(&mut pcm)
        .map_err(|_| XwmaDecodeError::Pipe("stdout-read"))?;
    let mut err = Vec::new();
    let _ = stderr.read_to_end(&mut err);
    write
        .join()
        .map_err(|_| XwmaDecodeError::Pipe("stdin-join"))?
        .map_err(|_| XwmaDecodeError::Pipe("stdin-write"))?;
    let status = child.wait().map_err(|_| XwmaDecodeError::Wait)?;
    if !status.success() {
        return Err(XwmaDecodeError::Process {
            status: status.code(),
            stderr: clip_stderr(&err),
        });
    }
    if pcm.is_empty() {
        return Err(XwmaDecodeError::EmptyPcm);
    }
    Ok(pcm)
}
