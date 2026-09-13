use std::num::NonZeroU64;

use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use super::super::gpu_resources::padded_upload_len;

const RANGE_GRANULE: usize = 4096;

fn stream_capacity(required: u64) -> u64 {
    let slack = required.saturating_add(required / 2).max(required);
    let padded = padded_upload_len(usize::try_from(slack).unwrap_or(usize::MAX));
    u64::try_from(padded).expect("stream capacity fits u64")
}

#[derive(Default)]
pub(super) struct GpuStream {
    buffer: Option<Buffer>,

    capacity: u64,

    len: usize,

    stride: usize,

    allocation: u64,
}

impl GpuStream {
    pub(super) fn buffer(&self) -> Option<&Buffer> {
        self.buffer.as_ref()
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn allocation(&self) -> u64 {
        self.allocation
    }

    fn fits(&self, bytes: u64) -> bool {
        self.buffer.is_some() && bytes <= self.capacity
    }

    pub(super) fn holds(&self, elements: usize) -> bool {
        self.fits(u64::try_from(elements.saturating_mul(self.stride)).unwrap_or(u64::MAX))
    }

    pub(super) fn reserve(
        &mut self,
        device: &RenderDevice,
        label: &'static str,
        usage: BufferUsages,
        elements: usize,
        stride: usize,
    ) {
        self.stride = stride;
        let bytes = u64::try_from(elements.saturating_mul(stride)).unwrap_or(u64::MAX);
        if self.fits(bytes) {
            return;
        }
        self.capacity = stream_capacity(bytes);
        self.buffer = Some(device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: self.capacity,
            usage: usage | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.allocation = self.allocation.wrapping_add(1);
    }

    pub(super) fn set_len(&mut self, elements: usize) {
        self.len = elements;
    }

    #[must_use]
    pub(super) fn write_at(&self, queue: &RenderQueue, offset: u64, bytes: &[u8]) -> bool {
        let Some(buffer) = self.buffer.as_ref() else {
            return false;
        };
        debug_assert_eq!(offset % RANGE_GRANULE as u64, 0, "stream write is aligned");
        let padded = bytes.len().next_multiple_of(RANGE_GRANULE);
        let Some(size) = NonZeroU64::new(padded as u64) else {
            return true;
        };
        if offset.saturating_add(size.get()) > self.capacity {
            return false;
        }
        let mut view = queue
            .write_buffer_with(buffer, offset, size)
            .expect("stream write fits its capacity");
        view.slice(..bytes.len()).copy_from_slice(bytes);
        view.slice(bytes.len()..).fill(0);
        true
    }
}

pub(super) fn aligned_stream_span(
    start: usize,
    end: usize,
    total: usize,
) -> Option<(usize, usize)> {
    if start >= end || start >= total {
        return None;
    }
    let start = start / RANGE_GRANULE * RANGE_GRANULE;
    let end = end.next_multiple_of(RANGE_GRANULE).min(total);
    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

#[derive(Default)]
pub(super) struct GpuMesh {
    pub(super) vertex: GpuStream,
    pub(super) index: GpuStream,

    pub(super) uploaded_vertices: u64,

    pub(super) uploaded_topology: u64,
}

impl GpuMesh {
    pub(super) fn drawable(&self) -> bool {
        self.vertex.buffer().is_some()
            && self.index.buffer().is_some()
            && !self.vertex.is_empty()
            && !self.index.is_empty()
    }
}
