use std::collections::HashMap;
use std::hash::Hash;

use playerstate_iw4::AnimPair;
use sim::{
    AreaEntityLinkSnapshot, AreaEntityWorldSnapshot, AreaSectorSnapshot, ClassId,
    ClassRejectReason, ClientAction, ClientId, ClientLifecycle, ClientSnapshotMeta,
    ConfigurationChangeRejectReason, DamageSource, DestructibleLoopSound, DroppedItemAmmo,
    EntityEventPayload, EntityEventRecord, EntityKernelOccupiedSnapshot, EntityKernelSlotSnapshot,
    EntityKernelSnapshot, EntityRef, EntityRelations, EntityRunKind, EventAudience, EventRecord,
    EventSequence, GiveRejectReason, GlassCause, GlassPieceSnapshot, GlassPieceState,
    GlassShatterSeed, ItemPickupRecord, LifeSequence, LoadoutSpec, MatchEndReason, MatchPhase,
    PelletFxRecord, PlayerCorpsePool, PlayerCorpseSlot, RngDebugMeta, ScriptModelId, SimEvent,
    SnapshotMeta, SpawnPick, Tick, WorldObjectSnapshot,
};

use crate::transport::wire::{WireError, WireReader, WireWriter};

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
        let mut out = WireWriter::with_capacity(64);
        let current = canonical_world_object_snapshot(tick, current);
        out.put_i32(current.as_of_ms);
        out.put_u32(current.map_round_epoch);
        out.put_u32(current.fracture_profile_version);
        let full = self.force_full || tick.0 % WORLD_SYNC_PERIOD_TICKS == 0;
        if full {
            out.put_u8(2);
            encode_world_object_full(&mut out, &current);
            self.baseline = current;
            self.force_full = false;
        } else {
            let (destructibles, glass) = world_object_delta(&self.baseline, &current);
            let loops_spoke =
                self.baseline.destructible_loop_sounds != current.destructible_loop_sounds;
            if destructibles.is_empty() && glass.is_empty() && !loops_spoke {
                out.put_u8(0);
                self.baseline.as_of_ms = current.as_of_ms;
                self.baseline.map_round_epoch = current.map_round_epoch;
                self.baseline.fracture_profile_version = current.fracture_profile_version;
            } else {
                out.put_u8(1);
                encode_world_object_delta(&mut out, &self.baseline, &current);
                self.baseline = current;
            }
        }
        out.finish()
    }
}

#[derive(Debug, Default)]
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

    pub fn apply_wire(&mut self, wire: &[u8]) -> Result<WorldObjectSnapshot, WireError> {
        let mut input = WireReader::new(wire);
        decode_world_object_sync(&mut input, &mut self.state)?;
        if !input.is_empty() {
            return Err(WireError::Malformed(
                "trailing bytes after world object sync",
            ));
        }
        Ok(self.state.clone())
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

#[derive(Debug)]
struct PairDelta<K, V> {
    changed: Vec<(K, V)>,
    removed: Vec<K>,
}

impl<K, V> Default for PairDelta<K, V> {
    fn default() -> Self {
        Self {
            changed: Vec::new(),
            removed: Vec::new(),
        }
    }
}

impl<K, V> PairDelta<K, V> {
    fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

fn pair_delta<K: Copy + Eq + Ord + Hash, V: Copy + Eq>(
    baseline: &[(K, V)],
    current: &[(K, V)],
) -> PairDelta<K, V> {
    let base: HashMap<K, V> = baseline.iter().copied().collect();
    let cur: HashMap<K, V> = current.iter().copied().collect();
    let mut changed = Vec::new();
    for (id, value) in current {
        match base.get(id) {
            Some(old) if old == value => {}
            _ => changed.push((*id, *value)),
        }
    }
    let mut removed = Vec::new();
    for (id, _) in baseline {
        if !cur.contains_key(id) {
            removed.push(*id);
        }
    }
    removed.sort_unstable();
    changed.sort_by_key(|(id, _)| *id);
    PairDelta { changed, removed }
}

fn world_object_delta(
    baseline: &WorldObjectSnapshot,
    current: &WorldObjectSnapshot,
) -> (
    PairDelta<ScriptModelId, u8>,
    PairDelta<u32, GlassPieceSnapshot>,
) {
    (
        pair_delta(&baseline.destructible_stages, &current.destructible_stages),
        pair_delta(&baseline.glass_pieces, &current.glass_pieces),
    )
}

fn canonical_world_object_snapshot(
    _tick: Tick,
    current: &WorldObjectSnapshot,
) -> WorldObjectSnapshot {
    current.clone()
}

fn encode_world_object_full(out: &mut WireWriter, snap: &WorldObjectSnapshot) {
    debug_assert!(snap.destructible_stages.len() <= u16::MAX as usize);
    debug_assert!(snap.glass_pieces.len() <= u16::MAX as usize);
    out.put_u16(snap.destructible_stages.len() as u16);
    for (id, stage) in &snap.destructible_stages {
        out.put_u32(id.to_wire());
        out.put_u8(*stage);
    }
    out.put_u16(snap.glass_pieces.len() as u16);
    for (id, row) in &snap.glass_pieces {
        out.put_u32(*id);
        encode_glass_piece_snapshot(out, *row);
    }
    encode_destructible_loop_sounds(out, &snap.destructible_loop_sounds);
}

fn encode_world_object_delta(
    out: &mut WireWriter,
    baseline: &WorldObjectSnapshot,
    current: &WorldObjectSnapshot,
) {
    let (destructibles, glass) = world_object_delta(baseline, current);
    debug_assert!(destructibles.changed.len() <= u16::MAX as usize);
    debug_assert!(destructibles.removed.len() <= u16::MAX as usize);
    debug_assert!(glass.changed.len() <= u16::MAX as usize);
    debug_assert!(glass.removed.len() <= u16::MAX as usize);
    out.put_u16(destructibles.changed.len() as u16);
    for (id, stage) in &destructibles.changed {
        out.put_u32(id.to_wire());
        out.put_u8(*stage);
    }
    out.put_u16(destructibles.removed.len() as u16);
    for id in &destructibles.removed {
        out.put_u32(id.to_wire());
    }
    out.put_u16(glass.changed.len() as u16);
    for (id, row) in &glass.changed {
        out.put_u32(*id);
        encode_glass_piece_snapshot(out, *row);
    }
    out.put_u16(glass.removed.len() as u16);
    for id in &glass.removed {
        out.put_u32(*id);
    }
    encode_destructible_loop_sounds(out, &current.destructible_loop_sounds);
}

/// The loops are a handful of rows that only move when a stage does, so they
/// ride whole rather than as a delta of their own.
fn encode_destructible_loop_sounds(out: &mut WireWriter, rows: &[DestructibleLoopSound]) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for row in rows {
        out.put_u32(row.owner.to_wire());
        out.put_u8(row.alias_index);
        for v in row.origin {
            out.put_f32(v);
        }
    }
}

fn decode_destructible_loop_sounds(
    input: &mut WireReader<'_>,
) -> Result<Vec<DestructibleLoopSound>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let owner = ScriptModelId::from_wire(input.get_u32()?);
        let alias_index = input.get_u8()?;
        let mut origin = [0.0; 3];
        for v in &mut origin {
            *v = input.get_f32()?;
        }
        rows.push(DestructibleLoopSound {
            owner,
            alias_index,
            origin,
        });
    }
    Ok(rows)
}

fn apply_pair_delta<K: Copy + Ord, V: Copy>(table: &mut Vec<(K, V)>, delta: &PairDelta<K, V>) {
    for id in &delta.removed {
        table.retain(|(key, _)| key != id);
    }
    for (id, value) in &delta.changed {
        if let Some(row) = table.iter_mut().find(|(key, _)| key == id) {
            row.1 = *value;
        } else {
            table.push((*id, *value));
        }
    }
    table.sort_by_key(|(id, _)| *id);
}

fn decode_world_object_sync(
    input: &mut WireReader<'_>,
    state: &mut WorldObjectSnapshot,
) -> Result<(), WireError> {
    state.as_of_ms = input.get_i32()?;
    state.map_round_epoch = input.get_u32()?;
    state.fracture_profile_version = input.get_u32()?;
    match input.get_u8()? {
        0 => {}
        1 => {
            let destructible_changed = input.get_u16()? as usize;
            let mut destructibles = PairDelta::<ScriptModelId, u8>::default();
            for _ in 0..destructible_changed {
                destructibles
                    .changed
                    .push((ScriptModelId::from_wire(input.get_u32()?), input.get_u8()?));
            }
            let destructible_removed = input.get_u16()? as usize;
            for _ in 0..destructible_removed {
                destructibles
                    .removed
                    .push(ScriptModelId::from_wire(input.get_u32()?));
            }
            let glass_changed = input.get_u16()? as usize;
            let mut glass = PairDelta::<u32, GlassPieceSnapshot>::default();
            for _ in 0..glass_changed {
                glass
                    .changed
                    .push((input.get_u32()?, decode_glass_piece_snapshot(input)?));
            }
            let glass_removed = input.get_u16()? as usize;
            for _ in 0..glass_removed {
                glass.removed.push(input.get_u32()?);
            }
            let destructible_loop_sounds = decode_destructible_loop_sounds(input)?;
            apply_pair_delta(&mut state.destructible_stages, &destructibles);
            apply_pair_delta(&mut state.glass_pieces, &glass);
            state.destructible_loop_sounds = destructible_loop_sounds;
        }
        2 => {
            let destructible_count = input.get_u16()? as usize;
            let mut destructible_stages = Vec::with_capacity(destructible_count.min(4096));
            for _ in 0..destructible_count {
                destructible_stages
                    .push((ScriptModelId::from_wire(input.get_u32()?), input.get_u8()?));
            }
            let glass_count = input.get_u16()? as usize;
            let mut glass_pieces = Vec::with_capacity(glass_count.min(4096));
            for _ in 0..glass_count {
                glass_pieces.push((input.get_u32()?, decode_glass_piece_snapshot(input)?));
            }
            let destructible_loop_sounds = decode_destructible_loop_sounds(input)?;
            *state = WorldObjectSnapshot {
                as_of_ms: state.as_of_ms,
                map_round_epoch: state.map_round_epoch,
                fracture_profile_version: state.fracture_profile_version,
                destructible_stages,
                glass_pieces,
                destructible_loop_sounds,
            };
        }
        _ => return Err(WireError::Malformed("unknown world object sync tag")),
    }
    Ok(())
}

fn encode_glass_piece_snapshot(out: &mut WireWriter, row: GlassPieceSnapshot) {
    out.put_u8(row.state.as_u8());
    out.put_u32(row.revision);
    out.put_i32(row.last_state_change_time);
    out.put_u8(row.cause.as_u8());
    out.put_u64(row.deterministic_seed);
    match row.shatter_seed {
        None => out.put_u8(0),
        Some(seed) => {
            assert_eq!(
                row.state,
                GlassPieceState::Shattered,
                "glass shatter seed on non-shattered state"
            );
            out.put_u8(1);
            out.put_u8(seed.impact_dir);
            out.put_u8(seed.impact_pos[0]);
            out.put_u8(seed.impact_pos[1]);
        }
    }
}

fn decode_glass_piece_snapshot(
    input: &mut WireReader<'_>,
) -> Result<GlassPieceSnapshot, WireError> {
    let state = GlassPieceState::from_u8(input.get_u8()?)
        .ok_or(WireError::Malformed("unknown glass piece state"))?;
    let revision = input.get_u32()?;
    let last_state_change_time = input.get_i32()?;
    let cause =
        GlassCause::from_u8(input.get_u8()?).ok_or(WireError::Malformed("unknown glass cause"))?;
    let deterministic_seed = input.get_u64()?;
    let shatter_seed = match input.get_u8()? {
        0 => None,
        1 if state == GlassPieceState::Shattered => {
            let impact_dir = input.get_u8()?;
            let impact_pos = [input.get_u8()?, input.get_u8()?];
            Some(
                GlassShatterSeed::new(impact_dir, impact_pos)
                    .ok_or(WireError::Malformed("invalid glass shatter seed"))?,
            )
        }
        1 => return Err(WireError::Malformed("glass seed on non-shattered state")),
        _ => return Err(WireError::Malformed("unknown glass shatter seed tag")),
    };
    Ok(GlassPieceSnapshot {
        state,
        revision,
        last_state_change_time,
        shatter_seed,
        deterministic_seed,
        cause,
    })
}

pub fn encode_actions(out: &mut WireWriter, actions: &[(ClientId, ClientAction)]) {
    debug_assert!(actions.len() <= u16::MAX as usize);
    out.put_u16(actions.len() as u16);
    for (client, action) in actions {
        out.put_u32(client.0);
        encode_action(out, action);
    }
}

pub fn decode_actions(
    input: &mut WireReader<'_>,
) -> Result<Vec<(ClientId, ClientAction)>, WireError> {
    let count = input.get_u16()? as usize;
    let mut out = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let client = ClientId(input.get_u32()?);
        out.push((client, decode_action(input)?));
    }
    Ok(out)
}

pub(crate) fn encode_action(out: &mut WireWriter, action: &ClientAction) {
    match *action {
        ClientAction::JoinMatch { request_id } => {
            out.put_u8(2);
            out.put_u32(request_id);
        }
        ClientAction::LeaveMatch { request_id } => {
            out.put_u8(3);
            out.put_u32(request_id);
        }
        ClientAction::SelectClass {
            request_id,
            class_id,
            revision,
        } => {
            out.put_u8(1);
            out.put_u32(request_id);
            out.put_u32(class_id.0);
            out.put_u32(revision);
        }
        ClientAction::GiveWeapon { request_id, weapon } => {
            out.put_u8(6);
            out.put_u32(request_id);
            out.put_u32(weapon);
        }
        ClientAction::ChangeWeaponConfiguration {
            request_id,
            from,
            to,
        } => {
            out.put_u8(15);
            out.put_u32(request_id);
            out.put_u32(from);
            out.put_u32(to);
        }
        ClientAction::Move {
            request_id,
            origin,
            angles,
        } => {
            out.put_u8(7);
            out.put_u32(request_id);
            out.put_f32(origin[0]);
            out.put_f32(origin[1]);
            out.put_f32(origin[2]);
            out.put_f32(angles[0]);
            out.put_f32(angles[1]);
            out.put_f32(angles[2]);
        }
        ClientAction::BeginScriptMoverRotateVelocity { request_id, speed } => {
            out.put_u8(9);
            out.put_u32(request_id);
            out.put_f32(speed);
        }
        ClientAction::DebugDamage { request_id, amount } => {
            out.put_u8(10);
            out.put_u32(request_id);
            out.put_i32(amount);
        }
        ClientAction::ForceDeath { request_id } => {
            out.put_u8(4);
            out.put_u32(request_id);
        }
        ClientAction::SpawnClient { request_id } => {
            out.put_u8(8);
            out.put_u32(request_id);
        }
        ClientAction::ForceSpawn { request_id, pick } => {
            out.put_u8(14);
            out.put_u32(request_id);
            match pick {
                SpawnPick::Seeded(seed) => {
                    out.put_u8(0);
                    out.put_u32(seed as u32);
                    out.put_u32((seed >> 32) as u32);
                }
                SpawnPick::At { origin, yaw } => {
                    out.put_u8(1);
                    out.put_f32(origin[0]);
                    out.put_f32(origin[1]);
                    out.put_f32(origin[2]);
                    out.put_f32(yaw);
                }
            }
        }
        ClientAction::SpawnIntermission { request_id } => {
            out.put_u8(12);
            out.put_u32(request_id);
        }
        ClientAction::SetMatchPhase { request_id, phase } => {
            out.put_u8(5);
            out.put_u32(request_id);
            out.put_u8(phase_tag(phase));
        }
        ClientAction::SetName { request_id, name } => {
            out.put_u8(11);
            out.put_u32(request_id);
            out.put_bytes(&name);
        }
        ClientAction::UseCopycat { request_id } => {
            out.put_u8(13);
            out.put_u32(request_id);
        }
    }
}

pub(crate) fn decode_action(input: &mut WireReader<'_>) -> Result<ClientAction, WireError> {
    match input.get_u8()? {
        1 => Ok(ClientAction::SelectClass {
            request_id: input.get_u32()?,
            class_id: ClassId(input.get_u32()?),
            revision: input.get_u32()?,
        }),
        2 => Ok(ClientAction::JoinMatch {
            request_id: input.get_u32()?,
        }),
        3 => Ok(ClientAction::LeaveMatch {
            request_id: input.get_u32()?,
        }),
        4 => Ok(ClientAction::ForceDeath {
            request_id: input.get_u32()?,
        }),
        8 => Ok(ClientAction::SpawnClient {
            request_id: input.get_u32()?,
        }),
        12 => Ok(ClientAction::SpawnIntermission {
            request_id: input.get_u32()?,
        }),
        5 => Ok(ClientAction::SetMatchPhase {
            request_id: input.get_u32()?,
            phase: phase_from_tag(input.get_u8()?)?,
        }),
        6 => Ok(ClientAction::GiveWeapon {
            request_id: input.get_u32()?,
            weapon: input.get_u32()?,
        }),
        15 => Ok(ClientAction::ChangeWeaponConfiguration {
            request_id: input.get_u32()?,
            from: input.get_u32()?,
            to: input.get_u32()?,
        }),
        7 => Ok(ClientAction::Move {
            request_id: input.get_u32()?,
            origin: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
            angles: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        }),
        9 => Ok(ClientAction::BeginScriptMoverRotateVelocity {
            request_id: input.get_u32()?,
            speed: input.get_f32()?,
        }),
        10 => Ok(ClientAction::DebugDamage {
            request_id: input.get_u32()?,
            amount: input.get_i32()?,
        }),
        11 => {
            let request_id = input.get_u32()?;
            let mut name = [0u8; 16];
            input.get_bytes(&mut name)?;
            Ok(ClientAction::SetName { request_id, name })
        }
        13 => Ok(ClientAction::UseCopycat {
            request_id: input.get_u32()?,
        }),
        14 => {
            let request_id = input.get_u32()?;
            let pick = match input.get_u8()? {
                0 => {
                    let lo = input.get_u32()? as u64;
                    let hi = input.get_u32()? as u64;
                    SpawnPick::Seeded(lo | hi << 32)
                }
                1 => SpawnPick::At {
                    origin: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    yaw: input.get_f32()?,
                },
                _ => return Err(WireError::Malformed("unknown SpawnPick tag")),
            };
            Ok(ClientAction::ForceSpawn { request_id, pick })
        }
        _ => Err(WireError::Malformed("unknown ClientAction tag")),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotMetaSectionBytes {
    pub match_header: usize,
    pub events: usize,
    pub aliases: usize,
    pub entity_dobjs: usize,
    pub corpses: usize,
    pub entities: usize,
    pub script_movers: usize,
    pub entity_kernel: usize,
    pub item_tables: usize,
    pub area_entities: usize,
    pub map_doors: usize,
    pub objectives: usize,
    pub world_objects: usize,
}

impl SnapshotMetaSectionBytes {
    pub fn total(self) -> usize {
        self.match_header
            + self.events
            + self.aliases
            + self.entity_dobjs
            + self.corpses
            + self.entities
            + self.script_movers
            + self.entity_kernel
            + self.item_tables
            + self.area_entities
            + self.map_doors
            + self.objectives
            + self.world_objects
    }
}

fn section_span(out: &WireWriter, from: usize) -> usize {
    out.len() - from
}

pub fn encode_snapshot_meta(out: &mut WireWriter, meta: &SnapshotMeta, world_objects_wire: &[u8]) {
    let _ = encode_snapshot_meta_sections(out, meta, world_objects_wire);
}

pub fn encode_snapshot_meta_sections(
    out: &mut WireWriter,
    meta: &SnapshotMeta,
    world_objects_wire: &[u8],
) -> SnapshotMetaSectionBytes {
    let mut sizes = SnapshotMetaSectionBytes::default();
    let mut mark = out.len();
    out.put_u8(phase_tag(meta.phase));
    out.put_u32(meta.match_elapsed_ms);
    let (prematch_tag, elapsed_ms) = match meta.prematch {
        gamemode_iw4::PrematchStep::Waiting { elapsed_ms } => (0, elapsed_ms),
        gamemode_iw4::PrematchStep::Starting { elapsed_ms } => (1, elapsed_ms),
        gamemode_iw4::PrematchStep::Done => (2, 0),
    };
    out.put_u8(prematch_tag);
    out.put_u32(elapsed_ms);
    out.put_i32(meta.score_limit);
    out.put_u32(meta.time_limit_ms);
    out.put_u8(meta.kind.wire_tag());
    debug_assert!(meta.clients.len() <= u16::MAX as usize);
    out.put_u16(meta.clients.len() as u16);
    for (client, row) in &meta.clients {
        out.put_u32(client.0);
        encode_client_meta(out, row);
    }
    sizes.match_header = section_span(out, mark);
    mark = out.len();
    debug_assert!(meta.journal.len() <= u16::MAX as usize);
    out.put_u16(meta.journal.len() as u16);
    for record in &meta.journal {
        encode_event_record(out, record);
    }
    debug_assert!(meta.entity_events.len() <= u16::MAX as usize);
    out.put_u16(meta.entity_events.len() as u16);
    for record in &meta.entity_events {
        encode_entity_event_record(out, record);
    }
    debug_assert!(meta.pellet_fx.len() <= u16::MAX as usize);
    out.put_u16(meta.pellet_fx.len() as u16);
    for record in &meta.pellet_fx {
        encode_pellet_fx_record(out, record);
    }
    sizes.events = section_span(out, mark);
    mark = out.len();
    encode_sound_alias_cs(out, &meta.sound_aliases);
    encode_sound_alias_cs(out, &meta.effect_names);
    encode_sound_alias_cs(out, &meta.hud_materials);
    encode_rng_debug(out, &meta.rng);
    sizes.aliases = section_span(out, mark);
    mark = out.len();
    encode_entity_dobjs(out, &meta.entity_dobjs);
    sizes.entity_dobjs = section_span(out, mark);
    mark = out.len();
    encode_corpse_pool(out, &meta.corpses);
    sizes.corpses = section_span(out, mark);
    mark = out.len();
    encode_entity_states(out, &meta.entities);
    sizes.entities = section_span(out, mark);
    mark = out.len();
    encode_script_movers(out, &meta.script_movers);
    sizes.script_movers = section_span(out, mark);
    mark = out.len();
    encode_entity_kernel(out, &meta.entity_kernel);
    sizes.entity_kernel = section_span(out, mark);
    mark = out.len();
    encode_item_ammo(out, &meta.item_ammo);
    encode_item_pickups(out, &meta.item_pickups);
    sizes.item_tables = section_span(out, mark);
    mark = out.len();
    encode_area_entities(out, meta.area_entities.as_ref());
    sizes.area_entities = section_span(out, mark);
    mark = out.len();
    encode_map_doors(out, meta.map_doors.as_ref());
    sizes.map_doors = section_span(out, mark);
    mark = out.len();
    encode_objectives(out, &meta.objectives);
    sizes.objectives = section_span(out, mark);
    mark = out.len();
    debug_assert!(world_objects_wire.len() <= u16::MAX as usize);
    out.put_u16(world_objects_wire.len() as u16);
    out.put_bytes(world_objects_wire);
    sizes.world_objects = section_span(out, mark);
    sizes
}

pub fn decode_snapshot_meta(
    input: &mut WireReader<'_>,
    world_decoder: &mut WorldObjectSyncDecoder,
) -> Result<(SnapshotMeta, Vec<u8>), WireError> {
    let phase = phase_from_tag(input.get_u8()?)?;
    let match_elapsed_ms = input.get_u32()?;
    let prematch_tag = input.get_u8()?;
    let elapsed_ms = input.get_u32()?;
    let prematch = match prematch_tag {
        0 => gamemode_iw4::PrematchStep::Waiting { elapsed_ms },
        1 => gamemode_iw4::PrematchStep::Starting { elapsed_ms },
        2 => gamemode_iw4::PrematchStep::Done,
        _ => return Err(WireError::Malformed("unknown prematch tag")),
    };
    let score_limit = input.get_i32()?;
    let time_limit_ms = input.get_u32()?;
    let kind = gamemode_iw4::GameModeKind::from_wire_tag(input.get_u8()?)
        .ok_or(WireError::Malformed("unknown GameModeKind tag"))?;
    let count = input.get_u16()? as usize;
    let mut clients = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let client = ClientId(input.get_u32()?);
        clients.push((client, decode_client_meta(input)?));
    }
    let journal_count = input.get_u16()? as usize;
    let mut journal = Vec::with_capacity(journal_count.min(64));
    for _ in 0..journal_count {
        journal.push(decode_event_record(input)?);
    }
    let entity_event_count = input.get_u16()? as usize;
    let mut entity_events = Vec::with_capacity(entity_event_count.min(64));
    for _ in 0..entity_event_count {
        entity_events.push(decode_entity_event_record(input)?);
    }
    let pellet_fx_count = input.get_u16()? as usize;
    let mut pellet_fx = Vec::with_capacity(pellet_fx_count.min(256));
    for _ in 0..pellet_fx_count {
        pellet_fx.push(decode_pellet_fx_record(input)?);
    }
    let sound_aliases = decode_sound_alias_cs(input)?;
    let effect_names = decode_sound_alias_cs(input)?;
    let hud_materials = decode_sound_alias_cs(input)?;
    let rng = decode_rng_debug(input)?;
    let entity_dobjs = decode_entity_dobjs(input)?;
    let corpses = decode_corpse_pool(input)?;
    let entities = decode_entity_states(input)?;
    let script_movers = decode_script_movers(input)?;
    let entity_kernel = decode_entity_kernel(input)?;
    let item_ammo = decode_item_ammo(input)?;
    let item_pickups = decode_item_pickups(input)?;
    let area_entities = decode_area_entities(input)?;
    let map_doors = decode_map_doors(input)?;
    let objectives = decode_objectives(input)?;
    let wire_len = input.get_u16()? as usize;
    let mut wire = vec![0u8; wire_len];
    input.get_bytes(&mut wire)?;
    let world_objects = world_decoder.apply_wire(&wire)?;
    Ok((
        SnapshotMeta {
            phase,
            match_elapsed_ms,
            prematch,
            score_limit,
            time_limit_ms,
            kind,
            clients,
            journal,
            entity_events,
            pellet_fx,
            sound_aliases,
            effect_names,
            hud_materials,
            rng,
            world_objects,
            area_entities,
            map_doors,
            objectives,
            entity_dobjs,
            entities,
            script_movers,
            entity_kernel,
            corpses,
            item_ammo,
            item_pickups,
        },
        wire,
    ))
}

fn encode_area_entities(out: &mut WireWriter, snapshot: Option<&AreaEntityWorldSnapshot>) {
    let Some(snapshot) = snapshot else {
        out.put_u8(0);
        return;
    };
    out.put_u8(1);
    for value in snapshot.world_mid {
        out.put_f32(value);
    }
    for value in snapshot.world_half {
        out.put_f32(value);
    }
    debug_assert!(snapshot.free_prefix.len() <= u16::MAX as usize);
    out.put_u16(snapshot.free_prefix.len() as u16);
    for &index in &snapshot.free_prefix {
        out.put_u16(index);
    }
    out.put_u16(snapshot.contiguous_free_head);
    debug_assert!(snapshot.sectors.len() <= u16::MAX as usize);
    out.put_u16(snapshot.sectors.len() as u16);
    for row in &snapshot.sectors {
        out.put_u16(row.index);
        out.put_u32(row.contents_entities);
        out.put_u32(row.linkcontents_entities);
        out.put_u16(row.entities);
        out.put_f32(row.dist);
        out.put_u16(row.axis);
        out.put_u16(row.parent);
        out.put_u16(row.child[0]);
        out.put_u16(row.child[1]);
    }
    debug_assert!(snapshot.entities.len() <= u16::MAX as usize);
    out.put_u16(snapshot.entities.len() as u16);
    for row in &snapshot.entities {
        out.put_u16(row.entity_num);
        out.put_u16(row.world_sector);
        out.put_u16(row.next_entity);
        out.put_u32(row.linkcontents);
        for value in row.linkmin {
            out.put_f32(value);
        }
        for value in row.linkmax {
            out.put_f32(value);
        }
        out.put_u32(row.contents);
        for value in row.bounds_mid {
            out.put_f32(value);
        }
        for value in row.bounds_half {
            out.put_f32(value);
        }
    }
}

fn decode_area_entities(
    input: &mut WireReader<'_>,
) -> Result<Option<AreaEntityWorldSnapshot>, WireError> {
    match input.get_u8()? {
        0 => return Ok(None),
        1 => {}
        _ => return Err(WireError::Malformed("unknown CM area-sector snapshot tag")),
    }
    let world_mid = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    let world_half = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    let free_count = input.get_u16()? as usize;
    if free_count > 1022 {
        return Err(WireError::Malformed(
            "CM area free prefix exceeds sector pool",
        ));
    }
    let mut free_prefix = Vec::with_capacity(free_count);
    for _ in 0..free_count {
        free_prefix.push(input.get_u16()?);
    }
    let contiguous_free_head = input.get_u16()?;
    let sector_count = input.get_u16()? as usize;
    if sector_count > 1023 {
        return Err(WireError::Malformed("CM area live rows exceed sector pool"));
    }
    let mut sectors = Vec::with_capacity(sector_count);
    for _ in 0..sector_count {
        sectors.push(AreaSectorSnapshot {
            index: input.get_u16()?,
            contents_entities: input.get_u32()?,
            linkcontents_entities: input.get_u32()?,
            entities: input.get_u16()?,
            dist: input.get_f32()?,
            axis: input.get_u16()?,
            parent: input.get_u16()?,
            child: [input.get_u16()?, input.get_u16()?],
        });
    }
    let entity_count = input.get_u16()? as usize;
    if entity_count > 1024 {
        return Err(WireError::Malformed("CM area links exceed entity pool"));
    }
    let mut entities = Vec::with_capacity(entity_count);
    for _ in 0..entity_count {
        entities.push(AreaEntityLinkSnapshot {
            entity_num: input.get_u16()?,
            world_sector: input.get_u16()?,
            next_entity: input.get_u16()?,
            linkcontents: input.get_u32()?,
            linkmin: [input.get_f32()?, input.get_f32()?],
            linkmax: [input.get_f32()?, input.get_f32()?],
            contents: input.get_u32()?,
            bounds_mid: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
            bounds_half: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        });
    }
    let snapshot = AreaEntityWorldSnapshot {
        world_mid,
        world_half,
        free_prefix,
        contiguous_free_head,
        sectors,
        entities,
    };
    snapshot
        .validate()
        .map_err(|_| WireError::Malformed("invalid CM area-sector snapshot"))?;
    Ok(Some(snapshot))
}

fn encode_sound_alias_cs(out: &mut WireWriter, occupied: &[(u8, String)]) {
    debug_assert!(occupied.len() <= u8::MAX as usize);
    out.put_u8(occupied.len() as u8);
    for (index, name) in occupied {
        out.put_u8(*index);
        let bytes = name.as_bytes();
        debug_assert!(bytes.len() <= u8::MAX as usize);
        out.put_u8(bytes.len() as u8);
        out.put_bytes(bytes);
    }
}

fn decode_sound_alias_cs(input: &mut WireReader<'_>) -> Result<Vec<(u8, String)>, WireError> {
    let count = input.get_u8()? as usize;
    let mut occupied = Vec::with_capacity(count);
    for _ in 0..count {
        let index = input.get_u8()?;
        let len = input.get_u8()? as usize;
        let mut bytes = vec![0u8; len];
        input.get_bytes(&mut bytes)?;
        let name = String::from_utf8(bytes)
            .map_err(|_| WireError::Malformed("sound alias CS name is not utf-8"))?;
        occupied.push((index, name));
    }
    Ok(occupied)
}

fn encode_entity_event_record(out: &mut WireWriter, record: &EntityEventRecord) {
    out.put_u32(record.sequence.0);
    out.put_u32(record.tick.0);
    encode_audience(out, &record.audience);
    out.put_i32(record.event.0);
    let payload = record.payload;
    out.put_i32(payload.number);
    out.put_i32(payload.other_entity_num);
    out.put_i32(payload.attacker_entity_num);
    out.put_i32(payload.event_parm);
    out.put_u32(payload.weapon);
    out.put_u32(payload.correlation);
    out.put_u16(payload.pellet);
    out.put_u8(payload.hand);
    for value in payload.origin {
        out.put_f32(value);
    }
    for value in payload.origin2 {
        out.put_f32(value);
    }
    for value in payload.direction {
        out.put_f32(value);
    }
    out.put_u8(payload.surf_type);
    out.put_u32(payload.surface_flags);
    out.put_u8(payload.simulation_flags);
}

fn decode_entity_event_record(input: &mut WireReader<'_>) -> Result<EntityEventRecord, WireError> {
    let sequence = EventSequence(input.get_u32()?);
    let tick = Tick(input.get_u32()?);
    let audience = decode_audience(input)?;
    let event = entity_iw4::EntityEventKind(input.get_i32()?);
    if event == entity_iw4::EntityEventKind::NONE
        || event.0 > entity_iw4::EntityEventKind::MANTLE.0
        || event.0 < 0
    {
        return Err(WireError::Malformed("entity event outside retail table"));
    }
    Ok(EntityEventRecord {
        sequence,
        tick,
        audience,
        event,
        payload: EntityEventPayload {
            number: input.get_i32()?,
            other_entity_num: input.get_i32()?,
            attacker_entity_num: input.get_i32()?,
            event_parm: input.get_i32()?,
            weapon: input.get_u32()?,
            correlation: input.get_u32()?,
            pellet: input.get_u16()?,
            hand: input.get_u8()?,
            origin: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
            origin2: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
            direction: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
            surf_type: input.get_u8()?,
            surface_flags: input.get_u32()?,
            simulation_flags: input.get_u8()?,
        },
    })
}

fn encode_pellet_fx_record(out: &mut WireWriter, record: &PelletFxRecord) {
    out.put_i32(record.attacker);
    out.put_u32(record.weapon);
    out.put_u32(record.correlation);
    out.put_u16(record.pellet);
    out.put_u8(record.hand);
    for value in record.start {
        out.put_f32(value);
    }
    for value in record.end {
        out.put_f32(value);
    }
    for value in record.normal {
        out.put_f32(value);
    }
    out.put_u8(record.surf_type);
    out.put_u32(record.surface_flags);
    out.put_u8(record.flesh_flags);
}

fn decode_pellet_fx_record(input: &mut WireReader<'_>) -> Result<PelletFxRecord, WireError> {
    Ok(PelletFxRecord {
        attacker: input.get_i32()?,
        weapon: input.get_u32()?,
        correlation: input.get_u32()?,
        pellet: input.get_u16()?,
        hand: input.get_u8()?,
        start: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        end: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        normal: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        surf_type: input.get_u8()?,
        surface_flags: input.get_u32()?,
        flesh_flags: input.get_u8()?,
    })
}

fn encode_rng_debug(out: &mut WireWriter, rng: &RngDebugMeta) {
    out.put_u64(rng.root_seed);
    out.put_u32(rng.scheme);
    out.put_u64(rng.spawn_draws);
    out.put_u64(rng.combat_draws);
    out.put_u64(rng.bot_draws);
}

fn decode_rng_debug(input: &mut WireReader<'_>) -> Result<RngDebugMeta, WireError> {
    Ok(RngDebugMeta {
        root_seed: input.get_u64()?,
        scheme: input.get_u32()?,
        spawn_draws: input.get_u64()?,
        combat_draws: input.get_u64()?,
        bot_draws: input.get_u64()?,
    })
}

fn encode_event_record(out: &mut WireWriter, record: &EventRecord) {
    out.put_u32(record.sequence.0);
    out.put_u32(record.tick.0);
    encode_audience(out, &record.audience);
    encode_event(out, &record.event);
}

fn decode_event_record(input: &mut WireReader<'_>) -> Result<EventRecord, WireError> {
    Ok(EventRecord {
        sequence: EventSequence(input.get_u32()?),
        tick: Tick(input.get_u32()?),
        audience: decode_audience(input)?,
        event: decode_event(input)?,
    })
}

fn encode_audience(out: &mut WireWriter, audience: &EventAudience) {
    match audience {
        EventAudience::All => out.put_u8(0),
        EventAudience::AllExcept(id) => {
            out.put_u8(3);
            out.put_u32(id.0);
        }
        EventAudience::Client(id) => {
            out.put_u8(1);
            out.put_u32(id.0);
        }
        EventAudience::Clients(ids) => {
            out.put_u8(2);
            debug_assert!(ids.len() <= u8::MAX as usize);
            out.put_u8(ids.len() as u8);
            for id in ids {
                out.put_u32(id.0);
            }
        }
    }
}

fn decode_audience(input: &mut WireReader<'_>) -> Result<EventAudience, WireError> {
    match input.get_u8()? {
        0 => Ok(EventAudience::All),
        3 => Ok(EventAudience::AllExcept(ClientId(input.get_u32()?))),
        1 => Ok(EventAudience::Client(ClientId(input.get_u32()?))),
        2 => {
            let n = input.get_u8()? as usize;
            let mut ids = Vec::with_capacity(n.min(16));
            for _ in 0..n {
                ids.push(ClientId(input.get_u32()?));
            }
            Ok(EventAudience::Clients(ids))
        }
        _ => Err(WireError::Malformed("unknown EventAudience tag")),
    }
}

fn encode_client_meta(out: &mut WireWriter, meta: &ClientSnapshotMeta) {
    match meta.killcam_hud {
        None => out.put_u8(0),
        Some(hud) => {
            out.put_u8(if hud.final_kill { 2 } else { 1 });
            out.put_i32(hud.time_until_respawn_ms);
            out.put_i32(hud.kc_timer_ms);
        }
    }
    out.put_u8(lifecycle_tag(meta.lifecycle));
    out.put_u32(meta.life_sequence.0);
    out.put_i32(meta.item_use_spawn_ms);
    encode_optional_entity_ref(out, meta.item_use_entity);
    out.put_i32(meta.ammo_clip);
    out.put_i32(meta.ammo_stock);
    out.put_i32(meta.score);
    out.put_i32(meta.kills);
    out.put_i32(meta.deaths);
    match &meta.loadout {
        None => out.put_u8(0),
        Some(loadout) => {
            out.put_u8(1);
            encode_loadout(out, loadout);
        }
    }

    debug_assert!(meta.ammo_by_weapon.len() <= u16::MAX as usize);
    out.put_u16(meta.ammo_by_weapon.len() as u16);
    for (weapon, clip, stock) in &meta.ammo_by_weapon {
        out.put_u32(*weapon);
        out.put_i32(*clip);
        out.put_i32(*stock);
    }
    debug_assert!(meta.taped_mag_spent.len() <= u8::MAX as usize);
    out.put_u8(meta.taped_mag_spent.len() as u8);
    for weapon in &meta.taped_mag_spent {
        out.put_u32(*weapon);
    }
    out.put_u8(meta.weapon_shot_count);
    out.put_u8(u8::from(meta.burst_latch));
    out.put_u8(u8::from(meta.rechamber_pending));
    match meta.dead_since_tick {
        None => out.put_u8(0),
        Some(tick) => {
            out.put_u8(1);
            out.put_u32(tick);
        }
    }
    out.put_i32(meta.look_at_killer_yaw);
    out.put_bytes(&meta.name);

    encode_hud_bank(out, &meta.hud_archival);
    encode_hud_bank(out, &meta.hud_current);
    match meta.ffa_team {
        None => out.put_u8(0),
        Some(team) => {
            out.put_u8(1);
            out.put_u8(team);
        }
    }
    out.put_i32(meta.client_state_team);
    out.put_i32(meta.rank);
    out.put_i32(meta.prestige);
    out.put_u32(meta.player_card_icon);
    out.put_u32(meta.player_card_title);
    out.put_u32(meta.player_card_nameplate);
}

fn decode_client_meta(input: &mut WireReader<'_>) -> Result<ClientSnapshotMeta, WireError> {
    let killcam_hud = match input.get_u8()? {
        0 => None,
        tag @ (1 | 2) => Some(sim::KillcamHud {
            final_kill: tag == 2,
            time_until_respawn_ms: input.get_i32()?,
            kc_timer_ms: input.get_i32()?,
        }),
        _ => return Err(WireError::Malformed("bad killcam HUD tag")),
    };
    let lifecycle = lifecycle_from_tag(input.get_u8()?)?;
    let life_sequence = LifeSequence(input.get_u32()?);
    let item_use_spawn_ms = input.get_i32()?;
    let item_use_entity = decode_optional_entity_ref(input)?;
    let ammo_clip = input.get_i32()?;
    let ammo_stock = input.get_i32()?;
    let score = input.get_i32()?;
    let kills = input.get_i32()?;
    let deaths = input.get_i32()?;
    let loadout = match input.get_u8()? {
        0 => None,
        1 => Some(decode_loadout(input)?),
        _ => return Err(WireError::Malformed("bad loadout tag")),
    };
    let ammo_rows = input.get_u16()? as usize;
    let mut ammo_by_weapon = Vec::with_capacity(ammo_rows.min(32));
    for _ in 0..ammo_rows {
        let weapon = input.get_u32()?;
        let clip = input.get_i32()?;
        let stock = input.get_i32()?;
        ammo_by_weapon.push((weapon, clip, stock));
    }
    let spent_rows = input.get_u8()? as usize;
    let mut taped_mag_spent = Vec::with_capacity(spent_rows);
    for _ in 0..spent_rows {
        taped_mag_spent.push(input.get_u32()?);
    }
    let weapon_shot_count = input.get_u8()?;
    let burst_latch = input.get_u8()? != 0;
    let rechamber_pending = input.get_u8()? != 0;
    let dead_since_tick = match input.get_u8()? {
        0 => None,
        1 => Some(input.get_u32()?),
        _ => return Err(WireError::Malformed("bad dead_since_tick tag")),
    };
    let look_at_killer_yaw = input.get_i32()?;
    let mut name = [0u8; 16];
    input.get_bytes(&mut name)?;
    let hud_archival = decode_hud_bank(input)?;
    let hud_current = decode_hud_bank(input)?;
    let ffa_team = match input.get_u8()? {
        0 => None,
        1 => Some(input.get_u8()?),
        _ => return Err(WireError::Malformed("bad ffa_team tag")),
    };
    let client_state_team = input.get_i32()?;
    let rank = input.get_i32()?;
    let prestige = input.get_i32()?;
    let player_card_icon = input.get_u32()?;
    let player_card_title = input.get_u32()?;
    let player_card_nameplate = input.get_u32()?;
    Ok(ClientSnapshotMeta {
        killcam_hud,
        lifecycle,
        loadout,
        life_sequence,
        item_use_spawn_ms,
        item_use_entity,
        ammo_clip,
        ammo_stock,
        score,
        kills,
        deaths,
        ammo_by_weapon,
        taped_mag_spent,
        weapon_shot_count,
        burst_latch,
        rechamber_pending,
        dead_since_tick,
        look_at_killer_yaw,
        name,
        hud_archival,
        hud_current,
        ffa_team,
        client_state_team,
        rank,
        prestige,
        player_card_icon,
        player_card_title,
        player_card_nameplate,
    })
}

fn encode_hud_bank(out: &mut WireWriter, bank: &[hud_iw4::HudElem]) {
    let n = bank.len().min(hud_iw4::HUDELEM_BANK_CAPACITY);
    out.put_u8(n as u8);
    for elem in bank.iter().take(n) {
        encode_hud_elem(out, elem);
    }
}

fn decode_hud_bank(input: &mut WireReader<'_>) -> Result<Vec<hud_iw4::HudElem>, WireError> {
    let n = input.get_u8()? as usize;
    if n > hud_iw4::HUDELEM_BANK_CAPACITY {
        return Err(WireError::Malformed("hud bank longer than 31"));
    }
    let mut bank = Vec::with_capacity(n);
    for _ in 0..n {
        bank.push(decode_hud_elem(input)?);
    }
    Ok(bank)
}

fn encode_hud_elem(out: &mut WireWriter, e: &hud_iw4::HudElem) {
    out.put_i32(e.elem_type);
    out.put_f32(e.y);
    out.put_f32(e.x);
    out.put_f32(e.z);
    out.put_i32(e.target_ent_num);
    out.put_f32(e.font_scale);
    out.put_f32(e.from_font_scale);
    out.put_i32(e.font_scale_start_time);
    out.put_i32(e.font_scale_time);
    out.put_i32(e.label);
    out.put_i32(e.font);
    out.put_i32(e.align_org);
    out.put_i32(e.align_screen);
    out.put_u32(e.color_rgba);
    out.put_u32(e.from_color_rgba);
    out.put_i32(e.fade_start_time);
    out.put_i32(e.fade_time);
    out.put_i32(e.height);
    out.put_i32(e.width);
    out.put_i32(e.material_index);
    out.put_f32(e.from_y);
    out.put_f32(e.from_x);
    out.put_i32(e.from_align_org);
    out.put_i32(e.from_align_screen);
    out.put_i32(e.move_start_time);
    out.put_i32(e.move_time);
    out.put_i32(e.from_height);
    out.put_i32(e.from_width);
    out.put_i32(e.scale_start_time);
    out.put_i32(e.scale_time);
    out.put_f32(e.value);
    out.put_i32(e.time);
    out.put_i32(e.duration);
    out.put_i32(e.text);
    out.put_f32(e.sort);
    out.put_u32(e.glow_color_rgba);
    out.put_i32(e.fx_birth_time);
    out.put_i32(e.fx_letter_time);
    out.put_i32(e.fx_decay_start_time);
    out.put_i32(e.fx_decay_duration);
    out.put_i32(e.sound_id);
    out.put_i32(e.flags);
}

fn decode_hud_elem(input: &mut WireReader<'_>) -> Result<hud_iw4::HudElem, WireError> {
    Ok(hud_iw4::HudElem {
        elem_type: input.get_i32()?,
        y: input.get_f32()?,
        x: input.get_f32()?,
        z: input.get_f32()?,
        target_ent_num: input.get_i32()?,
        font_scale: input.get_f32()?,
        from_font_scale: input.get_f32()?,
        font_scale_start_time: input.get_i32()?,
        font_scale_time: input.get_i32()?,
        label: input.get_i32()?,
        font: input.get_i32()?,
        align_org: input.get_i32()?,
        align_screen: input.get_i32()?,
        color_rgba: input.get_u32()?,
        from_color_rgba: input.get_u32()?,
        fade_start_time: input.get_i32()?,
        fade_time: input.get_i32()?,
        height: input.get_i32()?,
        width: input.get_i32()?,
        material_index: input.get_i32()?,
        from_y: input.get_f32()?,
        from_x: input.get_f32()?,
        from_align_org: input.get_i32()?,
        from_align_screen: input.get_i32()?,
        move_start_time: input.get_i32()?,
        move_time: input.get_i32()?,
        from_height: input.get_i32()?,
        from_width: input.get_i32()?,
        scale_start_time: input.get_i32()?,
        scale_time: input.get_i32()?,
        value: input.get_f32()?,
        time: input.get_i32()?,
        duration: input.get_i32()?,
        text: input.get_i32()?,
        sort: input.get_f32()?,
        glow_color_rgba: input.get_u32()?,
        fx_birth_time: input.get_i32()?,
        fx_letter_time: input.get_i32()?,
        fx_decay_start_time: input.get_i32()?,
        fx_decay_duration: input.get_i32()?,
        sound_id: input.get_i32()?,
        flags: input.get_i32()?,
    })
}

fn encode_loadout(out: &mut WireWriter, loadout: &LoadoutSpec) {
    out.put_u32(loadout.class_id.0);
    out.put_u32(loadout.revision);
    out.put_u32(loadout.primary);
    out.put_u32(loadout.secondary);
    for a in loadout.primary_attachments {
        out.put_u32(a);
    }
    for a in loadout.secondary_attachments {
        out.put_u32(a);
    }
    out.put_u32(loadout.lethal);
    out.put_u32(loadout.tactical);
    for p in loadout.perks {
        out.put_u32(p);
    }
}

fn decode_loadout(input: &mut WireReader<'_>) -> Result<LoadoutSpec, WireError> {
    let class_id = ClassId(input.get_u32()?);
    let revision = input.get_u32()?;
    let primary = input.get_u32()?;
    let secondary = input.get_u32()?;
    let mut primary_attachments = [0u32; 4];
    for slot in &mut primary_attachments {
        *slot = input.get_u32()?;
    }
    let mut secondary_attachments = [0u32; 4];
    for slot in &mut secondary_attachments {
        *slot = input.get_u32()?;
    }
    let lethal = input.get_u32()?;
    let tactical = input.get_u32()?;
    let mut perks = [0u32; 3];
    for slot in &mut perks {
        *slot = input.get_u32()?;
    }
    Ok(LoadoutSpec {
        class_id,
        revision,
        primary,
        secondary,
        primary_attachments,
        secondary_attachments,
        lethal,
        tactical,
        perks,
    })
}

pub(crate) fn encode_event(out: &mut WireWriter, event: &SimEvent) {
    match *event {
        SimEvent::ClassAccepted {
            request_id,
            class_id,
            revision,
        } => {
            out.put_u8(1);
            out.put_u32(request_id);
            out.put_u32(class_id.0);
            out.put_u32(revision);
        }
        SimEvent::ClassRejected {
            request_id,
            class_id,
            revision,
            reason,
        } => {
            out.put_u8(2);
            out.put_u32(request_id);
            out.put_u32(class_id.0);
            out.put_u32(revision);
            out.put_u8(reject_reason_tag(reason));
        }
        SimEvent::Spawned {
            class_id,
            spawn_index,
            life_sequence,
        } => {
            out.put_u8(3);
            out.put_u32(class_id.0);
            out.put_u32(spawn_index);
            out.put_u32(life_sequence.0);
        }
        SimEvent::AttackReleased => out.put_u8(5),
        SimEvent::Died {
            victim,
            life_sequence,
            attacker,
            attacker_life,
            source,
            weapon,
            killcam_entity_start_time,
        } => {
            out.put_u8(6);
            out.put_u32(victim.0);
            out.put_u32(life_sequence.0);
            match attacker {
                None => out.put_u8(0),
                Some(id) => {
                    out.put_u8(1);
                    out.put_u32(id.0);
                }
            }
            match attacker_life {
                None => out.put_u8(0),
                Some(life) => {
                    out.put_u8(1);
                    out.put_u32(life.0);
                }
            }
            match source {
                None => out.put_u8(0),
                Some(source) => {
                    out.put_u8(1);
                    encode_damage_source(out, source);
                }
            }
            out.put_u32(weapon);
            out.put_i32(killcam_entity_start_time);
        }
        SimEvent::ScoreChanged {
            client,
            score,
            kills,
            deaths,
        } => {
            out.put_u8(11);
            out.put_u32(client.0);
            out.put_i32(score);
            out.put_i32(kills);
            out.put_i32(deaths);
        }
        SimEvent::MatchEnded { reason } => {
            out.put_u8(12);
            out.put_u8(match reason {
                MatchEndReason::ScoreLimit => 0,
                MatchEndReason::TimeLimit => 1,
            });
        }
        SimEvent::GiveAccepted { request_id, weapon } => {
            out.put_u8(14);
            out.put_u32(request_id);
            out.put_u32(weapon);
        }
        SimEvent::GiveRejected {
            request_id,
            weapon,
            reason,
        } => {
            out.put_u8(15);
            out.put_u32(request_id);
            out.put_u32(weapon);
            out.put_u8(give_reject_reason_tag(reason));
        }
        SimEvent::ConfigurationChangeAccepted {
            request_id,
            from,
            to,
        } => {
            out.put_u8(16);
            out.put_u32(request_id);
            out.put_u32(from);
            out.put_u32(to);
        }
        SimEvent::ConfigurationChangeRejected {
            request_id,
            from,
            to,
            reason,
        } => {
            out.put_u8(17);
            out.put_u32(request_id);
            out.put_u32(from);
            out.put_u32(to);
            out.put_u8(configuration_change_reject_reason_tag(reason));
        }
    }
}

pub(crate) fn decode_event(input: &mut WireReader<'_>) -> Result<SimEvent, WireError> {
    match input.get_u8()? {
        1 => Ok(SimEvent::ClassAccepted {
            request_id: input.get_u32()?,
            class_id: ClassId(input.get_u32()?),
            revision: input.get_u32()?,
        }),
        2 => Ok(SimEvent::ClassRejected {
            request_id: input.get_u32()?,
            class_id: ClassId(input.get_u32()?),
            revision: input.get_u32()?,
            reason: reject_reason_from_tag(input.get_u8()?)?,
        }),
        3 => Ok(SimEvent::Spawned {
            class_id: ClassId(input.get_u32()?),
            spawn_index: input.get_u32()?,
            life_sequence: LifeSequence(input.get_u32()?),
        }),
        5 => Ok(SimEvent::AttackReleased),
        6 => {
            let victim = ClientId(input.get_u32()?);
            let life_sequence = LifeSequence(input.get_u32()?);
            let attacker = match input.get_u8()? {
                0 => None,
                1 => Some(ClientId(input.get_u32()?)),
                _ => return Err(WireError::Malformed("bad Died attacker tag")),
            };
            let attacker_life = match input.get_u8()? {
                0 => None,
                1 => Some(LifeSequence(input.get_u32()?)),
                _ => return Err(WireError::Malformed("bad Died attacker_life tag")),
            };
            let source = match input.get_u8()? {
                0 => None,
                1 => Some(decode_damage_source(input)?),
                _ => return Err(WireError::Malformed("bad Died source tag")),
            };
            let weapon = input.get_u32()?;
            let killcam_entity_start_time = input.get_i32()?;
            Ok(SimEvent::Died {
                victim,
                life_sequence,
                attacker,
                attacker_life,
                source,
                weapon,
                killcam_entity_start_time,
            })
        }
        11 => Ok(SimEvent::ScoreChanged {
            client: ClientId(input.get_u32()?),
            score: input.get_i32()?,
            kills: input.get_i32()?,
            deaths: input.get_i32()?,
        }),
        12 => Ok(SimEvent::MatchEnded {
            reason: match input.get_u8()? {
                0 => MatchEndReason::ScoreLimit,
                1 => MatchEndReason::TimeLimit,
                _ => return Err(WireError::Malformed("unknown MatchEndReason tag")),
            },
        }),
        14 => Ok(SimEvent::GiveAccepted {
            request_id: input.get_u32()?,
            weapon: input.get_u32()?,
        }),
        15 => Ok(SimEvent::GiveRejected {
            request_id: input.get_u32()?,
            weapon: input.get_u32()?,
            reason: give_reject_reason_from_tag(input.get_u8()?)?,
        }),
        16 => Ok(SimEvent::ConfigurationChangeAccepted {
            request_id: input.get_u32()?,
            from: input.get_u32()?,
            to: input.get_u32()?,
        }),
        17 => Ok(SimEvent::ConfigurationChangeRejected {
            request_id: input.get_u32()?,
            from: input.get_u32()?,
            to: input.get_u32()?,
            reason: configuration_change_reject_reason_from_tag(input.get_u8()?)?,
        }),
        _ => Err(WireError::Malformed("unknown SimEvent tag")),
    }
}

fn encode_damage_source(out: &mut WireWriter, source: DamageSource) {
    match source {
        DamageSource::Shot(id) => {
            out.put_u8(1);
            out.put_u32(id.0);
        }
        DamageSource::Projectile(id) => {
            out.put_u8(2);
            out.put_u32(id.0);
        }
        DamageSource::Radius(id) => {
            out.put_u8(3);
            out.put_u32(id.to_wire());
        }
        DamageSource::Melee => {
            out.put_u8(4);
        }
    }
}

fn decode_damage_source(input: &mut WireReader<'_>) -> Result<DamageSource, WireError> {
    match input.get_u8()? {
        1 => Ok(DamageSource::Shot(sim::ShotId(input.get_u32()?))),
        2 => Ok(DamageSource::Projectile(sim::ProjectileId(
            input.get_u32()?,
        ))),
        3 => Ok(DamageSource::Radius(sim::ScriptModelId::from_wire(
            input.get_u32()?,
        ))),
        4 => Ok(DamageSource::Melee),
        _ => Err(WireError::Malformed("unknown DamageSource tag")),
    }
}

fn reject_reason_tag(reason: ClassRejectReason) -> u8 {
    match reason {
        ClassRejectReason::UnknownOrStaleClass => 1,
        ClassRejectReason::NoSpawnAvailable => 2,
        ClassRejectReason::LockedContent => 3,
        ClassRejectReason::UnknownWeaponId => 4,
    }
}

fn reject_reason_from_tag(tag: u8) -> Result<ClassRejectReason, WireError> {
    match tag {
        1 => Ok(ClassRejectReason::UnknownOrStaleClass),
        2 => Ok(ClassRejectReason::NoSpawnAvailable),
        3 => Ok(ClassRejectReason::LockedContent),
        4 => Ok(ClassRejectReason::UnknownWeaponId),
        _ => Err(WireError::Malformed("unknown ClassRejectReason tag")),
    }
}

fn give_reject_reason_tag(reason: GiveRejectReason) -> u8 {
    match reason {
        GiveRejectReason::NotAlive => 1,
        GiveRejectReason::InvalidWeapon => 2,
        GiveRejectReason::UnknownWeaponId => 3,
        GiveRejectReason::EmptyCombatProfile => 4,
        GiveRejectReason::UnsupportedWeapon => 5,
    }
}

fn give_reject_reason_from_tag(tag: u8) -> Result<GiveRejectReason, WireError> {
    match tag {
        1 => Ok(GiveRejectReason::NotAlive),
        2 => Ok(GiveRejectReason::InvalidWeapon),
        3 => Ok(GiveRejectReason::UnknownWeaponId),
        4 => Ok(GiveRejectReason::EmptyCombatProfile),
        5 => Ok(GiveRejectReason::UnsupportedWeapon),
        _ => Err(WireError::Malformed("unknown GiveRejectReason tag")),
    }
}

fn configuration_change_reject_reason_tag(reason: ConfigurationChangeRejectReason) -> u8 {
    match reason {
        ConfigurationChangeRejectReason::NotAlive => 1,
        ConfigurationChangeRejectReason::StaleSource => 2,
        ConfigurationChangeRejectReason::InvalidTarget => 3,
        ConfigurationChangeRejectReason::DifferentFamily => 4,
        ConfigurationChangeRejectReason::Busy => 7,
        ConfigurationChangeRejectReason::NoInventorySlot => 5,
        ConfigurationChangeRejectReason::AmmoTableFull => 6,
        ConfigurationChangeRejectReason::SharedAmmoConflict => 8,
    }
}

fn configuration_change_reject_reason_from_tag(
    tag: u8,
) -> Result<ConfigurationChangeRejectReason, WireError> {
    match tag {
        1 => Ok(ConfigurationChangeRejectReason::NotAlive),
        2 => Ok(ConfigurationChangeRejectReason::StaleSource),
        3 => Ok(ConfigurationChangeRejectReason::InvalidTarget),
        4 => Ok(ConfigurationChangeRejectReason::DifferentFamily),
        7 => Ok(ConfigurationChangeRejectReason::Busy),
        5 => Ok(ConfigurationChangeRejectReason::NoInventorySlot),
        6 => Ok(ConfigurationChangeRejectReason::AmmoTableFull),
        8 => Ok(ConfigurationChangeRejectReason::SharedAmmoConflict),
        _ => Err(WireError::Malformed(
            "unknown ConfigurationChangeRejectReason tag",
        )),
    }
}

fn lifecycle_tag(life: ClientLifecycle) -> u8 {
    match life {
        ClientLifecycle::Connecting => 0,
        ClientLifecycle::ChoosingClass => 1,
        ClientLifecycle::Alive => 2,
        ClientLifecycle::Dead => 3,
        ClientLifecycle::Spectating => 4,
        ClientLifecycle::SpawnPending => 5,
        ClientLifecycle::RespawnPending => 6,
        ClientLifecycle::Intermission => 7,
    }
}

fn lifecycle_from_tag(tag: u8) -> Result<ClientLifecycle, WireError> {
    match tag {
        0 => Ok(ClientLifecycle::Connecting),
        1 => Ok(ClientLifecycle::ChoosingClass),
        2 => Ok(ClientLifecycle::Alive),
        3 => Ok(ClientLifecycle::Dead),
        4 => Ok(ClientLifecycle::Spectating),
        5 => Ok(ClientLifecycle::SpawnPending),
        6 => Ok(ClientLifecycle::RespawnPending),
        7 => Ok(ClientLifecycle::Intermission),
        _ => Err(WireError::Malformed("unknown ClientLifecycle tag")),
    }
}

fn phase_tag(phase: MatchPhase) -> u8 {
    match phase {
        MatchPhase::Warmup => 0,
        MatchPhase::Playing => 1,
        MatchPhase::Intermission => 2,
        MatchPhase::PostGame => 3,
    }
}

fn phase_from_tag(tag: u8) -> Result<MatchPhase, WireError> {
    match tag {
        0 => Ok(MatchPhase::Warmup),
        1 => Ok(MatchPhase::Playing),
        2 => Ok(MatchPhase::Intermission),
        3 => Ok(MatchPhase::PostGame),
        _ => Err(WireError::Malformed("unknown MatchPhase tag")),
    }
}

fn encode_corpse_pool(out: &mut WireWriter, pool: &PlayerCorpsePool) {
    out.put_u8(pool.spawn_ring);
    for slot in &pool.slots {
        if !slot.occupied {
            out.put_u8(0);
            continue;
        }
        out.put_u8(1);
        out.put_u32(slot.victim.0);
        for v in slot.origin {
            out.put_f32(v);
        }
        for v in slot.viewangles {
            out.put_f32(v);
        }
        out.put_i32(slot.anim.legs_anim);
        out.put_i32(slot.anim.torso_anim);
        out.put_f32(slot.view_height_current);
        out.put_u32(slot.weapon);
        out.put_u32(slot.e_flags);
        out.put_i32(slot.entnum);
        out.put_i32(slot.tr_type);
        out.put_i32(slot.tr_time);
        out.put_i32(slot.tr_duration);
        for v in slot.tr_delta {
            out.put_f32(v);
        }
        for v in slot.tr_base {
            out.put_f32(v);
        }
        out.put_u8(u8::from(slot.falling));
        out.put_i32(slot.ground_entity_num);
    }
}

fn decode_corpse_pool(input: &mut WireReader<'_>) -> Result<PlayerCorpsePool, WireError> {
    let spawn_ring = input.get_u8()?;
    let mut pool = PlayerCorpsePool {
        spawn_ring,
        ..PlayerCorpsePool::default()
    };
    for slot in &mut pool.slots {
        match input.get_u8()? {
            0 => *slot = PlayerCorpseSlot::default(),
            1 => {
                *slot = PlayerCorpseSlot {
                    occupied: true,
                    victim: ClientId(input.get_u32()?),
                    origin: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    viewangles: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    anim: AnimPair {
                        legs_anim: input.get_i32()?,
                        torso_anim: input.get_i32()?,
                    },
                    view_height_current: input.get_f32()?,
                    weapon: input.get_u32()?,
                    e_flags: input.get_u32()?,
                    entnum: input.get_i32()?,
                    tr_type: input.get_i32()?,
                    tr_time: input.get_i32()?,
                    tr_duration: input.get_i32()?,
                    tr_delta: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    tr_base: [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    falling: input.get_u8()? != 0,
                    ground_entity_num: input.get_i32()?,
                };
            }
            _ => return Err(WireError::Malformed("bad corpse slot tag")),
        }
    }
    Ok(pool)
}

fn encode_entity_states(out: &mut WireWriter, rows: &[entity_iw4::EntityState]) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for es in rows {
        encode_entity_state(out, es);
    }
}

fn encode_entity_state(out: &mut WireWriter, es: &entity_iw4::EntityState) {
    out.put_i32(es.number);
    out.put_i32(es.e_type);
    out.put_u32(es.e_flags);
    out.put_i32(es.tr_time);
    out.put_i32(es.tr_type);
    out.put_i32(es.tr_duration);
    for v in es.tr_delta {
        out.put_f32(v);
    }
    for v in es.tr_base {
        out.put_f32(v);
    }
    out.put_i32(es.apos_tr_time);
    out.put_i32(es.apos_tr_type);
    out.put_i32(es.apos_tr_duration);
    for v in es.apos_tr_delta {
        out.put_f32(v);
    }
    for v in es.apos_tr_base {
        out.put_f32(v);
    }
    out.put_i32(es.index);
    out.put_i32(es.launch_time());
    out.put_i32(es.client_num);

    out.put_i32(es.legs_anim);
    out.put_i32(es.torso_anim);
    out.put_i32(es.ground_entity_num);
    out.put_i32(es.event_sequence);
    for v in es.events {
        out.put_i32(v);
    }
    for v in es.event_parms {
        out.put_i32(v);
    }
}

fn decode_entity_states(
    input: &mut WireReader<'_>,
) -> Result<Vec<entity_iw4::EntityState>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        rows.push(decode_entity_state(input)?);
    }
    Ok(rows)
}

fn decode_entity_state(input: &mut WireReader<'_>) -> Result<entity_iw4::EntityState, WireError> {
    let mut es = entity_iw4::EntityState::default();
    es.number = input.get_i32()?;
    es.e_type = input.get_i32()?;
    es.e_flags = input.get_u32()?;
    es.tr_time = input.get_i32()?;
    es.tr_type = input.get_i32()?;
    es.tr_duration = input.get_i32()?;
    es.tr_delta = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    es.tr_base = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    es.apos_tr_time = input.get_i32()?;
    es.apos_tr_type = input.get_i32()?;
    es.apos_tr_duration = input.get_i32()?;
    es.apos_tr_delta = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    es.apos_tr_base = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    es.index = input.get_i32()?;
    es.set_launch_time(input.get_i32()?);
    es.client_num = input.get_i32()?;
    es.legs_anim = input.get_i32()?;
    es.torso_anim = input.get_i32()?;
    es.ground_entity_num = input.get_i32()?;
    es.event_sequence = input.get_i32()?;
    es.events = [
        input.get_i32()?,
        input.get_i32()?,
        input.get_i32()?,
        input.get_i32()?,
    ];
    es.event_parms = [
        input.get_i32()?,
        input.get_i32()?,
        input.get_i32()?,
        input.get_i32()?,
    ];
    Ok(es)
}

fn encode_script_movers(out: &mut WireWriter, movers: &[sim::ScriptMoverGentity]) {
    debug_assert!(movers.len() <= u16::MAX as usize);
    out.put_u16(movers.len() as u16);
    for mover in movers {
        out.put_u32(mover.id.to_wire());
        encode_entity_state(out, &mover.state);
    }
}

fn decode_script_movers(
    input: &mut WireReader<'_>,
) -> Result<Vec<sim::ScriptMoverGentity>, WireError> {
    let count = input.get_u16()? as usize;
    let mut movers = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        movers.push(sim::ScriptMoverGentity {
            id: ScriptModelId::from_wire(input.get_u32()?),
            state: decode_entity_state(input)?,
            ..Default::default()
        });
    }
    Ok(movers)
}

fn encode_entity_kernel(out: &mut WireWriter, kernel: &EntityKernelSnapshot) {
    debug_assert!(kernel.validate().is_ok());
    out.put_i32(kernel.high_water);
    out.put_i32(kernel.level_time_ms);
    out.put_u32(kernel.frame_serial);
    out.put_u16(kernel.slots.len() as u16);
    for slot in &kernel.slots {
        out.put_u32(slot.generation);
        out.put_i32(slot.freed_at_ms);
        let Some(occupied) = slot.occupied else {
            out.put_u8(0);
            continue;
        };
        out.put_u8(1);
        out.put_u8(entity_run_kind_tag(occupied.kind));
        out.put_u8(u8::from(occupied.linked));
        encode_optional_entity_ref(out, occupied.relations.owner);
        encode_optional_entity_ref(out, occupied.relations.parent);
        encode_optional_entity_ref(out, occupied.relations.ground);
        out.put_i32(occupied.relations.parent_tag);
        encode_axis43(
            out,
            occupied.relations.parent_link_axis,
            occupied.relations.parent_link_origin,
        );
        encode_optional_i32(out, occupied.next_think_ms);
        encode_optional_i32(out, occupied.transient_event_time_ms);
    }
    out.put_u16(kernel.free_fifo.len() as u16);
    for number in &kernel.free_fifo {
        out.put_i32(*number);
    }
}

fn decode_entity_kernel(input: &mut WireReader<'_>) -> Result<EntityKernelSnapshot, WireError> {
    let high_water = input.get_i32()?;
    let level_time_ms = input.get_i32()?;
    let frame_serial = input.get_u32()?;
    let slot_count = input.get_u16()? as usize;
    let mut slots = Vec::with_capacity(slot_count);
    for _ in 0..slot_count {
        let generation = input.get_u32()?;
        let freed_at_ms = input.get_i32()?;
        let occupied = match input.get_u8()? {
            0 => None,
            1 => {
                let kind = entity_run_kind_from_tag(input.get_u8()?)?;
                let linked = input.get_u8()? != 0;
                let owner = decode_optional_entity_ref(input)?;
                let parent = decode_optional_entity_ref(input)?;
                let ground = decode_optional_entity_ref(input)?;
                let parent_tag = input.get_i32()?;
                let (parent_link_axis, parent_link_origin) = decode_axis43(input)?;
                Some(EntityKernelOccupiedSnapshot {
                    kind,
                    linked,
                    relations: EntityRelations {
                        owner,
                        parent,
                        ground,
                        parent_tag,
                        parent_link_axis,
                        parent_link_origin,
                    },
                    next_think_ms: decode_optional_i32(input)?,
                    transient_event_time_ms: decode_optional_i32(input)?,
                })
            }
            _ => return Err(WireError::Malformed("bad EntityKernel occupancy tag")),
        };
        slots.push(EntityKernelSlotSnapshot {
            generation,
            occupied,
            freed_at_ms,
        });
    }
    let free_count = input.get_u16()? as usize;
    if free_count > slot_count {
        return Err(WireError::Malformed(
            "EntityKernel free FIFO exceeds slot count",
        ));
    }
    let mut free_fifo = Vec::with_capacity(free_count);
    for _ in 0..free_count {
        free_fifo.push(input.get_i32()?);
    }
    let kernel = EntityKernelSnapshot {
        slots,
        high_water,
        free_fifo,
        level_time_ms,
        frame_serial,
    };
    kernel
        .validate()
        .map_err(|_| WireError::Malformed("invalid EntityKernel snapshot"))?;
    Ok(kernel)
}

fn encode_optional_entity_ref(out: &mut WireWriter, entity: Option<EntityRef>) {
    match entity {
        Some(entity) => {
            out.put_u8(1);
            out.put_i32(entity.number());
            out.put_u32(entity.generation());
        }
        None => out.put_u8(0),
    }
}

fn decode_optional_entity_ref(input: &mut WireReader<'_>) -> Result<Option<EntityRef>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => EntityRef::from_parts(input.get_i32()?, input.get_u32()?)
            .map(Some)
            .map_err(|_| WireError::Malformed("EntityKernel relation number out of range")),
        _ => Err(WireError::Malformed("bad EntityKernel relation tag")),
    }
}

fn encode_optional_i32(out: &mut WireWriter, value: Option<i32>) {
    match value {
        Some(value) => {
            out.put_u8(1);
            out.put_i32(value);
        }
        None => out.put_u8(0),
    }
}

fn decode_optional_i32(input: &mut WireReader<'_>) -> Result<Option<i32>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => Ok(Some(input.get_i32()?)),
        _ => Err(WireError::Malformed("bad optional i32 tag")),
    }
}

fn encode_axis43(out: &mut WireWriter, axis: [[f32; 3]; 3], origin: [f32; 3]) {
    for row in axis {
        for c in row {
            out.put_f32(c);
        }
    }
    for c in origin {
        out.put_f32(c);
    }
}

fn decode_axis43(input: &mut WireReader<'_>) -> Result<([[f32; 3]; 3], [f32; 3]), WireError> {
    let axis = [
        [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        [input.get_f32()?, input.get_f32()?, input.get_f32()?],
        [input.get_f32()?, input.get_f32()?, input.get_f32()?],
    ];
    let origin = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    Ok((axis, origin))
}

fn entity_run_kind_tag(kind: EntityRunKind) -> u8 {
    match kind {
        EntityRunKind::ScriptMover => 0,
        EntityRunKind::Item => 1,
        EntityRunKind::Missile => 2,
        EntityRunKind::TempEvent => 3,
        EntityRunKind::PrimaryLight => 4,
        EntityRunKind::General => 5,
        EntityRunKind::PlayerCorpse => 6,
    }
}

fn entity_run_kind_from_tag(tag: u8) -> Result<EntityRunKind, WireError> {
    match tag {
        0 => Ok(EntityRunKind::ScriptMover),
        1 => Ok(EntityRunKind::Item),
        2 => Ok(EntityRunKind::Missile),
        3 => Ok(EntityRunKind::TempEvent),
        4 => Ok(EntityRunKind::PrimaryLight),
        5 => Ok(EntityRunKind::General),
        6 => Ok(EntityRunKind::PlayerCorpse),
        _ => Err(WireError::Malformed("unknown EntityRunKind tag")),
    }
}

fn encode_item_ammo(out: &mut WireWriter, rows: &[DroppedItemAmmo]) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for row in rows {
        out.put_i32(row.entnum);
        out.put_i32(row.clip_r);
        out.put_i32(row.clip_l);
        out.put_i32(row.stock);
        out.put_i32(row.scavenger);
    }
}

fn decode_item_ammo(input: &mut WireReader<'_>) -> Result<Vec<DroppedItemAmmo>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        rows.push(DroppedItemAmmo {
            entnum: input.get_i32()?,
            clip_r: input.get_i32()?,
            clip_l: input.get_i32()?,
            stock: input.get_i32()?,
            scavenger: input.get_i32()?,
        });
    }
    Ok(rows)
}

fn encode_item_pickups(out: &mut WireWriter, rows: &[ItemPickupRecord]) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for row in rows {
        out.put_i32(row.picker);
        out.put_u32(row.weapon);
        out.put_i32(row.from_entnum);
        out.put_i32(row.clip_r);
        out.put_i32(row.clip_l);
        out.put_i32(row.stock);
        out.put_i32(row.swapped_entnum);
        out.put_i32(row.picker_pm_type);
    }
}

fn decode_item_pickups(input: &mut WireReader<'_>) -> Result<Vec<ItemPickupRecord>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        rows.push(ItemPickupRecord {
            picker: input.get_i32()?,
            weapon: input.get_u32()?,
            from_entnum: input.get_i32()?,
            clip_r: input.get_i32()?,
            clip_l: input.get_i32()?,
            stock: input.get_i32()?,
            swapped_entnum: input.get_i32()?,
            picker_pm_type: input.get_i32()?,
        });
    }
    Ok(rows)
}

fn encode_entity_dobjs(
    out: &mut WireWriter,
    rows: &[(sim::AuthorityModelOwner, sim::DObjSemanticState)],
) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for (owner, state) in rows {
        match owner {
            sim::AuthorityModelOwner::ScriptModel(id) => {
                out.put_u8(0);
                out.put_u32(id.to_wire());
            }
        }
        out.put_u32(state.composition.revision);
        debug_assert!(state.composition.models.len() <= u16::MAX as usize);
        out.put_u16(state.composition.models.len() as u16);
        for model in &state.composition.models {
            put_text(out, &model.model);
            out.put_u16(model.parent_model.unwrap_or(u16::MAX));
            put_optional_text(out, model.attach_tag.as_deref());
            out.put_u8(u8::from(model.ignore_collision));
        }
        out.put_u32(state.pose_revision);
        put_optional_part_bits(out, state.requested_parts);
        for word in state.hide_part_bits.words() {
            out.put_u32(*word);
        }
        match &state.tree {
            None => out.put_u8(0),
            Some(tree) => {
                out.put_u8(1);
                out.put_u32(tree.definition_revision);
                out.put_u32(tree.state_revision);
                debug_assert!(tree.nodes.len() <= u16::MAX as usize);
                out.put_u16(tree.nodes.len() as u16);
                for node in &tree.nodes {
                    out.put_u16(node.parent.map_or(u16::MAX, |id| id.0));
                    out.put_u8(match node.kind {
                        sim::XAnimSemanticNodeKind::Blend => 0,
                        sim::XAnimSemanticNodeKind::Additive => 1,
                        sim::XAnimSemanticNodeKind::Leaf => 2,
                    });
                    put_optional_text(out, node.clip.as_deref());
                    put_optional_part_bits(out, node.parts);
                    let state = node.state;
                    out.put_f32(state.time);
                    out.put_f32(state.old_time);
                    out.put_i32(i32::from(state.cycle_count));
                    out.put_i32(i32::from(state.old_cycle_count));
                    out.put_f32(state.goal_time);
                    out.put_f32(state.goal_weight);
                    out.put_f32(state.weight);
                    out.put_f32(state.rate);
                }
            }
        }
    }
}

fn decode_entity_dobjs(
    input: &mut WireReader<'_>,
) -> Result<Vec<(sim::AuthorityModelOwner, sim::DObjSemanticState)>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let owner = match input.get_u8()? {
            0 => sim::AuthorityModelOwner::ScriptModel(sim::ScriptModelId::from_wire(
                input.get_u32()?,
            )),
            _ => return Err(WireError::Malformed("unknown DObj owner tag")),
        };
        let composition_revision = input.get_u32()?;
        let model_count = input.get_u16()? as usize;
        let mut models = Vec::with_capacity(model_count.min(20));
        for _ in 0..model_count {
            let model = get_text(input)?;
            let parent = input.get_u16()?;
            models.push(sim::DObjModelDescriptor {
                model,
                parent_model: (parent != u16::MAX).then_some(parent),
                attach_tag: get_optional_text(input)?,
                ignore_collision: input.get_u8()? != 0,
            });
        }
        let pose_revision = input.get_u32()?;
        let requested_parts = get_optional_part_bits(input)?;
        let mut hide_words = [0u32; sim::PartBits::WORDS];
        for word in &mut hide_words {
            *word = input.get_u32()?;
        }
        let tree = match input.get_u8()? {
            0 => None,
            1 => {
                let definition_revision = input.get_u32()?;
                let state_revision = input.get_u32()?;
                let node_count = input.get_u16()? as usize;
                let mut nodes = Vec::with_capacity(node_count.min(256));
                for _ in 0..node_count {
                    let parent = input.get_u16()?;
                    let kind = match input.get_u8()? {
                        0 => sim::XAnimSemanticNodeKind::Blend,
                        1 => sim::XAnimSemanticNodeKind::Additive,
                        2 => sim::XAnimSemanticNodeKind::Leaf,
                        _ => return Err(WireError::Malformed("unknown XAnim node tag")),
                    };
                    let clip = get_optional_text(input)?;
                    let parts = get_optional_part_bits(input)?;
                    let time = input.get_f32()?;
                    let old_time = input.get_f32()?;
                    let cycle_count = i16::try_from(input.get_i32()?)
                        .map_err(|_| WireError::Malformed("XAnim cycle count overflow"))?;
                    let old_cycle_count = i16::try_from(input.get_i32()?)
                        .map_err(|_| WireError::Malformed("XAnim old cycle count overflow"))?;
                    nodes.push(sim::XAnimSemanticNode {
                        parent: (parent != u16::MAX).then_some(sim::XAnimNodeId(parent)),
                        kind,
                        clip,
                        parts,
                        state: sim::XAnimNodeState {
                            time,
                            old_time,
                            cycle_count,
                            old_cycle_count,
                            goal_time: input.get_f32()?,
                            goal_weight: input.get_f32()?,
                            weight: input.get_f32()?,
                            rate: input.get_f32()?,
                        },
                    });
                }
                Some(sim::XAnimTreeSnapshot {
                    definition_revision,
                    state_revision,
                    nodes,
                })
            }
            _ => return Err(WireError::Malformed("unknown optional XAnim tree tag")),
        };
        rows.push((
            owner,
            sim::DObjSemanticState {
                composition: sim::DObjCompositionDescriptor {
                    revision: composition_revision,
                    models,
                },
                pose_revision,
                tree,
                requested_parts,
                hide_part_bits: sim::HidePartBits::from_words(hide_words),
            },
        ));
    }
    Ok(rows)
}

fn put_optional_part_bits(out: &mut WireWriter, bits: Option<sim::PartBits>) {
    out.put_u8(u8::from(bits.is_some()));
    if let Some(bits) = bits {
        for word in bits.words() {
            out.put_u32(*word);
        }
    }
}

fn get_optional_part_bits(input: &mut WireReader<'_>) -> Result<Option<sim::PartBits>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => {
            let mut words = [0u32; sim::PartBits::WORDS];
            for word in &mut words {
                *word = input.get_u32()?;
            }
            Ok(Some(sim::PartBits::from_words(words)))
        }
        _ => Err(WireError::Malformed("unknown optional part-bits tag")),
    }
}

fn put_text(out: &mut WireWriter, text: &str) {
    debug_assert!(text.len() <= u16::MAX as usize);
    out.put_u16(text.len() as u16);
    out.put_bytes(text.as_bytes());
}

fn get_text(input: &mut WireReader<'_>) -> Result<String, WireError> {
    let len = input.get_u16()? as usize;
    let mut bytes = vec![0; len];
    input.get_bytes(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| WireError::Malformed("DObj text is not UTF-8"))
}

fn put_optional_text(out: &mut WireWriter, text: Option<&str>) {
    out.put_u8(u8::from(text.is_some()));
    if let Some(text) = text {
        put_text(out, text);
    }
}

fn get_optional_text(input: &mut WireReader<'_>) -> Result<Option<String>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => get_text(input).map(Some),
        _ => Err(WireError::Malformed("unknown optional text tag")),
    }
}

fn encode_map_doors(out: &mut WireWriter, doors: Option<&sim::MapDoors>) {
    out.put_u8(u8::from(doors.is_some()));
    let Some(d) = doors else {
        return;
    };
    for s in &d.switches {
        out.put_u32(s.cmodel);
        for v in s.origin.into_iter().chain(s.half) {
            out.put_f32(v);
        }
    }
    for l in &d.leaves {
        out.put_u32(l.model);
        out.put_u32(l.brush);
        out.put_u32(l.cmodel);
        for v in l.origin.into_iter().chain(l.angles).chain(l.model_angles) {
            out.put_f32(v);
        }
    }
    for v in d.sound_origin {
        out.put_f32(v);
    }
    for at in [d.startup_at, d.started_at] {
        out.put_u8(u8::from(at.is_some()));
        if let Some(at) = at {
            out.put_u32(at);
        }
    }
    out.put_u8(u8::from(d.open));
    out.put_u8(u8::from(d.completed));
    out.put_u8(d.alarm_count);
    out.put_u32(d.activations);
    for clients in [&d.hints, &d.held] {
        out.put_u16(clients.len() as u16);
        for id in clients {
            out.put_u32(id.0);
        }
    }
}
fn decode_map_doors(input: &mut WireReader<'_>) -> Result<Option<sim::MapDoors>, WireError> {
    if input.get_u8()? == 0 {
        return Ok(None);
    }
    let mut d = sim::MapDoors::default();
    for s in &mut d.switches {
        s.cmodel = input.get_u32()?;
        for v in s.origin.iter_mut().chain(&mut s.half) {
            *v = input.get_f32()?;
        }
    }
    for l in &mut d.leaves {
        l.model = input.get_u32()?;
        l.brush = input.get_u32()?;
        l.cmodel = input.get_u32()?;
        for v in l
            .origin
            .iter_mut()
            .chain(&mut l.angles)
            .chain(&mut l.model_angles)
        {
            *v = input.get_f32()?;
        }
    }
    for v in &mut d.sound_origin {
        *v = input.get_f32()?;
    }
    for at in [&mut d.startup_at, &mut d.started_at] {
        if input.get_u8()? != 0 {
            *at = Some(input.get_u32()?);
        }
    }
    d.open = input.get_u8()? != 0;
    d.completed = input.get_u8()? != 0;
    d.alarm_count = input.get_u8()?;
    d.activations = input.get_u32()?;
    for clients in [&mut d.hints, &mut d.held] {
        let count = input.get_u16()?;
        if count > 64 {
            return Err(WireError::Malformed("map door client count"));
        }
        for _ in 0..count {
            clients.push(ClientId(input.get_u32()?));
        }
    }
    Ok(Some(d))
}

fn encode_objective_view(out: &mut WireWriter, v: &sim::ObjectiveView) {
    out.put_u32(v.id);
    out.put_u32(v.model_source);
    put_text(out, &v.label);
    for x in v.origin {
        out.put_f32(x);
    }
    out.put_u8(v.owner as u8);
    out.put_f32(v.progress);
    out.put_u8(v.capturing as u8);
    out.put_u8(u8::from(v.contested));
    out.put_u16(v.users.len() as u16);
    for id in &v.users {
        out.put_u32(id.0);
    }
    match v.flash {
        Some(flash) => {
            out.put_u8(flash.teams);
            out.put_u32(flash.start_ms);
            objective_optional(out, flash.stop_ms);
        }
        None => out.put_u8(0),
    }
}
fn objective_team(input: &mut WireReader<'_>) -> Result<gamemode_iw4::Team, WireError> {
    gamemode_iw4::Team::from_retail_u8(input.get_u8()?)
        .ok_or(WireError::Malformed("objective team"))
}
fn decode_objective_view(input: &mut WireReader<'_>) -> Result<sim::ObjectiveView, WireError> {
    let id = input.get_u32()?;
    let model_source = input.get_u32()?;
    let label = get_text(input)?;
    let origin = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
    let owner = objective_team(input)?;
    let progress = input.get_f32()?;
    let capturing = objective_team(input)?;
    let contested = input.get_u8()? != 0;
    let count = input.get_u16()?;
    if count > 64
        || !progress.is_finite()
        || !(0.0..=1.0).contains(&progress)
        || origin.iter().any(|x| !x.is_finite())
    {
        return Err(WireError::Malformed("objective view"));
    }
    let mut users = Vec::new();
    for _ in 0..count {
        users.push(ClientId(input.get_u32()?));
    }
    let teams = input.get_u8()?;
    let flash = match teams {
        0 => None,
        teams if teams & !(sim::ObjectiveFlash::AXIS | sim::ObjectiveFlash::ALLIES) == 0 => {
            Some(sim::ObjectiveFlash {
                teams,
                start_ms: input.get_u32()?,
                stop_ms: read_objective_optional(input)?,
            })
        }
        _ => return Err(WireError::Malformed("objective flash")),
    };
    Ok(sim::ObjectiveView {
        id,
        model_source,
        label,
        origin,
        owner,
        progress,
        capturing,
        contested,
        users,
        flash,
    })
}
fn objective_optional(out: &mut WireWriter, value: Option<u32>) {
    out.put_u8(u8::from(value.is_some()));
    if let Some(v) = value {
        out.put_u32(v);
    }
}
fn read_objective_optional(input: &mut WireReader<'_>) -> Result<Option<u32>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => Ok(Some(input.get_u32()?)),
        _ => Err(WireError::Malformed("objective optional")),
    }
}
fn encode_objectives(out: &mut WireWriter, state: &sim::ObjectiveMatch) {
    for model in &state.flag_models {
        put_text(out, model);
    }
    for weapon in state.use_weapons {
        out.put_u32(weapon);
    }
    out.put_u16(state.restoring.len() as u16);
    for (id, weapon, temporary) in &state.restoring {
        out.put_u32(id.0);
        out.put_u32(*weapon);
        out.put_u32(*temporary);
    }
    out.put_u16(state.flags.len() as u16);
    for flag in &state.flags {
        encode_objective_view(out, flag);
    }
    out.put_u16(state.bombs.len() as u16);
    for b in &state.bombs {
        encode_objective_view(out, &b.view);
        for sources in [&b.intact_sources, &b.destroyed_sources] {
            out.put_u16(sources.len() as u16);
            for id in sources {
                out.put_u32(*id);
            }
        }
        out.put_u16(b.hulls.len() as u16);
        for h in &b.hulls {
            for x in h.mid.into_iter().chain(h.half) {
                out.put_f32(x);
            }
            out.put_u16(h.slabs.len() as u16);
            for (dir, mid, half) in &h.slabs {
                for x in *dir {
                    out.put_f32(x);
                }
                out.put_f32(*mid);
                out.put_f32(*half);
            }
        }
        for x in b
            .mins
            .into_iter()
            .chain(b.maxs)
            .chain(b.bomb_origin)
            .chain(b.bomb_angles)
        {
            out.put_f32(x);
        }
        objective_optional(out, b.planted_at_ms);
        objective_optional(out, b.planter.map(|id| id.0));
        out.put_u8(u8::from(b.destroyed));
        objective_optional(out, b.user.map(|id| id.0));
        objective_optional(out, b.return_weapon);
        out.put_i32(b.hold.cur_progress);
        out.put_f32(b.hold.use_rate);
        out.put_u8(u8::from(b.hold.wait_for_weapon));
        out.put_i32(b.hold.timed_out_ms);
        out.put_u8(u8::from(b.hold.in_use));
    }
    for score in state.scores {
        out.put_i32(score);
    }
    out.put_u8(state.attackers as u8);
    out.put_u32(state.round);
    out.put_u32(state.round_remaining_ms);
    objective_optional(out, state.round_end_at_ms);
    objective_optional(out, state.winner.map(|t| t as u32));
    out.put_u8(u8::from(state.match_over));
}
fn decode_objectives(input: &mut WireReader<'_>) -> Result<sim::ObjectiveMatch, WireError> {
    let mut state = sim::ObjectiveMatch::default();
    for model in &mut state.flag_models {
        *model = get_text(input)?;
    }
    for weapon in &mut state.use_weapons {
        *weapon = input.get_u32()?;
    }
    let count = input.get_u16()?;
    if count > 64 {
        return Err(WireError::Malformed("too many objective restores"));
    }
    for _ in 0..count {
        state.restoring.push((
            ClientId(input.get_u32()?),
            input.get_u32()?,
            input.get_u32()?,
        ));
    }
    let count = input.get_u16()?;
    if count > 16 {
        return Err(WireError::Malformed("too many flags"));
    }
    for _ in 0..count {
        state.flags.push(decode_objective_view(input)?);
    }
    let count = input.get_u16()?;
    if count > 2 {
        return Err(WireError::Malformed("too many bomb sites"));
    }
    for _ in 0..count {
        let view = decode_objective_view(input)?;
        let mut sources = [Vec::new(), Vec::new()];
        for list in &mut sources {
            let count = input.get_u16()?;
            if count > 1024 {
                return Err(WireError::Malformed("too many objective visuals"));
            }
            for _ in 0..count {
                list.push(input.get_u32()?);
            }
        }
        let [intact_sources, destroyed_sources] = sources;
        let count = input.get_u16()?;
        if count > 1024 {
            return Err(WireError::Malformed("too many objective hulls"));
        }
        let mut hulls = Vec::new();
        for _ in 0..count {
            let mid = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
            let half = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
            let count = input.get_u16()?;
            if count > 1024 {
                return Err(WireError::Malformed("too many objective slabs"));
            }
            let mut slabs = Vec::new();
            for _ in 0..count {
                slabs.push((
                    [input.get_f32()?, input.get_f32()?, input.get_f32()?],
                    input.get_f32()?,
                    input.get_f32()?,
                ));
            }
            if mid.iter().any(|v| !v.is_finite())
                || half.iter().any(|v| !v.is_finite() || *v < 0.0)
                || slabs.iter().any(|(dir, center, extent)| {
                    dir.iter().any(|v| !v.is_finite())
                        || !center.is_finite()
                        || !extent.is_finite()
                        || *extent < 0.0
                })
            {
                return Err(WireError::Malformed("objective hull"));
            }
            hulls.push(sim::ObjectiveHull { mid, half, slabs });
        }
        let mins = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
        let maxs = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
        let bomb_origin = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
        let bomb_angles = [input.get_f32()?, input.get_f32()?, input.get_f32()?];
        let planted_at_ms = read_objective_optional(input)?;
        let planter = read_objective_optional(input)?.map(ClientId);
        let destroyed = input.get_u8()? != 0;
        let user = read_objective_optional(input)?.map(ClientId);
        let return_weapon = read_objective_optional(input)?;
        let hold = gamemode_iw4::UseHoldLoopState {
            cur_progress: input.get_i32()?,
            use_rate: input.get_f32()?,
            wait_for_weapon: input.get_u8()? != 0,
            timed_out_ms: input.get_i32()?,
            in_use: input.get_u8()? != 0,
        };
        if mins
            .iter()
            .chain(&maxs)
            .chain(&bomb_origin)
            .chain(&bomb_angles)
            .any(|x| !x.is_finite())
            || !hold.use_rate.is_finite()
            || (0..3).any(|i| mins[i] > maxs[i])
        {
            return Err(WireError::Malformed("bomb bounds/hold"));
        }
        state.bombs.push(sim::BombSite {
            view,
            intact_sources,
            destroyed_sources,
            hulls,
            mins,
            maxs,
            bomb_origin,
            bomb_angles,
            planted_at_ms,
            planter,
            destroyed,
            user,
            return_weapon,
            hold,
        });
    }
    for score in &mut state.scores {
        *score = input.get_i32()?;
    }
    state.attackers = objective_team(input)?;
    state.round = input.get_u32()?;
    state.round_remaining_ms = input.get_u32()?;
    state.round_end_at_ms = read_objective_optional(input)?;
    state.winner = read_objective_optional(input)?
        .map(|n| {
            u8::try_from(n)
                .ok()
                .and_then(gamemode_iw4::Team::from_retail_u8)
                .ok_or(WireError::Malformed("objective winner"))
        })
        .transpose()?;
    state.match_over = input.get_u8()? != 0;
    Ok(state)
}
