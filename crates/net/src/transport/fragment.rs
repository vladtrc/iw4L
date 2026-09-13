use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const FRAGMENT_HEADER_BYTES: usize = 16;
const FRAGMENT_MAGIC: [u8; 2] = *b"RF";
const FRAGMENT_VERSION: u8 = 2;

const MAX_PENDING_PACKETS: usize = 32;

const MAX_PENDING_BYTES: usize = 2 * 1024 * 1024;

const FRAGMENT_TTL: Duration = Duration::from_secs(1);

const MAX_FRAGMENTS: usize = u16::MAX as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragmentError(pub String);

impl core::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FragmentError {}

impl From<FragmentError> for String {
    fn from(error: FragmentError) -> Self {
        error.0
    }
}

fn err(message: impl Into<String>) -> FragmentError {
    FragmentError(message.into())
}

#[derive(Debug)]
pub struct Fragmenter {
    payload_bytes: usize,
    max_packet_bytes: usize,
    next_id: u32,
}

impl Fragmenter {
    pub fn new(datagram_bytes: usize, max_packet_bytes: usize) -> Self {
        Self {
            payload_bytes: datagram_bytes.saturating_sub(FRAGMENT_HEADER_BYTES).max(1),
            max_packet_bytes,
            next_id: 0,
        }
    }

    pub fn split(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, FragmentError> {
        if bytes.is_empty() || bytes.len() > self.max_packet_bytes {
            return Err(err(format!(
                "packet length {} must be 1..={}",
                bytes.len(),
                self.max_packet_bytes
            )));
        }
        let count = bytes.len().div_ceil(self.payload_bytes);
        if count > MAX_FRAGMENTS {
            return Err(err(format!(
                "packet length {} needs {count} fragments (max {MAX_FRAGMENTS})",
                bytes.len()
            )));
        }
        let total_len = bytes.len() as u32;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let mut fragments = Vec::with_capacity(count);
        for (index, payload) in bytes.chunks(self.payload_bytes).enumerate() {
            let mut fragment = Vec::with_capacity(FRAGMENT_HEADER_BYTES + payload.len());
            fragment.extend_from_slice(&FRAGMENT_MAGIC);
            fragment.push(FRAGMENT_VERSION);
            fragment.push(0);
            fragment.extend_from_slice(&(index as u16).to_le_bytes());
            fragment.extend_from_slice(&(count as u16).to_le_bytes());
            fragment.extend_from_slice(&id.to_le_bytes());
            fragment.extend_from_slice(&total_len.to_le_bytes());
            fragment.extend_from_slice(payload);
            fragments.push(fragment);
        }
        Ok(fragments)
    }
}

struct PartialPacket {
    total_len: usize,
    created_at: Instant,
    fragments: Vec<Option<Vec<u8>>>,
    received: usize,
}

#[derive(Debug)]
pub struct Reassembler {
    payload_bytes: usize,
    max_packet_bytes: usize,
    pending: HashMap<u32, PartialPacket>,
}

impl core::fmt::Debug for PartialPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PartialPacket")
            .field("total_len", &self.total_len)
            .field("received", &self.received)
            .field("fragments", &self.fragments.len())
            .finish()
    }
}

impl Reassembler {
    pub fn new(datagram_bytes: usize, max_packet_bytes: usize) -> Self {
        Self {
            payload_bytes: datagram_bytes.saturating_sub(FRAGMENT_HEADER_BYTES).max(1),
            max_packet_bytes,
            pending: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, FragmentError> {
        if bytes.len() < FRAGMENT_HEADER_BYTES || bytes[..2] != FRAGMENT_MAGIC {
            return Err(err("malformed fragment header"));
        }

        if bytes[2] != FRAGMENT_VERSION || bytes[3] != 0 {
            return Err(err(format!(
                "fragment header version {} (reserved {}), ours is {FRAGMENT_VERSION} — \
                 the peer is a different build",
                bytes[2], bytes[3]
            )));
        }
        let index = u16::from_le_bytes(bytes[4..6].try_into().expect("2 bytes")) as usize;
        let count = u16::from_le_bytes(bytes[6..8].try_into().expect("2 bytes")) as usize;
        let packet_id = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
        let total_len = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
        if total_len == 0 || total_len > self.max_packet_bytes {
            return Err(err(format!(
                "packet length {total_len} is out of bounds (max {})",
                self.max_packet_bytes
            )));
        }
        let expected_count = total_len.div_ceil(self.payload_bytes);
        if count == 0 || count > MAX_FRAGMENTS || count != expected_count || index >= count {
            return Err(err("malformed fragment range"));
        }
        let expected_len = if index + 1 == count {
            total_len - self.payload_bytes * index
        } else {
            self.payload_bytes
        };
        let payload = &bytes[FRAGMENT_HEADER_BYTES..];
        if payload.len() != expected_len {
            return Err(err("malformed fragment length"));
        }

        self.pending
            .retain(|_, packet| packet.created_at.elapsed() < FRAGMENT_TTL);
        if !self.pending.contains_key(&packet_id) {
            let pending_bytes: usize = self.pending.values().map(|packet| packet.total_len).sum();
            if self.pending.len() >= MAX_PENDING_PACKETS
                || pending_bytes + total_len > MAX_PENDING_BYTES
            {
                return Err(err(format!(
                    "reassembly capacity exceeded: packet {packet_id} of {total_len} B \
                     ({count} fragments) with {} packet(s) / {pending_bytes} B pending \
                     (max {MAX_PENDING_PACKETS} / {MAX_PENDING_BYTES})",
                    self.pending.len()
                )));
            }
        }
        let packet = self
            .pending
            .entry(packet_id)
            .or_insert_with(|| PartialPacket {
                total_len,
                created_at: Instant::now(),
                fragments: vec![None; count],
                received: 0,
            });
        if packet.total_len != total_len || packet.fragments.len() != count {
            self.pending.remove(&packet_id);
            return Err(err("fragment metadata changed within packet"));
        }
        match packet.fragments[index].as_ref() {
            Some(existing) if existing.as_slice() != payload => {
                self.pending.remove(&packet_id);
                return Err(err("fragment payload changed within packet"));
            }
            Some(_) => return Ok(None),
            None => {
                packet.fragments[index] = Some(payload.to_vec());
                packet.received += 1;
            }
        }
        if packet.received != count {
            return Ok(None);
        }
        let packet = self
            .pending
            .remove(&packet_id)
            .expect("completed packet exists");
        let mut assembled = Vec::with_capacity(packet.total_len);
        for fragment in packet.fragments {
            assembled.extend(fragment.expect("completed packet has every fragment"));
        }
        Ok(Some(assembled))
    }
}
