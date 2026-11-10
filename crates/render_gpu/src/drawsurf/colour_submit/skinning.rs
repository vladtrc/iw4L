use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Weak};

use bevy::prelude::Resource;
use bevy::render::render_resource::*;
use bevy::render::renderer::{PendingCommandBuffers, RenderDevice, RenderQueue};
use render_frame::skinning::{PackedSkinJob, PackedSkinShape, SkinBatchLimits, plan_skin_batches};

const VERTEX_BYTES: u64 = 32;
const PARAMS_BYTES: u64 = 48;

struct Pipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

struct ResidentShape {
    owner: Weak<PackedSkinShape>,
    buffer: Buffer,
    last_used: u64,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Skinned {
    pub(super) jobs: usize,
    pub(super) vertices: usize,
}

#[derive(Resource, Default)]
pub(super) struct SkinGpu {
    pipeline: Option<Pipeline>,
    device: Option<RenderDevice>,
    device_generation: u64,
    shapes: HashMap<u64, ResidentShape>,
    frame: u64,
    output_bytes: u64,
    frame_buffers: Vec<Buffer>,
    retired: Vec<Buffer>,
    retired_bytes: Arc<std::sync::atomic::AtomicU64>,
}

impl SkinGpu {
    pub(super) fn device_generation(&self) -> u64 {
        self.device_generation
    }

    pub(super) fn resident_bytes(&self) -> u64 {
        self.shapes
            .values()
            .map(|shape| shape.buffer.size())
            .sum::<u64>()
            + self
                .frame_buffers
                .iter()
                .map(|buffer| buffer.size())
                .sum::<u64>()
            + self.output_bytes.saturating_mul(2)
            + self.retired.iter().map(|buffer| buffer.size()).sum::<u64>()
            + self
                .retired_bytes
                .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(super) fn dispatch(
        &mut self,
        jobs: &[PackedSkinJob],
        output: Option<&Buffer>,
        vertex_count: usize,
        device: &RenderDevice,
        pending: &mut PendingCommandBuffers,
        budget: u64,
    ) -> Result<Skinned, &'static str> {
        let _prepare = perf::span!("SkinPrepare", "iw4l.render");
        if self
            .device
            .as_ref()
            .is_none_or(|previous| previous.wgpu_device() != device.wgpu_device())
        {
            let generation = self
                .device_generation
                .checked_add(1)
                .expect("skin device generation exhausted");
            *self = Self::default();
            self.device = Some(device.clone());
            self.device_generation = generation;
        }
        self.frame = self.frame.wrapping_add(1);
        self.retired.append(&mut self.frame_buffers);
        let expired: Vec<u64> = self
            .shapes
            .iter()
            .filter(|(_, shape)| shape.owner.strong_count() == 0)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(shape) = self.shapes.remove(&id) {
                self.retired.push(shape.buffer);
            }
        }
        if jobs.is_empty() {
            return Ok(Skinned::default());
        }
        let output = output.ok_or("unsupported skin product")?;
        self.output_bytes = output.size();
        let limits = device.limits();
        let batches = plan_skin_batches(
            jobs,
            output.size().min(vertex_count as u64 * VERTEX_BYTES),
            SkinBatchLimits {
                storage_bytes: limits.max_storage_buffer_binding_size,
                buffer_bytes: limits.max_buffer_size,
                storage_alignment: u64::from(limits.min_storage_buffer_offset_alignment),
                uniform_alignment: u64::from(limits.min_uniform_buffer_offset_alignment),
                dispatch_groups: limits.max_compute_workgroups_per_dimension,
            },
        )?;
        let needed: HashSet<u64> = batches
            .iter()
            .flat_map(|batch| batch.jobs.iter().map(|index| jobs[*index].shape.id()))
            .collect();
        let unique: HashMap<u64, &PackedSkinShape> = batches
            .iter()
            .flat_map(|batch| {
                batch
                    .jobs
                    .iter()
                    .map(|index| (jobs[*index].shape.id(), jobs[*index].shape.as_ref()))
            })
            .collect();
        let inputs = unique
            .values()
            .map(|shape| shape.vertices().len() as u64 * 64)
            .sum::<u64>();
        let transient = batches
            .iter()
            .map(|batch| batch.palette_bytes + batch.parameter_bytes)
            .sum::<u64>();
        if inputs
            .saturating_add(transient.saturating_mul(2))
            .saturating_add(output.size().saturating_mul(2))
            > budget
        {
            return Err("skin residency budget exhausted");
        }
        while self
            .resident_bytes()
            .saturating_add(inputs)
            .saturating_add(transient.saturating_mul(2))
            > budget
        {
            let cold = self
                .shapes
                .iter()
                .filter(|(id, _)| !needed.contains(id))
                .min_by_key(|(_, shape)| shape.last_used)
                .map(|(id, _)| *id);
            let Some(cold) = cold else {
                break;
            };
            if let Some(shape) = self.shapes.remove(&cold) {
                self.retired.push(shape.buffer);
            }
        }
        let additional = unique
            .iter()
            .filter(|(id, _)| !self.shapes.contains_key(id))
            .map(|(_, shape)| shape.vertices().len() as u64 * 64)
            .sum::<u64>();
        if self
            .resident_bytes()
            .saturating_add(additional)
            .saturating_add(transient)
            > budget
        {
            return Err("skin residency budget exhausted");
        }
        for (id, shape) in unique {
            let resident = self.shapes.entry(id).or_insert_with(|| ResidentShape {
                owner: Arc::downgrade(
                    &jobs
                        .iter()
                        .find(|job| job.shape.id() == id)
                        .expect("shape owner")
                        .shape,
                ),
                buffer: device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("packed_skin_shape"),
                    contents: bytemuck::cast_slice(shape.vertices()),
                    usage: BufferUsages::STORAGE,
                }),
                last_used: self.frame,
            });
            resident.last_used = self.frame;
        }
        let stride =
            PARAMS_BYTES.next_multiple_of(u64::from(limits.min_uniform_buffer_offset_alignment));
        let pipeline = self.pipeline.get_or_insert_with(|| make_pipeline(device));
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("packed_skin"),
        });
        let mut skinned = Skinned::default();
        for batch in batches {
            let mut bone_rows = Vec::new();
            let mut param_rows = Vec::new();
            for &index in &batch.jobs {
                let job = &jobs[index];
                let base = bone_rows.len() as u32;
                bone_rows.extend_from_slice(&job.bones);
                for range in job.ranges.iter().filter(|range| range.count != 0) {
                    let params = [
                        job.destination - (batch.output_offset / VERTEX_BYTES) as u32,
                        range.count,
                        base,
                        range.start,
                        job.offset[0].to_bits(),
                        job.offset[1].to_bits(),
                        job.offset[2].to_bits(),
                        0,
                        u32::from(range.rigid),
                        0,
                        0,
                        0,
                    ];
                    let slot = param_rows.len().div_ceil(stride as usize);
                    param_rows.resize(slot * stride as usize, 0);
                    param_rows.extend_from_slice(bytemuck::cast_slice(&params));
                }
            }
            let bones = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("packed_skin_bones"),
                contents: bytemuck::cast_slice(&bone_rows),
                usage: BufferUsages::STORAGE,
            });
            let params = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("packed_skin_params"),
                contents: &param_rows,
                usage: BufferUsages::UNIFORM,
            });
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("packed_skin"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            let mut slot = 0_u64;
            for index in batch.jobs {
                let job = &jobs[index];
                let shape = &self.shapes[&job.shape.id()];
                let binding = device.create_bind_group(
                    "packed_skin",
                    &pipeline.layout,
                    &BindGroupEntries::sequential((
                        shape.buffer.as_entire_binding(),
                        bones.as_entire_binding(),
                        BindingResource::Buffer(BufferBinding {
                            buffer: &params,
                            offset: 0,
                            size: NonZeroU64::new(PARAMS_BYTES),
                        }),
                        BindingResource::Buffer(BufferBinding {
                            buffer: output,
                            offset: batch.output_offset,
                            size: NonZeroU64::new(batch.output_bytes),
                        }),
                    )),
                );
                for range in job.ranges.iter().filter(|range| range.count != 0) {
                    pass.set_bind_group(0, &*binding, &[(slot * stride) as u32]);
                    pass.dispatch_workgroups(range.count.div_ceil(64), 1, 1);
                    slot += 1;
                    skinned.vertices += range.count as usize;
                }
                skinned.jobs += 1;
            }
            drop(pass);
            self.frame_buffers.extend([bones.clone(), params.clone()]);
        }
        // Command buffers own the referenced pages through GPU execution.
        pending.push([encoder.finish()]);
        Ok(skinned)
    }
}

pub(super) fn retire_submitted_buffers(
    mut skin: bevy::prelude::ResMut<SkinGpu>,
    queue: bevy::prelude::Res<RenderQueue>,
) {
    let retired = std::mem::take(&mut skin.retired);
    if retired.is_empty() {
        return;
    }
    let bytes = retired.iter().map(|buffer| buffer.size()).sum::<u64>();
    let pending = Arc::clone(&skin.retired_bytes);
    pending.fetch_add(bytes, std::sync::atomic::Ordering::AcqRel);
    queue.on_submitted_work_done(move || {
        drop(retired);
        pending.fetch_sub(bytes, std::sync::atomic::Ordering::AcqRel);
    });
}

fn make_pipeline(device: &RenderDevice) -> Pipeline {
    let storage = |binding, read_only| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(
        "packed_skin",
        &[
            storage(0, true),
            storage(1, true),
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(PARAMS_BYTES),
                },
                count: None,
            },
            storage(3, false),
        ],
    );
    let module = device.create_and_validate_shader_module(ShaderModuleDescriptor {
        label: Some("packed_skin"),
        source: ShaderSource::Wgsl(include_str!("skinning.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("packed_skin"),
        bind_group_layouts: &[Some(&*layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
        label: Some("packed_skin"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("skin_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    Pipeline { layout, pipeline }
}
