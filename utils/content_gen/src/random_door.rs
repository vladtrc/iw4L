//! Create a reusable random-door template. Pairings are resolved per match load.
use asset_transport::{GamesRoot, find_zone_file};
use serde_json::{Value, json};
use std::path::Path;

pub fn create_template(games: &GamesRoot, artifacts: &Path, maps: &[String]) -> Result<(), String> {
    if maps.is_empty() {
        return Err("usage: make map random-door <map>...".into());
    }
    let mut worlds = Vec::new();
    let mut catalogs = Vec::new();
    let mut clips = Vec::new();
    for map in maps {
        let found = find_zone_file(games, map)?;
        let family = asset_transport::zone_game_for_path(&found.path)
            .ok_or("random-door: unknown map family")?;
        let zone = format!("{}:{}", family.as_str(), found.zone_name);
        let (catalog, clip) = crate::doors::random_door_catalog(games, &zone)?;
        clips.push(clip);
        diag::announce_stdout(&format!(
            "random-door: {zone}: {} usable entrances",
            catalog.as_array().map_or(0, Vec::len)
        ));
        worlds.push(zone);
        catalogs.push(catalog);
    }
    annotate_trim_clearance(&mut catalogs, &clips)?;
    let template = json!({"schema_version":1,"generator":{"algorithm":"door-composition","version":1},"template":"random-door","trim_clearance_version":4,"worlds":worlds,"door_catalogs":catalogs});
    // Validate before replacing an existing working template.
    assets::shuffle_random_doors(&mut template.clone(), 1)?;
    let directory = artifacts.join("maps");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("random-door.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&template).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    diag::announce_stdout(&format!(
        "random-door: template -> {} (pairings shuffle at each load)",
        path.display()
    ));
    Ok(())
}

/// Enumerate the actual shared sizes that this catalog can request. Validate
/// each reduced opening after its real leaf cut and the physical trim boxes.
fn annotate_trim_clearance(
    catalogs: &mut [Value],
    clips: &[std::sync::Arc<asset_world::ClipCollision>],
) -> Result<(), String> {
    let all = catalogs
        .iter()
        .flat_map(|c| c.as_array().into_iter().flatten())
        .cloned()
        .collect::<Vec<_>>();
    for (world, catalog) in catalogs.iter_mut().enumerate() {
        for row in catalog.as_array_mut().ok_or("invalid door catalog")? {
            let ow = row["width"].as_f64().ok_or("invalid door width")?.min(
                row["width_high"]
                    .as_f64()
                    .ok_or("invalid upper door width")?,
            );
            let oh = row["height"].as_f64().ok_or("invalid door height")?;
            let mut sizes = Vec::<(f64, f64)>::new();
            for other in &all {
                let bw = other["width"]
                    .as_f64()
                    .unwrap()
                    .min(other["width_high"].as_f64().unwrap());
                let bh = other["height"].as_f64().unwrap();
                if (ow - bw).abs() > assets::MAX_DOOR_WIDTH_TRIM
                    || (oh - bh).abs() > assets::MAX_DOOR_HEIGHT_TRIM
                {
                    continue;
                }
                let value = (ow.min(bw), oh.min(bh));
                if !sizes.contains(&value) {
                    sizes.push(value);
                }
            }
            let mut accepted = Vec::new();
            for (w, h) in sizes {
                let mut endpoint = row.clone();
                endpoint["world"] = json!(0);
                endpoint["width"] = json!(w);
                endpoint["height"] = json!(h);
                let vector = |key: &str| -> Result<[f32; 3], String> {
                    let values = row[key]
                        .as_array()
                        .ok_or_else(|| format!("door has no {key}"))?;
                    Ok([
                        values[0].as_f64().unwrap() as f32,
                        values[1].as_f64().unwrap() as f32,
                        values[2].as_f64().unwrap() as f32,
                    ])
                };
                let base = vector("base")?;
                let normal = vector("normal")?;
                let thickness = row["thickness"].as_f64().ok_or("door has no thickness")? as f32;
                let replacements: Vec<_> = assets::parse_portal_replacement(&endpoint, 1)?
                    .into_iter()
                    .collect();
                let mut clip = (*clips[world]).clone();
                asset_world::PortalDoorReplacement::apply_all_collision(
                    &replacements.into_iter().take(1).collect::<Vec<_>>(),
                    &mut clip,
                )?;
                let trim = asset_world::PortalDoorTrim {
                    clearance: asset_world::PortalClearance::Walk,
                    two_sided: row["two_sided"]
                        .as_bool()
                        .ok_or("missing native door sidedness")?,
                    world: 0,
                    door_id: row["id"].as_str().unwrap().into(),
                    opening_base: vector("opening_base")?,
                    opening_size: [
                        row["opening_size"][0]
                            .as_f64()
                            .ok_or("missing opening width")? as f32,
                        row["opening_size"][1]
                            .as_f64()
                            .ok_or("missing opening height")? as f32,
                    ],
                    thickness,
                    aperture: asset_world::AtlasDoorway {
                        world: 0,
                        base,
                        normal,
                        width: w as f32,
                        height: h as f32,
                    },
                };
                trim.apply_collision(&mut clip)?;
                let (brushes, bsp, mesh, _) = session::sim_clip_map(&clip);
                let door = sim::world_graph::AtlasDoor {
                    two_sided: row["two_sided"]
                        .as_bool()
                        .ok_or("missing native door sidedness")?,
                    world: sim::world_graph::WorldId::ROOT,
                    base,
                    normal,
                    width: w as f32,
                    height: h as f32,
                };
                match sim::door_exit::validate(&door, thickness, |a, b| {
                    sim::trace_world_clip(
                        &brushes,
                        &bsp,
                        &mesh,
                        &sim::TraceExtents {
                            start: a,
                            end: b,
                            mins: [-15., -15., 0.],
                            maxs: [15., 15., 70.],
                            mask: sim::MASK_PLAYER_SOLID,
                        },
                    )
                }) {
                    Ok(()) => accepted.push(json!([w, h])),
                    Err(error) => diag::announce_stdout(&format!(
                        "random-door: trim rejected {} aperture={w}x{h}: {error}",
                        row["id"]
                    )),
                }
            }
            if accepted.is_empty() {
                return Err(format!("{} has no validated shared aperture", row["id"]));
            }
            row["clearance_apertures"] = json!(accepted);
        }
    }
    Ok(())
}

pub fn connect_maps(
    games: &GamesRoot,
    artifacts: &Path,
    maps: &[String],
    seed: Option<u64>,
) -> Result<(), String> {
    create_template(games, artifacts, maps)?;
    if let Some(seed) = seed {
        let path = artifacts.join("maps/random-door.json");
        let mut recipe: Value =
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        assets::shuffle_random_doors(&mut recipe, seed)?;
        recipe.as_object_mut().unwrap().remove("template");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&recipe).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
