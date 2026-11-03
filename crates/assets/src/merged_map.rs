//! Merged maps: several maps' worlds side by side with doors between them
//! (`sim::projectile_flight`), played under a name of their own.
//!
//! A merged map is a recipe in `<artifacts>/maps/<name>.json`; `map <name>`
//! loads it like a map:
//!
//! ```json
//! { "worlds": ["iw4:mp_terminal", "t5:mp_firingrange", "t6:mp_nuketown_2020"],
//!   "portals": [{ "a": { "world": 0, "base": [x, y, z], "normal": [x, y, 0],
//!                        "width": 59.8, "height": 101.8 },
//!                 "b": { "world": 1, ... }, "flip": true }] }
//! ```
//!
//! World 0 is the match's map: its scripts, spawns and gametype run. World
//! `i` keeps its native coordinates and is placed `i · ATLAS_SLAB` along x.

use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use serde_json::Value;

/// The merged map the match loads; absent for a plain map.
#[derive(Resource, Clone, Debug)]
pub struct MergedMap {
    pub name: String,
    pub path: PathBuf,
    /// The zones of every world, world 0 first.
    pub worlds: Vec<String>,
    pub recipe: Value,
}

pub fn validate_scene_version(recipe: &Value) -> Result<(), String> {
    if recipe
        .get("schema_version")
        .is_some_and(|v| v.as_u64() != Some(1))
    {
        return Err("unsupported scene schema_version; expected 1".into());
    }
    if recipe["dvars"]
        .as_object()
        .is_some_and(|vars| vars.keys().any(|key| key.starts_with("brainrot_")))
    {
        return Err(
            "legacy projectile settings: migrate this recipe with iw4l-content migrate".into(),
        );
    }
    let worlds = recipe["worlds"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("scene needs at least one world")?;
    if worlds
        .iter()
        .any(|v| v.as_str().is_none_or(|name| name.trim().is_empty()))
    {
        return Err("invalid scene world reference".into());
    }
    if recipe.get("portals").is_some_and(|v| !v.is_array()) {
        return Err("scene portals must be an array".into());
    }
    let portals = recipe["portals"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if portals.len() > 255 {
        return Err("scene exceeds 255 portals".into());
    }
    for portal in portals {
        if !portal["flip"].is_boolean() {
            return Err("portal flip must be explicit".into());
        }
        for side in ["a", "b"] {
            let end = &portal[side];
            if end["world"]
                .as_u64()
                .is_none_or(|id| id >= worlds.len() as u64)
            {
                return Err(format!("portal {side} references an unavailable world"));
            }
            let base = parse_door_vec(end, "base").ok_or("invalid portal base")?;
            let normal = parse_door_vec(end, "normal").ok_or("invalid portal normal")?;
            let length = normal.iter().map(|v| v * v).sum::<f32>();
            if base.iter().chain(normal.iter()).any(|v| !v.is_finite())
                || (length - 1.0).abs() > 0.001
                || normal[2].abs() > 0.001
            {
                return Err("portal needs finite geometry and a horizontal unit normal".into());
            }
            if !end["two_sided"].is_boolean()
                || ["width", "height", "thickness"].iter().any(|key| {
                    end[key]
                        .as_f64()
                        .is_none_or(|v| !v.is_finite() || v <= 0.0 || !(v as f32).is_finite())
                })
            {
                return Err(
                    "portal needs explicit sidedness and positive dimensions/thickness".into(),
                );
            }
        }
    }
    Ok(())
}

fn parse_door_vec(value: &Value, key: &str) -> Option<[f32; 3]> {
    let a = value[key].as_array()?;
    Some([
        a.first()?.as_f64()? as f32,
        a.get(1)?.as_f64()? as f32,
        a.get(2)?.as_f64()? as f32,
    ])
}

impl MergedMap {
    /// The merged map `name` names, `None` when there is no such recipe.
    pub fn find(artifacts: &Path, name: &str) -> Option<Result<Self, String>> {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || name.contains(['/', '\\', ':', '.']) {
            return None;
        }
        let path = artifacts.join("maps").join(format!("{name}.json"));
        if !path.is_file() {
            return None;
        }
        Some(Self::read(name, path))
    }

    fn read(name: String, mut path: PathBuf) -> Result<Self, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut recipe: Value =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        validate_scene_version(&recipe)?;
        if recipe["template"].as_str() == Some("random-door") {
            if recipe["trim_clearance_version"] != 4 {
                return Err(format!(
                    "{}: incompatible door catalog; regenerate the visible-geometry catalog (version 4)",
                    path.display()
                ));
            }
            use std::hash::{BuildHasher, Hasher};
            let seed = std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish();
            crate::shuffle_random_doors(&mut recipe, seed)?;
            recipe.as_object_mut().unwrap().remove("template");
            path = path.with_file_name(format!("random-door-{seed:016x}.json"));
            // Freeze this run for demos. Reloading the template shuffles again;
            // replay loads this exact recipe rather than a different layout.
            std::fs::write(
                &path,
                serde_json::to_vec_pretty(&recipe).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("{}: {e}", path.display()))?;
            diag::info!(
                World,
                "random-door: seed={seed:016x} portals={} snapshot={}",
                recipe["portals"].as_array().map_or(0, Vec::len),
                path.display()
            );
        }
        if recipe["portals"].as_array().is_some_and(|p| !p.is_empty())
            && recipe["aperture_version"] != 4
        {
            return Err(format!(
                "{}: incompatible random-door aperture layout; regenerate with measured visible apertures (version 4)",
                path.display()
            ));
        }
        let name = recipe["display_name"].as_str().unwrap_or(&name).to_owned();
        let worlds: Vec<String> = recipe["worlds"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|zone| zone.as_str().map(str::to_owned))
            .collect::<Option<_>>()
            .ok_or_else(|| format!("{}: a world is not a zone name", path.display()))?;
        if worlds.is_empty() {
            return Err(format!("{}: no worlds", path.display()));
        }
        let map = Self {
            name,
            path,
            worlds,
            recipe,
        };
        map.door_trims()?;
        Ok(map)
    }

    pub fn door_trims(&self) -> Result<Vec<asset_world::PortalDoorTrim>, String> {
        if self.recipe.get("portals").is_none() {
            return Ok(Vec::new());
        }
        let pairs = self.recipe["portals"]
            .as_array()
            .ok_or("visible aperture layout has no pairs")?;
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        if self.recipe["aperture_version"] != 4 {
            return Err("incompatible aperture layout: regenerate version 4".into());
        }
        let mut out = Vec::new();
        for pair in pairs {
            let a_sides = pair["a"]["two_sided"]
                .as_bool()
                .ok_or("missing native a sidedness")?;
            let b_sides = pair["b"]["two_sided"]
                .as_bool()
                .ok_or("missing native b sidedness")?;
            if !a_sides && !b_sides && pair["flip"] != true {
                return Err(
                    "one-sided endpoints have no valid route without a flipped pairing".into(),
                );
            }
            let width = pair["shared_aperture"]["width"]
                .as_f64()
                .ok_or("missing shared aperture width")? as f32;
            let height = pair["shared_aperture"]["height"]
                .as_f64()
                .ok_or("missing shared aperture height")? as f32;
            for end in [&pair["a"], &pair["b"]] {
                let number = |key: &str| {
                    end[key]
                        .as_f64()
                        .map(|v| v as f32)
                        .ok_or_else(|| format!("invalid door {key}"))
                };
                let size = |key: &str| -> Result<[f32; 2], String> {
                    let v = end[key]
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or_else(|| format!("missing {key}"))?;
                    Ok([
                        v[0].as_f64().ok_or("invalid size")? as f32,
                        v[1].as_f64().ok_or("invalid size")? as f32,
                    ])
                };
                if end["geometry_version"] != 4 {
                    return Err("missing measured door geometry version 4".into());
                }
                let original = size("original_size")?;
                let opening_size = size("opening_size")?;
                let opening_base =
                    parse_door_vec(end, "opening_base").ok_or("missing visible opening base")?;
                let thickness = end["trim"]["thickness"]
                    .as_f64()
                    .ok_or("missing trim thickness")? as f32;
                let world = end["world"].as_u64().ok_or("missing door world")? as usize;
                let normal = parse_door_vec(end, "normal").ok_or("missing door normal")?;
                let base = parse_door_vec(end, "base").ok_or("missing threshold base")?;
                let tangent = [-normal[1], normal[0], 0.];
                let delta = std::array::from_fn::<_, 3, _>(|i| base[i] - opening_base[i]);
                let across = delta.iter().zip(tangent).map(|(a, b)| a * b).sum::<f32>();
                let plane = delta.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
                let bottom = delta[2];
                let left = opening_size[0] * 0.5 + across - width * 0.5;
                let right = opening_size[0] * 0.5 - across - width * 0.5;
                let top = opening_size[1] - bottom - height;
                if world >= self.worlds.len()
                    || width < 30.0
                    || height < 70.0
                    || thickness <= 0.0
                    || thickness > 128.0
                    || number("width")? != width
                    || number("height")? != height
                    || original[0] < width
                    || original[1] < height
                    || original[0] - width > crate::MAX_DOOR_WIDTH_TRIM as f32 + 0.001
                    || original[1] - height > crate::MAX_DOOR_HEIGHT_TRIM as f32 + 0.001
                    || left < -0.001
                    || right < -0.001
                    || bottom < -0.001
                    || bottom > 18.01
                    || top < -0.001
                    || plane.abs() > 0.01
                    || !original
                        .iter()
                        .chain(opening_size.iter())
                        .chain([width, height, thickness].iter())
                        .chain(normal.iter())
                        .chain(base.iter())
                        .chain(opening_base.iter())
                        .all(|v| v.is_finite())
                    || ((normal[0] * normal[0] + normal[1] * normal[1]).sqrt() - 1.).abs() > 0.005
                    || normal[2].abs() > 0.001
                {
                    return Err("invalid measured shared aperture dimensions".into());
                }
                for (key, want) in [
                    ("left", left),
                    ("right", right),
                    ("top", top),
                    ("bottom", bottom),
                ] {
                    if end["trim"][key]
                        .as_f64()
                        .is_none_or(|v| (v as f32 - want).abs() > 0.002)
                    {
                        return Err(format!("inconsistent doorway trim {key}"));
                    }
                }
                let door_id = end["id"].as_str().ok_or("missing doorway id")?.to_owned();
                diag::info!(
                    World,
                    "door pair end={door_id} world={world} aperture={width}x{height} threshold={bottom} trim=({left},{right},{top},{bottom}) thickness={thickness}"
                );
                out.push(asset_world::PortalDoorTrim {
                    clearance: match &end["clearance"] {
                        Value::Null => asset_world::PortalClearance::Walk,
                        Value::String(value) if value == "walk" => {
                            asset_world::PortalClearance::Walk
                        }
                        Value::String(value) if value == "flight" => {
                            asset_world::PortalClearance::Flight
                        }
                        value => return Err(format!("invalid portal clearance {value}")),
                    },
                    two_sided: end["two_sided"]
                        .as_bool()
                        .ok_or("missing native door sidedness")?,
                    world,
                    door_id,
                    opening_base,
                    opening_size,
                    thickness,
                    aperture: asset_world::AtlasDoorway {
                        world,
                        base,
                        normal,
                        width,
                        height,
                    },
                });
            }
        }
        Ok(out)
    }

    pub fn door_replacements(&self) -> Result<Vec<asset_world::PortalDoorReplacement>, String> {
        let mut out = Vec::new();
        for endpoint in self.recipe["portals"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|pair| [&pair["a"], &pair["b"]])
        {
            let Some(replace) = endpoint.get("replace").filter(|value| value.is_object()) else {
                continue;
            };
            let invalid = || {
                format!(
                    "{}: invalid replacement for {}",
                    self.path.display(),
                    endpoint["id"]
                )
            };
            let indices = |key: &str| -> Result<Vec<usize>, String> {
                replace[key]
                    .as_array()
                    .ok_or_else(invalid)?
                    .iter()
                    .map(|v| {
                        v.as_u64()
                            .and_then(|v| usize::try_from(v).ok())
                            .ok_or_else(invalid)
                    })
                    .collect()
            };
            let origin = replace["model_origin"]
                .as_array()
                .filter(|a| a.len() == 3)
                .ok_or_else(invalid)?;
            let mut model_origin = [0.0; 3];
            for i in 0..3 {
                model_origin[i] = origin[i].as_f64().ok_or_else(invalid)? as f32;
            }
            let leaf = asset_world::AtlasDoorway {
                world: endpoint["world"].as_u64().ok_or_else(invalid)? as usize,
                base: parse_door_vec(replace, "leaf_base").ok_or_else(invalid)?,
                normal: parse_door_vec(replace, "leaf_normal").ok_or_else(invalid)?,
                width: replace["leaf_size"][0].as_f64().ok_or_else(invalid)? as f32,
                height: replace["leaf_size"][1].as_f64().ok_or_else(invalid)? as f32,
            };
            if leaf.world >= self.worlds.len()
                || !model_origin
                    .iter()
                    .chain(leaf.base.iter())
                    .chain(leaf.normal.iter())
                    .chain([leaf.width, leaf.height].iter())
                    .all(|v| v.is_finite())
                || leaf.width <= 0.0
                || leaf.height <= 0.0
                || ((leaf.normal[0] * leaf.normal[0] + leaf.normal[1] * leaf.normal[1]).sqrt()
                    - 1.0)
                    .abs()
                    > 0.005
                || leaf.normal[2].abs() > 0.001
            {
                return Err(invalid());
            }
            out.push(asset_world::PortalDoorReplacement {
                world: endpoint["world"].as_u64().ok_or_else(invalid)? as usize,
                door_id: endpoint["id"].as_str().ok_or_else(invalid)?.to_owned(),
                render_instance: replace["render_instance"].as_u64().ok_or_else(invalid)? as usize,
                model_name: replace["model_name"]
                    .as_str()
                    .ok_or_else(invalid)?
                    .to_owned(),
                model_origin,
                clip_models: indices("clip_models")?
                    .into_iter()
                    .map(|i| u32::try_from(i).map_err(|_| invalid()))
                    .collect::<Result<_, _>>()?,
                clip_brushes: indices("clip_brushes")?,
                leaf,
            });
        }
        Ok(out)
    }

    /// Keep the exact map mechanism beside each portable demo/clip.
    pub fn save_recording_layout(&self, demo: &Path, zone: &str) -> Result<(), String> {
        let path = demo.with_extension("layout.json");
        let package = serde_json::json!({"schema":1,"zone":zone,"recipe":self.recipe});
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&package).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Restore a bundled frozen layout before playback tries to resolve its map.
    /// Existing layouts must agree; a sidecar must never silently replace one.
    pub fn install_recording_layout(
        artifacts: &Path,
        demo: &Path,
        zone: &str,
    ) -> Result<(), String> {
        let sidecar = demo.with_extension("layout.json");
        if !sidecar.is_file() {
            return Ok(());
        }
        if zone.is_empty()
            || !zone
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("recording layout has an invalid zone name".into());
        }
        let package: Value =
            serde_json::from_slice(&std::fs::read(&sidecar).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if package["schema"] != 1
            || package["zone"].as_str() != Some(zone)
            || package["recipe"]["template"].is_string()
            || package["recipe"]["worlds"]
                .as_array()
                .is_none_or(|worlds| worlds.is_empty() || worlds.iter().any(|w| !w.is_string()))
        {
            return Err(format!(
                "{}: invalid or mismatching frozen layout",
                sidecar.display()
            ));
        }
        let maps = artifacts.join("maps");
        let target = maps.join(format!("{zone}.json"));
        if target.exists() {
            let existing: Value =
                serde_json::from_slice(&std::fs::read(&target).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if existing != package["recipe"] {
                return Err(format!(
                    "{} conflicts with recorded layout",
                    target.display()
                ));
            }
        } else {
            std::fs::create_dir_all(maps).map_err(|e| e.to_string())?;
            std::fs::write(
                &target,
                serde_json::to_vec_pretty(&package["recipe"]).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        diag::info!(
            World,
            "recording layout: verified {} from {}",
            target.display(),
            sidecar.display()
        );
        Ok(())
    }

    /// A frozen layout's file name is its recording identity; its displayed
    /// title remains the template's name.
    pub fn recording_name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(&self.name)
    }

    /// The match's own map: world 0.
    pub fn zone(&self) -> &str {
        &self.worlds[0]
    }
}

/// One world of a merged map as the loading screen shows it.
#[derive(Clone, Debug)]
pub struct LoadingWorld {
    /// `game:map`, as a plain map's loading title reads.
    pub title: String,
    pub zone_ff: Option<PathBuf>,
    pub zone: String,
}

impl MergedMap {
    /// Every world's loading title and zone file, world 0 first.
    pub fn loading_worlds(&self, games: &asset_transport::GamesRoot) -> Vec<LoadingWorld> {
        self.worlds
            .iter()
            .map(|world| {
                let found = asset_transport::find_zone_file(games, world).ok();
                let key = world.trim().to_ascii_lowercase();
                let game = asset_transport::split_zone_key(&key).0.or_else(|| {
                    found
                        .as_ref()
                        .and_then(|z| asset_transport::zone_game_for_path(&z.path))
                });
                LoadingWorld {
                    title: asset_transport::map_load_title(&key, game),
                    zone: found
                        .as_ref()
                        .map_or_else(|| world.clone(), |z| z.zone_name.clone()),
                    zone_ff: found.map(|z| z.path),
                }
            })
            .collect()
    }
}
