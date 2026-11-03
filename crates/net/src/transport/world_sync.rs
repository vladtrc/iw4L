use super::meta_wire::{META_SEGMENTS, decode_snapshot_meta_body};
use super::wire::{WireError, WireReader};
use super::world_object_wire::{self, PairDelta, WorldObjectChange, WorldObjectUpdate};
use sim::{SnapshotMeta, Tick, WorldObjectSnapshot};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

pub const WORLD_SYNC_PERIOD_TICKS: u32 = 200;

#[derive(Debug, Default)]
pub struct WorldObjectSyncEncoder {
    baseline: WorldObjectSnapshot,
    force_full: bool,
}

impl WorldObjectSyncEncoder {
    pub fn reset(&mut self) {
        self.baseline = WorldObjectSnapshot::default();
        self.force_full = true;
    }

    pub fn adopt_baseline(&mut self, baseline: WorldObjectSnapshot) {
        self.baseline = baseline;
        self.force_full = false;
    }

    pub fn encode(&mut self, tick: Tick, current: &WorldObjectSnapshot) -> Vec<u8> {
        let full = self.force_full || tick.0 % WORLD_SYNC_PERIOD_TICKS == 0;
        let encoded = world_object_wire::encode_sync(&self.baseline, current, full);
        if encoded.content_changed {
            self.baseline = current.clone();
        } else {
            self.baseline.as_of_ms = current.as_of_ms;
            self.baseline.map_round_epoch = current.map_round_epoch;
            self.baseline.fracture_profile_version = current.fracture_profile_version;
        }
        self.force_full = false;
        encoded.wire
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorldObjectSyncDecoder {
    state: WorldObjectSnapshot,
}

impl WorldObjectSyncDecoder {
    pub fn reset(&mut self) {
        self.state = WorldObjectSnapshot::default();
    }

    pub fn adopt_baseline(&mut self, baseline: WorldObjectSnapshot) {
        self.state = baseline;
    }

    pub fn state(&self) -> &WorldObjectSnapshot {
        &self.state
    }

    pub(super) fn apply(&mut self, update: WorldObjectUpdate) -> WorldObjectSnapshot {
        update.apply(&mut self.state);
        self.state.clone()
    }

    pub fn apply_wire(&mut self, wire: &[u8]) -> Result<WorldObjectSnapshot, WireError> {
        let update = world_object_wire::decode_sync(wire)?;
        Ok(self.apply(update))
    }
}

pub fn encode_world_object_sync(
    encoder: &mut WorldObjectSyncEncoder,
    tick: Tick,
    current: &WorldObjectSnapshot,
) -> Vec<u8> {
    encoder.encode(tick, current)
}

pub fn decode_world_object_sync_wire(
    decoder: &mut WorldObjectSyncDecoder,
    wire: &[u8],
) -> Result<WorldObjectSnapshot, WireError> {
    decoder.apply_wire(wire)
}

pub fn decode_snapshot_meta(
    input: &mut WireReader<'_>,
    decoder: &mut WorldObjectSyncDecoder,
    remaining_after: &mut [usize; META_SEGMENTS],
) -> Result<(SnapshotMeta, Vec<u8>), WireError> {
    let decoded = decode_snapshot_meta_body(input, remaining_after)?;
    let mut meta = decoded.body;
    meta.world_objects = decoder.apply(decoded.world_update);
    Ok((meta, decoded.world_wire))
}

fn apply_pair_delta<K: Copy + Ord, V: Copy>(table: &mut Vec<(K, V)>, delta: &PairDelta<K, V>) {
    if !delta.removed.is_empty() {
        let removed: BTreeSet<_> = delta.removed.iter().copied().collect();
        table.retain(|(key, _)| !removed.contains(key));
    }
    table.sort_by_key(|(id, _)| *id);
    // A full sync may repeat a key; a change lands on its first row.
    let existing = table.len();
    let mut added: BTreeMap<K, usize> = BTreeMap::new();
    for &(id, value) in &delta.changed {
        let index = table[..existing].partition_point(|(key, _)| *key < id);
        if index < existing && table[index].0 == id {
            table[index].1 = value;
            continue;
        }
        match added.entry(id) {
            Entry::Occupied(row) => table[*row.get()].1 = value,
            Entry::Vacant(slot) => {
                slot.insert(table.len());
                table.push((id, value));
            }
        }
    }
    table.sort_by_key(|(id, _)| *id);
}

impl WorldObjectUpdate {
    fn apply(self, state: &mut WorldObjectSnapshot) {
        state.as_of_ms = self.as_of_ms;
        state.map_round_epoch = self.map_round_epoch;
        state.fracture_profile_version = self.fracture_profile_version;
        match self.change {
            WorldObjectChange::Unchanged => {}
            WorldObjectChange::Delta {
                glass,
                loops,
                portal_exits,
            } => {
                apply_pair_delta(&mut state.glass_pieces, &glass);
                state.destructible_loop_sounds = loops;
                state.portal_blocked_exits = portal_exits;
            }
            WorldObjectChange::Full {
                glass,
                loops,
                portal_exits,
            } => {
                state.glass_pieces = glass;
                state.destructible_loop_sounds = loops;
                state.portal_blocked_exits = portal_exits;
            }
        }
    }
}
