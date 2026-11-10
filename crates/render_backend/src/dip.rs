use crate::tess::{
    DYNAMIC_INDEX_BUFFER_CAPACITY, GfxDrawPrimArgs, GfxDynamicIndexBuffer, IndexDataAppend,
    copy_u32_indices_into_ring,
};
use crate::{SmodelRigidFlush, TrianglesListFlush, XModelRigidFlush};

pub struct ModelIndexStream {
    ring: GfxDynamicIndexBuffer,
    dest: Vec<u32>,
    closed: Vec<Vec<u32>>,
}

impl Default for ModelIndexStream {
    fn default() -> Self {
        Self::new()
    }
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
        let append = self.ring.set_index_data(count / 3);
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

pub const SMODEL_RIGID_DIP_VERTEX_COUNT: u32 = 0x2100;

pub const XMODEL_RIGID_DIP_VERTEX_COUNT: u32 = 0x1800;

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

pub fn set_index_data_source_copy(
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

pub fn set_index_data_smodel_flush(
    stream: &mut ModelIndexStream,
    source: &[u32],
    flush: SmodelRigidFlush,
) -> Result<(u32, IndexDataAppend, GfxDrawPrimArgs), ModelIndexRingCopyRefuse> {
    let (epoch, append) =
        set_index_data_source_copy(stream, source, flush.index_byte_offset, flush.tri_count)?;
    Ok((epoch, append, prim_args_from_smodel_flush(flush, append)))
}

pub fn set_index_data_xmodel_flush(
    stream: &mut ModelIndexStream,
    source: &[u32],
    flush: XModelRigidFlush,
) -> Result<(u32, IndexDataAppend, GfxDrawPrimArgs), ModelIndexRingCopyRefuse> {
    let (epoch, append) =
        set_index_data_source_copy(stream, source, flush.index_byte_offset, flush.tri_count)?;
    Ok((epoch, append, prim_args_from_xmodel_flush(flush, append)))
}
