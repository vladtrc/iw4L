//! The atlas of a merged map (`assets::MergedMap`): other maps' world clip
//! beside the match's own, and the doors between them (`sim::projectile_flight`).
//!
//! Worlds 1.. of the merged map are each loaded headless for their clip
//! only, while the match installs, so a merged map loads several maps.

use std::sync::Arc;

use asset_transport::{GamesRoot, NamespaceTrees, find_runtime_common_mp, find_zone_file};
use render_frontend::adapters::anim::killcam::AtlasClips;
use serde_json::Value;

/// The atlas of `merged`, or `None` for a plain map: the sim's, and the
/// clips and doors the killcam camera moves in.
pub(crate) fn load_atlas(
    merged: Option<&assets::MergedMap>,
    identity: Option<&frame::LaunchIdentity>,
    base_clip: Option<&asset_world::ClipCollision>,
) -> Result<Option<(Arc<sim::world_graph::Atlas>, AtlasClips)>, String> {
    let Some(merged) = merged else {
        return Ok(None);
    };
    let path = merged.path.display();
    if let Some(clip) = base_clip {
        validate_atlas_clip(clip).map_err(|error| format!("atlas base in {path}: {error}"))?;
    }
    let identity = identity.ok_or("atlas: no launch identity to find the maps")?;
    let portals = parse_portals(&merged.recipe, merged.worlds.len())
        .map_err(|error| format!("{error} in {path}"))?;
    let (games, families) = games_and_families(identity)?;
    let mut atlas = sim::world_graph::Atlas {
        portals,
        portal_clearance: merged
            .door_trims()?
            .chunks_exact(2)
            .map(|ends| [ends[0].clearance, ends[1].clearance])
            .collect(),
        portal_thickness: merged.recipe["portals"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|pair| {
                let thickness = |side: &str| {
                    pair[side]["thickness"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.)
                        .map(|n| n as f32)
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| {
                            format!("atlas: missing/invalid {side} door thickness in {path}")
                        })
                };
                Ok([thickness("a")?, thickness("b")?])
            })
            .collect::<Result<Vec<_>, String>>()?,
        ..Default::default()
    };
    let mut clips = vec![None];
    for zone in &merged.worlds[1..] {
        let (world, clip) = load_world(&games, &families, zone, merged, atlas.worlds.len() + 1)?;
        atlas.worlds.push(Arc::new(world));
        clips.push(Some(clip));
    }
    let door = |door: &sim::world_graph::AtlasDoor| *door;
    let atlas = Arc::new(atlas);
    let flight = base_clip.map(|clip| {
        let (brushes, bsp, mesh, cmodels) = crate::sim_clip_map(clip);
        let base = Arc::new(sim::world_graph::AtlasWorld {
            zone: merged.worlds[0].clone(),
            brushes,
            bsp,
            mesh,
            cmodels,
        });
        let rules = sim::projectile_flight::ProjectileFlightRules::default()
            .with_dvars(|key| merged.recipe["dvars"][key].as_str()?.parse().ok());
        (atlas.clone(), base, rules)
    });
    let killcam = AtlasClips {
        flight,
        clips,
        doors: atlas
            .portals
            .iter()
            .map(|portal| render_frame::PortalPair {
                a: door(&portal.a),
                b: door(&portal.b),
                flip: portal.flip,
            })
            .collect(),
    };
    diag::info!(
        World,
        "atlas: {} worlds, {} portals from {path}",
        atlas.worlds.len() + 1,
        atlas.portals.len()
    );
    Ok(Some((atlas, killcam)))
}

fn games_and_families(
    identity: &frame::LaunchIdentity,
) -> Result<(GamesRoot, frame::AssetFamilies), String> {
    let games = GamesRoot(identity.games_root.clone());
    let trees = NamespaceTrees::discover(&games);
    let have = frame::AssetFamilies::from_families(trees.present().map(|(ns, _)| ns.as_str())).0;
    let families = frame::AssetFamilies::from_inventory(&have, &frame::UiMenuDvars::default())?;
    Ok((games, families))
}

/// The portals of a merged map's recipe with `worlds` worlds.
fn parse_portals(
    recipe: &Value,
    worlds: usize,
) -> Result<Vec<sim::world_graph::AtlasPortal>, String> {
    let mut portals = Vec::new();
    for portal in recipe["portals"].as_array().into_iter().flatten() {
        portals.push(sim::world_graph::AtlasPortal {
            a: door(&portal["a"])?,
            b: door(&portal["b"])?,
            flip: portal["flip"].as_bool().unwrap_or(false),
        });
    }
    if portals.len() > 255 {
        return Err("atlas: more than 255 interactive portals".into());
    }
    for portal in &portals {
        for door in [&portal.a, &portal.b] {
            if door.world.index() >= worlds {
                return Err(format!("atlas: a door is in world {}", door.world.index()));
            }
        }
    }
    Ok(portals)
}

fn load_world(
    games: &GamesRoot,
    families: &frame::AssetFamilies,
    zone: &str,
    merged: &assets::MergedMap,
    world_id: usize,
) -> Result<
    (
        sim::world_graph::AtlasWorld,
        Arc<asset_world::ClipCollision>,
    ),
    String,
> {
    let prepared = load_prepared(games, families, zone)?;
    let clip = prepared
        .clip
        .ok_or_else(|| format!("atlas: {zone}: the map has no clip"))?;
    validate_atlas_clip(&clip).map_err(|error| format!("atlas: {zone}: {error}"))?;
    let clip = replace_clip(clip, merged, world_id)?;
    let (brushes, bsp, mesh, cmodels) = crate::sim_clip_map(&clip);
    diag::info!(World, "atlas: {zone}: {} brushes", brushes.len());
    Ok((
        sim::world_graph::AtlasWorld {
            zone: zone.to_owned(),
            brushes,
            bsp,
            mesh,
            cmodels,
        },
        clip,
    ))
}

fn validate_atlas_clip(clip: &asset_world::ClipCollision) -> Result<(), String> {
    let model = clip.cmodels.first().ok_or("missing world clip model")?;
    if !asset_core::atlas::atlas_bounds_fit(model.mins[0], model.maxs[0]) {
        return Err(format!(
            "native x bounds {}..{} exceed atlas coordinate domain",
            model.mins[0], model.maxs[0]
        ));
    }
    Ok(())
}

/// `zone` loaded headless.
fn load_prepared(
    games: &GamesRoot,
    families: &frame::AssetFamilies,
    zone: &str,
) -> Result<assets::PreparedMatch, String> {
    let found = find_zone_file(games, zone).map_err(|error| format!("atlas: {zone}: {error}"))?;
    let common_mp = find_runtime_common_mp(games, &found.path).map(|zone| zone.path);
    match bevy::tasks::futures_lite::future::block_on(assets::load_prepared_match(
        Ok(found.path),
        common_mp,
        asset_transport::LoadProgress::default(),
        families.clone(),
    )) {
        assets::MatchLoadOutcome::Ready(prepared) => Ok(*prepared),
        assets::MatchLoadOutcome::Canceled => Err(format!("atlas: {zone}: canceled")),
        assets::MatchLoadOutcome::Refused(reason) => Err(format!("atlas: {zone}: {reason}")),
    }
}

fn door(value: &Value) -> Result<sim::world_graph::AtlasDoor, String> {
    let vec3 = |key: &str| -> Result<[f32; 3], String> {
        let row = value[key]
            .as_array()
            .filter(|row| row.len() == 3)
            .ok_or_else(|| format!("atlas: door {key} is not [x, y, z]"))?;
        Ok(std::array::from_fn(|i| {
            row[i].as_f64().unwrap_or(0.0) as f32
        }))
    };
    let number = |key: &str| -> Result<f64, String> {
        value[key]
            .as_f64()
            .ok_or_else(|| format!("atlas: door {key} is not a number"))
    };
    Ok(sim::world_graph::AtlasDoor {
        two_sided: value["two_sided"]
            .as_bool()
            .ok_or("atlas: missing native sidedness; regenerate layout 4")?,
        world: sim::world_graph::WorldId(number("world")? as usize),
        base: vec3("base")?,
        normal: vec3("normal")?,
        width: number("width")? as f32,
        height: number("height")? as f32,
    })
}

pub fn replace_clip(
    clip: Arc<asset_world::ClipCollision>,
    merged: &assets::MergedMap,
    world: usize,
) -> Result<Arc<asset_world::ClipCollision>, String> {
    let replacements: Vec<_> = merged
        .door_replacements()?
        .into_iter()
        .filter(|r| r.world == world)
        .collect();
    let trims: Vec<_> = merged
        .door_trims()?
        .into_iter()
        .filter(|t| t.world == world)
        .collect();
    if replacements.is_empty() && trims.is_empty() {
        return Ok(clip);
    }
    let mut clip = Arc::unwrap_or_clone(clip);
    asset_world::PortalDoorReplacement::apply_all_collision(&replacements, &mut clip)?;
    for trim in &trims {
        trim.apply_collision(&mut clip)?;
    }
    if !trims.is_empty() {
        let (brushes, bsp, mesh, _) = crate::sim_clip_map(&clip);
        for trim in &trims {
            let door = sim::world_graph::AtlasDoor {
                two_sided: trim.two_sided,
                world: sim::world_graph::WorldId::ROOT,
                base: trim.aperture.base,
                normal: trim.aperture.normal,
                width: trim.aperture.width,
                height: trim.aperture.height,
            };
            let flight = trim.clearance == asset_world::PortalClearance::Flight;
            let validate = if flight {
                sim::door_exit::validate_flight
            } else {
                sim::door_exit::validate
            };
            validate(&door, trim.thickness, |start, end| {
                sim::trace_world_clip(
                    &brushes,
                    &bsp,
                    &mesh,
                    &clipmap_iw4::TraceExtents {
                        start,
                        end,
                        mins: if flight {
                            [-2.0; 3]
                        } else {
                            [-15.0, -15.0, 0.0]
                        },
                        maxs: if flight { [2.0; 3] } else { [15.0, 15.0, 70.0] },
                        mask: if flight {
                            sim::MASK_SHOT
                        } else {
                            sim::MASK_PLAYER_SOLID
                        },
                    },
                )
            })
            .map_err(|error| {
                format!(
                    "{}: trimmed doorway has no valid accessible exit: {error}",
                    trim.door_id
                )
            })?;
        }
    }
    for replacement in replacements {
        diag::info!(
            World,
            "portal collision replacement: {} world={world} models={:?} brushes={:?}",
            replacement.door_id,
            replacement.clip_models,
            replacement.clip_brushes
        );
    }
    Ok(Arc::new(clip))
}
