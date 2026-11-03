//! Measure the opening the renderer will actually show after leaf removal.
//! Project opaque BSP/static geometry through the measured wall. Coplanar wall
//! faces matter too, so collision rays alone cannot describe this silhouette.
use super::{Door, MapClip};
use bevy::math::{Mat3, Vec2, Vec3};
use bevy::mesh::{Mesh, VertexAttributeValues};

pub(super) struct VisibleScene<'a> {
    prepared: &'a assets::PreparedMatch,
    bounds: Vec<Option<(Vec3, Vec3)>>,
    access: super::accessibility::NativeAccess,
}

fn positions(mesh: &Mesh) -> Option<&Vec<[f32; 3]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(p) => Some(p),
        _ => None,
    }
}
fn intersects(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}
fn bounds(points: impl Iterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    let (lo, hi) = points.fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(a, b), p| (a.min(p), b.max(p)),
    );
    lo.is_finite().then_some((lo, hi))
}

impl<'a> VisibleScene<'a> {
    pub fn new(prepared: &'a assets::PreparedMatch) -> Result<Self, String> {
        let bounds = prepared
            .world
            .static_model_meshes
            .iter()
            .map(|m| {
                bounds(
                    m.surfaces()
                        .iter()
                        .filter_map(|s| positions(&s.mesh))
                        .flatten()
                        .copied()
                        .map(Vec3::from_array),
                )
            })
            .collect();
        Ok(Self {
            prepared,
            bounds,
            access: super::accessibility::NativeAccess::new(prepared)?,
        })
    }
    fn triangles(&self, door: &Door) -> Result<Vec<[Vec3; 3]>, String> {
        let center = Vec3::from_array(door.base) + Vec3::Z * 64.;
        let half = Vec3::new(220., 220., 160.);
        let region = (center - half, center + half);
        let draw = self
            .prepared
            .world
            .draw
            .as_ref()
            .ok_or("missing render world")?;
        let mut triangles = Vec::new();
        for range in draw
            .dpvs
            .camera_ranges
            .iter()
            .filter(|r| r.kind == asset_world::CameraRangeKind::LitOpaque)
        {
            for surface in range.begin..range.end {
                if draw.dpvs.sky_start_surfs.contains(&surface) {
                    continue;
                }
                let i = surface as usize;
                let Some(b) = draw.dpvs.surface_bounds.get(i) else {
                    continue;
                };
                let b = (
                    Vec3::from_array(b.mid()) - Vec3::from_array(b.half()),
                    Vec3::from_array(b.mid()) + Vec3::from_array(b.half()),
                );
                if !intersects(region, b) {
                    continue;
                }
                let Some(&(start, count)) = draw.surface_index_ranges.get(i) else {
                    continue;
                };
                for tri in draw
                    .packed_indices
                    .get(start as usize..(start + count) as usize)
                    .ok_or("invalid BSP render indices")?
                    .as_chunks::<3>()
                    .0
                {
                    let mut out = [Vec3::ZERO; 3];
                    for (at, &index) in out.iter_mut().zip(tri) {
                        *at = Vec3::from_array(
                            *draw
                                .positions
                                .get(index as usize)
                                .ok_or("invalid BSP render vertex")?,
                        );
                    }
                    triangles.push(out);
                }
            }
        }
        let removed = door
            .replacement
            .as_ref()
            .and_then(|r| r["render_instance"].as_u64());
        for (index, placement) in self
            .prepared
            .world
            .static_model_instances
            .iter()
            .enumerate()
        {
            if removed == Some(index as u64) {
                continue;
            }
            let Some(p) = placement else {
                continue;
            };
            let Some(Some((lo, hi))) = self.bounds.get(p.mesh) else {
                continue;
            };
            let axis = Mat3::from_cols_array_2d(&p.axis) * p.scale;
            let origin = Vec3::from_array(p.origin);
            let world = |v: Vec3| origin + axis * v;
            let b = bounds((0..8).map(|i| {
                world(Vec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                ))
            }))
            .unwrap();
            if !intersects(region, b) {
                continue;
            }
            let mesh = &self.prepared.world.static_model_meshes[p.mesh];
            for surface in mesh.surfaces() {
                let opaque = surface
                    .material
                    .and_then(|m| self.prepared.materials.map_ids.get(m).copied().flatten())
                    .and_then(|m| self.prepared.materials.population.materials.get(m))
                    .is_some_and(|m| m.camera_region == asset_iw4::CAMERA_REGION_LIT_OPAQUE);
                if !opaque {
                    continue;
                }
                let Some(vertices) = positions(&surface.mesh) else {
                    return Err("missing static-model render positions".into());
                };
                let indices = surface
                    .mesh
                    .indices()
                    .map(|v| v.iter().collect::<Vec<_>>())
                    .unwrap_or_else(|| (0..vertices.len()).collect());
                for tri in indices.as_chunks::<3>().0 {
                    let mut out = [Vec3::ZERO; 3];
                    for (at, &i) in out.iter_mut().zip(tri) {
                        *at = world(Vec3::from_array(
                            *vertices.get(i).ok_or("invalid static-model render index")?,
                        ));
                    }
                    if bounds(out.into_iter()).is_some_and(|b| intersects(region, b)) {
                        triangles.push(out);
                    }
                }
            }
        }
        Ok(triangles)
    }

    pub fn refine(&mut self, door: &mut Door) -> Result<(), String> {
        let triangles = self.triangles(door)?;
        let mut native = self
            .prepared
            .clip
            .as_deref()
            .ok_or("missing collision")?
            .clone();
        let mut end = serde_json::json!({"id":format!("{}#{}",door.zone,door.index),"replace":door.replacement});
        end["world"] = serde_json::json!(0);
        let replacements: Vec<_> = assets::parse_portal_replacement(&end, 1)?
            .into_iter()
            .collect();
        asset_world::PortalDoorReplacement::apply_all_collision(
            &replacements.into_iter().take(1).collect::<Vec<_>>(),
            &mut native,
        )?;
        let (brushes, bsp, mesh, _) = session::sim_clip_map(&native);
        let clip = MapClip { brushes, bsp, mesh };
        let old = Vec3::from_array(door.base);
        let n = Vec3::from_array(door.normal).normalize();
        let t = Vec3::new(-n.y, n.x, 0.);
        let mut jambs = [Vec::new(), Vec::new()];
        for (side_index, side) in [-1., 1.].into_iter().enumerate() {
            for extra in [2., 8., 16., 32.] {
                let jamb = old + t * (side * (door.width * 0.5 + extra)) + Vec3::Z * 40.;
                let hits = ray_triangle_hits(&triangles, jamb - n * 96., jamb + n * 96.);
                for (i, &first) in hits.iter().enumerate() {
                    for &last in &hits[i + 1..] {
                        let lo = first - 96.;
                        let hi = last - 96.;
                        if hi - lo > 0.1 && hi - lo <= 128. {
                            jambs[side_index].push((lo, hi, extra));
                        }
                    }
                }
            }
        }
        let distance = |lo: f32, hi: f32| lo.max(0.).max((-hi).max(0.));
        // A diagonal opening can meet different walls at its two jambs.
        // Include individual local slabs, then prefer jointly supported bounds
        // when their distance to the authored door is otherwise equivalent.
        let mut candidates = Vec::new();
        for slabs in &jambs {
            for &(lo, hi, extra) in slabs {
                candidates.push((lo, hi, extra, extra, 1usize));
            }
        }
        for &(alo, ahi, ae) in &jambs[0] {
            for &(blo, bhi, be) in &jambs[1] {
                let lo = alo.max(blo);
                let hi = ahi.min(bhi);
                if hi - lo > 0.1 {
                    candidates.push((lo, hi, ae, be, 2));
                }
            }
        }
        let score = |lo: f32, hi: f32| lo.abs().max(hi.abs()) + distance(lo, hi);
        let (back, front, left_extra, right_extra, _) = candidates
            .into_iter()
            .min_by(|a, b| {
                score(a.0, a.1)
                    .total_cmp(&score(b.0, b.1))
                    .then_with(|| ((a.0 + a.1).abs()).total_cmp(&((b.0 + b.1).abs())))
                    .then_with(|| b.4.cmp(&a.4))
                    .then_with(|| (a.2 + a.3).total_cmp(&(b.2 + b.3)))
            })
            .ok_or("no measured wall faces at jambs")?;
        diag::announce_stdout(&format!(
            "doors: jamb slab {}#{} wall=[{back},{front}] probes=[{left_extra},{right_extra}]",
            door.zone, door.index
        ));
        let plane = old + n * ((front + back) * 0.5);
        door.thickness = front - back;
        let flight = door.clearance == asset_world::PortalClearance::Flight;
        let sides = if flight {
            let dpvs = &self
                .prepared
                .world
                .draw
                .as_ref()
                .ok_or("missing native visibility")?
                .dpvs;
            let planes = dpvs_iw4::DpvsPlanes {
                planes: &dpvs.planes,
                nodes: &dpvs.nodes,
                cell_count: dpvs.cell_count as u32,
            };
            [-1.0, 1.0].map(|side| {
                let eye =
                    (plane + n * (side * (door.thickness * 0.5 + 4.0)) + Vec3::Z * 40.0).to_array();
                planes.cell_for_point(eye).is_some() && !clip.solid_at(eye)
            })
        } else {
            self.access.sides(plane.to_array(), n.to_array())
        };
        if sides == [false, false] {
            return Err("no naturally accessible native side".into());
        }
        door.two_sided = Some(sides == [true, true]);
        let n = if sides == [true, false] { -n } else { n };
        let t = Vec3::new(-n.y, n.x, 0.);
        door.normal = n.to_array();
        diag::announce_stdout(&format!(
            "doors: native sides {}#{} negative={} positive={} canonical_normal={:?}",
            door.zone, door.index, sides[0], sides[1], door.normal
        ));
        diag::announce_stdout(&format!(
            "doors: wall {}#{} plane={plane:?} thickness={} triangles={}",
            door.zone,
            door.index,
            door.thickness,
            triangles.len()
        ));
        let silhouette = Silhouette::new(&triangles, plane, n, t, door.thickness);
        let mut left = f32::NEG_INFINITY;
        let mut right = f32::INFINITY;
        let mut outer_left = f32::INFINITY;
        let mut outer_right = f32::NEG_INFINITY;
        for z in [24., 40., 64.] {
            let (l, r) = silhouette.free(0., z, true)?;
            left = left.max(l);
            right = right.min(r);
            outer_left = outer_left.min(l);
            outer_right = outer_right.max(r);
        }
        if right - left < 34. {
            return Err("visible frame is narrower than player opening".into());
        }
        let floor_at = |u: f32, along: f32| -> Option<f32> {
            let at = plane + t * u + n * along;
            let hit = sim::trace_world_clip(
                &clip.brushes,
                &clip.bsp,
                &clip.mesh,
                &sim::TraceExtents {
                    start: (at + Vec3::Z * 48.).to_array(),
                    end: (at - Vec3::Z * 32.).to_array(),
                    mins: [0.; 3],
                    maxs: [0.; 3],
                    mask: sim::MASK_PLAYER_SOLID,
                },
            );
            (hit.startsolid == 0 && hit.fraction < 1. && hit.normal[2] >= 0.7)
                .then_some(hit.endpos[2] - plane.z)
        };
        let edges: Vec<_> = (0..=(right - left).ceil() as usize)
            .map(|i| left + 0.125 + (right - left - 0.25) * i as f32 / (right - left).ceil())
            .collect();
        // Polygon vertices capture extrema between the regular support rays.
        let mut coordinates = edges.clone();
        coordinates.extend(
            silhouette
                .polygons
                .iter()
                .flatten()
                .flat_map(|p| [p.x - 0.01, p.x, p.x + 0.01])
                .filter(|u| *u > edges[0] && *u < *edges.last().unwrap()),
        );
        coordinates.sort_by(f32::total_cmp);
        coordinates.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        let mut raw_floor = f32::INFINITY;
        let mut outer_lintel = f32::NEG_INFINITY;
        let mut profile = Vec::new();
        for u in coordinates {
            let native_floor = floor_at(u, 0.);
            if let Some(floor) = native_floor {
                raw_floor = raw_floor.min(floor);
            }
            let limits = silhouette.limits(u, 40., false).ok();
            let render_floor = limits.and_then(|v| v.0);
            let render_top = limits.and_then(|v| v.1);
            if let Some(top) = render_top {
                outer_lintel = outer_lintel.max(top);
            }
            let at = plane + t * u + Vec3::Z * 40.;
            let up = sim::trace_world_clip(
                &clip.brushes,
                &clip.bsp,
                &clip.mesh,
                &sim::TraceExtents {
                    start: at.to_array(),
                    end: (at + Vec3::Z * 160.).to_array(),
                    mins: [0.; 3],
                    maxs: [0.; 3],
                    mask: sim::MASK_PLAYER_SOLID,
                },
            );
            let top = render_top.map(|top| {
                if up.startsolid == 0 && up.fraction < 1. {
                    top.min(up.endpos[2] - plane.z)
                } else {
                    top
                }
            });
            let floor = native_floor.map(|floor| floor.max(render_floor.unwrap_or(floor)));
            profile.push((u, floor, top, limits.is_none() || up.startsolid != 0));
        }
        if !raw_floor.is_finite() || !outer_lintel.is_finite() {
            return Err("no measured floor/lintel profile".into());
        }
        let opening_middle = (outer_left + outer_right) * 0.5;
        let mut candidates = Vec::new();
        for (i, &lo) in edges.iter().enumerate() {
            for &hi in &edges[i + 1..] {
                let width = ((hi - lo) * 10.).floor() / 10.;
                if !(34.0..=140.0).contains(&width) {
                    continue;
                }
                let mut bottom = f32::NEG_INFINITY;
                let mut top = f32::INFINITY;
                let mut blocked = false;
                for &(u, floor, ceiling, solid) in &profile {
                    if u < lo - 0.001 || u > hi + 0.001 {
                        continue;
                    }
                    if solid || ceiling.is_none() {
                        blocked = true;
                        break;
                    }
                    if let Some(floor) = floor {
                        bottom = bottom.max(floor);
                    }
                    top = top.min(ceiling.unwrap());
                }
                let height = ((top - bottom - 0.25) * 10.).floor() / 10.;
                if blocked || !bottom.is_finite() || !(70.0..=180.0).contains(&height) {
                    continue;
                }
                let middle = (lo + hi) * 0.5;
                let front = if flight {
                    floor_at(middle, door.thickness * 0.5 + 1.)
                        .unwrap_or(raw_floor + door.step_front)
                } else {
                    let Some(front) = floor_at(middle, door.thickness * 0.5 + 1.) else {
                        continue;
                    };
                    front
                };
                let back = if door.two_sided == Some(true) {
                    if flight {
                        floor_at(middle, -door.thickness * 0.5 - 1.)
                            .unwrap_or(raw_floor + door.step_back)
                    } else {
                        let Some(back) = floor_at(middle, -door.thickness * 0.5 - 1.) else {
                            continue;
                        };
                        back
                    }
                } else {
                    front
                };
                if !flight && ((bottom - front).abs() > 18.01 || (bottom - back).abs() > 18.01) {
                    continue;
                }
                let frame_floor = if flight {
                    raw_floor
                } else {
                    raw_floor.min(front).min(back)
                };
                if !flight && bottom - frame_floor > 18.01 {
                    continue;
                }
                let mut candidate = door.clone();
                candidate.base = (plane + t * middle + Vec3::Z * bottom).to_array();
                candidate.opening_base =
                    (plane + t * opening_middle + Vec3::Z * frame_floor).to_array();
                candidate.opening_size = [
                    ((outer_right - outer_left) * 10.).ceil() / 10.,
                    ((outer_lintel - frame_floor) * 10.).ceil() / 10.,
                ];
                candidate.width = width;
                candidate.height = height;
                candidate.step_front = front - bottom;
                candidate.step_back = back - bottom;
                candidate.width_high = width;
                candidates.push((lo, hi, bottom, top, candidate));
            }
        }
        candidates.sort_by(|a, b| {
            (b.4.width * b.4.height)
                .total_cmp(&(a.4.width * a.4.height))
                .then_with(|| (a.0 + a.1).abs().total_cmp(&(b.0 + b.1).abs()))
        });
        let mut checked = Vec::<(f32, f32, f32, f32)>::new();
        let mut attempts = 0;
        let mut last_error = "no fitting visible rectangle".to_owned();
        let mut heights: Vec<_> = silhouette
            .polygons
            .iter()
            .flatten()
            .flat_map(|p| [p.y - 0.001, p.y + 0.001])
            .collect();
        heights.sort_by(f32::total_cmp);
        heights.dedup_by(|a, b| (*a - *b).abs() < 0.0005);
        for (lo, hi, bottom, top, mut candidate) in candidates {
            // A narrower rectangle with the same floor and ceiling only adds
            // collision to an already checked opening; it cannot open a route.
            if !flight
                && checked.iter().any(|&(a, b, f, c)| {
                    a <= lo + 0.001
                        && b >= hi - 0.001
                        && (f - bottom).abs() < 0.001
                        && c >= top - 0.001
                })
            {
                continue;
            }
            let middle = (lo + hi) * 0.5;
            let half = candidate.width * 0.5;
            let ceiling = bottom + candidate.height;
            let clear = heights
                .iter()
                .copied()
                .filter(|z| *z > bottom + 0.001 && *z < ceiling - 0.001)
                .chain([bottom + 0.001, ceiling - 0.001])
                .all(|z| {
                    silhouette.free(middle, z, true).is_ok_and(|(l, r)| {
                        l <= middle - half + 0.001 && r >= middle + half - 0.001
                    })
                });
            if !clear {
                continue;
            }
            let Ok((l, r)) = silhouette.free(middle, bottom + candidate.height * 0.8, true) else {
                continue;
            };
            candidate.width_high = ((r - l - 0.25) * 10.).floor() / 10.;
            if flight && let Err(error) = super::validate_corridor(&clip, &candidate) {
                last_error = error;
                continue;
            }
            checked.push((lo, hi, bottom, top));
            attempts += 1;
            match super::validate_door(&candidate, self.prepared.clip.as_deref().unwrap()) {
                Ok(()) => {
                    diag::announce_stdout(&format!(
                        "doors: fitted {}#{} attempts={attempts} aperture={}x{} base={:?}",
                        door.zone, door.index, candidate.width, candidate.height, candidate.base
                    ));
                    *door = candidate;
                    return Ok(());
                }
                Err(error) => last_error = error,
            }
        }
        Err(format!(
            "no valid visible aperture after {attempts} candidates: {last_error}"
        ))
    }
}

fn ray_triangle_hits(triangles: &[[Vec3; 3]], start: Vec3, end: Vec3) -> Vec<f32> {
    let d = end - start;
    let length = d.length();
    let mut hits: Vec<_> = triangles
        .iter()
        .filter_map(|v| {
            let ab = v[1] - v[0];
            let ac = v[2] - v[0];
            let p = d.cross(ac);
            let det = ab.dot(p);
            if det.abs() < 1e-6 {
                return None;
            }
            let q = start - v[0];
            let u = q.dot(p) / det;
            if !(-1e-5..=1.00001).contains(&u) {
                return None;
            }
            let cross = q.cross(ab);
            let w = d.dot(cross) / det;
            if w < -1e-5 || u + w > 1.00001 {
                return None;
            }
            let f = ac.dot(cross) / det;
            (0. ..=1.).contains(&f).then_some(f * length)
        })
        .collect();
    hits.sort_by(f32::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    hits
}

struct Silhouette {
    polygons: Vec<Vec<Vec2>>,
}
impl Silhouette {
    fn new(triangles: &[[Vec3; 3]], base: Vec3, n: Vec3, t: Vec3, thickness: f32) -> Self {
        let mut polygons = Vec::new();
        for tri in triangles {
            let mut p = tri.to_vec();
            for sign in [-1., 1.] {
                let mut out = Vec::new();
                for i in 0..p.len() {
                    let a = p[i];
                    let b = p[(i + 1) % p.len()];
                    let da = thickness * 0.5 + 0.5 - sign * (a - base).dot(n);
                    let db = thickness * 0.5 + 0.5 - sign * (b - base).dot(n);
                    if da >= 0. {
                        out.push(a);
                    }
                    if (da >= 0.) != (db >= 0.) {
                        out.push(a + (b - a) * (da / (da - db)));
                    }
                }
                p = out;
            }
            if p.len() >= 2 {
                polygons.push(
                    p.into_iter()
                        .map(|v| Vec2::new((v - base).dot(t), v.z - base.z))
                        .collect(),
                );
            }
        }
        Self { polygons }
    }
    fn free(&self, center: f32, level: f32, horizontal: bool) -> Result<(f32, f32), String> {
        let (left, right) = self.limits(center, level, horizontal)?;
        left.zip(right).ok_or_else(||format!("visible opening lacks a boundary at ({center},{level}) horizontal={horizontal}, bounds={left:?}/{right:?}"))
    }
    fn limits(
        &self,
        center: f32,
        level: f32,
        horizontal: bool,
    ) -> Result<(Option<f32>, Option<f32>), String> {
        let (center, level) = if horizontal {
            (center, level)
        } else {
            (level, center)
        };
        let mut left = -160f32;
        let mut right = 160f32;
        let mut has_left = false;
        let mut has_right = false;
        for polygon in &self.polygons {
            let mut points = Vec::new();
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let (a, b) = if horizontal {
                    (a, b)
                } else {
                    (Vec2::new(a.y, a.x), Vec2::new(b.y, b.x))
                };
                if (a.y - level).abs() < 0.0001 {
                    points.push(a.x);
                }
                if (a.y - level) * (b.y - level) < 0. {
                    points.push(a.x + (b.x - a.x) * (level - a.y) / (b.y - a.y));
                }
            }
            if points.is_empty() {
                continue;
            }
            let lo = points.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = points.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            if lo < center - 0.001 && hi > center + 0.001 {
                return Err(format!(
                    "opaque render geometry covers doorway at ({center},{level})"
                ));
            }
            if hi <= center + 0.001 && hi > left {
                left = hi;
                has_left = true;
            }
            if lo >= center - 0.001 && lo < right {
                right = lo;
                has_right = true;
            }
        }
        Ok((has_left.then_some(left), has_right.then_some(right)))
    }
}
