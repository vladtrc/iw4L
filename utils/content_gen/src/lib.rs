pub mod doors;
pub mod random_door;
pub mod shot_solver;

use serde_json::Value;
use std::path::Path;

pub fn migrate(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!("{} already exists", destination.display()));
    }
    let mut recipe: Value =
        serde_json::from_slice(&std::fs::read(source).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if let Some(dvars) = recipe["dvars"].as_object_mut() {
        for key in dvars
            .keys()
            .filter(|key| key.starts_with("brainrot_"))
            .cloned()
            .collect::<Vec<_>>()
        {
            let renamed = format!("projectile_{}", key.strip_prefix("brainrot_").unwrap());
            if dvars.contains_key(&renamed) {
                return Err(format!("conflicting recipe settings: {key}, {renamed}"));
            }
            let value = dvars.remove(&key).unwrap();
            dvars.insert(renamed, value);
        }
    }
    recipe["schema_version"] = 1.into();
    let bytes = serde_json::to_vec_pretty(&recipe).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    output.write_all(&bytes).map_err(|e| e.to_string())
}

pub fn validate(games: &asset_transport::GamesRoot, path: &Path) -> Result<(), String> {
    let mut recipe: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    assets::validate_scene_version(&recipe)?;
    if recipe["template"].as_str() == Some("random-door") {
        assets::shuffle_random_doors(&mut recipe, 1)?;
    }
    recipe
        .as_object_mut()
        .ok_or("scene must be an object")?
        .remove("template");
    let map = assets::MergedMap::from_recipe("validation".into(), path.into(), recipe)?;
    for (index, world) in map.scene.worlds.iter().enumerate() {
        let (prepared, _) = doors::load_map(games, world)?;
        let clip = prepared
            .clip
            .as_ref()
            .ok_or_else(|| format!("{world}: no collision"))?;
        session::prepare_scene_collision(clip.clone(), &map, index)?;
    }
    diag::announce_stdout(&format!(
        "{}: validated {} native maps",
        path.display(),
        map.scene.worlds.len()
    ));
    Ok(())
}
