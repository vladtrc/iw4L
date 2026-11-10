use fastfile_iw4::{GfxWorldGeometry, ZoneStream};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRegion {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl WorldRegion {
    pub fn new(min: [f32; 3], max: [f32; 3]) -> Result<Self, String> {
        if (0..3).any(|i| !min[i].is_finite() || !max[i].is_finite() || min[i] >= max[i]) {
            return Err("world region needs finite min < max on every axis".into());
        }
        Ok(Self { min, max })
    }

    pub fn contains(self, point: [f32; 3]) -> bool {
        (0..3).all(|i| point[i] >= self.min[i] && point[i] <= self.max[i])
    }

    pub fn intersects(self, min: [f32; 3], max: [f32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= max[i] && self.max[i] >= min[i])
    }

    pub fn surfaces(
        self,
        s: &ZoneStream<'_>,
        g: GfxWorldGeometry,
    ) -> Result<Vec<bool>, crate::WorldMeshError> {
        Ok(self
            .triangles(s, g)?
            .iter()
            .map(|triangles| triangles.iter().any(|keep| *keep))
            .collect())
    }

    pub fn triangles(
        self,
        s: &ZoneStream<'_>,
        g: GfxWorldGeometry,
    ) -> Result<Vec<Vec<bool>>, crate::WorldMeshError> {
        use asset_iw4::size as sz;
        let surfaces = g.surfaces.ok_or(crate::WorldMeshError::NoGeometry)?;
        let vertices = g.vertices.ok_or(crate::WorldMeshError::NoGeometry)?;
        let indices = g.indices.ok_or(crate::WorldMeshError::NoGeometry)?;
        let mut submodel = vec![false; g.surface_count];
        if let Some(models) = g.models {
            for i in 1..g.model_count {
                let model = models.at(i * sz::GFX_BRUSH_MODEL);
                let start = s.u16_at(model, sz::gfx_brush_model::START_SURF)? as usize;
                let count = s.u16_at(model, sz::gfx_brush_model::SURFACE_COUNT)? as usize;
                let end = (start + count).min(submodel.len());
                submodel[start.min(end)..end].fill(true);
            }
        }
        (0..g.surface_count)
            .map(|i| {
                let surface = surfaces.at(i * s.layout(sz::GFX_SURFACE, 32));
                let first = s.u32_at(surface, sz::gfx_surface::FIRST_VERTEX)? as usize;
                let count = s.u16_at(surface, sz::gfx_surface::TRI_COUNT)? as usize;
                let base = s.u32_at(surface, sz::gfx_surface::BASE_INDEX)? as usize;
                (0..count)
                    .map(|triangle| {
                        let mut min = [f32::INFINITY; 3];
                        let mut max = [f32::NEG_INFINITY; 3];
                        for corner in 0..3 {
                            let offset = base + triangle * 3 + corner;
                            if offset >= g.index_count {
                                return Ok(false);
                            }
                            let vertex = first + s.u16_at(indices, offset * 2)? as usize;
                            if vertex >= g.vertex_count {
                                return Ok(false);
                            }
                            for axis in 0..3 {
                                let value =
                                    s.f32_at(vertices.at(vertex * sz::GFX_WORLD_VERTEX), axis * 4)?;
                                min[axis] = min[axis].min(value);
                                max[axis] = max[axis].max(value);
                            }
                        }
                        Ok(submodel[i] || self.intersects(min, max))
                    })
                    .collect()
            })
            .collect()
    }

    pub fn smodels(
        self,
        s: &ZoneStream<'_>,
        g: GfxWorldGeometry,
    ) -> Result<Vec<bool>, crate::WorldMeshError> {
        let Some(instances) = g.smodel_insts else {
            return Ok(vec![false; g.smodel_count]);
        };
        (0..g.smodel_count)
            .map(|i| {
                let p = instances.at(i * asset_iw4::size::GFX_STATIC_MODEL_INST);
                let mut min = [0.; 3];
                let mut max = [0.; 3];
                for axis in 0..3 {
                    let mid = s.f32_at(p, axis * 4)?;
                    let half = s.f32_at(p, 12 + axis * 4)?;
                    min[axis] = mid - half;
                    max[axis] = mid + half;
                }
                Ok(self.intersects(min, max))
            })
            .collect()
    }
}
