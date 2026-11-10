use crate::drawsurf::ExtractedRenderFrameProducts;
use crate::drawsurf::gpu_resources::SamplerTable;
use bevy::prelude::Resource;
use render_frame::{MaterialExecFrame, SunShadowForcedFrame};
use render_material::{MaterialGenerationId, PreparedMaterialTable, RuntimeMaterialCatalog};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct ExtractedStaticGeometry {
    pub world_bounds: Vec<Option<([f32; 3], f32)>>,
    pub smodel_bounds: Vec<Option<([f32; 3], f32)>>,
    pub world_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>>,
    pub world_layer: Arc<Vec<u8>>,
    pub world_indices: Arc<Vec<u32>>,
    pub world_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub world_vertex_refusal: Option<render_frame::WorldVertexRefusal>,
    pub smodel_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub smodel_indices: Arc<Vec<u32>>,
    pub smodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub smodel_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub smodel_cached_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub smodel_surface_verts: Arc<Vec<(u32, u32)>>,
    pub smodel_vertex_lighting: Arc<Vec<[u8; 4]>>,
}

/// Packed world/model vertices both start with their float3 position.
/// Compute once when geometry is published, rather than scan vertices per view.
pub fn surface_bounds<const N: usize>(
    vertices: &[[u8; N]],
    indices: &[u32],
    ranges: &[(u32, u32)],
) -> Vec<Option<([f32; 3], f32)>> {
    ranges
        .iter()
        .map(|&(start, count)| {
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for &index in
                indices.get(start as usize..(start as usize).checked_add(count as usize)?)?
            {
                let vertex = vertices.get(index as usize)?;
                for axis in 0..3 {
                    let at = axis * 4;
                    let value = f32::from_le_bytes(vertex.get(at..at + 4)?.try_into().ok()?);
                    if !value.is_finite() {
                        return None;
                    }
                    lo[axis] = lo[axis].min(value);
                    hi[axis] = hi[axis].max(value);
                }
            }
            if count == 0 {
                return None;
            }
            let center = std::array::from_fn(|i| (lo[i] + hi[i]) * 0.5);
            let radius = (0..3)
                .map(|i| (hi[i] - center[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            Some((center, radius))
        })
        .collect()
}

#[derive(Resource, Clone, Debug, Default)]
pub struct InstalledRenderWorld(Arc<RenderWorldData>);

impl InstalledRenderWorld {
    pub fn new(data: RenderWorldData) -> Self {
        Self(Arc::new(data))
    }
}

impl std::ops::Deref for InstalledRenderWorld {
    type Target = RenderWorldData;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderWorldData {
    pub generation: MaterialGenerationId,
    pub world_products: frame::WorldProducts,
    pub smc_revision: Option<u64>,
    pub ports: Arc<Vec<crate::drawsurf::AdmittedExactPort>>,
    pub static_geometry: Arc<ExtractedStaticGeometry>,
    pub smodel_pretess_indices: std::sync::Arc<Vec<u16>>,
    pub smodel_index_layout_revision: u64,
    pub smc_index_baked: Arc<Vec<u16>>,
    pub sampler_table: Option<SamplerTable>,
    pub image_handles: crate::drawsurf::gpu_resources::RuntimeImageHandles,
    pub catalog: Option<Arc<RuntimeMaterialCatalog>>,
    pub prepared: Option<Arc<PreparedMaterialTable>>,
    pub sorted_material_names: Arc<Vec<String>>,
    pub shader_program_names: Arc<Vec<Option<String>>>,
    pub sun_effects: Option<render_frame::SunEffectsDef>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StaticGeometryIdentity {
    pub world_products: frame::WorldProducts,
    pub world_vertex_count: usize,
    pub world_index_count: usize,
    pub world_layer_count: usize,
    pub smodel_vertex_count: usize,
    pub smodel_index_count: usize,
}

impl StaticGeometryIdentity {
    pub fn matches(self, other: Self) -> bool {
        self.world_products.same_as(other.world_products)
            && self.world_vertex_count == other.world_vertex_count
            && self.world_index_count == other.world_index_count
            && self.world_layer_count == other.world_layer_count
            && self.smodel_vertex_count == other.smodel_vertex_count
            && self.smodel_index_count == other.smodel_index_count
    }
}

impl RenderWorldData {
    pub fn static_geometry_identity(&self) -> StaticGeometryIdentity {
        StaticGeometryIdentity {
            world_products: self.world_products,
            world_vertex_count: self.static_geometry.world_vertices.len(),
            world_index_count: self.static_geometry.world_indices.len(),
            world_layer_count: self.static_geometry.world_layer.len(),
            smodel_vertex_count: self.static_geometry.smodel_vertices.len(),
            smodel_index_count: self.static_geometry.smodel_indices.len(),
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct PublishedRenderFrame {
    world: InstalledRenderWorld,
    data: Arc<RenderFrameData>,
}

impl PublishedRenderFrame {
    pub fn seal(world: InstalledRenderWorld, data: RenderFrameData) -> Self {
        assert_eq!(world.generation, data.generation);
        Self {
            world,
            data: Arc::new(data),
        }
    }
    pub fn world(&self) -> &InstalledRenderWorld {
        &self.world
    }
}

impl std::ops::Deref for PublishedRenderFrame {
    type Target = RenderFrameData;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderFrameData {
    pub skin_refused: bool,
    pub skin_jobs: Vec<render_frame::skinning::PackedSkinJob>,
    pub frame_products: ExtractedRenderFrameProducts,
    pub generation: MaterialGenerationId,
    pub sun_shadow: Option<SunShadowForcedFrame>,
    pub sun_effects: Option<render_frame::SunEffectsFrame>,
    pub warm_pipelines: bool,
    pub pipeline_world_materials: Arc<std::collections::HashSet<u16>>,
    pub pipeline_smodel_materials: Arc<std::collections::HashSet<u16>>,
    pub pipeline_demand_revision: u64,
    pub smc_vb_patches: Vec<(lighting_iw4::SmcPatchLock, Vec<u8>)>,
    pub smc_ib_patches: Vec<(u32, Vec<u8>)>,
    pub xmodel_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub xmodel_indices: Arc<Vec<u32>>,
    pub xmodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub xmodel_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub fx_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub fx_indices: Arc<Vec<u32>>,
    pub fx_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub fx_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub fx_revision: u64,
    pub xmodel_revision: u64,
    pub xmodel_topology_revision: u64,
    pub particle_cloud_vertices: Arc<Vec<[u8; fx_iw4::GFX_POS_TEX_VERTEX_STRIDE]>>,
    pub particle_cloud_indices: Arc<Vec<u32>>,
    pub particle_cloud_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub particle_cloud_template: (usize, usize),
    pub particle_cloud_revision: u64,
    pub mark_mesh_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>>,
    pub mark_mesh_indices: Arc<Vec<u16>>,
    pub mark_mesh_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub mark_mesh_revision: u64,
    pub glass_mesh_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub glass_mesh_indices: Arc<Vec<u32>>,
    pub glass_mesh_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub glass_mesh_revision: u64,
    pub glass_mesh_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub glass_mesh_bounds: Arc<Vec<[f32; 4]>>,
    pub exec_frame: MaterialExecFrame,
}

#[derive(Clone, Copy)]
pub(super) struct ExtractedColourRefs<'a> {
    pub(super) world: &'a InstalledRenderWorld,
    pub(super) frame: &'a PublishedRenderFrame,
}

impl<'a> ExtractedColourRefs<'a> {
    pub(super) fn new(frame: &'a PublishedRenderFrame) -> Self {
        Self {
            world: frame.world(),
            frame,
        }
    }
}
