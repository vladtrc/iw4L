//! Readbacks are mapped on a later render frame, after submission. Pending
//! buffers are never reused or waited for; saturation admits conservatively.
use bevy::render::renderer::RenderDevice;
use render_frame::portal_occlusion::PortalOcclusionFrame;
use std::sync::mpsc::{self, Receiver};

pub(crate) struct QueryBatch {
    pub queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    context: PortalOcclusionFrame,
    keys: Vec<Vec<u32>>,
    capacity: usize,
}

struct Pending {
    batch: QueryBatch,
    mapped: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

#[derive(Default)]
pub(crate) struct PortalQueryGpu {
    pending: Vec<Pending>,
    free: Vec<QueryBatch>,
}

impl PortalQueryGpu {
    pub fn begin(
        &mut self,
        device: &RenderDevice,
        context: Option<&PortalOcclusionFrame>,
        keys: Vec<Vec<u32>>,
    ) -> Option<QueryBatch> {
        let frame = context.map_or(u64::MAX, |c| c.frame);
        for p in &mut self.pending {
            if p.mapped.is_none() && p.batch.context.frame < frame {
                let (tx, rx) = mpsc::channel();
                p.batch
                    .readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result);
                    });
                p.mapped = Some(rx);
            }
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut resolved = 0;
        let mut visible = 0;
        let mut index = 0;
        while index < self.pending.len() {
            let Some(rx) = &self.pending[index].mapped else {
                index += 1;
                continue;
            };
            let outcome = rx.try_recv();
            if matches!(outcome, Err(mpsc::TryRecvError::Empty)) {
                index += 1;
                continue;
            }
            let p = self.pending.swap_remove(index);
            if matches!(outcome, Ok(Ok(()))) {
                if let Ok(data) = p.batch.readback.slice(..).get_mapped_range() {
                    for (key, bytes) in p.batch.keys.iter().zip(data.as_chunks::<8>().0.iter()) {
                        let samples = u64::from_le_bytes(*bytes);
                        resolved += 1;
                        visible += u32::from(samples > 0);
                        p.batch.context.publish(key.clone(), samples);
                    }
                }
                p.batch.readback.unmap();
                self.free.push(p.batch);
            } else {
                p.batch.readback.unmap();
            }
        }
        perf::counter!("portal_query_resolved", "iw4l.render", f64::from(resolved));
        perf::counter!("portal_query_visible", "iw4l.render", f64::from(visible));
        let context = context?;
        if keys.is_empty() || self.pending.len() >= 3 {
            return None;
        }
        if let Some(mut batch) = self.free.pop()
            && batch.capacity >= keys.len()
        {
            batch.context = context.clone();
            batch.keys = keys;
            return Some(batch);
        }
        let bytes = keys.len() as u64 * 8;
        let gpu = device.wgpu_device();
        Some(QueryBatch {
            queries: gpu.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("portal visibility"),
                ty: wgpu::QueryType::Occlusion,
                count: keys.len() as u32,
            }),
            resolve: gpu.create_buffer(&wgpu::BufferDescriptor {
                label: Some("portal query resolve"),
                size: bytes,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: gpu.create_buffer(&wgpu::BufferDescriptor {
                label: Some("portal query readback"),
                size: bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            capacity: keys.len(),
            context: context.clone(),
            keys,
        })
    }

    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder, batch: QueryBatch) {
        let count = batch.keys.len() as u32;
        encoder.resolve_query_set(&batch.queries, 0..count, &batch.resolve, 0);
        encoder.copy_buffer_to_buffer(&batch.resolve, 0, &batch.readback, 0, u64::from(count) * 8);
        self.pending.push(Pending {
            batch,
            mapped: None,
        });
        perf::counter!("portal_query_submitted", "iw4l.render", f64::from(count));
    }
}
