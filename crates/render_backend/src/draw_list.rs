use render_frame::FRONTEND_DRAW_LISTS_DWORDS;

pub const DRAW_LIST_ENTRY_STRIDE: usize = 0x10;

pub const ENTRY_KIND: usize = 0x00;
pub const ENTRY_SORT_KEY: usize = 0x04;
pub const ENTRY_PEEK_FN: usize = 0x08;
pub const ENTRY_WORK_FN: usize = 0x0c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxDrawSurfListKind {
    pub id: u8,

    pub cursor: u16,
}

pub const LIST_BSP_SURFACES: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 0,
    cursor: 0x08,
};

pub const LIST_WORLD_DRAWSURFS: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 0,
    cursor: 0x14,
};

pub const LIST_KIND_1: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 1,
    cursor: 0x24,
};

pub const LIST_KIND_2: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 2,
    cursor: 0x44,
};

pub const LIST_KIND_3: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 3,
    cursor: 0x54,
};
pub const LIST_KIND_4: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 4,
    cursor: 0x60,
};

pub const LIST_KIND_5: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 5,
    cursor: 0x34,
};

pub const LIST_CODE_MESH: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 6,
    cursor: 0x6c,
};

pub const LIST_KIND_7: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 7,
    cursor: 0x74,
};

pub const LIST_XMODEL_RIGID: GfxDrawSurfListKind = LIST_KIND_7;

pub const LIST_STATIC_MODEL_CACHED: GfxDrawSurfListKind = LIST_KIND_2;

pub const LIST_STATIC_MODEL_PRETESS: GfxDrawSurfListKind = LIST_KIND_3;

pub const LIST_STATIC_MODEL_SKINNED: GfxDrawSurfListKind = LIST_KIND_5;
pub const LIST_STATIC_MODEL_RIGID: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 8,
    cursor: 0x80,
};
pub const LIST_KIND_9: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 9,
    cursor: 0x8c,
};
pub const LIST_KIND_10: GfxDrawSurfListKind = GfxDrawSurfListKind {
    id: 10,
    cursor: 0x94,
};

pub const DRAW_LIST_REGISTRATION_ORDER: [GfxDrawSurfListKind; 12] = [
    LIST_BSP_SURFACES,
    LIST_WORLD_DRAWSURFS,
    LIST_KIND_1,
    LIST_KIND_2,
    LIST_KIND_3,
    LIST_KIND_4,
    LIST_KIND_5,
    LIST_CODE_MESH,
    LIST_KIND_7,
    LIST_STATIC_MODEL_RIGID,
    LIST_KIND_9,
    LIST_KIND_10,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawListRegistrationGuard {
    Always,

    EmittersOrMode2,

    Emitters,
}

impl GfxDrawSurfListKind {
    pub const fn guard(&self) -> DrawListRegistrationGuard {
        match self.cursor {
            0x08 | 0x14 => DrawListRegistrationGuard::Always,
            0x24 | 0x44 | 0x54 | 0x60 | 0x34 => DrawListRegistrationGuard::EmittersOrMode2,
            _ => DrawListRegistrationGuard::Emitters,
        }
    }
}

pub struct GfxDrawListEntry<'a> {
    pub kind: GfxDrawSurfListKind,

    pub sort_key: u32,

    pub worker: &'a mut dyn DrawSurfListWorker,
}

pub trait DrawSurfListWorker {
    fn work(&mut self, ctx: &mut GfxCmdBufContext) -> bool;

    fn peek_sort_key(&self) -> u32;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxCmdBufContext {
    pub view_info: u32,

    pub arg1: u32,

    pub tech_type_src: u32,
}

pub const LIST_ARGS_DWORDS: usize = (0x90 / 4) + 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GfxDrawList {
    pub head_cur: u32,

    pub registered: u32,

    pub list_args: [u32; LIST_ARGS_DWORDS],

    pub registered_kinds: Vec<GfxDrawSurfListKind>,
}

impl Default for GfxDrawList {
    fn default() -> Self {
        Self {
            head_cur: 0,
            registered: 0,
            list_args: [0; LIST_ARGS_DWORDS],
            registered_kinds: Vec::new(),
        }
    }
}

impl GfxDrawList {
    pub fn cursor_pair(&self, kind: GfxDrawSurfListKind) -> (u32, u32) {
        let i = list_args_index(kind.cursor);
        (self.list_args[i], self.list_args[i + 1])
    }

    pub fn is_nonempty(&self, kind: GfxDrawSurfListKind) -> bool {
        let (cur, end) = self.cursor_pair(kind);
        cur != end
    }
}

pub const fn list_args_index(cursor: u16) -> usize {
    (cursor as usize - 8) / 4
}

pub fn r_init_draw_surf_list_args(
    src: &[u32; FRONTEND_DRAW_LISTS_DWORDS],
    dest: &mut [u32; LIST_ARGS_DWORDS],
) {
    let count0 = src[0];
    if count0 < 2 {
        dest[0] = 0;
        dest[1] = 0;
    } else {
        dest[0] = src[1];
        dest[1] = src[1].wrapping_add(count0.wrapping_mul(2)).wrapping_sub(2);
    }
    dest[0x0c / 4] = src[0x08 / 4];
    dest[0x10 / 4] = src[0x08 / 4].wrapping_add(src[0x0c / 4].wrapping_mul(0x18));
    dest[0x18 / 4] = src[0x18 / 4];
    dest[0x1c / 4] = src[0x14 / 4];
    dest[0x20 / 4] = src[0x10 / 4].wrapping_add(src[0x14 / 4]);
    dest[0x28 / 4] = src[0x24 / 4];
    dest[0x2c / 4] = src[0x20 / 4];
    dest[0x30 / 4] = src[0x1c / 4].wrapping_add(src[0x20 / 4]);
    dest[0x38 / 4] = src[0x30 / 4];
    dest[0x3c / 4] = src[0x2c / 4];
    dest[0x40 / 4] = src[0x28 / 4].wrapping_add(src[0x2c / 4]);
    dest[0x48 / 4] = src[0x3c / 4];
    dest[0x4c / 4] = src[0x38 / 4];
    dest[0x50 / 4] = src[0x34 / 4].wrapping_add(src[0x38 / 4]);
    dest[0x58 / 4] = src[0x40 / 4];
    dest[0x5c / 4] = src[0x40 / 4].wrapping_add(src[0x44 / 4].wrapping_mul(8));
    dest[0x64 / 4] = src[0x48 / 4];
    dest[0x68 / 4] = src[0x48 / 4].wrapping_add(src[0x4c / 4].wrapping_mul(0x10));
    dest[0x6c / 4] = src[0x50 / 4];
    dest[0x70 / 4] = src[0x50 / 4].wrapping_add(src[0x54 / 4].wrapping_mul(0x10));
    dest[0x78 / 4] = src[0x58 / 4];
    dest[0x7c / 4] = src[0x58 / 4].wrapping_add(src[0x5c / 4].wrapping_mul(0x10));
    dest[0x84 / 4] = src[0x64 / 4];
    dest[0x88 / 4] = src[0x64 / 4].wrapping_add(src[0x68 / 4].wrapping_mul(4));
    dest[0x8c / 4] = src[0x6c / 4];
    dest[0x90 / 4] = src[0x6c / 4].wrapping_add(src[0x70 / 4].wrapping_mul(4));
}

pub fn r_setup_draw_list(
    src: &[u32; FRONTEND_DRAW_LISTS_DWORDS],
    draw_list_info_nonzero: bool,
    mode2: bool,
) -> GfxDrawList {
    let mut list = GfxDrawList::default();
    r_init_draw_surf_list_args(src, &mut list.list_args);
    list.registered = 0;
    for kind in DRAW_LIST_REGISTRATION_ORDER {
        let allow = match kind.guard() {
            DrawListRegistrationGuard::Always => true,
            DrawListRegistrationGuard::EmittersOrMode2 => draw_list_info_nonzero || mode2,
            DrawListRegistrationGuard::Emitters => draw_list_info_nonzero,
        };
        if allow && list.is_nonempty(kind) {
            list.registered += 1;
            list.registered_kinds.push(kind);
        }
    }
    list.head_cur = 0;
    list
}

pub fn r_dispatch_draw_surf_list_unsorted(
    entries: &mut [GfxDrawListEntry<'_>],
    ctx: &mut GfxCmdBufContext,
) {
    for entry in entries.iter_mut() {
        while entry.worker.work(ctx) {}
    }
}

pub type GfxDrawListHeapRecord = [u32; 4];

#[inline]
#[must_use]
pub const fn draw_list_record_precedes(
    left: &GfxDrawListHeapRecord,
    right: &GfxDrawListHeapRecord,
) -> bool {
    left[1] < right[1] || (left[1] == right[1] && left[0] < right[0])
}

pub fn draw_list_heap_sift(heap: &mut [GfxDrawListHeapRecord], n: usize) {
    if n < 2 || heap.len() < n {
        return;
    }
    let e0 = heap[0];
    let e1 = heap[1];
    if !draw_list_record_precedes(&e1, &e0) {
        return;
    }
    heap[0] = e1;
    let mut i = 2usize;
    if 2 < n {
        while i < n {
            let cur = heap[i];
            if e0[1] < cur[1] || (e0[1] == cur[1] && e0[0] <= cur[0]) {
                break;
            }
            heap[i - 1] = cur;
            i += 1;
        }
    }
    heap[i - 1] = e0;
}

pub fn draw_list_heap_build(heap: &mut [GfxDrawListHeapRecord], n: usize) {
    if n < 2 || heap.len() < n {
        return;
    }
    let mut i = n - 2;
    loop {
        draw_list_heap_sift(&mut heap[i..], n - i);
        if i == 0 {
            break;
        }
        i -= 1;
    }
}

pub fn r_dispatch_draw_list_records(
    recs: &mut [GfxDrawListHeapRecord],
    sorted: bool,
    ctx: &mut GfxCmdBufContext,
    mut step: impl FnMut(usize, &mut GfxCmdBufContext) -> Option<(u32, u32)>,
) {
    let n = recs.len();
    if n == 0 {
        return;
    }
    if !sorted {
        for rec in recs.iter() {
            let idx = rec[2] as usize;
            while step(idx, ctx).is_some() {}
        }
        return;
    }
    if n > 1 {
        draw_list_heap_build(recs, n);
    }
    let mut start = 0usize;
    let mut remaining = n;
    if remaining > 1 {
        loop {
            loop {
                let idx = recs[start][2] as usize;
                let Some((key, kind)) = step(idx, ctx) else {
                    break;
                };
                recs[start][1] = key;
                recs[start][0] = kind;
                draw_list_heap_sift(&mut recs[start..], remaining);
            }
            start += 1;
            remaining -= 1;
            if remaining == 1 {
                break;
            }
        }
    }
    if remaining == 1 {
        let idx = recs[start][2] as usize;
        while step(idx, ctx).is_some() {}
    }
}

pub fn r_dispatch_draw_surf_list_sorted(
    entries: &mut [GfxDrawListEntry<'_>],
    ctx: &mut GfxCmdBufContext,
) {
    let n = entries.len();
    if n == 0 {
        return;
    }
    let mut recs: Vec<GfxDrawListHeapRecord> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| [u32::from(e.kind.id), e.sort_key, i as u32, 0])
        .collect();
    if n > 1 {
        draw_list_heap_build(&mut recs, n);
    }
    let mut start = 0usize;
    let mut remaining = n;
    if remaining > 1 {
        loop {
            loop {
                let idx = recs[start][2] as usize;
                if !entries[idx].worker.work(ctx) {
                    break;
                }
                recs[start][1] = entries[idx].worker.peek_sort_key();
                recs[start][0] = u32::from(entries[idx].kind.id);
                draw_list_heap_sift(&mut recs[start..], remaining);
            }
            start += 1;
            remaining -= 1;
            if remaining == 1 {
                break;
            }
        }
    }
    if remaining == 1 {
        let idx = recs[start][2] as usize;
        while entries[idx].worker.work(ctx) {}
    }
}

pub fn r_bind_draw_list_context(view_info: u32, arg1: u32, tech_type_src: u32) -> GfxCmdBufContext {
    GfxCmdBufContext {
        view_info,
        arg1,
        tech_type_src,
    }
}

pub const DEPTH_RANGE_BAND: f32 = 0.015_625;

pub const GFX_DEPTH_RANGE_SCENE: i32 = 0;

pub const GFX_DEPTH_RANGE_FULL: i32 = -1;

#[derive(Clone, Debug, PartialEq)]
pub struct GfxCmdBufDepthState {
    pub prim_draw: [u32; 3],

    pub invert_projection: u32,

    pub camera_view: u32,

    pub depth_range_type: i32,

    pub depth_min: f32,

    pub depth_max: f32,

    pub projection_scale: f32,
}

impl Default for GfxCmdBufDepthState {
    fn default() -> Self {
        Self {
            prim_draw: [0; 3],
            invert_projection: 0,
            camera_view: 0,
            depth_range_type: GFX_DEPTH_RANGE_SCENE,
            depth_min: 0.0,
            depth_max: 0.0,
            projection_scale: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxEndDrawList {
    pub restore_n: u32,

    pub depth_range_type: i32,
}

pub fn camera_view_depth_range_type(camera_view: u32) -> i32 {
    if camera_view != 0 {
        GFX_DEPTH_RANGE_SCENE
    } else {
        GFX_DEPTH_RANGE_FULL
    }
}

pub fn r_change_depth_range(state: &mut GfxCmdBufDepthState, depth_range_type: i32) {
    state.depth_range_type = depth_range_type;
    if depth_range_type == GFX_DEPTH_RANGE_SCENE {
        state.depth_min = DEPTH_RANGE_BAND;
        state.depth_max = 1.0;
    } else {
        state.depth_min = 0.0;
        state.depth_max = DEPTH_RANGE_BAND;
    }
}

pub fn r_end_draw_list_shadow() -> GfxEndDrawList {
    let mut cmd = GfxCmdBufDepthState::default();
    let mut state = GfxCmdBufDepthState::default();
    r_end_draw_list(&mut cmd, &mut state, None)
}

pub fn r_end_draw_list(
    cmd: &mut GfxCmdBufDepthState,
    state: &mut GfxCmdBufDepthState,
    pre: Option<(&mut GfxCmdBufDepthState, &mut GfxCmdBufDepthState)>,
) -> GfxEndDrawList {
    cmd.prim_draw = [0; 3];
    if cmd.invert_projection != 0 {
        cmd.projection_scale = -cmd.projection_scale;
        cmd.invert_projection = 0;
    }
    let mut restore_n = 0u32;
    let want = camera_view_depth_range_type(cmd.camera_view);
    if want != state.depth_range_type {
        r_change_depth_range(state, want);
        restore_n = restore_n.saturating_add(1);
    }
    if let Some((pre_cmd, pre_state)) = pre {
        let want2 = camera_view_depth_range_type(pre_cmd.camera_view);
        if want2 != pre_state.depth_range_type {
            r_change_depth_range(pre_state, want2);
            restore_n = restore_n.saturating_add(1);
        }
    }
    GfxEndDrawList {
        restore_n,
        depth_range_type: state.depth_range_type,
    }
}
