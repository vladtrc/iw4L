use std::sync::Arc;

use bevy::prelude::*;
use fx_iw4::{
    FX_PARTICLE_CLOUD_FLAG_SPARK, FX_PARTICLE_CLOUD_GRID_X, FX_PARTICLE_CLOUD_GRID_Y,
    FX_PARTICLE_CLOUD_GRID_Z, FX_PARTICLE_CLOUD_INDICES_PER_CELL, FX_PARTICLE_CLOUD_TEMPLATE_CELLS,
    FX_PARTICLE_CLOUD_VERTS_PER_CELL, FX_PARTICLE_SPARK_INDICES_PER_CELL,
    FX_PARTICLE_SPARK_VERTS_PER_CELL, GFX_POS_TEX_VERTEX_STRIDE, GfxParticleCloud, GfxPosTexVertex,
    MSVCRT_HOLDRAND_DEFAULT, fx_particle_cloud_cell_indices, fx_particle_cloud_cell_radius_sq,
    fx_particle_cloud_cell_verts, fx_particle_cloud_cell_xyz,
    fx_particle_cloud_compare_cell_radius, fx_particle_cloud_draw_counts,
    fx_particle_cloud_particle_id, fx_particle_spark_cell_indices, fx_particle_spark_cell_verts,
    fx_spark_fountain_cell_indices, fx_spark_fountain_index_count, gfx_pos_tex_vertex_bytes,
    msvcrt_rand01, r_add_particle_cloud_custom_allows,
};

use super::fx::FxPassMaterial;

#[derive(Clone, Copy, Debug)]
pub struct FxParticleCloudDraw {
    pub material: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub clouds: [GfxParticleCloud; 3],
}

#[derive(Resource, Clone, Debug)]
pub struct FxParticleCloudPlan {
    pub vertices: Arc<Vec<[u8; GFX_POS_TEX_VERTEX_STRIDE]>>,
    pub indices: Arc<Vec<u32>>,
    pub materials: Vec<FxPassMaterial>,
    pub draws: Vec<FxParticleCloudDraw>,
    pub revision: u64,
    pub miss_material: u32,

    pub spark_index_count: u32,
    template_vert_count: u32,
    template_index_count: u32,
    custom_live: u32,

    pub tmpl_first_xyz: [f32; 3],

    pub tmpl_first_r2: f32,

    pub tmpl_holdrand: u32,

    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
}

impl Default for FxParticleCloudPlan {
    fn default() -> Self {
        let built = spark_template_crt_rand();
        let template_vert_count = built.vertices.len() as u32;
        let template_index_count = built.indices.len() as u32;
        Self {
            vertices: Arc::new(built.vertices),
            indices: Arc::new(built.indices),
            materials: Vec::new(),
            draws: Vec::new(),
            revision: 0,
            miss_material: 0,
            spark_index_count: built.spark_index_count,
            template_vert_count,
            template_index_count,
            custom_live: 0,
            tmpl_first_xyz: built.first_xyz,
            tmpl_first_r2: built.first_r2,
            tmpl_holdrand: built.holdrand,
            range_share: None,
        }
    }
}

impl FxParticleCloudPlan {
    pub fn clear_draws(&mut self) {
        self.materials.clear();
        self.draws.clear();
        self.range_share = None;
        self.miss_material = 0;
        self.custom_live = 0;

        if self.vertices.len() > self.template_vert_count as usize {
            Arc::make_mut(&mut self.vertices).truncate(self.template_vert_count as usize);
        }
        if self.indices.len() > self.template_index_count as usize {
            Arc::make_mut(&mut self.indices).truncate(self.template_index_count as usize);
        }
    }

    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn template_counts(&self) -> (usize, usize) {
        (
            self.template_vert_count as usize,
            self.template_index_count as usize,
        )
    }

    pub fn publish_share(&mut self) {
        self.range_share = Some(super::publish_index_ranges(
            self.draws
                .iter()
                .map(|draw| (draw.index_start, draw.index_count)),
        ));
    }

    pub fn begin_material_draw(
        &mut self,
        color: Option<Handle<Image>>,
        sort_key: u8,
        material_sorted_index: Option<u32>,
        clouds: [GfxParticleCloud; 3],
    ) -> u32 {
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color,
            sort_key,
            material_sorted_index,
        });
        let flags = clouds[0].flags;
        let spark = flags & FX_PARTICLE_CLOUD_FLAG_SPARK != 0;
        let (_, prims) = fx_particle_cloud_draw_counts(spark, flags);
        let index_start = if spark { 0 } else { self.spark_index_count };
        self.draws.push(FxParticleCloudDraw {
            material,
            index_start,
            index_count: prims.wrapping_mul(3),
            clouds,
        });
        self.draws.len() as u32 - 1
    }

    pub fn begin_custom_draw(
        &mut self,
        color: Option<Handle<Image>>,
        sort_key: u8,
        material_sorted_index: Option<u32>,
        cloud: GfxParticleCloud,
        cells: &[[GfxPosTexVertex; 8]],
    ) -> Option<u32> {
        if cloud.flags & FX_PARTICLE_CLOUD_FLAG_SPARK != 0 {
            return None;
        }
        if !r_add_particle_cloud_custom_allows(self.custom_live) {
            return None;
        }
        if cells.is_empty() {
            return None;
        }
        let verts = Arc::make_mut(&mut self.vertices);
        let inds = Arc::make_mut(&mut self.indices);
        let index_start = inds.len() as u32;
        let mut cell = 0u32;
        while cell < cells.len() as u32 {
            let v0 = verts.len() as u32;
            let eight = cells[cell as usize];
            let mut k = 0usize;
            while k < 8 {
                verts.push(gfx_pos_tex_vertex_bytes(eight[k]));
                k += 1;
            }
            for idx in fx_spark_fountain_cell_indices(0) {
                inds.push(v0.wrapping_add(u32::from(idx)));
            }
            cell = cell.saturating_add(1);
        }
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color,
            sort_key,
            material_sorted_index,
        });
        let empty = fx_iw4::fx_empty_particle_cloud();
        self.draws.push(FxParticleCloudDraw {
            material,
            index_start,
            index_count: fx_spark_fountain_index_count(cells.len() as u32),
            clouds: [cloud, empty, empty],
        });
        self.custom_live = self.custom_live.saturating_add(1);
        Some(self.draws.len() as u32 - 1)
    }
}

struct SparkTemplate {
    vertices: Vec<[u8; GFX_POS_TEX_VERTEX_STRIDE]>,
    indices: Vec<u32>,
    spark_index_count: u32,
    first_xyz: [f32; 3],
    first_r2: f32,
    holdrand: u32,
}

fn spark_template_crt_rand() -> SparkTemplate {
    let spark_vert_count =
        FX_PARTICLE_CLOUD_TEMPLATE_CELLS * FX_PARTICLE_SPARK_VERTS_PER_CELL as usize;
    let spark_index_count =
        FX_PARTICLE_CLOUD_TEMPLATE_CELLS * FX_PARTICLE_SPARK_INDICES_PER_CELL as usize;
    let cloud_vert_count =
        FX_PARTICLE_CLOUD_TEMPLATE_CELLS * FX_PARTICLE_CLOUD_VERTS_PER_CELL as usize;
    let cloud_index_count =
        FX_PARTICLE_CLOUD_TEMPLATE_CELLS * FX_PARTICLE_CLOUD_INDICES_PER_CELL as usize;
    let mut holdrand = MSVCRT_HOLDRAND_DEFAULT;
    let mut spark_cells: Vec<[GfxPosTexVertex; 8]> =
        Vec::with_capacity(FX_PARTICLE_CLOUD_TEMPLATE_CELLS);
    let mut cloud_cells: Vec<[GfxPosTexVertex; 4]> =
        Vec::with_capacity(FX_PARTICLE_CLOUD_TEMPLATE_CELLS);
    let mut spark_indices = Vec::with_capacity(spark_index_count);
    for x in 0..FX_PARTICLE_CLOUD_GRID_X {
        for y in 0..FX_PARTICLE_CLOUD_GRID_Y {
            for z in 0..FX_PARTICLE_CLOUD_GRID_Z {
                let id = fx_particle_cloud_particle_id(x, y, z);
                let xyz = fx_particle_cloud_cell_xyz(
                    x,
                    y,
                    z,
                    [
                        msvcrt_rand01(&mut holdrand),
                        msvcrt_rand01(&mut holdrand),
                        msvcrt_rand01(&mut holdrand),
                    ],
                );
                spark_cells.push(fx_particle_spark_cell_verts(xyz));
                cloud_cells.push(fx_particle_cloud_cell_verts(xyz));
                for index in fx_particle_spark_cell_indices(id) {
                    spark_indices.push(u32::from(index));
                }
            }
        }
    }
    spark_cells.sort_unstable_by(|a, b| {
        match fx_particle_cloud_compare_cell_radius(a[0].xyz, b[0].xyz) {
            -1 => core::cmp::Ordering::Less,
            1 => core::cmp::Ordering::Greater,
            _ => core::cmp::Ordering::Equal,
        }
    });
    cloud_cells.sort_unstable_by(|a, b| {
        match fx_particle_cloud_compare_cell_radius(a[0].xyz, b[0].xyz) {
            -1 => core::cmp::Ordering::Less,
            1 => core::cmp::Ordering::Greater,
            _ => core::cmp::Ordering::Equal,
        }
    });
    let first_xyz = spark_cells[0][0].xyz;
    let first_r2 = fx_particle_cloud_cell_radius_sq(first_xyz);
    let mut vertices = Vec::with_capacity(spark_vert_count + cloud_vert_count);
    for cell in &spark_cells {
        for vert in cell {
            vertices.push(gfx_pos_tex_vertex_bytes(*vert));
        }
    }
    for cell in &cloud_cells {
        for vert in cell {
            vertices.push(gfx_pos_tex_vertex_bytes(*vert));
        }
    }
    let spark_base = spark_vert_count as u32;
    let mut indices = spark_indices;
    for cell in 0..FX_PARTICLE_CLOUD_TEMPLATE_CELLS as u32 {
        for index in fx_particle_cloud_cell_indices(cell) {
            indices.push(spark_base.wrapping_add(u32::from(index)));
        }
    }
    debug_assert_eq!(vertices.len(), spark_vert_count + cloud_vert_count);
    debug_assert_eq!(indices.len(), spark_index_count + cloud_index_count);
    SparkTemplate {
        vertices,
        indices,
        spark_index_count: spark_index_count as u32,
        first_xyz,
        first_r2,
        holdrand,
    }
}
