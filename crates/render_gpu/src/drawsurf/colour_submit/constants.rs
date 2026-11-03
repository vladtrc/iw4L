const PLACED_ARENA_MARK: u32 = 1 << 31;

use super::as_u32;
use crate::drawsurf::exact_pipeline::ExactPipelineRegistry;
use crate::drawsurf::gpu_prepare::{ConstantPackRefusal, PassConstantBuffers};
use crate::drawsurf::gpu_resources::padded_upload_len;
use bevy::platform::collections::HashMap;
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, BindingResource, Buffer, BufferBinding,
    BufferDescriptor, BufferUsages,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use render_material::{
    ExecutablePassView, PackedCodeConstantLane, PackedCodeConstants, PortId, RuntimeShaderStage,
};
use std::num::NonZeroU64;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct GpuConstantArena {
    buffer: Option<Buffer>,
    capacity: u64,
    pub(super) bind_group: Option<BindGroup>,
    uploaded_len: usize,
    dirty_scratch: Vec<(usize, usize)>,
}

fn mark_placed_base(placed_rows: u32) -> u32 {
    placed_rows | PLACED_ARENA_MARK
}

pub(super) fn resolve_arena_base(base: u32, identity_rows: u32) -> u32 {
    if base & PLACED_ARENA_MARK == 0 {
        base
    } else {
        (base & !PLACED_ARENA_MARK).saturating_add(identity_rows)
    }
}

fn write_arena_span(
    queue: &RenderQueue,
    buffer: &Buffer,
    identity: &[u8],
    placed: &[u8],
    reserved: usize,
    start: usize,
    end: usize,
) {
    let Some(size) = NonZeroU64::new(end.saturating_sub(start) as u64) else {
        return;
    };
    let mut view = queue
        .write_buffer_with(buffer, start as u64, size)
        .expect("constant arena upload fits its buffer");
    let identity_end = end.min(identity.len());
    if start < identity_end {
        view.slice(0..identity_end - start)
            .copy_from_slice(&identity[start..identity_end]);
    }
    let pad_start = start.max(identity.len());
    let pad_end = end.min(reserved);
    if pad_start < pad_end {
        view.slice(pad_start - start..pad_end - start).fill(0);
    }
    let placed_start = start.max(reserved);
    let placed_end = end.min(reserved.saturating_add(placed.len()));
    if placed_start < placed_end {
        view.slice(placed_start - start..placed_end - start)
            .copy_from_slice(&placed[placed_start - reserved..placed_end - reserved]);
    }
    let tail_start = start.max(placed_end).max(pad_end);
    if tail_start < end {
        view.slice(tail_start - start..end - start).fill(0);
    }
}
fn grow_identity_reserved(current: usize, needed: usize) -> usize {
    if needed <= current {
        current
    } else {
        needed.next_power_of_two().max(needed)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ArenaDirty {
    Identity { start: usize, end: usize },
    Placed { start: usize, end: usize },
}

fn gpu_dirty_range(dirty: ArenaDirty, reserved: usize) -> (usize, usize) {
    match dirty {
        ArenaDirty::Identity { start, end } => (start, end),
        ArenaDirty::Placed { start, end } => {
            (reserved.saturating_add(start), reserved.saturating_add(end))
        }
    }
}

fn coalesce_gpu_dirty(ranges: &mut Vec<(usize, usize)>) {
    if ranges.is_empty() {
        return;
    }
    ranges.sort_unstable_by_key(|range| range.0);
    let mut merged = 0;
    for i in 1..ranges.len() {
        let (start, end) = ranges[i];
        const MAX_UPLOAD_GAP: usize = 16 * 1024;
        if start.saturating_sub(ranges[merged].1) <= MAX_UPLOAD_GAP {
            ranges[merged].1 = ranges[merged].1.max(end);
        } else {
            merged += 1;
            ranges[merged] = (start, end);
        }
    }
    ranges.truncate(merged + 1);
}

fn aligned_backing_span(start: usize, end: usize, backing_len: usize) -> Option<(usize, usize)> {
    if start >= end || start >= backing_len {
        return None;
    }
    let end = end.min(backing_len);
    let start = start / 4 * 4;
    let end = end.next_multiple_of(4).min(backing_len);
    if start >= end || (end - start) % 4 != 0 {
        return None;
    }
    Some((start, end))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlacedPackKey {
    placement_domain: u32,
    port: PortId,
    local: usize,
    slots: usize,
    drawsurf_key: u64,
    pass_index: u32,
}

#[derive(Clone, Debug)]
struct PlacedSpan {
    base: u32,
    len: usize,
    stamp: u32,
    version: u64,

    vertex_len: u32,
    pixel_len: u32,

    lanes: Vec<SpanLane>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpanLane {
    pixel_stage: bool,
    destination: u16,
    row_count: u8,
}

impl SpanLane {
    fn of(lane: &PackedCodeConstantLane) -> Self {
        Self {
            pixel_stage: matches!(lane.stage, RuntimeShaderStage::Pixel),
            destination: lane.destination,
            row_count: lane.row_count,
        }
    }
}

#[derive(Clone, Copy)]
struct SpanLaneWrite<'a> {
    site: SpanLane,
    rows: &'a [[u32; 4]],
}

fn span_lane_writes<'a>(
    lanes: impl Iterator<Item = &'a PackedCodeConstantLane> + Clone + 'a,
) -> impl Iterator<Item = SpanLaneWrite<'a>> + Clone + 'a {
    lanes.map(|lane| {
        let first = usize::from(lane.first_row);
        let end = first.saturating_add(usize::from(lane.row_count));
        SpanLaneWrite {
            site: SpanLane::of(lane),
            rows: lane.rows.get(first..end).unwrap_or(&[]),
        }
    })
}

pub(super) struct PlacedConstants<'a> {
    pub(super) overlay: &'a [PackedCodeConstantLane],
    pub(super) drawsurf_key: u64,
    pub(super) execution_pass_index: u32,
    pub(super) placement_domain: u32,
}

impl PlacedConstants<'_> {
    fn key(&self, executable: ExecutablePassView<'_>, slots: &Arc<[u32]>) -> PlacedPackKey {
        PlacedPackKey {
            placement_domain: self.placement_domain,
            port: executable.port,
            local: executable
                .local_banks
                .map(|banks| Arc::as_ptr(banks) as usize)
                .unwrap_or(0),
            slots: Arc::as_ptr(slots).cast::<u32>() as usize,
            drawsurf_key: self.drawsurf_key,
            pass_index: self.execution_pass_index,
        }
    }

    fn version(&self, code: u64) -> u64 {
        code ^ PackedCodeConstants::hash_lanes(self.overlay)
    }
}

fn take_exact_free(free: &mut Vec<(u32, usize)>, need: usize) -> Option<u32> {
    let i = free.iter().position(|(_, len)| *len == need)?;
    Some(free.swap_remove(i).0)
}

struct SpanSource<'a, F> {
    lanes: &'a [PackedCodeConstantLane],
    overlay: &'a [PackedCodeConstantLane],
    vertex_len: usize,
    pixel_len: usize,
    slots: &'a [u32],
    pack: F,
}

impl<F> SpanSource<'_, F> {
    fn writes(&self) -> impl Iterator<Item = SpanLaneWrite<'_>> + Clone {
        span_lane_writes(self.lanes.iter().chain(self.overlay))
    }

    fn sites(&self) -> Vec<SpanLane> {
        self.writes().map(|write| write.site).collect()
    }

    fn patch_span(
        &self,
        span: &mut [u8],
        span_start: usize,
        recorded: &[SpanLane],
        dirty: &mut Vec<(usize, usize)>,
    ) -> Option<()> {
        let lanes = self.writes();
        let vertex_len = self.vertex_len;
        let pixel_len = self.pixel_len;
        let slots = self.slots;
        if recorded.len() != lanes.clone().count() {
            return None;
        }
        let mut touched: Option<(usize, usize)> = None;
        let mark = |at: usize, end: usize, touched: &mut Option<(usize, usize)>| match touched {
            Some((lo, hi)) => {
                *lo = (*lo).min(at);
                *hi = (*hi).max(end);
            }
            None => *touched = Some((at, end)),
        };
        for (site, write) in recorded.iter().zip(lanes) {
            if *site != write.site {
                return None;
            }
            let (bank_off, bank_len) = if site.pixel_stage {
                (vertex_len, pixel_len)
            } else {
                (0, vertex_len)
            };
            let first = usize::from(site.destination);
            let count = usize::from(site.row_count);
            if first.saturating_add(count) > bank_len || write.rows.len() != count {
                return None;
            }
            let at = (bank_off + first) * 16;
            let end = at + count * 16;
            if end > span.len() {
                return None;
            }
            let bytes: &[u8] = bytemuck::cast_slice(write.rows);
            if span[at..end] != *bytes {
                span[at..end].copy_from_slice(bytes);
                mark(at, end, &mut touched);
            }
        }
        let tail = (vertex_len + pixel_len) * 16;
        let bytes: &[u8] = bytemuck::cast_slice(slots);
        if tail + bytes.len() > span.len() {
            return None;
        }
        if span[tail..tail + bytes.len()] != *bytes {
            span[tail..tail + bytes.len()].copy_from_slice(bytes);
            mark(tail, tail + bytes.len(), &mut touched);
        }
        if let Some((lo, hi)) = touched {
            dirty.push((span_start + lo, span_start + hi));
        }
        Some(())
    }
}

fn place_span(placed: &mut Vec<u8>, free: &mut Vec<(u32, usize)>, packed: &[u8]) -> (u32, usize) {
    if let Some(base) = take_exact_free(free, packed.len()) {
        let start = base as usize * 16;
        placed[start..start + packed.len()].copy_from_slice(packed);
        (base, start)
    } else {
        let start = placed.len();
        placed.extend_from_slice(packed);
        (
            u32::try_from(start / 16).expect("exact constant arena row fits u32"),
            start,
        )
    }
}

#[derive(Default)]
pub(super) struct ArenaPack {
    identity: Vec<u8>,

    identity_reserved: usize,

    seen: HashMap<(usize, usize), (u32, Arc<PassConstantBuffers>, Arc<[u32]>, u32)>,
    placed: Vec<u8>,

    placed_seen: HashMap<PlacedPackKey, PlacedSpan>,
    identity_free: Vec<(u32, usize)>,
    placed_free: Vec<(u32, usize)>,
    share_n: u32,
    stamp: u32,
    dirty: Vec<ArenaDirty>,

    pack_scratch: Vec<u8>,
    placed_dirty_scratch: Vec<(usize, usize)>,
    full_upload: bool,
}

impl ArenaPack {
    fn intern_placed_span<E, F>(
        &mut self,
        key: PlacedPackKey,
        version: u64,
        source: SpanSource<'_, F>,
    ) -> Result<(u32, bool), E>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<(), E>,
    {
        let Self {
            placed_seen: intern,
            placed,
            stamp,
            placed_dirty_scratch: dirty,
            placed_free: free,
            pack_scratch: scratch,
            ..
        } = self;
        let stamp = *stamp;
        if let Some(entry) = intern.get_mut(&key) {
            entry.stamp = stamp;
            if entry.version == version {
                return Ok((mark_placed_base(entry.base), true));
            }
            let start = entry.base as usize * 16;
            if start.saturating_add(entry.len) <= placed.len()
                && entry.vertex_len as usize == source.vertex_len
                && entry.pixel_len as usize == source.pixel_len
                && source
                    .patch_span(
                        &mut placed[start..start + entry.len],
                        start,
                        &entry.lanes,
                        dirty,
                    )
                    .is_some()
            {
                entry.version = version;
                return Ok((mark_placed_base(entry.base), true));
            }
            scratch.clear();
            let sites = source.sites();
            let packed = scratch;
            (source.pack)(packed)?;
            if packed.len() == entry.len && start.saturating_add(entry.len) <= placed.len() {
                placed[start..start + entry.len].copy_from_slice(packed);
                entry.version = version;
                entry.lanes = sites;
                entry.vertex_len = as_u32(source.vertex_len);
                entry.pixel_len = as_u32(source.pixel_len);
                dirty.push((start, start + entry.len));
                return Ok((mark_placed_base(entry.base), true));
            }
            free.push((entry.base, entry.len));
            let (base, start) = place_span(placed, free, packed);
            entry.base = base;
            entry.len = packed.len();
            entry.version = version;
            entry.lanes = sites;
            entry.vertex_len = as_u32(source.vertex_len);
            entry.pixel_len = as_u32(source.pixel_len);
            dirty.push((start, start + packed.len()));
            return Ok((mark_placed_base(base), false));
        }
        scratch.clear();
        let sites = source.sites();
        let vertex_len = source.vertex_len;
        let pixel_len = source.pixel_len;
        let packed = scratch;
        (source.pack)(packed)?;
        let (base, start) = place_span(placed, free, packed);
        intern.insert(
            key,
            PlacedSpan {
                base,
                len: packed.len(),
                stamp,
                version,
                vertex_len: as_u32(vertex_len),
                pixel_len: as_u32(pixel_len),
                lanes: sites,
            },
        );
        dirty.push((start, start + packed.len()));
        Ok((mark_placed_base(base), false))
    }

    pub(super) fn reserve_identity(&mut self) -> u32 {
        let reserved = grow_identity_reserved(self.identity_reserved, self.identity.len());
        if reserved != self.identity_reserved {
            self.identity_reserved = reserved;
            self.full_upload = true;
        }
        self.reserved_rows()
    }

    pub(super) fn upload(
        &mut self,
        arena: &mut GpuConstantArena,
        layout: Option<&BindGroupLayoutDescriptor>,
        registry: &ExactPipelineRegistry,
        device: &RenderDevice,
        queue: &RenderQueue,
        label: &'static str,
    ) -> (u32, usize, usize) {
        let full_upload = self.full_upload;
        self.full_upload = false;
        let mut dirty = std::mem::take(&mut self.dirty);
        let share_n = self.share_n;
        let total_len = arena.upload(
            PackedArenaUpdate {
                identity: &self.identity,
                placed: &self.placed,
                identity_reserved: self.identity_reserved,
                dirty: &dirty,
                full_upload,
            },
            layout,
            registry,
            device,
            queue,
            label,
        );
        dirty.clear();
        self.dirty = dirty;
        (share_n, total_len, 0)
    }

    pub(super) fn begin_list(&mut self) {
        let live = self.stamp;
        if self.seen.values().any(|(_, _, _, stamp)| *stamp != live) {
            let dead: Vec<_> = self
                .seen
                .iter()
                .filter(|(_, (_, _, _, stamp))| *stamp != live)
                .map(|(key, (base, constants, slots, _))| {
                    (
                        *key,
                        *base,
                        std::mem::size_of_val(constants.vertex.as_slice())
                            + std::mem::size_of_val(constants.pixel.as_slice())
                            + d3d9_sm3::texture_slot_rows(slots.len()) * 16,
                    )
                })
                .collect();
            for (key, base, len) in dead {
                self.seen.remove(&key);
                self.identity_free.push((base, len));
            }
        }
        if self.placed_seen.values().any(|span| span.stamp != live) {
            let dead: Vec<_> = self
                .placed_seen
                .iter()
                .filter(|(_, span)| span.stamp != live)
                .map(|(key, span)| (*key, span.base, span.len))
                .collect();
            for (key, base, len) in dead {
                self.placed_seen.remove(&key);
                self.placed_free.push((base, len));
            }
        }
        self.stamp = self.stamp.wrapping_add(1);
        self.share_n = 0;
        self.dirty.clear();
    }

    fn reserved_rows(&self) -> u32 {
        u32::try_from(self.identity_reserved.max(self.identity.len()) / 16)
            .expect("exact constant arena row fits u32")
    }

    pub(super) fn base_for(
        &mut self,
        constants: &Arc<PassConstantBuffers>,
        slots: &Arc<[u32]>,
    ) -> u32 {
        let key = (
            Arc::as_ptr(constants) as usize,
            Arc::as_ptr(slots).cast::<u32>() as usize,
        );
        if let Some((base, _, _, stamp)) = self.seen.get_mut(&key) {
            *stamp = self.stamp;
            self.share_n = self.share_n.saturating_add(1);
            return *base;
        }
        let mut span = Vec::new();
        append_constant_span(&mut span, constants, slots);
        let (base, start) = if let Some(base) = take_exact_free(&mut self.identity_free, span.len())
        {
            let start = base as usize * 16;
            self.identity[start..start + span.len()].copy_from_slice(&span);
            (base, start)
        } else {
            let start = self.identity.len();
            let base = u32::try_from(start / 16).expect("exact constant arena row fits u32");
            self.identity.extend_from_slice(&span);
            (base, start)
        };
        self.seen.insert(
            key,
            (base, Arc::clone(constants), Arc::clone(slots), self.stamp),
        );
        self.dirty.push(ArenaDirty::Identity {
            start,
            end: start + span.len(),
        });
        base
    }

    pub(super) fn append_hit(
        &mut self,
        port: &crate::drawsurf::AdmittedExactPort,
        executable: ExecutablePassView<'_>,
        slots: &Arc<[u32]>,
        placed: PlacedConstants<'_>,
    ) -> Result<(u32, bool), ConstantPackRefusal> {
        let overlay = placed.overlay;
        let key = placed.key(executable, slots);
        let version = placed.version(executable.code_constant_id);
        let Some(banks) = executable.local_banks else {
            return Err(ConstantPackRefusal::MissingLocalBanks);
        };
        self.placed_dirty_scratch.clear();
        let (base, hit) = self.intern_placed_span(
            key,
            version,
            SpanSource {
                lanes: executable.code_constants,
                overlay,
                vertex_len: banks.vertex.len(),
                pixel_len: banks.pixel.len(),
                slots,
                pack: |placed: &mut Vec<u8>| {
                    port.pack_hit_into(executable, overlay, placed)?;
                    append_slot_rows(placed, slots);
                    Ok(())
                },
            },
        )?;
        for (start, end) in self.placed_dirty_scratch.drain(..) {
            self.dirty.push(ArenaDirty::Placed { start, end });
        }
        if hit {
            self.share_n = self.share_n.saturating_add(1);
        }
        Ok((base, hit))
    }
}

pub(super) fn append_constant_span(
    bytes: &mut Vec<u8>,
    constants: &PassConstantBuffers,
    slots: &[u32],
) {
    bytes.extend_from_slice(bytemuck::cast_slice(constants.vertex.as_slice()));
    bytes.extend_from_slice(bytemuck::cast_slice(constants.pixel.as_slice()));
    append_slot_rows(bytes, slots);
}

fn append_slot_rows(bytes: &mut Vec<u8>, slots: &[u32]) {
    bytes.extend_from_slice(bytemuck::cast_slice(slots));
    let padding = d3d9_sm3::texture_slot_rows(slots.len()) * 4 - slots.len();
    bytes.extend(std::iter::repeat_n(0u8, padding * 4));
}

fn arena_capacity(required: usize) -> u64 {
    u64::try_from(required.max(256))
        .expect("exact constant arena size fits u64")
        .next_power_of_two()
}

#[derive(Debug, PartialEq, Eq)]
enum ArenaUploadPlan {
    Idle,
    Full,
    Ranges,
}

fn plan_arena_upload(
    old_len: usize,
    total_len: usize,
    resize: bool,
    full_upload: bool,
    dirty_empty: bool,
) -> ArenaUploadPlan {
    if resize || full_upload {
        return ArenaUploadPlan::Full;
    }
    if total_len == 0 || (old_len == total_len && dirty_empty) {
        return ArenaUploadPlan::Idle;
    }
    ArenaUploadPlan::Ranges
}

pub(super) struct PackedArenaUpdate<'a> {
    pub(super) identity: &'a [u8],
    pub(super) placed: &'a [u8],
    pub(super) identity_reserved: usize,
    pub(super) dirty: &'a [ArenaDirty],
    pub(super) full_upload: bool,
}

impl GpuConstantArena {
    pub(super) fn upload(
        &mut self,
        update: PackedArenaUpdate<'_>,
        layout: Option<&BindGroupLayoutDescriptor>,
        registry: &ExactPipelineRegistry,
        device: &RenderDevice,
        queue: &RenderQueue,
        label: &'static str,
    ) -> usize {
        let PackedArenaUpdate {
            identity,
            placed,
            identity_reserved,
            dirty,
            full_upload,
        } = update;
        let arena = self;
        let reserved = identity_reserved.max(identity.len());
        let total_len = reserved.saturating_add(placed.len());
        let required = padded_upload_len(total_len);
        let resize = arena.buffer.is_none()
            || u64::try_from(required).expect("constant arena size fits u64") > arena.capacity;
        if resize {
            arena.capacity = arena_capacity(required);
            arena.buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: arena.capacity,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            arena.bind_group = None;
            arena.uploaded_len = 0;
            perf::counter!("arena_realloc", "iw4l.render", 1.0);
        }

        let old_len = arena.uploaded_len;
        let plan = plan_arena_upload(old_len, total_len, resize, full_upload, dirty.is_empty());
        perf::counter!("arena_used_bytes", "iw4l.render", total_len as f64);
        perf::counter!("arena_capacity_bytes", "iw4l.render", arena.capacity as f64);
        let logical_dirty_bytes: usize = dirty
            .iter()
            .copied()
            .map(|range| {
                let (start, end) = gpu_dirty_range(range, reserved);
                end.saturating_sub(start)
            })
            .sum();
        perf::counter!(
            "arena_dirty_bytes",
            "iw4l.render",
            logical_dirty_bytes as f64
        );
        let mut uploaded_bytes: usize = 0;
        let mut upload_calls: u32 = 0;

        match plan {
            ArenaUploadPlan::Idle => {}
            ArenaUploadPlan::Full => {
                if total_len > 0 {
                    write_arena_span(
                        queue,
                        arena.buffer.as_ref().expect("constant arena exists"),
                        identity,
                        placed,
                        reserved,
                        0,
                        padded_upload_len(total_len),
                    );
                    arena.uploaded_len = total_len;
                    uploaded_bytes = total_len;
                    upload_calls = 1;
                    if resize {
                        perf::counter!("arena_full_resize", "iw4l.render", 1.0);
                    } else {
                        perf::counter!("arena_full_flag", "iw4l.render", 1.0);
                    }
                }
            }
            ArenaUploadPlan::Ranges => {
                let grew = total_len > old_len;
                arena.uploaded_len = total_len;
                let mut gpu_ranges = std::mem::take(&mut arena.dirty_scratch);
                gpu_ranges.clear();
                gpu_ranges.extend(
                    dirty
                        .iter()
                        .copied()
                        .map(|range| gpu_dirty_range(range, reserved)),
                );
                if grew {
                    gpu_ranges.push((old_len, total_len));
                }
                coalesce_gpu_dirty(&mut gpu_ranges);
                for &(start, end) in gpu_ranges.iter() {
                    let Some((start, end)) = aligned_backing_span(start, end, total_len) else {
                        continue;
                    };
                    upload_calls = upload_calls.saturating_add(1);
                    uploaded_bytes = uploaded_bytes.saturating_add(end.saturating_sub(start));
                    write_arena_span(
                        queue,
                        arena.buffer.as_ref().expect("constant arena exists"),
                        identity,
                        placed,
                        reserved,
                        start,
                        end,
                    );
                }
                gpu_ranges.clear();
                arena.dirty_scratch = gpu_ranges;
            }
        }
        perf::counter!("arena_uploaded_bytes", "iw4l.render", uploaded_bytes as f64);
        perf::counter!("arena_upload_calls", "iw4l.render", f64::from(upload_calls));
        if arena.bind_group.is_none()
            && let Some(layout_source) = layout
        {
            let bgl = registry.bind_group_layout(device, layout_source);
            let bind_group = device.create_bind_group(
                "iw4_exact_constant_arena",
                &bgl,
                &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: arena.buffer.as_ref().expect("constant arena exists"),
                        offset: 0,
                        size: None,
                    }),
                }],
            );
            arena.bind_group = Some(bind_group);
        }
        total_len
    }
}
