use entity_iw4::{
    GGlassPiece, GLASS_DAMAGE_INVALID, glass_apply_damage, glass_collapse_piece, glass_is_solid,
    glass_shatter_seed_from_hit, glass_state_from_damage, glass_weakened_collapse_time_cs,
};

use crate::identities::ScriptModelId;

pub use entity_iw4::{
    GLASS_BLAST_DAMAGE_SCALE, GLASS_BLAST_RADIUS_CAP, GLASS_DAMAGE_TO_DESTROY,
    GLASS_DAMAGE_TO_WEAKEN, GLASS_FRACTURE_PROFILE_VERSION, GLASS_MELEE_DAMAGE,
    GLASS_PROJECTILE_PANE_HOPS, GlassBreakRecord, GlassCause, GlassPaneBasis, GlassPieceState,
    GlassShatterSeed, MISSILE_GLASS_SHATTER_VEL, glass_blast_cone_keeps,
    glass_blast_integer_damage,
};

pub type GlassPieceId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassPieceSnapshot {
    pub state: GlassPieceState,
    pub revision: u32,
    pub last_state_change_time: i32,
    pub shatter_seed: Option<GlassShatterSeed>,
    pub deterministic_seed: u64,
    pub cause: GlassCause,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectSnapshot {
    pub as_of_ms: i32,
    pub map_round_epoch: u32,
    pub fracture_profile_version: u32,
    pub glass_pieces: Vec<(GlassPieceId, GlassPieceSnapshot)>,

    pub destructible_loop_sounds: Vec<DestructibleLoopSound>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestructibleLoopSound {
    pub owner: ScriptModelId,
    pub alias_index: u8,
    pub origin: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectState {
    destructible_loop_sounds: Vec<DestructibleLoopSound>,
    glass_pieces: Vec<(GlassPieceId, GGlassPiece)>,

    glass_native: Vec<(GlassPieceId, GlassNativeMeta)>,

    glass_panes: Vec<(GlassPieceId, GlassPaneBasis)>,

    map_round_epoch: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GlassNativeMeta {
    revision: u32,
    cause: GlassCause,
    deterministic_seed: u64,
}

impl WorldObjectState {
    pub fn set_destructible_loop_sounds(&mut self, rows: Vec<DestructibleLoopSound>) {
        self.destructible_loop_sounds = rows;
    }

    pub fn glass_piece_state(&self, id: GlassPieceId) -> GlassPieceState {
        self.glass_piece(id).state()
    }

    pub fn glass_damage(&self, id: GlassPieceId) -> u16 {
        self.glass_piece(id).damage
    }

    pub fn glass_is_solid(&self, id: GlassPieceId) -> bool {
        glass_is_solid(self.glass_piece(id).damage)
    }

    pub fn glass_damage_pairs(&self) -> Vec<(GlassPieceId, u16)> {
        self.glass_pieces
            .iter()
            .map(|(id, piece)| (*id, piece.damage))
            .collect()
    }

    pub fn glass_radius_targets(&self) -> Vec<(GlassPieceId, GlassPaneBasis)> {
        self.glass_panes
            .iter()
            .copied()
            .filter(|(id, _)| self.glass_is_solid(*id))
            .collect()
    }

    pub fn apply_glass_damage(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        weakened_collapse_time_cs: Option<u16>,
        shatter_seed: Option<GlassShatterSeed>,
    ) -> GlassPieceState {
        self.apply_glass_damage_caused(
            id,
            damage,
            at_time_ms,
            weakened_collapse_time_cs,
            shatter_seed,
            GlassCause::Impact,
        )
    }

    pub fn apply_glass_damage_caused(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        weakened_collapse_time_cs: Option<u16>,
        shatter_seed: Option<GlassShatterSeed>,
        cause: GlassCause,
    ) -> GlassPieceState {
        if damage == 0 {
            return self.glass_piece_state(id);
        }
        let mut piece = self.glass_piece(id);
        if piece.damage == GLASS_DAMAGE_INVALID {
            return GlassPieceState::Deleted;
        }
        if let Some(change) = glass_apply_damage(
            &mut piece,
            damage,
            at_time_ms,
            weakened_collapse_time_cs,
            shatter_seed,
        ) {
            let mut meta = self.glass_native_meta(id);
            meta.revision = meta.revision.saturating_add(1);
            if change.current == GlassPieceState::Shattered {
                meta.cause = cause;
                meta.deterministic_seed = GlassBreakRecord::mix_seed(id, at_time_ms, shatter_seed);
            } else if change.current == GlassPieceState::Weakened {
                meta.cause = cause;
            }
            upsert_value(&mut self.glass_native, id, meta);
        }
        if piece.damage == 0 {
            remove_key(&mut self.glass_pieces, id);
            remove_key(&mut self.glass_native, id);
        } else {
            upsert_value(&mut self.glass_pieces, id, piece);
        }
        self.glass_piece_state(id)
    }

    pub fn script_destroy_glass(&mut self, id: GlassPieceId, at_time_ms: i32) -> GlassPieceState {
        let (hit, dir) = self
            .glass_pane(id)
            .map(|pane| (pane.origin, [0.0, 0.0, 1.0]))
            .unwrap_or(([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        self.force_shatter_glass(id, at_time_ms, hit, dir, GlassCause::Script)
    }

    pub fn force_shatter_glass(
        &mut self,
        id: GlassPieceId,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        cause: GlassCause,
    ) -> GlassPieceState {
        if !self.glass_is_solid(id) {
            return self.glass_piece_state(id);
        }
        let seed = self
            .glass_pane(id)
            .and_then(|pane| glass_shatter_seed_from_hit(pane, hit, dir));
        self.apply_glass_damage_caused(
            id,
            u32::from(GLASS_DAMAGE_TO_DESTROY),
            at_time_ms,
            None,
            seed,
            cause,
        )
    }

    pub fn apply_glass_blast(
        &mut self,
        origin: [f32; 3],
        inner_damage: i32,
        outer_damage: i32,
        radius: f32,
        at_time_ms: i32,
        next_random: &mut impl FnMut() -> f32,
    ) {
        self.apply_glass_blast_oriented(
            origin,
            inner_damage,
            outer_damage,
            radius,
            [0.0; 3],
            0.0,
            at_time_ms,
            next_random,
        );
    }

    pub fn apply_glass_blast_oriented(
        &mut self,
        origin: [f32; 3],
        inner_damage: i32,
        outer_damage: i32,
        radius: f32,
        cone_dir: [f32; 3],
        cone_cos: f32,
        at_time_ms: i32,
        next_random: &mut impl FnMut() -> f32,
    ) {
        if inner_damage <= 0 && outer_damage <= 0 {
            return;
        }
        let r = if radius > entity_iw4::GLASS_BLAST_RADIUS_CAP {
            entity_iw4::GLASS_BLAST_RADIUS_CAP
        } else {
            radius
        };
        if !(r > 0.0) {
            return;
        }
        let panes: Vec<(GlassPieceId, GlassPaneBasis)> = self.glass_panes.clone();
        for (id, pane) in panes {
            if !self.glass_is_solid(id) {
                continue;
            }
            let dx = pane.origin[0] - origin[0];
            let dy = pane.origin[1] - origin[1];
            let dz = pane.origin[2] - origin[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if !glass_blast_cone_keeps(cone_dir, cone_cos, [dx, dy, dz]) {
                continue;
            }
            let amount = glass_blast_integer_damage(inner_damage, outer_damage, r, d);
            if amount == 0 {
                continue;
            }
            let dir = if d > 1.0e-4 {
                [dx / d, dy / d, dz / d]
            } else {
                [0.0, 0.0, 1.0]
            };
            let _ = self.apply_glass_hit_caused(
                id,
                amount,
                at_time_ms,
                origin,
                dir,
                next_random,
                GlassCause::Blast,
            );
        }
    }

    pub fn install_glass_panes(&mut self, panes: Vec<(GlassPieceId, GlassPaneBasis)>) {
        self.glass_panes = panes;
        sort_pairs(&mut self.glass_panes);
    }

    pub fn set_map_round_epoch(&mut self, epoch: u32) {
        self.map_round_epoch = epoch;
    }

    pub fn map_round_epoch(&self) -> u32 {
        self.map_round_epoch
    }

    pub fn glass_pane(&self, id: GlassPieceId) -> Option<GlassPaneBasis> {
        lookup_value(&self.glass_panes, id)
    }

    pub fn clone_glass_panes_from(&mut self, other: &Self) {
        self.glass_panes = other.glass_panes.clone();
    }

    pub fn apply_glass_hit(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        next_random: &mut impl FnMut() -> f32,
    ) -> GlassPieceState {
        self.apply_glass_hit_caused(
            id,
            damage,
            at_time_ms,
            hit,
            dir,
            next_random,
            GlassCause::Impact,
        )
    }

    pub fn apply_glass_hit_caused(
        &mut self,
        id: GlassPieceId,
        damage: u32,
        at_time_ms: i32,
        hit: [f32; 3],
        dir: [f32; 3],
        next_random: &mut impl FnMut() -> f32,
        cause: GlassCause,
    ) -> GlassPieceState {
        if damage == 0 {
            return self.glass_piece_state(id);
        }
        let previous = self.glass_piece_state(id);
        if previous == GlassPieceState::Deleted {
            return previous;
        }
        let next =
            glass_state_from_damage(entity_iw4::glass_add_damage(self.glass_damage(id), damage));
        let collapse = (next != previous && next == GlassPieceState::Weakened)
            .then(|| glass_weakened_collapse_time_cs(next_random));
        let seed = (next != previous && next == GlassPieceState::Shattered)
            .then(|| {
                self.glass_pane(id)
                    .and_then(|pane| glass_shatter_seed_from_hit(pane, hit, dir))
            })
            .flatten();
        self.apply_glass_damage_caused(id, damage, at_time_ms, collapse, seed, cause)
    }

    pub fn glass_update(&mut self, at_time_ms: i32) {
        let ids: Vec<GlassPieceId> = self.glass_pieces.iter().map(|(id, _)| *id).collect();
        for id in ids {
            let mut piece = self.glass_piece(id);
            if let Some(change) = glass_collapse_piece(&mut piece, at_time_ms) {
                upsert_value(&mut self.glass_pieces, id, piece);
                if change.current == GlassPieceState::Shattered {
                    let mut meta = self.glass_native_meta(id);
                    meta.revision = meta.revision.saturating_add(1);
                    meta.cause = GlassCause::Collapse;
                    meta.deterministic_seed = GlassBreakRecord::mix_seed(id, at_time_ms, None);
                    upsert_value(&mut self.glass_native, id, meta);
                }
            }
        }
    }

    pub fn to_snapshot(&self) -> WorldObjectSnapshot {
        let mut glass_pieces = self
            .glass_pieces
            .iter()
            .filter_map(|(id, piece)| {
                let state = piece.state();
                (state != GlassPieceState::Intact).then_some((
                    *id,
                    GlassPieceSnapshot {
                        state,
                        revision: self.glass_native_meta(*id).revision.max(1),
                        last_state_change_time: piece.last_state_change_time,
                        shatter_seed: piece.shatter_seed(),
                        deterministic_seed: self.glass_native_meta(*id).deterministic_seed,
                        cause: self.glass_native_meta(*id).cause,
                    },
                ))
            })
            .collect::<Vec<_>>();
        sort_pairs(&mut glass_pieces);

        WorldObjectSnapshot {
            as_of_ms: 0,
            map_round_epoch: self.map_round_epoch,
            fracture_profile_version: GLASS_FRACTURE_PROFILE_VERSION,
            glass_pieces,
            destructible_loop_sounds: self.destructible_loop_sounds.clone(),
        }
    }

    pub fn adopt_snapshot(&mut self, snap: &WorldObjectSnapshot) {
        self.map_round_epoch = snap.map_round_epoch;
        self.destructible_loop_sounds = snap.destructible_loop_sounds.clone();
        self.glass_pieces.clear();
        self.glass_native.clear();
        for (id, snapshot) in &snap.glass_pieces {
            let damage = glass_damage_for_state(snapshot.state);
            if damage != 0 {
                let mut piece = GGlassPiece::default();
                piece.damage = damage;
                piece.last_state_change_time = snapshot.last_state_change_time;
                if let Some(seed) = snapshot.shatter_seed {
                    piece.impact_dir = seed.impact_dir;
                    piece.impact_pos = seed.impact_pos;
                } else if snapshot.state == GlassPieceState::Shattered {
                    piece.impact_dir = entity_iw4::GLASS_IMPACT_DIR_NONE;
                }
                upsert_value(&mut self.glass_pieces, *id, piece);
                upsert_value(
                    &mut self.glass_native,
                    *id,
                    GlassNativeMeta {
                        revision: snapshot.revision,
                        cause: snapshot.cause,
                        deterministic_seed: snapshot.deterministic_seed,
                    },
                );
            }
        }
    }

    fn glass_piece(&self, id: GlassPieceId) -> GGlassPiece {
        match lookup_value(&self.glass_pieces, id) {
            Some(piece) => piece,
            None => GGlassPiece::default(),
        }
    }

    fn glass_native_meta(&self, id: GlassPieceId) -> GlassNativeMeta {
        lookup_value(&self.glass_native, id).unwrap_or_default()
    }

    pub fn glass_break_record(&self, id: GlassPieceId) -> Option<GlassBreakRecord> {
        let piece = self.glass_piece(id);
        if piece.state() != GlassPieceState::Shattered {
            return None;
        }
        let meta = self.glass_native_meta(id);
        Some(GlassBreakRecord {
            break_tick: piece.last_state_change_time,
            deterministic_seed: meta.deterministic_seed,
            shatter_seed: piece.shatter_seed(),
            cause: meta.cause,
        })
    }

    pub fn glass_revision(&self, id: GlassPieceId) -> u32 {
        self.glass_native_meta(id).revision
    }
}

pub(crate) fn glass_piece_is_solid(damage: u16) -> bool {
    glass_is_solid(damage)
}

fn glass_damage_for_state(state: GlassPieceState) -> u16 {
    match state {
        GlassPieceState::Intact => 0,
        GlassPieceState::Weakened => GLASS_DAMAGE_TO_WEAKEN,
        GlassPieceState::Shattered => GLASS_DAMAGE_TO_DESTROY,
        GlassPieceState::Deleted => GLASS_DAMAGE_INVALID,
    }
}

fn lookup_value<K: Copy + PartialEq, V: Copy>(table: &[(K, V)], id: K) -> Option<V> {
    table.iter().find(|(key, _)| *key == id).map(|(_, v)| *v)
}

fn upsert_value<K: Copy + Ord, V: Copy>(table: &mut Vec<(K, V)>, id: K, value: V) {
    if let Some(row) = table.iter_mut().find(|(key, _)| *key == id) {
        row.1 = value;
        return;
    }
    table.push((id, value));
    sort_pairs(table);
}

fn remove_key<K: Copy + PartialEq, V>(table: &mut Vec<(K, V)>, id: K) {
    table.retain(|(key, _)| *key != id);
}

fn sort_pairs<K: Copy + Ord, V>(table: &mut [(K, V)]) {
    table.sort_by(|(a, _), (b, _)| a.cmp(b));
}
