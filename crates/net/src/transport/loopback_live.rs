use bevy::prelude::Resource;
use sim::{ClientId, Snapshot};

use crate::client::predict::CmdSeq;
use crate::transport::delta::{SnapshotDecoder, SnapshotEncoder};
use crate::transport::frame::{
    Frame, LoopbackTransport, Transport, TransportError, frame_from_acked_tick,
};

#[derive(Resource, Debug, Default)]
pub struct ListenLoopback {
    transport: LoopbackTransport,
    encoder: SnapshotEncoder,
    decoder: SnapshotDecoder,

    pub sent: u64,

    pub received: u64,

    pub last_backlog: usize,

    pub peak_backlog: usize,

    direct: std::collections::VecDeque<ReceivedTick>,
}

impl ListenLoopback {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> usize {
        self.transport.pending() + self.direct.len()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_tick(
        &mut self,
        snapshot: &Snapshot,
        acks: Vec<(ClientId, CmdSeq)>,
        svc_sounds: Vec<crate::SvcSound>,
        svc_scores: Option<String>,
        svc_card_slots: Vec<crate::SvcCardSlot>,
        svc_open_menus: Vec<crate::SvcOpenMenu>,
        svc_hud_splashes: Vec<crate::SvcHudSplash>,
        svc_game_notifies: Vec<crate::SvcGameNotify>,
        reliable: crate::ReliablePayload,
    ) -> Result<(), TransportError> {
        if asset_transport::campaign_zone_set() {
            self.direct.push_back(ReceivedTick {
                snapshot: snapshot.clone(),
                frame: Frame {
                    tick: snapshot.tick,
                    state_hash: 0,
                    acks,
                    snapshot_delta: crate::transport::delta::SnapshotDelta::empty(snapshot.tick),
                    snapshot_meta: Default::default(),
                    world_objects_wire: Vec::new(),
                    reliable,
                    svc_sounds,
                    svc_scores,
                    svc_card_slots,
                    svc_open_menus,
                    svc_hud_splashes,
                    svc_game_notifies,
                },
            });
            self.sent = self.sent.saturating_add(1);
            return Ok(());
        }
        let mut frame = frame_from_acked_tick(&mut self.encoder, snapshot, acks);
        frame.svc_sounds = svc_sounds;
        frame.svc_scores = svc_scores;
        frame.svc_card_slots = svc_card_slots;
        frame.svc_open_menus = svc_open_menus;
        frame.svc_hud_splashes = svc_hud_splashes;
        frame.svc_game_notifies = svc_game_notifies;

        frame.reliable = reliable;
        self.transport.send(&frame)?;
        self.sent = self.sent.saturating_add(1);
        Ok(())
    }

    pub fn recv_tick(&mut self) -> Result<Option<ReceivedTick>, TransportError> {
        if let Some(tick) = self.direct.pop_front() {
            self.received = self.received.saturating_add(1);
            return Ok(Some(tick));
        }
        let Some(frame) = self.transport.recv()? else {
            return Ok(None);
        };
        let mut snapshot = self.decoder.decode(&frame.snapshot_delta)?;
        snapshot.meta = frame.snapshot_meta.clone();
        self.received = self.received.saturating_add(1);
        Ok(Some(ReceivedTick { snapshot, frame }))
    }

    pub fn recv_all(&mut self, out: &mut Vec<ReceivedTick>) -> Result<(), TransportError> {
        self.last_backlog = self.transport.pending() + self.direct.len();
        self.peak_backlog = self.peak_backlog.max(self.last_backlog);
        loop {
            match self.recv_tick() {
                Ok(Some(tick)) => out.push(tick),
                Ok(None) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug)]
pub struct ReceivedTick {
    pub snapshot: Snapshot,
    pub frame: Frame,
}

impl ReceivedTick {
    pub fn ack_for(&self, client: ClientId) -> Option<CmdSeq> {
        self.frame.ack_for(client)
    }
}
