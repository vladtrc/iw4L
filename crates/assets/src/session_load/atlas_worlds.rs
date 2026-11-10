//! The other maps of a merged map (`crate::merged_map`), loaded with the
//! match and drawn with it (`asset_world::AtlasLayer`).
//!
//! Each world 1.. is walked by its own lane, its materials absorbed into the

use super::*;
use crate::merged_map::MergedMap;

/// What a merged map adds to the match load: the zone files of worlds 1..
/// and the doors between the worlds.
#[derive(Default)]
pub struct AtlasLoad {
    pub(crate) space: crate::SceneSpace,
    pub(crate) regions: Vec<Option<asset_world::WorldRegion>>,
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
    if let Err(error) = merged.scene().validate() {
        return AtlasLoad {
            refusal: Some(error),
            ..Default::default()
        };
    }
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
    for (index, zone) in merged.scene().worlds.iter().enumerate().skip(1) {
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
        space: merged.scene().space.clone(),
        regions: merged.scene().regions.clone(),
        trims: merged.scene().trims().cloned().collect(),
        replacements: merged.scene().replacements().cloned().collect(),
        refusal: None,
        zones,
        doors: merged
            .scene()
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
    regions: &[Option<asset_world::WorldRegion>],
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
        let loaded = match regions.get(world.index()).copied().flatten() {
            Some(region) => selected.load_world_region(
                &path,
                &image,
                progress,
                material_seed.clone(),
                film_visions,
                region,
            ),
            None => selected.load_world(
                &path,
                &image,
                progress,
                Default::default(),
                material_seed.clone(),
                film_visions,
            ),
        };
        if loaded.collision.is_none() {
            let reasons = loaded
                .gaps
                .iter()
                .map(|gap| gap.reason.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "atlas world {world}: {zone}: missing collision: {reasons}"
            ));
        }
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
pub(super) struct AtlasGeometry {
    pub space: crate::SceneSpace,
    pub doors: Vec<asset_world::AtlasDoorPair>,
    pub replacements: Vec<asset_world::PortalDoorReplacement>,
    pub trims: Vec<asset_world::PortalDoorTrim>,
}

pub(super) fn absorb_atlas_worlds(
    host: &mut PreparedWorld,
    global: &mut MaterialCatalog,
    walks: Vec<AtlasWorldWalk>,
    geometry: AtlasGeometry,
    report: &mut Vec<String>,
) -> Result<Vec<PreparedAtlasWorld>, String> {
    let AtlasGeometry {
        space,
        doors,
        replacements,
        trims,
    } = geometry;
    space.validate(walks.len() + 1, doors.len())?;
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
            facts,
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
        let frame = space.frame(world)?;
        append_atlas_world(host, world.index(), extra, frame, &linked)
            .map_err(|error| format!("atlas world {world}: {}: {error}", path.display()))?;
        let line = format!(
            "atlas world {world}: {} drawn with frame {:?} (materials linked={})",
            path.display(),
            frame,
            linked.len()
        );
        diag::info!(World, "{line}");
        report.push(line);
        prepared.push(PreparedAtlasWorld {
            id: world,
            compass: crate::SessionCompass {
                namespace: zone
                    .split_once(':')
                    .and_then(|(family, _)| asset_core::FamilyId::parse(family)),
                corners: facts.minimap_corners.map(|mut corners| {
                    let origin = asset_core::atlas::WorldLayout.origin(world);
                    corners.a[0] += origin[0];
                    corners.b[0] += origin[0];
                    corners
                }),
                north_yaw: facts.north_yaw,
                declaration: facts.compass,
                ..Default::default()
            },
            zone,
            collision: Arc::new(collision),
        });
    }
    if let Some(draw) = host.draw.as_mut() {
        draw.space = space;
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
    frame: asset_core::world_frame::WorldFrame,
    linked: &[usize],
) -> Result<(), String> {
    let brush_base = host
        .draw
        .as_ref()
        .ok_or("host world has no draw")?
        .brush_models
        .len();
    let imported =
        super::atlas_dynents::absorb(host, &mut extra, world, frame, linked, brush_base)?;
    let (Some(draw), Some(extra_draw)) = (host.draw.as_mut(), extra.draw) else {
        return Err("a world has no draw".into());
    };
    let probe_base = draw.reflection_probes.len() as u8;
    let light_base = draw.primary_lights.len() as u8;
    let smodel_base = host.static_model_instances.len();
    draw.append_atlas_layer(world, extra_draw, frame, linked, smodel_base)
        .map_err(|error| error.to_string())?;
    if let Some(layer) = draw.atlas_layers.last_mut() {
        layer.bmodel_poses = imported.poses;
        layer.bmodel_bounds = imported.bounds;
        layer.lighting.source_namespace = extra.source_namespace;
        layer.lighting.light_grid_codec = extra.light_grid.as_ref().map(|grid| grid.codec);
        layer.lighting.exp_fog = extra.exp_fog;
        layer.lighting.world_bounds = extra.world_bounds.map(|bounds| {
            let (mid, half) = frame.bounds_to_scene(
                [bounds[0], bounds[1], bounds[2]],
                [bounds[3], bounds[4], bounds[5]],
            );
            [mid[0], mid[1], mid[2], half[0], half[1], half[2]]
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
    let carry = super::atlas_dynents::frame_pose(frame);
    let rotation = bevy::math::Quat::from_mat4(&carry);
    host.static_model_instances
        .extend(extra.static_model_instances.into_iter().map(|placement| {
            placement.map(|mut placement| {
                placement.mesh += mesh_base;
                place_static_model_pose(&mut placement, frame, carry, rotation);
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
            sample.lighting_origin = frame.point_to_scene(sample.lighting_origin);
            sample
        }));
    Ok(())
}

fn place_static_model_pose(
    placement: &mut asset_world::StaticModelPlacement,
    frame: asset_core::world_frame::WorldFrame,
    carry: bevy::math::Mat4,
    rotation: bevy::math::Quat,
) {
    placement.transform.translation = carry.transform_point3(placement.transform.translation);
    placement.transform.rotation = rotation * placement.transform.rotation;
    placement.origin = frame.point_to_scene(placement.origin);
    // Static-model skinning consumes the captured basis, not Transform.rotation.
    placement.axis = placement.axis.map(|axis| frame.vector_to_scene(axis));
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
