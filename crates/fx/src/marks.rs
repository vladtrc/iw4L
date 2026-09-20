use marks_iw4::{
    FX_MARK_HANDLE_NONE, FX_MARKS_INIT_ALLOCED_COUNT, FX_MARKS_LIMIT, FX_POINT_GROUP_CHAIN_NONE,
    FX_POINT_GROUP_LIMIT, FX_POINT_GROUP_NEXT_NONE, FX_TRI_GROUP_CHAIN_NONE, FX_TRI_GROUP_LIMIT,
    FX_TRI_GROUP_NEXT_NONE, FxAllocMarkRequest, FxMarkConstructed, FxMarkStagingPoint,
    FxMarkStagingTri, FxPointGroup, FxTriGroup, GFX_MARK_MESH_VERTEX_STRIDE, GfxMarkMeshBudget,
    fx_alloc_and_construct_mark, fx_copy_mark_points, fx_copy_mark_tris,
    fx_generate_mark_verts_begin, fx_impact_mark_models_generate, fx_impact_mark_outer_gate,
    fx_init_mark_next_handle, fx_init_point_next_slot, fx_init_tri_next_slot,
    fx_mark_context_is_world_list, fx_mark_contexts_equal, fx_mark_point_groups_for_count,
    fx_mark_tri_groups_for_staging, fx_pack_mark_world_vertex, r_add_mark_mesh_draw_surf,
};

#[derive(Clone, Debug)]
pub struct FxMarksSystemHost {
    first_free: u16,

    next: Vec<u16>,
    live: u32,

    alloced: u32,
    constructed: Vec<Option<FxMarkConstructed>>,

    material_names: Vec<Option<String>>,
    tri_first: u32,
    tri_next: Vec<u32>,
    tri_groups: Vec<FxTriGroup>,
    point_first: u32,
    point_next: Vec<u32>,
    point_groups: Vec<FxPointGroup>,

    pub no_marks: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GfxMarkMeshSurf {
    pub index_start: u32,
    pub index_count: u32,
    pub mark_slot: u16,

    pub context: [u8; 7],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GfxMarkMeshCensus {
    pub budget: GfxMarkMeshBudget,
    pub first_xyz: Option<[f32; 3]>,
    pub verts: Vec<FxMarkStagingPoint>,
    pub packed: Vec<[u8; GFX_MARK_MESH_VERTEX_STRIDE]>,
    pub indices: Vec<u16>,
    pub surfs: Vec<GfxMarkMeshSurf>,
}

#[derive(Clone, Debug)]
pub struct MarkTraceRecord {
    pub id: u32,
    pub bolt: u8,
    pub entered: bool,
    pub against_world: bool,
    pub against_models: bool,
    pub parent: Option<String>,
    pub elem: Option<u8>,
    pub msec: Option<i32>,
    pub origin: Option<[f32; 3]>,
    pub size0: Option<f32>,

    pub color: Option<u32>,

    pub color_kind: &'static str,
    pub mat0: Option<String>,
    pub mat1: Option<String>,

    pub bound: Option<String>,
    pub slot: Option<u16>,
    pub tri_n: Option<u8>,
    pub point_n: Option<i16>,
    pub native_color: Option<u32>,
    pub context: Option<u32>,
    pub skip_why: Option<&'static str>,
    pub vis: Option<u8>,
    pub mat0_edge: Option<String>,
    pub mat1_edge: Option<String>,
    pub nx: Option<f32>,
    pub ny: Option<f32>,
    pub nz: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct MarkReceiverEnable {
    pub fx_marks: bool,
    pub fx_marks_ents: bool,
    pub fx_marks_smodels: bool,
}

impl Default for MarkReceiverEnable {
    fn default() -> Self {
        Self {
            fx_marks: true,
            fx_marks_ents: true,
            fx_marks_smodels: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MarkImpactRequest {
    pub skip_world: bool,
    pub receivers: MarkReceiverEnable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkImpactResult {
    pub entered: bool,
    pub against_world: bool,
    pub against_models: bool,
}

impl FxMarksSystemHost {
    pub fn init() -> Self {
        let n = FX_MARKS_LIMIT as usize;
        let mut next = vec![FX_MARK_HANDLE_NONE; n];
        for slot in 0..FX_MARKS_LIMIT {
            next[slot as usize] = fx_init_mark_next_handle(slot);
        }
        let mut tri_next = vec![FX_TRI_GROUP_NEXT_NONE; FX_TRI_GROUP_LIMIT as usize];
        for slot in 0..FX_TRI_GROUP_LIMIT {
            tri_next[slot as usize] = fx_init_tri_next_slot(slot);
        }
        let mut point_next = vec![FX_POINT_GROUP_NEXT_NONE; FX_POINT_GROUP_LIMIT as usize];
        for slot in 0..FX_POINT_GROUP_LIMIT {
            point_next[slot as usize] = fx_init_point_next_slot(slot);
        }
        Self {
            first_free: 0,
            next,
            live: 0,
            alloced: FX_MARKS_INIT_ALLOCED_COUNT,
            constructed: vec![None; n],
            material_names: vec![None; n],
            tri_first: 0,
            tri_next,
            tri_groups: vec![FxTriGroup::ZERO; FX_TRI_GROUP_LIMIT as usize],
            point_first: 0,
            point_next,
            point_groups: vec![FxPointGroup::ZERO; FX_POINT_GROUP_LIMIT as usize],
            no_marks: false,
        }
    }

    pub fn live_count(&self) -> u32 {
        self.live
    }

    pub fn alloced_count(&self) -> u32 {
        self.alloced
    }

    pub fn first_free_handle(&self) -> u16 {
        self.first_free
    }

    pub fn tri_first_free(&self) -> u32 {
        self.tri_first
    }

    pub fn point_first_free(&self) -> u32 {
        self.point_first
    }

    pub fn alloc_mark_from_go_callback(
        &mut self,
        req: FxAllocMarkRequest,
        staging_tris: &[FxMarkStagingTri],
        staging_points: &[FxMarkStagingPoint],
    ) -> Option<u16> {
        if fx_alloc_and_construct_mark(&req, 0, 0).is_err() {
            return None;
        }
        if (staging_tris.len() as u32) < req.tri_count
            || (staging_points.len() as u32) < req.point_count
        {
            return None;
        }
        if self.first_free == FX_MARK_HANDLE_NONE {
            return None;
        }
        let tris = &staging_tris[..req.tri_count as usize];
        let points = &staging_points[..req.point_count as usize];
        let tri_head = self.pop_tri_chain(fx_mark_tri_groups_for_staging(tris))?;
        let point_head = self.pop_point_chain(fx_mark_point_groups_for_count(req.point_count))?;
        let copied_tri = fx_copy_mark_tris(&mut self.tri_groups, tri_head, tris);
        let copied_point = fx_copy_mark_points(&mut self.point_groups, point_head, points);
        if copied_tri != req.tri_count || copied_point != req.point_count {
            return None;
        }
        let mark = fx_alloc_and_construct_mark(&req, tri_head, point_head).ok()?;
        let handle = self.first_free;
        self.first_free = self.next[handle as usize];
        self.constructed[handle as usize] = Some(mark);
        self.live = self.live.saturating_add(1);
        self.alloced = self.alloced.saturating_add(1);
        Some(handle)
    }

    pub fn set_material_name(&mut self, handle: u16, name: Option<&str>) {
        if let Some(slot) = self.material_names.get_mut(handle as usize) {
            *slot = name.filter(|n| !n.is_empty()).map(str::to_owned);
        }
    }

    pub fn material_name(&self, handle: u16) -> Option<&str> {
        self.material_names
            .get(handle as usize)
            .and_then(|n| n.as_deref())
    }

    pub fn tri_group(&self, slot: u16) -> Option<&FxTriGroup> {
        self.tri_groups.get(slot as usize)
    }

    pub fn point_group(&self, slot: u16) -> Option<&FxPointGroup> {
        self.point_groups.get(slot as usize)
    }

    fn pop_tri_chain(&mut self, n: u32) -> Option<u16> {
        if n == 0 {
            return None;
        }
        let saved = self.tri_first;
        let mut popped = Vec::with_capacity(n as usize);
        let mut remaining = n;
        while remaining > 0 {
            let slot = self.tri_first as usize;
            if slot >= self.tri_next.len() {
                self.tri_first = saved;
                return None;
            }
            popped.push(self.tri_first as u16);
            self.tri_first = self.tri_next[slot];
            remaining -= 1;
        }
        let mut prev = FX_TRI_GROUP_CHAIN_NONE;
        for &slot in &popped {
            self.tri_groups[slot as usize] = FxTriGroup::ZERO;
            self.tri_groups[slot as usize].next = prev;
            prev = slot;
        }
        Some(prev)
    }

    fn pop_point_chain(&mut self, n: u32) -> Option<u16> {
        if n == 0 {
            return None;
        }
        let saved = self.point_first;
        let mut popped = Vec::with_capacity(n as usize);
        let mut remaining = n;
        while remaining > 0 {
            let slot = self.point_first as usize;
            if slot >= self.point_next.len() {
                self.point_first = saved;
                return None;
            }
            popped.push(self.point_first as u16);
            self.point_first = self.point_next[slot];
            remaining -= 1;
        }
        let mut prev = FX_POINT_GROUP_CHAIN_NONE;
        for &slot in &popped {
            self.point_groups[slot as usize] = FxPointGroup::ZERO;
            self.point_groups[slot as usize].next = prev;
            prev = slot;
        }
        Some(prev)
    }

    pub fn constructed(&self, handle: u16) -> Option<&FxMarkConstructed> {
        self.constructed
            .get(handle as usize)
            .and_then(|m| m.as_ref())
    }

    pub fn hide_glass_marks(&mut self, piece: u16) -> u32 {
        let mut hidden = 0u32;
        for slot in 0..self.constructed.len() {
            let Some(mark) = self.constructed[slot] else {
                continue;
            };
            if mark.context as u8 != 4 || (mark.context >> 16) as u16 != piece {
                continue;
            }
            self.recycle_tri_chain(mark.tris);
            self.recycle_point_chain(mark.points);
            self.constructed[slot] = None;
            if let Some(name) = self.material_names.get_mut(slot) {
                *name = None;
            }
            self.next[slot] = self.first_free;
            self.first_free = slot as u16;
            self.live = self.live.saturating_sub(1);
            hidden = hidden.saturating_add(1);
        }
        hidden
    }

    fn recycle_tri_chain(&mut self, mut head: u16) {
        while head != FX_TRI_GROUP_CHAIN_NONE {
            let slot = head as usize;
            if slot >= self.tri_groups.len() {
                break;
            }
            let next = self.tri_groups[slot].next;
            self.tri_groups[slot] = FxTriGroup::ZERO;
            self.tri_next[slot] = self.tri_first;
            self.tri_first = head as u32;
            head = next;
        }
    }

    fn recycle_point_chain(&mut self, mut head: u16) {
        while head != FX_POINT_GROUP_CHAIN_NONE {
            let slot = head as usize;
            if slot >= self.point_groups.len() {
                break;
            }
            let next = self.point_groups[slot].next;
            self.point_groups[slot] = FxPointGroup::ZERO;
            self.point_next[slot] = self.point_first;
            self.point_first = head as u32;
            head = next;
        }
    }

    pub fn generate_world_mark_verts(&self) -> GfxMarkMeshCensus {
        let mut census = GfxMarkMeshCensus::default();
        for slot in 0..FX_MARKS_LIMIT {
            let Some(mark) = self.constructed[slot as usize] else {
                continue;
            };
            if !fx_mark_context_is_world_list(mark.context as u8) && mark.context as u8 != 4 {
                continue;
            }
            if self
                .emit_one_world_mark(&mut census, slot as u16, mark, &|point| point)
                .is_err()
            {
                break;
            }
        }
        census
    }

    pub fn append_model_mark(
        &self,
        census: &mut GfxMarkMeshCensus,
        slot: u16,
        origin: [f32; 3],
        tex_coord_axis: [f32; 3],
        transform_point: &dyn Fn(FxMarkStagingPoint) -> FxMarkStagingPoint,
    ) -> Result<(), marks_iw4::GfxMarkMeshRefuse> {
        let Some(mut mark) = self.constructed(slot).copied() else {
            return Ok(());
        };
        mark.origin = origin;
        mark.tex_coord_axis = tex_coord_axis;
        self.emit_one_world_mark(census, slot, mark, transform_point)
    }

    fn emit_one_world_mark(
        &self,
        census: &mut GfxMarkMeshCensus,
        mark_slot: u16,
        mark: FxMarkConstructed,
        transform_point: &dyn Fn(FxMarkStagingPoint) -> FxMarkStagingPoint,
    ) -> Result<(), marks_iw4::GfxMarkMeshRefuse> {
        let (base_vert, _) =
            fx_generate_mark_verts_begin(&mut census.budget, mark.point_count, mark.tri_count)?;
        census
            .verts
            .resize(census.budget.vert_n as usize, FxMarkStagingPoint::ZERO);
        census.packed.resize(
            census.budget.vert_n as usize,
            [0u8; GFX_MARK_MESH_VERTEX_STRIDE],
        );
        let mut remaining_pts = mark.point_count.max(0) as u16;
        let mut pt_handle = mark.points;
        let mut out_i = base_vert as usize;
        while remaining_pts > 0 {
            let Some(group) = self.point_group(pt_handle) else {
                break;
            };
            let take = remaining_pts.min(2);
            for k in 0..take {
                let pt = transform_point(group.points[k as usize]);
                if let Some(slot) = census.verts.get_mut(out_i) {
                    *slot = pt;
                }
                if let Some(row) = census.packed.get_mut(out_i) {
                    let pack = if matches!(mark.context as u8, 0 | 2) {
                        fx_pack_mark_world_vertex
                    } else {
                        marks_iw4::fx_pack_mark_model_vertex
                    };
                    *row = pack(
                        &pt,
                        mark.origin,
                        mark.radius,
                        mark.tex_coord_axis,
                        mark.native_color,
                    );
                }
                if census.first_xyz.is_none() {
                    census.first_xyz = Some(pt.xyz);
                }
                out_i += 1;
            }
            remaining_pts -= take;
            if group.next == FX_POINT_GROUP_CHAIN_NONE {
                break;
            }
            pt_handle = group.next;
        }

        let mut remaining_tri = mark.tri_count;
        let mut tri_handle = mark.tris;
        let mut cur_context: Option<[u8; 7]> = None;
        let mut batch_index_count: u32 = 0;
        let mut batch_index_start = census.indices.len() as u32;
        while remaining_tri > 0 {
            let Some(group) = self.tri_group(tri_handle) else {
                break;
            };
            let n = remaining_tri.min(group.tri_count);
            if let Some(ctx) = cur_context {
                if !fx_mark_contexts_equal(&ctx, &group.context) && batch_index_count != 0 {
                    let _ = r_add_mark_mesh_draw_surf(&mut census.budget, batch_index_count);
                    census.surfs.push(GfxMarkMeshSurf {
                        index_start: batch_index_start,
                        index_count: batch_index_count,
                        mark_slot,
                        context: ctx,
                    });
                    batch_index_start = census.indices.len() as u32;
                    batch_index_count = 0;
                }
            }
            cur_context = Some(group.context);
            for t in 0..n {
                let idx = group.indices[t as usize];
                for c in 0..3 {
                    census.indices.push(base_vert.wrapping_add(idx[c as usize]));
                }
                batch_index_count = batch_index_count.saturating_add(3);
            }
            remaining_tri -= n;
            if group.next == FX_TRI_GROUP_CHAIN_NONE {
                break;
            }
            tri_handle = group.next;
        }
        let _ = r_add_mark_mesh_draw_surf(&mut census.budget, batch_index_count);
        census.surfs.push(GfxMarkMeshSurf {
            index_start: batch_index_start,
            index_count: batch_index_count,
            mark_slot,
            context: cur_context.unwrap_or([0; 7]),
        });
        Ok(())
    }

    pub fn impact_mark(&self, req: MarkImpactRequest) -> MarkImpactResult {
        if !fx_impact_mark_outer_gate(req.receivers.fx_marks, self.no_marks) {
            return MarkImpactResult {
                entered: false,
                against_world: false,
                against_models: false,
            };
        }

        MarkImpactResult {
            entered: true,
            against_world: !req.skip_world,
            against_models: fx_impact_mark_models_generate(
                req.receivers.fx_marks_ents,
                req.receivers.fx_marks_smodels,
            ),
        }
    }
}
