use bevy::prelude::*;
use render_frame::{SmodelVertex, SourceRevisions};
use render_scene::{SmodelPassMaterial, XModelSurfaceDraw};

pub const XMODEL_OBJECT_ID_FX_BASE: u16 = 0x700;

#[derive(Clone, Debug)]
pub struct FxModelAssetDraw {
    pub model_index: usize,
    pub lod: u8,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FxModelDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub assets: Vec<FxModelAssetDraw>,
    pub draws: Vec<XModelSurfaceDraw>,
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
    pub packed_vertices: assets::RetailPackedVertexPayload,
}

impl FxModelDrawPlan {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.assets.clear();
        self.draws.clear();
        self.generated = 0;
        self.skipped_no_catalog = 0;
        self.skipped_no_pose = 0;
        self.skipped_no_lod = 0;
        self.skipped_culled = 0;
        self.skipped_no_material = 0;
        self.skipped_no_lighting = 0;
        self.skipped_render_fx_flags = 0;
        self.revisions.set_topology_from(&[], &[], 0);
        self.revisions.bump_packed_write(&mut self.revision);
        match &mut self.packed_vertices {
            assets::RetailPackedVertexPayload::Iw4(rows) => rows.clear(),
            unavailable @ assets::RetailPackedVertexPayload::Unavailable { .. } => {
                *unavailable = assets::RetailPackedVertexPayload::default();
            }
        }
    }

    pub fn asset(&self, model_index: usize, lod: u8) -> Option<&FxModelAssetDraw> {
        self.assets
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
