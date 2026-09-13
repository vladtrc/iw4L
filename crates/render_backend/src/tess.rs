pub const GFX_TESS_VERTEX_STRIDE: u32 = 0x20;

pub const DYNAMIC_TESSELLATION_VB_CAPACITY: u32 = 0x100000;

pub const TESS_VB_LOCK_DISCARD: u32 = 0x2000;

pub const TESS_VB_LOCK_NOOVERWRITE: u32 = 0x1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxDrawPrimArgs {
    pub vertex_count: u32,

    pub tri_count: u32,

    pub base_index: u32,
}

pub const DYNAMIC_INDEX_BUFFER_CAPACITY: u32 = 0x100000;

pub fn copy_u32_indices_into_ring(dest: &mut Vec<u32>, base_index: u32, source: &[u32]) {
    let start = base_index as usize;
    let end = start.saturating_add(source.len());
    if dest.len() < end {
        dest.resize(end, 0);
    }
    dest[start..end].copy_from_slice(source);
}

pub fn copy_u16_indices_into_ring(dest: &mut Vec<u16>, base_index: u32, source: &[u16]) {
    let start = base_index as usize;
    let end = start.saturating_add(source.len());
    if dest.len() < end {
        dest.resize(end, 0);
    }
    dest[start..end].copy_from_slice(source);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxDynamicIndexBuffer {
    pub cur_index_count: u32,

    pub capacity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexDataAppend {
    pub base_index: u32,

    pub lock_byte_offset: u32,

    pub lock_byte_len: u32,

    pub discard: bool,
}

impl GfxDynamicIndexBuffer {
    pub fn r_set_index_data(&mut self, tri_count: u32) -> IndexDataAppend {
        let index_count = tri_count * 3;
        let discard = self.cur_index_count + index_count > self.capacity;
        if discard {
            self.cur_index_count = 0;
        }
        let base_index = self.cur_index_count;
        self.cur_index_count += index_count;
        IndexDataAppend {
            base_index,
            lock_byte_offset: base_index * 2,
            lock_byte_len: tri_count * 6,
            discard,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GfxTess {
    pub vertex_count: u32,

    pub index_count: u32,

    pub indices: Vec<u16>,

    pub pending_prim_count: u32,

    pub pending_prim_base: u32,
}

impl GfxTess {
    pub fn rb_end_tess_surface(
        &mut self,
        ring: &mut GfxDynamicIndexBuffer,
    ) -> Option<(GfxDrawPrimArgs, IndexDataAppend)> {
        if self.vertex_count == 0 && self.index_count == 0 {
            return None;
        }
        let tri_count = self.index_count / 3;
        let append = ring.r_set_index_data(tri_count);
        let args = GfxDrawPrimArgs {
            vertex_count: self.vertex_count,
            tri_count,
            base_index: append.base_index,
        };
        self.vertex_count = 0;
        self.index_count = 0;
        self.indices.clear();
        Some((args, append))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxTessTechniqueCache {
    pub orig_material: u32,

    pub orig_tech_type: i32,

    pub technique: u32,

    pub pass_count: u16,

    pub pending_pass_index: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TessTechniqueSlot {
    pub token: u32,

    pub pass_count: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexDataAppend {
    pub lock_byte_offset: u32,

    pub lock_byte_len: u32,

    pub lock_flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxDynamicVertexBuffer {
    pub used_bytes: u32,

    pub capacity: u32,
}

impl Default for GfxDynamicVertexBuffer {
    fn default() -> Self {
        Self {
            used_bytes: 0,
            capacity: DYNAMIC_TESSELLATION_VB_CAPACITY,
        }
    }
}

impl GfxDynamicVertexBuffer {
    pub fn r_set_vertex_data(&mut self, vertex_count: u32) -> VertexDataAppend {
        let lock_flags = if self.used_bytes != 0 {
            TESS_VB_LOCK_NOOVERWRITE
        } else {
            TESS_VB_LOCK_DISCARD
        };
        let lock_byte_offset = self.used_bytes;
        let lock_byte_len = vertex_count.saturating_mul(GFX_TESS_VERTEX_STRIDE);
        self.used_bytes = self.used_bytes.saturating_add(lock_byte_len);
        VertexDataAppend {
            lock_byte_offset,
            lock_byte_len,
            lock_flags,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxStreamSource0 {
    pub buffer: u32,
    pub offset: u32,
    pub stride: u32,
}

#[must_use]
pub fn gfx_tess_stream0(buffer: u32, append: VertexDataAppend) -> GfxStreamSource0 {
    GfxStreamSource0 {
        buffer,
        offset: append.lock_byte_offset,
        stride: GFX_TESS_VERTEX_STRIDE,
    }
}

#[must_use]
pub fn r_set_stream_source0(cached: &mut GfxStreamSource0, next: GfxStreamSource0) -> bool {
    if *cached == next {
        false
    } else {
        *cached = next;
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxCmdBufStreams {
    pub stream0: GfxStreamSource0,
    pub stream1: GfxStreamSource0,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamSourceAction {
    pub bind_stream0: bool,

    pub clear_stream1: bool,
}

#[must_use]
pub fn r_set_stream_source(
    state: &mut GfxCmdBufStreams,
    buffer: u32,
    offset: u32,
    stride: u32,
) -> StreamSourceAction {
    let bind_stream0 = r_set_stream_source0(
        &mut state.stream0,
        GfxStreamSource0 {
            buffer,
            offset,
            stride,
        },
    );
    let clear_stream1 = state.stream1 != GfxStreamSource0::default();
    if clear_stream1 {
        state.stream1 = GfxStreamSource0::default();
    }
    StreamSourceAction {
        bind_stream0,
        clear_stream1,
    }
}

#[must_use]
pub fn tess_vertex_lock_range(append: VertexDataAppend) -> Option<(usize, usize)> {
    let start = append.lock_byte_offset as usize;
    let len = append.lock_byte_len as usize;
    let end = start.checked_add(len)?;
    if end > DYNAMIC_TESSELLATION_VB_CAPACITY as usize {
        None
    } else {
        Some((start, end))
    }
}

#[must_use]
pub fn copy_tess_vertex_bytes(ring: &mut [u8], append: VertexDataAppend, packed: &[u8]) -> bool {
    if packed.len() != append.lock_byte_len as usize {
        return false;
    }
    let Some((start, end)) = tess_vertex_lock_range(append) else {
        return false;
    };
    if end > ring.len() {
        return false;
    }
    ring[start..end].copy_from_slice(packed);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TessTechniqueDraw {
    pub wrap_to_zero: bool,
    pub vertex: VertexDataAppend,
    pub pass_count: u16,

    pub dip_count: u16,
    pub stream0_stride: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TessFlush {
    pub args: GfxDrawPrimArgs,
    pub index: IndexDataAppend,
    pub draw: TessTechniqueDraw,
}

pub fn r_draw_tess_technique(
    args: GfxDrawPrimArgs,
    vb: &mut GfxDynamicVertexBuffer,
    pass_count: u16,
) -> TessTechniqueDraw {
    let need = args.vertex_count.saturating_mul(GFX_TESS_VERTEX_STRIDE);
    let wrap_to_zero = vb.capacity < vb.used_bytes.saturating_add(need);
    if wrap_to_zero {
        vb.used_bytes = 0;
    }
    let vertex = vb.r_set_vertex_data(args.vertex_count);
    TessTechniqueDraw {
        wrap_to_zero,
        vertex,
        pass_count,
        dip_count: pass_count,
        stream0_stride: GFX_TESS_VERTEX_STRIDE,
    }
}

pub fn rb_set_tess_technique(
    cache: &mut GfxTessTechniqueCache,
    tess: &mut GfxTess,
    ib: &mut GfxDynamicIndexBuffer,
    vb: &mut GfxDynamicVertexBuffer,
    material: u32,
    tech_type: i32,
    technique: TessTechniqueSlot,
) -> Option<TessFlush> {
    if cache.orig_material == material && cache.orig_tech_type == tech_type {
        return None;
    }
    let flushed = if tess.vertex_count != 0 {
        tess.rb_end_tess_surface(ib).map(|(args, index)| TessFlush {
            args,
            index,
            draw: r_draw_tess_technique(args, vb, cache.pass_count),
        })
    } else {
        None
    };
    tess.pending_prim_count = 0;
    tess.pending_prim_base = 0;
    cache.pending_pass_index = 0;
    cache.technique = technique.token;
    cache.orig_material = material;
    cache.orig_tech_type = tech_type;
    cache.pass_count = technique.pass_count;
    flushed
}
