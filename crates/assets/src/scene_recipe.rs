use std::collections::BTreeMap;

use asset_core::atlas::{PortalDoor, PortalPair, SceneVersions, WorldId};
use asset_world::{PortalClearance, PortalDoorReplacement, PortalDoorTrim};
use serde_json::Value;

pub(crate) fn valid_character_source(source: &str) -> bool {
    source
        .split_once(':')
        .is_some_and(|(family, zone)| asset_core::FamilyId::valid_token(family) && !zone.is_empty())
}

#[derive(Clone, Debug)]
pub struct SceneRecipe {
    pub worlds: Vec<String>,
    pub multiplayer: Option<SceneMultiplayer>,
    pub characters: Vec<SceneCharacter>,
    pub regions: Vec<Option<asset_world::WorldRegion>>,
    pub playerclip_openings: Vec<ScenePlayerclipOpening>,
    pub space: SceneSpace,
    pub portals: Vec<ScenePortal>,
    pub dvars: BTreeMap<String, String>,
    pub versions: SceneVersions,
}

/// Multiplayer content and authored FFA spawn points for a scene world.
#[derive(Clone, Debug)]
pub struct SceneMultiplayer {
    pub source: String,
    pub spawns: Vec<asset_world::SpawnPoint>,
}

/// An optional player kit captured from an installed donor zone.
#[derive(Clone, Debug)]
pub struct SceneCharacter {
    pub client: u32,
    pub world: Option<usize>,
    pub source: String,
    /// Play a repeating clip authored on the donor rig instead of player locomotion.
    pub native_loop: Option<String>,
    pub native_loop_source: Option<String>,
    /// Another captured body whose rest rig matches the selected original clip.
    pub native_loop_reference: Option<String>,
    pub native_actions: Vec<SceneCharacterAction>,
    pub kit: asset_model::SoldierKit,
}

/// A timed original animation; times are actor command-clock milliseconds.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneCharacterAction {
    pub clip: String,
    #[serde(default)]
    pub source: Option<String>,
    pub start_ms: i32,
    pub duration_ms: i32,
    pub blend_ms: i32,
}

pub use asset_core::world_frame::SceneSpace;

#[derive(Clone, Debug)]
pub struct ScenePlayerclipOpening {
    pub world: usize,
    pub region: asset_world::WorldRegion,
}

#[derive(Clone, Debug)]
pub struct ScenePortal {
    pub ends: [SceneEndpoint; 2],
    pub flip: bool,
}

#[derive(Clone, Debug)]
pub struct SceneEndpoint {
    pub original_size: [f32; 2],
    pub trim: PortalDoorTrim,
    pub thickness: f32,
    pub replacement: Option<PortalDoorReplacement>,
}

impl SceneEndpoint {
    pub fn door(&self) -> PortalDoor {
        let aperture = self.trim.aperture;
        PortalDoor {
            world: WorldId(aperture.world),
            two_sided: self.trim.two_sided,
            base: aperture.base,
            normal: aperture.normal,
            width: aperture.width,
            height: aperture.height,
        }
    }
}

impl ScenePortal {
    pub fn pair(&self) -> PortalPair {
        PortalPair {
            a: self.ends[0].door(),
            b: self.ends[1].door(),
            flip: self.flip,
        }
    }
}

impl SceneRecipe {
    pub fn parse(source: &Value) -> Result<Self, String> {
        crate::merged_map::validate_scene_version(source)?;
        if source.get("template").is_some_and(|value| !value.is_null()) {
            return Err("scene template must be selected by the authority before loading".into());
        }
        let worlds: Vec<String> = source["worlds"]
            .as_array()
            .ok_or("missing worlds")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("invalid world name".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let regions = match source.get("regions") {
            None => vec![None; worlds.len()],
            Some(rows) => rows
                .as_array()
                .filter(|rows| rows.len() == worlds.len())
                .ok_or("regions needs one entry per world (null means full world)")?
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    if row.is_null() {
                        return Ok(None);
                    }
                    if index == 0 {
                        return Err(
                            "region selection is supported for additional worlds only".into()
                        );
                    }
                    if !worlds[index].starts_with("iw4:") {
                        return Err("region selection currently supports IW4 maps only".into());
                    }
                    asset_world::WorldRegion::new(
                        vector::<3>(row, "min")?,
                        vector::<3>(row, "max")?,
                    )
                    .map(Some)
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        if source.get("native_playerclip").is_some() {
            return Err(
                "native_playerclip is unsupported: author bounded playerclip_openings instead"
                    .into(),
            );
        }
        let playerclip_openings = source
            .get("playerclip_openings")
            .map(|rows| {
                rows.as_array()
                    .ok_or("playerclip_openings must be an array")?
                    .iter()
                    .map(|row| {
                        let world = row["world"]
                            .as_u64()
                            .and_then(|world| usize::try_from(world).ok())
                            .filter(|world| *world < worlds.len())
                            .ok_or("invalid playerclip opening world")?;
                        let region = asset_world::WorldRegion::new(
                            vector::<3>(row, "min")?,
                            vector::<3>(row, "max")?,
                        )?;
                        Ok(ScenePlayerclipOpening { world, region })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?
            .unwrap_or_else(Vec::new);
        let optional_integer = |key: &str| {
            source
                .get(key)
                .map(|v| v.as_u64().ok_or_else(|| format!("invalid scene {key}")))
                .transpose()
        };
        let versions = SceneVersions {
            schema: version(source, "schema_version", 1)?,
            aperture: version(source, "aperture_version", 0)?,
            trim_clearance: version(source, "trim_clearance_version", 0)?,
            generator_algorithm: source["generator"]["algorithm"]
                .as_str()
                .unwrap_or("")
                .into(),
            generator_version: version(&source["generator"], "version", 0)?,
            seed: optional_integer("seed")?,
            template_digest: optional_integer("template_digest")?,
        };
        let mut portals = Vec::new();
        for pair in source["portals"].as_array().into_iter().flatten() {
            if versions.aperture != 4 {
                return Err("incompatible aperture layout: regenerate version 4".into());
            }
            let flip = pair["flip"].as_bool().ok_or("missing portal flip")?;
            let width = number(&pair["shared_aperture"], "width")?;
            let height = number(&pair["shared_aperture"], "height")?;
            let ends = [
                parse_endpoint(&pair["a"], worlds.len(), width, height)?,
                parse_endpoint(&pair["b"], worlds.len(), width, height)?,
            ];
            if !ends[0].trim.two_sided && !ends[1].trim.two_sided && !flip {
                return Err(
                    "one-sided endpoints have no valid route without a flipped pairing".into(),
                );
            }
            portals.push(ScenePortal { ends, flip });
        }
        if source.get("dvars").is_some_and(|value| !value.is_object()) {
            return Err("scene dvars must be an object".into());
        }
        let dvars = source["dvars"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, value)| {
                (
                    key.clone(),
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                )
            })
            .collect();
        let space = parse_space(source, worlds.len())?;
        space.validate(worlds.len(), portals.len())?;
        let characters = source.get("characters").map(|rows| {
            rows.as_array().ok_or("scene characters must be an array")?.iter().map(|row| {
                let string = |key: &str| row[key].as_str().filter(|s| !s.is_empty())
                    .map(str::to_owned).ok_or_else(|| format!("invalid character {key}"));
                let optional = |key: &str| match row.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    _ => string(key).map(Some),
                };
                if row.get("arms").is_some_and(|value| !value.is_null()) {
                    return Err("scene characters currently override world bodies and heads; first-person hands remain multiplayer hands".into());
                }
                Ok(SceneCharacter {
                    client: row["client"].as_u64().and_then(|n| u32::try_from(n).ok())
                        .ok_or("invalid character client")?,
                    world: row.get("world").filter(|v| !v.is_null()).map(|v| v.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("invalid character world")).transpose()?,
                    source: string("source")?,
                    native_loop: optional("native_loop")?,
                    native_loop_source: optional("native_loop_source")?,
                    native_loop_reference: optional("native_loop_reference")?,
                    native_actions: row.get("native_actions").map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string())).transpose()?.unwrap_or_else(Vec::new),
                    kit: asset_model::SoldierKit { body: string("body")?, head: optional("head")?, arms: None },
                })
            }).collect::<Result<Vec<_>, String>>()
        }).transpose()?.unwrap_or_else(Vec::new);
        let multiplayer = source
            .get("multiplayer")
            .filter(|v| !v.is_null())
            .map(|row| {
                if row["gametype"].as_str() != Some("dm") {
                    return Err("scene multiplayer currently requires gametype dm".to_owned());
                }
                let source = row["source"]
                    .as_str()
                    .ok_or("missing multiplayer source")?
                    .to_owned();
                let spawns = row["spawns"]
                    .as_array()
                    .ok_or("missing multiplayer spawns")?
                    .iter()
                    .map(|spawn| {
                        Ok(asset_world::SpawnPoint {
                            classname: "mp_dm_spawn".into(),
                            origin: vector::<3>(spawn, "origin")?,
                            angles: vector::<3>(spawn, "angles")?,
                            script_linkto: String::new(),
                            script_destructable_area: String::new(),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(SceneMultiplayer { source, spawns })
            })
            .transpose()?;
        let recipe = Self {
            worlds,
            multiplayer,
            characters,
            regions,
            playerclip_openings,
            space,
            portals,
            dvars,
            versions,
        };
        recipe.validate()?;
        Ok(recipe)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.space.validate(self.worlds.len(), self.portals.len())?;
        if self.worlds.iter().any(|world| world.trim().is_empty())
            || self.regions.len() != self.worlds.len()
            || self.portals.len() > 255
            || self.versions.schema != 1
            || (!self.portals.is_empty() && self.versions.aperture != 4)
        {
            return Err("invalid scene worlds, regions, portals or version".into());
        }
        if let Some(mp) = &self.multiplayer
            && (!mp.source.starts_with("iw4:mp_")
                || !self.worlds[0].starts_with("iw4:")
                || mp.spawns.is_empty()
                || mp.spawns.len() > 256
                || mp.spawns.iter().any(|p| {
                    p.classname != "mp_dm_spawn"
                        || p.origin
                            .iter()
                            .chain(p.angles.iter())
                            .any(|v| !v.is_finite())
                }))
        {
            return Err(
                "scene multiplayer needs an IW4 world, IW4 MP source and 1..256 finite FFA spawns"
                    .into(),
            );
        }
        let mut clients = std::collections::BTreeSet::new();
        for character in &self.characters {
            let mut action_end = i32::MIN;
            for action in &character.native_actions {
                if character.native_loop.is_none()
                    || action.clip.is_empty()
                    || action
                        .source
                        .as_ref()
                        .is_some_and(|s| !valid_character_source(s))
                    || action.start_ms < action_end
                    || action.duration_ms <= 0
                    || action.blend_ms < 0
                    || action.blend_ms > action.duration_ms / 2
                    || action.start_ms.checked_add(action.duration_ms).is_none()
                {
                    return Err("invalid character animation action".into());
                }
                action_end = action.start_ms + action.duration_ms;
            }
            if character.kit.arms.is_some()
                || character.client >= 18
                || !clients.insert((character.client, character.world))
                || character
                    .world
                    .is_some_and(|world| world >= self.worlds.len())
                || !valid_character_source(&character.source)
                || character.native_loop_source.as_ref().is_some_and(|source| {
                    character.native_loop.is_none() || !valid_character_source(source)
                })
                || (self.multiplayer.is_none() && !self.worlds[0].starts_with("iw4:mp_"))
                || character
                    .native_loop_reference
                    .as_ref()
                    .is_some_and(|reference| {
                        character.native_loop.is_none()
                            || !self.characters.iter().any(|other| {
                                other.kit.body == *reference
                                    && other.native_loop_reference.is_none()
                            })
                    })
                || std::iter::once(&character.kit.body)
                    .chain(character.kit.head.iter())
                    .chain(character.kit.arms.iter())
                    .any(|name| {
                        name.is_empty()
                            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
            {
                return Err("characters require unique client/world bindings with clients 0..17, qualified sources, model names and an IW4 multiplayer root".into());
            }
        }
        for (world, region) in self.regions.iter().enumerate() {
            if let Some(region) = region {
                asset_world::WorldRegion::new(region.min, region.max)?;
                if world == 0 || !self.worlds[world].starts_with("iw4:") {
                    return Err("region selection supports additional IW4 worlds only".into());
                }
            }
        }
        for opening in &self.playerclip_openings {
            if opening.world >= self.worlds.len() {
                return Err("invalid playerclip opening world".into());
            }
            asset_world::WorldRegion::new(opening.region.min, opening.region.max)?;
        }
        for portal in &self.portals {
            let [a, b] = &portal.ends;
            if a.trim.aperture.width != b.trim.aperture.width
                || a.trim.aperture.height != b.trim.aperture.height
                || (!a.trim.two_sided && !b.trim.two_sided && !portal.flip)
            {
                return Err("incompatible portal endpoints".into());
            }
            for end in &portal.ends {
                let trim = &end.trim;
                let door = trim.aperture;
                let delta: [f32; 3] = std::array::from_fn(|i| door.base[i] - trim.opening_base[i]);
                let across = -delta[0] * door.normal[1] + delta[1] * door.normal[0];
                let plane: f32 = delta.iter().zip(door.normal).map(|(a, b)| a * b).sum();
                if trim.world >= self.worlds.len()
                    || trim.world != door.world
                    || trim.door_id.is_empty()
                    || end.original_size.iter().any(|v| !v.is_finite())
                    || end.original_size[0] < door.width
                    || end.original_size[1] < door.height
                    || end.original_size[0] - door.width > crate::MAX_DOOR_WIDTH_TRIM as f32 + 0.001
                    || end.original_size[1] - door.height
                        > crate::MAX_DOOR_HEIGHT_TRIM as f32 + 0.001
                    || door
                        .base
                        .iter()
                        .chain(door.normal.iter())
                        .chain(trim.opening_base.iter())
                        .chain(trim.opening_size.iter())
                        .chain([door.width, door.height, trim.thickness, end.thickness].iter())
                        .any(|v| !v.is_finite())
                    || door.width < 30.0
                    || door.height < 70.0
                    || !(0.0..=128.0).contains(&trim.thickness)
                    || trim.thickness == 0.0
                    || end.thickness <= 0.0
                    || (door.normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 0.001
                    || door.normal[2].abs() > 0.001
                    || plane.abs() > 0.01
                    || trim.opening_size[0] * 0.5 - across.abs() - door.width * 0.5 < -0.001
                    || !(-0.001..=18.01).contains(&delta[2])
                    || trim.opening_size[1] - delta[2] - door.height < -0.001
                {
                    return Err("invalid typed portal geometry or world".into());
                }
                if let Some(replacement) = &end.replacement {
                    let leaf = replacement.leaf;
                    if replacement.world != trim.world
                        || leaf.world != trim.world
                        || replacement.door_id != trim.door_id
                        || replacement.model.as_ref().is_some_and(|model| {
                            model.model_name.is_empty()
                                || model.model_origin.iter().any(|v| !v.is_finite())
                        })
                        || leaf
                            .base
                            .iter()
                            .chain(leaf.normal.iter())
                            .chain([leaf.width, leaf.height].iter())
                            .any(|v| !v.is_finite())
                        || leaf.width <= 0.0
                        || leaf.height <= 0.0
                        || (leaf.normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 0.01
                        || leaf.normal[2].abs() > 0.001
                    {
                        return Err("invalid typed portal replacement".into());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn to_value(&self) -> Result<Value, String> {
        self.validate()?;
        use serde_json::json;
        let mut source = json!({
            "schema_version": self.versions.schema,
            "worlds": self.worlds,
            "characters": self.characters.iter().map(|c| json!({"client":c.client,"world":c.world,"source":c.source,"native_loop":c.native_loop,"native_loop_source":c.native_loop_source,"native_loop_reference":c.native_loop_reference,"native_actions":c.native_actions,"body":c.kit.body,"head":c.kit.head})).collect::<Vec<_>>(),
            "regions": self.regions.iter().map(|region| region.map(|r| json!({"min":r.min,"max":r.max}))).collect::<Vec<_>>(),
            "playerclip_openings": self.playerclip_openings.iter().map(|o| json!({"world":o.world,"min":o.region.min,"max":o.region.max})).collect::<Vec<_>>(),
            "dvars":self.dvars,
            "aperture_version":self.versions.aperture,
            "trim_clearance_version":self.versions.trim_clearance,
            "generator":{"algorithm":self.versions.generator_algorithm,"version":self.versions.generator_version},
        });
        if let Some(mp) = &self.multiplayer {
            source["multiplayer"] = json!({"source":mp.source,"gametype":"dm",
                "spawns":mp.spawns.iter().map(|p| json!({"origin":p.origin,"angles":p.angles})).collect::<Vec<_>>()});
        }
        if let Some(seed) = self.versions.seed {
            source["seed"] = json!(seed);
        }
        if let Some(digest) = self.versions.template_digest {
            source["template_digest"] = json!(digest);
        }
        if let SceneSpace::Shared(frames) = &self.space {
            source["space"] = json!("shared");
            source["placements"] = json!(frames.iter().map(|frame| json!({
                "origin":frame.point_to_scene([0.0;3]),
                "axes":[frame.vector_to_scene([1.,0.,0.]),frame.vector_to_scene([0.,1.,0.]),frame.vector_to_scene([0.,0.,1.])],
            })).collect::<Vec<_>>());
        }
        let endpoint = |end: &SceneEndpoint| {
            let trim = &end.trim;
            let door = trim.aperture;
            let across = -(door.base[0] - trim.opening_base[0]) * door.normal[1]
                + (door.base[1] - trim.opening_base[1]) * door.normal[0];
            let bottom = door.base[2] - trim.opening_base[2];
            let replace = end.replacement.as_ref().map(|r| {
                let mut value = json!({"clip_brushes":r.clip_brushes,"clip_triangles":r.clip_triangles,
                    "leaf_base":r.leaf.base,"leaf_normal":r.leaf.normal,"leaf_size":[r.leaf.width,r.leaf.height]});
                if let Some(model) = &r.model {
                    value["render_instance"] = json!(model.render_instance);
                    value["model_name"] = json!(model.model_name);
                    value["model_origin"] = json!(model.model_origin);
                    value["clip_models"] = json!(model.clip_models);
                }
                value
            });
            json!({"geometry_version":4,"world":trim.world,"id":trim.door_id,
                "base":door.base,"normal":door.normal,"width":door.width,"height":door.height,
                "original_size":end.original_size,"opening_base":trim.opening_base,"opening_size":trim.opening_size,
                "thickness":end.thickness,"two_sided":trim.two_sided,"replace":replace,
                "clearance":if trim.clearance == PortalClearance::Flight {"flight"} else {"walk"},
                "trim":{"thickness":trim.thickness,"left":trim.opening_size[0]*0.5+across-door.width*0.5,
                    "right":trim.opening_size[0]*0.5-across-door.width*0.5,"bottom":bottom,"top":trim.opening_size[1]-bottom-door.height},
            })
        };
        source["portals"] = json!(self.portals.iter().map(|portal| json!({
            "flip":portal.flip,"shared_aperture":{"width":portal.ends[0].trim.aperture.width,"height":portal.ends[0].trim.aperture.height},
            "a":endpoint(&portal.ends[0]),"b":endpoint(&portal.ends[1]),
        })).collect::<Vec<_>>());
        Ok(source)
    }

    pub fn trims(&self) -> impl Iterator<Item = &PortalDoorTrim> {
        self.portals
            .iter()
            .flat_map(|pair| pair.ends.iter().map(|end| &end.trim))
    }

    pub fn replacements(&self) -> impl Iterator<Item = &PortalDoorReplacement> {
        self.portals
            .iter()
            .flat_map(|pair| pair.ends.iter().filter_map(|end| end.replacement.as_ref()))
    }
}

fn parse_space(source: &Value, worlds: usize) -> Result<SceneSpace, String> {
    match source.get("space").and_then(Value::as_str) {
        None if source.get("space").is_none() && source.get("placements").is_none() => {
            Ok(SceneSpace::PortalAtlas)
        }
        Some("portal_atlas") if source.get("placements").is_none() => Ok(SceneSpace::PortalAtlas),
        Some("shared") => {
            let placements = source["placements"]
                .as_array()
                .filter(|rows| rows.len() == worlds)
                .ok_or("shared space needs one placement per world")?;
            let frames = placements
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let origin = vector::<3>(row, "origin")?;
                    let axes = row["axes"]
                        .as_array()
                        .filter(|rows| rows.len() == 3)
                        .ok_or("invalid placement axes")?;
                    let mut basis = [[0.0; 3]; 3];
                    for (axis, value) in basis.iter_mut().zip(axes) {
                        *axis = vector_value::<3>(value, "placement axis")?;
                    }
                    asset_core::world_frame::WorldFrame::new(origin, basis)
                        .map_err(|error| format!("world {index}: {error}"))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(SceneSpace::Shared(frames))
        }
        _ => Err("invalid scene space or placements".into()),
    }
}

fn version(source: &Value, key: &str, default: u64) -> Result<u64, String> {
    source.get(key).map_or(Ok(default), |v| {
        v.as_u64().ok_or_else(|| format!("invalid scene {key}"))
    })
}

fn number(value: &Value, key: &str) -> Result<f32, String> {
    value[key]
        .as_f64()
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid door {key}"))
}

fn vector<const N: usize>(value: &Value, key: &str) -> Result<[f32; N], String> {
    vector_value(&value[key], key)
}

fn vector_value<const N: usize>(value: &Value, key: &str) -> Result<[f32; N], String> {
    let row = value
        .as_array()
        .filter(|row| row.len() == N)
        .ok_or_else(|| format!("invalid door {key}"))?;
    let mut result = [0.0; N];
    for (slot, value) in result.iter_mut().zip(row) {
        *slot = value
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("invalid door {key}"))?;
    }
    Ok(result)
}

fn world_id(value: &Value, worlds: usize) -> Result<usize, String> {
    value["world"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v < worlds)
        .ok_or("portal references an unavailable world".into())
}

fn parse_endpoint(
    end: &Value,
    worlds: usize,
    width: f32,
    height: f32,
) -> Result<SceneEndpoint, String> {
    if end["geometry_version"] != 4 {
        return Err("missing measured door geometry version 4".into());
    }
    let world = world_id(end, worlds)?;
    let original = vector::<2>(end, "original_size")?;
    let opening_size = vector::<2>(end, "opening_size")?;
    let opening_base = vector::<3>(end, "opening_base")?;
    let base = vector::<3>(end, "base")?;
    let normal = vector::<3>(end, "normal")?;
    let thickness = number(&end["trim"], "thickness")?;
    let traversal_thickness = number(end, "thickness")?;
    let tangent = [-normal[1], normal[0], 0.0];
    let delta = std::array::from_fn::<_, 3, _>(|i| base[i] - opening_base[i]);
    let across = delta.iter().zip(tangent).map(|(a, b)| a * b).sum::<f32>();
    let plane = delta.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
    let bottom = delta[2];
    let left = opening_size[0] * 0.5 + across - width * 0.5;
    let right = opening_size[0] * 0.5 - across - width * 0.5;
    let top = opening_size[1] - bottom - height;
    if width < 30.0
        || height < 70.0
        || thickness <= 0.0
        || thickness > 128.0
        || traversal_thickness <= 0.0
        || number(end, "width")? != width
        || number(end, "height")? != height
        || original[0] < width
        || original[1] < height
        || original[0] - width > crate::MAX_DOOR_WIDTH_TRIM as f32 + 0.001
        || original[1] - height > crate::MAX_DOOR_HEIGHT_TRIM as f32 + 0.001
        || left < -0.001
        || right < -0.001
        || !(-0.001..=18.01).contains(&bottom)
        || top < -0.001
        || plane.abs() > 0.01
        || (normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 0.001
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
        if (number(&end["trim"], key)? - want).abs() > 0.002 {
            return Err(format!("inconsistent doorway trim {key}"));
        }
    }
    let clearance = match end.get("clearance") {
        None | Some(Value::Null) => PortalClearance::Walk,
        Some(Value::String(v)) if v == "walk" => PortalClearance::Walk,
        Some(Value::String(v)) if v == "flight" => PortalClearance::Flight,
        _ => return Err("invalid portal clearance".into()),
    };
    Ok(SceneEndpoint {
        original_size: original,
        thickness: traversal_thickness,
        replacement: parse_portal_replacement(end, worlds)?,
        trim: PortalDoorTrim {
            clearance,
            two_sided: end["two_sided"]
                .as_bool()
                .ok_or("missing native door sidedness")?,
            world,
            door_id: end["id"].as_str().ok_or("missing doorway id")?.into(),
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
        },
    })
}

pub fn parse_portal_replacement(
    endpoint: &Value,
    worlds: usize,
) -> Result<Option<PortalDoorReplacement>, String> {
    let replace = match endpoint.get("replace") {
        None | Some(Value::Null) => return Ok(None),
        Some(value) if value.is_object() => value,
        _ => return Err("invalid portal replacement".into()),
    };
    let world = world_id(endpoint, worlds)?;
    let indices = |key: &str| -> Result<Vec<usize>, String> {
        replace[key]
            .as_array()
            .ok_or("invalid replacement indices")?
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or("invalid replacement index".into())
            })
            .collect()
    };
    let base = vector::<3>(replace, "leaf_base")?;
    let normal = vector::<3>(replace, "leaf_normal")?;
    let size = vector::<2>(replace, "leaf_size")?;
    if size.iter().any(|v| *v <= 0.0)
        || ((normal[0] * normal[0] + normal[1] * normal[1]).sqrt() - 1.0).abs() > 0.005
        || normal[2].abs() > 0.001
    {
        return Err("invalid portal replacement geometry".into());
    }
    Ok(Some(PortalDoorReplacement {
        world,
        door_id: endpoint["id"].as_str().ok_or("missing doorway id")?.into(),
        model: match replace.get("render_instance") {
            None | Some(Value::Null) => {
                if replace.get("model_name").is_some()
                    || replace.get("model_origin").is_some()
                    || replace.get("clip_models").is_some()
                {
                    return Err("BSP replacement must omit model identity fields".into());
                }
                None
            }
            Some(index) => Some(asset_world::PortalDoorModel {
                render_instance: index
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or("invalid render instance")?,
                model_name: replace["model_name"]
                    .as_str()
                    .ok_or("invalid replacement model")?
                    .into(),
                model_origin: vector::<3>(replace, "model_origin")?,
                clip_models: indices("clip_models")?
                    .into_iter()
                    .map(|i| u32::try_from(i).map_err(|_| "invalid replacement model index".into()))
                    .collect::<Result<_, String>>()?,
            }),
        },
        clip_brushes: indices("clip_brushes")?,
        clip_triangles: if replace.get("clip_triangles").is_some() {
            indices("clip_triangles")?
        } else {
            Vec::new()
        },
        leaf: asset_world::AtlasDoorway {
            world,
            base,
            normal,
            width: size[0],
            height: size[1],
        },
    }))
}
