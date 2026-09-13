use crate::tess::{
    DYNAMIC_INDEX_BUFFER_CAPACITY, GfxDrawPrimArgs, GfxDynamicIndexBuffer, IndexDataAppend,
    copy_u32_indices_into_ring,
};
use crate::{ShadowDrawListWork, SmodelRigidFlush, TrianglesListFlush, XModelRigidFlush};

pub struct ModelIndexStream {
    ring: GfxDynamicIndexBuffer,
    dest: Vec<u32>,
    closed: Vec<Vec<u32>>,
}

impl ModelIndexStream {
    pub fn new() -> Self {
        Self::with_open(0, Vec::new(), DYNAMIC_INDEX_BUFFER_CAPACITY)
    }

    pub fn with_open(cur_index_count: u32, dest: Vec<u32>, capacity: u32) -> Self {
        Self {
            ring: GfxDynamicIndexBuffer {
                cur_index_count,
                capacity,
            },
            dest,
            closed: Vec::new(),
        }
    }

    pub fn open_dest(&self) -> &[u32] {
        &self.dest
    }

    pub fn append_indices(
        &mut self,
        indices: &[u32],
    ) -> Result<(u32, IndexDataAppend), ModelIndexRingCopyRefuse> {
        let count =
            u32::try_from(indices.len()).map_err(|_| ModelIndexRingCopyRefuse::ExceedsCapacity)?;
        if count > self.ring.capacity {
            return Err(ModelIndexRingCopyRefuse::ExceedsCapacity);
        }
        if self.ring.cur_index_count.saturating_add(count) > self.ring.capacity {
            if self.ring.cur_index_count == 0 {
                return Err(ModelIndexRingCopyRefuse::ExceedsCapacity);
            }
            self.close_open_epoch();
        }
        let epoch = u32::try_from(self.closed.len()).unwrap_or(u32::MAX);
        let append = self.ring.r_set_index_data(count / 3);
        copy_u32_indices_into_ring(&mut self.dest, append.base_index, indices);
        Ok((epoch, append))
    }

    pub fn finish(mut self) -> Vec<Vec<u32>> {
        self.close_open_epoch();
        self.closed
    }

    fn close_open_epoch(&mut self) {
        if !self.dest.is_empty() {
            self.closed.push(std::mem::take(&mut self.dest));
        }
        self.ring.cur_index_count = 0;
    }
}

pub const D3DPT_TRIANGLELIST: u32 = 4;

pub const SMODEL_RIGID_DIP_VERTEX_COUNT: u32 = 0x2100;

pub const XMODEL_RIGID_DIP_VERTEX_COUNT: u32 = 0x1800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dDrawIndexedPrimitive {
    pub primitive_type: u32,
    pub base_vertex_index: i32,
    pub min_vertex_index: u32,
    pub num_vertices: u32,
    pub start_index: u32,
    pub primitive_count: u32,
}

pub fn r_draw_indexed_primitive(args: GfxDrawPrimArgs) -> D3dDrawIndexedPrimitive {
    D3dDrawIndexedPrimitive {
        primitive_type: D3DPT_TRIANGLELIST,
        base_vertex_index: 0,
        min_vertex_index: 0,
        num_vertices: args.vertex_count,
        start_index: args.base_index,
        primitive_count: args.tri_count,
    }
}

pub fn prim_args_u32_index_span(args: GfxDrawPrimArgs) -> (u32, u32) {
    (args.base_index, args.tri_count.saturating_mul(3))
}

pub fn prim_args_from_world_flush(flush: TrianglesListFlush) -> GfxDrawPrimArgs {
    GfxDrawPrimArgs {
        vertex_count: flush.vertex_count,
        tri_count: flush.tri_count,
        base_index: flush.base_index,
    }
}

pub fn smodel_source_index_span(flush: SmodelRigidFlush) -> (u32, u32) {
    index_byte_source_span(flush.index_byte_offset, flush.tri_count)
}

fn index_byte_source_span(index_byte_offset: u32, tri_count: u32) -> (u32, u32) {
    (index_byte_offset / 2, tri_count.saturating_mul(3))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelIndexRingCopyRefuse {
    SourceMissing,

    ExceedsCapacity,
}

pub fn prim_args_from_smodel_flush(
    flush: SmodelRigidFlush,
    append: IndexDataAppend,
) -> GfxDrawPrimArgs {
    GfxDrawPrimArgs {
        vertex_count: SMODEL_RIGID_DIP_VERTEX_COUNT,
        tri_count: flush.tri_count,
        base_index: append.base_index,
    }
}

pub fn prim_args_from_xmodel_flush(
    flush: XModelRigidFlush,
    append: IndexDataAppend,
) -> GfxDrawPrimArgs {
    GfxDrawPrimArgs {
        vertex_count: XMODEL_RIGID_DIP_VERTEX_COUNT,
        tri_count: flush.tri_count,
        base_index: append.base_index,
    }
}

pub fn r_set_index_data_source_copy(
    stream: &mut ModelIndexStream,
    source: &[u32],
    index_byte_offset: u32,
    tri_count: u32,
) -> Result<(u32, IndexDataAppend), ModelIndexRingCopyRefuse> {
    let (start, count) = index_byte_source_span(index_byte_offset, tri_count);
    let src = source
        .get(start as usize..start as usize + count as usize)
        .ok_or(ModelIndexRingCopyRefuse::SourceMissing)?;
    stream.append_indices(src)
}

pub fn r_set_index_data_smodel_flush(
    stream: &mut ModelIndexStream,
    source: &[u32],
    flush: SmodelRigidFlush,
) -> Result<(u32, IndexDataAppend, GfxDrawPrimArgs), ModelIndexRingCopyRefuse> {
    let (epoch, append) =
        r_set_index_data_source_copy(stream, source, flush.index_byte_offset, flush.tri_count)?;
    Ok((epoch, append, prim_args_from_smodel_flush(flush, append)))
}

pub fn r_set_index_data_xmodel_flush(
    stream: &mut ModelIndexStream,
    source: &[u32],
    flush: XModelRigidFlush,
) -> Result<(u32, IndexDataAppend, GfxDrawPrimArgs), ModelIndexRingCopyRefuse> {
    let (epoch, append) =
        r_set_index_data_source_copy(stream, source, flush.index_byte_offset, flush.tri_count)?;
    Ok((epoch, append, prim_args_from_xmodel_flush(flush, append)))
}

pub fn r_draw_indexed_from_shadow_work(work: &ShadowDrawListWork) -> Vec<D3dDrawIndexedPrimitive> {
    let mut ring = GfxDynamicIndexBuffer {
        cur_index_count: 0,
        capacity: DYNAMIC_INDEX_BUFFER_CAPACITY,
    };
    let mut out = Vec::with_capacity(
        work.world_flushes.len()
            + work.xmodel_flushes.len()
            + work.smodel_flushes.len()
            + work.smodel_pretess_flushes.len()
            + work.smodel_cached_flushes.len(),
    );
    for flush in &work.world_flushes {
        out.push(r_draw_indexed_primitive(prim_args_from_world_flush(*flush)));
    }
    for flush in &work.xmodel_flushes {
        let append = ring.r_set_index_data(flush.tri_count);
        out.push(r_draw_indexed_primitive(prim_args_from_xmodel_flush(
            *flush, append,
        )));
    }
    for flush in &work.smodel_flushes {
        let append = ring.r_set_index_data(flush.tri_count);
        out.push(r_draw_indexed_primitive(prim_args_from_smodel_flush(
            *flush, append,
        )));
    }
    for flush in &work.smodel_pretess_flushes {
        out.push(r_draw_indexed_primitive(GfxDrawPrimArgs {
            vertex_count: lighting_iw4::SMC_BANK_VERTS,
            tri_count: flush.tri_count,
            base_index: flush.index_byte_offset / 2,
        }));
    }
    for flush in &work.smodel_cached_flushes {
        let append = ring.r_set_index_data(flush.tri_count);
        out.push(r_draw_indexed_primitive(prim_args_from_smodel_flush(
            *flush, append,
        )));
    }
    out
}
