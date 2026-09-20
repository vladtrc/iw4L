use std::sync::Arc;

use asset_iw4::size::GFX_WORLD_VERTEX;
use bevy::prelude::*;
use marks_iw4::GFX_MARK_MESH_VERTEX_STRIDE;

use super::fx::FxPassMaterial;
use render_frame::{SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs};

const _: () = assert!(GFX_MARK_MESH_VERTEX_STRIDE == GFX_WORLD_VERTEX);

#[derive(Clone, Copy, Debug)]
pub struct GfxMarkMeshDraw {
    pub material: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub sub_key: GfxMarkSubKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxMarkSubKey {
    pub lmap: u8,
    pub packed: bool,
    pub entity: Option<u16>,
    pub smodel: Option<u16>,
    pub glass: Option<u16>,
    pub primary_light: u8,
    pub probe: u8,
}

impl GfxMarkSubKey {
    pub fn from_context(context: &[u8; 7]) -> Self {
        Self {
            packed: !matches!(context[0], 0 | 2),
            glass: (context[0] == 4).then(|| u16::from_le_bytes([context[2], context[3]])),
            entity: (context[0] == 3).then(|| u16::from_le_bytes([context[2], context[3]])),
            smodel: (context[0] & 0xc0 == 0x40)
                .then(|| u16::from_le_bytes([context[2], context[3]])),

            lmap: if context[0] == 3 {
                marks_iw4::GFX_SURFACE_LIGHTMAP_NONE
            } else {
                marks_iw4::fx_mark_context_lmap(context)
            },
            primary_light: marks_iw4::fx_mark_context_primary_light(context),
            probe: marks_iw4::fx_mark_context_probe(context),
        }
    }
}

pub fn mark_mesh_surface_samplers(sub_key: GfxMarkSubKey) -> SurfaceSamplerInputs {
    let has_lmap = sub_key.lmap != marks_iw4::GFX_SURFACE_LIGHTMAP_NONE;
    SurfaceSamplerInputs {
        reflection_probe: Some(SurfaceReflectionProbeId(sub_key.probe)),
        primary_lightmap: has_lmap.then_some(SurfaceLightmapId(sub_key.lmap)),
        secondary_lightmap: has_lmap.then_some(SurfaceLightmapId(sub_key.lmap)),
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct GfxMarkMeshPlan {
    pub vertices: Arc<Vec<[u8; GFX_WORLD_VERTEX]>>,
    pub indices: Arc<Vec<u16>>,
    pub materials: Vec<FxPassMaterial>,
    pub draws: Vec<GfxMarkMeshDraw>,
    pub revision: u64,
    pub skip_why: Option<&'static str>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
}

impl GfxMarkMeshPlan {
    fn inds_mut(&mut self) -> &mut Vec<u16> {
        Arc::make_mut(&mut self.indices)
    }

    pub fn clear(&mut self) {
        super::reset_rows(&mut self.vertices);
        super::reset_rows(&mut self.indices);
        self.range_share = None;
        self.materials.clear();
        self.draws.clear();
        self.skip_why = None;
    }

    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn publish_share(&mut self) {
        self.range_share = Some(super::publish_index_ranges(
            self.draws
                .iter()
                .map(|draw| (draw.index_start, draw.index_count)),
        ));
    }

    pub fn packed_rows(&self) -> &[[u8; GFX_WORLD_VERTEX]] {
        self.vertices.as_slice()
    }

    pub fn index_rows(&self) -> &[u16] {
        self.indices.as_slice()
    }

    pub fn append_run_indices(
        &mut self,
        sort_key: u8,
        material_sorted_index: Option<u32>,
        sub_key: GfxMarkSubKey,
        indices: &[u16],
    ) {
        let index_start = self.indices.len() as u32;
        let index_count = indices.len() as u32;
        self.inds_mut().extend_from_slice(indices);
        if let Some(run) = self.draws.last_mut() {
            let material = &self.materials[run.material as usize];
            if material.sort_key == sort_key
                && material.material_sorted_index == material_sorted_index
                && run.sub_key == sub_key
                && run.index_start + run.index_count == index_start
            {
                run.index_count += index_count;
                return;
            }
        }
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color: None,
            sort_key,
            material_sorted_index,
        });
        self.draws.push(GfxMarkMeshDraw {
            material,
            index_start,
            index_count,
            sub_key,
        });
    }
}
