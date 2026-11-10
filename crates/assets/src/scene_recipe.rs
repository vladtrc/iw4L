use std::collections::BTreeMap;

use asset_core::atlas::{PortalDoor, PortalPair, SceneVersions, WorldId};
use asset_world::{PortalClearance, PortalDoorReplacement, PortalDoorTrim};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct SceneRecipe {
    pub worlds: Vec<String>,
    pub regions: Vec<Option<asset_world::WorldRegion>>,
    pub playerclip_openings: Vec<ScenePlayerclipOpening>,
    pub space: SceneSpace,
    pub portals: Vec<ScenePortal>,
    pub dvars: BTreeMap<String, String>,
    pub versions: SceneVersions,
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
        let recipe = Self {
            worlds,
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
                        || replacement.model_name.is_empty()
                        || replacement
                            .model_origin
                            .iter()
                            .chain(leaf.base.iter())
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
            "regions": self.regions.iter().map(|region| region.map(|r| json!({"min":r.min,"max":r.max}))).collect::<Vec<_>>(),
            "playerclip_openings": self.playerclip_openings.iter().map(|o| json!({"world":o.world,"min":o.region.min,"max":o.region.max})).collect::<Vec<_>>(),
            "dvars":self.dvars,
            "aperture_version":self.versions.aperture,
            "trim_clearance_version":self.versions.trim_clearance,
            "generator":{"algorithm":self.versions.generator_algorithm,"version":self.versions.generator_version},
        });
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
            let replace = end.replacement.as_ref().map(|r| json!({
                "render_instance":r.render_instance,"model_name":r.model_name,"model_origin":r.model_origin,
                "clip_models":r.clip_models,"clip_brushes":r.clip_brushes,
                "leaf_base":r.leaf.base,"leaf_normal":r.leaf.normal,"leaf_size":[r.leaf.width,r.leaf.height],
            }));
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
    let model_origin = vector::<3>(replace, "model_origin")?;
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
        render_instance: replace["render_instance"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("invalid render instance")?,
        model_name: replace["model_name"]
            .as_str()
            .ok_or("invalid replacement model")?
            .into(),
        model_origin,
        clip_models: indices("clip_models")?
            .into_iter()
            .map(|i| u32::try_from(i).map_err(|_| "invalid replacement model index".into()))
            .collect::<Result<_, String>>()?,
        clip_brushes: indices("clip_brushes")?,
        leaf: asset_world::AtlasDoorway {
            world,
            base,
            normal,
            width: size[0],
            height: size[1],
        },
    }))
}
