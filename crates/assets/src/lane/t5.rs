use std::path::Path;

use super::{
    CommonCensus, CommonWalkSink, LoadedWorld, MaterialPopulation, MaterialPopulationSink,
    ZoneLane, ZoneWalkSink,
};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use crate::progress::{LoadProgress, StageId};
use crate::session_load::{PreparedWorld, WorldDrawPolicy};
use crate::{
    MASK_PLAYER_SOLID, T5ZoneMemory, ZoneGame, ZoneImage, build_t5_clip_collision,
    build_t5_world_draw, decode_material_color_maps, decode_reflection_probe_cubemap,
    dm_spawn_points_t5, intermission_view_t5, minimap_corners_t5,
};

pub struct T5Lane;

impl T5Lane {
    pub const GAME: ZoneGame = ZoneGame::T5;
    pub const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::SupportedPopulated),
        (
            PreparedCapability::PreparedWorld,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::CollisionSpawns,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::WeaponCatalog,
            LaneStatus::MissingEvidence,
        ),
        (
            PreparedCapability::BodySkeleton,
            LaneStatus::MissingEvidence,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];
}

impl ZoneLane for T5Lane {
    fn game(&self) -> ZoneGame {
        Self::GAME
    }

    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        material_seed: crate::MaterialCatalog,
        _common_film_visions: &mut std::collections::BTreeMap<
            String,
            Result<crate::FilmVision, crate::FilmVisionParseError>,
        >,
    ) -> LoadedWorld {
        let mut report = vec![format!("game: T5 ({})", path.display())];
        let stage = progress.begin_scoped(StageId::MapAssets, "header", None);
        let header = match image.t5_header() {
            Ok(h) => {
                stage.done();
                h
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::t5(),
                    PreparedCapability::PreparedWorld,
                    format!("T5 zone header: {e}"),
                    Some("assets::lane::t5::load_world/zone_header"),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "memory", None);
        report.push(crate::zone::xfile_arena_row(
            "zone arenas map",
            &header.block_size,
            fastfile_t5::XFILE_BLOCK_TEMP,
            fastfile_t5::XFILE_BLOCK_VIRTUAL,
        ));
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(s) => {
                stage.done();
                s
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::t5(),
                    PreparedCapability::PreparedWorld,
                    format!("T5 zone arenas: {e}"),
                    Some("assets::lane::t5::load_world/zone_arenas"),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "walk", None);
        let mut sink = ZoneWalkSink::default();
        let seeded_techsets = material_seed.technique_set_facts().to_vec();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::T5);
        sink.sound = Some(asset_audio::ZoneSoundCapture::for_map(
            path,
            asset_audio::ZoneGame::T5,
            "map",
        ));
        let walked = fastfile_t5::load_zone(&mut stream, &mut sink);
        let map_sound = sink
            .sound
            .take()
            .map(|sound| sound.finish(walked.as_ref().map(|_| ()).map_err(|e| e.to_string())));
        match &walked {
            Ok(_) => report.push(format!("zone walk: complete, {} assets", sink.walked)),
            Err(e) => report.push(format!(
                "zone walk: stopped after {} assets — {e}",
                sink.walked
            )),
        }
        stage.set_completed(sink.walked as u64);
        report.push(format!(
            "pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        let runtime_overrun = stream.block_overrun(fastfile_t5::XFILE_BLOCK_RUNTIME as u8);
        report.push(format!(
            "retail block overrun: runtime +{runtime_overrun} bytes"
        ));

        let clip = if let Some(geometry) = stream.clip_map() {
            report.push(format!(
                "clipmap: planes={} brushes={} leaves={} nodes={} cmodels={} verts={} tris={}",
                geometry.plane_count,
                geometry.brush_count,
                geometry.leaf_count,
                geometry.node_count,
                geometry.cmodel_count,
                geometry.vert_count,
                geometry.tri_count
            ));
            match build_t5_clip_collision(&stream, geometry).and_then(|mut clip| {
                sink.map_xmodels
                    .attach_t5_clip_models(&stream, geometry, &mut clip)?;
                Ok(clip)
            }) {
                Ok(clip) => {
                    let solid = clip
                        .brushes
                        .iter()
                        .filter(|b| b.contents & MASK_PLAYER_SOLID != 0)
                        .count();
                    let dropped = geometry.brush_count.saturating_sub(clip.brushes.len());
                    report.push(format!(
                        "clip brushes extracted: {} ({} player-solid by contents mask)",
                        clip.brushes.len(),
                        solid
                    ));
                    if dropped > 0 {
                        report.push(format!(
                            "t5 clip: dropped {dropped} map-sized player-solid volumes \
                             (IW4 pmove startsolid inside the playable box)"
                        ));
                    }
                    report.push(format!(
                        "t5 clip mesh: verts={} tris={} (walk verts={} tris={})",
                        clip.mesh.verts.len(),
                        clip.mesh.tri_indices.len() / 3,
                        geometry.vert_count,
                        geometry.tri_count
                    ));
                    Some(clip)
                }
                Err(e) => {
                    report.push(format!("clip brushes: extract failed — {e}"));
                    None
                }
            }
        } else {
            report.push("clipmap: not reached".into());
            None
        };

        let exp_fog = sink.exp_fog;
        let createart_name = sink.createart_name.clone();
        let t5_teamset = sink.t5_teamset.clone();
        let script_sound = std::mem::take(&mut sink.script_sound).finish();
        let mut scripts = std::mem::take(&mut sink.scripts);
        if let Some(entities) = crate::map_ents_entity_string_t5(&stream) {
            scripts.set_entities(entities.to_owned());
        }
        match (&createart_name, exp_fog) {
            (Some(name), Some(fog)) => report.push(format!(
                "t5 createart: {name} fog=ready start={:.1} half={:.1}",
                fog.start_dist, fog.halfway_dist
            )),
            (Some(name), None) => {
                report.push(format!("t5 createart: {name} fog=unparsed"));
            }
            (None, _) => report
                .push("t5 createart: none (RawFile not captured or walk aborted first)".into()),
        }
        report.push(match t5_teamset.as_deref() {
            Some(name) => format!("t5 teamset: {name} (map GSC _teamset_*::level_init)"),
            None => {
                "t5 teamset: none (map GSC missing, packed decode failed, or no level_init)".into()
            }
        });

        stage.finish_from(&walked);
        let stage = progress.begin_scoped(StageId::Images, "map", None);
        let compass = std::mem::take(&mut sink.compass).resolve(&sink.materials);
        report.push(format!("compass: {:?}", compass));
        let mut materials = std::mem::take(&mut sink.materials);
        let absorbed = materials.absorb_technique_set_tables(&seeded_techsets);
        let promoted = materials.promote_iw5_fallback_tables();
        let t5_alias = materials.absorb_t5_feature_token_donors();
        let stub_routed = materials.reroute_stub_materials();
        report.push(format!(
            "material route (pre-decode): absorbed_techsets={absorbed} iw5_promoted={promoted} t5_tech_alias={t5_alias} \
             stub_routed={stub_routed} unrouted={}",
            materials.unrouted_material_count()
        ));
        let map_xmodels = std::mem::take(&mut sink.map_xmodels);
        let bodies = std::mem::take(&mut sink.bodies);
        let fpv_meshes = std::mem::take(&mut sink.fpv_meshes);
        match decode_material_color_maps(
            path,
            &mut materials,
            &stage,
            crate::session_load::load_pool(),
        ) {
            Ok(stats) => {
                report.push(format!(
                "IWD color/normal maps: {}/{} decoded, {} missing, {} unsupported from {} archives",
                stats.decoded, stats.requested, stats.missing, stats.unsupported, stats.archives
            ));
                if let Some(gap) = stats.first_gap {
                    report.push(format!("IWD material-map gap: {gap}"));
                }
            }
            Err(error) => report.push(format!("IWD material-map gap: {error}")),
        }
        report.push(format!(
            "material catalog: {} materials, {} images, {} capture gaps",
            materials.materials.len(),
            materials.images.len(),
            materials.capture_gaps
        ));

        stage.done();
        let stage = progress.begin_scoped(StageId::MapAssets, "geometry", None);
        let Some(geometry) = stream.gfx_world() else {
            stage.fail();
            report.push("no GfxWorld retained — nothing to draw".into());
            let dm_spawns = dm_spawn_points_t5(&stream);
            let mut loaded = LoadedWorld {
                scripts,
                sound: map_sound,
                world: PreparedWorld {
                    policy: WorldDrawPolicy::t5(),
                    exp_fog,
                    createart_name,
                    ..Default::default()
                },
                collision: clip,
                spawns: dm_spawns,
                bodies,
                fpv_meshes,
                facts: crate::MapFacts {
                    t5_teamset: t5_teamset.clone(),
                    script_sound: script_sound.clone(),
                    ..Default::default()
                },
                report,
                ..Default::default()
            };
            loaded.push_gap(
                PreparedCapability::PreparedWorld,
                "no GfxWorld retained — nothing to draw",
                Some("assets::lane::t5::load_world/no_gfx_world"),
            );
            return loaded;
        };
        report.push(format!(
            "gfx retained: verts={} indices={} surfaces={} lightmaps={} cells={} smodels={}",
            geometry.vertex_count,
            geometry.index_count,
            geometry.surface_count,
            geometry.lightmap_count,
            geometry.cell_count,
            geometry.smodel_count
        ));

        let world_draw = build_t5_world_draw(&stream, geometry, materials);
        stage.finish_from(&world_draw);
        match world_draw {
            Ok((mut draw, map_materials)) => {
                let mut map_xmodels = map_xmodels;
                if let Some(name) = geometry.sky_box_model.and_then(|p| stream.cstr(p).ok()) {
                    draw.sky_model = map_xmodels.take_named_mesh(name);
                    report.push(format!(
                        "skybox model: {name} captured={}",
                        draw.sky_model.is_some()
                    ));
                }
                let map_models = super::build_t5_static_model_draw(&stream, geometry, map_xmodels);
                if let Some(error) = map_models.static_error.as_ref() {
                    report.push(format!("static models: {error}"));
                }
                let smodels = &map_models.static_draw;
                report.push(format!(
                "static models: {}/{} authored slots resolved to {} unique meshes ({} unresolved)",
                smodels.resolved_count(),
                geometry.smodel_count,
                smodels.meshes.len(),
                smodels.gaps
            ));
                report.push(format!(
                    "script_model: {} placements linked (MapEnts props; separate visibility owner)",
                    map_models.script_instances.len()
                ));
                report.push(format!(
                    "script_brushmodel: {} *N placements (SP_script_brushmodel, not DrawInst)",
                    map_models.script_brush_models.len()
                ));
                let crate::PreparedMapModels {
                    static_draw:
                        crate::StaticModelDraw {
                            meshes: static_model_meshes,
                            placements: static_model_instances,
                            ..
                        },
                    scene_assets: map_xmodel_scene_assets,
                    script_instances: script_model_instances,
                    script_brush_models,
                    flag_descriptors,
                    script_structs,
                    ..
                } = map_models;
                let intermission_view = intermission_view_t5(&stream);
                let minimap_corners = minimap_corners_t5(&stream);
                let north_yaw = crate::worldspawn_north_yaw_t5(&stream);
                let dm_spawns = dm_spawn_points_t5(&stream);
                report.push(format!(
                    "ffa spawns: {} mp_dm_spawn* ({} start)",
                    dm_spawns.len(),
                    dm_spawns.iter().filter(|p| p.is_initial()).count()
                ));
                match intermission_view {
                    Some(view) => report.push(format!(
                        "camera: mp_global_intermission origin={:?} angles={:?}",
                        view.origin, view.angles
                    )),
                    None => report.push("camera: mp_global_intermission not found".into()),
                }
                report.push(format!(
                    "world mesh: {} vertices, {} triangles, {} surfaces ({} skipped)",
                    draw.stats.vertices,
                    draw.stats.triangles,
                    draw.stats.surfaces,
                    draw.stats.skipped_surfaces
                ));
                let lightmapped_surfaces = draw
                    .surface_lightmapped
                    .iter()
                    .filter(|&&lightmapped| lightmapped)
                    .count();
                report.push(format!(
                    "world batches: {lightmapped_surfaces} lightmapped, {} fallback surfaces",
                    draw.surface_lightmapped.len() - lightmapped_surfaces
                ));
                let material_surfaces = draw
                    .surface_materials
                    .iter()
                    .filter(|material| material.is_some())
                    .count();
                report.push(format!(
                    "material surfaces: {material_surfaces}/{}, {} compact batches",
                    draw.surface_materials.len(),
                    draw.batches.len()
                ));
                report.push(format!(
                    "primary lights: {} (sun index ≤ {})",
                    draw.primary_lights.len(),
                    geometry.sun_primary_light_index
                ));
                report.push(
                    "draw path: T5 materials + lightmaps + clip + DPVS + lights/probes/sky".into(),
                );
                match &draw.lightmap {
                    Ok(pages) => {
                        let decoded = pages.iter().flatten().count();
                        let exact = pages
                            .iter()
                            .flatten()
                            .filter(|p| p.primary_image.is_some() && p.secondary_image.is_some())
                            .count();
                        let names: Vec<&str> = pages
                            .iter()
                            .flatten()
                            .map(|p| p.ambient_source_name.as_str())
                            .collect();
                        report.push(format!(
                            "lightmap: {decoded}/{} pages decoded exact={exact}/{} ({})",
                            pages.len(),
                            pages.len(),
                            names.join(", ")
                        ));
                    }
                    Err(gap) => report.push(format!("lightmap gap: {gap}")),
                }
                let reflection_probe_images = draw
                    .reflection_probes
                    .iter()
                    .map(|probe| {
                        probe.image.and_then(|image| {
                            map_materials.images.get(image).and_then(|source| {
                                match decode_reflection_probe_cubemap(source) {
                                    Ok(image) => Some(image),
                                    Err(error) => {
                                        report.push(format!(
                                            "reflection probe {} gap: {error}",
                                            source.name
                                        ));
                                        None
                                    }
                                }
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                report.push(format!(
                    "reflection probes: {}/{} cubemaps decoded",
                    reflection_probe_images.iter().flatten().count(),
                    reflection_probe_images.len()
                ));
                report.push(format!(
                    "dpvs: cells={} planes={} nodes={} sorted={} portal_verts={} cleared_boxes={}",
                    draw.dpvs.cell_count,
                    draw.dpvs.planes.len(),
                    draw.dpvs.nodes.len(),
                    draw.dpvs.sorted_surf_index.len(),
                    draw.dpvs.portal_verts.len(),
                    draw.dpvs.cleared_boxes
                ));
                let smodel_lighting_samples = {
                    use crate::model_lighting::{
                        OwnedLightGrid, build_smodel_lighting_samples_with_sight,
                        census_lit_fragment_tiles, collect_t5_smodel_lighting_origins,
                    };
                    use lighting_iw4::LIGHT_GRID_SIGHT_CONTENT_MASK;
                    match OwnedLightGrid::from_t5_stream(&stream, geometry.light_grid) {
                        Some(owned) => {
                            report.push(format!(
                                "t5 light_grid: READY rows={} entries={} colors={} row_axis={} col_axis={} regions={}",
                                owned.row_data_start.len() / 2,
                                owned.entries.len() / 4,
                                owned.color_count,
                                owned.row_axis,
                                owned.col_axis,
                                u8::from(owned.has_light_regions)
                            ));
                            let origins: Vec<_> =
                                collect_t5_smodel_lighting_origins(&stream, geometry)
                                    .into_iter()
                                    .filter(|(slot, _)| {
                                        static_model_instances
                                            .get(*slot)
                                            .and_then(|placement| placement.as_ref())
                                            .is_some()
                                    })
                                    .collect();
                            let (tiles, census) = if let Some(ref clip_map) = clip {
                                let clear = |start: [f32; 3], end: [f32; 3]| {
                                    clip_map.box_sight_clear(
                                        start,
                                        end,
                                        LIGHT_GRID_SIGHT_CONTENT_MASK,
                                    )
                                };
                                let (tiles, census) = build_smodel_lighting_samples_with_sight(
                                    &owned.view(),
                                    &origins,
                                    Some(&clear),
                                );
                                report.push(format!(
                                "smodel lighting: lit={} / candidates={} (blocked row={} trunc={} empty={}; CM sight mask=0x{LIGHT_GRID_SIGHT_CONTENT_MASK:x} corners need={} cleared={} suppressed={})",
                                census.lit,
                                census.candidates,
                                census.blocked_unmodelled_row,
                                census.blocked_truncated,
                                census.blocked_no_live_corner,
                                census.corners_needing_sight,
                                census.corners_needing_sight.saturating_sub(census.corners_sight_suppressed),
                                census.corners_sight_suppressed,
                            ));
                                (tiles, census)
                            } else {
                                let (tiles, census) = build_smodel_lighting_samples_with_sight(
                                    &owned.view(),
                                    &origins,
                                    None,
                                );
                                report.push(format!(
                                "smodel lighting: lit={} / candidates={} (no clipmap — needsTrace corners suppressed; blocked row={} trunc={} empty={})",
                                census.lit,
                                census.candidates,
                                census.blocked_unmodelled_row,
                                census.blocked_truncated,
                                census.blocked_no_live_corner,
                            ));
                                (tiles, census)
                            };
                            if let Some(frag) = census_lit_fragment_tiles(&tiles) {
                                report.push(format!(
                                "smodel lit_fragment mid-grey: tiles={} lum min={:.4} max={:.4} mean={:.4} (specular=0)",
                                frag.tiles, frag.lum_min, frag.lum_max, frag.lum_mean
                            ));
                            }
                            let _ = census;
                            (tiles, Some(owned))
                        }
                        None => {
                            report.push(format!(
                                "t5 light_grid: none (missing tables or rowAxis/colAxis not 0..2; row={} col={} entries={} colors={})",
                                geometry.light_grid.row_axis,
                                geometry.light_grid.col_axis,
                                geometry.light_grid.entry_count,
                                geometry.light_grid.color_count
                            ));
                            (Vec::new(), None)
                        }
                    }
                };
                let (smodel_lighting_samples, light_grid) = smodel_lighting_samples;
                let min = draw.stats.min;
                let max = draw.stats.max;
                LoadedWorld {
                    scripts,
                    sound: map_sound,
                    materials: map_materials,
                    world: PreparedWorld {
                        draw: Some(draw),
                        dynamic_light: None,
                        static_model_meshes,
                        static_model_instances,
                        map_xmodel_scene_assets,
                        script_model_instances,
                        script_brush_models,
                        flag_descriptors,
                        script_structs,
                        dyn_ents: crate::DynEntCatalog::default(),
                        smodel_lighting_samples,
                        light_grid,
                        fx: sink.fx,
                        fx_models: Default::default(),

                        fx_glass: None,
                        impact_fx: sink.impact_fx.take_table(),
                        reflection_probe_images,
                        intermission_view,
                        exp_fog,
                        film_vision: None,
                        film_visions: Default::default(),
                        createart_name,
                        min,
                        max,

                        world_bounds: None,
                        policy: WorldDrawPolicy::t5(),
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    facts: crate::MapFacts {
                        minimap_corners,
                        north_yaw,
                        compass,
                        t5_teamset: t5_teamset.clone(),
                        script_sound: script_sound.clone(),
                        ..Default::default()
                    },
                    report,
                    ..Default::default()
                }
            }
            Err(e) => {
                report.push(format!("T5 world mesh: {e}"));
                let dm_spawns = dm_spawn_points_t5(&stream);
                let mut loaded = LoadedWorld {
                    scripts,
                    sound: map_sound,
                    world: PreparedWorld {
                        policy: WorldDrawPolicy::t5(),
                        exp_fog,
                        createart_name,
                        ..Default::default()
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    facts: crate::MapFacts {
                        t5_teamset: t5_teamset.clone(),
                        script_sound: script_sound.clone(),
                        ..Default::default()
                    },
                    report,
                    ..Default::default()
                };
                loaded.push_gap(
                    PreparedCapability::PreparedWorld,
                    format!("T5 world mesh: {e}"),
                    Some("assets::lane::t5::load_world/world_mesh"),
                );
                loaded
            }
        }
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        decode_color_maps: bool,
        material_seed: crate::MaterialCatalog,
    ) -> CommonCensus {
        let header = match image.t5_header() {
            Ok(header) => header,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: T5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: T5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = CommonWalkSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::T5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T5,
            "common census",
        );
        let walk = fastfile_t5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = vec![format!("common_mp (T5): walked {} assets", sink.walked)];
        if let Err(error) = walk {
            report.push(format!(
                "common_mp T5 walk: stopped after {} assets — {error}",
                sink.walked
            ));
        }
        report.push(format!(
            "common_mp T5 pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.push(format!(
            "s2 t5 walk: reuse_mat={} reuse_img={} pool_mat={} pool_img={}",
            sink.materials.link_reused_materials,
            sink.materials.link_reused_images,
            sink.materials.materials.len(),
            sink.materials.images.len(),
        ));
        let captured = sink.weapons.len();

        sink.weapons.resolve_reticles(&sink.materials);
        sink.weapons
            .resolve_combat_fx(&sink.fx, &crate::TracerCatalog::default());
        let leftover_fx = sink.fx.len();
        let leftover_fx_gaps = sink.fx.capture_gaps;
        for key in sink.weapons.rocket_model_hints() {
            if let Some(entry) = sink.projectile_meshes.get(key.namespace, &key.name) {
                sink.fpv_meshes
                    .insert_in(key.namespace, entry.skel.clone(), Some(&sink.materials));
            }
        }
        let projectile_keys = sink.weapons.projectile_model_hints();
        sink.projectile_meshes.keep_referenced(&projectile_keys);
        let mut weapons = sink.weapons.into_build();
        weapons.stamp_namespace(crate::AssetNamespace::T5);
        weapons.apply_stats_tables(sink.stats_tables.values());
        weapons.resolve_sz_xanim_edges(&sink.xanims);
        weapons.resolve_fpv_mesh_edges(&sink.fpv_meshes);
        weapons.resolve_world_model_edges(&sink.world_weapons);
        report.push(format!(
            "common_mp statsTable: tables={} item_groups={}",
            sink.stats_tables.len(),
            weapons.item_group_count(),
        ));
        let gun_named = weapons.gun_xmodel_count();
        report.push(format!(
        "common_mp weapons: {captured} captures → {} unique catalog ids (sorted; not retail bg_weaponIndex); {gun_named} with gunXModel[0]",
        weapons.len()
    ));
        report.push(format!(
            "common_mp leftover t5 fx: {leftover_fx} captured ({leftover_fx_gaps} gaps)"
        ));
        report.push(format!(
        "common_mp FPV mesh catalog: {} bind-pose viewmodel_* meshes retained ({} with tag_view)",
        sink.fpv_meshes.len(),
        sink.fpv_meshes.tag_view_count()
    ));
        report.push(format!(
            "common_mp T5 xanims: {} captured ({} gaps)",
            sink.xanims.len(),
            sink.xanims.capture_gaps
        ));

        let mut materials = sink.materials;

        let mut pending_images = None;
        if decode_color_maps {
            let stage = progress.begin_scoped(StageId::Images, "common_mp", None);
            let (inline, plan) = crate::material_images::plan_material_color_maps(
                path,
                &mut materials,
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp T5 IWD color maps: {} claimed for the merged pool, {} in-zone bodies decoded here ({} missing, {} unsupported)",
                plan.len(),
                inline.decoded,
                inline.missing,
                inline.unsupported
            ));
            // The plan itself is decoded later, under its own stage. This one
            // planned and decoded the in-zone bodies, and it finished; letting
            // the handle drop would record it as interrupted.
            stage.done();
            pending_images = Some(plan);
        }
        report.push(format!(
            "common_mp T5 teamset scripts: {} (_teamset_*.gsc icons)",
            sink.teamsets.len()
        ));
        CommonCensus {
            pending_images,
            weapons,
            cac_tables: sink.stats_tables.into_values().collect(),
            material_population: materials,
            fpv: sink.fpv_meshes,
            projectile_meshes: sink.projectile_meshes,
            world_weapons: sink.world_weapons,
            xanims: sink.xanims,
            fx: sink.fx,
            impact_fx: sink.impact_fx.take_table(),
            fx_models: sink.fx_models,
            report,
            teamsets: sink.teamsets,
            scripts: sink.scripts,
            film_visions: std::collections::BTreeMap::new(),
            ..Default::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: crate::MaterialCatalog,
    ) -> MaterialPopulation {
        let _ = progress;
        let zone_name = path.file_stem().map_or_else(
            || "startup".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let header = match image.t5_header() {
            Ok(header) => header,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: T5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: T5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = MaterialPopulationSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::T5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T5,
            "material population",
        );
        let walk = fastfile_t5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = Vec::new();
        if let Err(error) = walk {
            report.push(format!(
                "startup materials: T5 walk stopped after {} assets — {error}",
                sink.walked
            ));
        }
        let memory = sink.materials.image_memory();
        report.push(format!(
            "startup material walk: zone={zone_name} assets={} materials={} images={} decoded={} fpv=0 weapons=0 (materials-only sink; not load_common_mp)",
            sink.walked,
            sink.materials.materials.len(),
            memory.images,
            memory.decoded_images,
        ));
        MaterialPopulation {
            walked: sink.walked,
            light_defs: Vec::new(),
            materials: sink.materials,
            report,
            cac_tables: sink.stats_tables.into_values().collect(),
            scripts: sink.scripts,
        }
    }
}
