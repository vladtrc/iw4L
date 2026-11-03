//! Another map of a projectile_flight atlas, drawn by the match together with its own.
//!
//! The sim keeps atlas world `i` at `x + i·ATLAS_SLAB` (`sim::world_graph::world_offset`).
//! A layer is that world's draw data moved there and appended to the match's
//! one `WorldDraw`: its surfaces, vertices, batches, lightmaps, probes and
//! primary lights come after the match's own, every index into them shifted.
//! Its DPVS stays its own (`AtlasLayer`), in the shared coordinates but with
//! layer-local surface and static-model numbers, so a camera in that world
//! walks that world's cells.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use dpvs_iw4::Bounds;

use crate::{DpvsWorldData, SurfaceCastsSunShadow, WorldDraw, WorldShadowGeometry};

/// The distance along x between atlas worlds; `sim::world_graph::ATLAS_SLAB`.

/// Exact authored geometry replaced by a portal. Indices remain stable: render
/// instances become empty slots and collision brushes keep their BSP indices.
#[derive(Clone, Debug)]
pub struct PortalDoorReplacement {
    pub world: usize,
    pub door_id: String,
    pub render_instance: usize,
    pub model_name: String,
    pub model_origin: [f32; 3],
    pub clip_models: Vec<u32>,
    pub clip_brushes: Vec<usize>,
    pub leaf: AtlasDoorway,
}

impl PortalDoorReplacement {
    pub fn apply_all_collision(
        replacements: &[Self],
        clip: &mut crate::ClipCollision,
    ) -> Result<(), String> {
        let mut cuts = std::collections::BTreeMap::<usize, Vec<[[f32; 4]; 6]>>::new();
        for replacement in replacements {
            for index in &replacement.clip_models {
                let model = clip
                    .static_models
                    .iter()
                    .find(|m| m.index == *index)
                    .ok_or_else(|| {
                        format!("{}: collision model {index} is absent", replacement.door_id)
                    })?;
                if model.name != replacement.model_name
                    || (0..3)
                        .any(|i| (model.model.origin[i] - replacement.model_origin[i]).abs() > 0.1)
                {
                    return Err(format!(
                        "{}: collision model {index} identity changed",
                        replacement.door_id
                    ));
                }
            }
            let door = replacement.leaf;
            let normal = Vec3::from_array(door.normal).normalize();
            let across = Vec3::new(-normal.y, normal.x, 0.0);
            let middle = Vec3::from_array(door.base) + Vec3::Z * door.height * 0.5;
            let plane = |n: Vec3, half: f32| [n.x, n.y, n.z, n.dot(middle) + half];
            let aperture = [
                plane(across, door.width * 0.5),
                plane(-across, door.width * 0.5),
                plane(normal, 16.0),
                plane(-normal, 16.0),
                plane(Vec3::Z, door.height * 0.5),
                plane(-Vec3::Z, door.height * 0.5),
            ];
            for &index in &replacement.clip_brushes {
                if index >= clip.brushes.len() {
                    return Err(format!("{}: brush {index} is absent", replacement.door_id));
                }
                cuts.entry(index).or_default().push(aperture);
            }
        }
        clip.static_models.retain(|m| {
            !replacements
                .iter()
                .any(|r| r.clip_models.contains(&m.index))
        });
        let mut remap = std::collections::BTreeMap::<usize, Vec<u16>>::new();
        for (index, apertures) in cuts {
            let mut fragments = vec![clip.brushes[index].clone()];
            for aperture in apertures {
                fragments = fragments
                    .into_iter()
                    .flat_map(|brush| subtract_aperture(brush, aperture))
                    .collect();
            }
            let mut ids = Vec::new();
            if fragments.is_empty() {
                clip.brushes[index].contents = 0;
            } else {
                for (part, brush) in fragments.into_iter().enumerate() {
                    let id = if part == 0 {
                        clip.brushes[index] = brush;
                        index
                    } else {
                        let id = clip.brushes.len();
                        clip.brushes.push(brush);
                        id
                    };
                    ids.push(
                        u16::try_from(id)
                            .map_err(|_| "portal collision exceeds brush index capacity")?,
                    );
                }
            }
            remap.insert(index, ids);
        }
        // Keep original ranges available to other native clip owners, and
        // append the carved leaf/cmodel ranges without changing their IDs.
        let mut references = clip.leafbrushes.clone();
        for leaf in &mut clip.leaves {
            let start = references.len();
            for id in &clip.leafbrushes
                [leaf.first_brush as usize..leaf.first_brush as usize + leaf.num_brushes as usize]
            {
                if let Some(parts) = remap.get(&usize::from(*id)) {
                    references.extend_from_slice(parts);
                } else {
                    references.push(*id);
                }
            }
            leaf.first_brush = u32::try_from(start)
                .map_err(|_| "portal collision exceeds leaf reference capacity")?;
            leaf.num_brushes = u16::try_from(references.len() - start)
                .map_err(|_| "portal collision exceeds leaf brush capacity")?;
        }
        for model in &mut clip.cmodels {
            let start = references.len();
            for id in &clip.leafbrushes[model.first_brush as usize
                ..model.first_brush as usize + model.num_brushes as usize]
            {
                if let Some(parts) = remap.get(&usize::from(*id)) {
                    references.extend_from_slice(parts);
                } else {
                    references.push(*id);
                }
            }
            model.first_brush = u32::try_from(start)
                .map_err(|_| "portal collision exceeds cmodel reference capacity")?;
            model.num_brushes = u16::try_from(references.len() - start)
                .map_err(|_| "portal collision exceeds cmodel brush capacity")?;
        }
        clip.leafbrushes = references;
        Ok(())
    }
}

fn subtract_aperture(
    mut inside: crate::ClipBrush,
    aperture: [[f32; 4]; 6],
) -> Vec<crate::ClipBrush> {
    let mut outside = Vec::new();
    for plane in aperture {
        let mut fragment = inside.clone();
        fragment.planes.push(plane.map(|v| -v));
        fragment.plane_surface_flags.push(0);
        if convex_volume(&fragment.planes) {
            outside.push(fragment);
        }
        inside.planes.push(plane);
        inside.plane_surface_flags.push(0);
        if !convex_volume(&inside.planes) {
            break;
        }
    }
    outside
}

fn convex_volume(planes: &[[f32; 4]]) -> bool {
    let mut vertices = Vec::<Vec3>::new();
    for a in 0..planes.len() {
        for b in a + 1..planes.len() {
            for c in b + 1..planes.len() {
                let rows = [planes[a], planes[b], planes[c]];
                let m = Mat3::from_cols(
                    Vec3::from_slice(&rows[0][..3]),
                    Vec3::from_slice(&rows[1][..3]),
                    Vec3::from_slice(&rows[2][..3]),
                )
                .transpose();
                if m.determinant().abs() < 1e-5 {
                    continue;
                }
                let p = m.inverse() * Vec3::new(rows[0][3], rows[1][3], rows[2][3]);
                if planes
                    .iter()
                    .all(|r| Vec3::new(r[0], r[1], r[2]).dot(p) - r[3] < 0.01)
                {
                    vertices.push(p);
                }
            }
        }
    }
    let Some(&origin) = vertices.first() else {
        return false;
    };
    let Some(axis) = vertices
        .iter()
        .map(|p| *p - origin)
        .find(|p| p.length_squared() > 0.001)
    else {
        return false;
    };
    let Some(normal) = vertices
        .iter()
        .map(|p| axis.cross(*p - origin))
        .find(|p| p.length_squared() > 0.001)
    else {
        return false;
    };
    vertices
        .iter()
        .any(|p| normal.normalize().dot(*p - origin).abs() > 0.01)
}

/// The DPVS of one appended world.
#[derive(Clone, Debug)]
pub struct AtlasLayer {
    /// The atlas world this layer is.
    pub world: usize,
    pub bmodel_poses: Vec<(usize, Mat4)>,
    pub bmodel_bounds: Vec<(u16, Bounds)>,
    pub shift: [f32; 3],
    /// The layer's surface `s` is the merged surface `surface_base + s`.
    pub surface_base: u32,
    pub surface_count: u32,
    /// The layer's static model `m` is the merged one `smodel_base + m`.
    pub smodel_base: u32,
    /// Coordinates shifted; surface and static-model numbers layer-local.
    pub dpvs: DpvsWorldData,
    /// What the layer's map lights its own materials with.
    pub lighting: AtlasLayerLighting,
    pub light_grid: Option<asset_model::OwnedLightGrid>,
    /// The layer map's sky, drawn around the eye (T5, T6); material slots
    /// the match's.
    pub sky_model: Option<crate::model_mesh::ModelMesh>,
}

/// A layer map's world-wide lighting inputs. The match's map has its own in
/// `WorldDraw`; a layer keeps these so its family's code constants are its
/// map's, not the match's.
#[derive(Clone, Debug, Default)]
pub struct AtlasLayerLighting {
    pub source_namespace: Option<asset_core::FamilyId>,
    pub light_grid_codec: Option<&'static asset_model::LightGridCodec>,
    pub exp_fog: Option<crate::ExpFog>,
    pub world_bounds: Option<[f32; 6]>,
    pub t5_sun_parse_exposure: Option<f32>,
    pub t6_exposure: Option<f32>,
    pub sky_dynamic_intensity: Option<[f32; 4]>,
    pub t5_sun_light: Option<crate::WorldSunLight>,
    /// The layer's `is_sun` directional primary light, moved with it.
    pub sun_primary_light: Option<crate::WorldPrimaryLight>,
}

/// A doorway of one atlas world, in that world's native coordinates; the
/// render side of `sim::world_graph::AtlasDoor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasDoorway {
    pub world: usize,
    /// Bottom centre of the opening.
    pub base: [f32; 3],
    /// Horizontal unit normal of the opening.
    pub normal: [f32; 3],
    pub width: f32,
    pub height: f32,
}

/// Two doorways that are one; `flip` as `sim::world_graph::AtlasPortal`'s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasDoorPair {
    pub two_sided: [bool; 2],
    pub a: AtlasDoorway,
    pub b: AtlasDoorway,
    pub flip: bool,
}

#[derive(Debug)]
pub enum AtlasLayerError {
    /// A per-surface `u8` index (lightmap, probe, primary light) or a `u16`
    /// surface number would not fit.
    Overflow { what: &'static str, count: usize },
    /// The two worlds keep different packed vertex rows.
    VertexPayload(&'static str),
}

impl std::fmt::Display for AtlasLayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow { what, count } => {
                write!(f, "atlas layer: {count} {what} do not fit the merged world")
            }
            Self::VertexPayload(cause) => write!(f, "atlas layer: vertex payload {cause}"),
        }
    }
}

impl std::error::Error for AtlasLayerError {}

fn moved(p: [f32; 3], shift: [f32; 3]) -> [f32; 3] {
    [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]]
}

fn moved_bounds(bounds: Bounds, shift: [f32; 3]) -> Bounds {
    if bounds.is_cleared() {
        return bounds;
    }
    Bounds::from_mid_half(moved(bounds.mid(), shift), bounds.half())
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn u8_base(what: &'static str, base: usize, extra: usize) -> Result<u8, AtlasLayerError> {
    let count = base + extra;
    if count > 255 {
        return Err(AtlasLayerError::Overflow { what, count });
    }
    Ok(base as u8)
}

impl DpvsWorldData {
    /// The same cells `shift` away.
    fn moved(mut self, shift: [f32; 3]) -> Self {
        for plane in &mut self.planes {
            plane.dist += dot(plane.normal, shift);
        }
        for tree in &mut self.aabb_trees {
            for node in tree {
                node.bounds = moved_bounds(node.bounds, shift);
            }
        }
        for bounds in self
            .surface_bounds
            .iter_mut()
            .chain(self.smodel_bounds.iter_mut())
        {
            *bounds = moved_bounds(*bounds, shift);
        }
        for vert in &mut self.portal_verts {
            *vert = moved(*vert, shift);
        }
        for cell in &mut self.portals_per_cell {
            for portal in cell {
                let normal = [portal.plane[0], portal.plane[1], portal.plane[2]];
                // Portal planes use n·p + d, unlike the BSP CPlane distance above.
                portal.plane[3] -= dot(normal, shift);
            }
        }
        self
    }
}

impl WorldDraw {
    /// Appends `extra`, atlas world `world`, moved by `shift`.
    ///
    /// `materials` takes `extra`'s walk-local material slots to the match's;
    /// `smodel_base` is where `extra`'s static models start in the match's.
    pub fn append_atlas_layer(
        &mut self,
        world: usize,
        mut extra: WorldDraw,
        shift: [f32; 3],
        materials: &[usize],
        smodel_base: usize,
    ) -> Result<(), AtlasLayerError> {
        let material = |slot: Option<usize>| slot.and_then(|slot| materials.get(slot).copied());
        let vertex_base = self.positions.len() as u32;
        let surface_base = self.surface_draw_fields.len();
        let surface_count = extra.surface_draw_fields.len();
        if self.brush_models.len() + extra.brush_models.len() > usize::from(u16::MAX) {
            return Err(AtlasLayerError::Overflow {
                what: "brush models",
                count: self.brush_models.len() + extra.brush_models.len(),
            });
        }

        if surface_base + surface_count > usize::from(u16::MAX) {
            return Err(AtlasLayerError::Overflow {
                what: "surfaces",
                count: surface_base + surface_count,
            });
        }
        for (local, model) in extra.brush_models.iter().enumerate() {
            self.brush_models.push(crate::GfxBrushModelSurfs {
                start_surf: if local == 0 || model.surface_count == 0 {
                    0
                } else {
                    u16::try_from(surface_base + usize::from(model.start_surf)).map_err(|_| {
                        AtlasLayerError::Overflow {
                            what: "brush surfaces",
                            count: surface_base + usize::from(model.start_surf),
                        }
                    })?
                },
                // Each native model zero is the whole world, not an entity.
                surface_count: if local == 0 { 0 } else { model.surface_count },
                surface_count_no_decal: if local == 0 {
                    0
                } else {
                    model.surface_count_no_decal
                },
            });
        }
        // Brush bounds stay model-local; their instance pose carries the slab.
        self.brush_model_bounds
            .extend(extra.brush_model_bounds.iter().copied());
        let lightmap_n = self.lightmap.as_ref().map_or(0, Vec::len);
        let extra_lightmaps = match std::mem::replace(&mut extra.lightmap, Ok(Vec::new())) {
            Ok(pages) => pages,
            Err(_) => Vec::new(),
        };
        let lightmap_base = u8_base("lightmaps", lightmap_n, extra_lightmaps.len())?;
        let probe_base = u8_base(
            "reflection probes",
            self.reflection_probes.len(),
            extra.reflection_probes.len(),
        )?;
        let light_base = u8_base(
            "primary lights",
            self.primary_lights.len(),
            extra.primary_lights.len(),
        )?;
        let batch_base = self.batches.len();
        let index_base = self.packed_indices.len() as u32;
        let raw_index_base = self
            .surface_draw_fields
            .iter()
            .map(|s| s.base_index + u32::from(s.tri_count) * 3)
            .max()
            .unwrap_or(0);

        match (&mut self.packed_vertices, extra.packed_vertices) {
            (
                crate::WorldVertexPayload::Captured { rows, .. },
                crate::WorldVertexPayload::Captured { rows: more, .. },
            ) => {
                for mut row in more {
                    for axis in 0..3 {
                        let at = axis * 4;
                        let value =
                            f32::from_le_bytes(row[at..at + 4].try_into().unwrap()) + shift[axis];
                        row[at..at + 4].copy_from_slice(&value.to_le_bytes());
                    }
                    rows.push(row);
                }
            }
            (crate::WorldVertexPayload::Unavailable { source_layout }, _) => {
                return Err(AtlasLayerError::VertexPayload(source_layout));
            }
            (_, crate::WorldVertexPayload::Unavailable { source_layout }) => {
                return Err(AtlasLayerError::VertexPayload(source_layout));
            }
        }

        // The parallel layer stream: one row per vertex, zeros where a world has none.
        let vertex_n = self.positions.len();
        let extra_vertex_n = extra.positions.len();
        let layer_base = self.vertex_layer.len() as i32;
        let stride = [
            (&self.vertex_layer, vertex_n),
            (&extra.vertex_layer, extra_vertex_n),
        ]
        .iter()
        .find(|(layer, n)| !layer.is_empty() && *n > 0)
        .map(|(layer, n)| layer.len() / n);
        if let Some(stride) = stride {
            self.vertex_layer.resize(vertex_n * stride, 0);
            if extra.vertex_layer.is_empty() {
                self.vertex_layer
                    .resize((vertex_n + extra_vertex_n) * stride, 0);
            } else {
                self.vertex_layer.extend_from_slice(&extra.vertex_layer);
            }
        }
        self.surface_vertex_layer.extend(
            extra
                .surface_vertex_layer
                .iter()
                .map(|&off| if off < 0 { off } else { off + layer_base }),
        );

        self.surface_first_vertex
            .extend(extra.surface_first_vertex.iter().map(|v| v + vertex_base));
        self.surface_draw_fields
            .extend(
                extra
                    .surface_draw_fields
                    .iter()
                    .map(|s| crate::SurfaceDrawFields {
                        first_vertex: s.first_vertex + vertex_base,
                        tri_count: s.tri_count,
                        base_index: s.base_index + raw_index_base,
                        lightmap_index: s.lightmap_index.saturating_add(lightmap_base),
                        reflection_probe_index: s.reflection_probe_index.saturating_add(probe_base),
                        primary_light_index: s.primary_light_index.saturating_add(light_base),
                    }),
            );

        self.positions
            .extend(extra.positions.iter().map(|&p| moved(p, shift)));
        self.normals.extend(extra.normals);
        self.tangents.extend(extra.tangents);
        self.colors.extend(extra.colors);
        self.texture_uvs.extend(extra.texture_uvs);
        self.lightmap_uvs.extend(extra.lightmap_uvs);
        self.packed_indices
            .extend(extra.packed_indices.iter().map(|i| i + vertex_base));
        self.surface_index_ranges.extend(
            extra
                .surface_index_ranges
                .iter()
                .map(|&(start, count)| (start + index_base, count)),
        );
        self.surface_batch_ranges.extend(
            extra
                .surface_batch_ranges
                .iter()
                .map(|&(batch, start, count)| (batch + batch_base, start, count)),
        );
        self.surface_lightmapped.extend(extra.surface_lightmapped);
        self.surface_lightmap_indices.extend(
            extra
                .surface_lightmap_indices
                .iter()
                .map(|i| i.saturating_add(lightmap_base)),
        );
        self.surface_reflection_probes.extend(
            extra
                .surface_reflection_probes
                .iter()
                .map(|i| i.saturating_add(probe_base)),
        );
        self.surface_primary_lights.extend(
            extra
                .surface_primary_lights
                .iter()
                .map(|i| i.saturating_add(light_base)),
        );
        self.surface_materials
            .extend(extra.surface_materials.iter().map(|&slot| material(slot)));

        for mut batch in extra.batches {
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                batch.mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                for p in positions {
                    *p = moved(*p, shift);
                }
            }
            batch.material = material(batch.material);
            batch.lightmap_index = batch.lightmap_index.saturating_add(lightmap_base);
            batch.reflection_probe_index = batch.reflection_probe_index.saturating_add(probe_base);
            batch.primary_light_index = batch.primary_light_index.saturating_add(light_base);
            self.batches.push(batch);
        }

        match &mut self.lightmap {
            Ok(pages) => pages.extend(extra_lightmaps),
            Err(_) if !extra_lightmaps.is_empty() => {
                // The match's map has no lightmap: its surfaces stay unlit and
                // the layer's pages start at 0, as `lightmap_base` said.
                self.lightmap = Ok(extra_lightmaps);
            }
            Err(_) => {}
        }

        let mut casters = SurfaceCastsSunShadow::with_len(surface_base + surface_count);
        for s in 0..surface_base {
            if self.capture.casters.get(s) {
                casters.set(s);
            }
        }
        for s in 0..surface_count {
            if extra.capture.casters.get(s) {
                casters.set(surface_base + s);
            }
        }
        self.capture.casters = casters;
        self.capture
            .packed_draw_surfs
            .extend(extra.capture.packed_draw_surfs);

        let lighting = AtlasLayerLighting {
            t5_sun_parse_exposure: extra.t5_sun_parse_exposure,
            t6_exposure: extra.t6_exposure,
            sky_dynamic_intensity: extra.sky_dynamic_intensity,
            t5_sun_light: extra.t5_sun_light,
            sun_primary_light: extra
                .primary_lights
                .iter()
                .find(|light| light.is_sun && light.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
                .cloned(),
            ..Default::default()
        };
        for mut light in extra.primary_lights {
            light.origin = moved(light.origin, shift);
            self.primary_lights.push(light);
        }
        self.light_defs.extend(extra.light_defs);
        // The layer's lights cast no shadows and own no light region: both
        // tables are per light, so they are padded to the new count.
        self.shadow_geometry
            .resize(self.primary_lights.len(), WorldShadowGeometry::default());
        if let Some(hulls) = self.light_region_hulls.as_mut() {
            hulls.resize(self.primary_lights.len(), Vec::new());
        }

        for mut probe in extra.reflection_probes {
            probe.origin = moved(probe.origin, shift);
            // A walk-local image slot of the other zone means nothing here;
            // the decoded cube rides in `PreparedWorld::reflection_probe_images`.
            probe.image = None;
            self.reflection_probes.push(probe);
        }

        let sky_model = extra.sky_model.take().map(|mut sky| {
            for surface in sky.lod_surfaces.iter_mut().flatten() {
                surface.material = material(surface.material);
            }
            sky
        });

        self.stats.vertices += extra.stats.vertices;
        self.stats.triangles += extra.stats.triangles;
        self.stats.surfaces += extra.stats.surfaces;
        self.stats.skipped_surfaces += extra.stats.skipped_surfaces;
        self.stats.sky_surfaces += extra.stats.sky_surfaces;

        let mut dpvs = extra.dpvs.moved(shift);
        for cell in &mut dpvs.cell_reflection_probes {
            for probe in cell {
                *probe = probe.saturating_add(probe_base);
            }
        }
        self.atlas_layers.push(AtlasLayer {
            world,
            bmodel_poses: Vec::new(),
            bmodel_bounds: Vec::new(),
            shift,
            surface_base: surface_base as u32,
            surface_count: surface_count as u32,
            smodel_base: smodel_base as u32,
            dpvs,
            lighting,
            light_grid: None,
            sky_model,
        });
        Ok(())
    }
}
