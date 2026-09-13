use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bevy::prelude::Resource;
use master_protocol::MemberId;
use playerstate_iw4::UserCmd;
use sim::{ClientAction, ClientId, Snapshot, Tick, TickInput};

use crate::authority::runtime::ClientShotSamples;
use crate::client::predict::CmdSeq;
use crate::transport::acked_baseline::AckedBaselineTable;
use crate::transport::bootstrap::{
    BootstrapAck, BootstrapLane, BootstrapMessage, BootstrapTransaction, decode_bootstrap,
    encode_bootstrap, epoch_applies,
};
use crate::transport::delta::{SnapshotDecoder, SnapshotEncoder};
use crate::transport::frame::{Frame, frame_from_acked_tick};
use crate::transport::loopback_live::ReceivedTick;
use crate::transport::meta_wire::WorldObjectSyncDecoder;
use crate::transport::protocol::{
    ClientPacket, ConnectionId, ConnectionTable, HandshakeHello, HandshakeReject, PacketHeader,
    ProtocolLimits, ServerPacket, decode_client_packet, decode_server_packet, evaluate_handshake,
};
use crate::transport::udp_socket::{DEFAULT_RECV_BUDGET_PER_TICK, UdpDatagramSocket, UdpSendError};
use crate::transport::wire::WireReader;

const RELAY_MAIL_CAP: usize = 64;

#[derive(Clone, Debug)]
pub struct RelayMailbox {
    inbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    outbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    control_inbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    control_outbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    cap: usize,
}

impl RelayMailbox {
    pub fn new(cap: usize) -> Self {
        Self {
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            control_inbound: Arc::new(Mutex::new(Vec::new())),
            control_outbound: Arc::new(Mutex::new(Vec::new())),
            cap,
        }
    }

    pub fn push_control_inbound(
        &self,
        member: MemberId,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        push_mail(&self.control_inbound, self.cap, member, bytes)
    }
    pub fn take_control_inbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        take_mail(&self.control_inbound)
    }
    pub fn push_control_outbound(
        &self,
        member: MemberId,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        push_mail(&self.control_outbound, self.cap, member, bytes)
    }
    pub fn take_control_outbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        take_mail(&self.control_outbound)
    }

    pub fn with_default_cap() -> Self {
        Self::new(RELAY_MAIL_CAP)
    }

    pub fn push_inbound(&self, member: MemberId, bytes: Vec<u8>) -> Result<(), &'static str> {
        push_mail(&self.inbound, self.cap, member, bytes)
    }

    pub fn take_inbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        take_mail(&self.inbound)
    }

    pub fn push_outbound(&self, member: MemberId, bytes: Vec<u8>) -> Result<(), &'static str> {
        push_mail(&self.outbound, self.cap, member, bytes)
    }

    pub fn take_outbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        take_mail(&self.outbound)
    }
}

fn push_mail(
    queue: &Mutex<Vec<(MemberId, Vec<u8>)>>,
    cap: usize,
    member: MemberId,
    bytes: Vec<u8>,
) -> Result<(), &'static str> {
    let mut queue = queue.lock().expect("relay mailbox poisoned");
    if queue.len() >= cap {
        return Err("relay mailbox full");
    }
    queue.push((member, bytes));
    Ok(())
}

fn take_mail(queue: &Mutex<Vec<(MemberId, Vec<u8>)>>) -> Vec<(MemberId, Vec<u8>)> {
    std::mem::take(&mut *queue.lock().expect("relay mailbox poisoned"))
}

fn relay_send_error(message: &'static str) -> UdpSendError {
    UdpSendError::Io(io::Error::new(io::ErrorKind::WouldBlock, message))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PeerTarget {
    Udp(SocketAddr),
    Relay(MemberId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommittedAdmission {
    pub member_id: MemberId,
    pub epoch: u32,
    pub bootstrap_id: u32,
    pub connection_id: u64,

    pub first_commit: bool,
}

#[derive(Resource)]
pub struct UdpAuthorityHub {
    socket: Option<UdpDatagramSocket>,
    relay: Option<RelayMailbox>,
    pub hello: HandshakeHello,
    pub limits: ProtocolLimits,
    pub connections: ConnectionTable,
    peers: HashMap<ConnectionId, PeerTarget>,
    by_addr: HashMap<SocketAddr, ConnectionId>,

    pending_replies: Vec<(SocketAddr, ServerPacket)>,
    replication: HashMap<ConnectionId, PeerReplicationState>,

    bootstrap: Option<Arc<BootstrapLane>>,
    next_bootstrap_id: HashMap<ConnectionId, u32>,
    member_by_addr: HashMap<SocketAddr, master_protocol::MemberId>,
    member_by_conn: HashMap<ConnectionId, master_protocol::MemberId>,
    committed_admissions: Vec<CommittedAdmission>,

    denied: HashSet<master_protocol::MemberId>,
}

#[derive(Debug)]
struct PeerReplicationState {
    baseline: AckedBaselineTable,
    encoder: SnapshotEncoder,
    next_snap_seq: u32,
    out_seq: u32,
    in_ack: u32,
    last_full: Option<Tick>,
    sent_ticks: std::collections::VecDeque<Tick>,
    admission: PeerAdmission,
}

#[derive(Debug)]
enum PeerAdmission {
    Uncommitted,
    Pending {
        bootstrap_id: u32,
        snapshot_seq: u32,
        epoch: u32,

        #[allow(dead_code)]
        tick_b: u32,

        #[allow(dead_code)]
        offer_bytes: Vec<u8>,
    },
    Committed {
        bootstrap_id: u32,
    },
}

impl PeerReplicationState {
    fn new() -> Self {
        Self {
            baseline: AckedBaselineTable::new(32),
            encoder: SnapshotEncoder::new(),
            next_snap_seq: 1,
            out_seq: 0,
            in_ack: 0,
            last_full: None,
            sent_ticks: std::collections::VecDeque::new(),
            admission: PeerAdmission::Uncommitted,
        }
    }

    fn reset_match(&mut self) {
        *self = Self::new();
    }

    fn admits_gameplay(&self, relay: bool) -> bool {
        if !relay {
            return true;
        }
        matches!(self.admission, PeerAdmission::Committed { .. })
    }
}

const FULL_SNAPSHOT_RESEND_TICKS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppliedAction {
    Commit,
    RepeatEnter,
    Ignore,
}

fn applied_action(
    pending: Option<(u32, u32, u32)>,
    committed: Option<u32>,
    bootstrap_id: u32,
    snapshot_seq: u32,
    epoch: u32,
) -> AppliedAction {
    match pending {
        Some(expected) if expected == (bootstrap_id, snapshot_seq, epoch) => AppliedAction::Commit,
        _ if committed == Some(bootstrap_id) => AppliedAction::RepeatEnter,
        _ => AppliedAction::Ignore,
    }
}

impl UdpAuthorityHub {
    pub fn bind(addr: SocketAddr, hello: HandshakeHello) -> std::io::Result<Self> {
        Self::bind_with_first_client(addr, hello, 0)
    }

    pub fn bind_with_first_client(
        addr: SocketAddr,
        hello: HandshakeHello,
        first_client: u32,
    ) -> std::io::Result<Self> {
        let limits = hello.limits;
        Ok(Self {
            socket: Some(UdpDatagramSocket::bind(addr, &limits)?),
            relay: None,
            hello,
            limits,
            connections: ConnectionTable::starting_at(first_client),
            peers: HashMap::new(),
            by_addr: HashMap::new(),
            pending_replies: Vec::new(),
            replication: HashMap::new(),
            bootstrap: None,
            next_bootstrap_id: HashMap::new(),
            member_by_addr: HashMap::new(),
            member_by_conn: HashMap::new(),
            committed_admissions: Vec::new(),
            denied: HashSet::new(),
        })
    }

    pub fn relay(hello: HandshakeHello, first_client: u32, mailbox: RelayMailbox) -> Self {
        let limits = hello.limits;
        Self {
            socket: None,
            relay: Some(mailbox),
            hello,
            limits,
            connections: ConnectionTable::starting_at(first_client),
            peers: HashMap::new(),
            by_addr: HashMap::new(),
            pending_replies: Vec::new(),
            replication: HashMap::new(),
            bootstrap: None,
            next_bootstrap_id: HashMap::new(),
            member_by_addr: HashMap::new(),
            member_by_conn: HashMap::new(),
            committed_admissions: Vec::new(),
            denied: HashSet::new(),
        }
    }

    pub fn mailbox(&self) -> Option<RelayMailbox> {
        self.relay.clone()
    }

    pub fn attach_bootstrap(&mut self, lane: Arc<BootstrapLane>) {
        self.bootstrap = Some(lane);
    }

    pub fn note_member_addr(&mut self, member_id: master_protocol::MemberId, addr: SocketAddr) {
        self.member_by_addr.insert(addr, member_id);
        if let Some(conn) = self.by_addr.get(&addr).copied() {
            self.member_by_conn.insert(conn, member_id);
        }
    }

    fn enroll_relay_member(&mut self, member_id: MemberId) -> ConnectionId {
        if let Some((conn, _)) = self.member_by_conn.iter().find(|(_, id)| **id == member_id) {
            return *conn;
        }
        let (conn, _) = self.connections.accept_new();
        self.peers.insert(conn, PeerTarget::Relay(member_id));
        self.replication.insert(conn, PeerReplicationState::new());
        self.member_by_conn.insert(conn, member_id);
        conn
    }

    pub fn reconcile_relay_membership(&mut self, members: &[MemberId], local: MemberId) {
        if self.relay.is_none() {
            return;
        }
        for member_id in members {
            if *member_id == local || self.denied.contains(member_id) {
                continue;
            }
            self.enroll_relay_member(*member_id);
        }
    }

    pub fn take_committed_admissions(&mut self) -> Vec<CommittedAdmission> {
        std::mem::take(&mut self.committed_admissions)
    }

    pub fn client_of_member(&self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        let conn = self
            .member_by_conn
            .iter()
            .find_map(|(conn, id)| (*id == member_id).then_some(*conn))?;
        self.connections.client_of(conn).map(ClientId)
    }

    pub fn deny_member(&mut self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        let client = self.retire_member(member_id);
        self.denied.insert(member_id);
        client
    }

    pub fn retire_member(&mut self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        self.denied.remove(&member_id);
        let conn = self
            .member_by_conn
            .iter()
            .find_map(|(conn, id)| (*id == member_id).then_some(*conn));
        let Some(conn) = conn else {
            self.member_by_addr.retain(|_, id| *id != member_id);
            return None;
        };
        self.retire_connection(conn)
    }

    pub fn retire_client(&mut self, client: ClientId) -> bool {
        self.retire_client_with_reason(client, "ConnectionRetired")
    }

    pub fn retire_client_with_reason(&mut self, client: ClientId, reason: &str) -> bool {
        let Some(conn) = self
            .peers
            .keys()
            .copied()
            .find(|conn| self.connections.resolve(*conn, 0) == Ok(client.0))
        else {
            return false;
        };
        if let Some(member) = self.member_by_conn.get(&conn).copied() {
            if let Some(mailbox) = &self.relay {
                let packet = ServerPacket::Control {
                    header: PacketHeader {
                        connection: conn,
                        sequence: 0,
                        ack: 0,
                        epoch: self.live_packet_epoch(),
                    },
                    payload: crate::ReliablePayload {
                        ack_through: 0,
                        rows: vec![(1, crate::ReliableRow::Failure(reason.to_owned()))],
                        dropped_oldest: 0,
                    },
                };
                if let Err(error) = mailbox.push_control_outbound(member, packet.to_bytes()) {
                    diag::warn!(Net, "retire control: {error}");
                }
            }
            self.denied.insert(member);
        }
        self.retire_connection(conn).is_some()
    }

    pub fn retire_connection(&mut self, conn: ConnectionId) -> Option<ClientId> {
        let client = self.connections.retire(conn).map(ClientId);
        if let Some(PeerTarget::Udp(addr)) = self.peers.remove(&conn) {
            self.by_addr.remove(&addr);
            self.member_by_addr.remove(&addr);
        }
        self.member_by_conn.remove(&conn);
        self.replication.remove(&conn);
        self.next_bootstrap_id.remove(&conn);
        self.pending_replies
            .retain(|(addr, _)| self.by_addr.contains_key(addr));
        client
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket
            .as_ref()
            .ok_or_else(|| io::Error::other("relay hub has no UDP socket"))?
            .local_addr()
    }

    fn send_outgoing(&mut self, target: PeerTarget, bytes: &[u8]) -> Result<(), UdpSendError> {
        match target {
            PeerTarget::Udp(addr) => {
                let socket = self
                    .socket
                    .as_mut()
                    .ok_or_else(|| relay_send_error("udp hub has no socket"))?;
                socket.send_to(bytes, addr)
            }
            PeerTarget::Relay(member) => {
                let mailbox = self
                    .relay
                    .as_ref()
                    .ok_or_else(|| relay_send_error("relay hub has no mailbox"))?;
                mailbox
                    .push_outbound(member, bytes.to_vec())
                    .map_err(relay_send_error)
            }
        }
    }

    fn live_packet_epoch(&self) -> u32 {
        self.bootstrap
            .as_ref()
            .map(|lane| lane.epoch())
            .unwrap_or(0)
    }

    pub fn reset_match(&mut self) {
        self.pending_replies.clear();
        self.next_bootstrap_id.clear();
        self.committed_admissions.clear();
        self.denied.clear();
        for peer in self.replication.values_mut() {
            peer.reset_match();
        }
        if let Some(lane) = &self.bootstrap {
            lane.set_epoch(0);
        }
    }

    pub fn ingress(
        &mut self,
        cmd_inbox: &mut crate::ClientCommandInbox,
        action_inbox: &mut crate::ClientActionInbox,
        _samples: Option<&mut ClientShotSamples>,
        mut reliable: Option<&mut crate::ReliableEventHub>,
    ) -> Result<(), String> {
        self.apply_admission_acks();
        let mut datagrams = Vec::new();
        if let Some(socket) = self.socket.as_mut() {
            socket
                .recv_budget(DEFAULT_RECV_BUDGET_PER_TICK, &mut datagrams)
                .map_err(|e| e.to_string())?;
        }
        let mut packets: Vec<(Vec<u8>, PeerTarget, bool)> = datagrams
            .into_iter()
            .map(|(bytes, addr)| (bytes, PeerTarget::Udp(addr), false))
            .collect();
        if let Some(mailbox) = &self.relay {
            for (member, bytes) in mailbox.take_inbound() {
                packets.push((bytes, PeerTarget::Relay(member), false));
            }
        }
        if let Some(mailbox) = &self.relay {
            for (member, bytes) in mailbox.take_control_inbound() {
                packets.push((bytes, PeerTarget::Relay(member), true));
            }
        }
        for (bytes, from, control) in packets {
            let packet = match decode_client_packet(&bytes, &self.limits) {
                Ok(p) => p,
                Err(_) => continue,
            };
            match packet {
                ClientPacket::Connect(client_hello) => {
                    let PeerTarget::Udp(addr) = from else {
                        continue;
                    };
                    match evaluate_handshake(&self.hello, &client_hello) {
                        Ok(()) => {
                            let (conn, client) =
                                if let Some(conn) = self.by_addr.get(&addr).copied() {
                                    let client = self
                                        .connections
                                        .resolve(conn, 0)
                                        .expect("address map must name a live connection");
                                    (conn, client)
                                } else {
                                    let (conn, client) = self.connections.accept_new();
                                    self.peers.insert(conn, PeerTarget::Udp(addr));
                                    self.by_addr.insert(addr, conn);
                                    self.replication.insert(conn, PeerReplicationState::new());
                                    if let Some(member) = self.member_by_addr.get(&addr).copied() {
                                        self.member_by_conn.insert(conn, member);
                                    }
                                    (conn, client)
                                };
                            self.pending_replies.push((
                                addr,
                                ServerPacket::Accept {
                                    connection: conn,
                                    assigned_client: client,
                                    hello: self.hello,
                                },
                            ));
                        }
                        Err(reject) => {
                            self.pending_replies
                                .push((addr, ServerPacket::Reject(reject)));
                        }
                    }
                }
                ClientPacket::Commands {
                    header,
                    claimed_client,
                    cmds,
                    samples: cmd_samples,
                    actions,
                    reliable_ack,
                } => {
                    if self.peers.get(&header.connection) != Some(&from) {
                        continue;
                    }
                    if !header.applies_to_epoch(self.live_packet_epoch()) {
                        diag::warn!(
                            Net,
                            "authority ignored commands for epoch {} (live {})",
                            header.epoch,
                            self.live_packet_epoch()
                        );
                        continue;
                    }
                    let relay = self.bootstrap.is_some();
                    if !self
                        .replication
                        .get(&header.connection)
                        .is_some_and(|peer| peer.admits_gameplay(relay))
                    {
                        continue;
                    }
                    let client = match self.connections.resolve(header.connection, claimed_client) {
                        Ok(id) => ClientId(id),
                        Err(_) => continue,
                    };
                    if let Some(peer) = self.replication.get_mut(&header.connection) {
                        peer.in_ack = header.sequence;
                    }
                    if let Some(reliable) = reliable.as_deref_mut() {
                        reliable.ack(client, reliable_ack);
                    }
                    for (seq, cmd) in cmds {
                        let mut sample = cmd_samples
                            .iter()
                            .find(|(sample_seq, _)| *sample_seq == seq)
                            .map(|(_, sample)| *sample);
                        if let Some(claim) = sample.as_mut() {
                            if claim.quality.claims_history()
                                && !self
                                    .replication
                                    .get(&header.connection)
                                    .is_some_and(|peer| {
                                        peer.sent_ticks.contains(&claim.left)
                                            && peer.sent_ticks.contains(&claim.right)
                                    })
                            {
                                claim.alpha = f32::NAN;
                            }
                        }
                        cmd_inbox.push(client, Some(seq), cmd, sample);
                    }
                    if control {
                        for action in actions {
                            action_inbox.push_from_peer(client, action);
                        }
                    }
                }
                ClientPacket::SnapshotAck {
                    header,
                    snapshot_seq,
                } => {
                    if self.peers.get(&header.connection) != Some(&from) {
                        continue;
                    }
                    if !header.applies_to_epoch(self.live_packet_epoch()) {
                        diag::warn!(
                            Net,
                            "authority ignored snapshot ack for epoch {} (live {})",
                            header.epoch,
                            self.live_packet_epoch()
                        );
                        continue;
                    }
                    if let Some(peer) = self.replication.get_mut(&header.connection) {
                        peer.in_ack = header.sequence;
                        let _ = peer.baseline.ack(snapshot_seq);
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_admission_acks(&mut self) {
        let Some(lane) = &self.bootstrap else {
            return;
        };
        let live = lane.epoch();
        for ack in lane.take_acks() {
            self.commit_admission(ack, live);
        }
    }

    fn commit_admission(&mut self, ack: BootstrapAck, live: u32) {
        if !epoch_applies(ack.epoch, live) {
            diag::warn!(
                Net,
                "authority ignored bootstrap applied for epoch {} (live {live})",
                ack.epoch
            );
            return;
        }
        match self.member_by_conn.get(&ack.connection).copied() {
            Some(mapped) if mapped == ack.member_id => {}
            _ => {
                diag::warn!(
                    Net,
                    "authority ignored bootstrap applied: connection {:?} does not map to member {}",
                    ack.connection,
                    ack.member_id
                );
                return;
            }
        }
        let Some(peer) = self.replication.get_mut(&ack.connection) else {
            diag::warn!(
                Net,
                "authority ignored bootstrap applied: no peer on {:?}",
                ack.connection
            );
            return;
        };
        let pending = match &peer.admission {
            PeerAdmission::Pending {
                bootstrap_id,
                snapshot_seq,
                epoch,
                ..
            } => Some((*bootstrap_id, *snapshot_seq, *epoch)),
            _ => None,
        };
        let committed = match &peer.admission {
            PeerAdmission::Committed { bootstrap_id } => Some(*bootstrap_id),
            _ => None,
        };
        let action = applied_action(
            pending,
            committed,
            ack.bootstrap_id,
            ack.snapshot_seq,
            ack.epoch,
        );
        let first_commit = match action {
            AppliedAction::Ignore => {
                diag::warn!(
                    Net,
                    "authority ignored bootstrap applied {} for member {}: no matching offer",
                    ack.bootstrap_id,
                    ack.member_id
                );
                return;
            }
            AppliedAction::RepeatEnter => false,
            AppliedAction::Commit => {
                peer.admission = PeerAdmission::Committed {
                    bootstrap_id: ack.bootstrap_id,
                };
                let _ = peer.baseline.ack(ack.snapshot_seq);
                true
            }
        };
        self.committed_admissions.push(CommittedAdmission {
            member_id: ack.member_id,
            epoch: ack.epoch,
            bootstrap_id: ack.bootstrap_id,
            connection_id: ack.connection.0,
            first_commit,
        });
    }

    pub fn fanout(
        &mut self,
        input: &TickInput,
        snapshot: &Snapshot,
        acks: &[(ClientId, CmdSeq)],
    ) -> Result<(), UdpSendError> {
        self.fanout_with_seats(
            input,
            snapshot,
            acks,
            |_, live| live.clone(),
            None,
            None,
            None,
            None,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fanout_with_seats(
        &mut self,
        input: &TickInput,
        snapshot: &Snapshot,
        acks: &[(ClientId, CmdSeq)],
        mut for_peer: impl FnMut(ClientId, &Snapshot) -> Snapshot,
        mut pending_svc: Option<&mut crate::PendingSvcSounds>,
        mut pending_playercard: Option<&mut crate::PendingPlayerCard>,
        mut pending_gamenotify: Option<&mut crate::PendingGameNotify>,
        reliable: Option<&crate::ReliableEventHub>,
        scores_due: bool,
    ) -> Result<(), UdpSendError> {
        let mut last_err = None;
        if let Some(socket) = self.socket.as_mut() {
            for (addr, packet) in self.pending_replies.drain(..) {
                if let Err(e) = socket.send_to(&packet.to_bytes(), addr) {
                    last_err = Some(e);
                }
            }
        } else {
            self.pending_replies.clear();
        }
        let live_epoch = self.live_packet_epoch();
        let peer_ids: Vec<ConnectionId> = self.peers.keys().copied().collect();
        for conn in peer_ids {
            let Some(target) = self.peers.get(&conn).copied() else {
                continue;
            };
            let Some(client_u32) = self.connections.resolve(conn, 0).ok() else {
                continue;
            };
            let client = ClientId(client_u32);
            let peer = self
                .replication
                .entry(conn)
                .or_insert_with(PeerReplicationState::new);
            if peer.admits_gameplay(self.bootstrap.is_some()) {
                if let (Some(mailbox), PeerTarget::Relay(member), Some(reliable)) =
                    (&self.relay, target, reliable)
                {
                    let payload = reliable.payload(client);
                    if !payload.rows.is_empty() {
                        let packet = ServerPacket::Control {
                            header: PacketHeader {
                                connection: conn,
                                sequence: 0,
                                ack: 0,
                                epoch: live_epoch,
                            },
                            payload,
                        };
                        if let Err(error) = mailbox.push_control_outbound(member, packet.to_bytes())
                        {
                            last_err = Some(relay_send_error(error));
                        }
                    }
                }
            }
            let resync = peer.baseline.clear_missing_ack();
            if resync {
                diag::warn!(
                    Net,
                    "acked baseline missing for {target:?}: sending a full snapshot"
                );
            }
            let baseline_seq = peer.baseline.baseline_seq_for_encode();
            if !peer.baseline.may_encode_against(baseline_seq) {
                diag::warn!(
                    Net,
                    "snapshot encode refused for {target:?} against baseline {baseline_seq}"
                );
                continue;
            }
            if baseline_seq == 0
                && !resync
                && let Some(last) = peer.last_full
                && snapshot.tick.0.saturating_sub(last.0) < FULL_SNAPSHOT_RESEND_TICKS
            {
                continue;
            }
            if baseline_seq == 0
                && let Some(lane) = &self.bootstrap
                && lane.epoch() == 0
            {
                continue;
            }
            let mut encoder = std::mem::take(&mut peer.encoder);
            if baseline_seq == 0 {
                peer.last_full = Some(snapshot.tick);
                encoder.reset();
            } else if let Some(baseline) = peer.baseline.baseline_for_encode() {
                encoder.adopt_baseline(baseline);
            }
            let peer_acks: Vec<(ClientId, CmdSeq)> = acks
                .iter()
                .copied()
                .filter(|(id, _)| *id == client)
                .collect();
            let peer_snap = for_peer(client, snapshot);
            let mut frame = frame_from_acked_tick(&mut encoder, input, &peer_snap, peer_acks);
            peer.encoder = encoder;
            if let Some(pending) = pending_svc.as_mut() {
                frame.svc_sounds = pending.take_for(client);
            }
            if let Some(pending) = pending_playercard.as_mut() {
                let (slots, menus, splashes) = pending.take_for(client);
                frame.svc_card_slots = slots;
                frame.svc_open_menus = menus;
                frame.svc_hud_splashes = splashes;
            }
            if let Some(pending) = pending_gamenotify.as_mut() {
                frame.svc_game_notifies = pending.take_broadcast();
            }

            frame
                .snapshot_meta
                .journal
                .retain(|record| !sim::sim_event_is_reliable(&record.event));

            if scores_due {
                frame.svc_scores = Some(crate::format_scoreboard_from_snapshot(&peer_snap));
            }
            let snapshot_seq = peer.next_snap_seq;
            peer.next_snap_seq = snapshot_seq.wrapping_add(1);
            peer.baseline.remember(snapshot_seq, peer_snap);
            peer.out_seq = peer.out_seq.wrapping_add(1);
            let header = PacketHeader {
                connection: conn,
                sequence: peer.out_seq,
                ack: peer.in_ack,
                epoch: live_epoch,
            };
            let packet = ServerPacket::Snapshot {
                header,
                baseline_seq,
                snapshot_seq,
                payload: frame.to_bytes(),
            };
            let relay_bootstrap = self.bootstrap.is_some() && baseline_seq == 0;
            let already_admitted = matches!(peer.admission, PeerAdmission::Committed { .. });
            if relay_bootstrap && !already_admitted {
                let Some(lane) = &self.bootstrap else {
                    continue;
                };
                let epoch = lane.epoch();
                if let PeerAdmission::Pending {
                    epoch: pending_epoch,
                    ..
                } = &peer.admission
                    && *pending_epoch == epoch
                {
                    continue;
                }
                let bootstrap_id = {
                    let id = self.next_bootstrap_id.entry(conn).or_insert(1);
                    let current = *id;
                    *id = id.wrapping_add(1).max(1);
                    current
                };
                let packet_bytes = packet.to_bytes();
                let tick_b = snapshot.tick.0;
                match BootstrapTransaction::from_snapshot(
                    epoch,
                    bootstrap_id,
                    tick_b,
                    snapshot_seq,
                    conn,
                    packet_bytes,
                ) {
                    Ok(txn) => {
                        peer.admission = PeerAdmission::Pending {
                            bootstrap_id,
                            snapshot_seq,
                            epoch,
                            tick_b,
                            offer_bytes: txn.offer_bytes.clone(),
                        };
                        diag::info!(
                            Net,
                            "bootstrap-io session=host event=bootstrap offer encoded peer={target:?} epoch={epoch} bootstrap_id={bootstrap_id} snapshot_seq={snapshot_seq} bytes={}",
                            txn.offer_bytes.len()
                        );
                        let member = match target {
                            PeerTarget::Relay(member) => Some(member),
                            PeerTarget::Udp(_) => self.member_by_conn.get(&conn).copied(),
                        };
                        if let Err(error) = lane.push_to_worker(member, txn.offer_bytes) {
                            diag::warn!(Net, "bootstrap offer dropped for {target:?}: {error}");
                        } else {
                            peer.sent_ticks.push_back(snapshot.tick);
                            while peer.sent_ticks.len() > 32 {
                                peer.sent_ticks.pop_front();
                            }
                        }
                    }
                    Err(error) => {
                        diag::warn!(Net, "bootstrap offer encode failed for {target:?}: {error}");
                    }
                }
                continue;
            }
            if let Err(e) = self.send_outgoing(target, &packet.to_bytes()) {
                last_err = Some(e);
            } else if let Some(peer) = self.replication.get_mut(&conn) {
                peer.sent_ticks.push_back(snapshot.tick);
                while peer.sent_ticks.len() > 32 {
                    peer.sent_ticks.pop_front();
                }
            }
        }
        match last_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[derive(Resource)]
pub struct UdpClientLink {
    socket: Option<UdpDatagramSocket>,
    relay: Option<RelayMailbox>,
    pub server: SocketAddr,
    pub hello: HandshakeHello,
    pub limits: ProtocolLimits,
    pub connection: Option<ConnectionId>,
    pub assigned_client: Option<ClientId>,

    baselines: BTreeMap<u32, Snapshot>,
    out_seq: u32,
    in_ack: u32,
    last_snapshot_seq: Option<u32>,
    applied_bootstrap_id: Option<u32>,
    last_applied_offer: Option<(u32, u32)>,
    pending_applied: Vec<BootstrapMessage>,

    held_bootstrap: Vec<Vec<u8>>,

    failed: Option<HandshakeReject>,
    bootstrap: Option<Arc<BootstrapLane>>,
    controls: Vec<crate::ReliablePayload>,
    sent_actions: HashSet<sim::ActionRequestId>,
}

impl UdpClientLink {
    pub fn connect(server: SocketAddr, hello: HandshakeHello) -> std::io::Result<Self> {
        let limits = hello.limits;
        let socket = UdpDatagramSocket::bind("0.0.0.0:0", &limits)?;
        Ok(Self {
            socket: Some(socket),
            relay: None,
            server,
            hello,
            limits,
            connection: None,
            assigned_client: None,
            baselines: BTreeMap::new(),
            out_seq: 0,
            in_ack: 0,
            last_snapshot_seq: None,
            applied_bootstrap_id: None,
            last_applied_offer: None,
            pending_applied: Vec::new(),
            controls: Vec::new(),
            sent_actions: HashSet::new(),
            held_bootstrap: Vec::new(),
            failed: None,
            bootstrap: None,
        })
    }

    pub fn relay(hello: HandshakeHello, mailbox: RelayMailbox) -> Self {
        let limits = hello.limits;
        Self {
            socket: None,
            relay: Some(mailbox),
            server: "0.0.0.0:0".parse().expect("literal"),
            hello,
            limits,
            connection: None,
            assigned_client: None,
            baselines: BTreeMap::new(),
            out_seq: 0,
            in_ack: 0,
            last_snapshot_seq: None,
            applied_bootstrap_id: None,
            last_applied_offer: None,
            pending_applied: Vec::new(),
            controls: Vec::new(),
            sent_actions: HashSet::new(),
            held_bootstrap: Vec::new(),
            failed: None,
            bootstrap: None,
        }
    }

    pub fn mailbox(&self) -> Option<RelayMailbox> {
        self.relay.clone()
    }

    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), UdpSendError> {
        if let Some(socket) = self.socket.as_mut() {
            return socket.send_to(bytes, self.server);
        }
        let mailbox = self
            .relay
            .as_ref()
            .ok_or_else(|| relay_send_error("client link has no carrier"))?;
        mailbox
            .push_outbound(MemberId([0; 16]), bytes.to_vec())
            .map_err(relay_send_error)
    }

    pub fn attach_bootstrap(&mut self, lane: Arc<BootstrapLane>) {
        self.bootstrap = Some(lane);
    }

    pub fn should_offer_connect(&self) -> bool {
        self.relay.is_none() && self.connection.is_none() && self.failed.is_none()
    }

    pub fn handshake_reject(&self) -> Option<HandshakeReject> {
        self.failed
    }

    pub fn has_applied_snapshot(&self) -> bool {
        self.last_snapshot_seq.is_some()
    }

    pub fn has_applied_direct_snapshot(&self) -> bool {
        self.relay.is_none() && self.connection.is_some() && self.has_applied_snapshot()
    }

    pub fn match_epoch(&self) -> u32 {
        self.bootstrap
            .as_ref()
            .map(|lane| lane.epoch())
            .unwrap_or(0)
    }

    pub fn has_entered_match(&self) -> bool {
        let Some(lane) = &self.bootstrap else {
            return true;
        };
        matches!(
            self.applied_bootstrap_id,
            Some(applied) if applied != 0 && applied == lane.entered_bootstrap()
        )
    }

    pub fn admission_bootstrap_id(&self) -> Option<u32> {
        self.applied_bootstrap_id.filter(|id| *id != 0)
    }

    pub fn note_applied_bootstrap(&mut self, bootstrap_id: u32) {
        self.applied_bootstrap_id = Some(bootstrap_id);
    }

    pub fn note_reject(&mut self, reason: HandshakeReject) {
        self.failed = Some(reason);
    }

    pub fn note_accept(&mut self, connection: ConnectionId, client: ClientId) {
        if self.failed.is_some() {
            return;
        }
        self.connection = Some(connection);
        self.assigned_client = Some(client);
    }

    pub fn note_applied_snapshot(&mut self, snapshot_seq: u32) {
        self.last_snapshot_seq.replace(snapshot_seq);
    }

    pub fn flush_applied_after_adopt(&mut self) {
        let pending = std::mem::take(&mut self.pending_applied);
        let Some(lane) = self.bootstrap.clone() else {
            self.pending_applied = pending;
            return;
        };
        let mut leftover = Vec::new();
        let mut blocked = false;
        for applied in pending {
            if blocked {
                leftover.push(applied);
                continue;
            }
            if let BootstrapMessage::Applied { bootstrap_id, .. } = &applied {
                self.applied_bootstrap_id = Some(*bootstrap_id);
            }
            match encode_bootstrap(&applied) {
                Ok(bytes) => {
                    if let Err(error) = lane.push_to_worker(None, bytes) {
                        diag::warn!(Net, "bootstrap applied not queued: {error}");
                        leftover.push(applied);
                        blocked = true;
                    }
                }
                Err(error) => {
                    diag::warn!(Net, "bootstrap applied encode failed: {error}");
                    leftover.push(applied);
                    blocked = true;
                }
            }
        }
        self.pending_applied = leftover;
    }

    pub fn reset_match(&mut self) {
        self.connection = None;
        self.assigned_client = None;
        self.baselines.clear();
        self.out_seq = 0;
        self.in_ack = 0;
        self.last_snapshot_seq = None;
        self.applied_bootstrap_id = None;
        self.last_applied_offer = None;
        self.pending_applied.clear();
        self.held_bootstrap.clear();
        self.controls.clear();
        self.sent_actions.clear();
        self.failed = None;
        if let Some(lane) = &self.bootstrap {
            lane.set_epoch(0);
        }
    }

    pub fn ensure_connected(&mut self) -> Result<(), UdpSendError> {
        if !self.should_offer_connect() {
            return Ok(());
        }
        let bytes = ClientPacket::Connect(self.hello).to_bytes();
        self.send_bytes(&bytes)
    }

    pub fn take_controls(&mut self) -> Vec<crate::ReliablePayload> {
        std::mem::take(&mut self.controls)
    }

    pub fn recv_ticks(&mut self) -> Result<Vec<ReceivedTick>, String> {
        let mut ticks = Vec::new();
        let mut snap_acks = Vec::new();
        let mut datagrams = Vec::new();
        if let Some(socket) = self.socket.as_mut() {
            socket
                .recv_budget(DEFAULT_RECV_BUDGET_PER_TICK, &mut datagrams)
                .map_err(|e| e.to_string())?;
        }
        let mut packets: Vec<Vec<u8>> = datagrams
            .into_iter()
            .filter(|(_, source)| *source == self.server)
            .map(|(bytes, _)| bytes)
            .collect();
        if let Some(mailbox) = &self.relay {
            packets.extend(mailbox.take_inbound().into_iter().map(|(_, bytes)| bytes));
        }
        for bytes in packets {
            let packet = match decode_server_packet(&bytes, &self.limits) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !matches!(packet, ServerPacket::Control { .. }) {
                self.apply_server_packet(packet, &mut ticks, &mut snap_acks)?;
            }
        }
        self.drain_bootstrap_offers(&mut ticks, &mut snap_acks)?;
        if let Some(mailbox) = self.relay.clone() {
            for (_, bytes) in mailbox.take_control_inbound() {
                let packet =
                    decode_server_packet(&bytes, &self.limits).map_err(|e| e.to_string())?;
                if matches!(packet, ServerPacket::Control { .. }) {
                    self.apply_server_packet(packet, &mut ticks, &mut snap_acks)?;
                }
            }
        }
        self.adopt_entered_client();
        if self.bootstrap.is_none() {
            for seq in snap_acks {
                let _ = self.send_snapshot_ack(seq);
            }
        }
        Ok(ticks)
    }

    fn adopt_entered_client(&mut self) {
        let Some(lane) = &self.bootstrap else {
            return;
        };
        let entered = lane.entered_bootstrap();
        if entered == 0 || self.applied_bootstrap_id != Some(entered) {
            return;
        }
        let client = ClientId(lane.entered_client());
        if client.0 != 0 && self.assigned_client != Some(client) {
            self.assigned_client = Some(client);
        }
    }

    fn drain_bootstrap_offers(
        &mut self,
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<(), String> {
        let (live, from_lane) = {
            let Some(lane) = &self.bootstrap else {
                return Ok(());
            };
            (lane.epoch(), lane.take_from_worker())
        };
        let mut offers = std::mem::take(&mut self.held_bootstrap);
        offers.extend(from_lane.into_iter().map(|ingress| ingress.bytes));
        for bytes in offers {
            match decode_bootstrap(&bytes) {
                Ok(Some(BootstrapMessage::Offer {
                    epoch,
                    bootstrap_id,
                    tick_b,
                    snapshot_seq,
                    connection,
                    packet,
                })) => {
                    if !epoch_applies(epoch, live) {
                        diag::warn!(
                            Net,
                            "client ignored bootstrap offer for epoch {epoch} (live {live})"
                        );
                        continue;
                    }
                    diag::info!(
                        Net,
                        "bootstrap-io session=join event=bootstrap offer epoch={epoch} live={live} bootstrap_id={bootstrap_id} snapshot_seq={snapshot_seq} bytes={}",
                        packet.len()
                    );
                    if self.connection.is_none() {
                        self.connection = Some(connection);
                    }
                    match self.apply_server_bytes(&packet, ticks, snap_acks) {
                        Ok(true) => {
                            self.last_applied_offer = Some((bootstrap_id, snapshot_seq));
                            self.pending_applied.push(BootstrapMessage::Applied {
                                epoch,
                                bootstrap_id,
                                tick_b,
                                snapshot_seq,
                                connection,
                            });
                        }
                        Ok(false) => {
                            if self.last_applied_offer == Some((bootstrap_id, snapshot_seq)) {
                                self.pending_applied.push(BootstrapMessage::Applied {
                                    epoch,
                                    bootstrap_id,
                                    tick_b,
                                    snapshot_seq,
                                    connection,
                                });
                            }
                        }
                        Err(error) => {
                            diag::warn!(
                                Net,
                                "bootstrap offer named the connection; snapshot not applied: {error}"
                            );
                        }
                    }
                }
                Ok(Some(BootstrapMessage::Applied { .. })) => {
                    diag::warn!(Net, "client ignored a bootstrap applied on the offer lane");
                }
                Ok(None) => {
                    diag::warn!(Net, "client ignored non-bootstrap bytes on the offer lane");
                }
                Err(error) => {
                    diag::warn!(Net, "client ignored malformed bootstrap offer: {error}");
                }
            }
        }
        Ok(())
    }

    fn apply_server_bytes(
        &mut self,
        bytes: &[u8],
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<bool, String> {
        let packet = decode_server_packet(bytes, &self.limits).map_err(|e| e.to_string())?;
        self.apply_server_packet(packet, ticks, snap_acks)
    }

    fn apply_server_packet(
        &mut self,
        packet: ServerPacket,
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<bool, String> {
        match packet {
            ServerPacket::Control { header, payload } => {
                if self.failed.is_none()
                    && self.connection == Some(header.connection)
                    && header.applies_to_epoch(self.match_epoch())
                {
                    self.controls.push(payload);
                }
                Ok(false)
            }
            ServerPacket::Accept {
                connection,
                assigned_client,
                ..
            } => {
                if self.failed.is_none() {
                    self.note_accept(connection, ClientId(assigned_client));
                }
                Ok(false)
            }
            ServerPacket::Reject(reason) => {
                self.note_reject(reason);
                let ours = self.hello.content;
                Err(format!(
                    "handshake rejected: {reason}; our content gameplay={:016x} \
                     map={:016x} models={:016x} weapons={:016x} classes={:016x}",
                    ours.gameplay, ours.map, ours.models, ours.weapons, ours.classes
                ))
            }
            ServerPacket::Snapshot {
                header,
                baseline_seq,
                snapshot_seq,
                payload,
            } => {
                if self.failed.is_some() || self.connection != Some(header.connection) {
                    return Ok(false);
                }
                if !header.applies_to_epoch(self.match_epoch()) {
                    diag::warn!(
                        Net,
                        "client ignored snapshot for epoch {} (live {})",
                        header.epoch,
                        self.match_epoch()
                    );
                    return Ok(false);
                }
                if self.baselines.contains_key(&snapshot_seq) {
                    return Ok(false);
                }
                let baseline = if baseline_seq == 0 {
                    None
                } else {
                    match self.baselines.get(&baseline_seq).cloned() {
                        Some(baseline) => Some(baseline),
                        None => return Ok(false),
                    }
                };
                let mut decoder = SnapshotDecoder::new();
                let mut world_decoder = WorldObjectSyncDecoder::default();
                if let Some(baseline) = baseline.as_ref() {
                    decoder.adopt_baseline(baseline);
                    world_decoder.adopt_baseline(baseline.meta.world_objects.clone());
                }
                self.in_ack = header.sequence;
                let mut input = WireReader::new(&payload);
                let frame =
                    Frame::decode(&mut input, &mut world_decoder).map_err(|e| e.to_string())?;
                let mut snapshot = decoder
                    .decode(&frame.snapshot_delta)
                    .map_err(|e| e.to_string())?;
                snapshot.meta = frame.snapshot_meta.clone();
                self.note_applied_snapshot(snapshot_seq);
                self.baselines.insert(snapshot_seq, snapshot.clone());
                self.retain_applied_baseline();
                snap_acks.push(snapshot_seq);
                ticks.push(ReceivedTick { snapshot, frame });
                Ok(true)
            }
        }
    }

    fn retain_applied_baseline(&mut self) {
        let pinned = self.last_snapshot_seq;
        while self.baselines.len() > 64 {
            let oldest = self
                .baselines
                .keys()
                .copied()
                .find(|seq| Some(*seq) != pinned);
            match oldest {
                Some(seq) => {
                    self.baselines.remove(&seq);
                }
                None => break,
            }
        }
    }

    fn send_snapshot_ack(&mut self, snapshot_seq: u32) -> Result<(), UdpSendError> {
        let Some(connection) = self.connection else {
            return Ok(());
        };
        self.out_seq = self.out_seq.wrapping_add(1);
        let packet = ClientPacket::SnapshotAck {
            header: PacketHeader {
                connection,
                sequence: self.out_seq,
                ack: self.in_ack,
                epoch: self.match_epoch(),
            },
            snapshot_seq,
        };
        self.send_bytes(&packet.to_bytes())
    }

    pub fn has_unsent_actions(&self, actions: &[ClientAction]) -> bool {
        actions
            .iter()
            .any(|action| !self.sent_actions.contains(&sim::action_request_id(action)))
    }

    pub fn send_commands(
        &mut self,
        cmds: &[(CmdSeq, UserCmd, sim::ShotSampleProvenance)],
        actions: &[ClientAction],
        reliable_ack: u16,
    ) -> Result<(), UdpSendError> {
        let Some(connection) = self.connection else {
            return Ok(());
        };
        if let Some(seq) = self.last_snapshot_seq {
            let _ = self.send_snapshot_ack(seq);
        }

        const CMDS_PER_PACKET: usize = 16;
        let count = CMDS_PER_PACKET.min(usize::from(self.limits.max_cmds_per_tick));
        if count == 0 {
            return Err(relay_send_error("peer permits no commands per packet"));
        }
        for chunk in cmds.chunks(count) {
            self.send_command_packet(connection, chunk, &[], reliable_ack)?;
        }

        self.sent_actions.retain(|id| {
            actions
                .iter()
                .any(|action| sim::action_request_id(action) == *id)
        });
        let fresh: Vec<_> = actions
            .iter()
            .copied()
            .filter(|action| !self.sent_actions.contains(&sim::action_request_id(action)))
            .collect();
        if !fresh.is_empty() || cmds.is_empty() {
            self.send_command_packet(connection, &[], &fresh, reliable_ack)?;
            self.sent_actions
                .extend(fresh.iter().map(sim::action_request_id));
        }
        Ok(())
    }

    fn send_command_packet(
        &mut self,
        connection: ConnectionId,
        cmds: &[(CmdSeq, UserCmd, sim::ShotSampleProvenance)],
        actions: &[ClientAction],
        reliable_ack: u16,
    ) -> Result<(), UdpSendError> {
        self.out_seq = self.out_seq.wrapping_add(1);
        let packet = ClientPacket::Commands {
            header: PacketHeader {
                connection,
                sequence: self.out_seq,
                ack: self.in_ack,
                epoch: self.match_epoch(),
            },
            claimed_client: self.assigned_client.map(|c| c.0).unwrap_or(0),
            cmds: cmds.iter().map(|(seq, cmd, _)| (*seq, *cmd)).collect(),
            samples: cmds
                .iter()
                .map(|(seq, _, sample)| (*seq, *sample))
                .collect(),
            actions: actions.to_vec(),
            reliable_ack,
        };
        if cmds.is_empty() {
            if let Some(mailbox) = &self.relay {
                mailbox
                    .push_control_outbound(MemberId([0; 16]), packet.to_bytes())
                    .map_err(relay_send_error)
            } else if actions.is_empty() {
                self.send_bytes(&packet.to_bytes())
            } else {
                Err(relay_send_error(
                    "transaction requires the QUIC control carrier",
                ))
            }
        } else {
            self.send_bytes(&packet.to_bytes())
        }
    }
}
