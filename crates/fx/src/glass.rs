use fx_iw4::{
    FX_GLASS_ANGULAR_VEL_MAX, FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_FALL_GRAVITY,
    FX_GLASS_FALL_TIME_NEVER, FX_GLASS_FREE_SENTINEL, FX_GLASS_GEOMETRY_DATA,
    FX_GLASS_LINEAR_VEL_MAX, FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_PIECE_DYNAMICS,
    FX_GLASS_PIECE_PLACE, FX_GLASS_PIECE_STATE, FX_GLASS_STATE_AREA_X2,
    FX_GLASS_STATE_FLAG_CHILD_CLEAR, FX_GLASS_STATE_FLAG_SHATTERED, FX_GLASS_TRACE_INTERVAL_MSEC,
    FX_GLASS_VERT_SCALE, fx_glass_alloc_piece, fx_glass_ballistic_origin, fx_glass_dynamics_avel,
    fx_glass_dynamics_fall_time, fx_glass_dynamics_init_row, fx_glass_dynamics_phys_obj,
    fx_glass_dynamics_software_launch, fx_glass_dynamics_vel, fx_glass_free_piece,
    fx_glass_fringe_cap, fx_glass_fringe_prune_knock_order, fx_glass_geo_vert,
    fx_glass_interior_branch_count, fx_glass_is_in_use, fx_glass_lerp_range, fx_glass_loop_area_x2,
    fx_glass_pack_geo_vert, fx_glass_piece_speed_scale, fx_glass_place_origin, fx_glass_place_quat,
    fx_glass_place_set_origin, fx_glass_place_set_quat, fx_glass_radial_split,
    fx_glass_reset_copy_geo, fx_glass_reset_copy_piece, fx_glass_reset_free_list,
    fx_glass_set_in_use, fx_glass_shatter_rand, fx_glass_software_rotate_quat,
    fx_glass_software_trace_due, fx_glass_state_area_x2, fx_glass_state_flags,
    fx_glass_state_geo_start, fx_glass_state_set_flags, fx_glass_state_set_support_mask,
    fx_glass_state_support_mask, fx_glass_state_vert_count, fx_unit_quat_to_axis,
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

    pub time: i32,
    pub prev_time: i32,

    pub moved: bool,
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
        }
    }

    pub fn shatter(&mut self, piece: u32, hit: [f32; 3], dir: [f32; 3]) -> bool {
        let idx = piece as usize;
        let Some(state) = self.piece_states.get_mut(idx) else {
            return false;
        };
        let flags = fx_glass_state_flags(state) | FX_GLASS_STATE_FLAG_SHATTERED;
        fx_glass_state_set_flags(state, flags);
        let place = self.piece_places.get(idx).copied();
        let state = self.piece_states.get(idx).copied();
        let (Some(place), Some(state)) = (place, state) else {
            return false;
        };
        let vert_n = usize::from(fx_glass_state_vert_count(&state));
        let start = usize::from(fx_glass_state_geo_start(&state));
        if vert_n < 3 {
            return false;
        }
        let end = start.saturating_add(vert_n);
        let Some(geo) = self.geo_data.get(start..end) else {
            return false;
        };
        let mut outline = [[0i16; 2]; 32];
        if vert_n > outline.len() {
            return false;
        }
        for (dst, word) in outline.iter_mut().take(vert_n).zip(geo.iter()) {
            *dst = fx_glass_geo_vert(word);
        }
        let origin = fx_glass_place_origin(&place);
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(&place));
        let delta = [hit[0] - origin[0], hit[1] - origin[1], hit[2] - origin[2]];
        let local = [
            (delta[0] * axis[0][0] + delta[1] * axis[0][1] + delta[2] * axis[0][2])
                / FX_GLASS_VERT_SCALE,
            (delta[0] * axis[1][0] + delta[1] * axis[1][1] + delta[2] * axis[1][2])
                / FX_GLASS_VERT_SCALE,
        ];
        let rand01 = fx_glass_shatter_rand(&mut self.shatter_rand);
        let branch_n = fx_glass_interior_branch_count(rand01);
        let loops = fx_glass_radial_split(&outline[..vert_n], local, branch_n);
        let parent_support = fx_glass_state_support_mask(&state);
        let original_area = {
            let a = fx_glass_state_area_x2(&state);
            if a > 0.0 {
                a
            } else {
                fx_glass_loop_area_x2(&outline[..vert_n])
            }
        };
        let mut spawned = [0u32; 6];
        let mut spawned_n = 0usize;
        for child in loops.iter().filter(|l| l.vert_n >= 3) {
            let support = parent_support & child.original_edges;
            if let Some(id) = self.emit_child(idx, &place, &state, child, support) {
                spawned[spawned_n] = id;
                spawned_n += 1;
            }
        }
        if spawned_n == 0 {
            return false;
        }
        self.free(piece);
        self.prune_fringe_and_launch(&spawned[..spawned_n], original_area, dir);
        true
    }

    fn prune_fringe_and_launch(&mut self, children: &[u32], original_area: f32, dir: [f32; 3]) {
        let mut areas = [0.0f32; 6];
        let mut supports = [0u32; 6];
        let n = children.len().min(6);
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
        let order = fx_glass_fringe_prune_knock_order(&areas[..n], &supports[..n]);
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
        for i in 0..n {
            if supports[i] != 0 {
                continue;
            }
            self.launch_software_shard(children[i], areas[i], dir);
        }
    }

    fn launch_software_shard(&mut self, piece: u32, area_x2: f32, dir: [f32; 3]) {
        let scale = fx_glass_piece_speed_scale(area_x2);
        let lin_r = fx_glass_shatter_rand(&mut self.shatter_rand);
        let speed =
            fx_glass_lerp_range(FX_GLASS_LINEAR_VEL_MIN, FX_GLASS_LINEAR_VEL_MAX, lin_r) * scale;
        let dir_len_sq = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        let vel = if dir_len_sq > 0.0 {
            [dir[0] * speed, dir[1] * speed, dir[2] * speed]
        } else {
            [0.0, 0.0, 0.0]
        };
        let ang_r = fx_glass_shatter_rand(&mut self.shatter_rand);
        let ang =
            fx_glass_lerp_range(FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_ANGULAR_VEL_MAX, ang_r) * scale;
        let avel = if dir_len_sq > 0.0 {
            [0.0, 0.0, ang]
        } else {
            [0.0, 0.0, 0.0]
        };
        if let Some(row) = self.piece_dynamics.get_mut(piece as usize) {
            fx_glass_dynamics_software_launch(row, self.time, vel, avel);
        }
    }

    pub fn advance(&mut self, msec: i32) {
        if self.time == 0 && self.prev_time == 0 {
            self.time = msec;
            self.prev_time = msec;
            return;
        }
        self.prev_time = self.time;
        self.time = msec;
        if self.time == self.prev_time {
            return;
        }
        self.integrate_software();
    }

    fn integrate_software(&mut self) {
        let prev = self.prev_time;
        let now = self.time;
        let dt = now.wrapping_sub(prev);
        let start = if prev < now { prev } else { now };
        let n = self.piece_limit as usize;
        for piece in 0..n {
            if !self.is_in_use(piece as u32) {
                continue;
            }
            let Some(dyn_row) = self.piece_dynamics.get(piece).copied() else {
                continue;
            };
            let fall = fx_glass_dynamics_fall_time(&dyn_row);
            if fall == FX_GLASS_FALL_TIME_NEVER {
                continue;
            }
            if fx_glass_dynamics_phys_obj(&dyn_row) != 0 {
                continue;
            }
            let _due =
                fx_glass_software_trace_due(piece as u32, start, now, FX_GLASS_TRACE_INTERVAL_MSEC);
            let vel = fx_glass_dynamics_vel(&dyn_row);
            let avel = fx_glass_dynamics_avel(&dyn_row);
            let Some(place) = self.piece_places.get(piece) else {
                continue;
            };
            let origin = fx_glass_place_origin(place);
            let quat = fx_glass_place_quat(place);
            let next =
                fx_glass_ballistic_origin(origin, vel, fall, prev, now, FX_GLASS_FALL_GRAVITY);
            let q = fx_glass_software_rotate_quat(quat, avel, dt);
            if let Some(place) = self.piece_places.get_mut(piece) {
                fx_glass_place_set_origin(place, next);
                fx_glass_place_set_quat(place, q);
            }
            self.moved = true;
        }
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
        let start = usize::from(geo_start);
        for (i, v) in loop_i.verts.iter().take(vert as usize).enumerate() {
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
        Some(child)
    }
}
