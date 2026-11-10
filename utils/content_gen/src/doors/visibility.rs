//! Potential door visibility from native DPVS cells, compiled once per catalog.
use asset_world::DpvsWorldData;
use dpvs_iw4::{CellClipPlanes, CellPortalGraph, DpvsPlanes, PortalView, VisBits, WalkScratch};
use serde_json::{Value, json};

pub(super) fn annotate(catalog: &mut Value, dpvs: &DpvsWorldData) -> Result<(), String> {
    let rows = catalog
        .as_array_mut()
        .ok_or("door catalog is not an array")?;
    let endpoints: Vec<_> = rows
        .iter()
        .map(|row| {
            let vector = |key: &str| -> Result<[f32; 3], String> {
                let a = row[key]
                    .as_array()
                    .filter(|a| a.len() == 3)
                    .ok_or("invalid door vector")?;
                let mut v = [0.; 3];
                for i in 0..3 {
                    v[i] = a[i]
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .ok_or("invalid door coordinate")? as f32;
                }
                Ok(v)
            };
            Ok((
                vector("base")?,
                vector("normal")?,
                row["width"].as_f64().ok_or("invalid door width")? as f32,
                row["height"].as_f64().ok_or("invalid door height")? as f32,
            ))
        })
        .collect::<Result<_, String>>()?;
    let edges: Vec<Vec<_>> = dpvs
        .portals_per_cell
        .iter()
        .map(|cell| {
            cell.iter()
                .map(|p| PortalView {
                    plane: p.plane,
                    neighbor: p.neighbor,
                    vertices: dpvs
                        .portal_verts
                        .get(p.vert_start..p.vert_start + p.vert_count)
                        .unwrap_or(&[]),
                    hull_axis: p.hull_axis,
                })
                .collect()
        })
        .collect();
    let refs: Vec<_> = edges.iter().map(Vec::as_slice).collect();
    let graph = CellPortalGraph { portals: &refs };
    let planes = DpvsPlanes {
        planes: &dpvs.planes,
        nodes: &dpvs.nodes,
        cell_count: dpvs.cell_count as u32,
    };
    let windings: Vec<_> = endpoints
        .iter()
        .map(|&(base, n, w, h)| {
            let t = [-n[1], n[0], 0.];
            [(-1., 0.), (1., 0.), (1., h), (-1., h)].map(|(side, z)| {
                std::array::from_fn(|i| {
                    base[i] + side * w * 0.5 * t[i] + if i == 2 { z } else { 0. }
                })
            })
        })
        .collect();
    let ids: Vec<_> = rows.iter().map(|r| r["id"].clone()).collect();
    for (source, &(base, n, w, h)) in endpoints.iter().enumerate() {
        let mut visible = vec![false; rows.len()];
        let mut unknown = false;
        let mut seeded_sides = [false; 2];
        let offset = rows[source]["thickness"].as_f64().unwrap_or(8.) as f32 * 0.5 + 4.;
        // Both physical sides and several positions across the opening. No
        // camera frustum: child hulls still clip each native portal path.
        for (side_index, side) in [-1., 1.].into_iter().enumerate() {
            for across in [-0.4, 0., 0.4] {
                for z in [24., (h - 8.).min(64.)] {
                    let t = [-n[1], n[0], 0.];
                    let eye = std::array::from_fn(|i| {
                        base[i]
                            + side * offset * n[i]
                            + across * w * t[i]
                            + if i == 2 { z } else { 0. }
                    });
                    let Some(cell) = planes.cell_for_point(eye) else {
                        continue;
                    };
                    seeded_sides[side_index] = true;
                    let mut clips = vec![Vec::<CellClipPlanes>::new(); dpvs.cell_count];
                    let mut words = vec![0; dpvs_iw4::words_for_bits(dpvs.cell_count.max(1))];
                    let mut scratch = Box::new(WalkScratch::new());
                    let stats = scratch.visit_cells(
                        &graph,
                        dpvs_iw4::PortalWalkView {
                            camera_cell: cell,
                            eye,
                            view_plane: [0.; 4],
                            clip_planes: &[],
                            bevels: None,
                        },
                        0,
                        &mut VisBits::new(&mut words),
                        None,
                        Some(&mut |cell, clip| clips[cell].push(*clip)),
                    );
                    if stats.walk_limit_hits > 0 {
                        unknown = true;
                    }
                    for (target, winding) in windings.iter().enumerate() {
                        if target != source
                            && planes.winding_reaches_visible_cell(winding, &mut |cell, piece| {
                                clips.get(cell).is_some_and(|clips| {
                                    clips.iter().any(|clip| {
                                        dpvs_iw4::winding_survives_planes(piece, clip.as_slice())
                                    })
                                })
                            })
                        {
                            visible[target] = true;
                        }
                    }
                }
            }
        }
        unknown |= seeded_sides.iter().any(|&seeded| !seeded);
        if unknown {
            for (target, value) in visible.iter_mut().enumerate() {
                *value = target != source;
            }
        }
        rows[source]["pair_visibility_version"] = json!(1);
        rows[source]["visibility_unknown"] = json!(unknown);
        rows[source]["visible_doors"] = json!(
            ids.iter()
                .enumerate()
                .filter(|(i, _)| visible[*i])
                .map(|(_, id)| id)
                .collect::<Vec<_>>()
        );
    }
    diag::announce_stdout(&format!(
        "doors: DPVS visibility cells={} sources={} unknown={} directed_pairs={}",
        dpvs.cell_count,
        rows.len(),
        rows.iter()
            .filter(|r| r["visibility_unknown"] == true)
            .count(),
        rows.iter()
            .map(|r| r["visible_doors"].as_array().map_or(0, Vec::len))
            .sum::<usize>()
    ));
    Ok(())
}
