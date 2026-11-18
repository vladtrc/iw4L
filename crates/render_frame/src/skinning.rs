use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PackedSkinVertex {
    pub packed: [u32; 8],
    pub bones: [u32; 4],
    pub weights: [f32; 4],
}

#[derive(Debug)]
pub struct PackedSkinShape {
    id: u64,
    vertices: Vec<PackedSkinVertex>,
    required_bones: usize,
    valid: bool,
}

impl PackedSkinShape {
    pub fn new(vertices: Vec<PackedSkinVertex>) -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("skin shape identity exhausted");
        let required_bones = vertices
            .iter()
            .filter(|vertex| vertex.bones[0] != u32::MAX)
            .flat_map(|vertex| {
                std::iter::once(vertex.bones[0]).chain(
                    (1..4)
                        .filter(|&extra| vertex.weights[extra] > 0.0)
                        .map(|extra| vertex.bones[extra]),
                )
            })
            .max()
            .map_or(0, |bone| bone as usize + 1);
        let valid = vertices.iter().all(|vertex| {
            vertex.packed[..3]
                .iter()
                .all(|word| f32::from_bits(*word).is_finite())
                && vertex
                    .weights
                    .iter()
                    .all(|weight| weight.is_finite() && *weight >= 0.0 && *weight <= 1.0)
                && vertex.weights[1..].iter().sum::<f32>() <= 1.0001
        });
        Arc::new(Self {
            valid,
            id,
            vertices,
            required_bones,
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn required_bones(&self) -> usize {
        self.required_bones
    }
    pub fn vertices(&self) -> &[PackedSkinVertex] {
        &self.vertices
    }
}

#[derive(Clone, Debug)]
pub struct SkinVertexRange {
    pub start: u32,
    pub count: u32,
    pub rigid: bool,
}

#[derive(Clone, Debug)]
pub struct PackedSkinJob {
    pub shape: Arc<PackedSkinShape>,
    pub bones: Arc<[[[f32; 4]; 4]]>,
    pub ranges: Arc<[SkinVertexRange]>,
    pub destination: u32,
    pub offset: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct SkinBatchLimits {
    pub storage_bytes: u64,
    pub buffer_bytes: u64,
    pub storage_alignment: u64,
    pub uniform_alignment: u64,
    pub dispatch_groups: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinBatchFull {
    OutputPage,
    Palette,
    Parameters,
}

#[derive(Debug)]
pub struct SkinBatch {
    pub opened_by: Option<SkinBatchFull>,
    pub jobs: Vec<usize>,
    pub output_offset: u64,
    pub output_bytes: u64,
    pub palette_bytes: u64,
    pub parameter_bytes: u64,
}

pub fn plan_skin_batches(
    jobs: &[PackedSkinJob],
    output_bytes: u64,
    limits: SkinBatchLimits,
) -> Result<Vec<SkinBatch>, &'static str> {
    let stride = 48_u64.next_multiple_of(limits.uniform_alignment.max(1));
    let alignment = limits.storage_alignment.max(32).next_multiple_of(32);
    let mut order: Vec<usize> = (0..jobs.len())
        .filter(|index| jobs[*index].ranges.iter().any(|range| range.count != 0))
        .collect();
    order.sort_unstable_by_key(|index| jobs[*index].destination);
    let mut batches: Vec<SkinBatch> = Vec::new();
    let mut covered = 0_u64;
    for index in order {
        let job = &jobs[index];
        let vertices = job.shape.vertices().len() as u64;
        if vertices == 0 || !job.shape.valid || job.offset.iter().any(|n| !n.is_finite()) {
            return Err("invalid skin shape");
        }
        if job.bones.is_empty()
            || job.shape.required_bones() > job.bones.len()
            || job.bones.iter().flatten().flatten().any(|n| !n.is_finite())
        {
            return Err("invalid skin palette");
        }
        if job.ranges.iter().any(|range| {
            range
                .start
                .checked_add(range.count)
                .is_none_or(|end| end as u64 > vertices)
        }) {
            return Err("invalid skin range");
        }
        let start = u64::from(job.destination) * 32;
        let end = start
            .checked_add(vertices.checked_mul(32).ok_or("unsupported skin product")?)
            .ok_or("unsupported skin product")?;
        if start < covered {
            return Err("overlapping skin output");
        }
        covered = end;
        if end > output_bytes {
            return Err("invalid skin range");
        }
        let palette = job.bones.len() as u64 * 64;
        let parameters = job.ranges.iter().filter(|range| range.count != 0).count() as u64 * stride;
        let offset = start / alignment * alignment;
        if vertices * 64 > limits.storage_bytes
            || end - offset > limits.storage_bytes
            || palette > limits.storage_bytes
            || parameters > limits.buffer_bytes.min(u32::MAX as u64)
            || job
                .ranges
                .iter()
                .any(|range| range.count.div_ceil(64) > limits.dispatch_groups)
        {
            return Err("unsupported skin product");
        }
        let full = batches.last().and_then(|batch| {
            if end - batch.output_offset > limits.storage_bytes {
                Some(SkinBatchFull::OutputPage)
            } else if batch.palette_bytes + palette > limits.storage_bytes {
                Some(SkinBatchFull::Palette)
            } else if batch.parameter_bytes + parameters > limits.buffer_bytes.min(u32::MAX as u64)
            {
                Some(SkinBatchFull::Parameters)
            } else {
                None
            }
        });
        let append = !batches.is_empty() && full.is_none();
        if append {
            let batch = batches.last_mut().expect("batch present");
            batch.jobs.push(index);
            batch.output_bytes = end - batch.output_offset;
            batch.palette_bytes += palette;
            batch.parameter_bytes += parameters;
        } else {
            batches.push(SkinBatch {
                opened_by: full,
                jobs: vec![index],
                output_offset: offset,
                output_bytes: end - offset,
                palette_bytes: palette,
                parameter_bytes: parameters,
            });
        }
    }
    Ok(batches)
}
