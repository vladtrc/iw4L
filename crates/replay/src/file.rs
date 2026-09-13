use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use net::WorldObjectSyncDecoder;
use net::{Frame, PROTOCOL_VERSION, Transport, TransportError, WireError, WireReader};
use sim::RNG_DOMAIN_SCHEME;

pub const MAGIC: [u8; 8] = *b"IW4LDEMO";

pub const ZONE_FIELD_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchRecordIdentity {
    pub rng_scheme: u32,
    pub root_seed: u64,
    pub content_digest: u64,

    pub zone: [u8; ZONE_FIELD_LEN],
}

impl MatchRecordIdentity {
    pub fn from_world(world: &sim::SimWorld) -> Self {
        Self {
            rng_scheme: RNG_DOMAIN_SCHEME,
            root_seed: world.root_seed(),
            content_digest: world.content_digest(),
            zone: [0; ZONE_FIELD_LEN],
        }
    }

    pub fn from_world_on_zone(world: &sim::SimWorld, zone: &str) -> Self {
        let mut identity = Self::from_world(world);
        identity.set_zone(zone);
        identity
    }

    pub fn set_zone(&mut self, zone: &str) {
        self.zone = encode_zone_field(zone);
    }

    pub fn zone_name(&self) -> Option<&str> {
        decode_zone_field(&self.zone)
    }
}

fn encode_zone_field(zone: &str) -> [u8; ZONE_FIELD_LEN] {
    let mut out = [0u8; ZONE_FIELD_LEN];
    let cleaned: String = zone
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(ZONE_FIELD_LEN - 1)
        .collect();
    let bytes = cleaned.as_bytes();
    out[..bytes.len()].copy_from_slice(bytes);
    out
}

fn decode_zone_field(zone: &[u8; ZONE_FIELD_LEN]) -> Option<&str> {
    let end = zone.iter().position(|&b| b == 0).unwrap_or(zone.len());
    let s = core::str::from_utf8(&zone[..end]).ok()?;
    if s.is_empty() { None } else { Some(s) }
}

fn looks_like_zone_field(zone: &[u8; ZONE_FIELD_LEN]) -> bool {
    if zone.iter().all(|&b| b == 0) {
        return true;
    }
    let Some(end) = zone.iter().position(|&b| b == 0) else {
        return zone.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_');
    };
    if end == 0 {
        return false;
    }
    zone[..end]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
        && zone[end..].iter().all(|&b| b == 0)
}

const MAX_FRAME_LEN: u32 = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum ReplayError {
    Io(std::io::Error),

    Magic { found: [u8; 8] },

    Version { found: u32, expected: u32 },
    Wire(WireError),
}

impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReplayError::Io(e) => write!(f, "{e}"),
            ReplayError::Magic { found } => write!(
                f,
                "not an iw4l recording: magic was {:?}, expected {:?}",
                String::from_utf8_lossy(found),
                String::from_utf8_lossy(&MAGIC)
            ),
            ReplayError::Version { found, expected } => write!(
                f,
                "recording is protocol {found}, this build speaks {expected}; \
                 refusing to guess at the difference"
            ),
            ReplayError::Wire(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<std::io::Error> for ReplayError {
    fn from(value: std::io::Error) -> Self {
        ReplayError::Io(value)
    }
}

impl From<WireError> for ReplayError {
    fn from(value: WireError) -> Self {
        ReplayError::Wire(value)
    }
}

impl From<ReplayError> for TransportError {
    fn from(value: ReplayError) -> Self {
        match value {
            ReplayError::Io(e) => TransportError::Io(e),
            ReplayError::Wire(e) => TransportError::Wire(e),
            other => TransportError::Io(std::io::Error::other(other.to_string())),
        }
    }
}

pub fn demo_path(artifacts_root: &Path, name: &str) -> PathBuf {
    artifacts_root
        .join("demos")
        .join(format!("{name}.iw4ldemo"))
}

pub fn sanitize_demo_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(64)
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn demo_stem(requested: &str) -> &str {
    requested
        .strip_suffix(".iw4ldemo")
        .unwrap_or(requested)
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(requested)
}

#[derive(Debug)]
pub struct RecordWriter {
    file: BufWriter<File>,
    path: PathBuf,
    frames: u64,
}

impl RecordWriter {
    pub fn create(path: &Path) -> Result<Self, ReplayError> {
        Self::create_with_identity(path, MatchRecordIdentity::default())
    }

    pub fn create_with_identity(
        path: &Path,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&MAGIC)?;
        file.write_all(&PROTOCOL_VERSION.to_le_bytes())?;
        let scheme = if identity.rng_scheme == 0 {
            RNG_DOMAIN_SCHEME
        } else {
            identity.rng_scheme
        };
        file.write_all(&scheme.to_le_bytes())?;
        file.write_all(&identity.root_seed.to_le_bytes())?;
        file.write_all(&identity.content_digest.to_le_bytes())?;
        file.write_all(&identity.zone)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
            frames: 0,
        })
    }

    pub fn write_frame(&mut self, frame: &Frame) -> Result<(), ReplayError> {
        let bytes = frame.to_bytes();
        self.file.write_all(&(bytes.len() as u32).to_le_bytes())?;
        self.file.write_all(&bytes)?;
        self.frames += 1;
        Ok(())
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn finish(mut self) -> Result<(u64, PathBuf), ReplayError> {
        self.file.flush()?;
        Ok((self.frames, self.path))
    }
}

#[derive(Debug)]
pub struct RecordReader {
    file: BufReader<File>,
    path: PathBuf,
    frames: u64,
    identity: MatchRecordIdentity,
    world_decoder: WorldObjectSyncDecoder,
}

impl RecordReader {
    pub fn open(path: &Path) -> Result<Self, ReplayError> {
        let mut file = BufReader::new(File::open(path)?);
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(ReplayError::Magic { found: magic });
        }
        let mut version = [0u8; 4];
        file.read_exact(&mut version)?;
        let version = u32::from_le_bytes(version);
        if version != PROTOCOL_VERSION {
            return Err(ReplayError::Version {
                found: version,
                expected: PROTOCOL_VERSION,
            });
        }
        let mut scheme = [0u8; 4];
        file.read_exact(&mut scheme)?;
        let mut root_seed = [0u8; 8];
        file.read_exact(&mut root_seed)?;
        let mut content_digest = [0u8; 8];
        file.read_exact(&mut content_digest)?;
        let mut zone = [0u8; ZONE_FIELD_LEN];
        file.read_exact(&mut zone)?;
        if !looks_like_zone_field(&zone) {
            file.seek(SeekFrom::Current(-(ZONE_FIELD_LEN as i64)))?;
            zone = [0u8; ZONE_FIELD_LEN];
        }
        let identity = MatchRecordIdentity {
            rng_scheme: u32::from_le_bytes(scheme),
            root_seed: u64::from_le_bytes(root_seed),
            content_digest: u64::from_le_bytes(content_digest),
            zone,
        };
        Ok(Self {
            file,
            path: path.to_path_buf(),
            frames: 0,
            identity,
            world_decoder: WorldObjectSyncDecoder::default(),
        })
    }

    pub fn identity(&self) -> MatchRecordIdentity {
        self.identity
    }

    pub fn read_frame(&mut self) -> Result<Option<Frame>, ReplayError> {
        let mut length = [0u8; 4];
        match self.file.read_exact(&mut length) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let length = u32::from_le_bytes(length);
        if length > MAX_FRAME_LEN {
            return Err(ReplayError::Wire(WireError::Malformed(
                "frame length prefix is implausible; the file is corrupt",
            )));
        }
        let mut bytes = vec![0u8; length as usize];
        match self.file.read_exact(&mut bytes) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let mut reader = WireReader::new(&bytes);
        let frame = Frame::decode(&mut reader, &mut self.world_decoder)?;
        self.frames += 1;
        Ok(Some(frame))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
pub struct FileTransport {
    writer: Option<RecordWriter>,
    reader: Option<RecordReader>,
}

impl FileTransport {
    pub fn recording(path: &Path) -> Result<Self, ReplayError> {
        Self::recording_with_identity(path, MatchRecordIdentity::default())
    }

    pub fn recording_with_identity(
        path: &Path,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        Ok(Self {
            writer: Some(RecordWriter::create_with_identity(path, identity)?),
            reader: None,
        })
    }

    pub fn playback(path: &Path) -> Result<Self, ReplayError> {
        Ok(Self {
            writer: None,
            reader: Some(RecordReader::open(path)?),
        })
    }

    pub fn identity(&self) -> Option<MatchRecordIdentity> {
        self.reader.as_ref().map(RecordReader::identity)
    }

    pub fn finish(&mut self) -> Result<Option<(u64, PathBuf)>, ReplayError> {
        match self.writer.take() {
            Some(writer) => writer.finish().map(Some),
            None => Ok(None),
        }
    }
}

impl Transport for FileTransport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError> {
        let Some(writer) = self.writer.as_mut() else {
            return Err(TransportError::Io(std::io::Error::other(
                "this FileTransport was opened for playback, not recording",
            )));
        };
        writer.write_frame(frame).map_err(TransportError::from)
    }

    fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        let Some(reader) = self.reader.as_mut() else {
            return Err(TransportError::Ended);
        };
        match reader.read_frame().map_err(TransportError::from)? {
            Some(frame) => Ok(Some(frame)),
            None => Err(TransportError::Ended),
        }
    }
}
