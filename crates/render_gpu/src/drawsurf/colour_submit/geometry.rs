use super::StaticGeometryIdentity;
use super::publication::PublishedRenderFrame;
use super::residency;
use super::{CameraWorldPretess, ExactTessBind, NEUTRAL_VERTEX_LIGHTING};
use crate::drawsurf::smodel_cache_gpu::SmodelCacheGpu;
use bevy::prelude::Resource;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use render_frame::{CODE_MESH_INDEX_CAP, CODE_MESH_VERT_CAP, CODE_MESH_VERT_STRIDE};
use render_material::MaterialGenerationId;

#[derive(Resource, Default)]
pub(super) struct ExactColourGeometry {
    pub(super) generation: MaterialGenerationId,
    pub(super) static_identity: StaticGeometryIdentity,
    pub(super) world_vertex: Option<Buffer>,
    pub(super) world_layer: Option<Buffer>,
    pub(super) world_effect: Option<Buffer>,
    pub(super) world_index: Option<Buffer>,
    pub(super) world_surface_ranges: Vec<(u32, u32)>,

    pub(super) world_cpu_indices: Vec<u32>,
    pub(super) smodel_vertex: Option<Buffer>,
    pub(super) smodel_vertex_lighting: Option<Buffer>,
    pub(super) smodel_index: Option<Buffer>,
    pub(super) smodel_surface_ranges: Vec<(u32, u32)>,
    pub(super) smodel_cached_vertex: Option<Buffer>,
    pub(super) smodel_cached_index: Option<Buffer>,
    pub(super) smodel_cached_vertex_count: usize,

    pub(super) xmodel: residency::GpuMesh,
    pub(super) xmodel_surface_ranges: Vec<(u32, u32)>,
    pub(super) fx_vertex: Option<Buffer>,
    pub(super) fx_index: Option<Buffer>,
    pub(super) fx_surface_ranges: Vec<(u32, u32)>,
    pub(super) fx_vertex_count: usize,
    pub(super) fx_index_count: usize,
    pub(super) fx_revision: u64,

    pub(super) fx_copy_dst: bool,
    pub(super) particle_cloud: residency::GpuMesh,
    pub(super) particle_cloud_template_resident: Option<(u64, u64, (usize, usize))>,
    pub(super) particle_cloud_surface_ranges: Vec<(u32, u32)>,
    pub(super) mark_mesh: residency::GpuMesh,
    pub(super) mark_mesh_surface_ranges: Vec<(u32, u32)>,
    pub(super) glass_mesh: residency::GpuMesh,
    pub(super) glass_mesh_surface_ranges: Vec<(u32, u32)>,
    pub(super) neutral_vertex_lighting: Option<Buffer>,
    pub(super) neutral_vertex_lighting_count: usize,
}

impl ExactColourGeometry {
    pub(super) fn world_ready(&self) -> bool {
        self.world_vertex.is_some() && self.world_index.is_some()
    }
    pub(super) fn smodel_ready(&self) -> bool {
        self.smodel_vertex.is_some() && self.smodel_index.is_some()
    }
    pub(super) fn xmodel_ready(&self) -> bool {
        self.xmodel.drawable()
    }
    pub(super) fn fx_ready(&self) -> bool {
        self.fx_vertex.is_some() && self.fx_index.is_some()
    }
    pub(super) fn particle_cloud_ready(&self) -> bool {
        self.particle_cloud.drawable()
    }
    pub(super) fn mark_mesh_ready(&self) -> bool {
        self.mark_mesh.drawable()
    }
    pub(super) fn glass_mesh_ready(&self) -> bool {
        self.glass_mesh.drawable()
    }

    pub(super) fn xmodel_index_epochs(&self) -> &[Buffer] {
        self.xmodel
            .index
            .buffer()
            .map(std::slice::from_ref)
            .unwrap_or(&[])
    }
}

#[derive(Default)]
pub(super) struct GeometryUploadResult {
    pub(super) code_mesh_gpu_kind: Option<i32>,
}

pub(super) fn upload_exact_geometry(
    source: &PublishedRenderFrame,
    geometry: &mut ExactColourGeometry,
    smodel_cache_gpu: &mut SmodelCacheGpu,
    device: &RenderDevice,
    queue: &RenderQueue,
) -> GeometryUploadResult {
    smodel_cache_gpu.ensure(device);
    for (lock, bytes) in &source.smc_vb_patches {
        let _ = smodel_cache_gpu.patch(queue, *lock, bytes);
    }
    for (off, bytes) in &source.smc_ib_patches {
        let _ = smodel_cache_gpu.patch_indices(queue, *off, bytes);
    }
    if geometry.world_cpu_indices.is_empty()
        && !source.world().static_geometry.world_indices.is_empty()
    {
        geometry
            .world_cpu_indices
            .clone_from(source.world().static_geometry.world_indices.as_ref());
        if geometry.world_surface_ranges.is_empty() {
            geometry
                .world_surface_ranges
                .clone_from(source.world().static_geometry.world_surface_ranges.as_ref());
        }
    }
    let empty_source = source.world().static_geometry.world_vertices.is_empty()
        && source.world().static_geometry.smodel_vertices.is_empty();
    if empty_source && geometry.static_identity.world_products.0.is_some() {
        return GeometryUploadResult::default();
    }
    let static_matches = geometry
        .static_identity
        .matches(source.world().static_geometry_identity());
    if !static_matches {
        geometry.static_identity.world_products = source.world().world_products;
        geometry.world_vertex = None;
        geometry.world_layer = None;
        geometry.world_effect = None;
        geometry.world_index = None;
        geometry.world_cpu_indices.clear();
        geometry.world_surface_ranges.clear();
        geometry.static_identity.world_vertex_count =
            source.world().static_geometry.world_vertices.len();
        geometry.static_identity.world_layer_count =
            source.world().static_geometry.world_layer.len();
        geometry.static_identity.world_index_count =
            source.world().static_geometry.world_indices.len();
        geometry.smodel_vertex = None;
        geometry.smodel_vertex_lighting = None;
        geometry.smodel_index = None;
        geometry.smodel_surface_ranges.clear();
        geometry.static_identity.smodel_vertex_count =
            source.world().static_geometry.smodel_vertices.len();
        geometry.static_identity.smodel_index_count =
            source.world().static_geometry.smodel_indices.len();
        geometry.smodel_cached_vertex = None;
        geometry.smodel_cached_index = None;
        geometry.smodel_cached_vertex_count =
            source.world().static_geometry.smodel_cached_vertices.len();
        if !source.world().static_geometry.world_vertices.is_empty()
            && !source.world().static_geometry.world_indices.is_empty()
        {
            diag::info!(
                World,
                "exact geometry: upload static buffers — world {} + smodel {} vertices, products {:?}",
                source.world().static_geometry.world_vertices.len(),
                source.world().static_geometry.smodel_vertices.len(),
                source.world().world_products.0,
            );
            geometry.world_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_vb"),
                contents: bytemuck::cast_slice(
                    source.world().static_geometry.world_vertices.as_slice(),
                ),
                usage: BufferUsages::VERTEX,
            }));
            // Dynamic impact updates can patch this independently of static vertices.
            geometry.world_effect = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("world_surface_effect_vb"),
                contents: bytemuck::cast_slice(&vec![
                    0u32;
                    geometry.static_identity.world_vertex_count
                ]),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            }));
            geometry.world_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_ib"),
                contents: bytemuck::cast_slice(
                    source.world().static_geometry.world_indices.as_slice(),
                ),
                usage: BufferUsages::INDEX,
            }));
            geometry
                .world_surface_ranges
                .clone_from(source.world().static_geometry.world_surface_ranges.as_ref());
            geometry
                .world_cpu_indices
                .clone_from(source.world().static_geometry.world_indices.as_ref());
        }
        if !source.world().static_geometry.world_layer.is_empty() {
            geometry.world_layer = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_layer_vb"),
                contents: source.world().static_geometry.world_layer.as_slice(),
                usage: BufferUsages::VERTEX,
            }));
        }
        if !source.world().static_geometry.smodel_vertices.is_empty()
            && !source.world().static_geometry.smodel_indices.is_empty()
        {
            geometry.smodel_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_smodel_vb"),
                contents: bytemuck::cast_slice(
                    source.world().static_geometry.smodel_vertices.as_slice(),
                ),
                usage: BufferUsages::VERTEX,
            }));
            if source.world().static_geometry.smodel_vertex_lighting.len()
                == source.world().static_geometry.smodel_vertices.len()
            {
                geometry.smodel_vertex_lighting = Some(
                    device.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("iw4_exact_colour_smodel_vertex_lighting_vb"),
                        contents: bytemuck::cast_slice(
                            source
                                .world()
                                .static_geometry
                                .smodel_vertex_lighting
                                .as_slice(),
                        ),
                        usage: BufferUsages::VERTEX,
                    }),
                );
            }
            geometry.smodel_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_smodel_ib"),
                contents: bytemuck::cast_slice(
                    source.world().static_geometry.smodel_indices.as_slice(),
                ),
                usage: BufferUsages::INDEX,
            }));
            geometry.smodel_surface_ranges.clone_from(
                source
                    .world()
                    .static_geometry
                    .smodel_surface_ranges
                    .as_ref(),
            );
        }
    }
    geometry.generation = source.world().generation;

    upload_xmodel_streams(geometry, source, device, queue);
    upload_particle_cloud(geometry, source, device, queue);

    upload_retained_mesh(
        &mut geometry.mark_mesh,
        &mut geometry.mark_mesh_surface_ranges,
        RetainedMeshSource {
            labels: (
                "iw4_exact_colour_mark_mesh_vb",
                "iw4_exact_colour_mark_mesh_ib",
            ),
            revision: source.mark_mesh_revision,
            vertices: bytemuck::cast_slice(source.mark_mesh_vertices.as_slice()),
            vertex_count: source.mark_mesh_vertices.len(),
            vertex_stride: asset_iw4::size::GFX_WORLD_VERTEX,
            indices: bytemuck::cast_slice(source.mark_mesh_indices.as_slice()),
            index_count: source.mark_mesh_indices.len(),
            index_stride: 2,
            surface_ranges: &source.mark_mesh_surface_ranges,
        },
        device,
        queue,
    );

    upload_retained_mesh(
        &mut geometry.glass_mesh,
        &mut geometry.glass_mesh_surface_ranges,
        RetainedMeshSource {
            labels: (
                "iw4_exact_colour_glass_mesh_vb",
                "iw4_exact_colour_glass_mesh_ib",
            ),
            revision: source.glass_mesh_revision,
            vertices: bytemuck::cast_slice(source.glass_mesh_vertices.as_slice()),
            vertex_count: source.glass_mesh_vertices.len(),
            vertex_stride: asset_iw4::size::GFX_PACKED_VERTEX,
            indices: bytemuck::cast_slice(source.glass_mesh_indices.as_slice()),
            index_count: source.glass_mesh_indices.len(),
            index_stride: 4,
            surface_ranges: &source.glass_mesh_surface_ranges,
        },
        device,
        queue,
    );

    let fx_kind = colour_code_mesh_upload_kind(
        geometry.fx_revision,
        geometry.fx_vertex_count,
        geometry.fx_index_count,
        geometry.fx_vertex.is_some() && geometry.fx_index.is_some() && geometry.fx_copy_dst,
        source.fx_revision,
        source.fx_vertices.len(),
        source.fx_indices.len(),
    );
    if fx_kind == 0 {
        if source.fx_vertices.is_empty() || source.fx_indices.is_empty() {
            geometry.fx_revision = source.fx_revision;
            geometry.fx_vertex_count = source.fx_vertices.len();
            geometry.fx_index_count = source.fx_indices.len();
            geometry.fx_surface_ranges.clear();
        }
    } else {
        if fx_kind == 2 {
            geometry.fx_vertex = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_code_mesh_vb"),
                size: u64::from(CODE_MESH_VERT_CAP) * u64::from(CODE_MESH_VERT_STRIDE),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            geometry.fx_index = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_code_mesh_ib"),
                size: u64::from(CODE_MESH_INDEX_CAP) * 4,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            geometry.fx_copy_dst = true;
        }
        geometry.fx_revision = source.fx_revision;
        geometry.fx_vertex_count = source.fx_vertices.len();
        geometry.fx_index_count = source.fx_indices.len();
        if !source.fx_vertices.is_empty() && !source.fx_indices.is_empty() {
            if let (Some(vb), Some(ib)) = (geometry.fx_vertex.as_ref(), geometry.fx_index.as_ref())
            {
                queue.write_buffer(vb, 0, bytemuck::cast_slice(source.fx_vertices.as_slice()));
                queue.write_buffer(ib, 0, bytemuck::cast_slice(source.fx_indices.as_slice()));
                geometry
                    .fx_surface_ranges
                    .clone_from(&*source.fx_surface_ranges);
            }
        }
    }
    ensure_neutral_vertex_lighting(geometry, device);
    GeometryUploadResult {
        code_mesh_gpu_kind: Some(fx_kind),
    }
}

// Every packed stream that a T6 vertex-lit pipeline can draw from without
// its own baked colours reads this buffer at the same vertex index.
fn ensure_neutral_vertex_lighting(geometry: &mut ExactColourGeometry, device: &RenderDevice) {
    const VERTEX_BYTES: u64 = asset_iw4::size::GFX_PACKED_VERTEX as u64;
    let vertex_buffers = [
        geometry.smodel_vertex.as_ref(),
        geometry.smodel_cached_vertex.as_ref(),
        geometry.xmodel.vertex.buffer(),
        geometry.fx_vertex.as_ref(),
        geometry.particle_cloud.vertex.buffer(),
        geometry.mark_mesh.vertex.buffer(),
        geometry.glass_mesh.vertex.buffer(),
    ];
    let need = vertex_buffers
        .into_iter()
        .flatten()
        .map(|buffer| buffer.size() / VERTEX_BYTES)
        .chain([u64::from(lighting_iw4::SMC_BANK_VB_BYTES) / VERTEX_BYTES])
        .max()
        .unwrap_or(0) as usize;
    if geometry.neutral_vertex_lighting.is_some() && geometry.neutral_vertex_lighting_count >= need
    {
        return;
    }
    let count = need.next_power_of_two();
    geometry.neutral_vertex_lighting =
        Some(device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("iw4_exact_colour_neutral_vertex_lighting_vb"),
            contents: bytemuck::cast_slice(&vec![NEUTRAL_VERTEX_LIGHTING; count]),
            usage: BufferUsages::VERTEX,
        }));
    geometry.neutral_vertex_lighting_count = count;
}

// Only the custom tail changes frame to frame; the template ahead of it is
// written again only when a stream is reallocated.
fn upload_particle_cloud(
    geometry: &mut ExactColourGeometry,
    source: &PublishedRenderFrame,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    let frame = source;
    let vertex_count = frame.particle_cloud_vertices.len();
    let index_count = frame.particle_cloud_indices.len();
    let mesh = &mut geometry.particle_cloud;
    if vertex_count == 0 || index_count == 0 {
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        geometry.particle_cloud_surface_ranges.clear();
        return;
    }
    let resident = mesh.uploaded_vertices == frame.particle_cloud_revision
        && mesh.vertex.len() == vertex_count
        && mesh.index.len() == index_count
        && mesh.drawable();
    if resident {
        return;
    }
    let vertex_stride = fx_iw4::GFX_POS_TEX_VERTEX_STRIDE;
    let index_stride = size_of::<u32>();
    mesh.vertex.reserve(
        device,
        "iw4_exact_colour_sparkcloud_vb",
        BufferUsages::VERTEX,
        vertex_count,
        vertex_stride,
    );
    mesh.index.reserve(
        device,
        "iw4_exact_colour_sparkcloud_ib",
        BufferUsages::INDEX,
        index_count,
        index_stride,
    );
    let template = frame.particle_cloud_template;
    let key = (mesh.vertex.allocation(), mesh.index.allocation(), template);
    let (vertex_from, index_from) = if geometry.particle_cloud_template_resident == Some(key) {
        (template.0 * vertex_stride, template.1 * index_stride)
    } else {
        (0, 0)
    };
    let vertices: &[u8] = bytemuck::cast_slice(frame.particle_cloud_vertices.as_slice());
    let indices: &[u8] = bytemuck::cast_slice(frame.particle_cloud_indices.as_slice());
    let written = write_stream_tail(&mesh.vertex, queue, vertices, vertex_from)
        && write_stream_tail(&mesh.index, queue, indices, index_from);
    if !written {
        diag::error!(
            World,
            "drawsurf geometry: sparkcloud upload skipped — revision {} is not resident",
            frame.particle_cloud_revision,
        );
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        geometry.particle_cloud_template_resident = None;
        geometry.particle_cloud_surface_ranges.clear();
        return;
    }
    mesh.vertex.set_len(vertex_count);
    mesh.index.set_len(index_count);
    mesh.uploaded_vertices = frame.particle_cloud_revision;
    mesh.uploaded_topology = frame.particle_cloud_revision;
    geometry.particle_cloud_template_resident = Some(key);
    geometry
        .particle_cloud_surface_ranges
        .clone_from(&*frame.particle_cloud_surface_ranges);
}

#[must_use]
fn write_stream_tail(
    stream: &residency::GpuStream,
    queue: &RenderQueue,
    bytes: &[u8],
    from: usize,
) -> bool {
    let Some((start, end)) = residency::aligned_stream_span(from, bytes.len(), bytes.len()) else {
        return true;
    };
    stream.write_at(queue, start as u64, &bytes[start..end])
}

struct RetainedMeshSource<'a> {
    labels: (&'static str, &'static str),
    revision: u64,
    vertices: &'a [u8],
    vertex_count: usize,
    vertex_stride: usize,
    indices: &'a [u8],
    index_count: usize,
    index_stride: usize,
    surface_ranges: &'a [(u32, u32)],
}

fn upload_retained_mesh(
    mesh: &mut residency::GpuMesh,
    ranges: &mut Vec<(u32, u32)>,
    src: RetainedMeshSource<'_>,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    if src.vertex_count == 0 || src.index_count == 0 {
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        mesh.uploaded_vertices = src.revision;
        mesh.uploaded_topology = src.revision;
        ranges.clear();
        return;
    }
    let resident = mesh.uploaded_vertices == src.revision
        && mesh.vertex.len() == src.vertex_count
        && mesh.index.len() == src.index_count
        && mesh.vertex.buffer().is_some()
        && mesh.index.buffer().is_some();
    if resident {
        return;
    }
    mesh.vertex.reserve(
        device,
        src.labels.0,
        BufferUsages::VERTEX,
        src.vertex_count,
        src.vertex_stride,
    );
    mesh.index.reserve(
        device,
        src.labels.1,
        BufferUsages::INDEX,
        src.index_count,
        src.index_stride,
    );
    mesh.vertex.set_len(src.vertex_count);
    mesh.index.set_len(src.index_count);
    let vertices_written = mesh.vertex.write_at(queue, 0, src.vertices);
    let indices_written = mesh.index.write_at(queue, 0, src.indices);
    if !(vertices_written && indices_written) {
        diag::error!(
            World,
            "drawsurf geometry: {} upload skipped (vertices={vertices_written} indices={indices_written}) — revision {} is not resident",
            src.labels.0,
            src.revision,
        );
        return;
    }
    mesh.uploaded_vertices = src.revision;
    mesh.uploaded_topology = src.revision;
    ranges.clear();
    ranges.extend_from_slice(src.surface_ranges);
}

fn upload_xmodel_streams(
    geometry: &mut ExactColourGeometry,
    source: &PublishedRenderFrame,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    let verts = source.xmodel_vertices.len();
    let indices = source.xmodel_indices.len();
    if verts == 0 || indices == 0 {
        geometry.xmodel.vertex.set_len(0);
        geometry.xmodel.index.set_len(0);
        geometry.xmodel.uploaded_vertices = source.xmodel_revision;
        geometry.xmodel.uploaded_topology = source.xmodel_topology_revision;
        geometry.xmodel_surface_ranges.clear();
        return;
    }
    let policy = colour_xmodel_upload(XModelUploadQuery {
        uploaded_vertices: geometry.xmodel.uploaded_vertices,
        uploaded_topology: geometry.xmodel.uploaded_topology,
        resident_verts: geometry.xmodel.vertex.len(),
        resident_indices: geometry.xmodel.index.len(),
        vertex_fits: geometry.xmodel.vertex.holds(verts),
        index_fits: geometry.xmodel.index.holds(indices),
        cpu_vertices_revision: source.xmodel_revision,
        cpu_topology_revision: source.xmodel_topology_revision,
        cpu_verts: verts,
        cpu_indices: indices,
    });
    let (write_vertices, write_indices) = match policy {
        XModelUpload::Resident => return,
        XModelUpload::Rewrite { vertices, indices } => (vertices, indices),
        XModelUpload::Grow => {
            geometry.xmodel.vertex.reserve(
                device,
                "iw4_exact_colour_xmodel_vb",
                BufferUsages::VERTEX | BufferUsages::STORAGE,
                verts,
                asset_iw4::size::GFX_PACKED_VERTEX,
            );
            geometry.xmodel.index.reserve(
                device,
                "iw4_exact_colour_xmodel_ib",
                BufferUsages::INDEX,
                indices,
                4,
            );
            (true, true)
        }
    };
    geometry.xmodel.vertex.set_len(verts);
    geometry.xmodel.index.set_len(indices);
    if write_vertices {
        if geometry.xmodel.vertex.write_at(
            queue,
            0,
            bytemuck::cast_slice(source.xmodel_vertices.as_slice()),
        ) {
            geometry.xmodel.uploaded_vertices = source.xmodel_revision;
        } else {
            diag::error!(
                World,
                "drawsurf geometry: xmodel vertex upload skipped — revision {} is not resident",
                source.xmodel_revision,
            );
        }
    }
    if write_indices {
        if geometry.xmodel.index.write_at(
            queue,
            0,
            bytemuck::cast_slice(source.xmodel_indices.as_slice()),
        ) {
            geometry.xmodel.uploaded_topology = source.xmodel_topology_revision;
            geometry
                .xmodel_surface_ranges
                .clone_from(&*source.xmodel_surface_ranges);
        } else {
            diag::error!(
                World,
                "drawsurf geometry: xmodel index upload skipped — topology revision {} is not resident",
                source.xmodel_topology_revision,
            );
        }
    }
}

pub(super) fn record_geometry<'a>(
    geometry: &'a ExactColourGeometry,
    cache: &'a SmodelCacheGpu,
    skinned: super::encode::RecordMesh<'a>,
    pretess: &'a CameraWorldPretess,
) -> super::encode::RecordGeometry<'a> {
    use super::encode::{RecordGeometry, RecordMesh};
    let mesh = |kind| {
        let (vertex, index) = match kind {
            ExactTessBind::World => (geometry.world_vertex.as_ref(), pretess.index()),
            ExactTessBind::Smodel => (
                geometry.smodel_vertex.as_ref(),
                geometry.smodel_index.as_ref(),
            ),
            ExactTessBind::SmodelCached => (cache.vertex_buffer(), cache.dynamic_index_buffer()),
            ExactTessBind::SmodelSkinned => (skinned.vertex, skinned.index),
            ExactTessBind::XModel => (
                geometry.xmodel.vertex.buffer(),
                geometry.xmodel.index.buffer(),
            ),
            ExactTessBind::CodeMesh => (geometry.fx_vertex.as_ref(), geometry.fx_index.as_ref()),
            ExactTessBind::ParticleCloud => (
                geometry.particle_cloud.vertex.buffer(),
                geometry.particle_cloud.index.buffer(),
            ),
            ExactTessBind::MarkMesh => (
                geometry.mark_mesh.vertex.buffer(),
                geometry.mark_mesh.index.buffer(),
            ),
            ExactTessBind::Glass => (
                geometry.glass_mesh.vertex.buffer(),
                geometry.glass_mesh.index.buffer(),
            ),
        };
        RecordMesh {
            vertex,
            index,
            lighting: vertex_lighting_stream(kind, geometry, skinned.lighting),
        }
    };
    RecordGeometry {
        world: mesh(ExactTessBind::World),
        smodel: mesh(ExactTessBind::Smodel),
        cached: mesh(ExactTessBind::SmodelCached),
        skinned: mesh(ExactTessBind::SmodelSkinned),
        xmodel: mesh(ExactTessBind::XModel),
        code: mesh(ExactTessBind::CodeMesh),
        particles: mesh(ExactTessBind::ParticleCloud),
        marks: mesh(ExactTessBind::MarkMesh),
        glass: mesh(ExactTessBind::Glass),
        world_layer: geometry.world_layer.as_ref(),
        world_effect: geometry.world_effect.as_ref(),
        world_index_count: pretess.logical_index_count(),
        world_layout_epoch: pretess.layout_epoch(),
    }
}

fn vertex_lighting_stream<'a>(
    tess: ExactTessBind,
    geometry: &'a ExactColourGeometry,
    smodel_skinned: Option<&'a Buffer>,
) -> Option<&'a Buffer> {
    match tess {
        ExactTessBind::World => None,
        ExactTessBind::Smodel => geometry
            .smodel_vertex_lighting
            .as_ref()
            .or(geometry.neutral_vertex_lighting.as_ref()),
        ExactTessBind::SmodelSkinned => smodel_skinned,
        _ => geometry.neutral_vertex_lighting.as_ref(),
    }
}

pub(super) fn shadow_record_geometry<'a>(
    geometry: &'a ExactColourGeometry,
    world_index: Option<&'a Buffer>,
    smodel_index_epochs: &'a [Buffer],
    xmodel_index_epochs: &'a [Buffer],
    skinned: super::encode::RecordMesh<'a>,
) -> super::shadow_encode::ShadowGeometry<'a> {
    use super::encode::RecordMesh;
    super::shadow_encode::ShadowGeometry {
        world: RecordMesh {
            vertex: geometry.world_vertex.as_ref(),
            index: world_index,
            lighting: None,
        },
        smodel: RecordMesh {
            vertex: geometry.smodel_vertex.as_ref(),
            index: None,
            lighting: vertex_lighting_stream(ExactTessBind::Smodel, geometry, skinned.lighting),
        },
        cached: RecordMesh {
            vertex: geometry.smodel_cached_vertex.as_ref(),
            index: geometry.smodel_cached_index.as_ref(),
            lighting: vertex_lighting_stream(
                ExactTessBind::SmodelCached,
                geometry,
                skinned.lighting,
            ),
        },
        skinned,
        xmodel: RecordMesh {
            vertex: geometry.xmodel.vertex.buffer(),
            index: None,
            lighting: vertex_lighting_stream(ExactTessBind::XModel, geometry, skinned.lighting),
        },
        smodel_index_epochs,
        xmodel_index_epochs,
        world_layer: geometry.world_layer.as_ref(),
        world_effect: geometry.world_effect.as_ref(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XModelUpload {
    Resident,

    Rewrite { vertices: bool, indices: bool },

    Grow,
}

struct XModelUploadQuery {
    uploaded_vertices: u64,
    uploaded_topology: u64,
    resident_verts: usize,
    resident_indices: usize,

    vertex_fits: bool,
    index_fits: bool,
    cpu_vertices_revision: u64,
    cpu_topology_revision: u64,
    cpu_verts: usize,
    cpu_indices: usize,
}

fn colour_xmodel_upload(q: XModelUploadQuery) -> XModelUpload {
    if !q.vertex_fits || !q.index_fits {
        return XModelUpload::Grow;
    }
    let vertices =
        q.uploaded_vertices != q.cpu_vertices_revision || q.resident_verts != q.cpu_verts;
    let indices =
        q.uploaded_topology != q.cpu_topology_revision || q.resident_indices != q.cpu_indices;
    if !vertices && !indices {
        return XModelUpload::Resident;
    }
    XModelUpload::Rewrite { vertices, indices }
}

fn colour_code_mesh_upload_kind(
    gpu_revision: u64,
    gpu_verts: usize,
    gpu_indices: usize,
    gpu_has_ring: bool,
    cpu_revision: u64,
    cpu_verts: usize,
    cpu_indices: usize,
) -> i32 {
    let over =
        cpu_verts > CODE_MESH_VERT_CAP as usize || cpu_indices > CODE_MESH_INDEX_CAP as usize;
    if over {
        return 0;
    }
    if cpu_verts == 0 || cpu_indices == 0 {
        return 0;
    }
    if gpu_has_ring
        && gpu_revision == cpu_revision
        && gpu_verts == cpu_verts
        && gpu_indices == cpu_indices
    {
        0
    } else if gpu_has_ring {
        1
    } else {
        2
    }
}
