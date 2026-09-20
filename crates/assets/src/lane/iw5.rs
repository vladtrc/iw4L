use std::path::Path;

use super::{
    CommonCensus, CommonWalkSink, LaneGap, LoadedWorld, MaterialPopulation, MaterialPopulationSink,
    ZoneLane, ZoneWalkSink,
};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use crate::progress::LoadProgress;
use crate::session_load::{PreparedWorld, WorldDrawPolicy};
use crate::{
    BodyMeshBuild, Iw5ZoneMemory, MASK_PLAYER_SOLID, OwnedLightGrid, XAnimBuild, ZoneGame,
    ZoneImage, attach_iw5_static_models, build_iw5_clip_collision, build_iw5_world_draw,
    decode_reflection_probe_cubemap, dm_spawn_points_iw5, intermission_view_iw5,
    minimap_corners_iw5,
};

pub struct Iw5Lane;

impl Iw5Lane {
    pub const GAME: ZoneGame = ZoneGame::Iw5;
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
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];

    fn stamp_map_tree_team_icons(path: &Path, loaded: &mut LoadedWorld) {
        if loaded.facts.team_icons.allies.is_some() || loaded.facts.team_icons.axis.is_some() {
            return;
        }
        match crate::find_zone_for_tree(path, "code_post_gfx_mp") {
            Ok(found) => {
                let (arena, table) = crate::load_iw5_team_icon_sources(&found.path);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(table) = table.as_ref() {
                    loaded.facts.team_icons =
                        crate::team_icons_for_zone(table, arena.as_deref(), stem);
                }
                match (
                    loaded.facts.team_icons.allies.as_deref(),
                    loaded.facts.team_icons.axis.as_deref(),
                ) {
                    (Some(a), Some(x)) => loaded.report.push(format!(
                        "team icons: iw5 arena allies={a} axis={x} zone={stem}"
                    )),
                    _ => loaded.report.push(format!(
                        "team icons gap: IW5 code_post_gfx_mp arena/table missed zone={stem}"
                    )),
                }
            }
            Err(error) => loaded.report.push(format!(
                "team icons gap: IW5 same-tree code_post_gfx_mp: {error}"
            )),
        }
    }

    fn finish_loaded(path: &Path, mut loaded: LoadedWorld) -> LoadedWorld {
        Self::stamp_map_tree_team_icons(path, &mut loaded);
        loaded
    }
}

impl ZoneLane for Iw5Lane {
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
        let mut report = vec![format!("game: IW5 ({})", path.display())];
        let stage = progress.stage("reading IW5 zone header");
        let header = match image.iw5_header() {
            Ok(h) => h,
            Err(e) => {
                return Self::finish_loaded(
                    path,
                    LoadedWorld::with_gap(
                        WorldDrawPolicy::iw5(),
                        PreparedCapability::PreparedWorld,
                        format!("IW5 zone header: {e}"),
                        Some("assets::lane::iw5::load_world/zone_header"),
                    ),
                );
            }
        };

        drop(stage);
        let stage = progress.stage("preparing IW5 zone memory");
        report.push(crate::zone::xfile_arena_row(
            "zone arenas map",
            &header.block_size,
            fastfile_iw5::XFILE_BLOCK_TEMP,
            fastfile_iw5::XFILE_BLOCK_VIRTUAL,
        ));
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(s) => s,
            Err(e) => {
                return Self::finish_loaded(
                    path,
                    LoadedWorld::with_gap(
                        WorldDrawPolicy::iw5(),
                        PreparedCapability::PreparedWorld,
                        format!("IW5 zone arenas: {e}"),
                        Some("assets::lane::iw5::load_world/zone_arenas"),
                    ),
                );
            }
        };

        drop(stage);
        let stage = progress.stage("walking IW5 map assets");
        let mut sink = ZoneWalkSink::default();
        let seeded_techsets = material_seed.technique_set_facts().to_vec();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::Iw5);
        match fastfile_iw5::load_zone(&mut stream, &mut sink) {
            Ok(_) => report.push(format!("zone walk: complete, {} assets", sink.walked)),
            Err(e) => report.push(format!(
                "zone walk: stopped after {} assets — {e}",
                sink.walked
            )),
        }
        let exp_fog = sink.exp_fog.take();
        let createart_name = sink.createart_name.take();
        report.push(match &exp_fog {
            Some(fog) => format!(
                "createart fog: READY start={:.3} half={:.3} maxOpacity={:.3} sun={} name={}",
                fog.start_dist,
                fog.halfway_dist,
                fog.max_opacity,
                fog.sun.is_some(),
                createart_name.as_deref().unwrap_or("?")
            ),
            None => match createart_name.as_deref() {
                Some(name) => {
                    format!("createart fog: RED {name} walked but setExpFog did not parse")
                }
                None => "createart fog: RED missing maps/createart/<map>_art|_fog setExpFog".into(),
            },
        });
        report.push(format!(
            "serialization: {} ({} B pointers)",
            stream.wire_format().name(),
            stream.pointer_bytes()
        ));
        report.push(format!(
            "pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.push(format!(
            "materials: {} authored ({} images, {} capture gaps)",
            sink.materials.materials.len(),
            sink.materials.images.len(),
            sink.materials.capture_gaps
        ));
        let compass = std::mem::take(&mut sink.compass).resolve(&sink.materials);
        report.push(format!("compass: {:?}", compass));
        let mut materials = sink.materials;
        let absorbed = materials.absorb_technique_set_tables(&seeded_techsets);
        let promoted = materials.promote_iw5_fallback_tables();
        let stub_routed = materials.reroute_stub_materials();
        report.push(format!(
            "material route (pre-draw): absorbed_techsets={absorbed} iw5_promoted={promoted} stub_routed={stub_routed} \
         unrouted={}",
            materials.unrouted_material_count()
        ));
        let map_xmodels = std::mem::take(&mut sink.map_xmodels);
        let bodies = std::mem::take(&mut sink.bodies);
        let fpv_meshes = std::mem::take(&mut sink.fpv_meshes);
        let map_xanims = std::mem::take(&mut sink.xanims);

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
            match build_iw5_clip_collision(&stream, geometry) {
                Ok(mut clip) => {
                    attach_iw5_static_models(&stream, geometry, &sink.xmodel_coll, &mut clip);
                    let solid = clip
                        .brushes
                        .iter()
                        .filter(|b| b.contents & MASK_PLAYER_SOLID != 0)
                        .count();
                    report.push(format!(
                        "clip brushes extracted: {} ({} player-solid by contents mask)",
                        clip.brushes.len(),
                        solid
                    ));
                    report.push(format!(
                        "clip mesh extracted: verts={} tris={} nodes={} leaves={} aabb={} cmodels={} smodels={}",
                        clip.mesh.verts.len(),
                        clip.mesh.tri_indices.len() / 3,
                        clip.nodes.len(),
                        clip.leaves.len(),
                        clip.mesh.aabb_trees.len(),
                        clip.cmodels.len(),
                        clip.static_models.len()
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

        drop(stage);
        let _stage = progress.stage("building IW5 world geometry");
        let Some(geometry) = stream.gfx_world() else {
            report.push("no GfxWorld retained — nothing to draw".into());
            report.push("bodies: empty (no GfxWorld; XModel bone capture not reached)".into());
            let dm_spawns = dm_spawn_points_iw5(&stream);
            return Self::finish_loaded(
                path,
                LoadedWorld {
                    world: PreparedWorld {
                        exp_fog,
                        createart_name,
                        policy: WorldDrawPolicy::iw5(),
                        ..Default::default()
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies: BodyMeshBuild::default(),
                    fpv_meshes,
                    xanims: XAnimBuild::default(),
                    report,
                    gaps: vec![LaneGap {
                        capability: PreparedCapability::PreparedWorld,
                        reason: "no GfxWorld retained — nothing to draw".into(),
                        addr: Some("assets::lane::iw5::load_world/no_gfx_world"),
                    }],
                    ..Default::default()
                },
            );
        };
        report.push(format!(
        "gfx retained: verts={} indices={} surfaces={} lightmaps={} cells={} smodels={} sun_lights={}",
        geometry.vertex_count,
        geometry.index_count,
        geometry.surface_count,
        geometry.lightmap_count,
        geometry.cell_count,
        geometry.smodel_count,
        geometry.sun_primary_light_count
    ));
        if let Some(com) = stream.com_world() {
            report.push(format!(
                "com_world: {} primary lights retained",
                com.primary_light_count
            ));
        } else {
            report.push("com_world: not retained".into());
        }

        match build_iw5_world_draw(&stream, geometry, materials) {
            Ok((draw, map_materials)) => {
                let map_models = super::build_iw5_static_model_draw(&stream, geometry, map_xmodels);
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
                    map_use_triggers,
                    flag_descriptors,
                    script_structs,
                    ..
                } = map_models;
                let intermission_view = intermission_view_iw5(&stream);
                let minimap_corners = minimap_corners_iw5(&stream);
                let north_yaw = crate::worldspawn_north_yaw_iw5(&stream);
                let dm_spawns = dm_spawn_points_iw5(&stream);
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
                    "world mesh: {} vertices, {} triangles, {} surfaces ({} skipped), {} batches, sky_surfs={} sky_start={}",
                    draw.stats.vertices,
                    draw.stats.triangles,
                    draw.stats.surfaces,
                    draw.stats.skipped_surfaces,
                    draw.batches.len(),
                    draw.stats.sky_surfaces,
                    draw.dpvs.sky_start_surfs.len()
                ));
                report.push(format!(
                    "materials in draw: {}; primary lights: {}; named_defs={}; recorded_light_defs={}",
                    map_materials.materials.len(),
                    draw.primary_lights.len(),
                    draw.primary_lights
                        .iter()
                        .filter(|light| light.def_name.is_some())
                        .count(),
                    draw.light_defs.len()
                ));
                report.push(match &draw.light_region_hulls {
                    None => "light_region_hulls: not retained".into(),
                    Some(lists) => {
                        let hulls: usize = lists.iter().map(Vec::len).sum();
                        format!("light_region_hulls: {} lights, {hulls} hulls", lists.len())
                    }
                });
                match &draw.lightmap {
                    Ok(pages) => {
                        let ok = pages.iter().filter(|p| p.is_some()).count();
                        let exact = pages
                            .iter()
                            .flatten()
                            .filter(|page| {
                                page.primary_image.is_some() && page.secondary_image.is_some()
                            })
                            .count();
                        report.push(format!(
                            "lightmaps: {ok}/{} pages decoded exact={exact}/{}",
                            pages.len(),
                            pages.len()
                        ));
                    }
                    Err(e) => report.push(format!("lightmaps: {e}")),
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
                let portal_count: usize = draw.dpvs.portals_per_cell.iter().map(|c| c.len()).sum();
                let aabb_nodes: usize = draw.dpvs.aabb_trees.iter().map(|t| t.len()).sum();
                report.push(format!(
                "dpvs: planes={} nodes={} cells={} portals={} aabb_nodes={} sorted_surfs={} lit_opaque=[{},{}) surfaces_bounds={} smodel_bounds={} cleared_boxes={}",
                draw.dpvs.planes.len(),
                draw.dpvs.nodes.len(),
                draw.dpvs.cell_count,
                portal_count,
                aabb_nodes,
                draw.dpvs.sorted_surf_index.len(),
                draw.dpvs.lit_opaque_begin,
                draw.dpvs.lit_opaque_end,
                draw.dpvs.surface_bounds.len(),
                draw.dpvs.smodel_bounds.len(),
                draw.dpvs.cleared_boxes
            ));
                let light_grid = OwnedLightGrid::from_iw5_stream(&stream, geometry.light_grid);
                match &light_grid {
                    Some(owned) => report.push(format!(
                        "light_grid: rows={} entries={} colors={} has_light_regions={} sun_primary={}",
                        owned.row_data_start.len() / 2,
                        owned.entries.len() / 4,
                        owned.color_count,
                        owned.has_light_regions,
                        owned.sun_primary_light_index,
                    )),
                    None => report.push(format!(
                        "light_grid: none (row_axis={} col_axis={} entries={} colors={})",
                        geometry.light_grid.row_axis,
                        geometry.light_grid.col_axis,
                        geometry.light_grid.entry_count,
                        geometry.light_grid.color_count,
                    )),
                }
                let smodel_lighting_samples = {
                    use crate::model_lighting::{
                        build_smodel_lighting_samples_with_sight, census_lit_fragment_tiles,
                        collect_iw5_smodel_lighting_origins,
                    };
                    use lighting_iw4::LIGHT_GRID_SIGHT_CONTENT_MASK;
                    match &light_grid {
                        Some(owned) => {
                            let origins: Vec<_> =
                                collect_iw5_smodel_lighting_origins(&stream, geometry)
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
                            tiles
                        }
                        None => {
                            report
                                .push("smodel lighting: none (no owned light-grid tables)".into());
                            Vec::new()
                        }
                    }
                };
                match &light_grid {
                    Some(grid) => report.push(format!(
                        "light grid: retained entries={} colors={} row_axis={} col_axis={}",
                        grid.entries.len() / 4,
                        grid.color_count,
                        grid.row_axis,
                        grid.col_axis
                    )),
                    None => report.push(
                        "light grid: none (missing tables or colAxis sandwich not 0..2)".into(),
                    ),
                }
                report.push("draw path: IW5 materials+lightmaps+lights+DPVS+static models".into());
                let body_lines = bodies.report_lines();
                if body_lines.is_empty() {
                    report.push(format!(
                        "bodies: 0 soldier XModels captured (decoder wired, {} map XModels walked)",
                        map_xmodel_scene_assets.len()
                    ));
                } else {
                    report.extend(body_lines);
                }
                report.push(format!("map xanims: {}", map_xanims.len()));
                Self::finish_loaded(
                    path,
                    LoadedWorld {
                        materials: map_materials,
                        world: PreparedWorld {
                            min: draw.stats.min,
                            max: draw.stats.max,
                            draw: Some(draw),
                            static_model_meshes,
                            static_model_instances,
                            map_xmodel_scene_assets,
                            script_model_instances,
                            script_brush_models,
                            map_use_triggers,
                            flag_descriptors,
                            script_structs,
                            intermission_view,
                            light_grid,
                            reflection_probe_images,
                            exp_fog,
                            createart_name,
                            policy: WorldDrawPolicy::iw5(),
                            smodel_lighting_samples,
                            ..Default::default()
                        },
                        collision: clip,
                        spawns: dm_spawns,
                        bodies,
                        fpv_meshes,
                        xanims: map_xanims,
                        facts: crate::MapFacts {
                            minimap_corners,
                            north_yaw,
                            compass,
                            ..Default::default()
                        },
                        report,
                        ..Default::default()
                    },
                )
            }
            Err(e) => {
                report.push(format!("world draw: {e}"));
                let dm_spawns = dm_spawn_points_iw5(&stream);
                Self::finish_loaded(
                    path,
                    LoadedWorld {
                        world: PreparedWorld {
                            exp_fog,
                            createart_name,
                            policy: WorldDrawPolicy::iw5(),
                            ..Default::default()
                        },
                        collision: clip,
                        spawns: dm_spawns,
                        bodies,
                        fpv_meshes,
                        xanims: map_xanims,
                        report,
                        gaps: vec![LaneGap {
                            capability: PreparedCapability::PreparedWorld,
                            reason: format!("world draw: {e}"),
                            addr: Some("assets::lane::iw5::load_world/world_mesh"),
                        }],
                        ..Default::default()
                    },
                )
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
        let header = match image.iw5_header() {
            Ok(header) => header,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: IW5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: IW5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = CommonWalkSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::Iw5);
        let walk = fastfile_iw5::load_zone(&mut stream, &mut sink);
        let mut report = vec![format!("common_mp (IW5): walked {} assets", sink.walked)];
        if let Err(error) = walk {
            report.push(format!(
                "common_mp IW5 walk: stopped after {} assets — {error}",
                sink.walked
            ));
        }
        report.push(format!(
            "common_mp IW5 pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        sink.weapons.resolve_reticles(&sink.materials);
        let captured = sink.weapons.len();
        let mut weapons = sink.weapons.into_build();
        weapons.stamp_namespace(crate::AssetNamespace::Iw5);
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
        "common_mp weapons: {captured} captures → {} unique catalog ids (sorted; not retail bg_weaponIndex); {gun_named} with gunXModel[0]; {} with szXAnims[IDLE]; {} with any szXAnims slot",
        weapons.len(),
        weapons.idle_anim_count(),
        weapons.sz_xanims_count()
    ));
        report.push(format!(
        "common_mp FPV mesh catalog: {} bind-pose viewmodel_* meshes retained ({} with tag_view)",
        sink.fpv_meshes.len(),
        sink.fpv_meshes.tag_view_count()
    ));

        report.push(format!(
            "common_mp XAnimParts: {} captured ({} gaps)",
            sink.xanims.len(),
            sink.xanims.capture_gaps
        ));
        report.push(format!(
            "common_mp IW5 materials: {} authored ({} techsets, {} images, {} capture gaps)",
            sink.materials.materials.len(),
            sink.materials.technique_set_facts().len(),
            sink.materials.images.len(),
            sink.materials.capture_gaps
        ));
        let light_defs = crate::capture_iw5_light_defs(&stream, &sink.materials);
        report.push(format!(
            "GfxLightDef common_mp: table={} bodies={} recorded={}",
            sink.light_def_table,
            sink.light_def_bodies,
            light_defs.len()
        ));
        let mut materials = sink.materials;

        let mut pending_images = None;
        if decode_color_maps {
            let stage = progress.stage("planning IW5 common_mp material images");
            let (inline, plan) = crate::material_images::plan_material_color_maps(
                path,
                &mut materials,
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp IW5 IWD color maps: {} claimed for the merged pool, {} in-zone bodies decoded here ({} missing, {} unsupported)",
                plan.len(),
                inline.decoded,
                inline.missing,
                inline.unsupported
            ));
            pending_images = Some(plan);
        }
        CommonCensus {
            pending_images,
            weapons,
            cac_tables: sink.stats_tables.into_values().collect(),
            material_population: materials,
            fpv: sink.fpv_meshes,
            world_weapons: sink.world_weapons,
            xanims: sink.xanims,
            light_defs,
            report,
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
        let header = match image.iw5_header() {
            Ok(header) => header,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: IW5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: IW5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = MaterialPopulationSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(crate::AssetNamespace::Iw5);
        let walk = fastfile_iw5::load_zone(&mut stream, &mut sink);
        let mut report = Vec::new();
        if let Err(error) = walk {
            report.push(format!(
                "startup materials: IW5 walk stopped after {} assets — {error}",
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
            materials: sink.materials,
            report,
        }
    }
}
