use serde_json::json;

pub fn probe_sweep(
    games: &asset_transport::GamesRoot,
    artifacts: &std::path::Path,
    zone: &str,
    queries: &std::path::Path,
) -> Result<(), String> {
    let queries: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(queries).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let vector = |query: &serde_json::Value,
                  name: &str,
                  default: Option<[f32; 3]>|
     -> Result<[f32; 3], String> {
        let value = match query.get(name) {
            Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
            None => default.ok_or_else(|| format!("missing {name}"))?,
        };
        if value.iter().any(|v| !v.is_finite()) {
            return Err(format!("non-finite {name}"));
        }
        Ok(value)
    };
    let (prepared, _) = crate::doors::load_map(games, zone)?;
    let clip = prepared.clip.as_ref().ok_or("map has no collision")?;
    let (brushes, bsp, mesh, _) = session::sim_clip_map(clip);
    let bsp_mesh_only = sim::SimClipMesh {
        tables: mesh.tables.clone(),
        ..Default::default()
    };
    let mut rows = Vec::new();
    for query in queries {
        let start = vector(&query, "start", None)?;
        let end = vector(&query, "end", None)?;
        let mins = vector(&query, "mins", Some([-15., -15., 0.]))?;
        let maxs = vector(&query, "maxs", Some([15., 15., 70.]))?;
        if (0..3).any(|axis| mins[axis] > maxs[axis]) {
            return Err("invalid sweep bounds".into());
        }
        for mask in [sim::MASK_PLAYER_SOLID, 1, 0x10000] {
            for (static_models, mesh) in [(true, &mesh), (false, &bsp_mesh_only)] {
                let hit = sim::trace_world_clip(
                    &brushes,
                    &bsp,
                    mesh,
                    &sim::TraceExtents {
                        start,
                        end,
                        mins,
                        maxs,
                        mask,
                    },
                );
                rows.push(json!({"start":start,"requested_end":end,"mins":mins,"maxs":maxs,
                    "mask":mask,"static_models":static_models,"fraction":hit.fraction,"end":hit.endpos,
                    "normal":hit.normal,"contents":hit.contents,"surface_flags":hit.surface_flags,
                    "startsolid":hit.startsolid,"allsolid":hit.allsolid}));
            }
        }
    }
    let directory = artifacts.join("content/layouts");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}-sweeps.json", zone.replace(':', "_")));
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({"zone":zone,"coordinate_space":"native","samples":rows}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    diag::announce_stdout(&format!("sweep probes: {}", path.display()));
    Ok(())
}

pub fn probe_floor(
    games: &asset_transport::GamesRoot,
    artifacts: &std::path::Path,
    zone: &str,
    points: &std::path::Path,
) -> Result<(), String> {
    let points: Vec<[f32; 3]> =
        serde_json::from_slice(&std::fs::read(points).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err("probe coordinates must be finite".into());
    }
    let (prepared, _) = crate::doors::load_map(games, zone)?;
    let clip = prepared.clip.as_ref().ok_or("map has no collision")?;
    let rows: Vec<_> = points
        .into_iter()
        .map(|start| {
            let end = [start[0], start[1], start[2] - 8192.];
            let hit = clip.sweep_box(
                start,
                end,
                [-15., -15., 0.],
                [15., 15., 70.],
                sim::MASK_PLAYER_SOLID,
            );
            json!({"start":start,"end":hit.endpos,"fraction":hit.fraction,"normal":hit.normal,
            "startsolid":hit.startsolid,"allsolid":hit.allsolid,"surface_flags":hit.surface_flags})
        })
        .collect();
    let directory = artifacts.join("content/layouts");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}-floors.json", zone.replace(':', "_")));
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({"zone":zone,"coordinate_space":"native","samples":rows}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    diag::announce_stdout(&format!("floor probes: {}", path.display()));
    Ok(())
}

pub fn inspect_layout(
    games: &asset_transport::GamesRoot,
    artifacts: &std::path::Path,
    zone: &str,
) -> Result<(), String> {
    let (prepared, _) = crate::doors::load_map(games, zone)?;
    let clip = prepared.clip.as_ref().ok_or("map has no collision")?;
    let ladders: Vec<_> = clip
        .brushes
        .iter()
        .enumerate()
        .filter(|(_, brush)| brush.plane_surface_flags.iter().any(|flags| flags & 8 != 0))
        .map(|(index, brush)| {
            json!({"brush":index,"planes":brush.planes,
            "contents":brush.contents,"surface_flags":brush.plane_surface_flags})
        })
        .collect();
    let models: Vec<_> = prepared
        .world
        .static_model_instances
        .iter()
        .enumerate()
        .filter_map(|(index, placement)| {
            let placement = placement.as_ref()?;
            let mesh = prepared.world.static_model_meshes.get(placement.mesh)?;
            Some(
                json!({"instance":index,"model":mesh.name,"origin":placement.origin,
                "axes":placement.axis,"scale":placement.scale}),
            )
        })
        .collect();
    let directory = artifacts.join("content/layouts");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}.json", zone.replace(':', "_")));
    let visibility_openings: Vec<_> = prepared.world.draw.as_ref().map(|draw| {
        draw.dpvs.portals_per_cell.iter().enumerate().flat_map(|(cell, portals)| {
            portals.iter().map(move |portal| json!({
                "cell": cell,
                "plane": portal.plane,
                "vertices": draw.dpvs.portal_verts.get(portal.vert_start..portal.vert_start + portal.vert_count).unwrap_or(&[]),
            }))
        }).collect()
    }).unwrap_or_else(Vec::new);
    let data = json!({"schema_version":1,"zone":zone,"coordinate_space":"native",
        "ladders":ladders,"models":models,"visibility_openings":visibility_openings});
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    diag::announce_stdout(&format!("layout: {}", path.display()));
    Ok(())
}
