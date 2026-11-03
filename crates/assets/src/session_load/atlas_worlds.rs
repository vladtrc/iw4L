//! The other maps of a merged map (`crate::merged_map`), loaded with the
//! match and drawn with it (`asset_world::AtlasLayer`).
//!
//! Each world 1.. is walked by its own lane, its materials absorbed into the
//! match's pool and its draw appended to the match's world at its slab,
//! `world · ATLAS_SLAB` along x.

use super::*;
use crate::merged_map::MergedMap;

/// What a merged map adds to the match load: the zone files of worlds 1..
/// and the doors between the worlds.
#[derive(Default)]
pub struct AtlasLoad {
    pub(crate) zones: Vec<(asset_core::atlas::WorldId, String, PathBuf)>,
    pub(crate) doors: Vec<asset_world::AtlasDoorPair>,
    pub(crate) replacements: Vec<asset_world::PortalDoorReplacement>,
    pub(crate) trims: Vec<asset_world::PortalDoorTrim>,
    pub(crate) refusal: Option<String>,
}

/// The zone files and doors of `merged`; empty for a plain map. A world that
/// cannot be found refuses the entire merged match.
pub fn atlas_load(
    games: Option<&asset_transport::GamesRoot>,
    merged: Option<&MergedMap>,
) -> AtlasLoad {
    let Some(merged) = merged else {
        return AtlasLoad::default();
    };
    let Some(games) = games else {
        return AtlasLoad {
            refusal: Some(format!(
                "atlas render: no games root to find the maps of {}",
                merged.name
            )),
            ..Default::default()
        };
    };
    let mut zones = Vec::new();
    for (index, zone) in merged.scene.worlds.iter().enumerate().skip(1) {
        match asset_transport::find_zone_file(games, zone) {
            Ok(found) => zones.push((asset_core::atlas::WorldId(index), zone.clone(), found.path)),
            Err(error) => {
                return AtlasLoad {
                    refusal: Some(format!("atlas render: {zone}: {error}")),
                    ..Default::default()
                };
            }
        }
    }
    diag::info!(
        World,
        "atlas render: {} more worlds from {}",
        zones.len(),
        merged.path.display()
    );
    AtlasLoad {
        trims: merged.scene.trims().cloned().collect(),
        replacements: merged.scene.replacements().cloned().collect(),
        refusal: None,
        zones,
        doors: merged
            .scene
            .portals
            .iter()
            .map(|portal| {
                let [a, b] = &portal.ends;
                asset_world::AtlasDoorPair {
                    two_sided: [a.trim.two_sided, b.trim.two_sided],
                    a: a.trim.aperture,
                    b: b.trim.aperture,
                    flip: portal.flip,
                }
            })
            .collect(),
    }
}

/// One atlas world walked by its lane, materials still its own.
pub(super) struct AtlasWorldWalk {
    pub world: asset_core::atlas::WorldId,
    pub zone: String,
    pub path: PathBuf,
    pub loaded: LoadedWorld,
}

pub(super) fn walk_atlas_worlds(
    zones: Vec<(asset_core::atlas::WorldId, String, PathBuf)>,
    material_seed: &MaterialCatalog,
    film_visions: &crate::FilmVisionCatalog,
    progress: &LoadProgress,
) -> Result<Vec<AtlasWorldWalk>, String> {
    let mut walks = Vec::new();
    for (world, zone, path) in zones {
        if progress.is_canceled() {
            return Err("atlas load canceled".into());
        }
        let image = open_zone_shared(&path)
            .map_err(|error| format!("atlas world {world}: {}: {error}", path.display()))?;
        let selected = lane(image.game).ok_or_else(|| {
            format!(
                "atlas world {world}: zone lane not installed: {}",
                image.game.as_str()
            )
        })?;
        let loaded = selected.load_world(
            &path,
            &image,
            progress,
            Default::default(),
            material_seed.clone(),
            film_visions,
        );
        walks.push(AtlasWorldWalk {
            world,
            zone,
            path,
            loaded,
        });
    }
    Ok(walks)
}

/// Absorbs each walk's materials into `global` and appends its world to
/// `host`'s at its slab.
pub(super) fn absorb_atlas_worlds(
    host: &mut PreparedWorld,
    global: &mut MaterialCatalog,
    walks: Vec<AtlasWorldWalk>,
    doors: Vec<asset_world::AtlasDoorPair>,
    replacements: Vec<asset_world::PortalDoorReplacement>,
    trims: Vec<asset_world::PortalDoorTrim>,
    report: &mut Vec<String>,
) -> Result<Vec<PreparedAtlasWorld>, String> {
    let mut prepared = Vec::with_capacity(walks.len());
    hide_door_leaves(host, 0, &replacements, report);
    apply_door_trims(host, 0, &trims, report)?;
    for AtlasWorldWalk {
        world,
        zone,
        path,
        loaded,
    } in walks
    {
        let LoadedWorld {
            world: mut extra,
            mut materials,
            collision,
            report: world_report,
            ..
        } = loaded;
        let collision =
            collision.ok_or_else(|| format!("atlas world {world}: {zone}: missing collision"))?;
        report.extend(world_report);
        hide_door_leaves(&mut extra, world.index(), &replacements, report);
        apply_door_trims(&mut extra, world.index(), &trims, report)?;
        materials.promote_fallback_tables();
        materials.absorb_feature_variant_donors();
        materials.reroute_stub_materials();
        if let Some(zone) = path.file_stem() {
            global
                .sort_leftover_zones
                .insert(zone.to_string_lossy().into_owned());
        }
        let linked = global.absorb_asset_population_host_materials_win(materials);
        let shift = asset_core::atlas::WorldLayout.origin(world);
        append_atlas_world(host, world.index(), extra, shift, &linked)
            .map_err(|error| format!("atlas world {world}: {}: {error}", path.display()))?;
        let line = format!(
            "atlas world {world}: {} drawn at x+{} (materials linked={})",
            path.display(),
            shift[0],
            linked.len()
        );
        diag::info!(World, "{line}");
        report.push(line);
        prepared.push(PreparedAtlasWorld {
            id: world,
            zone,
            collision: Arc::new(collision),
        });
    }
    if let Some(draw) = host.draw.as_mut() {
        draw.atlas_doors = doors;
    }
    Ok(prepared)
}

fn apply_door_trims(
    world: &mut PreparedWorld,
    id: usize,
    trims: &[asset_world::PortalDoorTrim],
    report: &mut Vec<String>,
) -> Result<(), String> {
    for trim in trims.iter().filter(|trim| trim.world == id) {
        let count = trim.apply_render(
            world
                .draw
                .as_mut()
                .ok_or("door trim world has no geometry")?,
        )?;
        let message = format!(
            "door trim render {} world={id} faces={count} original={:?} aperture={}x{}",
            trim.door_id, trim.opening_size, trim.aperture.width, trim.aperture.height
        );
        diag::info!(World, "{message}");
        report.push(message);
    }
    Ok(())
}

fn append_atlas_world(
    host: &mut PreparedWorld,
    world: usize,
    mut extra: PreparedWorld,
    shift: [f32; 3],
    linked: &[usize],
) -> Result<(), String> {
    let brush_base = host
        .draw
        .as_ref()
        .ok_or("host world has no draw")?
        .brush_models
        .len();
    let imported =
        super::atlas_dynents::absorb(host, &mut extra, world, shift, linked, brush_base)?;
    let (Some(draw), Some(extra_draw)) = (host.draw.as_mut(), extra.draw) else {
        return Err("a world has no draw".into());
    };
    let probe_base = draw.reflection_probes.len() as u8;
    let light_base = draw.primary_lights.len() as u8;
    let smodel_base = host.static_model_instances.len();
    draw.append_atlas_layer(world, extra_draw, shift, linked, smodel_base)
        .map_err(|error| error.to_string())?;
    if let Some(layer) = draw.atlas_layers.last_mut() {
        layer.bmodel_poses = imported.poses;
        layer.bmodel_bounds = imported.bounds;
        layer.lighting.source_namespace = extra.source_namespace;
        layer.lighting.light_grid_codec = extra.light_grid.as_ref().map(|grid| grid.codec);
        layer.lighting.exp_fog = extra.exp_fog;
        layer.lighting.world_bounds = extra.world_bounds.map(|mut bounds| {
            for i in 0..3 {
                bounds[i] += shift[i];
            }
            bounds
        });
        layer.light_grid = extra.light_grid;
    }

    host.reflection_probe_images
        .resize(probe_base as usize, None);
    host.reflection_probe_images
        .extend(extra.reflection_probe_images);

    let mesh_base = host.static_model_meshes.len();
    for mut mesh in extra.static_model_meshes {
        for surface in mesh.lod_surfaces.iter_mut().flatten() {
            surface.material = surface.material.and_then(|slot| linked.get(slot).copied());
        }
        host.static_model_meshes.push(mesh);
    }
    let moved = |p: [f32; 3]| [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]];
    host.static_model_instances
        .extend(extra.static_model_instances.into_iter().map(|placement| {
            placement.map(|mut placement| {
                placement.mesh += mesh_base;
                placement.transform.translation += bevy::math::Vec3::from(shift);
                placement.origin = moved(placement.origin);
                placement.reflection_probe_index =
                    placement.reflection_probe_index.saturating_add(probe_base);
                placement.primary_light_index =
                    placement.primary_light_index.saturating_add(light_base);
                placement
            })
        }));
    host.smodel_lighting_samples
        .extend(extra.smodel_lighting_samples.into_iter().map(|mut sample| {
            sample.authored_slot += smodel_base;
            sample.lighting_origin = moved(sample.lighting_origin);
            sample
        }));
    Ok(())
}

fn hide_door_leaves(
    world: &mut PreparedWorld,
    world_id: usize,
    replacements: &[asset_world::PortalDoorReplacement],
    report: &mut Vec<String>,
) {
    for replacement in replacements.iter().filter(|r| r.world == world_id) {
        let matches = world
            .static_model_instances
            .get(replacement.render_instance)
            .and_then(Option::as_ref)
            .is_some_and(|placement| {
                world
                    .static_model_meshes
                    .get(placement.mesh)
                    .is_some_and(|mesh| {
                        mesh.name == replacement.model_name
                            && (0..3).all(|i| {
                                (placement.origin[i] - replacement.model_origin[i]).abs() < 0.1
                            })
                    })
            });
        let line = if matches {
            world.static_model_instances[replacement.render_instance] = None;
            format!(
                "portal replacement: {} world={world_id} render_instance={} model={} origin={:?} collision_models={:?} collision_brushes={:?}",
                replacement.door_id,
                replacement.render_instance,
                replacement.model_name,
                replacement.model_origin,
                replacement.clip_models,
                replacement.clip_brushes
            )
        } else {
            format!(
                "portal replacement refused: {} render identity changed",
                replacement.door_id
            )
        };
        diag::info!(World, "{line}");
        report.push(line);
    }
}
