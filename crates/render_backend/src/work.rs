use std::cell::RefCell;

use crate::draw_list::{
    DrawSurfListWorker, GfxCmdBufContext, GfxCmdBufDepthState, GfxDrawList, GfxDrawListHeapRecord,
    LIST_STATIC_MODEL_CACHED, LIST_STATIC_MODEL_PRETESS, LIST_STATIC_MODEL_RIGID,
    LIST_STATIC_MODEL_SKINNED, LIST_WORLD_DRAWSURFS, LIST_XMODEL_RIGID, bind_draw_list_context,
    dispatch_draw_list_records, end_draw_list, end_draw_list_shadow, setup_draw_list,
};
use crate::tess_list::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, SmodelRigidFlush,
    TrianglesListFlush, XModelRigidFlush, smodel_rigid_list_step,
    tess_static_model_rigid_draw_surf_lighting, tess_static_model_rigid_draw_surf_list,
    tess_triangles_list_generic, tess_xmodel_rigid_draw_surf_lighting,
};
use render_frame::PackedFrontendLists;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedListKind {
    World,
    XModel,
    Smodel,
    Cached,
    Pretess,
    SmodelSkinned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedEmit {
    pub list: PackedListKind,
    pub entry: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ShadowDrawListWork {
    pub list: GfxDrawList,
    pub ctx: GfxCmdBufContext,
    pub world_flushes: Vec<TrianglesListFlush>,
    pub xmodel_flushes: Vec<XModelRigidFlush>,
    pub smodel_flushes: Vec<SmodelRigidFlush>,
    pub smodel_cached_flushes: Vec<SmodelRigidFlush>,
    pub smodel_pretess_flushes: Vec<SmodelRigidFlush>,
    pub smodel_skinned_flushes: Vec<SmodelRigidFlush>,

    pub smodel_skinned_unconsumed: u32,

    pub ended: bool,

    pub end_restore_n: u32,

    pub end_depth_range_type: i32,

    pub emit_order: Vec<PackedEmit>,
}

pub type ColourDrawListWork = ShadowDrawListWork;

struct WorldListWorker<'a> {
    entries: &'a [GfxTrianglesListEntry],
    sort_keys: &'a [u8],
    cur: usize,
    flushes: &'a mut Vec<TrianglesListFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
}

impl DrawSurfListWorker for WorldListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        if self.cur >= self.entries.len() {
            return false;
        }
        let key = self.entries[self.cur].sort_key;
        let mut end = self.cur + 1;
        while end < self.entries.len() && self.entries[end].sort_key == key {
            end += 1;
        }
        if let Some(log) = self.emit {
            let mut log = log.borrow_mut();
            for i in self.cur..end {
                log.push(PackedEmit {
                    list: PackedListKind::World,
                    entry: i as u32,
                });
            }
        }
        let base = self.cur as u32;
        self.flushes.extend(
            tess_triangles_list_generic(&self.entries[self.cur..end], |_, _| false)
                .into_iter()
                .map(|mut flush| {
                    flush.entry_start = flush.entry_start.saturating_add(base);
                    flush
                }),
        );
        self.cur = end;
        self.cur != self.entries.len()
    }

    fn peek_sort_key(&self) -> u32 {
        self.entries
            .get(self.cur)
            .map_or(0, |e| rank_sort_key(self.sort_keys, e.sort_key))
    }
}

struct SmodelListWorker<'a> {
    entries: &'a [GfxSmodelRigidEntry],
    sort_keys: &'a [u8],
    cur: usize,
    flushes: &'a mut Vec<SmodelRigidFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
    kind: PackedListKind,
}

impl DrawSurfListWorker for SmodelListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        if self.cur >= self.entries.len() {
            return false;
        }
        let step = tess_static_model_rigid_draw_surf_list(self.entries, self.cur, true);
        if let Some(log) = self.emit {
            let mut log = log.borrow_mut();
            for i in self.cur..step.cur {
                log.push(PackedEmit {
                    list: self.kind,
                    entry: i as u32,
                });
            }
        }
        let base = self.cur as u32;
        self.flushes.extend(
            tess_static_model_rigid_draw_surf_lighting(&self.entries[self.cur..step.cur], false)
                .into_iter()
                .map(|mut flush| {
                    flush.entry_start = flush.entry_start.saturating_add(base);
                    flush
                }),
        );
        self.cur = step.cur;
        step.more
    }

    fn peek_sort_key(&self) -> u32 {
        self.entries
            .get(self.cur)
            .map_or(0, |e| rank_sort_key(self.sort_keys, e.packed_key))
    }
}

struct XModelListWorker<'a> {
    entries: &'a [GfxXModelRigidEntry],
    sort_keys: &'a [u8],
    cur: usize,
    flushes: &'a mut Vec<XModelRigidFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
}

impl DrawSurfListWorker for XModelListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        if self.cur >= self.entries.len() {
            return false;
        }
        let step = smodel_rigid_list_step(self.entries, self.cur, true);
        if let Some(log) = self.emit {
            let mut log = log.borrow_mut();
            for i in self.cur..step.cur {
                log.push(PackedEmit {
                    list: PackedListKind::XModel,
                    entry: i as u32,
                });
            }
        }
        let base = self.cur as u32;
        self.flushes.extend(
            tess_xmodel_rigid_draw_surf_lighting(&self.entries[self.cur..step.cur], false)
                .into_iter()
                .map(|mut flush| {
                    flush.entry_start = flush.entry_start.saturating_add(base);
                    flush
                }),
        );
        self.cur = step.cur;
        step.more
    }

    fn peek_sort_key(&self) -> u32 {
        self.entries
            .get(self.cur)
            .map_or(0, |e| rank_sort_key(self.sort_keys, e.packed_key))
    }
}

// Merge on the primary sort key, not the batching key: its surf_type bits would
// draw every smodel before a lower-sort xmodel such as the T6 sky.
fn rank_sort_key(sort_keys: &[u8], key: u32) -> u32 {
    sort_keys
        .get((key & 0x7fff) as usize)
        .copied()
        .map_or(0, u32::from)
}

#[derive(Clone, Copy)]
enum Which {
    World,
    XModel,
    Smodel,
    Cached,
    Pretess,
    Skinned,
}

fn which_for_cursor(cursor: u16) -> Option<Which> {
    if cursor == LIST_WORLD_DRAWSURFS.cursor {
        Some(Which::World)
    } else if cursor == LIST_XMODEL_RIGID.cursor {
        Some(Which::XModel)
    } else if cursor == LIST_STATIC_MODEL_RIGID.cursor {
        Some(Which::Smodel)
    } else if cursor == LIST_STATIC_MODEL_CACHED.cursor {
        Some(Which::Cached)
    } else if cursor == LIST_STATIC_MODEL_PRETESS.cursor {
        Some(Which::Pretess)
    } else if cursor == LIST_STATIC_MODEL_SKINNED.cursor {
        Some(Which::Skinned)
    } else {
        None
    }
}

struct DrawListWorkers<'a> {
    world: WorldListWorker<'a>,
    xmodel: XModelListWorker<'a>,
    smodel: SmodelListWorker<'a>,
    cached: SmodelListWorker<'a>,
    pretess: SmodelListWorker<'a>,
    skinned: SmodelListWorker<'a>,
}

impl DrawListWorkers<'_> {
    fn peek_sort_key(&self, which: Which) -> u32 {
        match which {
            Which::World => self.world.peek_sort_key(),
            Which::XModel => self.xmodel.peek_sort_key(),
            Which::Smodel => self.smodel.peek_sort_key(),
            Which::Cached => self.cached.peek_sort_key(),
            Which::Pretess => self.pretess.peek_sort_key(),
            Which::Skinned => self.skinned.peek_sort_key(),
        }
    }

    fn work(&mut self, which: Which, ctx: &mut GfxCmdBufContext) -> bool {
        match which {
            Which::World => self.world.work(ctx),
            Which::XModel => self.xmodel.work(ctx),
            Which::Smodel => self.smodel.work(ctx),
            Which::Cached => self.cached.work(ctx),
            Which::Pretess => self.pretess.work(ctx),
            Which::Skinned => self.skinned.work(ctx),
        }
    }

    fn dispatch_registered(
        &mut self,
        list: &GfxDrawList,
        ctx: &mut GfxCmdBufContext,
        sorted: bool,
    ) {
        let mut which_slots: Vec<Which> = Vec::with_capacity(list.registered_kinds.len());
        let mut kind_ids: Vec<u32> = Vec::with_capacity(list.registered_kinds.len());
        let mut recs: Vec<GfxDrawListHeapRecord> = Vec::with_capacity(list.registered_kinds.len());
        for kind in &list.registered_kinds {
            let Some(which) = which_for_cursor(kind.cursor) else {
                continue;
            };
            let key = self.peek_sort_key(which);
            recs.push([u32::from(kind.id), key, which_slots.len() as u32, 0]);
            kind_ids.push(u32::from(kind.id));
            which_slots.push(which);
        }
        dispatch_draw_list_records(&mut recs, sorted, ctx, |i, ctx| {
            let more = self.work(which_slots[i], ctx);
            if !more {
                return None;
            }
            let key = self.peek_sort_key(which_slots[i]);
            Some((key, kind_ids[i]))
        });
    }
}

pub fn draw_surf_list_work_shadow(packed: &PackedFrontendLists) -> ShadowDrawListWork {
    let list = setup_draw_list(&packed.src, true, false);

    let ctx = bind_draw_list_context(0, 0, 3);
    run_packed_work(packed, list, ctx, false)
}

pub fn draw_surf_list_work_colour(packed: &PackedFrontendLists) -> ColourDrawListWork {
    let list = setup_draw_list(&packed.src, true, false);
    let ctx = bind_draw_list_context(0, 0, 0);
    run_packed_work(packed, list, ctx, true)
}

fn run_packed_work(
    packed: &PackedFrontendLists,
    list: GfxDrawList,
    mut ctx: GfxCmdBufContext,
    sorted: bool,
) -> ShadowDrawListWork {
    let mut world_flushes = Vec::new();
    let mut xmodel_flushes = Vec::new();
    let mut smodel_flushes = Vec::new();
    let mut smodel_cached_flushes = Vec::new();
    let mut smodel_pretess_flushes = Vec::new();
    let emit_cell = RefCell::new(Vec::new());
    let emit = if sorted { Some(&emit_cell) } else { None };
    let world = WorldListWorker {
        entries: &packed.world,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut world_flushes,
        emit,
    };
    let xmodel = XModelListWorker {
        entries: &packed.xmodel,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut xmodel_flushes,
        emit,
    };
    let smodel = SmodelListWorker {
        entries: &packed.smodel,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut smodel_flushes,
        emit,
        kind: PackedListKind::Smodel,
    };
    let pretess = SmodelListWorker {
        entries: &packed.smodel_pretess,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut smodel_pretess_flushes,
        emit,
        kind: PackedListKind::Pretess,
    };
    let cached = SmodelListWorker {
        entries: &packed.smodel_cached,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut smodel_cached_flushes,
        emit,
        kind: PackedListKind::Cached,
    };
    let mut smodel_skinned_flushes = Vec::new();
    let skinned = SmodelListWorker {
        entries: &packed.smodel_skinned,
        sort_keys: &packed.sort_key_by_rank,
        cur: 0,
        flushes: &mut smodel_skinned_flushes,
        emit,
        kind: PackedListKind::SmodelSkinned,
    };
    let mut workers = DrawListWorkers {
        world,
        xmodel,
        smodel,
        cached,
        pretess,
        skinned,
    };
    workers.dispatch_registered(&list, &mut ctx, sorted);
    let smodel_skinned_unconsumed = 0u32;
    let end = if sorted {
        let mut cmd = GfxCmdBufDepthState {
            camera_view: 1,
            ..GfxCmdBufDepthState::default()
        };
        let mut state = GfxCmdBufDepthState::default();
        end_draw_list(&mut cmd, &mut state, None)
    } else {
        end_draw_list_shadow()
    };
    ShadowDrawListWork {
        list,
        ctx,
        world_flushes,
        xmodel_flushes,
        smodel_flushes,
        smodel_cached_flushes,
        smodel_pretess_flushes,
        smodel_skinned_flushes,
        smodel_skinned_unconsumed,
        ended: true,
        end_restore_n: end.restore_n,
        end_depth_range_type: end.depth_range_type,
        emit_order: emit_cell.into_inner(),
    }
}
