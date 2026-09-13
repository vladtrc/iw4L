use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use crate::transport::fragment::{Fragmenter, Reassembler};
use crate::transport::protocol::ProtocolLimits;

pub const DEFAULT_RECV_BUDGET_PER_TICK: usize = 128;

pub const DATAGRAM_BYTES: usize = 1200;

const SOCKET_BUFFER_BYTES: usize = 4 * crate::transport::protocol::MAX_PACKET_BYTES as usize;

const MAX_REASSEMBLY_PEERS: usize = 64;

#[derive(Debug)]
pub struct UdpDatagramSocket {
    sock: UdpSocket,
    max_packet_bytes: usize,
    fragmenter: Fragmenter,

    inbound: HashMap<SocketAddr, Reassembler>,
}

#[derive(Debug)]
pub enum UdpSendError {
    Oversized {
        encoded_len: usize,
        max_packet_bytes: usize,
    },
    Io(io::Error),
}

impl core::fmt::Display for UdpSendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Oversized {
                encoded_len,
                max_packet_bytes,
            } => write!(
                f,
                "datagram {encoded_len} bytes exceeds max_packet_bytes {max_packet_bytes}"
            ),
            Self::Io(e) => write!(f, "udp send: {e}"),
        }
    }
}

impl std::error::Error for UdpSendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Oversized { .. } => None,
            Self::Io(e) => Some(e),
        }
    }
}

fn size_socket_buffers(sock: &UdpSocket) {
    let sock = socket2::SockRef::from(sock);

    match sock.set_recv_buffer_size(SOCKET_BUFFER_BYTES) {
        Ok(()) => {
            if let Ok(effective) = sock.recv_buffer_size()
                && effective < SOCKET_BUFFER_BYTES
            {
                diag::warn!(
                    Net,
                    "udp recv buffer clamped to {effective} of {SOCKET_BUFFER_BYTES} — a full baseline burst may not fit"
                );
            }
        }
        Err(e) => diag::warn!(Net, "udp recv buffer {SOCKET_BUFFER_BYTES} refused: {e}"),
    }
    if let Err(e) = sock.set_send_buffer_size(SOCKET_BUFFER_BYTES) {
        diag::warn!(Net, "udp send buffer {SOCKET_BUFFER_BYTES} refused: {e}");
    }
}

impl UdpDatagramSocket {
    pub fn bind<A: ToSocketAddrs>(addr: A, limits: &ProtocolLimits) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        size_socket_buffers(&sock);
        let max_packet_bytes = limits.max_packet_bytes as usize;
        Ok(Self {
            sock,
            max_packet_bytes,
            fragmenter: Fragmenter::new(DATAGRAM_BYTES, max_packet_bytes),
            inbound: HashMap::new(),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    pub fn max_packet_bytes(&self) -> usize {
        self.max_packet_bytes
    }

    pub fn send_to(&mut self, bytes: &[u8], addr: SocketAddr) -> Result<(), UdpSendError> {
        let fragments = self
            .fragmenter
            .split(bytes)
            .map_err(|_| UdpSendError::Oversized {
                encoded_len: bytes.len(),
                max_packet_bytes: self.max_packet_bytes,
            })?;
        for fragment in fragments {
            self.sock
                .send_to(&fragment, addr)
                .map_err(UdpSendError::Io)?;
        }
        Ok(())
    }

    pub fn recv_one(&self, buf: &mut [u8]) -> io::Result<Option<(usize, SocketAddr)>> {
        match self.sock.recv_from(buf) {
            Ok((n, addr)) => Ok(Some((n, addr))),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn recv_budget(
        &mut self,
        budget: usize,
        out: &mut Vec<(Vec<u8>, SocketAddr)>,
    ) -> io::Result<usize> {
        let mut buf = vec![0u8; DATAGRAM_BYTES];
        let mut got = 0usize;
        while got < budget {
            let Some((n, addr)) = self.recv_one(&mut buf)? else {
                break;
            };
            got += 1;
            if !self.inbound.contains_key(&addr) && self.inbound.len() >= MAX_REASSEMBLY_PEERS {
                diag::warn!(
                    Net,
                    "udp reassembly: {MAX_REASSEMBLY_PEERS} peers in flight, dropping fragment from {addr}"
                );
                continue;
            }
            let peer = self
                .inbound
                .entry(addr)
                .or_insert_with(|| Reassembler::new(DATAGRAM_BYTES, self.max_packet_bytes));
            match peer.push(&buf[..n]) {
                Ok(Some(packet)) => out.push((packet, addr)),
                Ok(None) => {}
                Err(error) => diag::warn!(Net, "udp reassembly from {addr}: {error}"),
            }
            if peer.is_empty() {
                self.inbound.remove(&addr);
            }
        }
        Ok(got)
    }
}
