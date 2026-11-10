pub mod authoring;
pub mod doors;
pub mod random_door;
pub mod shot_solver;

pub fn trace_map(
    games: &asset_transport::GamesRoot,
    zone: &str,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
    let queries: Value = match source.extension().and_then(|s| s.to_str()) {
        Some("yml" | "yaml") => serde_yaml_ng::from_slice(&bytes).map_err(|e| e.to_string())?,
        _ => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
    };
    let vector = |v: &Value| -> Result<[f32; 3], String> {
        let a = v
            .as_array()
            .filter(|a| a.len() == 3)
            .ok_or("trace vector needs three numbers")?;
        let mut result = [0.; 3];
        for (out, v) in result.iter_mut().zip(a) {
            *out = v
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid trace coordinate")? as f32;
        }
        Ok(result)
    };
    let (prepared, _) = doors::load_map(games, zone)?;
    let clip = prepared.clip.as_ref().ok_or("map has no collision")?;
    let (brushes, bsp, mesh, _) = session::sim_clip_map(clip);
    let mut results = Vec::new();
    for q in queries.as_array().ok_or("trace queries must be an array")? {
        let start = vector(&q["start"])?;
        let end = vector(&q["end"])?;
        let mins = if q["mins"].is_null() {
            [0.; 3]
        } else {
            vector(&q["mins"])?
        };
        let maxs = if q["maxs"].is_null() {
            [0.; 3]
        } else {
            vector(&q["maxs"])?
        };
        let mask = q["mask"]
            .as_u64()
            .map(u32::try_from)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(sim::MASK_SHOT);
        let hit = sim::trace_world_clip(
            &brushes,
            &bsp,
            &mesh,
            &sim::TraceExtents {
                start,
                end,
                mins,
                maxs,
                mask,
            },
        );
        results.push(serde_json::json!({"query":q,"fraction":hit.fraction,"startsolid":hit.startsolid,"allsolid":hit.allsolid,
            "normal":hit.normal,"end":std::array::from_fn::<_,3,_>(|i|start[i]+(end[i]-start[i])*hit.fraction)}));
    }
    std::fs::write(destination,serde_json::to_vec_pretty(&serde_json::json!({"zone":zone,"scope":"Static world clip and static models; no dynamic entities or other placed worlds.","results":results})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}

pub fn weapon_bindings(
    games: &asset_transport::GamesRoot,
    zone: &str,
    destination: &Path,
) -> Result<(), String> {
    let (prepared, _) = doors::load_map(games, zone)?;
    let weapons = &prepared.weapons;
    let rows: Vec<_> = (1..weapons.len() as u32).map(|id|serde_json::json!({
        "id":id,"key":weapons.namespaced_key_of(id),"configuration":weapons.configuration_label(id),
        "camos":weapons.camouflage_choices(id),
        "recoil":weapons.bind_published_row(id).ok().and_then(|w|w.fpv_facts()).map(|f|serde_json::json!({
            "hip_view_pitch":[f.kick.hip_view_kick_pitch_min,f.kick.hip_view_kick_pitch_max],
            "ads_view_pitch":[f.kick.ads_view_kick_pitch_min,f.kick.ads_view_kick_pitch_max],
            "hip_view_yaw":[f.kick.hip_view_kick_yaw_min,f.kick.hip_view_kick_yaw_max],
            "ads_view_yaw":[f.kick.ads_view_kick_yaw_min,f.kick.ads_view_kick_yaw_max],
            "hip_gun_pitch":[f.kick.hip_gun_kick_pitch_min,f.kick.hip_gun_kick_pitch_max],
            "ads_gun_pitch":[f.kick.ads_gun_kick_pitch_min,f.kick.ads_gun_kick_pitch_max],
            "gun_limits":[f.kick.gun_max_pitch,f.kick.gun_max_yaw],
            "reduced_bullets":[f.kick.hip_gun_kick_reduced_kick_bullets,f.kick.ads_gun_kick_reduced_kick_bullets],
            "reduced_percent":[f.kick.hip_gun_kick_reduced_kick_percent,f.kick.ads_gun_kick_reduced_kick_percent]
        }))
    })).collect();
    std::fs::write(
        destination,
        serde_json::to_vec_pretty(&serde_json::json!({"zone":zone,"weapons":rows}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

use serde_json::Value;
use std::path::Path;

pub fn compile_recipe(source: &Path, destination: &Path) -> Result<(), String> {
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
    let recipe: Value = match source.extension().and_then(|s| s.to_str()) {
        Some("yml" | "yaml") => serde_yaml_ng::from_slice(&bytes).map_err(|e| e.to_string())?,
        _ => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
    };
    assets::SceneRecipe::parse(&recipe)?;
    let bytes = serde_json::to_vec_pretty(&recipe).map_err(|e| e.to_string())?;
    std::fs::write(destination, bytes).map_err(|e| e.to_string())
}

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
    for (index, world) in map.scene().worlds.iter().enumerate() {
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
        map.scene().worlds.len()
    ));
    Ok(())
}
