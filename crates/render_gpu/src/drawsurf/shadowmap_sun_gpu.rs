use bevy::prelude::*;
use bevy::render::render_resource::{
    Buffer, BufferDescriptor, BufferUsages, Extent3d, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use super::backend::partition::D3dScissorRect;
use render_frame::SUN_SHADOW_FORCED_PROFILE;

pub const SHADOWMAP_SUN_COLOR_FORMAT: TextureFormat = TextureFormat::R32Float;

pub const SHADOWMAP_SUN_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth24Plus;

struct ShadowmapSunGpuTarget {
    _color: Texture,
    color_view: TextureView,
    _depth: Texture,
    depth_view: TextureView,
    width: u32,
    height: u32,
}

#[derive(Resource, Default)]
pub(super) struct ShadowmapSunGpu {
    target: Option<ShadowmapSunGpuTarget>,

    smodel_generation: [Option<u64>; 2],
    smodel_spans: [Vec<Option<SmodelShadowSpan>>; 2],
    smodel_index_epochs: [Vec<Buffer>; 2],
    smodel_index_epoch_bytes: [Vec<u64>; 2],
    xmodel_index_epochs: [Vec<Buffer>; 2],
    xmodel_index_epoch_bytes: [Vec<u64>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SmodelShadowSpan {
    pub draw_start: u32,
    pub draw_count: u32,
    pub ring_epoch: u32,
    pub entry_start: u32,
    pub entry_count: u32,
}

impl ShadowmapSunGpu {
    pub(super) fn depth_view(&self) -> Option<&TextureView> {
        self.target.as_ref().map(|target| &target.depth_view)
    }

    pub(super) fn smodel_generation(&self, partition: usize) -> Option<u64> {
        self.smodel_generation[partition.min(1)]
    }

    pub(super) fn smodel_spans(&self, partition: usize) -> &[Option<SmodelShadowSpan>] {
        &self.smodel_spans[partition.min(1)]
    }

    pub(super) fn stage_smodel_spans(
        &mut self,
        partition: usize,
        spans: Vec<Option<SmodelShadowSpan>>,
    ) {
        let i = partition.min(1);
        self.smodel_generation[i] = None;
        self.smodel_spans[i] = spans;
    }

    pub(super) fn commit_smodel_partition(&mut self, generation: u64, partition: usize) {
        self.smodel_generation[partition.min(1)] = Some(generation);
    }

    pub(super) fn invalidate_smodel_partition(&mut self, partition: usize) {
        self.smodel_generation[partition.min(1)] = None;
    }
}

pub(super) fn ensure_shadowmap_sun_target(
    gpu: &mut ShadowmapSunGpu,
    device: &RenderDevice,
) -> (Option<TextureView>, bool) {
    let profile = SUN_SHADOW_FORCED_PROFILE;
    let width = profile.gpu_width();
    let height = profile.gpu_height();
    let resized = gpu
        .target
        .as_ref()
        .is_none_or(|target| target.width != width || target.height != height);
    if !resized {
        return (
            gpu.target.as_ref().map(|target| target.color_view.clone()),
            false,
        );
    }
    let color = device.create_texture(&TextureDescriptor {
        label: Some("iw4_shadowmap_sun"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: SHADOWMAP_SUN_COLOR_FORMAT,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&TextureViewDescriptor {
        label: Some("iw4_shadowmap_sun_color"),
        ..Default::default()
    });
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("iw4_shadowmap_sun_depth"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: SHADOWMAP_SUN_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        label: Some("iw4_shadowmap_sun_depth"),
        ..Default::default()
    });
    gpu.target = Some(ShadowmapSunGpuTarget {
        _color: color,
        color_view: color_view.clone(),
        _depth: depth,
        depth_view,
        width,
        height,
    });
    (Some(color_view), true)
}

pub(super) fn smodel_shadow_geometry_reusable(
    retained_generation: Option<u64>,
    generation: u64,
    retained_span_n: usize,
    smodel_entry_n: usize,
) -> bool {
    retained_generation == Some(generation) && retained_span_n == smodel_entry_n
}

pub(super) fn upload_shadow_index_epochs(
    buffers: &mut Vec<Buffer>,
    capacities: &mut Vec<u64>,
    device: &RenderDevice,
    queue: &RenderQueue,
    epochs: &[Vec<u32>],
    label: &'static str,
) -> Vec<Buffer> {
    buffers.truncate(buffers.len().min(epochs.len()));
    capacities.truncate(capacities.len().min(epochs.len()));
    let mut uploaded = Vec::with_capacity(epochs.len());
    for (i, indices) in epochs.iter().enumerate() {
        let bytes = (indices.len() * 4) as u64;
        if buffers.len() <= i {
            buffers.push(device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: bytes.max(4),
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            capacities.push(bytes.max(4));
        } else if capacities[i] < bytes {
            buffers[i] = device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            capacities[i] = bytes;
        }
        let buf = &buffers[i];
        queue.write_buffer(buf, 0, bytemuck::cast_slice(indices));
        uploaded.push(buf.clone());
    }
    uploaded
}

pub(super) fn upload_smodel_index_epochs(
    gpu: &mut ShadowmapSunGpu,
    partition: usize,
    device: &RenderDevice,
    queue: &RenderQueue,
    epochs: &[Vec<u32>],
) -> Vec<Buffer> {
    let i = partition.min(1);
    upload_shadow_index_epochs(
        &mut gpu.smodel_index_epochs[i],
        &mut gpu.smodel_index_epoch_bytes[i],
        device,
        queue,
        epochs,
        "iw4_shadowmap_sun_smodel_index_epoch",
    )
}

pub(super) fn upload_xmodel_index_epochs(
    gpu: &mut ShadowmapSunGpu,
    partition: usize,
    device: &RenderDevice,
    queue: &RenderQueue,
    epochs: &[Vec<u32>],
) -> Vec<Buffer> {
    let i = partition.min(1);
    upload_shadow_index_epochs(
        &mut gpu.xmodel_index_epochs[i],
        &mut gpu.xmodel_index_epoch_bytes[i],
        device,
        queue,
        epochs,
        "iw4_shadowmap_sun_xmodel_index_epoch",
    )
}

pub(super) fn bind_resident_smodel_index_epochs(
    gpu: &ShadowmapSunGpu,
    partition: usize,
) -> Vec<Buffer> {
    gpu.smodel_index_epochs[partition.min(1)].clone()
}

pub fn scissor_xywh(scissor: D3dScissorRect) -> (u32, u32, u32, u32) {
    (
        scissor.left,
        scissor.top,
        scissor.right.saturating_sub(scissor.left),
        scissor.bottom.saturating_sub(scissor.top),
    )
}
