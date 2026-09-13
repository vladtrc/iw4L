use playerstate_iw4::UserCmd;
use sim::{ClientAction, ClientId, Snapshot, SnapshotMeta, Tick, TickInput};

use crate::client::predict::CmdSeq;
use crate::transport::delta::{SnapshotDelta, decode_usercmd, encode_usercmd};
use crate::transport::meta_wire::{
    SnapshotMetaSectionBytes, WorldObjectSyncDecoder, decode_actions, decode_snapshot_meta,
    encode_actions, encode_snapshot_meta_sections,
};
use crate::transport::netfields::compute_state_hash;
use crate::transport::reliable::{
    ReliablePayload, decode_reliable_payload, encode_reliable_payload,
};
use crate::transport::wire::{WireError, WireReader, WireWriter};

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub tick: Tick,

    pub state_hash: u32,
    pub cmds: Vec<(ClientId, UserCmd)>,
    pub actions: Vec<(ClientId, ClientAction)>,

    pub acks: Vec<(ClientId, CmdSeq)>,
    pub snapshot_delta: SnapshotDelta,

    pub snapshot_meta: SnapshotMeta,

    pub world_objects_wire: Vec<u8>,

    pub reliable: ReliablePayload,

    pub svc_sounds: Vec<crate::SvcSound>,

    pub svc_scores: Option<String>,

    pub svc_card_slots: Vec<crate::SvcCardSlot>,

    pub svc_open_menus: Vec<crate::SvcOpenMenu>,
    pub svc_hud_splashes: Vec<crate::SvcHudSplash>,

    pub svc_game_notifies: Vec<crate::SvcGameNotify>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameSectionBytes {
    pub header: usize,
    pub snapshot_delta: usize,
    pub meta: SnapshotMetaSectionBytes,
    pub reliable: usize,
    pub svc: usize,
    pub total: usize,
}

impl FrameSectionBytes {
    pub fn named_sum(self) -> usize {
        self.header + self.snapshot_delta + self.meta.total() + self.reliable + self.svc
    }
}

impl Frame {
    pub fn encode(&self, out: &mut WireWriter) {
        let _ = self.encode_sections(out);
    }

    pub fn section_bytes(&self) -> FrameSectionBytes {
        let mut out = WireWriter::new();
        self.encode_sections(&mut out)
    }

    fn encode_sections(&self, out: &mut WireWriter) -> FrameSectionBytes {
        let start = out.len();
        let mut mark = start;
        out.put_u32(self.tick.0);
        out.put_u32(self.state_hash);
        debug_assert!(
            self.cmds.len() <= u16::MAX as usize,
            "command count exceeds the wire width"
        );
        out.put_u16(self.cmds.len() as u16);
        for (client, cmd) in &self.cmds {
            out.put_u32(client.0);
            encode_usercmd(out, cmd);
        }
        encode_actions(out, &self.actions);
        debug_assert!(self.acks.len() <= u16::MAX as usize);
        out.put_u16(self.acks.len() as u16);
        for (client, seq) in &self.acks {
            out.put_u32(client.0);
            out.put_u32(seq.0);
        }
        let header = out.len() - mark;
        mark = out.len();
        self.snapshot_delta.encode(out);
        let snapshot_delta = out.len() - mark;
        let meta =
            encode_snapshot_meta_sections(out, &self.snapshot_meta, &self.world_objects_wire);
        mark = out.len();
        encode_reliable_payload(
            out,
            self.reliable.ack_through,
            &self.reliable.rows,
            self.reliable.dropped_oldest,
        );
        let reliable = out.len() - mark;
        mark = out.len();
        crate::svc_sound::encode_svc_sounds(out, &self.svc_sounds);
        crate::svc_scores::encode_svc_scores(out, self.svc_scores.as_deref());
        crate::svc_playercard::encode_svc_card_slots(out, &self.svc_card_slots);
        crate::svc_playercard::encode_svc_open_menus(out, &self.svc_open_menus);
        crate::svc_playercard::encode_svc_hud_splashes(out, &self.svc_hud_splashes);
        crate::svc_gamenotify::encode_svc_game_notifies(out, &self.svc_game_notifies);
        let svc = out.len() - mark;
        FrameSectionBytes {
            header,
            snapshot_delta,
            meta,
            reliable,
            svc,
            total: out.len() - start,
        }
    }

    pub fn decode(
        input: &mut WireReader<'_>,
        world_decoder: &mut WorldObjectSyncDecoder,
    ) -> Result<Self, WireError> {
        let tick = Tick(input.get_u32()?);
        let state_hash = input.get_u32()?;
        let cmd_count = input.get_u16()? as usize;
        let mut cmds = Vec::with_capacity(cmd_count.min(64));
        for _ in 0..cmd_count {
            let client = ClientId(input.get_u32()?);
            cmds.push((client, decode_usercmd(input)?));
        }
        let actions = decode_actions(input)?;
        let ack_count = input.get_u16()? as usize;
        let mut acks = Vec::with_capacity(ack_count.min(64));
        for _ in 0..ack_count {
            let client = ClientId(input.get_u32()?);
            acks.push((client, CmdSeq(input.get_u32()?)));
        }
        let snapshot_delta = SnapshotDelta::decode(input)?;
        let (snapshot_meta, world_objects_wire) = decode_snapshot_meta(input, world_decoder)?;
        let reliable = decode_reliable_payload(input)?;
        let svc_sounds = crate::svc_sound::decode_svc_sounds(input)?;
        let svc_scores = crate::svc_scores::decode_svc_scores(input)?;
        let svc_card_slots = crate::svc_playercard::decode_svc_card_slots(input)?;
        let svc_open_menus = crate::svc_playercard::decode_svc_open_menus(input)?;
        let svc_hud_splashes = crate::svc_playercard::decode_svc_hud_splashes(input)?;
        let svc_game_notifies = crate::svc_gamenotify::decode_svc_game_notifies(input)?;
        Ok(Self {
            tick,
            state_hash,
            cmds,
            actions,
            acks,
            snapshot_delta,
            snapshot_meta,
            world_objects_wire,
            reliable,
            svc_sounds,
            svc_scores,
            svc_card_slots,
            svc_open_menus,
            svc_hud_splashes,
            svc_game_notifies,
        })
    }

    pub fn ack_for(&self, client: ClientId) -> Option<CmdSeq> {
        self.acks
            .iter()
            .find(|(id, _)| *id == client)
            .map(|(_, seq)| *seq)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = WireWriter::new();
        self.encode(&mut out);
        out.finish()
    }

    pub fn tick_input(&self) -> TickInput {
        TickInput {
            cmds: self.cmds.clone(),
            actions: self.actions.clone(),
        }
    }
}

pub trait Transport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError>;

    fn recv(&mut self) -> Result<Option<Frame>, TransportError>;
}

#[derive(Debug)]
pub enum TransportError {
    Ended,

    Wire(WireError),

    Io(std::io::Error),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::Ended => write!(f, "frame stream ended"),
            TransportError::Wire(e) => write!(f, "{e}"),
            TransportError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<WireError> for TransportError {
    fn from(value: WireError) -> Self {
        TransportError::Wire(value)
    }
}

impl From<std::io::Error> for TransportError {
    fn from(value: std::io::Error) -> Self {
        TransportError::Io(value)
    }
}

#[derive(Debug, Default)]
pub struct LoopbackTransport {
    queue: std::collections::VecDeque<Vec<u8>>,
    world_decoder: WorldObjectSyncDecoder,
}

impl LoopbackTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

impl Transport for LoopbackTransport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError> {
        self.queue.push_back(frame.to_bytes());
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        let Some(bytes) = self.queue.pop_front() else {
            return Ok(None);
        };
        let mut reader = WireReader::new(&bytes);
        Ok(Some(Frame::decode(&mut reader, &mut self.world_decoder)?))
    }
}

pub fn frame_from_tick(
    coder: &mut crate::SnapshotEncoder,
    input: &TickInput,
    snapshot: &Snapshot,
) -> Frame {
    frame_from_acked_tick(coder, input, snapshot, Vec::new())
}

pub fn frame_from_acked_tick(
    coder: &mut crate::SnapshotEncoder,
    input: &TickInput,
    snapshot: &Snapshot,
    acks: Vec<(ClientId, CmdSeq)>,
) -> Frame {
    frame_from_acked_tick_with_reliable(
        coder,
        input,
        snapshot,
        acks,
        ReliablePayload {
            ack_through: 0,
            rows: Vec::new(),
            dropped_oldest: 0,
        },
    )
}

pub fn frame_from_acked_tick_with_reliable(
    coder: &mut crate::SnapshotEncoder,
    input: &TickInput,
    snapshot: &Snapshot,
    acks: Vec<(ClientId, CmdSeq)>,
    reliable: ReliablePayload,
) -> Frame {
    let world_objects_wire = coder
        .encode_world_objects(snapshot.tick, &snapshot.meta.world_objects)
        .to_vec();
    Frame {
        tick: snapshot.tick,
        state_hash: compute_state_hash(&snapshot.players),
        cmds: input.cmds.clone(),
        actions: input.actions.clone(),
        acks,
        snapshot_delta: coder.encode(snapshot),
        snapshot_meta: snapshot.meta.clone(),
        world_objects_wire,
        reliable,
        svc_sounds: Vec::new(),
        svc_scores: None,
        svc_card_slots: Vec::new(),
        svc_open_menus: Vec::new(),
        svc_hud_splashes: Vec::new(),
        svc_game_notifies: Vec::new(),
    }
}

pub fn authoritative_snapshot_hash(snapshot: &Snapshot) -> u64 {
    let mut encoder = crate::SnapshotEncoder::new();
    let frame = frame_from_tick(&mut encoder, &TickInput::default(), snapshot);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in frame.to_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
