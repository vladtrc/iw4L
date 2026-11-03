use std::collections::BTreeMap;

use asset_core::atlas::{PortalDoor, PortalPair, SceneVersions, WorldId};
use asset_world::{PortalClearance, PortalDoorReplacement, PortalDoorTrim};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct SceneRecipe {
    pub worlds: Vec<String>,
    pub portals: Vec<ScenePortal>,
    pub dvars: BTreeMap<String, String>,
    pub versions: SceneVersions,
}

#[derive(Clone, Debug)]
pub struct ScenePortal {
    pub ends: [SceneEndpoint; 2],
    pub flip: bool,
}

#[derive(Clone, Debug)]
pub struct SceneEndpoint {
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
        Ok(Self {
            worlds,
            portals,
            dvars,
            versions,
        })
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
    let row = value[key]
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
