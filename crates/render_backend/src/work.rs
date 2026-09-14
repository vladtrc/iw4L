use std::cell::RefCell;

use crate::draw_list::{
    DrawSurfListWorker, GfxCmdBufContext, GfxCmdBufDepthState, GfxDrawList, GfxDrawListHeapRecord,
    LIST_STATIC_MODEL_CACHED, LIST_STATIC_MODEL_PRETESS, LIST_STATIC_MODEL_RIGID,
    LIST_STATIC_MODEL_SKINNED, LIST_WORLD_DRAWSURFS, LIST_XMODEL_RIGID, r_bind_draw_list_context,
    r_dispatch_draw_list_records, r_end_draw_list, r_end_draw_list_shadow, r_setup_draw_list,
};
use crate::tess_list::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, SmodelRigidFlush,
    TrianglesListFlush, XModelRigidFlush, r_tess_static_model_rigid_draw_surf_lighting,
    r_tess_static_model_rigid_draw_surf_list, r_tess_triangles_list_generic,
    r_tess_xmodel_rigid_draw_surf_lighting, smodel_rigid_list_step,
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
    pub world_work_calls: u32,
    pub xmodel_work_calls: u32,
    pub smodel_work_calls: u32,

    pub ended: bool,

    pub end_restore_n: u32,

    pub end_depth_range_type: i32,

    pub emit_order: Vec<PackedEmit>,
}

pub type ColourDrawListWork = ShadowDrawListWork;

struct WorldListWorker<'a> {
    entries: &'a [GfxTrianglesListEntry],
    cur: usize,
    flushes: &'a mut Vec<TrianglesListFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
    calls: u32,
}

impl DrawSurfListWorker for WorldListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        self.calls += 1;
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
            r_tess_triangles_list_generic(&self.entries[self.cur..end], |_, _| false)
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
        self.entries.get(self.cur).map(|e| e.sort_key).unwrap_or(0)
    }
}

struct SmodelListWorker<'a> {
    entries: &'a [GfxSmodelRigidEntry],
    cur: usize,
    flushes: &'a mut Vec<SmodelRigidFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
    kind: PackedListKind,
    calls: u32,
}

impl DrawSurfListWorker for SmodelListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        self.calls += 1;
        if self.cur >= self.entries.len() {
            return false;
        }
        let step = r_tess_static_model_rigid_draw_surf_list(self.entries, self.cur, true);
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
            r_tess_static_model_rigid_draw_surf_lighting(&self.entries[self.cur..step.cur], false)
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
            .map(|e| e.packed_key)
            .unwrap_or(0)
    }
}

struct XModelListWorker<'a> {
    entries: &'a [GfxXModelRigidEntry],
    cur: usize,
    flushes: &'a mut Vec<XModelRigidFlush>,
    emit: Option<&'a RefCell<Vec<PackedEmit>>>,
    calls: u32,
}

impl DrawSurfListWorker for XModelListWorker<'_> {
    fn work(&mut self, _ctx: &mut GfxCmdBufContext) -> bool {
        self.calls += 1;
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
            r_tess_xmodel_rigid_draw_surf_lighting(&self.entries[self.cur..step.cur], false)
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
            .map(|e| e.packed_key)
            .unwrap_or(0)
    }
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

fn dispatch_registered(
    list: &GfxDrawList,
    ctx: &mut GfxCmdBufContext,
    sorted: bool,
    world: &mut WorldListWorker<'_>,
    xmodel: &mut XModelListWorker<'_>,
    smodel: &mut SmodelListWorker<'_>,
    cached: &mut SmodelListWorker<'_>,
    pretess: &mut SmodelListWorker<'_>,
    skinned: &mut SmodelListWorker<'_>,
) {
    let mut which_slots: Vec<Which> = Vec::with_capacity(list.registered_kinds.len());
    let mut kind_ids: Vec<u32> = Vec::with_capacity(list.registered_kinds.len());
    let mut recs: Vec<GfxDrawListHeapRecord> = Vec::with_capacity(list.registered_kinds.len());
    for kind in &list.registered_kinds {
        let Some(which) = which_for_cursor(kind.cursor) else {
            continue;
        };
        let key = match which {
            Which::World => world.peek_sort_key(),
            Which::XModel => xmodel.peek_sort_key(),
            Which::Smodel => smodel.peek_sort_key(),
            Which::Cached => cached.peek_sort_key(),
            Which::Pretess => pretess.peek_sort_key(),
            Which::Skinned => skinned.peek_sort_key(),
        };
        recs.push([u32::from(kind.id), key, which_slots.len() as u32, 0]);
        kind_ids.push(u32::from(kind.id));
        which_slots.push(which);
    }
    r_dispatch_draw_list_records(&mut recs, sorted, ctx, |i, ctx| {
        let more = match which_slots[i] {
            Which::World => world.work(ctx),
            Which::XModel => xmodel.work(ctx),
            Which::Smodel => smodel.work(ctx),
            Which::Cached => cached.work(ctx),
            Which::Pretess => pretess.work(ctx),
            Which::Skinned => skinned.work(ctx),
        };
        if !more {
            return None;
        }
        let key = match which_slots[i] {
            Which::World => world.peek_sort_key(),
            Which::XModel => xmodel.peek_sort_key(),
            Which::Smodel => smodel.peek_sort_key(),
            Which::Cached => cached.peek_sort_key(),
            Which::Pretess => pretess.peek_sort_key(),
            Which::Skinned => skinned.peek_sort_key(),
        };
        Some((key, kind_ids[i]))
    });
}

pub fn r_draw_surf_list_work_shadow(packed: &PackedFrontendLists) -> ShadowDrawListWork {
    let list = r_setup_draw_list(&packed.src, true, false);

    let ctx = r_bind_draw_list_context(0, 0, 3);
    run_packed_work(packed, list, ctx, false)
}

pub fn r_draw_surf_list_work_colour(packed: &PackedFrontendLists) -> ColourDrawListWork {
    let list = r_setup_draw_list(&packed.src, true, false);
    let ctx = r_bind_draw_list_context(0, 0, 0);
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
    let mut world = WorldListWorker {
        entries: &packed.world,
        cur: 0,
        flushes: &mut world_flushes,
        emit,
        calls: 0,
    };
    let mut xmodel = XModelListWorker {
        entries: &packed.xmodel,
        cur: 0,
        flushes: &mut xmodel_flushes,
        emit,
        calls: 0,
    };
    let mut smodel = SmodelListWorker {
        entries: &packed.smodel,
        cur: 0,
        flushes: &mut smodel_flushes,
        emit,
        kind: PackedListKind::Smodel,
        calls: 0,
    };
    let mut pretess = SmodelListWorker {
        entries: &packed.smodel_pretess,
        cur: 0,
        flushes: &mut smodel_pretess_flushes,
        emit,
        kind: PackedListKind::Pretess,
        calls: 0,
    };
    let mut cached = SmodelListWorker {
        entries: &packed.smodel_cached,
        cur: 0,
        flushes: &mut smodel_cached_flushes,
        emit,
        kind: PackedListKind::Cached,
        calls: 0,
    };
    let mut smodel_skinned_flushes = Vec::new();
    let mut skinned = SmodelListWorker {
        entries: &packed.smodel_skinned,
        cur: 0,
        flushes: &mut smodel_skinned_flushes,
        emit,
        kind: PackedListKind::SmodelSkinned,
        calls: 0,
    };
    dispatch_registered(
        &list,
        &mut ctx,
        sorted,
        &mut world,
        &mut xmodel,
        &mut smodel,
        &mut cached,
        &mut pretess,
        &mut skinned,
    );
    let world_work_calls = world.calls;
    let xmodel_work_calls = xmodel.calls;
    let smodel_work_calls = smodel.calls;
    let smodel_skinned_unconsumed = 0u32;
    let end = if sorted {
        let mut cmd = GfxCmdBufDepthState {
            camera_view: 1,
            ..GfxCmdBufDepthState::default()
        };
        let mut state = GfxCmdBufDepthState::default();
        r_end_draw_list(&mut cmd, &mut state, None)
    } else {
        r_end_draw_list_shadow()
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
        world_work_calls,
        xmodel_work_calls,
        smodel_work_calls,
        ended: true,
        end_restore_n: end.restore_n,
        end_depth_range_type: end.depth_range_type,
        emit_order: emit_cell.into_inner(),
    }
}
