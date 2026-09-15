use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use asset_transport::{
    cache_flight, cache_get, cache_put, ensure_artifacts_dir, fnv1a64, fnv1a64_more,
};

pub const T5_WMA: i32 = 7;

/// Bumped whenever the bytes this decoder hands back for the same input
/// change. It is part of the key, so an entry written by an older decoder is
/// never read back as a current one — it is left to the cache sweep instead.
const XWMA_CACHE_FORMAT: u32 = 1;
const XWMA_CACHE_KIND: &str = "xwma_pcm";
const XWMA_CACHE_MAGIC: &[u8; 8] = b"IWLXWMA\n";
/// Magic, format, channels, rate, byte length — then the samples themselves.
const XWMA_CACHE_HEADER: usize = 8 + 4 * 4;

/// How many clips one external process decodes.
///
/// What this batching removes is process startup, not decoding: `ffmpeg` takes
/// ~40 ms to come up and ~2 ms to decode an average clip of these zones, so the
/// number only has to be large enough to amortise the first against the second.
/// It is bounded because a batch is one command line — 64 clips is ~8 KiB of
/// arguments, well inside the 32 KiB a Windows command line allows — and
/// because a batch that fails costs at most this many lone retries.
const XWMA_BATCH: usize = 64;

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
    Process {
        status: Option<i32>,
        stderr: String,
    },
    EmptyPcm,
    /// The batch had nowhere on disk to hand its clips over.
    Scratch(String),
}

impl XwmaDecodeError {
    /// Whether the batch this ended is worth asking again one clip at a time.
    ///
    /// Only a process that *ran* and refused says anything about the clips: it
    /// opened one input it could not read and took the other sixty-three down
    /// with it, and they deserve to be asked on their own. An `ffmpeg` that is
    /// missing, unspawnable, or has nowhere to write is a fact about the
    /// machine, and retrying it per clip only multiplies the same failure.
    fn worth_retrying_alone(&self) -> bool {
        matches!(self, Self::Process { .. } | Self::Pipe(_) | Self::Wait)
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
            Self::Scratch(e) => write!(f, "xwma scratch: {e}"),
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

/// One clip a batch was asked about: the zone's own bytes, and the two
/// numbers the samples have to come back at.
#[derive(Clone, Copy, Debug)]
pub struct XwmaClip<'a> {
    pub packets: &'a [u8],
    pub seek_table: &'a [u32],
    pub channels: u32,
    pub rate: u32,
}

/// Decode one T5 XWMA clip to interleaved signed 16-bit LE samples, through
/// the artifact cache.
///
/// A batch of one, which is what a clip asked for on its own is: no temporary
/// files, one `ffmpeg` over a pipe, exactly as before batching existed. The
/// clips a match prepares come through [`decode_t5_xwma_batch`] instead.
pub fn decode_t5_xwma(
    packets: &[u8],
    seek_table: &[u32],
    channels: u32,
    rate: u32,
) -> Result<Vec<u8>, XwmaDecodeError> {
    let clip = XwmaClip {
        packets,
        seek_table,
        channels,
        rate,
    };
    let mut results = decode_t5_xwma_batch(std::slice::from_ref(&clip));
    debug_assert_eq!(results.len(), 1, "one clip in, one result out");
    results.pop().unwrap_or(Err(XwmaDecodeError::EmptyPcm))
}

/// Decode a set of T5 XWMA clips, and answer each of them in the order asked.
///
/// The clips the cache already holds cost a read. The rest are decoded by an
/// external `ffmpeg` — but by *one* process for up to [`XWMA_BATCH`] of them
/// rather than one process each, because starting `ffmpeg` costs twenty times
/// what decoding one of these clips costs. Only a decode that produced samples
/// is stored: a failure is counted and left uncached, because `ffmpeg` missing
/// from the PATH is a fact about this machine this minute and not about the
/// clip.
pub fn decode_t5_xwma_batch(clips: &[XwmaClip<'_>]) -> Vec<Result<Vec<u8>, XwmaDecodeError>> {
    let key_at = Instant::now();
    let keys: Vec<String> = clips
        .iter()
        .map(|clip| cache_key(clip.packets, clip.seek_table, clip.channels, clip.rate))
        .collect();
    KEY_NS.fetch_add(key_at.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let mut out: Vec<Option<Result<Vec<u8>, XwmaDecodeError>>> = vec![None; clips.len()];
    let mut pending = Vec::new();
    for (i, clip) in clips.iter().enumerate() {
        match cached(&keys[i], clip.channels, clip.rate) {
            Some(pcm) => out[i] = Some(Ok(pcm)),
            None => pending.push(i),
        }
    }

    // One decode per key. The clip walk reaches the same payload from more than
    // one alias and more than one namespace, and two `ffmpeg` runs for one clip
    // is exactly the cost this cache exists to remove — so the second asker
    // waits for the first and then reads what it wrote. Two batches can overlap
    // on several keys, so they are taken in key order: whoever is first on the
    // lowest key is first on all of them, and neither ends up holding what the
    // other is waiting for.
    pending.sort_unstable_by(|a, b| keys[*a].cmp(&keys[*b]));
    let mut flights = Vec::new();
    let mut leaders = Vec::new();
    let mut followers = Vec::new();
    let mut leader_of: HashMap<&str, usize> = HashMap::new();
    for &i in &pending {
        match leader_of.get(keys[i].as_str()) {
            // The same payload twice inside one batch. It is one decode, and
            // the second clip reads the answer rather than waiting on a flight
            // this thread is itself holding.
            Some(&leader) => followers.push((i, leader)),
            None => {
                leader_of.insert(keys[i].as_str(), i);
                flights.push(cache_flight(XWMA_CACHE_KIND, &keys[i]));
                leaders.push(i);
            }
        }
    }

    let mut muxed: Vec<(usize, Vec<u8>)> = Vec::new();
    for &i in &leaders {
        // Asked again now the flight is ours: whoever held it before may have
        // been decoding this very clip.
        if let Some(pcm) = cached(&keys[i], clips[i].channels, clips[i].rate) {
            out[i] = Some(Ok(pcm));
            continue;
        }
        match mux_t5_xwma(
            clips[i].packets,
            clips[i].seek_table,
            clips[i].channels,
            clips[i].rate,
        ) {
            Some(xwma) => muxed.push((i, xwma)),
            None => out[i] = Some(Err(XwmaDecodeError::Mux)),
        }
    }

    let decode_at = Instant::now();
    for chunk in muxed.chunks(XWMA_BATCH) {
        for (i, decoded) in decode_chunk(chunk, clips) {
            if let Ok(pcm) = &decoded {
                MISS.fetch_add(1, Ordering::Relaxed);
                PCM_BYTES.fetch_add(pcm.len() as u64, Ordering::Relaxed);
                store(&keys[i], clips[i].channels, clips[i].rate, pcm);
            }
            out[i] = Some(decoded);
        }
    }
    DECODE_NS.fetch_add(decode_at.elapsed().as_nanos() as u64, Ordering::Relaxed);

    for (i, leader) in followers {
        let answer = match &out[leader] {
            Some(Ok(pcm)) => {
                HIT.fetch_add(1, Ordering::Relaxed);
                PCM_BYTES.fetch_add(pcm.len() as u64, Ordering::Relaxed);
                Ok(pcm.clone())
            }
            Some(Err(error)) => Err(error.clone()),
            None => Err(XwmaDecodeError::EmptyPcm),
        };
        out[i] = Some(answer);
    }

    out.into_iter()
        .map(|answer| {
            let answer = answer.unwrap_or(Err(XwmaDecodeError::EmptyPcm));
            if answer.is_err() {
                FAILED.fetch_add(1, Ordering::Relaxed);
            }
            answer
        })
        .collect()
}

/// Decode one process' worth of clips, and ask again one at a time if that
/// process failed.
///
/// `ffmpeg` opens every input before it decodes any of them, so one clip it
/// refuses ends the batch before a single sample is written and takes its
/// sixty-three neighbours with it. They are not guilty of anything, so they are
/// asked again on their own — which is the old cost for a batch that was going
/// to fail anyway, and leaves the one bad clip as the only failure.
fn decode_chunk(
    chunk: &[(usize, Vec<u8>)],
    clips: &[XwmaClip<'_>],
) -> Vec<(usize, Result<Vec<u8>, XwmaDecodeError>)> {
    let alone =
        |(i, xwma): &(usize, Vec<u8>)| (*i, run_ffmpeg(xwma, clips[*i].channels, clips[*i].rate));
    if chunk.len() == 1 {
        return chunk.iter().map(alone).collect();
    }
    match run_ffmpeg_batch(chunk, clips) {
        Ok(decoded) => chunk.iter().map(|(i, _)| *i).zip(decoded).collect(),
        Err(error) if error.worth_retrying_alone() => {
            RETRIED.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            diag::warn!(
                Audio,
                "xwma batch of {} refused ({error}) — asking again one clip at a time",
                chunk.len()
            );
            chunk.iter().map(alone).collect()
        }
        Err(error) => chunk
            .iter()
            .map(|(i, _)| (*i, Err(error.clone())))
            .collect(),
    }
}

/// The clip as the cache holds it: what was asked for, then what came back, so
/// an entry cannot be read as an answer to a different question.
fn cache_encode(channels: u32, rate: u32, pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(XWMA_CACHE_HEADER + pcm.len());
    out.extend_from_slice(XWMA_CACHE_MAGIC);
    out.extend_from_slice(&XWMA_CACHE_FORMAT.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// `None` for anything that is not this decoder answering this question: a
/// short read, another format, another clip's channels or rate, a truncated
/// tail. The caller decodes again and overwrites, which is the cheap outcome
/// — handing back samples at the wrong rate is not.
fn cache_decode(bytes: &[u8], channels: u32, rate: u32) -> Option<Vec<u8>> {
    if bytes.len() < XWMA_CACHE_HEADER || &bytes[..8] != XWMA_CACHE_MAGIC {
        return None;
    }
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if word(8)? != XWMA_CACHE_FORMAT || word(12)? != channels || word(16)? != rate {
        return None;
    }
    let len = word(20)? as usize;
    let pcm = bytes.get(XWMA_CACHE_HEADER..XWMA_CACHE_HEADER + len)?;
    (!pcm.is_empty()).then(|| pcm.to_vec())
}

fn cached(key: &str, channels: u32, rate: u32) -> Option<Vec<u8>> {
    let io_at = Instant::now();
    let bytes = cache_get(XWMA_CACHE_KIND, key);
    IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let pcm = cache_decode(&bytes?, channels, rate)?;
    HIT.fetch_add(1, Ordering::Relaxed);
    PCM_BYTES.fetch_add(pcm.len() as u64, Ordering::Relaxed);
    Some(pcm)
}

fn store(key: &str, channels: u32, rate: u32, pcm: &[u8]) {
    let io_at = Instant::now();
    if let Err(error) = cache_put(XWMA_CACHE_KIND, key, &cache_encode(channels, rate, pcm)) {
        diag::warn!(Audio, "xwma cache store {key}: {error}");
    }
    IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
}

/// The whole input, not the alias that named it: two aliases over one payload
/// are one decode, and one alias over a payload the zone changed is two.
fn cache_key(packets: &[u8], seek_table: &[u32], channels: u32, rate: u32) -> String {
    let mut hash = fnv1a64(&XWMA_CACHE_FORMAT.to_le_bytes());
    hash = fnv1a64_more(hash, &T5_WMA.to_le_bytes());
    hash = fnv1a64_more(hash, &channels.to_le_bytes());
    hash = fnv1a64_more(hash, &rate.to_le_bytes());
    hash = fnv1a64_more(hash, &(seek_table.len() as u64).to_le_bytes());
    for entry in seek_table {
        hash = fnv1a64_more(hash, &entry.to_le_bytes());
    }
    hash = fnv1a64_more(hash, &(packets.len() as u64).to_le_bytes());
    hash = fnv1a64_more(hash, packets);
    format!("{XWMA_CACHE_FORMAT:08x}-{hash:016x}")
}

static HIT: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);
static SPAWNED: AtomicU64 = AtomicU64::new(0);
static FAILED: AtomicU64 = AtomicU64::new(0);
static RETRIED: AtomicU64 = AtomicU64::new(0);
static PCM_BYTES: AtomicU64 = AtomicU64::new(0);
static DECODE_NS: AtomicU64 = AtomicU64::new(0);
static KEY_NS: AtomicU64 = AtomicU64::new(0);
static IO_NS: AtomicU64 = AtomicU64::new(0);

/// What the XWMA path cost this process: how much of it the cache answered,
/// how much of it an external process answered, and what each side charged.
///
/// The three durations are summed over whichever threads did the work — the
/// clip prep workers run several at a time — so they are worker time and not
/// a stretch of the load. Only their ratio to each other is safe to read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XwmaDecodeCost {
    /// Clips answered from the artifact cache, launching nothing.
    pub hit: u64,
    /// Clips decoded externally and then stored.
    pub miss: u64,
    /// External `ffmpeg` processes launched.
    pub spawned: u64,
    /// Decodes that produced no samples. Nothing is cached for these.
    pub failed: u64,
    /// Clips a refused batch sent back through a process of their own.
    pub retried: u64,
    /// Bytes of signed 16-bit samples handed back, cached or decoded.
    pub pcm_bytes: u64,
    /// Worker time inside the external decoder, handing it the clips and
    /// reading its samples back included.
    pub decode_ms: f64,
    /// Worker time hashing payloads into cache keys.
    pub key_ms: f64,
    /// Worker time reading and writing the cache.
    pub io_ms: f64,
}

pub fn xwma_decode_cost() -> XwmaDecodeCost {
    XwmaDecodeCost {
        hit: HIT.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
        spawned: SPAWNED.load(Ordering::Relaxed),
        failed: FAILED.load(Ordering::Relaxed),
        retried: RETRIED.load(Ordering::Relaxed),
        pcm_bytes: PCM_BYTES.load(Ordering::Relaxed),
        decode_ms: DECODE_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        key_ms: KEY_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        io_ms: IO_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
    }
}

/// A directory for one batch's files, under the artifacts root the decoded
/// samples end up in.
///
/// Several inputs and several outputs in one command line is what makes one
/// process do the work of sixty-four, and `ffmpeg` takes each of them as a
/// path — so the clips are handed over on disk and taken back the same way.
/// The directory is removed as soon as the samples have been read.
fn scratch_dir() -> Result<PathBuf, XwmaDecodeError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = ensure_artifacts_dir()
        .map_err(XwmaDecodeError::Scratch)?
        .join("scratch")
        .join("xwma");
    let dir = root.join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir)
        .map_err(|error| XwmaDecodeError::Scratch(format!("{}: {error}", dir.display())))?;
    sweep_stale_scratch(&root);
    Ok(dir)
}

/// A batch that died with its process leaves its files behind, and nothing
/// else ever looks in here. Anything older than an hour belonged to a run that
/// is gone — the same age the artifact cache gives its own temporaries.
fn sweep_stale_scratch(root: &Path) {
    static SWEPT: Once = Once::new();
    SWEPT.call_once(|| {
        let stale = SystemTime::now() - Duration::from_secs(3600);
        let Ok(listing) = fs::read_dir(root) else {
            return;
        };
        for entry in listing.filter_map(Result::ok) {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() && meta.modified().is_ok_and(|at| at < stale) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    });
}

/// One `ffmpeg` over many clips: every clip is its own input with its own
/// demuxer and its own output file, so nothing is concatenated and no clip's
/// samples can run into the next one's. The order out is the order in.
///
/// The outer error is the batch's: the process never ran, or ran and refused,
/// and nothing here says which clip it was about. The inner ones are a single
/// clip's — its output file is missing or empty while the rest are fine.
fn run_ffmpeg_batch(
    chunk: &[(usize, Vec<u8>)],
    clips: &[XwmaClip<'_>],
) -> Result<Vec<Result<Vec<u8>, XwmaDecodeError>>, XwmaDecodeError> {
    let dir = scratch_dir()?;
    let decoded = decode_in(&dir, chunk, clips);
    let _ = fs::remove_dir_all(&dir);
    decoded
}

fn decode_in(
    dir: &Path,
    chunk: &[(usize, Vec<u8>)],
    clips: &[XwmaClip<'_>],
) -> Result<Vec<Result<Vec<u8>, XwmaDecodeError>>, XwmaDecodeError> {
    let mut command = Command::new("ffmpeg");
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
    let mut outputs = Vec::with_capacity(chunk.len());
    for (slot, (i, xwma)) in chunk.iter().enumerate() {
        let input = dir.join(format!("{slot}.xwma"));
        fs::write(&input, xwma)
            .map_err(|error| XwmaDecodeError::Scratch(format!("{}: {error}", input.display())))?;
        command.args(["-f", "xwma", "-i"]).arg(&input);
        outputs.push((dir.join(format!("{slot}.pcm")), clips[*i]));
    }
    for (slot, (output, clip)) in outputs.iter().enumerate() {
        command
            .args(["-map", &format!("{slot}:a"), "-f", "s16le"])
            .args(["-ac", &clip.channels.max(1).to_string()])
            .args(["-ar", &clip.rate.max(1).to_string()])
            .arg("-y")
            .arg(output);
    }
    let finished = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(finished) => {
            SPAWNED.fetch_add(1, Ordering::Relaxed);
            finished
        }
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(XwmaDecodeError::FfmpegMissing),
        Err(e) => return Err(XwmaDecodeError::Spawn(e.to_string())),
    };
    if !finished.status.success() {
        return Err(XwmaDecodeError::Process {
            status: finished.status.code(),
            stderr: clip_stderr(&finished.stderr),
        });
    }
    Ok(outputs
        .iter()
        .map(|(output, _)| match fs::read(output) {
            Ok(pcm) if !pcm.is_empty() => Ok(pcm),
            _ => Err(XwmaDecodeError::EmptyPcm),
        })
        .collect())
}

/// One `ffmpeg` over one clip, handed over a pipe rather than a file. This is
/// what a clip asked for on its own costs, and what every clip cost before
/// batching: the process is the expensive part of it.
fn run_ffmpeg(xwma: &[u8], channels: u32, rate: u32) -> Result<Vec<u8>, XwmaDecodeError> {
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
        Ok(child) => {
            SPAWNED.fetch_add(1, Ordering::Relaxed);
            child
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(XwmaDecodeError::FfmpegMissing);
        }
        Err(e) => return Err(XwmaDecodeError::Spawn(e.to_string())),
    };
    let mut stdin = child.stdin.take().ok_or(XwmaDecodeError::Pipe("stdin"))?;
    let mut stdout = child.stdout.take().ok_or(XwmaDecodeError::Pipe("stdout"))?;
    let mut stderr = child.stderr.take().ok_or(XwmaDecodeError::Pipe("stderr"))?;

    let payload = xwma.to_vec();
    let write = std::thread::spawn(move || stdin.write_all(&payload));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache entry is an answer, and the question is stored with it. The key
    /// is a hash, the sweep is free to drop files, and a decoder can change its
    /// output — so an entry that does not match the clip being asked about has
    /// to read as a miss. Handing back samples at the wrong rate is silent, and
    /// decoding again is only slow.
    #[test]
    fn a_cache_entry_answers_only_the_clip_it_was_written_for() {
        let pcm = [1u8, 0, 0xff, 0x7f];
        let blob = cache_encode(2, 48000, &pcm);
        assert_eq!(cache_decode(&blob, 2, 48000).as_deref(), Some(&pcm[..]));

        assert_eq!(cache_decode(&blob, 1, 48000), None, "other channel count");
        assert_eq!(cache_decode(&blob, 2, 44100), None, "other sample rate");
        assert_eq!(
            cache_decode(&blob[..blob.len() - 1], 2, 48000),
            None,
            "the samples the header promised are not all there"
        );
        assert_eq!(cache_decode(&blob[..4], 2, 48000), None, "no header at all");
        assert_eq!(
            cache_decode(&cache_encode(2, 48000, &[]), 2, 48000),
            None,
            "an empty decode is a failure, and is never stored as one"
        );

        let mut older = blob.clone();
        older[8] = older[8].wrapping_add(1);
        assert_eq!(cache_decode(&older, 2, 48000), None, "another format");
        let mut alien = blob.clone();
        alien[0] = b'X';
        assert_eq!(cache_decode(&alien, 2, 48000), None, "another writer");
    }

    /// A batch is a list of questions and a list of answers, and the only
    /// thing tying them together is position. A clip that the cache answered,
    /// a clip that never reached the decoder, and the same clip asked for
    /// twice all have to leave their neighbours where they were: an answer
    /// that slid one place is another clip's samples at this clip's rate, and
    /// nothing downstream can tell.
    #[test]
    fn a_batch_answers_every_clip_in_the_order_it_was_asked() {
        let seek = [1u32, 2];
        let stored = |packets: &[u8], pcm: &[u8]| {
            let key = cache_key(packets, &seek, 2, 48000);
            cache_put(XWMA_CACHE_KIND, &key, &cache_encode(2, 48000, pcm))
                .expect("the test's own cache entry");
        };
        let known = |packets: &'static [u8]| XwmaClip {
            packets,
            seek_table: &seek,
            channels: 2,
            rate: 48000,
        };
        stored(b"first", &[1, 0, 2, 0]);
        stored(b"third", &[3, 0, 4, 0]);

        // Short of `seek_table.len() * block_align` bytes: this one never
        // reaches `ffmpeg`, and asking for it twice must not leave the second
        // asker waiting on a flight this thread is holding itself.
        let unmuxable = known(b"too short to be two stereo blocks");
        let answers = decode_t5_xwma_batch(&[
            known(b"first"),
            unmuxable,
            known(b"third"),
            unmuxable,
            known(b"first"),
        ]);

        assert_eq!(
            answers,
            vec![
                Ok(vec![1, 0, 2, 0]),
                Err(XwmaDecodeError::Mux),
                Ok(vec![3, 0, 4, 0]),
                Err(XwmaDecodeError::Mux),
                Ok(vec![1, 0, 2, 0]),
            ]
        );
    }

    /// The key covers every input the decode reads, and nothing else. Two
    /// aliases over one payload have to land on one entry — that is the saving
    /// — and a payload the zone changed has to land on a different one.
    #[test]
    fn the_key_covers_every_input_the_decode_reads() {
        let packets = [7u8; 16];
        let seek = [1u32, 2];
        let key = cache_key(&packets, &seek, 2, 48000);

        assert_eq!(key, cache_key(&packets, &seek, 2, 48000));
        assert_ne!(key, cache_key(&[7u8; 17], &seek, 2, 48000), "payload bytes");
        assert_ne!(key, cache_key(&[9u8; 16], &seek, 2, 48000), "payload bytes");
        assert_ne!(key, cache_key(&packets, &[1, 3], 2, 48000), "seek table");
        assert_ne!(
            key,
            cache_key(&packets, &[1], 2, 48000),
            "seek table length"
        );
        assert_ne!(key, cache_key(&packets, &seek, 1, 48000), "channels");
        assert_ne!(key, cache_key(&packets, &seek, 2, 44100), "rate");
    }
}
