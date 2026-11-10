use super::wire::{WireError, WireReader, WireWriter};
use sim::{
    DestructibleLoopSound, GlassCause, GlassPieceSnapshot, GlassPieceState, GlassShatterSeed,
    ScriptModelId, WorldObjectSnapshot,
};
use std::collections::HashMap;
use std::hash::Hash;

pub(super) struct EncodedWorldObjects {
    pub(super) wire: Vec<u8>,
    pub(super) content_changed: bool,
}

pub(super) fn encode_sync(
    baseline: &WorldObjectSnapshot,
    current: &WorldObjectSnapshot,
    full: bool,
) -> EncodedWorldObjects {
    let mut out = WireWriter::with_capacity(64);
    out.put_i32(current.as_of_ms);
    out.put_u32(current.map_round_epoch);
    out.put_u32(current.fracture_profile_version);
    let content_changed;
    if full {
        content_changed = true;
        out.put_u8(6);
        encode_world_object_full(&mut out, current);
    } else if world_object_delta(baseline, current).is_empty()
        && baseline.destructible_loop_sounds == current.destructible_loop_sounds
        && baseline.portal_blocked_exits == current.portal_blocked_exits
    {
        content_changed = false;
        out.put_u8(0);
    } else {
        content_changed = true;
        out.put_u8(5);
        encode_world_object_delta(&mut out, baseline, current);
    }
    EncodedWorldObjects {
        wire: out.finish(),
        content_changed,
    }
}

pub(super) fn decode_sync(wire: &[u8]) -> Result<WorldObjectUpdate, WireError> {
    let mut input = WireReader::new(wire);
    let update = decode_world_object_sync(&mut input)?;
    if !input.is_empty() {
        return Err(WireError::Malformed(
            "trailing bytes after world object sync",
        ));
    }
    Ok(update)
}

#[derive(Debug)]
pub(super) struct PairDelta<K, V> {
    pub(super) changed: Vec<(K, V)>,
    pub(super) removed: Vec<K>,
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
) -> PairDelta<u32, GlassPieceSnapshot> {
    pair_delta(&baseline.glass_pieces, &current.glass_pieces)
}

fn encode_world_object_full(out: &mut WireWriter, snap: &WorldObjectSnapshot) {
    debug_assert!(snap.glass_pieces.len() <= u16::MAX as usize);
    out.put_u16(snap.glass_pieces.len() as u16);
    for (id, row) in &snap.glass_pieces {
        out.put_u32(*id);
        encode_glass_piece_snapshot(out, *row);
    }
    encode_destructible_loop_sounds(out, &snap.destructible_loop_sounds);
    encode_portal_exits(out, &snap.portal_blocked_exits);
}

fn encode_world_object_delta(
    out: &mut WireWriter,
    baseline: &WorldObjectSnapshot,
    current: &WorldObjectSnapshot,
) {
    let glass = world_object_delta(baseline, current);
    debug_assert!(glass.changed.len() <= u16::MAX as usize);
    debug_assert!(glass.removed.len() <= u16::MAX as usize);
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
    encode_portal_exits(out, &current.portal_blocked_exits);
}

fn encode_portal_exits(out: &mut WireWriter, exits: &[[bool; 2]]) {
    assert!(
        exits.len() <= 255,
        "portal availability exceeds render view identity range"
    );
    out.put_u8(exits.len() as u8);
    for [a, b] in exits {
        out.put_u8(u8::from(*a) | (u8::from(*b) << 1));
    }
}

fn decode_portal_exits(input: &mut WireReader<'_>) -> Result<Vec<[bool; 2]>, WireError> {
    let count = input.get_u8()?;
    (0..count)
        .map(|_| {
            let bits = input.get_u8()?;
            if bits & !3 != 0 {
                return Err(WireError::Malformed("invalid portal exit bits"));
            }
            Ok([bits & 1 != 0, bits & 2 != 0])
        })
        .collect()
}

fn encode_destructible_loop_sounds(out: &mut WireWriter, rows: &[DestructibleLoopSound]) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(rows.len() as u16);
    for row in rows {
        out.put_u32(row.owner.to_wire());
        out.put_u32(row.snd_ent.unwrap_or(u32::MAX));
        out.put_u8(row.alias_index);
        for v in row.origin {
            out.put_f32(v);
        }
    }
}

fn decode_destructible_loop_sounds(
    input: &mut WireReader<'_>,
    with_entity: bool,
) -> Result<Vec<DestructibleLoopSound>, WireError> {
    let count = input.get_u16()? as usize;
    let mut rows = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let owner = ScriptModelId::from_wire(input.get_u32()?);
        let snd_ent = if with_entity {
            match input.get_u32()? {
                u32::MAX => None,
                number if number <= i32::MAX as u32 => Some(number),
                _ => return Err(WireError::Malformed("invalid loop sound entity")),
            }
        } else {
            None
        };
        let alias_index = input.get_u8()?;
        let mut origin = [0.0; 3];
        for v in &mut origin {
            *v = input.get_f32()?;
        }
        rows.push(DestructibleLoopSound {
            snd_ent,
            owner,
            alias_index,
            origin,
        });
    }
    Ok(rows)
}

pub(super) enum WorldObjectChange {
    Unchanged,
    Delta {
        glass: PairDelta<u32, GlassPieceSnapshot>,
        loops: Vec<DestructibleLoopSound>,
        portal_exits: Vec<[bool; 2]>,
    },
    Full {
        glass: Vec<(u32, GlassPieceSnapshot)>,
        loops: Vec<DestructibleLoopSound>,
        portal_exits: Vec<[bool; 2]>,
    },
}

pub(super) struct WorldObjectUpdate {
    pub(super) as_of_ms: i32,
    pub(super) map_round_epoch: u32,
    pub(super) fracture_profile_version: u32,
    pub(super) change: WorldObjectChange,
}

fn decode_world_object_sync(input: &mut WireReader<'_>) -> Result<WorldObjectUpdate, WireError> {
    let as_of_ms = input.get_i32()?;
    let map_round_epoch = input.get_u32()?;
    let fracture_profile_version = input.get_u32()?;
    let tag = input.get_u8()?;
    let change = match tag {
        0 => WorldObjectChange::Unchanged,
        1 | 3 | 5 => {
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
            let destructible_loop_sounds = decode_destructible_loop_sounds(input, tag >= 3)?;

            WorldObjectChange::Delta {
                glass,
                loops: destructible_loop_sounds,
                portal_exits: if tag >= 5 {
                    decode_portal_exits(input)?
                } else {
                    Vec::new()
                },
            }
        }
        2 | 4 | 6 => {
            let glass_count = input.get_u16()? as usize;
            let mut glass_pieces = Vec::with_capacity(glass_count.min(4096));
            for _ in 0..glass_count {
                glass_pieces.push((input.get_u32()?, decode_glass_piece_snapshot(input)?));
            }
            let destructible_loop_sounds = decode_destructible_loop_sounds(input, tag >= 3)?;
            WorldObjectChange::Full {
                glass: glass_pieces,
                loops: destructible_loop_sounds,
                portal_exits: if tag >= 5 {
                    decode_portal_exits(input)?
                } else {
                    Vec::new()
                },
            }
        }
        _ => return Err(WireError::Malformed("unknown world object sync tag")),
    };
    Ok(WorldObjectUpdate {
        as_of_ms,
        map_round_epoch,
        fracture_profile_version,
        change,
    })
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
