use fx_iw4::{
    FX_GLASS_ACCENT_BOUNCE_CAP, FX_GLASS_AIRBORNE_CAP, FX_GLASS_AIRBORNE_PER_BREAK,
    FX_GLASS_ANGULAR_VEL_MAX, FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_CATCHUP_STEPS,
    FX_GLASS_FALL_GRAVITY, FX_GLASS_FALL_TIME_NEVER, FX_GLASS_FREE_SENTINEL,
    FX_GLASS_GEOMETRY_DATA, FX_GLASS_LANDING_AGGREGATE_MSEC, FX_GLASS_LANDING_CELL,
    FX_GLASS_LINEAR_VEL_MAX, FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_MOTION_STEP_MSEC,
    FX_GLASS_PENDING_MAX_MSEC, FX_GLASS_PENDING_MIN_MSEC, FX_GLASS_PENDING_SUPPORT_FRAC,
    FX_GLASS_PIECE_DYNAMICS, FX_GLASS_PIECE_PLACE, FX_GLASS_PIECE_STATE, FX_GLASS_RESTITUTION,
    FX_GLASS_SETTLED_CAP, FX_GLASS_SETTLED_FADE_MSEC, FX_GLASS_SETTLED_LIFETIME_MSEC,
    FX_GLASS_SHARD_LIFETIME_MSEC, FX_GLASS_SHARD_MAX, FX_GLASS_SPLIT_OP_CAP,
    FX_GLASS_STATE_AREA_X2, FX_GLASS_STATE_CRACK_DATA_COUNT, FX_GLASS_STATE_FAN_DATA_COUNT,
    FX_GLASS_STATE_FLAG_CHILD_CLEAR, FX_GLASS_STATE_FLAG_DAMAGED, FX_GLASS_STATE_FLAG_SIMPLE,
    FX_GLASS_STATE_HOLE_DATA_COUNT, FX_GLASS_STATE_VERT_COUNT, FX_GLASS_VERT_SCALE,
    FxGlassCrackRand, FxGlassCrackWork, FxGlassPieceGeo, FxGlassShard, glass_ballistic_origin,
    glass_clamp_to_piece, glass_cross3, glass_decode_geo, glass_dynamics_avel,
    glass_dynamics_fall_time, glass_dynamics_init_row, glass_dynamics_phys_obj,
    glass_dynamics_software_launch, glass_dynamics_vel, glass_extract_shards, glass_free_piece,
    glass_fringe_cap, glass_is_in_use, glass_launch_avel, glass_launch_dir, glass_lerp_range,
    glass_life_fade, glass_loop_area_x2, glass_needs_size_split, glass_normalize3,
    glass_piece_speed_scale, glass_piece_tex_vecs, glass_place_next_free, glass_place_origin,
    glass_place_quat, glass_place_radius, glass_place_set_origin, glass_place_set_quat,
    glass_point_in_piece, glass_recenter_offset, glass_reset_copy_geo, glass_reset_copy_piece,
    glass_reset_free_list, glass_set_in_use, glass_software_rotate_quat, glass_splitmix64,
    glass_state_area_x2, glass_state_def_index, glass_state_flags, glass_state_geo_span,
    glass_state_geo_start, glass_state_set_flags, glass_state_set_geo_start,
    glass_state_set_support_mask, glass_state_support_mask, glass_support_frac, unit_quat_to_axis,
};

/// The shortest crack the graph will cut, as a fraction of the piece's bounding radius.
const FX_GLASS_CRACK_LENGTH_MIN_FRAC: f32 = 0.75;

pub struct FxGlassInitTables<'a> {
    pub piece_limit: u32,
    pub geo_data_limit: u32,
    pub init_states: &'a [[u8; fx_iw4::FX_GLASS_INIT_PIECE_STATE]],
    pub init_geo: &'a [[u8; FX_GLASS_GEOMETRY_DATA]],
    pub defs: &'a [[u8; fx_iw4::FX_GLASS_DEF]],
}

#[derive(Clone, Debug, Default)]
pub struct FxGlassSystemHost {
    pub piece_limit: u32,
    pub init_piece_count: u32,
    pub geo_data_limit: u32,
    pub active_piece_count: u32,
    pub first_free_piece: u32,
    pub geo_data_count: u32,
    pub need_to_compact: bool,
    pub piece_places: Vec<[u8; FX_GLASS_PIECE_PLACE]>,
    pub piece_states: Vec<[u8; FX_GLASS_PIECE_STATE]>,
    pub piece_dynamics: Vec<[u8; FX_GLASS_PIECE_DYNAMICS]>,
    pub geo_data: Vec<[u8; FX_GLASS_GEOMETRY_DATA]>,
    pub is_in_use: Vec<u32>,
    pub half_thickness: Vec<f32>,
    pub link_org: Vec<[f32; 3]>,

    pub shatter_rand: u32,

    pub shatter_seed: u64,

    pub time: i32,
    pub prev_time: i32,

    pub moved: bool,

    pub motion_accum_msec: i32,
    pub motion_clock: Option<i32>,

    pub source_pane: Vec<u32>,
    pub generation: Vec<u32>,
    pub contact_mode: Vec<u8>,
    pub bounce_used: Vec<bool>,
    pub release_at: Vec<i32>,
    pub birth_at: Vec<i32>,
    pub pending_events: Vec<GlassPresentationEvent>,
    pub last_event_revision: Vec<u32>,
    pub defs: Vec<[u8; fx_iw4::FX_GLASS_DEF]>,
    last_landing_cell: Option<[i32; 3]>,
    last_landing_msec: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct GlassImpact {
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct GlassBreakPresentation {
    pub launch_airborne: bool,
    pub play_oneshot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassPresentationEvent {
    pub pane: u32,
    pub origin: [f32; 3],
    pub normal: [f32; 3],
    pub cause: u8,
    pub play_oneshot: bool,
    pub landing: bool,
    pub revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassWorldContact {
    pub fraction: f32,
    pub end: [f32; 3],
    pub normal: [f32; 3],
    pub startsolid: bool,
    pub pane: Option<u32>,
}

pub trait GlassWorldTrace {
    fn sweep(&self, start: [f32; 3], end: [f32; 3]) -> Option<GlassWorldContact>;
}

const CONTACT_NONE: u8 = 0;
const CONTACT_DISAPPEAR: u8 = 1;
const CONTACT_BOUNCE: u8 = 2;
const CONTACT_SETTLED: u8 = 3;

fn local_to_world(origin: [f32; 3], axis: [[f32; 3]; 3], vert: [i16; 2]) -> [f32; 3] {
    let x = vert[0] as f32 * FX_GLASS_VERT_SCALE;
    let y = vert[1] as f32 * FX_GLASS_VERT_SCALE;
    [
        origin[0] + axis[0][0] * x + axis[1][0] * y,
        origin[1] + axis[0][1] * x + axis[1][1] * y,
        origin[2] + axis[0][2] * x + axis[1][2] * y,
    ]
}

#[derive(Clone, Copy)]
enum ShardLaunch {
    Radial {
        dir: [f32; 3],
        hit: [f32; 3],
        radius: f32,
        axis: [[f32; 3]; 3],
    },
    Collapse {
        axis: [[f32; 3]; 3],
    },
}

impl FxGlassSystemHost {
    pub fn is_in_use(&self, piece: u32) -> bool {
        glass_is_in_use(&self.is_in_use, piece)
    }

    pub fn reset(&mut self, init: FxGlassInitTables<'_>) {
        *self = Self::default();
        if init.init_states.is_empty() || init.piece_limit == 0 {
            return;
        }
        let piece_limit = init.piece_limit as usize;
        let geo_limit = init.geo_data_limit as usize;
        let word_n = piece_limit.div_ceil(32);
        self.piece_limit = init.piece_limit;
        self.init_piece_count = init.init_states.len() as u32;
        self.geo_data_limit = init.geo_data_limit;
        self.piece_places = vec![[0u8; FX_GLASS_PIECE_PLACE]; piece_limit];
        self.piece_states = vec![[0u8; FX_GLASS_PIECE_STATE]; piece_limit];
        self.piece_dynamics = vec![[0u8; FX_GLASS_PIECE_DYNAMICS]; piece_limit];
        self.geo_data = vec![[0u8; FX_GLASS_GEOMETRY_DATA]; geo_limit.max(init.init_geo.len())];
        self.is_in_use = vec![0u32; word_n];
        self.half_thickness = vec![0.0; piece_limit];
        self.link_org = vec![[0.0; 3]; piece_limit];
        self.source_pane = vec![u32::MAX; piece_limit];
        self.generation = vec![0; piece_limit];
        self.contact_mode = vec![CONTACT_NONE; piece_limit];
        self.bounce_used = vec![false; piece_limit];
        self.release_at = vec![0; piece_limit];
        self.birth_at = vec![0; piece_limit];
        self.pending_events.clear();
        self.last_event_revision = vec![0; piece_limit];
        self.defs = init.defs.to_vec();
        self.last_landing_cell = None;
        self.last_landing_msec = 0;
        self.shatter_seed = 0x9E37_79B9_7F4A_7C15;
        self.motion_accum_msec = 0;
        self.motion_clock = None;

        let mut geo_cursor = 0u16;
        for (i, init_row) in init.init_states.iter().enumerate() {
            let piece = u16::try_from(i).unwrap_or(u16::MAX);
            let out = glass_reset_copy_piece(init_row, piece, geo_cursor, init.defs);
            geo_cursor = out.next_geo_start;
            if let Some(place) = self.piece_places.get_mut(i) {
                *place = out.place;
            }
            if let Some(state) = self.piece_states.get_mut(i) {
                *state = out.state;
            }
            if let Some(dyn_row) = self.piece_dynamics.get_mut(i) {
                glass_dynamics_init_row(dyn_row);
            }
            if let Some(thick) = self.half_thickness.get_mut(i) {
                *thick = out.half_thickness.unwrap_or(0.0);
            }
            glass_set_in_use(&mut self.is_in_use, i as u32);
            if let Some(src) = self.source_pane.get_mut(i) {
                *src = i as u32;
            }
        }

        let src: Vec<u8> = init.init_geo.iter().flatten().copied().collect();
        let mut dst = vec![0u8; src.len()];
        glass_reset_copy_geo(&mut dst, &src);
        for (i, chunk) in dst
            .as_chunks::<FX_GLASS_GEOMETRY_DATA>()
            .0
            .iter()
            .enumerate()
        {
            if let Some(word) = self.geo_data.get_mut(i) {
                word.copy_from_slice(chunk);
            }
        }

        self.first_free_piece = glass_reset_free_list(
            &mut self.piece_places,
            &mut self.link_org,
            self.init_piece_count,
            self.piece_limit,
        );
        self.active_piece_count = self.init_piece_count;
        self.geo_data_count = init.init_geo.len() as u32;
        self.need_to_compact = false;
    }

    pub fn alloc(&mut self, vert: u8, hole: u8, crack: u8, fan: u8) -> u32 {
        let piece = self.first_free_piece;
        if piece == FX_GLASS_FREE_SENTINEL {
            return FX_GLASS_FREE_SENTINEL;
        }
        let need = self.geo_data_count
            + u32::from(vert)
            + u32::from(hole)
            + u32::from(crack)
            + u32::from(fan);
        if self.geo_data_limit < need {
            return FX_GLASS_FREE_SENTINEL;
        }
        let Some(place) = self.piece_places.get_mut(piece as usize) else {
            return FX_GLASS_FREE_SENTINEL;
        };
        self.first_free_piece = glass_place_next_free(place);
        self.active_piece_count = self.active_piece_count.saturating_add(1);
        glass_set_in_use(&mut self.is_in_use, piece);
        let Some(state) = self.piece_states.get_mut(piece as usize) else {
            return FX_GLASS_FREE_SENTINEL;
        };
        state[FX_GLASS_STATE_VERT_COUNT] = vert;
        state[FX_GLASS_STATE_HOLE_DATA_COUNT] = hole;
        state[FX_GLASS_STATE_CRACK_DATA_COUNT] = crack;
        state[FX_GLASS_STATE_FAN_DATA_COUNT] = fan;
        glass_state_set_geo_start(state, self.geo_data_count as u16);
        glass_state_set_flags(state, 0);
        self.geo_data_count = need;
        piece
    }

    pub fn free(&mut self, piece: u32) {
        if glass_free_piece(
            &mut self.piece_places,
            &mut self.is_in_use,
            &mut self.first_free_piece,
            &mut self.active_piece_count,
            piece,
        ) {
            self.need_to_compact = true;
            self.moved = true;
            if let Some(slot) = self.generation.get_mut(piece as usize) {
                *slot = slot.wrapping_add(1);
            }
            if let Some(mode) = self.contact_mode.get_mut(piece as usize) {
                *mode = CONTACT_NONE;
            }
            if let Some(at) = self.release_at.get_mut(piece as usize) {
                *at = 0;
            }
            if let Some(src) = self.source_pane.get_mut(piece as usize) {
                *src = u32::MAX;
            }
            if let Some(birth) = self.birth_at.get_mut(piece as usize) {
                *birth = 0;
            }
            if let Some(used) = self.bounce_used.get_mut(piece as usize) {
                *used = false;
            }
            if let Some(org) = self.link_org.get_mut(piece as usize) {
                *org = [0.0; 3];
            }
        }
    }

    pub fn free_pane(&mut self, pane: u32) {
        let mut drop = Vec::new();
        for piece in 0..self.piece_limit {
            if !self.is_in_use(piece) {
                continue;
            }
            let src = self
                .source_pane
                .get(piece as usize)
                .copied()
                .unwrap_or(u32::MAX);
            if piece == pane || src == pane {
                drop.push(piece);
            }
        }
        for piece in drop {
            self.free(piece);
        }
    }

    pub fn take_events(&mut self) -> Vec<GlassPresentationEvent> {
        core::mem::take(&mut self.pending_events)
    }

    fn airborne_count(&self) -> u32 {
        (0..self.piece_limit)
            .filter(|p| self.is_airborne(*p as usize))
            .count() as u32
    }

    fn settled_count(&self) -> u32 {
        (0..self.piece_limit)
            .filter(|p| {
                self.is_in_use(*p)
                    && self.contact_mode.get(*p as usize).copied() == Some(CONTACT_SETTLED)
            })
            .count() as u32
    }

    fn is_airborne(&self, piece: usize) -> bool {
        if !self.is_in_use(piece as u32) {
            return false;
        }
        let Some(row) = self.piece_dynamics.get(piece) else {
            return false;
        };
        if glass_dynamics_fall_time(row) == FX_GLASS_FALL_TIME_NEVER {
            return false;
        }
        let mode = self
            .contact_mode
            .get(piece)
            .copied()
            .unwrap_or(CONTACT_NONE);
        mode == CONTACT_DISAPPEAR || mode == CONTACT_BOUNCE
    }

    pub fn piece_fade(&self, piece: usize) -> f32 {
        let mode = self
            .contact_mode
            .get(piece)
            .copied()
            .unwrap_or(CONTACT_NONE);
        if mode != CONTACT_SETTLED {
            return 1.0;
        }
        let Some(row) = self.piece_dynamics.get(piece) else {
            return 1.0;
        };
        let fall = glass_dynamics_fall_time(row);
        let age = self.time.wrapping_sub(fall);
        glass_life_fade(
            age,
            FX_GLASS_SETTLED_LIFETIME_MSEC,
            FX_GLASS_SETTLED_FADE_MSEC,
        )
    }

    fn hit_is_own_source(&self, piece: usize, hit: &GlassWorldContact) -> bool {
        let Some(pane) = hit.pane else {
            return false;
        };
        self.source_pane.get(piece).copied() == Some(pane)
    }

    fn next_rand(&mut self) -> f32 {
        glass_splitmix64(&mut self.shatter_seed)
    }

    pub fn compact_geo(&mut self) {
        if !self.need_to_compact {
            return;
        }
        let n = self.piece_limit as usize;
        let mut order: Vec<(u32, u16, u32)> = Vec::new();
        for piece in 0..n {
            if !self.is_in_use(piece as u32) {
                continue;
            }
            let Some(state) = self.piece_states.get(piece) else {
                continue;
            };
            order.push((
                piece as u32,
                glass_state_geo_start(state),
                glass_state_geo_span(state),
            ));
        }
        order.sort_by_key(|row| row.1);
        let mut dst = 0u32;
        for (piece, start, span) in order {
            if span == 0 {
                continue;
            }
            let src = u32::from(start);
            if src != dst {
                for k in 0..span {
                    let from = (src + k) as usize;
                    let to = (dst + k) as usize;
                    if let (Some(src_word), Some(dst_word)) =
                        (self.geo_data.get(from).copied(), self.geo_data.get_mut(to))
                    {
                        *dst_word = src_word;
                    }
                }
            }
            if let Some(state) = self.piece_states.get_mut(piece as usize) {
                glass_state_set_geo_start(state, dst as u16);
            }
            dst = dst.saturating_add(span);
        }
        self.geo_data_count = dst;
        self.need_to_compact = false;
    }

    /// One piece's stored geometry: the outer contour, its holes, its open cracks and
    /// the triangulation that spans them.
    fn piece_geo(&self, piece: u32) -> Option<([u8; FX_GLASS_PIECE_STATE], FxGlassPieceGeo)> {
        let state = self.piece_states.get(piece as usize).copied()?;
        let geo = glass_decode_geo(&state, &self.geo_data)?;
        Some((state, geo))
    }

    /// Runs the crack graph over one piece's contour and returns the shards it leaves.
    ///
    /// A planar half-edge graph with one circular chain per face, cracks walked with
    /// deflection and retry, then face extraction into shard geometry that keeps
    /// holes, open cracks and a fan.
    fn crack_piece(
        &mut self,
        outer: &[[i16; 2]],
        support: u32,
        impact: [f32; 2],
        out: &mut [FxGlassShard],
        retry_unsplit: bool,
    ) -> usize {
        let mut work = Box::new(FxGlassCrackWork::default());
        if !work.init_from_loop(outer, support) {
            return 0;
        }
        // The graph draws from the shared effect random table, seeded off the host
        // stream so a replayed shot re-cracks the same way.
        let seed = self.shatter_seed;
        let _ = self.next_rand();
        work.rand = FxGlassCrackRand::from_seed(seed);
        work.impact_pos = [
            impact[0] * FX_GLASS_VERT_SCALE,
            impact[1] * FX_GLASS_VERT_SCALE,
        ];
        let mut mins = [f32::INFINITY; 2];
        let mut maxs = [f32::NEG_INFINITY; 2];
        for v in outer {
            mins[0] = mins[0].min(f32::from(v[0]));
            mins[1] = mins[1].min(f32::from(v[1]));
            maxs[0] = maxs[0].max(f32::from(v[0]));
            maxs[1] = maxs[1].max(f32::from(v[1]));
        }
        let w = (maxs[0] - mins[0]) * FX_GLASS_VERT_SCALE;
        let h = (maxs[1] - mins[1]) * FX_GLASS_VERT_SCALE;
        let radius = 0.5 * (w * w + h * h).sqrt();
        work.original_radius = radius;
        work.crack_length_min = radius * FX_GLASS_CRACK_LENGTH_MIN_FRAC;
        work.crack_length_max = radius;
        // Bound unsuccessful attempts by contour complexity. A connected face
        // must get another chance to split before the fringe coverage policy runs.
        let attempts = if retry_unsplit {
            1 + (outer.len() * 2).max(10)
        } else {
            1
        };
        for attempt in 0..attempts {
            work.create_cracks();
            let n = glass_extract_shards(&work, out);
            if n != 1 || attempt + 1 == attempts {
                return n;
            }
            // An open crack can leave the entire face connected. Keep its graph
            // and try again from the border before applying the fringe area cap.
            let vertex = (self.next_rand() * outer.len() as f32) as usize;
            let p = outer[vertex.min(outer.len() - 1)];
            work.impact_pos = [
                f32::from(p[0]) * FX_GLASS_VERT_SCALE,
                f32::from(p[1]) * FX_GLASS_VERT_SCALE,
            ];
        }
        0
    }

    pub fn hit_segment(&mut self, start: [f32; 3], end: [f32; 3], seed: u64) -> usize {
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        if !start.iter().chain(end.iter()).all(|v| v.is_finite()) {
            return 0;
        }
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let mut hits = Vec::new();
        for piece in 0..self.piece_limit {
            if !self.is_in_use(piece) {
                continue;
            }
            let Some((state, pgeo)) = self.piece_geo(piece) else {
                continue;
            };
            if glass_state_flags(&state) & FX_GLASS_STATE_FLAG_SIMPLE == 0 {
                continue;
            }
            let place = self
                .draw_place(piece as usize)
                .unwrap_or(self.piece_places[piece as usize]);
            let origin = glass_place_origin(&place);
            let axis = unit_quat_to_axis(glass_place_quat(&place));
            let denominator = dot(delta, axis[2]);
            if denominator.abs() < 1e-6 {
                continue;
            }
            let offset = [
                origin[0] - start[0],
                origin[1] - start[1],
                origin[2] - start[2],
            ];
            let fraction = dot(offset, axis[2]) / denominator;
            if fraction <= 0.0 || fraction > 1.0 {
                continue;
            }
            let hit = [
                start[0] + delta[0] * fraction,
                start[1] + delta[1] * fraction,
                start[2] + delta[2] * fraction,
            ];
            let local = [hit[0] - origin[0], hit[1] - origin[1], hit[2] - origin[2]];
            if glass_point_in_piece(
                &pgeo,
                [
                    dot(local, axis[0]) / FX_GLASS_VERT_SCALE,
                    dot(local, axis[1]) / FX_GLASS_VERT_SCALE,
                ],
            ) {
                hits.push((fraction, piece, self.generation[piece as usize], hit, place));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut count = 0;
        // Capture hits before allocating descendants so one segment cannot hit its own fragments.
        for (_, piece, generation, hit, place) in hits.into_iter().take(5) {
            if !self.is_in_use(piece) || self.generation[piece as usize] != generation {
                continue;
            }
            self.piece_places[piece as usize] = place;
            if self.shatter_caused(
                piece,
                GlassImpact {
                    position: hit,
                    direction: delta,
                },
                Some(seed ^ u64::from(piece)),
                GlassBreakPresentation {
                    launch_airborne: true,
                    play_oneshot: true,
                },
                0,
                0,
            ) {
                count += 1;
            }
        }
        count
    }

    pub fn damage(&mut self, piece: u32) {
        if !self.is_in_use(piece) {
            return;
        }
        if let Some(state) = self.piece_states.get_mut(piece as usize) {
            glass_state_set_flags(
                state,
                glass_state_flags(state) | FX_GLASS_STATE_FLAG_DAMAGED,
            );
            self.moved = true;
        }
    }

    fn mark_shattered(&mut self, piece: u32) {
        if let Some(state) = self.piece_states.get_mut(piece as usize) {
            glass_state_set_flags(state, glass_state_flags(state) | FX_GLASS_STATE_FLAG_SIMPLE);
        }
    }

    pub fn shatter(&mut self, piece: u32, hit: [f32; 3], dir: [f32; 3]) -> bool {
        self.shatter_caused(
            piece,
            GlassImpact {
                position: hit,
                direction: dir,
            },
            None,
            GlassBreakPresentation {
                launch_airborne: true,
                play_oneshot: true,
            },
            0,
            1,
        )
    }

    pub fn shatter_caused(
        &mut self,
        piece: u32,
        impact: GlassImpact,
        seed: Option<u64>,
        presentation: GlassBreakPresentation,
        cause: u8,
        revision: u32,
    ) -> bool {
        let GlassImpact {
            position: hit,
            direction: dir,
        } = impact;
        let GlassBreakPresentation {
            launch_airborne,
            play_oneshot,
        } = presentation;
        if !self.is_in_use(piece) {
            return false;
        }
        self.compact_geo();
        let event_pane = self.source_pane[piece as usize];
        let idx = piece as usize;
        let Some(place) = self.piece_places.get(idx).copied() else {
            return false;
        };
        let Some((state, pgeo)) = self.piece_geo(piece) else {
            return false;
        };
        let simple = glass_state_flags(&state) & FX_GLASS_STATE_FLAG_SIMPLE != 0;
        let origin = glass_place_origin(&place);
        let axis = unit_quat_to_axis(glass_place_quat(&place));
        let delta = [hit[0] - origin[0], hit[1] - origin[1], hit[2] - origin[2]];
        let local = [
            (delta[0] * axis[0][0] + delta[1] * axis[0][1] + delta[2] * axis[0][2])
                / FX_GLASS_VERT_SCALE,
            (delta[0] * axis[1][0] + delta[1] * axis[1][1] + delta[2] * axis[1][2])
                / FX_GLASS_VERT_SCALE,
        ];
        let original_area = {
            let a = glass_state_area_x2(&state);
            if a > 0.0 {
                a
            } else {
                glass_loop_area_x2(pgeo.outer())
            }
        };
        let launch_dir = glass_launch_dir(dir, axis[2]);
        self.shatter_seed = seed.unwrap_or_else(|| {
            self.shatter_seed
                .wrapping_add(u64::from(piece) << 17)
                .wrapping_add(self.time as u32 as u64)
        });
        if self.shatter_seed == 0 {
            self.shatter_seed = 0x9E37_79B9_7F4A_7C15;
        }
        let mut work = vec![piece];
        let mut finals: Vec<u32> = Vec::new();
        let mut ops = 0u32;
        let mut first = true;
        while let Some(cur) = work.pop() {
            if ops >= FX_GLASS_SPLIT_OP_CAP {
                finals.push(cur);
                finals.append(&mut work);
                break;
            }
            let Some((cur_state, cur_geo)) = self.piece_geo(cur) else {
                continue;
            };
            let area = {
                let a = glass_state_area_x2(&cur_state);
                if a > 0.0 {
                    a
                } else {
                    glass_loop_area_x2(cur_geo.outer())
                }
            };
            let supported = glass_state_support_mask(&cur_state) != 0;
            // The graph is seeded from a single contour, so a shard that already
            // encloses a hole is left as it is rather than losing that hole to a
            // re-crack.
            let can_split = first || cur_geo.hole_n == 0;
            let must_split = can_split
                && (first || (!simple && glass_needs_size_split(area, original_area, supported)));
            let was_first = first;
            first = false;
            if !must_split {
                finals.push(cur);
                continue;
            }
            ops += 1;
            // The first pass cracks from the round the player fired; a later pass is a
            // piece too big to stand, which breaks about a point of its own.
            let want = if was_first {
                local
            } else {
                let outer = cur_geo.outer();
                let vertex = (self.next_rand() * outer.len() as f32) as usize;
                let p = outer[vertex.min(outer.len() - 1)];
                [f32::from(p[0]), f32::from(p[1])]
            };
            let split_impact = glass_clamp_to_piece(&cur_geo, want);
            let parent_support = glass_state_support_mask(&cur_state);
            let mut shards = vec![FxGlassShard::default(); FX_GLASS_SHARD_MAX];
            let shard_n = self.crack_piece(
                cur_geo.outer(),
                parent_support,
                split_impact,
                &mut shards,
                !simple && glass_needs_size_split(area, original_area, supported),
            );
            let Some(cur_place) = self.piece_places.get(cur as usize).copied() else {
                continue;
            };
            let mut spawned_this = Vec::new();
            for shard in &shards[..shard_n] {
                match self.emit_child(cur as usize, &cur_place, &cur_state, shard) {
                    Some(id) => spawned_this.push(id),
                    None => {
                        self.compact_geo();
                        if let Some(id) =
                            self.emit_child(cur as usize, &cur_place, &cur_state, shard)
                        {
                            spawned_this.push(id);
                        }
                    }
                }
            }
            if spawned_this.is_empty() {
                if simple {
                    return false;
                }
                if was_first || cur == piece {
                    self.free(cur);
                } else {
                    self.mark_shattered(cur);
                    finals.push(cur);
                }
                continue;
            }
            self.free(cur);
            for id in spawned_this {
                let Some(child_state) = self.piece_states.get(id as usize) else {
                    continue;
                };
                let child_area = glass_state_area_x2(child_state);
                let child_support = glass_state_support_mask(child_state) != 0;
                if !simple
                    && child_area + 1.0 < area
                    && glass_needs_size_split(child_area, original_area, child_support)
                {
                    work.push(id);
                } else {
                    finals.push(id);
                }
            }
        }
        if finals.is_empty() {
            if self.is_in_use(piece) {
                self.free(piece);
            }
            self.compact_geo();
            self.push_break_event(event_pane, origin, axis[2], play_oneshot, cause, revision);
            self.moved = true;
            return true;
        }
        self.prune_fringe_and_launch(
            &finals,
            if simple {
                f32::MAX
            } else {
                glass_fringe_cap(original_area)
            },
            if cause == 2 {
                ShardLaunch::Collapse { axis }
            } else {
                ShardLaunch::Radial {
                    dir: launch_dir,
                    hit,
                    radius: glass_place_radius(&place),
                    axis,
                }
            },
            launch_airborne,
        );
        self.compact_geo();
        self.push_break_event(event_pane, origin, axis[2], play_oneshot, cause, revision);
        self.moved = true;
        true
    }

    fn push_break_event(
        &mut self,
        pane: u32,
        origin: [f32; 3],
        normal: [f32; 3],
        play_oneshot: bool,
        cause: u8,
        revision: u32,
    ) {
        if revision != 0
            && self
                .last_event_revision
                .get(pane as usize)
                .copied()
                .unwrap_or(0)
                == revision
        {
            return;
        }
        if revision != 0
            && let Some(slot) = self.last_event_revision.get_mut(pane as usize)
        {
            *slot = revision;
        }
        self.pending_events.push(GlassPresentationEvent {
            pane,
            origin,
            normal,
            cause,
            play_oneshot,
            landing: false,
            revision,
        });
    }

    fn prune_fringe_and_launch(
        &mut self,
        children: &[u32],
        retained_area_limit: f32,
        launch: ShardLaunch,
        launch_airborne: bool,
    ) {
        let n = children.len();
        let mut areas = vec![0.0f32; n];
        let mut supports = vec![0u32; n];
        let mut retained = 0.0f32;
        for i in 0..n {
            let idx = children[i] as usize;
            let Some(state) = self.piece_states.get(idx) else {
                continue;
            };
            areas[i] = glass_state_area_x2(state);
            supports[i] = glass_state_support_mask(state);
            if supports[i] != 0 {
                retained += areas[i];
            }
        }
        let mut order: Vec<usize> = (0..n).filter(|&i| supports[i] != 0).collect();
        order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]));
        for &i in &order {
            if retained <= retained_area_limit {
                break;
            }
            if i >= n || supports[i] == 0 {
                continue;
            }
            retained -= areas[i];
            supports[i] = 0;
            if let Some(state) = self.piece_states.get_mut(children[i] as usize) {
                glass_state_set_support_mask(state, 0);
            }
        }
        let mut accents = 0u32;
        let mut launched = 0u32;
        for (i, support) in supports.iter().enumerate() {
            let idx = children[i] as usize;
            if *support != 0 {
                let verts = self
                    .piece_states
                    .get(idx)
                    .map(fx_iw4::glass_state_vert_count)
                    .unwrap_or(1);
                let frac = glass_support_frac(*support, verts);
                if frac < FX_GLASS_PENDING_SUPPORT_FRAC {
                    if !launch_airborne {
                        continue;
                    }
                    let t = self.next_rand();
                    let delay = FX_GLASS_PENDING_MIN_MSEC
                        + ((FX_GLASS_PENDING_MAX_MSEC - FX_GLASS_PENDING_MIN_MSEC) as f32 * t)
                            as i32;
                    if let Some(at) = self.release_at.get_mut(idx) {
                        *at = self.time.saturating_add(delay);
                    }
                }
                continue;
            }
            if !launch_airborne {
                self.free(children[i]);
                continue;
            }
            if launched >= FX_GLASS_AIRBORNE_PER_BREAK {
                self.free(children[i]);
                continue;
            }
            let bounce = accents < FX_GLASS_ACCENT_BOUNCE_CAP;
            if bounce {
                accents = accents.saturating_add(1);
            }
            self.launch_software_shard(children[i], areas[i], launch);
            if self.is_in_use(children[i]) {
                launched = launched.saturating_add(1);
            }
            if let Some(mode) = self.contact_mode.get_mut(idx) {
                *mode = if bounce {
                    CONTACT_BOUNCE
                } else {
                    CONTACT_DISAPPEAR
                };
            }
        }
    }

    fn launch_software_shard(&mut self, piece: u32, area_x2: f32, launch: ShardLaunch) {
        if self.airborne_count() >= FX_GLASS_AIRBORNE_CAP {
            self.free(piece);
            return;
        }
        let (collapse, dir, hit, radius, axis) = match launch {
            ShardLaunch::Radial {
                dir,
                hit,
                radius,
                axis,
            } => (false, dir, hit, radius, axis),
            ShardLaunch::Collapse { axis } => (true, [0.0, 0.0, -1.0], [0.0; 3], 0.0, axis),
        };
        let scale = glass_piece_speed_scale(area_x2);
        let lin_r = self.next_rand();
        let mut speed =
            glass_lerp_range(FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_LINEAR_VEL_MAX, lin_r) * scale;
        if collapse {
            speed *= 0.25;
        }
        let dir = if collapse {
            [0.0, 0.0, -1.0]
        } else {
            glass_launch_dir(dir, axis[2])
        };
        let mut vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];
        let origin = glass_place_origin(&self.piece_places[piece as usize]);
        let offset = [origin[0] - hit[0], origin[1] - hit[1], origin[2] - hit[2]];
        if !collapse && radius > 0.0 {
            let spread = speed * 0.5 / radius;
            for k in 0..3 {
                vel[k] += offset[k] * spread;
            }
        }
        let ang_r = self.next_rand();
        let ang =
            glass_lerp_range(FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_ANGULAR_VEL_MAX, ang_r) * scale;
        let avel = if !collapse && let Some(radial) = glass_normalize3(offset) {
            let side = if dir.iter().zip(axis[2]).map(|(a, b)| a * b).sum::<f32>() < 0.0 {
                -1.0
            } else {
                1.0
            };
            glass_cross3(axis[2], radial).map(|v| v * ang * side)
        } else {
            glass_launch_avel(dir, axis, ang)
        };
        if let Some(row) = self.piece_dynamics.get_mut(piece as usize) {
            glass_dynamics_software_launch(row, self.time, vel, avel);
        }
        if let Some(birth) = self.birth_at.get_mut(piece as usize) {
            *birth = self.time;
        }
        if let Some(mode) = self.contact_mode.get_mut(piece as usize)
            && *mode == CONTACT_NONE
        {
            *mode = CONTACT_DISAPPEAR;
        }
    }

    pub fn draw_place(&self, piece: usize) -> Option<[u8; FX_GLASS_PIECE_PLACE]> {
        let mut place = self.piece_places.get(piece).copied()?;
        let leftover = self.motion_accum_msec;
        if leftover <= 0 || !self.draw_extrapolates(piece) {
            return Some(place);
        }
        let dyn_row = self.piece_dynamics.get(piece)?;
        let fall = glass_dynamics_fall_time(dyn_row);
        let vel = glass_dynamics_vel(dyn_row);
        let avel = glass_dynamics_avel(dyn_row);
        let t0 = self
            .motion_clock
            .unwrap_or_else(|| self.time.wrapping_sub(leftover));
        let t1 = t0.wrapping_add(leftover);
        let origin = glass_place_origin(&place);
        let quat = glass_place_quat(&place);
        let next = glass_ballistic_origin(origin, vel, fall, t0, t1, FX_GLASS_FALL_GRAVITY);
        let q = glass_software_rotate_quat(quat, avel, leftover);
        glass_place_set_origin(&mut place, next);
        glass_place_set_quat(&mut place, q);
        Some(place)
    }

    fn draw_extrapolates(&self, piece: usize) -> bool {
        if !self.is_in_use(piece as u32) {
            return false;
        }
        let Some(dyn_row) = self.piece_dynamics.get(piece) else {
            return false;
        };
        if glass_dynamics_fall_time(dyn_row) == FX_GLASS_FALL_TIME_NEVER {
            return false;
        }
        if glass_dynamics_phys_obj(dyn_row) != 0 {
            return false;
        }
        let mode = self
            .contact_mode
            .get(piece)
            .copied()
            .unwrap_or(CONTACT_NONE);
        mode == CONTACT_DISAPPEAR || mode == CONTACT_BOUNCE
    }

    pub fn advance(&mut self, msec: i32) {
        self.advance_in_world(msec, None);
    }

    pub fn advance_in_world(&mut self, msec: i32, world: Option<&dyn GlassWorldTrace>) {
        if self.time == 0 && self.prev_time == 0 {
            self.time = msec;
            self.prev_time = msec;
            self.motion_clock = Some(msec);
            self.motion_accum_msec = 0;
            return;
        }
        self.prev_time = self.time;
        self.time = msec;
        if self.time == self.prev_time {
            return;
        }
        self.tilt_pending();
        self.release_pending();
        self.integrate_software(world);
    }

    fn tilt_pending(&mut self) {
        let dt = self.time.wrapping_sub(self.prev_time).max(0);
        if dt == 0 {
            return;
        }
        let n = self.piece_limit as usize;
        for piece in 0..n {
            let at = self.release_at.get(piece).copied().unwrap_or(0);
            if at == 0 || at <= self.time || !self.is_in_use(piece as u32) {
                continue;
            }
            let Some(state) = self.piece_states.get(piece) else {
                continue;
            };
            let mask = glass_state_support_mask(state);
            if mask.count_ones() != 1 {
                continue;
            }
            let bit = mask.leading_zeros() as usize;
            let Some(place) = self.piece_places.get(piece).copied() else {
                continue;
            };
            let axis = unit_quat_to_axis(glass_place_quat(&place));
            let axis_e = self
                .piece_geo(piece as u32)
                .and_then(|(_, pgeo)| {
                    let vert_n = pgeo.vert_n;
                    if vert_n < 2 || bit >= vert_n {
                        return None;
                    }
                    let a = pgeo.verts[bit];
                    let b = pgeo.verts[(bit + 1) % vert_n];
                    let origin = glass_place_origin(&place);
                    let wa = local_to_world(origin, axis, a);
                    let wb = local_to_world(origin, axis, b);
                    glass_normalize3([wb[0] - wa[0], wb[1] - wa[1], wb[2] - wa[2]])
                })
                .or_else(|| glass_normalize3(axis[0]))
                .unwrap_or([1.0, 0.0, 0.0]);
            let avel = [axis_e[0] * 1.2, axis_e[1] * 1.2, axis_e[2] * 1.2];
            let quat = glass_place_quat(&place);
            let q = glass_software_rotate_quat(quat, avel, dt);
            if let Some(place) = self.piece_places.get_mut(piece) {
                glass_place_set_quat(place, q);
            }
            self.moved = true;
        }
    }

    fn release_pending(&mut self) {
        let now = self.time;
        let n = self.piece_limit as usize;
        let mut launch = Vec::new();
        for piece in 0..n {
            let at = self.release_at.get(piece).copied().unwrap_or(0);
            if at == 0 || at > now || !self.is_in_use(piece as u32) {
                continue;
            }
            if let Some(state) = self.piece_states.get_mut(piece) {
                glass_state_set_support_mask(state, 0);
            }
            if let Some(at) = self.release_at.get_mut(piece) {
                *at = 0;
            }
            launch.push(piece as u32);
        }
        for piece in launch {
            let area = self
                .piece_states
                .get(piece as usize)
                .map(glass_state_area_x2)
                .unwrap_or(0.0);
            let axis = self
                .piece_places
                .get(piece as usize)
                .map(|p| unit_quat_to_axis(glass_place_quat(p)))
                .unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            self.launch_software_shard(piece, area, ShardLaunch::Collapse { axis });
            if let Some(mode) = self.contact_mode.get_mut(piece as usize) {
                *mode = CONTACT_DISAPPEAR;
            }
        }
    }

    fn integrate_software(&mut self, world: Option<&dyn GlassWorldTrace>) {
        let now = self.time;
        let total = now.wrapping_sub(self.prev_time).max(0);
        let cap = FX_GLASS_MOTION_STEP_MSEC * FX_GLASS_CATCHUP_STEPS;
        self.motion_accum_msec = self.motion_accum_msec.saturating_add(total).min(cap);
        let leftover = self.motion_accum_msec % FX_GLASS_MOTION_STEP_MSEC;
        let remain = self.motion_accum_msec - leftover;
        self.motion_accum_msec = leftover;
        let start = self.motion_clock.unwrap_or(self.prev_time);
        self.motion_clock = Some(start.wrapping_add(remain));
        let n = self.piece_limit as usize;
        let mut expired = Vec::new();
        for piece in 0..n {
            if !self.is_in_use(piece as u32) {
                continue;
            }
            let Some(dyn_row) = self.piece_dynamics.get(piece).copied() else {
                continue;
            };
            let mut fall = glass_dynamics_fall_time(&dyn_row);
            if fall == FX_GLASS_FALL_TIME_NEVER {
                continue;
            }
            if glass_dynamics_phys_obj(&dyn_row) != 0 {
                continue;
            }
            let mut mode = self
                .contact_mode
                .get(piece)
                .copied()
                .unwrap_or(CONTACT_DISAPPEAR);
            let life = if mode == CONTACT_SETTLED {
                FX_GLASS_SETTLED_LIFETIME_MSEC
            } else {
                FX_GLASS_SHARD_LIFETIME_MSEC
            };
            if now.wrapping_sub(fall) >= life {
                expired.push(piece as u32);
                continue;
            }
            if mode == CONTACT_SETTLED {
                let remain = life.saturating_sub(now.wrapping_sub(fall).max(0));
                if remain < FX_GLASS_SETTLED_FADE_MSEC {
                    self.moved = true;
                }
                continue;
            }
            let mut vel = glass_dynamics_vel(&dyn_row);
            let mut avel = glass_dynamics_avel(&dyn_row);
            let mut t0 = start;
            let mut left = remain;
            let mut killed = false;
            while left > 0 {
                let step_ms = FX_GLASS_MOTION_STEP_MSEC;
                let t1 = t0 + step_ms;
                let Some(place) = self.piece_places.get(piece).copied() else {
                    break;
                };
                let origin = glass_place_origin(&place);
                let quat = glass_place_quat(&place);
                let next = glass_ballistic_origin(origin, vel, fall, t0, t1, FX_GLASS_FALL_GRAVITY);
                let q = glass_software_rotate_quat(quat, avel, t1.wrapping_sub(t0));
                if let Some(hit) =
                    self.sweep_piece(world, piece as u32, origin, next, mode == CONTACT_BOUNCE)
                {
                    if hit.startsolid {
                        if self.hit_is_own_source(piece, &hit) {
                            // spawn inside the pane that just opened
                        } else {
                            expired.push(piece as u32);
                            killed = true;
                            break;
                        }
                    } else {
                        match mode {
                            CONTACT_BOUNCE => {
                                if hit.normal[2] > 0.7 {
                                    self.note_landing(hit.end, hit.normal);
                                }
                                if self.bounce_used.get(piece).copied().unwrap_or(false) {
                                    if hit.normal[2] > 0.7 {
                                        self.note_landing(hit.end, hit.normal);
                                        if self.settled_count() >= FX_GLASS_SETTLED_CAP {
                                            expired.push(piece as u32);
                                        } else {
                                            if let Some(place) = self.piece_places.get_mut(piece) {
                                                glass_place_set_origin(place, hit.end);
                                            }
                                            if let Some(m) = self.contact_mode.get_mut(piece) {
                                                *m = CONTACT_SETTLED;
                                            }
                                            if let Some(row) = self.piece_dynamics.get_mut(piece) {
                                                glass_dynamics_software_launch(
                                                    row,
                                                    now,
                                                    [0.0, 0.0, 0.0],
                                                    [0.0, 0.0, 0.0],
                                                );
                                            }
                                        }
                                    } else {
                                        self.note_landing(hit.end, hit.normal);
                                        expired.push(piece as u32);
                                    }
                                    killed = true;
                                    break;
                                }
                                let incoming = [
                                    next[0] - origin[0],
                                    next[1] - origin[1],
                                    next[2] - origin[2],
                                ];
                                let dt = (t1 - t0).max(1) as f32 / 1000.0;
                                let v = [incoming[0] / dt, incoming[1] / dt, incoming[2] / dt];
                                let vn = v[0] * hit.normal[0]
                                    + v[1] * hit.normal[1]
                                    + v[2] * hit.normal[2];
                                let bounced = [
                                    (v[0] - 2.0 * vn * hit.normal[0]) * FX_GLASS_RESTITUTION,
                                    (v[1] - 2.0 * vn * hit.normal[1]) * FX_GLASS_RESTITUTION,
                                    (v[2] - 2.0 * vn * hit.normal[2]) * FX_GLASS_RESTITUTION,
                                ];
                                if let Some(row) = self.piece_dynamics.get_mut(piece) {
                                    glass_dynamics_software_launch(
                                        row,
                                        t1,
                                        bounced,
                                        [avel[0] * 0.3, avel[1] * 0.3, avel[2] * 0.3],
                                    );
                                }
                                if let Some(place) = self.piece_places.get_mut(piece) {
                                    glass_place_set_origin(place, hit.end);
                                    glass_place_set_quat(place, q);
                                }
                                if let Some(used) = self.bounce_used.get_mut(piece) {
                                    *used = true;
                                }
                                if let Some(row) = self.piece_dynamics.get(piece) {
                                    vel = glass_dynamics_vel(row);
                                    avel = glass_dynamics_avel(row);
                                    fall = glass_dynamics_fall_time(row);
                                }
                                mode = self
                                    .contact_mode
                                    .get(piece)
                                    .copied()
                                    .unwrap_or(CONTACT_BOUNCE);
                                self.moved = true;
                                t0 = t1;
                                left -= step_ms;
                                continue;
                            }
                            _ => {
                                self.note_landing(hit.end, hit.normal);
                                expired.push(piece as u32);
                                killed = true;
                                break;
                            }
                        }
                    }
                }
                if let Some(place) = self.piece_places.get_mut(piece) {
                    glass_place_set_origin(place, next);
                    glass_place_set_quat(place, q);
                }
                t0 = t1;
                left -= step_ms;
                self.moved = true;
            }
            if killed {
                self.moved = true;
            }
        }
        if leftover > 0 {
            for piece in 0..n {
                if self.draw_extrapolates(piece) {
                    self.moved = true;
                    break;
                }
            }
        }
        for piece in expired {
            self.free(piece);
            self.moved = true;
        }
        if self.need_to_compact {
            self.compact_geo();
        }
    }

    fn sweep_piece(
        &self,
        world: Option<&dyn GlassWorldTrace>,
        piece: u32,
        origin: [f32; 3],
        next: [f32; 3],
        accent: bool,
    ) -> Option<GlassWorldContact> {
        let world = world?;
        if let Some(hit) = world.sweep(origin, next) {
            return Some(hit);
        }
        if !accent {
            return None;
        }
        let (_, pgeo) = self.piece_geo(piece)?;
        let vert_n = pgeo.vert_n;
        let place = self.piece_places.get(piece as usize)?;
        let axis = unit_quat_to_axis(glass_place_quat(place));
        let delta = [
            next[0] - origin[0],
            next[1] - origin[1],
            next[2] - origin[2],
        ];
        let samples = vert_n.min(4);
        if samples == 0 {
            return None;
        }
        for k in 0..samples {
            let i = k * vert_n / samples;
            let p = local_to_world(origin, axis, pgeo.verts[i]);
            let q = [p[0] + delta[0], p[1] + delta[1], p[2] + delta[2]];
            if let Some(hit) = world.sweep(p, q) {
                return Some(hit);
            }
        }
        None
    }

    fn note_landing(&mut self, origin: [f32; 3], normal: [f32; 3]) {
        let cell = [
            (origin[0] / FX_GLASS_LANDING_CELL) as i32,
            (origin[1] / FX_GLASS_LANDING_CELL) as i32,
            (origin[2] / FX_GLASS_LANDING_CELL) as i32,
        ];
        if self.last_landing_cell == Some(cell)
            && self.time.saturating_sub(self.last_landing_msec) < FX_GLASS_LANDING_AGGREGATE_MSEC
        {
            return;
        }
        self.last_landing_cell = Some(cell);
        self.last_landing_msec = self.time;
        self.pending_events.push(GlassPresentationEvent {
            pane: 0,
            origin,
            normal,
            cause: 0,
            play_oneshot: true,
            landing: true,
            revision: 0,
        });
    }

    fn emit_child(
        &mut self,
        parent: usize,
        parent_place: &[u8; FX_GLASS_PIECE_PLACE],
        parent_state: &[u8; FX_GLASS_PIECE_STATE],
        shard: &FxGlassShard,
    ) -> Option<u32> {
        let vert = shard.vert_count;
        if vert < 3 {
            return None;
        }
        let child = self.alloc(
            vert,
            shard.hole_data_count,
            shard.crack_data_count,
            shard.fan_data_count,
        );
        if child == FX_GLASS_FREE_SENTINEL {
            return None;
        }
        let cidx = child as usize;
        if let Some(place) = self.piece_places.get_mut(cidx) {
            *place = *parent_place;
        }
        let geo_start = self
            .piece_states
            .get(cidx)
            .map(glass_state_geo_start)
            .unwrap_or(0);
        if let Some(state) = self.piece_states.get_mut(cidx) {
            state[..0x0c].copy_from_slice(&parent_state[..0x0c]);
            state[0x0c..0x0e].copy_from_slice(&parent_state[0x0c..0x0e]);
            state[0x10] = parent_state[0x10];
            glass_state_set_support_mask(state, shard.support_mask);
            let flags = (glass_state_flags(parent_state) & !FX_GLASS_STATE_FLAG_CHILD_CLEAR)
                | FX_GLASS_STATE_FLAG_SIMPLE;
            glass_state_set_flags(state, flags);
            // The graph works in packed grid units and already netted each shard's
            // holes out of its area; piece state carries it in world units.
            let area = shard.area_x2 * FX_GLASS_VERT_SCALE * FX_GLASS_VERT_SCALE;
            let bytes = area.to_le_bytes();
            state[FX_GLASS_STATE_AREA_X2..FX_GLASS_STATE_AREA_X2 + 4].copy_from_slice(&bytes);
            state[fx_iw4::FX_GLASS_STATE_VERT_COUNT] = vert;
            state[fx_iw4::FX_GLASS_STATE_HOLE_DATA_COUNT] = shard.hole_data_count;
            state[fx_iw4::FX_GLASS_STATE_CRACK_DATA_COUNT] = shard.crack_data_count;
            state[fx_iw4::FX_GLASS_STATE_FAN_DATA_COUNT] = shard.fan_data_count;
            let gs = geo_start.to_le_bytes();
            state[fx_iw4::FX_GLASS_STATE_GEO_DATA_START] = gs[0];
            state[fx_iw4::FX_GLASS_STATE_GEO_DATA_START + 1] = gs[1];
        }
        // Piece geometry is stored about the piece origin, so the shard moves onto its
        // own centroid and the origin and texture offset follow it.
        let mut shard = *shard;
        let c = [shard.centroid[0].round(), shard.centroid[1].round()];
        shard.recenter([c[0] as i16, c[1] as i16]);
        let parent_origin = glass_place_origin(parent_place);
        let axis = unit_quat_to_axis(glass_place_quat(parent_place));
        let uv0 = [
            f32::from_le_bytes([
                parent_state[0],
                parent_state[1],
                parent_state[2],
                parent_state[3],
            ]),
            f32::from_le_bytes([
                parent_state[4],
                parent_state[5],
                parent_state[6],
                parent_state[7],
            ]),
        ];
        let def_i = usize::from(glass_state_def_index(parent_state));
        let tex = self
            .defs
            .get(def_i)
            .map(|def| glass_piece_tex_vecs(def, glass_state_flags(parent_state)))
            .unwrap_or([[0.0, 0.0], [0.0, 0.0]]);
        let (new_origin, new_uv) = glass_recenter_offset(c, parent_origin, axis, uv0, tex);
        if let Some(place) = self.piece_places.get_mut(cidx) {
            glass_place_set_origin(place, new_origin);
        }
        if let Some(state) = self.piece_states.get_mut(cidx) {
            state[0..4].copy_from_slice(&new_uv[0].to_le_bytes());
            state[4..8].copy_from_slice(&new_uv[1].to_le_bytes());
        }
        let start = usize::from(geo_start);
        for (i, word) in shard.geo().iter().enumerate() {
            if let Some(dst) = self.geo_data.get_mut(start + i) {
                *dst = *word;
            }
        }
        let parent_dyn = self
            .piece_dynamics
            .get(parent)
            .copied()
            .unwrap_or([0u8; FX_GLASS_PIECE_DYNAMICS]);
        let parent_thick = self.half_thickness.get(parent).copied().unwrap_or(0.0);
        if let Some(dyn_row) = self.piece_dynamics.get_mut(cidx) {
            *dyn_row = parent_dyn;
        }
        if let Some(thick) = self.half_thickness.get_mut(cidx) {
            *thick = parent_thick;
        }
        let parent_pane = self
            .source_pane
            .get(parent)
            .copied()
            .unwrap_or(parent as u32);
        if let Some(src) = self.source_pane.get_mut(cidx) {
            *src = parent_pane;
        }
        Some(child)
    }
}
