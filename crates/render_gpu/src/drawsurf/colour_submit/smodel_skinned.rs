use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use super::{ExtractedColourRefs, GpuSubmitRefusal};
use crate::drawsurf::gpu_resources::{padded_upload_len, write_buffer_padded};

#[derive(Default)]
pub(super) struct SmodelSkinnedTess {
    verts: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    indices: Vec<u32>,
    spans: HashMap<(u32, u32), (u32, u32)>,
    vertex: Option<Buffer>,
    index: Option<Buffer>,
    vertex_cap: usize,
    index_cap: usize,
}

impl SmodelSkinnedTess {
    pub(super) fn begin_frame(&mut self) {
        self.verts.clear();
        self.indices.clear();
        self.spans.clear();
    }

    pub(super) fn append_draw(
        &mut self,
        extracted: ExtractedColourRefs<'_>,
        placement: u32,
        surface: u32,
        world_from_local: Mat4,
    ) -> Result<(u32, u32), GpuSubmitRefusal> {
        if let Some(&span) = self.spans.get(&(placement, surface)) {
            return Ok(span);
        }
        let geom = extracted.world.static_geometry.as_ref();
        let &(packed_off, packed_n) = geom
            .smodel_surface_verts
            .get(surface as usize)
            .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
        if packed_n == 0 {
            return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
        }
        let packed_off_us = packed_off as usize;
        let packed_n_us = packed_n as usize;
        let src = geom
            .smodel_vertices
            .get(packed_off_us..packed_off_us.saturating_add(packed_n_us))
            .ok_or(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let &(index_start, index_count) = geom
            .smodel_surface_ranges
            .get(surface as usize)
            .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
        if index_count == 0 {
            return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
        }
        let index_start_us = index_start as usize;
        let index_count_us = index_count as usize;
        let src_ix = geom
            .smodel_indices
            .get(index_start_us..index_start_us.saturating_add(index_count_us))
            .ok_or(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let dest_base = self.verts.len() as u32;
        self.verts
            .resize(self.verts.len().saturating_add(packed_n_us), [0u8; 32]);
        let dest = self
            .verts
            .get_mut(dest_base as usize..dest_base as usize + packed_n_us)
            .ok_or(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let m = world_from_local.to_cols_array();
        let fixed = lighting_iw4::setup_transform_unit_vec(&m);
        lighting_iw4::r_skin_xsurface_unique_verts(dest, src, &m, &fixed)
            .map_err(|_| GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let dest_index_start = self.indices.len() as u32;
        self.indices.reserve(index_count_us);
        let packed_end = packed_off.saturating_add(packed_n);
        for &idx in src_ix {
            if idx < packed_off || idx >= packed_end {
                return Err(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement });
            }
            self.indices
                .push(idx.saturating_sub(packed_off).saturating_add(dest_base));
        }
        let span = (dest_index_start, index_count);
        self.spans.insert((placement, surface), span);
        Ok(span)
    }

    pub(super) fn upload(&mut self, device: &RenderDevice, queue: &RenderQueue) {
        if self.verts.is_empty() || self.indices.is_empty() {
            return;
        }
        let vbytes: &[u8] = bytemuck::cast_slice(self.verts.as_slice());
        let ibytes: &[u8] = bytemuck::cast_slice(self.indices.as_slice());
        let vneed = padded_upload_len(vbytes.len());
        let ineed = padded_upload_len(ibytes.len());
        if self.vertex.is_none() || self.vertex_cap < vneed {
            self.vertex = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_smodel_skinned_unique_vb"),
                size: vneed as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_cap = vneed;
        }
        if self.index.is_none() || self.index_cap < ineed {
            self.index = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_smodel_skinned_unique_ib"),
                size: ineed as u64,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.index_cap = ineed;
        }
        if let Some(buffer) = self.vertex.as_ref() {
            write_buffer_padded(queue, buffer, vbytes);
        }
        if let Some(buffer) = self.index.as_ref() {
            write_buffer_padded(queue, buffer, ibytes);
        }
    }

    pub(super) fn vertex_buffer(&self) -> Option<&Buffer> {
        self.vertex.as_ref()
    }

    pub(super) fn index_buffer(&self) -> Option<&Buffer> {
        self.index.as_ref()
    }
}
