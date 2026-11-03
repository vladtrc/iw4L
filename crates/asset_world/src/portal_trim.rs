//! Four physical borders fitted to the measured visible opening.
use crate::{
    AtlasDoorway, CameraRangeKind, CameraSurfRange, CameraSurfRanges, ClipBrush, ClipCollision,
    SurfaceCastsSunShadow, SurfaceDrawFields, WorldBatch, WorldDraw, WorldVertexPayload,
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use dpvs_iw4::{Bounds, DpvsPlanes};

#[derive(Clone, Debug)]
pub struct PortalDoorTrim {
    pub clearance: PortalClearance,
    pub two_sided: bool,
    pub world: usize,
    pub door_id: String,
    pub opening_base: [f32; 3],
    pub opening_size: [f32; 2],
    pub aperture: AtlasDoorway,
    pub thickness: f32,
}

pub use asset_core::atlas::PortalClearance;

#[derive(Clone, Debug)]
pub struct PortalTrimBox {
    pub center: Vec3,
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

/// Supplemental BSP faces retain native cell visibility without modifying the
/// authored AABB tree's sorted spans.
#[derive(Clone, Debug)]
pub struct GeneratedSurfaceCells {
    pub surface: usize,
    pub cells: Vec<usize>,
}

impl PortalTrimBox {
    pub fn bounds(&self) -> Bounds {
        let half = self
            .axes
            .iter()
            .zip(self.half.to_array())
            .fold(Vec3::ZERO, |sum, (axis, h)| sum + axis.abs() * h);
        Bounds::from_mid_half(self.center.to_array(), half.to_array())
    }
    pub fn planes(&self) -> Vec<[f32; 4]> {
        self.axes
            .iter()
            .zip(self.half.to_array())
            .flat_map(|(&axis, h)| [axis, -axis].map(|n| [n.x, n.y, n.z, n.dot(self.center) + h]))
            .collect()
    }
    fn faces(&self) -> impl Iterator<Item = ([Vec3; 4], Vec3)> + '_ {
        (0..3).flat_map(move |axis| {
            [-1.0, 1.0].map(move |sign| {
                let normal = self.axes[axis] * sign;
                let c = self.center + normal * self.half[axis];
                let u = self.axes[(axis + 1) % 3] * self.half[(axis + 1) % 3];
                let v = self.axes[(axis + 2) % 3] * self.half[(axis + 2) % 3] * sign;
                ([c - u - v, c + u - v, c + u + v, c - u + v], normal)
            })
        })
    }
}

impl PortalDoorTrim {
    pub fn boxes(&self) -> Vec<PortalTrimBox> {
        let n = Vec3::from_array(self.aperture.normal).normalize();
        let axes = [Vec3::new(-n.y, n.x, 0.0), Vec3::Z, n];
        let [ow, oh] = self.opening_size;
        let origin = Vec3::from_array(self.opening_base);
        let shift = Vec3::from_array(self.aperture.base) - origin;
        let u = shift.dot(axes[0]);
        let z = shift.z;
        let w = self.aperture.width;
        let h = self.aperture.height;
        [
            (-ow * 0.5 - 0.5, u - w * 0.5, -0.5, oh + 0.5),
            (u + w * 0.5, ow * 0.5 + 0.5, -0.5, oh + 0.5),
            (u - w * 0.5, u + w * 0.5, z + h, oh + 0.5),
            (u - w * 0.5, u + w * 0.5, -0.5, z),
        ]
        .into_iter()
        .filter(|&(lo, hi, bottom, top)| hi - lo > 0.001 && top - bottom > 0.001)
        .map(|(lo, hi, bottom, top)| PortalTrimBox {
            center: origin + axes[0] * ((lo + hi) * 0.5) + Vec3::Z * ((bottom + top) * 0.5),
            axes,
            half: Vec3::new(
                (hi - lo) * 0.5,
                (top - bottom) * 0.5,
                self.thickness * 0.5 + 0.5,
            ),
        })
        .collect()
    }

    pub fn apply_collision(&self, clip: &mut ClipCollision) -> Result<(), String> {
        let boxes = self.boxes();
        let first = clip.brushes.len();
        if first + boxes.len() > usize::from(u16::MAX) {
            return Err("door trim exceeds collision brush capacity".into());
        }
        clip.brushes.extend(boxes.iter().map(|b| ClipBrush {
            planes: b.planes(),
            contents: 1,
            plane_surface_flags: vec![0; 6],
            glass_encoded: 0,
        }));
        // Preserve existing cmodel reference ranges and append changed leaf ranges.
        let mut references = clip.leafbrushes.clone();
        for (leaf_id, leaf) in clip.leaves.iter_mut().enumerate() {
            let additions: Vec<_> = boxes
                .iter()
                .enumerate()
                .filter(|(_, b)| box_reaches_leaf(&clip.nodes, b.bounds(), leaf_id))
                .map(|(i, _)| (first + i) as u16)
                .collect();
            if additions.is_empty() {
                continue;
            }
            let begin = references.len();
            let old =
                leaf.first_brush as usize..leaf.first_brush as usize + leaf.num_brushes as usize;
            references.extend_from_slice(
                clip.leafbrushes
                    .get(old)
                    .ok_or("invalid native leaf brush span")?,
            );
            references.extend(additions);
            leaf.first_brush =
                u32::try_from(begin).map_err(|_| "door trim leaf reference overflow")?;
            leaf.num_brushes = u16::try_from(references.len() - begin)
                .map_err(|_| "door trim leaf brush overflow")?;
        }
        clip.leafbrushes = references;
        Ok(())
    }

    pub fn apply_render(&self, draw: &mut WorldDraw) -> Result<usize, String> {
        let boxes = self.boxes();
        if boxes.is_empty() {
            return Ok(0);
        }
        let old_surfaces = draw.surface_draw_fields.len();
        if old_surfaces + boxes.len() * 6 > usize::from(u16::MAX) {
            return Err("door trim exceeds render surface capacity".into());
        }
        for b in &boxes {
            let volume = b.bounds();
            let native = DpvsPlanes {
                planes: &draw.dpvs.planes,
                nodes: &draw.dpvs.nodes,
                cell_count: draw.dpvs.cell_count as u32,
            };
            let mut cells = Vec::new();
            dpvs_iw4::scene_ent_cells(&native, volume, &mut |cell| cells.push(cell as usize));
            if draw.dpvs.cell_count > 0 && cells.is_empty() {
                return Err(format!("{}: trim has no DPVS cell", self.door_id));
            }
            for (points, normal) in b.faces() {
                let source = neighbor_wall(
                    draw,
                    self,
                    points.iter().copied().sum::<Vec3>() * 0.25,
                    normal,
                )
                .ok_or_else(|| format!("{}: no opaque neighboring wall for trim", self.door_id))?;
                let source_fields = draw.surface_draw_fields[source];
                let (index, _) = draw.surface_index_ranges[source];
                let ids: Vec<_> = draw.packed_indices[index as usize..index as usize + 3]
                    .iter()
                    .map(|&v| v as usize)
                    .collect();
                let a = Vec3::from_array(draw.positions[ids[0]]);
                let ab = Vec3::from_array(draw.positions[ids[1]]) - a;
                let ac = Vec3::from_array(draw.positions[ids[2]]) - a;
                let source_normal = Vec3::from_array(draw.normals[ids[0]]);
                let reverse = ab.cross(ac).dot(source_normal) < 0.0;
                let source_uv = ids
                    .iter()
                    .map(|&i| Vec2::from_array(draw.texture_uvs[i]))
                    .collect::<Vec<_>>();
                let light_uv = ids
                    .iter()
                    .map(|&i| Vec2::from_array(draw.lightmap_uvs[i]))
                    .sum::<Vec2>()
                    / 3.0;
                let template = match &draw.packed_vertices {
                    WorldVertexPayload::Captured { rows, .. } => rows[ids[0]],
                    _ => return Err("door trim needs native world vertices".into()),
                };
                let material = draw.surface_materials[source];
                let lightmapped = draw.surface_lightmapped[source];
                let source_layer = draw.surface_vertex_layer.get(source).copied().unwrap_or(-1);
                draw.surface_vertex_layer
                    .resize(draw.surface_draw_fields.len(), -1);
                let stride = if draw.positions.is_empty() {
                    0
                } else {
                    draw.vertex_layer.len() / draw.positions.len()
                };
                let layer_row = draw
                    .vertex_layer
                    .get(ids[0] * stride..(ids[0] + 1) * stride)
                    .unwrap_or(&[])
                    .to_vec();
                let source_color = draw.colors[ids[0]];
                let surface = draw.surface_draw_fields.len();
                let vertex = draw.positions.len() as u32;
                let start = draw.packed_indices.len() as u32;
                let layer_start = draw.vertex_layer.len() as i32;
                let tangent = (points[1] - points[0]).normalize();
                let fold = Quat::from_rotation_arc(normal, source_normal.normalize_or_zero());
                let anchor = points[0];
                let anchor_delta = anchor - a;
                let uvs: Vec<_> = points
                    .iter()
                    .map(|p| {
                        let d = anchor_delta + fold * (*p - anchor);
                        let aa = ab.dot(ab);
                        let bb = ab.dot(ac);
                        let cc = ac.dot(ac);
                        let det = aa * cc - bb * bb;
                        if det.abs() > 1e-8 {
                            let u = (d.dot(ab) * cc - d.dot(ac) * bb) / det;
                            let v = (d.dot(ac) * aa - d.dot(ab) * bb) / det;
                            source_uv[0]
                                + (source_uv[1] - source_uv[0]) * u
                                + (source_uv[2] - source_uv[0]) * v
                        } else {
                            source_uv[0]
                        }
                    })
                    .collect();
                for (p, uv) in points.iter().zip(&uvs) {
                    let mut row = template;
                    for (i, v) in p.to_array().into_iter().enumerate() {
                        row[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    row[12..16].copy_from_slice(&1.0f32.to_le_bytes());
                    for (at, v) in [(20, uv.x), (24, uv.y), (28, light_uv.x), (32, light_uv.y)] {
                        row[at..at + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    row[36..40].copy_from_slice(
                        &asset_model::pack_unit_vec(normal.to_array()).to_le_bytes(),
                    );
                    row[40..44].copy_from_slice(
                        &asset_model::pack_unit_vec(tangent.to_array()).to_le_bytes(),
                    );
                    if let WorldVertexPayload::Captured { rows, .. } = &mut draw.packed_vertices {
                        rows.push(row);
                    }
                    draw.positions.push(p.to_array());
                    draw.normals.push(normal.to_array());
                    draw.tangents.push([tangent.x, tangent.y, tangent.z, 1.0]);
                    draw.colors.push(source_color);
                    draw.texture_uvs.push(uv.to_array());
                    draw.lightmap_uvs.push(light_uv.to_array());
                    if stride > 0 {
                        draw.vertex_layer.extend_from_slice(&layer_row);
                    }
                }
                let indices = if reverse {
                    vec![0, 2, 1, 0, 3, 2]
                } else {
                    vec![0, 1, 2, 0, 2, 3]
                };
                let mut mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
                );
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    points.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![normal.to_array(); 4]);
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_TANGENT,
                    vec![[tangent.x, tangent.y, tangent.z, 1.0]; 4],
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![source_color; 4]);
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    uvs.iter().map(|v| v.to_array()).collect::<Vec<_>>(),
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![light_uv.to_array(); 4]);
                mesh.insert_indices(Indices::U32(indices.clone()));
                draw.surface_batch_ranges.push((draw.batches.len(), 0, 6));
                draw.batches.push(WorldBatch {
                    mesh,
                    packed_indices: indices.iter().map(|i| vertex + i).collect(),
                    material,
                    lightmapped,
                    lightmap_index: source_fields.lightmap_index,
                    primary_light_index: source_fields.primary_light_index,
                    reflection_probe_index: source_fields.reflection_probe_index,
                });
                draw.packed_indices
                    .extend(indices.iter().map(|i| vertex + i));
                draw.surface_first_vertex.push(vertex);
                draw.surface_draw_fields.push(SurfaceDrawFields {
                    first_vertex: vertex,
                    tri_count: 2,
                    base_index: start,
                    ..source_fields
                });
                draw.surface_index_ranges.push((start, 6));
                draw.surface_materials.push(material);
                draw.surface_vertex_layer
                    .push(if source_layer < 0 { -1 } else { layer_start });
                draw.surface_lightmapped.push(lightmapped);
                draw.surface_lightmap_indices
                    .push(source_fields.lightmap_index);
                draw.surface_primary_lights
                    .push(source_fields.primary_light_index);
                draw.surface_reflection_probes
                    .push(source_fields.reflection_probe_index);
                draw.capture
                    .packed_draw_surfs
                    .push(draw.capture.packed_draw_surfs[source]);
                let lo = points
                    .iter()
                    .fold(Vec3::splat(f32::INFINITY), |v, p| v.min(*p));
                let hi = points
                    .iter()
                    .fold(Vec3::splat(f32::NEG_INFINITY), |v, p| v.max(*p));
                draw.dpvs.surface_bounds.push(Bounds::from_mid_half(
                    ((lo + hi) * 0.5).to_array(),
                    ((hi - lo) * 0.5).to_array(),
                ));
                draw.dpvs.generated_surfaces.push(GeneratedSurfaceCells {
                    surface,
                    cells: cells.clone(),
                });
            }
        }
        let mut cast = SurfaceCastsSunShadow::with_len(draw.surface_draw_fields.len());
        for i in 0..cast.len() {
            if i >= old_surfaces || draw.capture.casters.get(i) {
                cast.set(i);
            }
        }
        draw.capture.casters = cast;
        let mut ranges = draw.dpvs.camera_ranges.iter().collect::<Vec<_>>();
        ranges.push(CameraSurfRange {
            kind: CameraRangeKind::LitOpaque,
            begin: old_surfaces as u32,
            end: draw.surface_draw_fields.len() as u32,
        });
        draw.dpvs.camera_ranges = CameraSurfRanges::new(ranges.remove(0), ranges);
        draw.stats.vertices += boxes.len() * 24;
        draw.stats.triangles += boxes.len() * 12;
        draw.stats.surfaces += boxes.len() * 6;
        Ok(boxes.len() * 6)
    }
}

fn neighbor_wall(
    draw: &WorldDraw,
    trim: &PortalDoorTrim,
    center: Vec3,
    normal: Vec3,
) -> Option<usize> {
    let wall = Vec3::from_array(trim.aperture.normal).normalize();
    let choose = |oriented: bool| {
        draw.dpvs
            .camera_ranges
            .iter()
            .filter(|r| r.kind == CameraRangeKind::LitOpaque)
            .flat_map(|r| r.begin..r.end)
            .filter_map(|i| {
                let i = i as usize;
                if draw.dpvs.generated_surfaces.iter().any(|s| s.surface == i)
                    || draw.dpvs.sky_start_surfs.contains(&(i as u32))
                {
                    return None;
                }
                let bounds = draw.dpvs.surface_bounds.get(i)?;
                if draw.surface_materials.get(i)?.is_none()
                    || draw.surface_index_ranges.get(i)?.1 < 3
                {
                    return None;
                }
                let start = draw.surface_index_ranges.get(i)?.0 as usize;
                let ids = [0, 1, 2].map(|offset| draw.packed_indices.get(start + offset).copied());
                let [Some(a), Some(b), Some(c)] = ids else {
                    return None;
                };
                let point = |id: u32| {
                    draw.positions
                        .get(id as usize)
                        .copied()
                        .map(Vec3::from_array)
                };
                let (a, b, c) = (point(a)?, point(b)?, point(c)?);
                if (b - a).cross(c - a).length_squared() <= 1e-8 {
                    return None;
                }
                let vertex = *draw.packed_indices.get(start)? as usize;
                let source = Vec3::from_array(*draw.normals.get(vertex)?).normalize_or_zero();
                if if oriented {
                    source.dot(normal) < 0.7
                } else {
                    source.dot(wall).abs() < 0.7
                } {
                    return None;
                }
                let delta = (center - Vec3::from_array(bounds.mid())).abs()
                    - Vec3::from_array(bounds.half());
                let distance = delta.max(Vec3::ZERO).length_squared();
                (distance < 64. * 64.).then_some((i, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    choose(true).or_else(|| choose(false))
}

fn box_reaches_leaf(nodes: &[crate::ClipBspNode], bounds: Bounds, wanted: usize) -> bool {
    if nodes.is_empty() {
        return true;
    }
    let mut stack = vec![0i32];
    let mut budget = nodes.len() * 2 + 1;
    while let Some(index) = stack.pop() {
        if budget == 0 {
            return false;
        }
        budget -= 1;
        if index < 0 {
            if (!index) as usize == wanted {
                return true;
            }
            continue;
        }
        let Some(node) = nodes.get(index as usize) else {
            return false;
        };
        let n = Vec3::from_slice(&node.plane[..3]);
        let d = n.dot(Vec3::from_array(bounds.mid())) - node.plane[3];
        let r = n.abs().dot(Vec3::from_array(bounds.half())) + 0.125;
        if d >= r {
            stack.push(node.children[0]);
        } else if d <= -r {
            stack.push(node.children[1]);
        } else {
            stack.extend(node.children);
        }
    }
    false
}
