use fx_iw4::{
    FX_GLASS_ACCENT_BOUNCE_CAP, FX_GLASS_AIRBORNE_CAP, FX_GLASS_AIRBORNE_PER_BREAK,
    FX_GLASS_ANGULAR_VEL_MAX, FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_CATCHUP_STEPS,
    FX_GLASS_FALL_GRAVITY, FX_GLASS_FALL_TIME_NEVER, FX_GLASS_FREE_SENTINEL,
    FX_GLASS_FRINGE_MAX_PIECES, FX_GLASS_GEOMETRY_DATA, FX_GLASS_LANDING_AGGREGATE_MSEC,
    FX_GLASS_LANDING_CELL, FX_GLASS_LINEAR_VEL_MAX, FX_GLASS_LINEAR_VEL_MIN,
    FX_GLASS_MOTION_STEP_MSEC, FX_GLASS_PENDING_MAX_MSEC, FX_GLASS_PENDING_MIN_MSEC,
    FX_GLASS_PENDING_SUPPORT_FRAC, FX_GLASS_PIECE_DYNAMICS, FX_GLASS_PIECE_PLACE,
    FX_GLASS_PIECE_STATE, FX_GLASS_RESTITUTION, FX_GLASS_SETTLED_CAP, FX_GLASS_SETTLED_FADE_MSEC,
    FX_GLASS_SETTLED_LIFETIME_MSEC, FX_GLASS_SHARD_LIFETIME_MSEC, FX_GLASS_SHATTER_TWO_PI,
    FX_GLASS_SPLIT_MAX_CHILDREN, FX_GLASS_SPLIT_OP_CAP, FX_GLASS_STATE_AREA_X2,
    FX_GLASS_STATE_FLAG_CHILD_CLEAR, FX_GLASS_STATE_FLAG_SHATTERED, FX_GLASS_VERT_SCALE,
    FxGlassRadialSplit, fx_glass_alloc_piece, fx_glass_ballistic_origin, fx_glass_centroid,
    fx_glass_child_support, fx_glass_chord_split, fx_glass_clamp_impact, fx_glass_def_tex_vecs,
    fx_glass_dynamics_avel, fx_glass_dynamics_fall_time, fx_glass_dynamics_init_row,
    fx_glass_dynamics_phys_obj, fx_glass_dynamics_software_launch, fx_glass_dynamics_vel,
    fx_glass_free_piece, fx_glass_fringe_cap, fx_glass_fringe_prune_knock_order, fx_glass_geo_vert,
    fx_glass_interior_branch_count, fx_glass_is_in_use, fx_glass_launch_avel, fx_glass_launch_dir,
    fx_glass_lerp_range, fx_glass_life_fade, fx_glass_loop_area_x2, fx_glass_needs_size_split,
    fx_glass_normalize3, fx_glass_pack_geo_vert, fx_glass_piece_speed_scale, fx_glass_place_origin,
    fx_glass_place_quat, fx_glass_place_set_origin, fx_glass_place_set_quat, fx_glass_radial_split,
    fx_glass_recenter_loop, fx_glass_reset_copy_geo, fx_glass_reset_copy_piece,
    fx_glass_reset_free_list, fx_glass_set_in_use, fx_glass_software_rotate_quat,
    fx_glass_splitmix64, fx_glass_state_area_x2, fx_glass_state_def_index, fx_glass_state_flags,
    fx_glass_state_geo_span, fx_glass_state_geo_start, fx_glass_state_set_flags,
    fx_glass_state_set_geo_start, fx_glass_state_set_support_mask, fx_glass_state_support_mask,
    fx_glass_state_vert_count, fx_glass_support_frac, fx_unit_quat_to_axis,
};

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

impl FxGlassSystemHost {
    pub fn is_in_use(&self, piece: u32) -> bool {
        fx_glass_is_in_use(&self.is_in_use, piece)
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
            let out = fx_glass_reset_copy_piece(init_row, piece, geo_cursor, init.defs);
            geo_cursor = out.next_geo_start;
            if let Some(place) = self.piece_places.get_mut(i) {
                *place = out.place;
            }
            if let Some(state) = self.piece_states.get_mut(i) {
                *state = out.state;
            }
            if let Some(dyn_row) = self.piece_dynamics.get_mut(i) {
                fx_glass_dynamics_init_row(dyn_row);
            }
            if let Some(thick) = self.half_thickness.get_mut(i) {
                *thick = out.half_thickness.unwrap_or(0.0);
            }
            fx_glass_set_in_use(&mut self.is_in_use, i as u32);
            if let Some(src) = self.source_pane.get_mut(i) {
                *src = i as u32;
            }
        }

        let src: Vec<u8> = init.init_geo.iter().flatten().copied().collect();
        let mut dst = vec![0u8; src.len()];
        fx_glass_reset_copy_geo(&mut dst, &src);
        for (i, chunk) in dst.chunks_exact(FX_GLASS_GEOMETRY_DATA).enumerate() {
            if let Some(word) = self.geo_data.get_mut(i) {
                word.copy_from_slice(chunk);
            }
        }

        self.first_free_piece = fx_glass_reset_free_list(
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
        fx_glass_alloc_piece(
            &mut self.piece_places,
            &mut self.piece_states,
            &mut self.is_in_use,
            &mut self.first_free_piece,
            &mut self.active_piece_count,
            &mut self.geo_data_count,
            self.geo_data_limit,
            vert,
            hole,
            crack,
            fan,
        )
    }

    pub fn free(&mut self, piece: u32) {
        if fx_glass_free_piece(
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
        if fx_glass_dynamics_fall_time(row) == FX_GLASS_FALL_TIME_NEVER {
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
        let fall = fx_glass_dynamics_fall_time(row);
        let age = self.time.wrapping_sub(fall);
        fx_glass_life_fade(
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
        fx_glass_splitmix64(&mut self.shatter_seed)
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
                fx_glass_state_geo_start(state),
                fx_glass_state_geo_span(state),
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
                fx_glass_state_set_geo_start(state, dst as u16);
            }
            dst = dst.saturating_add(span);
        }
        self.geo_data_count = dst;
        self.need_to_compact = false;
    }

    fn piece_outline(
        &self,
        piece: u32,
    ) -> Option<([u8; FX_GLASS_PIECE_STATE], [[i16; 2]; 32], usize)> {
        let state = self.piece_states.get(piece as usize).copied()?;
        let vert_n = usize::from(fx_glass_state_vert_count(&state));
        let start = usize::from(fx_glass_state_geo_start(&state));
        if vert_n < 3 || vert_n > 32 {
            return None;
        }
        let end = start.saturating_add(vert_n);
        let geo = self.geo_data.get(start..end)?;
        let mut outline = [[0i16; 2]; 32];
        for (dst, word) in outline.iter_mut().take(vert_n).zip(geo.iter()) {
            *dst = fx_glass_geo_vert(word);
        }
        Some((state, outline, vert_n))
    }

    fn mark_shattered(&mut self, piece: u32) {
        if let Some(state) = self.piece_states.get_mut(piece as usize) {
            fx_glass_state_set_flags(
                state,
                fx_glass_state_flags(state) | FX_GLASS_STATE_FLAG_SHATTERED,
            );
        }
    }

    pub fn shatter(&mut self, piece: u32, hit: [f32; 3], dir: [f32; 3]) -> bool {
        self.shatter_caused(piece, hit, dir, None, true, true, 0, 1)
    }

    pub fn shatter_caused(
        &mut self,
        piece: u32,
        hit: [f32; 3],
        dir: [f32; 3],
        seed: Option<u64>,
        launch_airborne: bool,
        play_oneshot: bool,
        cause: u8,
        revision: u32,
    ) -> bool {
        if !self.is_in_use(piece) {
            return false;
        }
        self.compact_geo();
        let idx = piece as usize;
        let Some(place) = self.piece_places.get(idx).copied() else {
            return false;
        };
        let Some((state, outline, vert_n)) = self.piece_outline(piece) else {
            return false;
        };
        let origin = fx_glass_place_origin(&place);
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(&place));
        let delta = [hit[0] - origin[0], hit[1] - origin[1], hit[2] - origin[2]];
        let local = [
            (delta[0] * axis[0][0] + delta[1] * axis[0][1] + delta[2] * axis[0][2])
                / FX_GLASS_VERT_SCALE,
            (delta[0] * axis[1][0] + delta[1] * axis[1][1] + delta[2] * axis[1][2])
                / FX_GLASS_VERT_SCALE,
        ];
        let original_area = {
            let a = fx_glass_state_area_x2(&state);
            if a > 0.0 {
                a
            } else {
                fx_glass_loop_area_x2(&outline[..vert_n])
            }
        };
        let launch_dir = fx_glass_launch_dir(dir, axis[2]);
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
                finals.extend(work.drain(..));
                break;
            }
            let Some((cur_state, cur_outline, cur_n)) = self.piece_outline(cur) else {
                continue;
            };
            let area = {
                let a = fx_glass_state_area_x2(&cur_state);
                if a > 0.0 {
                    a
                } else {
                    fx_glass_loop_area_x2(&cur_outline[..cur_n])
                }
            };
            let supported = fx_glass_state_support_mask(&cur_state) != 0;
            let must_split = first || fx_glass_needs_size_split(area, original_area, supported);
            let was_first = first;
            first = false;
            if !must_split {
                finals.push(cur);
                continue;
            }
            ops += 1;
            let loops: Vec<fx_iw4::FxGlassSplitLoop> = if was_first {
                let split_impact = fx_glass_clamp_impact(&cur_outline[..cur_n], local);
                let rand01 = self.next_rand();
                let branch_n = fx_glass_interior_branch_count(rand01);
                let angle0 = self.next_rand() * FX_GLASS_SHATTER_TWO_PI;
                let mut ray_jitter = [0.5f32; FX_GLASS_SPLIT_MAX_CHILDREN];
                for jitter in ray_jitter.iter_mut().take(branch_n as usize) {
                    *jitter = self.next_rand();
                }
                fx_glass_radial_split(
                    &cur_outline[..cur_n],
                    FxGlassRadialSplit {
                        impact: split_impact,
                        branch_n,
                        angle0,
                        ray_jitter,
                    },
                )
                .into_iter()
                .filter(|l| l.vert_n >= 3)
                .collect()
            } else {
                let along_x = self.next_rand() >= 0.5;
                fx_glass_chord_split(&cur_outline[..cur_n], self.next_rand(), along_x)
                    .into_iter()
                    .filter(|l| l.vert_n >= 3)
                    .collect()
            };
            let parent_support = fx_glass_state_support_mask(&cur_state);
            let Some(cur_place) = self.piece_places.get(cur as usize).copied() else {
                continue;
            };
            let mut spawned_this = Vec::new();
            for child in &loops {
                let support = fx_glass_child_support(
                    &cur_outline[..cur_n],
                    parent_support,
                    &child.verts[..child.vert_n as usize],
                );
                match self.emit_child(cur as usize, &cur_place, &cur_state, child, support) {
                    Some(id) => spawned_this.push(id),
                    None => {
                        self.compact_geo();
                        if let Some(id) =
                            self.emit_child(cur as usize, &cur_place, &cur_state, child, support)
                        {
                            spawned_this.push(id);
                        }
                    }
                }
            }
            if spawned_this.is_empty() {
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
                let child_area = fx_glass_state_area_x2(child_state);
                let child_support = fx_glass_state_support_mask(child_state) != 0;
                if child_area + 1.0 < area
                    && fx_glass_needs_size_split(child_area, original_area, child_support)
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
            self.push_break_event(piece, origin, axis[2], play_oneshot, cause, revision);
            self.moved = true;
            return true;
        }
        self.prune_fringe_and_launch(
            &finals,
            original_area,
            launch_dir,
            axis,
            launch_airborne,
            cause,
        );
        self.compact_geo();
        self.push_break_event(piece, origin, axis[2], play_oneshot, cause, revision);
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
        if revision != 0 {
            if let Some(slot) = self.last_event_revision.get_mut(pane as usize) {
                *slot = revision;
            }
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
        original_area: f32,
        dir: [f32; 3],
        axis: [[f32; 3]; 3],
        launch_airborne: bool,
        cause: u8,
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
            areas[i] = fx_glass_state_area_x2(state);
            supports[i] = fx_glass_state_support_mask(state);
            if supports[i] != 0 {
                retained += areas[i];
            }
        }
        let cap = fx_glass_fringe_cap(original_area);
        let order = fx_glass_fringe_prune_knock_order(&areas, &supports);
        for slot in order.iter().take(n) {
            if retained <= cap {
                break;
            }
            let i = *slot as usize;
            if i >= n || supports[i] == 0 {
                continue;
            }
            retained -= areas[i];
            supports[i] = 0;
            if let Some(state) = self.piece_states.get_mut(children[i] as usize) {
                fx_glass_state_set_support_mask(state, 0);
            }
        }
        let mut kept = 0u32;
        for i in 0..n {
            if supports[i] == 0 {
                continue;
            }
            kept = kept.saturating_add(1);
            if kept as usize > FX_GLASS_FRINGE_MAX_PIECES {
                supports[i] = 0;
                if let Some(state) = self.piece_states.get_mut(children[i] as usize) {
                    fx_glass_state_set_support_mask(state, 0);
                }
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
                    .map(|s| fx_glass_state_vert_count(s))
                    .unwrap_or(1);
                let frac = fx_glass_support_frac(*support, verts);
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
            self.launch_software_shard(children[i], areas[i], dir, axis, cause);
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

    fn launch_software_shard(
        &mut self,
        piece: u32,
        area_x2: f32,
        dir: [f32; 3],
        axis: [[f32; 3]; 3],
        cause: u8,
    ) {
        if self.airborne_count() >= FX_GLASS_AIRBORNE_CAP {
            self.free(piece);
            return;
        }
        let collapse = cause == 2;
        let scale = fx_glass_piece_speed_scale(area_x2);
        let lin_r = self.next_rand();
        let mut speed =
            fx_glass_lerp_range(FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_LINEAR_VEL_MAX, lin_r) * scale;
        if collapse {
            speed *= 0.25;
        }
        let dir = if collapse {
            [0.0, 0.0, -1.0]
        } else {
            fx_glass_launch_dir(dir, axis[2])
        };
        let mut vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];
        if !collapse {
            if let Some((_, outline, vert_n)) = self.piece_outline(piece) {
                let c = fx_glass_centroid(&outline[..vert_n]);
                let push = [
                    axis[0][0] * c[0] + axis[1][0] * c[1],
                    axis[0][1] * c[0] + axis[1][1] * c[1],
                    axis[0][2] * c[0] + axis[1][2] * c[1],
                ];
                if let Some(p) = fx_glass_normalize3(push) {
                    let extra = speed * 0.15;
                    vel[0] += p[0] * extra;
                    vel[1] += p[1] * extra;
                    vel[2] += p[2] * extra;
                }
            }
        }
        let ang_r = self.next_rand();
        let ang =
            fx_glass_lerp_range(FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_ANGULAR_VEL_MAX, ang_r) * scale;
        let avel = fx_glass_launch_avel(dir, axis, ang);
        if let Some(row) = self.piece_dynamics.get_mut(piece as usize) {
            fx_glass_dynamics_software_launch(row, self.time, vel, avel);
        }
        if let Some(birth) = self.birth_at.get_mut(piece as usize) {
            *birth = self.time;
        }
        if let Some(mode) = self.contact_mode.get_mut(piece as usize) {
            if *mode == CONTACT_NONE {
                *mode = CONTACT_DISAPPEAR;
            }
        }
    }

    pub fn draw_place(&self, piece: usize) -> Option<[u8; FX_GLASS_PIECE_PLACE]> {
        let mut place = self.piece_places.get(piece).copied()?;
        let leftover = self.motion_accum_msec;
        if leftover <= 0 || !self.draw_extrapolates(piece) {
            return Some(place);
        }
        let dyn_row = self.piece_dynamics.get(piece)?;
        let fall = fx_glass_dynamics_fall_time(dyn_row);
        let vel = fx_glass_dynamics_vel(dyn_row);
        let avel = fx_glass_dynamics_avel(dyn_row);
        let t0 = self
            .motion_clock
            .unwrap_or_else(|| self.time.wrapping_sub(leftover));
        let t1 = t0.wrapping_add(leftover);
        let origin = fx_glass_place_origin(&place);
        let quat = fx_glass_place_quat(&place);
        let next = fx_glass_ballistic_origin(origin, vel, fall, t0, t1, FX_GLASS_FALL_GRAVITY);
        let q = fx_glass_software_rotate_quat(quat, avel, leftover);
        fx_glass_place_set_origin(&mut place, next);
        fx_glass_place_set_quat(&mut place, q);
        Some(place)
    }

    fn draw_extrapolates(&self, piece: usize) -> bool {
        if !self.is_in_use(piece as u32) {
            return false;
        }
        let Some(dyn_row) = self.piece_dynamics.get(piece) else {
            return false;
        };
        if fx_glass_dynamics_fall_time(dyn_row) == FX_GLASS_FALL_TIME_NEVER {
            return false;
        }
        if fx_glass_dynamics_phys_obj(dyn_row) != 0 {
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
            let mask = fx_glass_state_support_mask(state);
            if mask.count_ones() != 1 {
                continue;
            }
            let bit = mask.trailing_zeros() as usize;
            let Some(place) = self.piece_places.get(piece).copied() else {
                continue;
            };
            let axis = fx_unit_quat_to_axis(fx_glass_place_quat(&place));
            let axis_e = self
                .piece_outline(piece as u32)
                .and_then(|(_, outline, vert_n)| {
                    if vert_n < 2 || bit >= vert_n {
                        return None;
                    }
                    let a = outline[bit];
                    let b = outline[(bit + 1) % vert_n];
                    let origin = fx_glass_place_origin(&place);
                    let wa = local_to_world(origin, axis, a);
                    let wb = local_to_world(origin, axis, b);
                    fx_glass_normalize3([wb[0] - wa[0], wb[1] - wa[1], wb[2] - wa[2]])
                })
                .or_else(|| fx_glass_normalize3(axis[0]))
                .unwrap_or([1.0, 0.0, 0.0]);
            let avel = [axis_e[0] * 1.2, axis_e[1] * 1.2, axis_e[2] * 1.2];
            let quat = fx_glass_place_quat(&place);
            let q = fx_glass_software_rotate_quat(quat, avel, dt);
            if let Some(place) = self.piece_places.get_mut(piece) {
                fx_glass_place_set_quat(place, q);
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
                fx_glass_state_set_support_mask(state, 0);
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
                .map(fx_glass_state_area_x2)
                .unwrap_or(0.0);
            let axis = self
                .piece_places
                .get(piece as usize)
                .map(|p| fx_unit_quat_to_axis(fx_glass_place_quat(p)))
                .unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            self.launch_software_shard(piece, area, [0.0, 0.0, -1.0], axis, 2);
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
            let mut fall = fx_glass_dynamics_fall_time(&dyn_row);
            if fall == FX_GLASS_FALL_TIME_NEVER {
                continue;
            }
            if fx_glass_dynamics_phys_obj(&dyn_row) != 0 {
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
            let mut vel = fx_glass_dynamics_vel(&dyn_row);
            let mut avel = fx_glass_dynamics_avel(&dyn_row);
            let mut t0 = start;
            let mut left = remain;
            let mut killed = false;
            while left > 0 {
                let step_ms = FX_GLASS_MOTION_STEP_MSEC;
                let t1 = t0 + step_ms;
                let Some(place) = self.piece_places.get(piece).copied() else {
                    break;
                };
                let origin = fx_glass_place_origin(&place);
                let quat = fx_glass_place_quat(&place);
                let next =
                    fx_glass_ballistic_origin(origin, vel, fall, t0, t1, FX_GLASS_FALL_GRAVITY);
                let q = fx_glass_software_rotate_quat(quat, avel, t1.wrapping_sub(t0));
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
                                                fx_glass_place_set_origin(place, hit.end);
                                            }
                                            if let Some(m) = self.contact_mode.get_mut(piece) {
                                                *m = CONTACT_SETTLED;
                                            }
                                            if let Some(row) = self.piece_dynamics.get_mut(piece) {
                                                fx_glass_dynamics_software_launch(
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
                                    fx_glass_dynamics_software_launch(
                                        row,
                                        t1,
                                        bounced,
                                        [avel[0] * 0.3, avel[1] * 0.3, avel[2] * 0.3],
                                    );
                                }
                                if let Some(place) = self.piece_places.get_mut(piece) {
                                    fx_glass_place_set_origin(place, hit.end);
                                    fx_glass_place_set_quat(place, q);
                                }
                                if let Some(used) = self.bounce_used.get_mut(piece) {
                                    *used = true;
                                }
                                if let Some(row) = self.piece_dynamics.get(piece) {
                                    vel = fx_glass_dynamics_vel(row);
                                    avel = fx_glass_dynamics_avel(row);
                                    fall = fx_glass_dynamics_fall_time(row);
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
                    fx_glass_place_set_origin(place, next);
                    fx_glass_place_set_quat(place, q);
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
        let Some((_, outline, vert_n)) = self.piece_outline(piece) else {
            return None;
        };
        let Some(place) = self.piece_places.get(piece as usize) else {
            return None;
        };
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(place));
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
            let p = local_to_world(origin, axis, outline[i]);
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
        loop_i: &fx_iw4::FxGlassSplitLoop,
        support: u32,
    ) -> Option<u32> {
        let vert = loop_i.vert_n;
        let child = self.alloc(vert, 0, 0, 0);
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
            .map(fx_glass_state_geo_start)
            .unwrap_or(0);
        if let Some(state) = self.piece_states.get_mut(cidx) {
            state[..0x0c].copy_from_slice(&parent_state[..0x0c]);
            state[0x0c..0x0e].copy_from_slice(&parent_state[0x0c..0x0e]);
            state[0x10] = parent_state[0x10];
            fx_glass_state_set_support_mask(state, support);
            let flags = (fx_glass_state_flags(parent_state) & !FX_GLASS_STATE_FLAG_CHILD_CLEAR)
                | FX_GLASS_STATE_FLAG_SHATTERED;
            fx_glass_state_set_flags(state, flags);
            let area = fx_glass_loop_area_x2(&loop_i.verts[..vert as usize]);
            let bytes = area.to_le_bytes();
            state[FX_GLASS_STATE_AREA_X2..FX_GLASS_STATE_AREA_X2 + 4].copy_from_slice(&bytes);
            state[fx_iw4::FX_GLASS_STATE_VERT_COUNT] = vert;
            state[fx_iw4::FX_GLASS_STATE_HOLE_DATA_COUNT] = 0;
            state[fx_iw4::FX_GLASS_STATE_CRACK_DATA_COUNT] = 0;
            state[fx_iw4::FX_GLASS_STATE_FAN_DATA_COUNT] = 0;
            let gs = geo_start.to_le_bytes();
            state[fx_iw4::FX_GLASS_STATE_GEO_DATA_START] = gs[0];
            state[fx_iw4::FX_GLASS_STATE_GEO_DATA_START + 1] = gs[1];
        }
        let mut verts = loop_i.verts;
        let parent_origin = fx_glass_place_origin(parent_place);
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(parent_place));
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
        let def_i = usize::from(fx_glass_state_def_index(parent_state));
        let tex = self
            .defs
            .get(def_i)
            .map(fx_glass_def_tex_vecs)
            .unwrap_or([[0.0, 0.0], [0.0, 0.0]]);
        let (new_origin, new_uv) =
            fx_glass_recenter_loop(&mut verts, vert as usize, parent_origin, axis, uv0, tex);
        if let Some(place) = self.piece_places.get_mut(cidx) {
            fx_glass_place_set_origin(place, new_origin);
        }
        if let Some(state) = self.piece_states.get_mut(cidx) {
            state[0..4].copy_from_slice(&new_uv[0].to_le_bytes());
            state[4..8].copy_from_slice(&new_uv[1].to_le_bytes());
        }
        let start = usize::from(geo_start);
        for (i, v) in verts.iter().take(vert as usize).enumerate() {
            if let Some(word) = self.geo_data.get_mut(start + i) {
                *word = fx_glass_pack_geo_vert(v[0], v[1]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use fx_iw4::{
        FX_GLASS_AIRBORNE_PER_BREAK, FX_GLASS_CATCHUP_STEPS, FX_GLASS_DYN_FALL_TIME,
        FX_GLASS_DYN_VEL, FX_GLASS_FALL_GRAVITY, FX_GLASS_FALL_TIME_NEVER, FX_GLASS_INIT_AREA_X2,
        FX_GLASS_INIT_DEF_INDEX, FX_GLASS_INIT_ORIGIN, FX_GLASS_INIT_PIECE_STATE,
        FX_GLASS_INIT_SUPPORT_MASK, FX_GLASS_INIT_VERT_COUNT, FX_GLASS_MOTION_STEP_MSEC,
        FX_GLASS_SETTLED_FADE_MSEC, FX_GLASS_SETTLED_LIFETIME_MSEC, FX_GLASS_STATE_FLAG_SHATTERED,
        fx_glass_ballistic_origin, fx_glass_dynamics_fall_time, fx_glass_dynamics_software_launch,
        fx_glass_dynamics_vel, fx_glass_pack_geo_vert, fx_glass_place_origin, fx_glass_place_quat,
        fx_glass_place_set_origin, fx_glass_state_flags, fx_glass_state_set_support_mask,
    };

    fn write_f32(bytes: &mut [u8], off: usize, v: f32) {
        bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn square_host(half: i16, support: u32, piece_limit: u32, geo_limit: u32) -> FxGlassSystemHost {
        let mut init = [0u8; FX_GLASS_INIT_PIECE_STATE];
        write_f32(&mut init, 12, 1.0);
        write_f32(&mut init, FX_GLASS_INIT_ORIGIN, 0.0);
        write_f32(&mut init, FX_GLASS_INIT_ORIGIN + 4, 0.0);
        write_f32(&mut init, FX_GLASS_INIT_ORIGIN + 8, 0.0);
        init[FX_GLASS_INIT_SUPPORT_MASK..FX_GLASS_INIT_SUPPORT_MASK + 4]
            .copy_from_slice(&support.to_le_bytes());
        let verts = [[-half, -half], [half, -half], [half, half], [-half, half]];
        let area = fx_glass_loop_area_x2(&verts);
        write_f32(&mut init, FX_GLASS_INIT_AREA_X2, area);
        init[FX_GLASS_INIT_DEF_INDEX] = 0;
        init[FX_GLASS_INIT_VERT_COUNT] = 4;
        let geo = [
            fx_glass_pack_geo_vert(verts[0][0], verts[0][1]),
            fx_glass_pack_geo_vert(verts[1][0], verts[1][1]),
            fx_glass_pack_geo_vert(verts[2][0], verts[2][1]),
            fx_glass_pack_geo_vert(verts[3][0], verts[3][1]),
        ];
        let def = [0u8; fx_iw4::FX_GLASS_DEF];
        let mut host = FxGlassSystemHost::default();
        host.reset(FxGlassInitTables {
            piece_limit,
            geo_data_limit: geo_limit,
            init_states: &[init],
            init_geo: &geo,
            defs: &[def],
        });
        host
    }

    fn in_use_count(host: &FxGlassSystemHost) -> u32 {
        (0..host.piece_limit).filter(|p| host.is_in_use(*p)).count() as u32
    }

    #[test]
    fn shatter_consumes_parent_and_emits_children() {
        let mut host = square_host(160, 0xffff, 32, 256);
        assert!(host.is_in_use(0));
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        assert!(!host.is_in_use(0));
        assert!(in_use_count(&host) >= 2);
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let flags = fx_glass_state_flags(&host.piece_states[p as usize]);
            assert_ne!(flags & FX_GLASS_STATE_FLAG_SHATTERED, 0);
        }
    }

    #[test]
    fn shatter_outside_hit_and_zero_dir_still_flies() {
        let mut host = square_host(160, 0, 16, 128);
        assert!(host.shatter(0, [999.0, 999.0, 999.0], [0.0, 0.0, 0.0]));
        let mut flying = 0u32;
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let dyn_row = host.piece_dynamics[p as usize];
            let fall = i32::from_le_bytes(
                dyn_row[FX_GLASS_DYN_FALL_TIME..FX_GLASS_DYN_FALL_TIME + 4]
                    .try_into()
                    .unwrap(),
            );
            if fall == FX_GLASS_FALL_TIME_NEVER {
                continue;
            }
            let vx = f32::from_le_bytes(
                dyn_row[FX_GLASS_DYN_VEL..FX_GLASS_DYN_VEL + 4]
                    .try_into()
                    .unwrap(),
            );
            let vy = f32::from_le_bytes(
                dyn_row[FX_GLASS_DYN_VEL + 4..FX_GLASS_DYN_VEL + 8]
                    .try_into()
                    .unwrap(),
            );
            let vz = f32::from_le_bytes(
                dyn_row[FX_GLASS_DYN_VEL + 8..FX_GLASS_DYN_VEL + 12]
                    .try_into()
                    .unwrap(),
            );
            assert!(vx * vx + vy * vy + vz * vz > 0.0);
            flying += 1;
        }
        assert!(flying >= 1);
    }

    #[test]
    fn oversized_pane_keeps_splitting() {
        let mut host = square_host(2000, 0, 64, 1024);
        let original = fx_glass_state_area_x2(&host.piece_states[0]);
        assert!(original > 300.0);
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let mut max_area = 0.0f32;
        let mut n = 0u32;
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            n += 1;
            max_area = max_area.max(fx_glass_state_area_x2(&host.piece_states[p as usize]));
        }
        assert!(n >= 4, "got {n} pieces");
        assert!(max_area < original, "{max_area} vs {original}");
    }

    #[test]
    fn compact_reclaims_geo_after_shatter() {
        let mut host = square_host(160, 0, 16, 64);
        let before = host.geo_data_count;
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        let mut span = 0u32;
        for p in 0..host.piece_limit {
            if host.is_in_use(p) {
                span += fx_glass_state_geo_span(&host.piece_states[p as usize]);
            }
        }
        assert_eq!(host.geo_data_count, span);
        assert!(host.geo_data_count <= before + 24);
        assert!(!host.need_to_compact);
    }

    #[test]
    fn flying_shards_despawn_after_lifetime() {
        let mut host = square_host(160, 0, 16, 128);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        assert!(in_use_count(&host) >= 1);
        host.advance(10 + FX_GLASS_SHARD_LIFETIME_MSEC + 1);
        assert_eq!(in_use_count(&host), 0);
    }

    struct Floor;
    impl GlassWorldTrace for Floor {
        fn sweep(&self, start: [f32; 3], end: [f32; 3]) -> Option<GlassWorldContact> {
            if end[2] >= start[2] {
                return None;
            }
            let target = 0.0f32;
            if start[2] <= target && end[2] <= target {
                return Some(GlassWorldContact {
                    fraction: 0.0,
                    end: [start[0], start[1], target],
                    normal: [0.0, 0.0, 1.0],
                    startsolid: true,
                    pane: None,
                });
            }
            if start[2] > target && end[2] <= target {
                let t = (start[2] - target) / (start[2] - end[2]);
                Some(GlassWorldContact {
                    fraction: t.clamp(0.0, 1.0),
                    end: [
                        start[0] + (end[0] - start[0]) * t,
                        start[1] + (end[1] - start[1]) * t,
                        target,
                    ],
                    normal: [0.0, 0.0, 1.0],
                    startsolid: false,
                    pane: None,
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn shards_disappear_on_floor_contact() {
        let mut host = square_host(160, 0, 16, 128);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 8.0], [0.0, 0.0, -1.0]));
        host.advance_in_world(10 + 2000, Some(&Floor));
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let mode = host
                .contact_mode
                .get(p as usize)
                .copied()
                .unwrap_or(CONTACT_NONE);
            if mode == CONTACT_DISAPPEAR {
                panic!("disappear-on-contact shard survived floor");
            }
        }
    }

    #[test]
    fn shatter_children_recenter_off_parent_origin() {
        let mut host = square_host(160, 0, 32, 256);
        let parent = fx_glass_place_origin(&host.piece_places[0]);
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let mut moved = 0u32;
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let origin = fx_glass_place_origin(&host.piece_places[p as usize]);
            let dx = origin[0] - parent[0];
            let dy = origin[1] - parent[1];
            if dx * dx + dy * dy > 1.0 {
                moved += 1;
            }
        }
        assert!(moved >= 1);
        assert!(!host.take_events().is_empty());
    }

    #[test]
    fn late_shatter_opens_hole_without_airborne() {
        let mut host = square_host(160, 0xffff, 32, 256);
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(1),
            false,
            false,
            1,
            1
        ));
        assert!(!host.is_in_use(0));
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let fall = fx_glass_dynamics_fall_time(&host.piece_dynamics[p as usize]);
            assert_eq!(fall, FX_GLASS_FALL_TIME_NEVER);
        }
        let events = host.take_events();
        assert_eq!(events.len(), 1);
        assert!(!events[0].play_oneshot);
        assert!((events[0].normal[2] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn seek_replay_launches_shards_without_oneshot() {
        let mut host = square_host(160, 0, 32, 256);
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(1),
            true,
            false,
            0,
            1
        ));
        let events = host.take_events();
        assert_eq!(events.len(), 1);
        assert!(!events[0].play_oneshot);
        let airborne = (0..host.piece_limit)
            .filter(|p| {
                host.is_in_use(*p)
                    && fx_glass_dynamics_fall_time(&host.piece_dynamics[*p as usize])
                        != FX_GLASS_FALL_TIME_NEVER
            })
            .count();
        assert!(
            airborne >= 1,
            "seek within airborne life still launches shards"
        );
    }

    #[test]
    fn durable_seed_repeats_child_origins() {
        const SHATTER_SEED: u64 = 7;
        let mut a = square_host(160, 0, 32, 256);
        let mut b = square_host(160, 0, 32, 256);
        assert!(a.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(SHATTER_SEED),
            true,
            true,
            0,
            1
        ));
        assert!(b.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(SHATTER_SEED),
            true,
            true,
            0,
            1
        ));
        let origins_a: Vec<_> = (0..a.piece_limit)
            .filter(|p| a.is_in_use(*p))
            .map(|p| fx_glass_place_origin(&a.piece_places[p as usize]))
            .collect();
        let origins_b: Vec<_> = (0..b.piece_limit)
            .filter(|p| b.is_in_use(*p))
            .map(|p| fx_glass_place_origin(&b.piece_places[p as usize]))
            .collect();
        assert_eq!(origins_a, origins_b);
    }

    #[test]
    fn delete_frees_descendants_of_the_pane() {
        let mut host = square_host(160, 0, 32, 256);
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let kids = (0..host.piece_limit).filter(|p| host.is_in_use(*p)).count();
        assert!(kids >= 1);
        host.free_pane(0);
        let left = (0..host.piece_limit).filter(|p| host.is_in_use(*p)).count();
        assert_eq!(left, 0);
    }

    #[test]
    fn collapse_falls_instead_of_flying_along_the_normal() {
        let mut host = square_host(160, 0, 32, 256);
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(2),
            true,
            true,
            2,
            1
        ));
        let mut falling = 0u32;
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let vel = fx_glass_dynamics_vel(&host.piece_dynamics[p as usize]);
            let horiz = vel[0] * vel[0] + vel[1] * vel[1];
            if vel[2] < 0.0 && horiz < vel[2] * vel[2] {
                falling += 1;
            }
        }
        assert!(falling >= 1);
    }

    #[test]
    fn pending_single_support_tilts_before_release() {
        let mut host = square_host(160, 1, 32, 256);
        fx_glass_state_set_support_mask(&mut host.piece_states[0], 1);
        host.time = 10;
        host.prev_time = 10;
        host.release_at[0] = 2_000;
        let before = fx_glass_place_quat(&host.piece_places[0]);
        host.advance(410);
        let after = fx_glass_place_quat(&host.piece_places[0]);
        assert_ne!(before, after);
        assert!(host.is_in_use(0));
        assert_eq!(host.release_at[0], 2_000);
    }

    #[test]
    fn floor_contact_emits_an_aggregated_landing() {
        struct AlwaysFloor;
        impl GlassWorldTrace for AlwaysFloor {
            fn sweep(&self, start: [f32; 3], end: [f32; 3]) -> Option<GlassWorldContact> {
                Some(GlassWorldContact {
                    fraction: 0.5,
                    end: [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    startsolid: false,
                    pane: None,
                })
            }
        }
        let mut host = square_host(160, 0, 16, 128);
        if let Some(place) = host.piece_places.get_mut(0) {
            fx_glass_place_set_origin(place, [0.0, 0.0, 32.0]);
        }
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 32.0], [0.0, 0.0, -1.0]));
        let _ = host.take_events();
        host.advance_in_world(10 + 200, Some(&AlwaysFloor));
        let landings = host.take_events().into_iter().filter(|e| e.landing).count();
        assert!(landings >= 1);
        assert!(landings <= 4);
    }

    #[test]
    fn free_clears_source_pane_and_bumps_generation() {
        let mut host = square_host(160, 0, 32, 256);
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let kid = (0..host.piece_limit)
            .find(|p| host.is_in_use(*p))
            .expect("child");
        let generation = host.generation[kid as usize];
        host.free(kid);
        assert!(!host.is_in_use(kid));
        assert_eq!(host.source_pane[kid as usize], u32::MAX);
        assert_eq!(host.generation[kid as usize], generation.wrapping_add(1));
        assert_eq!(host.release_at[kid as usize], 0);
    }

    #[test]
    fn same_revision_does_not_emit_a_second_break_event() {
        let mut host = square_host(160, 0, 32, 256);
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(1),
            true,
            true,
            0,
            4
        ));
        assert_eq!(host.take_events().len(), 1);
        assert_eq!(host.last_event_revision[0], 4);
        host.push_break_event(0, [0.0; 3], [0.0, 0.0, 1.0], true, 0, 4);
        assert!(host.take_events().is_empty());
        host.push_break_event(0, [0.0; 3], [0.0, 0.0, 1.0], true, 0, 5);
        assert_eq!(host.take_events().len(), 1);
    }

    fn first_airborne(host: &FxGlassSystemHost) -> usize {
        (0..host.piece_limit as usize)
            .find(|&p| {
                host.is_in_use(p as u32)
                    && fx_glass_dynamics_fall_time(&host.piece_dynamics[p])
                        != FX_GLASS_FALL_TIME_NEVER
            })
            .expect("airborne shard")
    }

    #[test]
    fn leftover_does_not_simulate_and_draw_place_extrapolates() {
        let mut host = square_host(160, 0, 32, 256);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(1),
            true,
            true,
            0,
            1
        ));
        let piece = first_airborne(&host);
        let sim0 = fx_glass_place_origin(&host.piece_places[piece]);
        let vel = fx_glass_dynamics_vel(&host.piece_dynamics[piece]);
        let fall = fx_glass_dynamics_fall_time(&host.piece_dynamics[piece]);
        host.advance(18);
        assert_eq!(host.motion_accum_msec, 8);
        assert_eq!(fx_glass_place_origin(&host.piece_places[piece]), sim0);
        let draw = fx_glass_place_origin(&host.draw_place(piece).expect("place"));
        let expect = fx_glass_ballistic_origin(sim0, vel, fall, 10, 18, FX_GLASS_FALL_GRAVITY);
        assert_eq!(draw, expect);
        assert_ne!(draw, sim0);
        host.advance(26);
        assert_eq!(host.motion_accum_msec, 0);
        assert_ne!(fx_glass_place_origin(&host.piece_places[piece]), sim0);
    }

    #[test]
    fn hitch_matches_four_full_steps() {
        let mut short = square_host(160, 0, 32, 256);
        let mut hitch = square_host(160, 0, 32, 256);
        short.time = 10;
        short.prev_time = 10;
        hitch.time = 10;
        hitch.prev_time = 10;
        assert!(short.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(7),
            true,
            true,
            0,
            1
        ));
        assert!(hitch.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(7),
            true,
            true,
            0,
            1
        ));
        let span = FX_GLASS_MOTION_STEP_MSEC * FX_GLASS_CATCHUP_STEPS;
        short.advance(10 + span);
        hitch.advance(10 + 2000);
        assert_eq!(short.motion_accum_msec, 0);
        assert_eq!(hitch.motion_accum_msec, 0);
        let origins_short: Vec<_> = (0..short.piece_limit)
            .filter(|p| short.is_in_use(*p))
            .map(|p| fx_glass_place_origin(&short.piece_places[p as usize]))
            .collect();
        let origins_hitch: Vec<_> = (0..hitch.piece_limit)
            .filter(|p| hitch.is_in_use(*p))
            .map(|p| fx_glass_place_origin(&hitch.piece_places[p as usize]))
            .collect();
        assert_eq!(origins_short, origins_hitch);
        assert!(!origins_short.is_empty());
    }

    #[test]
    fn settled_fade_drops_in_the_last_quarter_second() {
        let mut host = square_host(160, 0, 32, 256);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter_caused(
            0,
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            Some(1),
            true,
            true,
            0,
            1
        ));
        let piece = first_airborne(&host);
        host.contact_mode[piece] = CONTACT_SETTLED;
        fx_glass_dynamics_software_launch(
            &mut host.piece_dynamics[piece],
            10,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        );
        host.time = 10 + FX_GLASS_SETTLED_LIFETIME_MSEC - FX_GLASS_SETTLED_FADE_MSEC / 2;
        let fade = host.piece_fade(piece);
        assert!((fade - 0.5).abs() < 1e-5, "{fade}");
        assert_eq!(host.piece_fade(0), 1.0);
    }

    #[test]
    fn startsolid_in_source_pane_does_not_kill() {
        struct OwnPane;
        impl GlassWorldTrace for OwnPane {
            fn sweep(&self, start: [f32; 3], _end: [f32; 3]) -> Option<GlassWorldContact> {
                Some(GlassWorldContact {
                    fraction: 0.0,
                    end: start,
                    normal: [0.0, 0.0, 1.0],
                    startsolid: true,
                    pane: Some(0),
                })
            }
        }
        let mut host = square_host(160, 0, 32, 256);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        let before = in_use_count(&host);
        assert!(before >= 1);
        host.advance_in_world(10 + FX_GLASS_MOTION_STEP_MSEC, Some(&OwnPane));
        assert_eq!(in_use_count(&host), before);
    }

    #[test]
    fn startsolid_in_world_still_kills() {
        struct WorldSolid;
        impl GlassWorldTrace for WorldSolid {
            fn sweep(&self, start: [f32; 3], _end: [f32; 3]) -> Option<GlassWorldContact> {
                Some(GlassWorldContact {
                    fraction: 0.0,
                    end: start,
                    normal: [0.0, 0.0, 1.0],
                    startsolid: true,
                    pane: None,
                })
            }
        }
        let mut host = square_host(160, 0, 16, 128);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        assert!(in_use_count(&host) >= 1);
        host.advance_in_world(10 + FX_GLASS_MOTION_STEP_MSEC, Some(&WorldSolid));
        for p in 0..host.piece_limit {
            if !host.is_in_use(p) {
                continue;
            }
            let mode = host
                .contact_mode
                .get(p as usize)
                .copied()
                .unwrap_or(CONTACT_NONE);
            if mode == CONTACT_DISAPPEAR || mode == CONTACT_BOUNCE {
                panic!("airborne shard survived world startsolid");
            }
        }
    }

    #[test]
    fn one_break_does_not_keep_more_than_sixty_four_airborne() {
        let mut host = square_host(2000, 0, 128, 4096);
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let airborne = (0..host.piece_limit)
            .filter(|p| host.is_airborne(*p as usize))
            .count();
        assert!(
            airborne <= FX_GLASS_AIRBORNE_PER_BREAK as usize,
            "airborne {airborne}"
        );
        assert!(airborne >= 1);
    }

    #[test]
    fn settled_piece_does_not_keep_falling() {
        let mut host = square_host(160, 0, 32, 256);
        host.time = 10;
        host.prev_time = 10;
        assert!(host.shatter(0, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let piece = first_airborne(&host);
        host.contact_mode[piece] = CONTACT_SETTLED;
        fx_glass_dynamics_software_launch(
            &mut host.piece_dynamics[piece],
            10,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        );
        if let Some(place) = host.piece_places.get_mut(piece) {
            fx_glass_place_set_origin(place, [0.0, 0.0, 32.0]);
        }
        let before = fx_glass_place_origin(&host.piece_places[piece]);
        host.advance(10 + 200);
        assert!(host.is_in_use(piece as u32));
        assert_eq!(fx_glass_place_origin(&host.piece_places[piece]), before);
        assert_eq!(host.contact_mode[piece], CONTACT_SETTLED);
    }
}
