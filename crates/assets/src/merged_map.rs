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
    scene: crate::SceneRecipe,
    source: Value,
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
    Ok(())
}

impl MergedMap {
    pub fn scene(&self) -> &crate::SceneRecipe {
        &self.scene
    }

    pub fn find(artifacts: &Path, name: &str) -> Option<Result<Self, String>> {
        let name = scene_name(name)?;
        let path = artifacts.join("maps").join(format!("{name}.json"));
        if path.is_file() {
            return Some(Self::read(name, path));
        }
        let (template, seed, digest) = seeded_name(&name)?;
        let source_path = artifacts.join("maps").join(format!("{template}.json"));
        Some((|| {
            let source = read_recipe(&source_path)?;
            if template_digest(&source)? != digest {
                return Err("scene template or generator differs from the authority".into());
            }
            Self::realize(name.clone(), path.clone(), source, seed, digest)
        })())
    }

    pub fn select(artifacts: &Path, name: &str) -> Option<Result<Self, String>> {
        let name = scene_name(name)?;
        let path = artifacts.join("maps").join(format!("{name}.json"));
        if !path.is_file() {
            return Self::find(artifacts, &name);
        }
        Some((|| {
            let source = read_recipe(&path)?;
            if source["template"].as_str() != Some("random-door") {
                return Self::from_recipe(name, path, source);
            }
            use std::hash::{BuildHasher, Hasher};
            let seed = std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish();
            let digest = template_digest(&source)?;
            let frozen = format!("{name}-{seed:016x}-{digest:016x}");
            if frozen.len() > 64 {
                return Err("scene template name exceeds the shared map-key budget".into());
            }
            let target = path.with_file_name(format!("{frozen}.json"));
            Self::realize(frozen, target, source, seed, digest)
        })())
    }

    pub fn select_name(artifacts: &Path, name: &str) -> Result<String, String> {
        Ok(Self::select(artifacts, name).transpose()?.map_or_else(
            || name.to_owned(),
            |scene| scene.recording_name().to_owned(),
        ))
    }

    fn realize(
        name: String,
        path: PathBuf,
        mut source: Value,
        seed: u64,
        digest: u64,
    ) -> Result<Self, String> {
        if source["template"].as_str() != Some("random-door")
            || source["trim_clearance_version"] != 4
        {
            return Err("scene requires a random-door template with door catalog version 4".into());
        }
        crate::shuffle_random_doors(&mut source, seed)?;
        source
            .as_object_mut()
            .ok_or("invalid scene template")?
            .remove("template");
        source["template_digest"] = serde_json::json!(digest);
        let map = Self::from_recipe(name, path, source)?;
        map.save_frozen()?;
        diag::info!(
            World,
            "random-door: seed={seed:016x} portals={} snapshot={}",
            map.scene.portals.len(),
            map.path.display()
        );
        Ok(map)
    }

    fn read(name: String, path: PathBuf) -> Result<Self, String> {
        Self::from_recipe(name, path.clone(), read_recipe(&path)?)
    }

    fn save_frozen(&self) -> Result<(), String> {
        if self.path.exists() {
            if read_recipe(&self.path)? != self.source {
                return Err(format!(
                    "{} conflicts with realized scene",
                    self.path.display()
                ));
            }
            return Ok(());
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = self
            .path
            .with_extension(format!("json.tmp-{}-{next}", std::process::id()));
        let result = (|| {
            std::fs::write(
                &temporary,
                serde_json::to_vec_pretty(&self.source).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::rename(&temporary, &self.path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }

    pub fn from_scene(name: String, scene: crate::SceneRecipe) -> Result<Self, String> {
        let name = scene_name(&name).ok_or("invalid scene name")?;
        let source = scene.to_value()?;
        Self::from_recipe(name.clone(), PathBuf::from(format!("{name}.json")), source)
    }

    pub fn from_recipe(name: String, path: PathBuf, source: Value) -> Result<Self, String> {
        let scene = crate::SceneRecipe::parse(&source)?;
        if let Some((_, seed, digest)) = seeded_name(&name)
            && (scene.versions.seed != Some(seed)
                || scene.versions.template_digest != Some(digest)
                || scene.versions.generator_algorithm != "random-door"
                || scene.versions.generator_version != crate::random_doors::GENERATOR_VERSION)
        {
            return Err("frozen scene does not match its shared key or generator".into());
        }
        let name = source["display_name"].as_str().unwrap_or(&name).to_owned();
        Ok(Self {
            name,
            path,
            scene,
            source,
        })
    }

    pub fn recording_recipe(&self) -> &Value {
        &self.source
    }

    /// Keep the exact map mechanism beside each portable demo/clip.
    pub fn save_recording_layout(&self, demo: &Path, zone: &str) -> Result<(), String> {
        let path = demo.with_extension("layout.json");
        let package = serde_json::json!({"schema":1,"zone":zone,"recipe":self.source});
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
        &self.scene.worlds[0]
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
        self.scene
            .worlds
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

fn scene_name(name: &str) -> Option<String> {
    let name = name.trim().to_ascii_lowercase();
    (!name.is_empty() && !name.contains(['/', '\\', ':', '.'])).then_some(name)
}

fn seeded_name(name: &str) -> Option<(&str, u64, u64)> {
    let (prefix, digest) = name.rsplit_once('-')?;
    let (template, seed) = prefix.rsplit_once('-')?;
    if template.is_empty() || seed.len() != 16 || digest.len() != 16 {
        return None;
    }
    Some((
        template,
        u64::from_str_radix(seed, 16).ok()?,
        u64::from_str_radix(digest, 16).ok()?,
    ))
}

fn read_recipe(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(
        &std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
    .map_err(|error| format!("{}: {error}", path.display()))
}

fn template_digest(source: &Value) -> Result<u64, String> {
    let bytes = serde_json::to_vec(source).map_err(|e| e.to_string())?;
    Ok(crate::fnv1a64_more(
        crate::fnv1a64(&crate::random_doors::GENERATOR_VERSION.to_le_bytes()),
        &bytes,
    ))
}
