use bevy::prelude::*;
use render_frame::{SmodelVertex, SourceRevisions};
use render_scene::{SmodelPassMaterial, XModelSurfaceDraw};
use std::sync::Arc;

pub const XMODEL_OBJECT_ID_FX_BASE: u16 = 0x700;

#[derive(Clone, Debug, PartialEq)]
pub struct FxModelAssetDraw {
    pub model_index: usize,
    pub lod: u8,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FxModelGeometry {
    pub(crate) vertices: Vec<SmodelVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) assets: Vec<FxModelAssetDraw>,
    pub(crate) packed_vertices: assets::RetailPackedVertexPayload,
}

#[derive(Resource, Default)]
pub struct FxModelStaging(pub FxModelDrawPlan);

#[derive(Resource, Clone, Debug, Default)]
pub struct PreparedFxModelGeometry(pub FxModelDrawPlan);

#[derive(Resource, Clone, Debug, Default)]
pub struct FxModelDrawPlan {
    pub(crate) geometry: Arc<FxModelGeometry>,
    pub(crate) draws: Vec<XModelSurfaceDraw>,
    pub generated: u32,
    pub skipped_no_catalog: u32,
    pub skipped_no_pose: u32,
    pub skipped_no_lod: u32,
    pub skipped_culled: u32,
    pub skipped_no_material: u32,
    pub skipped_no_lighting: u32,
    pub skipped_render_fx_flags: u32,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
}

impl FxModelDrawPlan {
    pub fn draws(&self) -> &[XModelSurfaceDraw] {
        &self.draws
    }

    pub fn push_draw(&mut self, draw: XModelSurfaceDraw) {
        self.draws.push(draw);
    }

    pub fn materials(&self) -> &[SmodelPassMaterial] {
        &self.geometry.materials
    }

    pub fn vertices(&self) -> &[SmodelVertex] {
        &self.geometry.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.geometry.indices
    }

    pub fn surface_ranges(&self) -> &[(u32, u32)] {
        &self.geometry.surface_ranges
    }

    pub fn packed_vertices(&self) -> &assets::RetailPackedVertexPayload {
        &self.geometry.packed_vertices
    }

    pub fn clear(&mut self) {
        self.draws.clear();
        self.generated = 0;
        self.skipped_no_catalog = 0;
        self.skipped_no_pose = 0;
        self.skipped_no_lod = 0;
        self.skipped_culled = 0;
        self.skipped_no_material = 0;
        self.skipped_no_lighting = 0;
        self.skipped_render_fx_flags = 0;
    }

    pub fn use_prepared(&mut self, prepared: &Self) {
        self.geometry = Arc::clone(&prepared.geometry);
        self.generation = prepared.generation;
    }

    pub fn publish_rebuild(&mut self, staged: &mut Self) {
        self.generation = staged.generation;
        let geometry = !Arc::ptr_eq(&self.geometry, &staged.geometry);
        if geometry {
            self.geometry = Arc::clone(&staged.geometry);
            self.revisions.set_topology_from(
                &self.geometry.indices,
                &self.geometry.surface_ranges,
                self.geometry.vertices.len(),
            );
            self.revisions.bump_packed_write(&mut self.revision);
        }
        if render_frame::publish_rows(&mut self.draws, &mut staged.draws) || geometry {
            self.revisions.bump_draws();
        }
        self.generated = staged.generated;
        self.skipped_no_catalog = staged.skipped_no_catalog;
        self.skipped_no_pose = staged.skipped_no_pose;
        self.skipped_no_lod = staged.skipped_no_lod;
        self.skipped_culled = staged.skipped_culled;
        self.skipped_no_material = staged.skipped_no_material;
        self.skipped_no_lighting = staged.skipped_no_lighting;
        self.skipped_render_fx_flags = staged.skipped_render_fx_flags;
    }

    pub fn asset(&self, model_index: usize, lod: u8) -> Option<&FxModelAssetDraw> {
        self.geometry
            .assets
            .iter()
            .find(|asset| asset.model_index == model_index && asset.lod == lod)
    }
}

impl FxModelDrawPlan {
    pub fn finalize_lighting(&mut self, resolved: &render_scene::ResolvedModelLightingTable) {
        use render_scene::ResolvedModelLighting;
        let mut failed = Vec::new();
        let mut changed = false;
        for draw in self.draws.iter_mut() {
            let Some(request) = draw.pending_lighting.take() else {
                continue;
            };
            match resolved.get(request.owner) {
                Some(ResolvedModelLighting::Seated {
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    packed_lighting,
                }) => {
                    changed |= (
                        draw.lighting_handle,
                        draw.scene_light_index,
                        draw.reflection_probe_index,
                        draw.packed_lighting,
                    ) != (
                        handle,
                        scene_light_index,
                        reflection_probe_index,
                        packed_lighting,
                    );
                    draw.lighting_handle = handle;
                    draw.scene_light_index = scene_light_index;
                    draw.reflection_probe_index = reflection_probe_index;
                    draw.packed_lighting = packed_lighting;
                }
                Some(ResolvedModelLighting::Failed) | None => failed.push(draw.object_id),
            }
        }
        if !failed.is_empty() {
            self.revisions.bump_admission();
            changed = true;
            failed.sort_unstable();
            failed.dedup();
            self.skipped_no_lighting = self.skipped_no_lighting.saturating_add(failed.len() as u32);
            self.draws.retain(|draw| !failed.contains(&draw.object_id));
        }
        if changed {
            self.revisions.bump_draws();
        }
    }
}
