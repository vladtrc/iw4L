//! `iw4l doors <zone>...`: the door catalog for cross-world portals.
//!
//! Every vertical DPVS portal of each map is a candidate opening. Traces on
//! the map's own clip decide whether it is a doorway in a wall: jambs left and
//! right, a lintel, a floor, a wall thickness. Doorways of maps from different
//! families are paired by size, and each pair gets the rigid transform (yaw and
//! translation only) that carries one opening onto the other.

use std::path::{Path, PathBuf};

use asset_transport::{GamesRoot, NamespaceTrees};
use asset_transport::{find_runtime_common_mp, find_zone_file};
use serde_json::{Value, json};
use sim::bullet_collision::MASK_SHOT;

mod accessibility;
mod geometry;
mod visibility;

const PROBE: f32 = 160.0;
const UP: [f32; 3] = [0.0, 0.0, 1.0];

/// One map's world clip in the simulation's types, native coordinates.
struct MapClip {
    brushes: Vec<sim::SimBrush>,
    bsp: sim::SimClipBsp,
    mesh: sim::SimClipMesh,
}

impl MapClip {
    /// The distance a point ray travels from `start` toward `end` before it
    /// hits solid, or `None` when it gets through.
    fn hit(&self, start: [f32; 3], end: [f32; 3]) -> Option<f32> {
        let trace = sim::trace_world_clip(
            &self.brushes,
            &self.bsp,
            &self.mesh,
            &sim::TraceExtents {
                start,
                end,
                mins: [0.0; 3],
                maxs: [0.0; 3],
                mask: MASK_SHOT,
            },
        );
        (trace.fraction < 1.0 || trace.startsolid != 0).then(|| trace.fraction * dist(start, end))
    }

    fn solid_at(&self, point: [f32; 3]) -> bool {
        let trace = sim::trace_world_clip(
            &self.brushes,
            &self.bsp,
            &self.mesh,
            &sim::TraceExtents {
                start: point,
                end: add(point, [0.0, 0.0, 0.01]),
                mins: [0.0; 3],
                maxs: [0.0; 3],
                mask: MASK_SHOT,
            },
        );
        trace.startsolid != 0
    }
}

#[derive(Clone, Debug)]
struct Door {
    clearance: asset_world::PortalClearance,
    two_sided: Option<bool>,
    zone: String,
    family: String,
    index: usize,
    /// Bottom centre of the opening, on the doorway floor.
    base: [f32; 3],
    opening_base: [f32; 3],
    opening_size: [f32; 2],
    /// Horizontal unit normal of the opening.
    normal: [f32; 3],
    width: f32,
    width_high: f32,
    height: f32,
    thickness: f32,
    /// Floor heights in front of and behind the opening, relative to `base`.
    step_front: f32,
    step_back: f32,
    portal: [f32; 2],
    closed: bool,
    source: String,
    replacement: Option<Value>,
}

impl Door {
    fn kind(&self) -> &'static str {
        if self.closed || (self.step_front.abs() <= 18.01 && self.step_back.abs() <= 18.01) {
            "door"
        } else {
            "window"
        }
    }

    fn json(&self) -> Value {
        json!({
            "id": format!("{}#{}", self.zone, self.index),
            "kind": self.kind(),
            "clearance_version": if self.clearance == asset_world::PortalClearance::Walk { 2 } else { 0 },
            "clearance": if self.clearance == asset_world::PortalClearance::Walk { "walk" } else { "flight" },
            "closed": self.closed,
            "source": self.source,
            "replace": self.replacement,
            "geometry_version":4,
            "two_sided":self.two_sided.expect("native door sidedness must be classified"),
            "opening_base":round3(self.opening_base),
            "opening_size":self.opening_size.map(round1),
            "base": round3(self.base),
            "normal": self.normal.map(|v|(v*1e6).round()/1e6),
            "yaw": round1(self.normal[1].atan2(self.normal[0]).to_degrees()),
            "width": round1(self.width),
            "width_high": round1(self.width_high),
            "height": round1(self.height),
            "thickness": round1(self.thickness),
            "step_front": round1(self.step_front),
            "step_back": round1(self.step_back),
            "portal": [round1(self.portal[0]), round1(self.portal[1])],
        })
    }
}

pub fn run_doors(games: GamesRoot, artifacts: PathBuf, zones: Vec<String>) -> Result<(), String> {
    run_catalog(games, artifacts, zones, false)
}

pub fn run_projectile_openings(
    games: GamesRoot,
    artifacts: PathBuf,
    zones: Vec<String>,
) -> Result<(), String> {
    run_catalog(games, artifacts, zones, true)
}

fn run_catalog(
    games: GamesRoot,
    artifacts: PathBuf,
    zones: Vec<String>,
    flight: bool,
) -> Result<(), String> {
    let out_dir = artifacts.join("content").join(if flight {
        "projectile-openings"
    } else {
        "doors"
    });
    if let Err(error) = std::fs::create_dir_all(&out_dir) {
        return Err(format!("doors: create {}: {error}", out_dir.display()));
    }
    let mut all = Vec::new();
    for zone in &zones {
        match map_doors(&games, zone, flight) {
            Ok(doors) => {
                let path = out_dir.join(format!("{}.json", zone.replace(':', "_")));
                let rows: Vec<Value> = doors.iter().map(Door::json).collect();
                write_json(&path, &json!({ "zone": zone, "doors": rows }))?;
                diag::announce_stdout(&format!(
                    "doors: {zone}: {} openings ({} doors) -> {}",
                    doors.len(),
                    doors.iter().filter(|d| d.kind() == "door").count(),
                    path.display()
                ));
                all.extend(doors);
            }
            Err(error) => return Err(format!("doors: {zone}: {error}")),
        }
    }
    let pairs = pair_doors(&all);
    let path = out_dir.join("pairs.json");
    diag::announce_stdout(&format!(
        "doors: {} cross-family pairs -> {}",
        pairs.len(),
        path.display()
    ));
    write_json(&path, &json!({ "zones": zones, "pairs": pairs }))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// One map loaded headless, as a match would load it, and its family.
pub fn load_map(
    games: &GamesRoot,
    zone: &str,
) -> Result<(assets::PreparedMatch, asset_core::FamilyId), String> {
    let found = find_zone_file(games, zone)?;
    let family = asset_transport::zone_game_for_path(&found.path)
        .ok_or("source game could not be identified")?;
    let common_mp = find_runtime_common_mp(games, &found.path).map(|zone| zone.path);
    let prepared = match bevy::tasks::futures_lite::future::block_on(assets::load_prepared_match(
        Ok(found.path),
        common_mp,
        asset_transport::LoadProgress::default(),
        frame::AssetFamilies::from_inventory(
            &net::content_inventory(
                NamespaceTrees::discover(games)
                    .present()
                    .map(|(id, _)| id.as_str()),
            ),
            &frame::UiMenuDvars::default(),
        )?,
    )) {
        assets::MatchLoadOutcome::Ready(prepared) => *prepared,
        assets::MatchLoadOutcome::Canceled => return Err("map walk canceled".into()),
        assets::MatchLoadOutcome::Refused(reason) => {
            return Err(format!("map walk refused: {reason}"));
        }
    };
    Ok((prepared, family))
}

fn map_doors(games: &GamesRoot, zone: &str, flight: bool) -> Result<Vec<Door>, String> {
    let (prepared, family) = load_map(games, zone)?;
    collect_map_doors(&prepared, family, zone, flight)
}

fn collect_map_doors(
    prepared: &assets::PreparedMatch,
    family: asset_core::FamilyId,
    zone: &str,
    flight: bool,
) -> Result<Vec<Door>, String> {
    let clip = prepared.clip.as_deref().ok_or("the map has no clip")?;
    let (brushes, bsp, mesh, _) = session::sim_clip_map(clip);
    let clip = MapClip { brushes, bsp, mesh };
    let dpvs = &prepared
        .world
        .draw
        .as_ref()
        .ok_or("the map has no world draw")?
        .dpvs;

    let mut seen = std::collections::HashSet::new();
    let mut doors = Vec::new();
    let mut candidates = 0usize;
    for cell_portals in &dpvs.portals_per_cell {
        for portal in cell_portals {
            let verts = dpvs
                .portal_verts
                .get(portal.vert_start..portal.vert_start + portal.vert_count)
                .unwrap_or(&[]);
            if verts.len() < 3 {
                continue;
            }
            let normal = [portal.plane[0], portal.plane[1], 0.0];
            if portal.plane[2].abs() > 0.1 || len(normal) < 0.5 {
                continue;
            }
            let centre = scale(
                verts.iter().fold([0.0; 3], |acc, v| add(acc, *v)),
                1.0 / verts.len() as f32,
            );
            let key = (
                (centre[0] / 4.0).round() as i32,
                (centre[1] / 4.0).round() as i32,
                (centre[2] / 4.0).round() as i32,
            );
            if !seen.insert(key) {
                continue;
            }
            let normal = norm(normal);
            let tangent = [-normal[1], normal[0], 0.0];
            let (mut umin, mut umax, mut zmin, mut zmax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for v in verts {
                let u = dot(sub(*v, centre), tangent);
                umin = umin.min(u);
                umax = umax.max(u);
                zmin = zmin.min(v[2]);
                zmax = zmax.max(v[2]);
            }
            let portal_size = [umax - umin, zmax - zmin];
            if !(24.0..=260.0).contains(&portal_size[0])
                || !(56.0..=260.0).contains(&portal_size[1])
            {
                continue;
            }
            candidates += 1;
            if let Some(door) = probe_opening(&clip, centre, zmin, normal, portal_size) {
                doors.push(Door {
                    zone: zone.to_owned(),
                    family: family.as_str().to_owned(),
                    index: doors.len(),
                    ..door
                });
            }
        }
    }
    for (instance, placement) in prepared
        .world
        .static_model_instances
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.as_ref().map(|p| (i, p)))
    {
        let Some(mesh) = prepared.world.static_model_meshes.get(placement.mesh) else {
            continue;
        };
        if !mesh.name.contains("_door") && !mesh.name.starts_with("door") {
            continue;
        }
        if let Some(mut door) = model_door(&clip, mesh, placement) {
            if doors
                .iter()
                .any(|d| dist(d.base, door.base) < 24.0 && dot(d.normal, door.normal).abs() > 0.98)
            {
                continue;
            }
            door.zone = zone.to_owned();
            door.family = family.as_str().to_owned();
            door.index = doors.len();
            door.replacement.as_mut().unwrap()["render_instance"] = json!(instance);
            diag::announce_stdout(&format!(
                "doors: model candidate {zone} {} at {:?} normal={:?}",
                mesh.name, door.base, door.normal
            ));
            doors.push(door);
        } else {
            diag::announce_stdout(&format!(
                "doors: model excluded {zone} {} at {:?} up={:.6}",
                mesh.name, placement.origin, placement.axis[2][2]
            ));
        }
    }
    let mut visible = geometry::VisibleScene::new(prepared)?;
    doors.retain_mut(|door| {
        door.clearance = if flight { asset_world::PortalClearance::Flight } else { asset_world::PortalClearance::Walk };
        if let Err(reason)=visible.refine(door) {
            diag::announce_stdout(&format!("doors: visible geometry rejected {}#{}: {reason}",zone,door.index));
            return false;
        }
        diag::announce_stdout(&format!("doors: measured {}#{} base={:?} visible={}x{} opening={:?}/{:?} steps=({}, {}) wall={}",zone,door.index,door.base,door.width,door.height,door.opening_base,door.opening_size,door.step_front,door.step_back,door.thickness));
        true
    });
    diag::info!(
        Launch,
        "doors: {zone}: {} vertical portals of opening size, {} pass the frame probe",
        candidates,
        doors.len()
    );
    Ok(doors)
}

/// Usable openings plus closed authored door panels. Decorative panels cannot
/// become portals merely because their asset name contains "door".
pub fn random_door_catalog(
    games: &GamesRoot,
    zone: &str,
) -> Result<(Value, std::sync::Arc<asset_world::ClipCollision>), String> {
    let (prepared, family) = load_map(games, zone)?;
    let doors = collect_map_doors(&prepared, family, zone, false)?;
    let mut catalog = Value::Array(
        doors
            .into_iter()
            .filter(|door| {
                door.kind() == "door"
                    && door.width >= 34.0
                    && door.width_high >= 34.0
                    && door.height >= 70.0
            })
            .map(|door| door.json())
            .collect(),
    );
    visibility::annotate(
        &mut catalog,
        &prepared.world.draw.as_ref().ok_or("map has no DPVS")?.dpvs,
    )?;
    Ok((
        catalog,
        prepared.clip.clone().ok_or("map has no collision")?,
    ))
}

/// Validate against the same aperture cut used by the match, leaving surrounding
/// collision intact. Each candidate is cut in isolation so another door cannot
/// accidentally make this one pass.
fn validate_door(door: &Door, original: &asset_world::ClipCollision) -> Result<(), String> {
    let mut endpoint = door.json();
    endpoint["world"] = json!(0);
    let replacements: Vec<_> = assets::parse_portal_replacement(&endpoint, 1)?
        .into_iter()
        .collect();
    let mut clip = original.clone();
    // The synthetic pair repeats the endpoint; apply its replacement once.
    asset_world::PortalDoorReplacement::apply_all_collision(
        &replacements.into_iter().take(1).collect::<Vec<_>>(),
        &mut clip,
    )?;
    asset_world::PortalDoorTrim {
        clearance: door.clearance,
        two_sided: door.two_sided.ok_or("missing native door sidedness")?,
        world: 0,
        door_id: format!("{}#{}", door.zone, door.index),
        opening_base: door.opening_base,
        opening_size: door.opening_size,
        thickness: door.thickness,
        aperture: asset_world::AtlasDoorway {
            world: 0,
            base: door.base,
            normal: door.normal,
            width: door.width.min(door.width_high),
            height: door.height,
        },
    }
    .apply_collision(&mut clip)?;
    let (brushes, bsp, mesh, _) = session::sim_clip_map(&clip);
    validate_corridor(&MapClip { brushes, bsp, mesh }, door)
}

fn validate_corridor(clip: &MapClip, door: &Door) -> Result<(), String> {
    let aperture = sim::world_graph::AtlasDoor {
        two_sided: door.two_sided.expect("classified door corridor"),
        world: sim::world_graph::WorldId::ROOT,
        base: door.base,
        normal: door.normal,
        width: door.width.min(door.width_high),
        height: door.height,
    };
    let flight = door.clearance == asset_world::PortalClearance::Flight;
    let validate = if flight {
        sim::door_exit::validate_flight
    } else {
        sim::door_exit::validate
    };
    validate(&aperture, door.thickness, |start, end| {
        sim::trace_world_clip(
            &clip.brushes,
            &clip.bsp,
            &clip.mesh,
            &sim::TraceExtents {
                start,
                end,
                mins: if flight {
                    [-2.0; 3]
                } else {
                    [-15.0, -15.0, 0.0]
                },
                maxs: if flight { [2.0; 3] } else { [15.0, 15.0, 70.0] },
                mask: if flight {
                    MASK_SHOT
                } else {
                    sim::MASK_PLAYER_SOLID
                },
            },
        )
    })
}

fn model_door(
    clip: &MapClip,
    mesh: &asset_world::ModelMesh,
    placement: &asset_world::StaticModelPlacement,
) -> Option<Door> {
    if !upright_door(placement.axis) {
        return None;
    }
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for surface in mesh.surfaces() {
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
            surface.mesh.attribute(bevy::mesh::Mesh::ATTRIBUTE_POSITION)
        {
            for v in vertices {
                for i in 0..3 {
                    low[i] = low[i].min(v[i]);
                    high[i] = high[i].max(v[i]);
                }
            }
        }
    }
    let thickness_axis = usize::from(high[0] - low[0] > high[1] - low[1]);
    let width_axis = 1 - thickness_axis;
    let width = (high[width_axis] - low[width_axis]) * placement.scale;
    let height = (high[2] - low[2]) * placement.scale;
    if !(34.0..=140.0).contains(&width) || !(72.0..=180.0).contains(&height) {
        return None;
    }
    let world = |p: [f32; 3]| {
        add(
            placement.origin,
            (0..3).fold([0.0; 3], |v, i| {
                add(v, scale(placement.axis[i], p[i] * placement.scale))
            }),
        )
    };
    for sign in [1.0, -1.0] {
        let axis = placement.axis[thickness_axis];
        let normal = norm([axis[0] * sign, axis[1] * sign, 0.0]);
        let mut local = std::array::from_fn(|i| (low[i] + high[i]) * 0.5);
        local[2] = low[2];
        local[thickness_axis] = if sign > 0.0 {
            high[thickness_axis]
        } else {
            low[thickness_axis]
        };
        let mut panel = world(local);
        // Mesh bounds include handles. Use the actual collision face so the
        // player-sized entrance also fits narrow corridors in front of a door.
        let from = add(add(panel, [0.0, 0.0, 45.0]), scale(normal, 8.0));
        let to = add(add(panel, [0.0, 0.0, 45.0]), scale(normal, -16.0));
        let face = sim::trace_world_clip(
            &clip.brushes,
            &clip.bsp,
            &clip.mesh,
            &sim::TraceExtents {
                start: from,
                end: to,
                mins: [0.0; 3],
                maxs: [0.0; 3],
                mask: sim::MASK_PLAYER_SOLID,
            },
        );
        if face.startsolid == 0 && face.fraction < 1.0 && dot(face.normal, normal) > 0.7 {
            panel[0] = face.endpos[0];
            panel[1] = face.endpos[1];
        }
        let front = add(panel, scale(normal, 16.0));
        let down = sim::trace_world_clip(
            &clip.brushes,
            &clip.bsp,
            &clip.mesh,
            &sim::TraceExtents {
                start: add(front, [0.0, 0.0, 64.0]),
                end: add(front, [0.0, 0.0, -48.0]),
                mins: [0.0; 3],
                maxs: [0.0; 3],
                mask: sim::MASK_PLAYER_SOLID,
            },
        );
        if std::env::var_os("IW4L_DOOR_DIAG").is_some() {
            diag::announce_stdout(&format!(
                "door probe {:?} sign={sign}: floor {down:?}",
                placement.origin
            ));
        }
        if down.fraction >= 1.0 || down.startsolid != 0 || down.normal[2] < 0.7 {
            continue;
        }
        let floor = down.endpos[2];
        if !(-18.0..=24.0).contains(&(floor - panel[2])) {
            continue;
        }
        let base = [front[0], front[1], floor - 1.0];
        let tangent = [-normal[1], normal[0], 0.0];
        let fits_width = |usable_width: f32| {
            [0.0, (usable_width*0.5-16.0).max(0.0), -(usable_width*0.5-16.0).max(0.0)].into_iter().all(|side| {
            let p=add(base,scale(tangent,side));
            (0..=9).all(|sample| {
                let at=add(p,scale(normal,1.0-sample as f32*1.25/9.0));
                [2.0,4.0,8.0,16.0].into_iter().any(|lift| {
                    let support=sim::trace_world_clip(&clip.brushes, &clip.bsp, &clip.mesh, &sim::TraceExtents { start: add(at,[0.0,0.0,lift]), end: add(at,[0.0,0.0,-18.0]), mins: [-15.0,-15.0,0.0], maxs: [15.0,15.0,70.0], mask: sim::MASK_PLAYER_SOLID });
                    if std::env::var_os("IW4L_DOOR_DIAG").is_some() { diag::announce_stdout(&format!("door probe {:?} sign={sign} side={side} sample={sample} lift={lift} support={support:?}",placement.origin)); }
                    if support.fraction>=1.0 || support.startsolid!=0 || support.normal[2]<0.7 {return false;}
                    let feet=add(support.endpos,[0.0,0.0,0.25]);
                    let hit=sim::trace_world_clip(&clip.brushes, &clip.bsp, &clip.mesh, &sim::TraceExtents { start: feet, end: add(feet,[0.0,0.0,0.01]), mins: [-15.0,-15.0,0.0], maxs: [15.0,15.0,70.0], mask: sim::MASK_PLAYER_SOLID });
                    if std::env::var_os("IW4L_DOOR_DIAG").is_some() { diag::announce_stdout(&format!("door probe {:?} sign={sign} side={side} sample={sample} feet={feet:?} fit={hit:?}",placement.origin)); }
                    hit.startsolid==0 && hit.fraction>=0.9999
                })
            })
        })
        };
        // Surrounding clutter can narrow an otherwise usable upright door.
        // Keep the largest supported opening rather than selecting its back.
        let usable_width = (0..=((width - 34.0) / 2.0).floor() as usize)
            .map(|step| width - 2.0 - step as f32 * 2.0)
            .find(|&candidate| candidate >= 34.0 && fits_width(candidate));
        let visible_height = (panel[2] + height - base[2]).min(height);
        if let Some(clear_width) = usable_width
            && visible_height >= 72.0
        {
            return Some(Door {
                clearance: asset_world::PortalClearance::Walk,
                two_sided: None,
                zone: String::new(),
                family: String::new(),
                index: 0,
                opening_base: [0.; 3],
                opening_size: [width, visible_height - 1.],
                base: [
                    world(std::array::from_fn(|i| (low[i] + high[i]) * 0.5))[0],
                    world(std::array::from_fn(|i| (low[i] + high[i]) * 0.5))[1],
                    base[2],
                ],
                normal,
                width,
                width_high: width,
                height: visible_height - 1.0,
                thickness: (high[thickness_axis] - low[thickness_axis]) * placement.scale,
                step_front: 0.0,
                step_back: 0.0,
                portal: [width, height],
                closed: true,
                source: mesh.name.clone(),
                replacement: Some(json!({
                    "render_instance":0,"model_name":mesh.name,"model_origin":placement.origin,
                    "leaf_base":panel,"leaf_normal":normal,"leaf_size":[width,height],
                    "clear_width":clear_width,"local_min":low,"local_max":high,
                    "clip_models":clip.mesh.static_models.iter().filter(|model|
                        model.name==mesh.name && dist(model.model.origin,placement.origin)<0.1)
                        .map(|model|model.index).collect::<Vec<_>>(),
                    "clip_brushes":clip.brushes.iter().enumerate().filter(|(_,brush)|
                        brush_intersects_panel(brush,placement,low,high)).map(|(index,_)|index).collect::<Vec<_>>()
                })),
            });
        }
    }
    None
}

/// Find thin collision volumes intersecting this leaf. They are cut by the
/// exact aperture at load time, preserving their remaining jambs and caps.
fn brush_intersects_panel(
    brush: &sim::SimBrush,
    placement: &asset_world::StaticModelPlacement,
    low: [f32; 3],
    high: [f32; 3],
) -> bool {
    use bevy::math::{Mat3, Vec3};
    let axis = Mat3::from_cols_array_2d(&placement.axis);
    let inverse = axis.inverse() / placement.scale;
    let middle = Vec3::from_array(placement.origin)
        + axis
            * Vec3::from_array(std::array::from_fn(|i| (low[i] + high[i]) * 0.5))
            * placement.scale;
    if brush
        .planes
        .iter()
        .any(|p| Vec3::new(p[0], p[1], p[2]).dot(middle) - p[3] > 8.0)
    {
        return false;
    }
    let mut vertices = 0;
    let mut bounds_low = [f32::INFINITY; 3];
    let mut bounds_high = [f32::NEG_INFINITY; 3];
    for a in 0..brush.planes.len() {
        for b in a + 1..brush.planes.len() {
            for c in b + 1..brush.planes.len() {
                let rows = [brush.planes[a], brush.planes[b], brush.planes[c]];
                let matrix = Mat3::from_cols(
                    Vec3::from_slice(&rows[0][..3]),
                    Vec3::from_slice(&rows[1][..3]),
                    Vec3::from_slice(&rows[2][..3]),
                )
                .transpose();
                if matrix.determinant().abs() < 1e-5 {
                    continue;
                }
                let vertex = matrix.inverse() * Vec3::new(rows[0][3], rows[1][3], rows[2][3]);
                if brush
                    .planes
                    .iter()
                    .any(|p| Vec3::new(p[0], p[1], p[2]).dot(vertex) - p[3] > 0.01)
                {
                    continue;
                }
                vertices += 1;
                let local = (inverse * (vertex - Vec3::from_array(placement.origin))).to_array();
                for i in 0..3 {
                    bounds_low[i] = bounds_low[i].min(local[i]);
                    bounds_high[i] = bounds_high[i].max(local[i]);
                }
            }
        }
    }
    let thin = usize::from(high[0] - low[0] > high[1] - low[1]);
    vertices >= 4
        && bounds_high[thin] - bounds_low[thin] <= 24.0
        && (0..3).all(|i| high[i].min(bounds_high[i]) - low[i].max(bounds_low[i]) > 0.1)
}

fn upright_door(axis: [[f32; 3]; 3]) -> bool {
    axis[2][2] >= 3.0_f32.to_radians().cos()
}

/// Whether the portal sits in a doorway, measured on the clip: a clear line
/// through it, solid left, right and above, and a floor below.
fn probe_opening(
    clip: &MapClip,
    centre: [f32; 3],
    portal_bottom: f32,
    normal: [f32; 3],
    portal: [f32; 2],
) -> Option<Door> {
    let tangent = [-normal[1], normal[0], 0.0];
    // The floor under the portal: from mid height down.
    let mid = [centre[0], centre[1], portal_bottom + portal[1] * 0.5];
    if clip.solid_at(mid) {
        return None;
    }
    let floor = mid[2] - clip.hit(mid, add(mid, [0.0, 0.0, -PROBE]))?;
    let mut p = [mid[0], mid[1], floor + 40.0];
    if clip.solid_at(p) {
        return None;
    }
    let right = clip.hit(p, add(p, scale(tangent, PROBE)))?;
    let left = clip.hit(p, add(p, scale(tangent, -PROBE)))?;
    p = add(p, scale(tangent, (right - left) * 0.5));
    let width = right + left;
    let high = add(p, [0.0, 0.0, 40.0]);
    let width_high = clip.hit(high, add(high, scale(tangent, PROBE)))?
        + clip.hit(high, add(high, scale(tangent, -PROBE)))?;
    let lintel = clip.hit(p, add(p, scale(UP, PROBE)))?;
    let height = p[2] + lintel - floor;
    if clip
        .hit(add(p, scale(normal, -48.0)), add(p, scale(normal, 48.0)))
        .is_some()
    {
        return None;
    }
    // Floors a step in front of and behind the opening.
    let step = |side: f32| -> f32 {
        let q = add(p, scale(normal, side * 40.0));
        match clip.hit(q, add(q, [0.0, 0.0, -PROBE])) {
            Some(down) => q[2] - down - floor,
            None => -PROBE,
        }
    };
    let step_front = step(1.0);
    let step_back = step(-1.0);
    // Wall thickness at the jamb: the front and back faces of the wall.
    let jamb = add(p, scale(tangent, right + 4.0 - (right - left) * 0.5));
    let front = clip.hit(
        add(jamb, scale(normal, 64.0)),
        add(jamb, scale(normal, -64.0)),
    );
    let back = clip.hit(
        add(jamb, scale(normal, -64.0)),
        add(jamb, scale(normal, 64.0)),
    );
    let thickness = match (front, back) {
        (Some(front), Some(back)) => (64.0 - front) + (64.0 - back),
        _ => return None,
    };
    if !(28.0..=140.0).contains(&width) || !(64.0..=180.0).contains(&height) || thickness <= 0.0 {
        return None;
    }
    Some(Door {
        clearance: asset_world::PortalClearance::Walk,
        two_sided: None,
        zone: String::new(),
        family: String::new(),
        index: 0,
        base: [p[0], p[1], floor],
        opening_base: [p[0], p[1], floor],
        opening_size: [width, height],
        normal,
        width,
        width_high,
        height,
        thickness,
        step_front,
        step_back,
        portal,
        closed: false,
        source: "opening".into(),
        replacement: None,
    })
}

/// Doorways of different families whose openings are about the same, with the
/// transform from the first onto the second. Both orientations are tried:
/// the second door's normal along or against the first's.
fn pair_doors(doors: &[Door]) -> Vec<Value> {
    let mut pairs = Vec::new();
    for (i, a) in doors.iter().enumerate() {
        for b in &doors[i + 1..] {
            if a.family == b.family || a.kind() != b.kind() {
                continue;
            }
            let dw = (a.width - b.width).abs();
            let dwh = (a.width_high - b.width_high).abs();
            let dh = (a.height - b.height).abs();
            let dt = (a.thickness - b.thickness).abs();
            if dw > 10.0 || dh > 14.0 || dt > 12.0 {
                continue;
            }
            let score = dw + dwh * 0.5 + dh + dt * 0.5;
            for flip in [false, true] {
                let target = if flip {
                    scale(b.normal, -1.0)
                } else {
                    b.normal
                };
                let yaw = target[1].atan2(target[0]) - a.normal[1].atan2(a.normal[0]);
                let (s, c) = yaw.sin_cos();
                let rotated = [
                    c * a.base[0] - s * a.base[1],
                    s * a.base[0] + c * a.base[1],
                    a.base[2],
                ];
                let translate = sub(b.base, rotated);
                pairs.push((
                    score,
                    json!({
                        "a": format!("{}#{}", a.zone, a.index),
                        "b": format!("{}#{}", b.zone, b.index),
                        "kind": a.kind(),
                        "flip": flip,
                        "score": round1(score),
                        "d_width": round1(dw),
                        "d_width_high": round1(dwh),
                        "d_height": round1(dh),
                        "d_thickness": round1(dt),
                        "yaw": round1(yaw.to_degrees()),
                        "translate": round3(translate),
                    }),
                ));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    pairs.into_iter().map(|(_, value)| value).collect()
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    len(sub(a, b))
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    scale(a, 1.0 / len(a))
}
fn round1(v: f32) -> f64 {
    (f64::from(v) * 10.0).round() / 10.0
}
fn round3(v: [f32; 3]) -> [f64; 3] {
    v.map(|x| (f64::from(x) * 1000.0).round() / 1000.0)
}
