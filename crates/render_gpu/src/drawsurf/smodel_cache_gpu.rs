use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use lighting_iw4::{
    SMC_BANK_VB_BYTES, SMC_INDEX_U16_N, SMC_VB_BYTES, SmcPatchLock, r_smc_stream_source_byte_offset,
};

use crate::drawsurf::backend::DYNAMIC_INDEX_BUFFER_CAPACITY;
use crate::drawsurf::gpu_resources::{padded_upload_len, write_buffer_padded};

#[derive(Resource, Default)]
pub struct SmodelCacheGpu {
    vb: Option<Buffer>,
    ib: Option<Buffer>,

    dynamic_ib: Option<Buffer>,
    uploaded_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmcPatchWriteRefuse {
    NoBuffer,
    LockRejected,
}

impl SmodelCacheGpu {
    pub fn ensure(&mut self, device: &RenderDevice) {
        if self.vb.is_none() {
            self.vb = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_r_alloc_static_model_cache_vb"),
                size: u64::from(SMC_VB_BYTES),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.ib.is_none() {
            self.ib = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_r_cache_static_model_indices"),
                size: (SMC_INDEX_U16_N * 2) as u64,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.dynamic_ib.is_none() {
            self.dynamic_ib = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_r_set_index_data_smc"),
                size: u64::from(DYNAMIC_INDEX_BUFFER_CAPACITY) * 2,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
    }

    pub fn vertex_buffer(&self) -> Option<&Buffer> {
        self.vb.as_ref()
    }

    pub fn index_buffer(&self) -> Option<&Buffer> {
        self.ib.as_ref()
    }

    pub fn dynamic_index_buffer(&self) -> Option<&Buffer> {
        self.dynamic_ib.as_ref()
    }

    pub fn write_dynamic_indices(
        &mut self,
        queue: &RenderQueue,
        indices: &[u16],
        layout_revision: u64,
    ) -> bool {
        if self.uploaded_revision == Some(layout_revision) && layout_revision != 0 {
            return true;
        }
        let Some(ib) = self.dynamic_ib.as_ref() else {
            return false;
        };
        if indices.is_empty() {
            self.uploaded_revision = Some(layout_revision);
            return false;
        }
        let bytes = bytemuck::cast_slice(indices);
        let cap = (DYNAMIC_INDEX_BUFFER_CAPACITY as usize).saturating_mul(2);
        if padded_upload_len(bytes.len()) > cap {
            return false;
        }
        write_buffer_padded(queue, ib, bytes);
        self.uploaded_revision = Some(layout_revision);
        false
    }

    pub fn bank_byte_range(cache_index: u16) -> Option<(u64, u64)> {
        let start = u64::from(r_smc_stream_source_byte_offset(cache_index)?);
        Some((start, start + u64::from(SMC_BANK_VB_BYTES)))
    }

    pub fn patch(
        &self,
        queue: &RenderQueue,
        lock: SmcPatchLock,
        bytes: &[u8],
    ) -> Result<(), SmcPatchWriteRefuse> {
        let Some(vb) = self.vb.as_ref() else {
            return Err(SmcPatchWriteRefuse::NoBuffer);
        };
        if !lock.accepts_bytes(bytes.len()) {
            return Err(SmcPatchWriteRefuse::LockRejected);
        }
        queue.write_buffer(vb, u64::from(lock.byte_offset), bytes);
        Ok(())
    }

    pub fn patch_indices(
        &self,
        queue: &RenderQueue,
        byte_offset: u32,
        bytes: &[u8],
    ) -> Result<(), SmcPatchWriteRefuse> {
        let Some(ib) = self.ib.as_ref() else {
            return Err(SmcPatchWriteRefuse::NoBuffer);
        };
        let end = (byte_offset as usize).saturating_add(bytes.len());
        if end > SMC_INDEX_U16_N * 2 {
            return Err(SmcPatchWriteRefuse::LockRejected);
        }
        queue.write_buffer(ib, u64::from(byte_offset), bytes);
        Ok(())
    }
}
