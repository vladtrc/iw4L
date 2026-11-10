use bevy::platform::collections::HashSet;

use bevy::prelude::*;
use dpvs_iw4::{
    AabbCullStats, AabbTreeCull, BspDrawSurfCensus, BspDrawSurfRun, CellClipPlanes, DpvsPlanes,
    DpvsVisData, GfxDrawSurf, VisBits, WalkScratch, add_aabb_tree_surfaces_in_frustum,
    add_bsp_draw_surfs_camera, add_sky_surfaces_dpvs, admit_cell_root_span, cell_caster_row_words,
    msb_get, or_caster_rows_for_visible, smodel_cull_dist_hides, words_for_bits,
};

use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::{
    PortalVisibility, WorldAtlasLayer, WorldDpvs, WorldDrawItem, WorldDrawItemKind, WorldDrawItems,
    WorldScene,
};

#[derive(Resource, Clone, Debug, Default)]
pub struct DpvsFrameStats {
    pub eye_cell: Option<usize>,

    pub eye_cell_unresolved: u32,
    pub visible_cells: u32,

    pub portals_frustum_skipped: u32,

    pub child_planes_unported: u32,

    pub aabb_nodes_visited: u32,

    pub aabb_planes_dropped: u32,

    pub surfaces_rejected_by_bounds: u32,

    pub surfaces_admitted_unbounded: u32,
    pub smodels_rejected_by_bounds: u32,
    pub smodels_admitted_unbounded: u32,

    pub bsp_visible_surfaces: u32,

    pub bsp_admitted_surfaces: u32,
    pub bsp_run_n: u32,

    pub bsp_input_gap_n: u32,

    pub bsp_ranges_unavailable: u32,
    pub keys: u32,

    pub material_ordinal_refused: u32,
    pub rebinds: u32,
    pub surfaces: u32,
    pub triangles: u32,
    pub lightmapped_triangles: u32,
    pub fallback_triangles: u32,
    pub unculled_triangles: u32,
    pub single_cell: bool,
    pub aspect: f32,
    pub fov_deg: f32,

    pub cell_static_started: Option<std::time::Instant>,

    pub submitted_batches: u32,

    pub rewritten_index_bytes: u64,

    pub visibility_changes: u32,

    pub sky_surf_n: u32,

    pub sky_mesh_n: u32,

    pub sky_vis_n: u32,

    pub sky_drawn_n: u32,

    pub sky_admitted: u32,

    pub view_prepared: u8,

    pub lock_pvs: u8,

    pub smodel_vis: Vec<u8>,

    pub smodel_vis_id: u64,

    pub g0_world_surfs: Vec<u16>,

    pub frustum_planes: Vec<[f32; 4]>,

    pub cell_vis: Vec<u32>,
    pub cell_vis_count: usize,

    pub cell_clips: Vec<CellClipPlanes>,

    pub cell_clip_visits: Vec<(u32, CellClipPlanes)>,

    pub cell_vis_all: bool,
}

const DRAW_DECALS: bool = true;

fn single_cell_from_env() -> bool {
    matches!(
        std::env::var("IW4L_SINGLE_CELL").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

#[derive(Component)]
pub struct WorldMeshEntity;

#[derive(Component)]
#[require(bevy::prelude::DespawnOnExit<frame::MatchScope> = bevy::prelude::DespawnOnExit(frame::MatchScope::Live))]
pub struct StaticModelEntity;

#[derive(Component)]
#[require(bevy::prelude::DespawnOnExit<frame::MatchScope> = bevy::prelude::DespawnOnExit(frame::MatchScope::Live))]
pub struct ScriptModelEntity;

pub use render_scene::DynEntModelEntity;

#[derive(Component)]
pub struct ScriptModelGameObject(pub String);

/// The eye as each world of a projectile_flight atlas sees it: carried through the
/// doors into the world whose slab holds the model. Outside an atlas every
/// model sees the eye itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorldEyes {
    eye: Option<Vec3>,
    by_world: Vec<Vec<Vec3>>,
}

impl WorldEyes {
    pub(crate) fn new(scene: Option<&WorldScene>, eye: Option<Vec3>) -> Self {
        let by_world = match (scene, eye) {
            (Some(scene), Some(eye)) if !scene.portal_pairs.is_empty() => {
                let Some(view) = scene.portal_view.as_ref() else {
                    return Self {
                        eye: Some(eye),
                        by_world: Vec::new(),
                    };
                };
                (0..scene.portal_worlds)
                    .map(|world| {
                        view.draw_views(world)
                            .into_iter()
                            .filter_map(|tag| {
                                view.eye_in(usize::from(tag - 1), eye.to_array())
                                    .map(Vec3::from_array)
                            })
                            .collect()
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        Self { eye, by_world }
    }

    /// The eye a model at `origin` is measured from; `None` when no door
    /// leads to its world.
    pub(crate) fn at(&self, origin: [f32; 3]) -> Option<Vec3> {
        if self.by_world.is_empty() {
            return self.eye;
        }
        let world = asset_core::atlas::WorldLayout.world_at(origin).0;
        self.by_world.get(world)?.iter().copied().min_by(|a, b| {
            a.distance_squared(Vec3::from_array(origin))
                .total_cmp(&b.distance_squared(Vec3::from_array(origin)))
        })
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct StaticModelCullView<'w> {
    prepared: Res<'w, PreparedSceneView>,
    lock_pvs: Res<'w, crate::prepare::scene::view_parms::RLockPvs>,
    lod_ramp: Res<'w, crate::prepare::scene::smodel_geom_cache::LodRampDvar>,
}

impl StaticModelCullView<'_> {
    pub(crate) fn eyes(&self, scene: &WorldScene) -> WorldEyes {
        WorldEyes::new(
            Some(scene),
            self.prepared
                .ready
                .then_some(self.lock_pvs.dpvs_eye(&self.prepared)),
        )
    }

    pub(crate) fn distance_skips(
        &self,
        distances: &[u16],
        slot: usize,
        origin: [f32; 3],
        eyes: &WorldEyes,
    ) -> bool {
        smodel_cull_dist_skips_slot(
            distances,
            slot,
            origin,
            eyes.at(origin),
            self.lod_ramp.scale_last,
        )
    }

    pub(crate) fn lod_args(&self) -> crate::assemble::drawsurf::tess::smodel::LodRampArgs {
        self.lod_ramp.args()
    }
}

pub(crate) fn smodel_cull_dist_skips_slot(
    cull_dists: &[u16],
    slot: usize,
    origin: [f32; 3],
    eye: Option<Vec3>,
    scale_last: Option<f32>,
) -> bool {
    let Some(eye) = eye else {
        return false;
    };
    let cull_dist = cull_dists.get(slot).copied().unwrap_or(0);
    let dx = eye.x - origin[0];
    let dy = eye.y - origin[1];
    let dz = eye.z - origin[2];
    smodel_cull_dist_hides(cull_dist, dx * dx + dy * dy + dz * dz, scale_last)
}

#[derive(Default)]
pub struct PortalCullScratch {
    walk: Box<WalkScratch>,
    words: Vec<u32>,
}

struct PortalCellVisibility {
    depth: u8,
    max_range: Option<f32>,
    planes: Vec<[f32; 4]>,
    eye: [f32; 3],
    seed: [f32; 3],
    cell: Option<usize>,
    visible_cells: Vec<usize>,
    clips: Vec<Vec<CellClipPlanes>>,
}

struct PortalCullFrame<'a> {
    scratch: &'a mut PortalCullScratch,
    cells: Vec<Option<PortalCellVisibility>>,
}

fn limit_portal_range(depth: u8, eye: Vec3, forward: Vec3, planes: &mut [[f32; 4]]) -> Option<f32> {
    if depth < 2 || planes.len() < 2 {
        return None;
    }
    let forward = forward.try_normalize()?;
    let range = 1500.0 / f32::from(depth);
    let far = Vec4::from_array(planes[1]);
    let denominator = -far.truncate().dot(forward);
    let distance = far.dot(eye.extend(1.0));
    if denominator > 0.0 && distance / denominator <= range {
        return Some(distance / denominator);
    }
    planes[1] = (-forward).extend(forward.dot(eye) + range).to_array();
    Some(range)
}

fn walk_portal_cells(
    dpvs: &WorldDpvs,
    view: &render_frame::PortalView,
    index: usize,
    pvs: &render_scene::LockPvsView,
    scratch: &mut PortalCullScratch,
) -> Option<PortalCellVisibility> {
    let eye = pvs.eye.to_array();
    let forward = pvs.forward;
    let frustum = pvs.frustum_planes.as_slice();
    let matrix = view.moves.get(index).copied().flatten()?;
    let mut planes = view.clip_planes_in(index, eye, frustum)?;
    let virtual_eye = view.eye_in(index, eye)?;
    let depth = view.view_depth(index)?;
    let direction = matrix
        .inverse()
        .transform_vector3(forward.as_dvec3())
        .as_vec3();
    let max_range =
        limit_portal_range(depth, Vec3::from_array(virtual_eye), direction, &mut planes);
    let seed = if view
        .plan
        .as_ref()
        .is_some_and(|p| p.settings.only.is_some())
    {
        virtual_eye
    } else {
        view.seed_in(index, eye)?
    };
    let native = DpvsPlanes {
        planes: &dpvs.planes,
        nodes: &dpvs.nodes,
        cell_count: dpvs.cell_count as u32,
    };
    let cell = native
        .cell_for_point(seed)
        .or_else(|| native.cell_for_point(virtual_eye));
    scratch
        .words
        .resize(words_for_bits(dpvs.cell_count.max(1)), 0);
    scratch.words.fill(0);
    let mut bits = VisBits::new(&mut scratch.words);
    let mut clips = vec![Vec::new(); dpvs.cell_count];
    if let Some(cell) = cell {
        scratch.walk.visit_cells(
            dpvs,
            dpvs_iw4::PortalWalkView {
                camera_cell: cell,
                eye: virtual_eye,
                view_plane: dpvs_iw4::dpvs_view_plane(direction.to_array(), virtual_eye),
                clip_planes: &planes,
                bevels: None,
            },
            0,
            &mut bits,
            None,
            Some(&mut |cell: usize, planes: &CellClipPlanes| clips[cell].push(*planes)),
        );
    } else {
        for cell in 0..dpvs.cell_count {
            bits.set(cell);
        }
    }
    Some(PortalCellVisibility {
        depth,
        max_range,
        planes,
        eye: virtual_eye,
        seed,
        cell,
        visible_cells: (0..dpvs.cell_count)
            .filter(|&cell| bits.get(cell))
            .collect(),
        clips,
    })
}

impl PortalCullFrame<'_> {
    fn admit_aperture(
        &mut self,
        host: &WorldDpvs,
        layers: &[WorldAtlasLayer],
        view: &render_frame::PortalView,
        opening: usize,
        pvs: &render_scene::LockPvsView,
    ) -> bool {
        if view.shared {
            // Native room connectivity cannot occlude an overlapping map.
            return true;
        }
        let cache = &mut self.cells;
        let scratch = &mut *self.scratch;
        let aperture = view.openings[opening];
        let parent = aperture.worlds[0];
        let world = view.worlds[parent];
        let dpvs = if world == 0 {
            host
        } else if let Some(layer) = layers.iter().find(|layer| layer.world == world) {
            &layer.dpvs
        } else {
            return true;
        };
        if cache.len() < view.worlds.len() {
            cache.resize_with(view.worlds.len(), || None);
        }
        if cache[parent].is_none() {
            cache[parent] = walk_portal_cells(dpvs, view, parent, pvs, scratch);
        }
        let Some(cells) = cache[parent].as_ref() else {
            return true;
        };
        let inverse = view.moves[parent].unwrap().inverse();
        let base = Vec3::from_array(aperture.base).as_dvec3();
        let across = Vec3::from_array(aperture.across).as_dvec3() * f64::from(aperture.width) * 0.5;
        let up = Vec3::from_array(aperture.up).as_dvec3() * f64::from(aperture.height);
        let winding = [
            base - across,
            base + across,
            base + across + up,
            base - across + up,
        ]
        .map(|p| inverse.transform_point3(p).as_vec3().to_array());
        // The aperture volume crosses the camera near plane during traversal.
        // Test its physical winding without that plane; retain ancestor exit planes.
        let aperture_planes = cells.planes.get(1..).unwrap_or(&[]);
        if !dpvs_iw4::winding_survives_planes(&winding, aperture_planes) {
            return false;
        }
        if cells.cell.is_none() {
            return true;
        }
        DpvsPlanes {
            planes: &dpvs.planes,
            nodes: &dpvs.nodes,
            cell_count: dpvs.cell_count as u32,
        }
        .winding_reaches_visible_cell(&winding, &mut |cell, fragment| {
            cells.clips.get(cell).is_some_and(|clips| {
                clips.iter().any(|clip| {
                    let planes = clip.as_slice();
                    let planes = if clip.frustum_plane_count > 0 {
                        planes.get(1..).unwrap_or(&[])
                    } else {
                        planes
                    };
                    dpvs_iw4::winding_survives_planes(fragment, planes)
                })
            })
        })
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
pub fn apply_dpvs_cull(
    mut scene: ResMut<WorldScene>,
    portal_controls: Res<render_scene::PortalRenderControls>,
    time: Res<Time<Real>>,
    active: Option<Res<net::GameActive>>,
    time_strategy: Res<bevy::time::TimeUpdateStrategy>,
    mut budget: Local<render_frame::portal_budget::PortalBudget>,
    presented: Option<Res<net::PresentedSnapshot>>,
    mut scratch: Local<PortalCullScratch>,
    occlusion: Local<render_frame::portal_occlusion::PortalOcclusionHistory>,
    mut stats: ResMut<DpvsFrameStats>,
    mut published_vis: ResMut<render_scene::PublishedCellVis>,
    prepared: Res<PreparedSceneView>,
    lock_pvs: Res<crate::prepare::scene::view_parms::RLockPvs>,
) {
    let _cull = perf::span!("cull", "iw4l.render");
    // Emit zeroes too: a frame without portals must not retain the previous
    // frame's plan counts in a trace viewer.
    let mut portal_counts = [0u32; 5];
    stats.frustum_planes.clear();
    stats.smodel_vis.clear();
    stats.smodel_vis_id = 0;
    stats.cell_vis.clear();
    stats.cell_vis_count = 0;
    stats.cell_clips.clear();
    stats.cell_clip_visits.clear();
    stats.cell_vis_all = false;
    stats.view_prepared = 0;
    stats.lock_pvs = 0;
    *published_vis = render_scene::PublishedCellVis::default();
    scene.portal_visibility.clear();
    scene.portal_view = None;
    let shared = matches!(scene.space, asset_core::world_frame::SceneSpace::Shared(_));
    let portal_space = scene.space.clone();
    let portaled = !scene.portal_pairs.is_empty();
    let generation = scene.products_id;
    let portal_description = portaled.then(|| {
        (
            scene.portal_pairs.clone(),
            scene.portal_worlds,
            scene.portal_surfaces.clone(),
        )
    });
    let mut portal_visibility = Vec::new();
    let Some(cull) = scene.cull.as_mut() else {
        return;
    };

    if cull.batch_count == 0 && cull.static_model_entities.is_empty() {
        return;
    }
    if !prepared.ready {
        return;
    }
    stats.view_prepared = 1;
    stats.lock_pvs = u8::from(lock_pvs.enabled && lock_pvs.frozen.is_some());

    let eye_pos = lock_pvs.dpvs_eye(&prepared);
    let eye = [eye_pos.x, eye_pos.y, eye_pos.z];
    let frustum = lock_pvs.dpvs_frustum(&prepared).to_vec();
    let forward = lock_pvs.dpvs_forward(&prepared);
    let (fov, aspect) = lock_pvs.dpvs_fov_aspect(&prepared);
    let pvs = render_scene::LockPvsView {
        eye: eye_pos,
        forward,
        frustum_planes: frustum,
        portal_bevels: lock_pvs.dpvs_portal_bevels(&prepared),
        fov,
        aspect,
    };
    let effective_settings = budget.update(
        generation,
        time.delta_secs_f64(),
        matches!(*time_strategy, bevy::time::TimeUpdateStrategy::Automatic)
            && active.as_ref().is_some_and(|active| active.get()),
        portal_controls.0,
    );
    perf::counter!(
        "portal_budget_views",
        "iw4l.render",
        effective_settings.max_views as f64
    );
    perf::counter!(
        "portal_budget_depth",
        "iw4l.render",
        f64::from(effective_settings.depth)
    );
    perf::counter!("portal_laggy", "iw4l.render", f64::from(budget.level()));
    let mut portal_cull = PortalCullFrame {
        scratch: &mut scratch,
        cells: Vec::new(),
    };
    let mut portal_view = portal_description.map(|(pairs, count, surfaces)| {
        render_frame::PortalView::new_render_with_blocked(
            render_frame::PortalAtlas {
                pairs: &pairs,
                world_count: count,
                surfaces,
                space: Some(&portal_space),
                camera_world: shared.then_some(prepared.physical_world),
            },
            render_frame::PortalCamera {
                eye,
                clip_from_world: prepared.clip_from_world,
                viewport: [
                    prepared.scene_viewport.x as f32,
                    prepared.scene_viewport.y as f32,
                    prepared.scene_viewport.width as f32,
                    prepared.scene_viewport.height as f32,
                ],
            },
            effective_settings,
            presented
                .as_ref()
                .and_then(|p| p.snapshot())
                .map_or(&[], |s| {
                    s.meta.world_objects.portal_blocked_exits.as_slice()
                }),
            Some(occlusion.begin(
                generation,
                eye,
                forward.to_array(),
                [fov, aspect],
                [
                    prepared.scene_viewport.x as f32,
                    prepared.scene_viewport.y as f32,
                    prepared.scene_viewport.width as f32,
                    prepared.scene_viewport.height as f32,
                ],
            )),
            &mut |view, opening| {
                portal_cull.admit_aperture(&cull.dpvs, &cull.atlas_layers, view, opening, &pvs)
            },
        )
    });
    perf::counter!(
        "portal_culled_query",
        "iw4l.render",
        portal_view
            .as_ref()
            .and_then(|v| v.plan.as_ref())
            .map_or(0.0, |p| f64::from(p.culled_query))
    );
    budget.previous_views = portal_view
        .as_ref()
        .and_then(|v| v.plan.as_ref())
        .map_or(1, |p| p.selected.iter().filter(|&&v| v).count());
    let mut portal_max_depth = 0u8;
    if let Some(plan) = portal_view.as_ref().and_then(|view| view.plan.as_ref()) {
        portal_counts[4] = plan.culled_dpvs;
        portal_counts[0] = plan.selected.iter().filter(|&&selected| selected).count() as u32;
        for aperture in &plan.apertures {
            match aperture.fallback {
                Some(render_frame::portal_plan::PortalFallback::Budget) => portal_counts[1] += 1,
                Some(render_frame::portal_plan::PortalFallback::Depth) => portal_counts[2] += 1,
                Some(render_frame::portal_plan::PortalFallback::Blocked) => portal_counts[3] += 1,
                Some(render_frame::portal_plan::PortalFallback::Occluded) => {}
                None => portal_max_depth = portal_max_depth.max(aperture.depth),
            }
        }
    }
    perf::counter!(
        "portal_max_depth",
        "iw4l.render",
        f64::from(portal_max_depth)
    );
    perf::counter!("portal_views", "iw4l.render", f64::from(portal_counts[0]));
    perf::counter!(
        "portal_fallback_budget",
        "iw4l.render",
        f64::from(portal_counts[1])
    );
    perf::counter!(
        "portal_fallback_depth",
        "iw4l.render",
        f64::from(portal_counts[2])
    );
    perf::counter!(
        "portal_fallback_blocked",
        "iw4l.render",
        f64::from(portal_counts[3])
    );
    perf::counter!(
        "portal_culled_dpvs",
        "iw4l.render",
        f64::from(portal_counts[4])
    );
    let dpvs = &cull.dpvs;
    if dpvs.cell_count == 0 && !shared {
        return;
    }

    stats.frustum_planes.clone_from(&pvs.frustum_planes);

    let camera_world = portal_view
        .as_ref()
        .map_or(if shared { prepared.physical_world } else { 0 }, |view| {
            view.camera_world
        });
    let camera_dpvs = if camera_world == 0 {
        dpvs
    } else {
        cull.atlas_layers
            .iter()
            .find(|layer| layer.world == camera_world)
            .map_or(dpvs, |layer| &layer.dpvs)
    };
    let cell_count = camera_dpvs.cell_count;
    let eye_cell = DpvsPlanes {
        planes: &camera_dpvs.planes,
        nodes: &camera_dpvs.nodes,
        cell_count: cell_count as u32,
    }
    .cell_for_point(eye);
    let single_cell = single_cell_from_env();
    let mut words = vec![
        0u32;
        words_for_bits(if shared {
            dpvs.cell_count.max(1)
        } else {
            cell_count.max(1)
        })
    ];
    let mut vis = VisBits::new(&mut words);

    let mut walk_stats = dpvs_iw4::WalkStats::default();
    if !portaled {
        match eye_cell {
            _ if shared => {
                for cell in 0..dpvs.cell_count {
                    vis.set(cell);
                }
            }
            Some(cell) if single_cell => vis.set(cell),

            Some(cell) => {
                let mut cell_clips = vec![CellClipPlanes::EMPTY; dpvs.cell_count];
                let bevels = pvs.portal_bevels;
                walk_stats = portal_cull.scratch.walk.visit_cells(
                    dpvs,
                    dpvs_iw4::PortalWalkView {
                        camera_cell: cell,
                        eye,
                        view_plane: dpvs_iw4::dpvs_view_plane(
                            lock_pvs.dpvs_forward(&prepared).to_array(),
                            eye,
                        ),
                        clip_planes: &pvs.frustum_planes,
                        bevels: bevels.as_ref(),
                    },
                    0,
                    &mut vis,
                    Some(&mut cell_clips),
                    Some(&mut |visited: usize, planes: &CellClipPlanes| {
                        stats.cell_clip_visits.push((visited as u32, *planes));
                    }),
                );
                stats.cell_clips = cell_clips;
            }

            None => {
                for cell in 0..dpvs.cell_count {
                    vis.set(cell);
                }
            }
        }
    }

    let surf_count = cull.surface_batch_ranges.len();
    cull.surface_vis.clear();
    cull.surface_vis.resize(surf_count, 0);
    cull.smodel_vis.clear();
    cull.smodel_vis.resize(cull.static_model_entities.len(), 0);
    cull.draws.items.clear();
    cull.draws.g0_surfs.clear();
    stats.g0_world_surfs.clear();

    let mut aabb_stats = AabbCullStats::default();
    if !portaled {
        admit_visible_cells(
            dpvs,
            |cell| vis.get(cell),
            &pvs.frustum_planes,
            &mut DpvsVisData {
                surface_vis: &mut cull.surface_vis,
                smodel_vis: &mut cull.smodel_vis,
            },
            &mut aabb_stats,
        );
    }

    if shared {
        // Native BSP portal walks describe each map's enclosed rooms, not
        // visibility between overlapping maps. Cull all cells in one frustum.
        for layer in &cull.atlas_layers {
            let start = layer.surface_base;
            let end = start + layer.surface_count;
            if let (Some(surfaces), Some(smodels)) = (
                cull.surface_vis.get_mut(start..end),
                cull.smodel_vis.get_mut(layer.smodel_base..),
            ) {
                admit_visible_cells(
                    &layer.dpvs,
                    |_| true,
                    &pvs.frustum_planes,
                    &mut DpvsVisData {
                        surface_vis: surfaces,
                        smodel_vis: smodels,
                    },
                    &mut aabb_stats,
                );
            }
        }
    }

    let mut planes_without_far = pvs.frustum_planes.clone();
    if planes_without_far.len() >= 2 {
        planes_without_far.remove(1);
    }
    let mut sky_admit = dpvs_iw4::SkyCullStats::default();
    if !portaled {
        sky_admit = add_sky_surfaces_dpvs(
            &dpvs.sky_start_surfs,
            &dpvs.surface_bounds,
            &planes_without_far,
            &mut cull.surface_vis,
        );
    }
    if let Some(view) = portal_view.as_mut() {
        let (reports, root_cells) = portal_cull.admit_views(
            dpvs,
            &cull.atlas_layers,
            view,
            &pvs,
            &mut DpvsVisData {
                surface_vis: &mut cull.surface_vis,
                smodel_vis: &mut cull.smodel_vis,
            },
            &mut aabb_stats,
        );
        portal_visibility = reports;
        vis.clear();
        for cell in root_cells {
            vis.set(cell);
        }
    }

    stats.sky_surf_n = dpvs.sky_start_surfs.len() as u32;
    stats.sky_mesh_n = dpvs
        .sky_start_surfs
        .iter()
        .filter(|&&surf| {
            cull.surface_index_ranges
                .get(surf as usize)
                .is_some_and(|&(_, n)| n > 0)
        })
        .count() as u32;
    stats.sky_vis_n = dpvs
        .sky_start_surfs
        .iter()
        .filter(|&&surf| cull.surface_vis.get(surf as usize).copied().unwrap_or(0) != 0)
        .count() as u32;
    stats.sky_admitted = sky_admit.admitted;
    stats.material_ordinal_refused = 0;

    let mut bsp_census = BspDrawSurfCensus::default();
    for range in dpvs.camera_ranges.iter() {
        let range = cull.draws.append_camera_bsp_range(
            range,
            &mut cull.surface_vis,
            &cull.surface_draw_fields,
            &cull.capture.packed_draw_surfs,
            &cull.surface_materials,
        );
        bsp_census.range_n = bsp_census.range_n.saturating_add(range.range_n);
        bsp_census.visible_n = bsp_census.visible_n.saturating_add(range.visible_n);
        bsp_census.admitted_n = bsp_census.admitted_n.saturating_add(range.admitted_n);
        bsp_census.run_n = bsp_census.run_n.saturating_add(range.run_n);
        bsp_census.input_gap_n = bsp_census.input_gap_n.saturating_add(range.input_gap_n);
        bsp_census.output_overflow_n = bsp_census
            .output_overflow_n
            .saturating_add(range.output_overflow_n);
    }
    for layer in &cull.atlas_layers {
        let base = layer.surface_base as u32;
        for range in layer.dpvs.camera_ranges.iter() {
            let range = cull.draws.append_camera_bsp_range(
                asset_world::CameraSurfRange {
                    kind: range.kind,
                    begin: base + range.begin,
                    end: base + range.end,
                },
                &mut cull.surface_vis,
                &cull.surface_draw_fields,
                &cull.capture.packed_draw_surfs,
                &cull.surface_materials,
            );
            bsp_census.visible_n = bsp_census.visible_n.saturating_add(range.visible_n);
            bsp_census.admitted_n = bsp_census.admitted_n.saturating_add(range.admitted_n);
            bsp_census.run_n = bsp_census.run_n.saturating_add(range.run_n);
            bsp_census.input_gap_n = bsp_census.input_gap_n.saturating_add(range.input_gap_n);
        }
    }
    stats.bsp_visible_surfaces = bsp_census.visible_n;
    stats.bsp_admitted_surfaces = bsp_census.admitted_n;
    stats.bsp_run_n = bsp_census.run_n;
    stats.bsp_input_gap_n = bsp_census
        .input_gap_n
        .saturating_add(bsp_census.output_overflow_n);
    stats.material_ordinal_refused = stats
        .material_ordinal_refused
        .saturating_add(bsp_census.input_gap_n);

    let mut draw_items_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(
        &mut draw_items_id,
        cull.draws.items.len() as u64,
    );
    stats.sky_drawn_n = cull
        .draws
        .items
        .iter()
        .map(|item| {
            (0..item.run)
                .filter(|offset| {
                    let surf = item.surf.saturating_add(*offset);
                    dpvs.sky_start_surfs.iter().any(|&s| s == u32::from(surf))
                })
                .count() as u32
        })
        .sum();

    let batch_count = cull.batch_count as usize;
    let mut batch_visible = vec![false; batch_count];
    let mut surfaces = 0u32;
    let mut prev_mat: Option<u16> = None;
    let mut rebinds = 0u32;
    let mut lightmapped_tris = 0u32;
    let mut fallback_tris = 0u32;
    for item in &cull.draws.items {
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, item.key);
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.surf));
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.run));
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            match item.kind {
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitOpaque) => 0,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitTrans) => 1,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Emissive) => 2,
                WorldDrawItemKind::BModel => 3,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Decal) => 4,
            },
        );
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            u64::from(item.setup_key_changed),
        );
        let mat = ((item.key >> 30) & 0xfff) as u16;
        if prev_mat != Some(mat) {
            rebinds += 1;
            prev_mat = Some(mat);
        }
        for offset in 0..item.run {
            let surf_usize = usize::from(item.surf.saturating_add(offset));
            let Some(&(batch, _s0, sc)) = cull.surface_batch_ranges.get(surf_usize) else {
                continue;
            };
            if sc == 0 {
                continue;
            }
            surfaces = surfaces.saturating_add(1);
            if let Some(slot) = batch_visible.get_mut(batch) {
                *slot = true;
            }
            let tris = sc / 3;
            if cull.batch_lightmapped.get(batch).copied().unwrap_or(false) {
                lightmapped_tris = lightmapped_tris.saturating_add(tris);
            } else {
                fallback_tris = fallback_tris.saturating_add(tris);
            }
        }
    }
    let tris = lightmapped_tris.saturating_add(fallback_tris);
    stats.eye_cell = eye_cell;
    if eye_cell.is_none() {
        stats.eye_cell_unresolved = stats.eye_cell_unresolved.saturating_add(1);
    }
    stats.visible_cells = vis.count_ones();

    // Root-sized bitsets cannot index the cells of another world.
    let vis_all = !portaled && (shared || eye_cell.is_none());
    stats.cell_vis.clone_from(&words);
    stats.cell_vis_count = cell_count;
    stats.cell_vis_all = vis_all;
    cull.cell_vis_world = camera_world;
    cull.cell_vis.clone_from(&words);
    cull.cell_vis_all = vis_all;
    let mut views: Vec<_> = portal_visibility
        .iter()
        .map(|report| {
            let dpvs = if report.world == 0 {
                dpvs
            } else {
                cull.atlas_layers
                    .iter()
                    .find(|layer| layer.world == report.world)
                    .map_or(dpvs, |layer| &layer.dpvs)
            };
            let mut words = vec![0u32; words_for_bits(dpvs.cell_count.max(1))];
            for &cell in &report.visible_cells {
                if let Some(word) = words.get_mut(cell / 32) {
                    *word |= 1 << (cell % 32);
                }
            }
            render_scene::PublishedCellView {
                view: report.view,
                world: report.world,
                shared,
                eye: report.eye,
                words,
                cell_count: dpvs.cell_count,
                planes: report.planes.clone(),
                vis_all: report.cell.is_none(),
            }
        })
        .collect();
    if !portaled {
        views.push(render_scene::PublishedCellView {
            view: 0,
            world: camera_world,
            shared,
            eye,
            words: words.clone(),
            cell_count,
            planes: pvs.frustum_planes.clone(),
            vis_all,
        });
    }
    *published_vis = render_scene::PublishedCellVis {
        world: camera_world,
        views,
        words: words.clone(),
        cell_count,
        vis_all,
    };
    stats.portals_frustum_skipped = walk_stats.skip_clip;
    stats.child_planes_unported = walk_stats.child_planes_unported;
    stats.aabb_nodes_visited = aabb_stats.nodes_visited;
    stats.aabb_planes_dropped = aabb_stats.planes_dropped;
    stats.surfaces_rejected_by_bounds = aabb_stats.surfaces_rejected_by_bounds;
    stats.surfaces_admitted_unbounded = aabb_stats.surfaces_admitted_unbounded;
    stats.smodels_rejected_by_bounds = aabb_stats.smodels_rejected_by_bounds;
    stats.smodels_admitted_unbounded = aabb_stats.smodels_admitted_unbounded;
    stats.keys = cull.draws.items.len() as u32;
    cull.draws.content_id = draw_items_id;
    stats.g0_world_surfs.clone_from(&cull.draws.g0_surfs);
    stats.rebinds = rebinds;
    stats.surfaces = surfaces;
    stats.triangles = tris;
    stats.lightmapped_triangles = lightmapped_tris;
    stats.fallback_triangles = fallback_tris;
    stats.unculled_triangles = (cull.packed_indices.len() / 3) as u32;
    stats.single_cell = single_cell;
    stats.aspect = aspect;
    stats.fov_deg = fov.to_degrees();

    let submitted_batches = batch_visible.iter().filter(|&&v| v).count() as u32;
    perf::counter!("batches", "iw4l.render", f64::from(submitted_batches));

    stats.submitted_batches = submitted_batches;
    stats.rewritten_index_bytes = 0;
    stats.visibility_changes = 0;
    stats.smodel_vis.clear();
    stats.smodel_vis.extend_from_slice(&cull.smodel_vis);
    let mut smodel_vis_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_bytes(&mut smodel_vis_id, &cull.smodel_vis);
    stats.smodel_vis_id = smodel_vis_id;
    scene.portal_visibility = portal_visibility;
    scene.portal_view = portal_view;
}

/// Admits the surfaces and static models of `dpvs`'s visible cells that the
/// frustum keeps, through each cell's AABB tree.
fn admit_visible_cells(
    dpvs: &WorldDpvs,
    visible: impl Fn(usize) -> bool,
    frustum: &[[f32; 4]],
    vis_data: &mut DpvsVisData<'_>,
    aabb_stats: &mut AabbCullStats,
) {
    for cell in 0..dpvs.cell_count {
        if !visible(cell) {
            continue;
        }
        let Some(tree) = dpvs.aabb_trees.get(cell) else {
            continue;
        };
        if tree.is_empty() {
            if let Some(root) = dpvs.cell_roots.get(cell)
                && root.count > 0
            {
                admit_cell_root_span(
                    &dpvs.sorted_surf_index,
                    root.start,
                    root.count,
                    vis_data,
                    aabb_stats,
                );
            }
            continue;
        }
        let cull_input = AabbTreeCull {
            nodes: tree,
            smodel_indexes: dpvs
                .aabb_smodel_indices
                .get(cell)
                .map_or(&[][..], Vec::as_slice),
            sorted_surf_index: &dpvs.sorted_surf_index,
            surfaces_bounds: &dpvs.surface_bounds,
            smodel_bounds: &dpvs.smodel_bounds,
            draw_decals: DRAW_DECALS,
        };
        add_aabb_tree_surfaces_in_frustum(&cull_input, frustum, vis_data, aabb_stats);
    }
    admit_generated_surfaces(dpvs, visible, frustum, vis_data.surface_vis);
}

fn admit_generated_surfaces(
    dpvs: &WorldDpvs,
    visible: impl Fn(usize) -> bool,
    planes: &[[f32; 4]],
    out: &mut [u8],
) {
    for generated in &dpvs.generated_surfaces {
        if (dpvs.cell_count == 0 || generated.cells.iter().any(|&cell| visible(cell)))
            && dpvs
                .surface_bounds
                .get(generated.surface)
                .is_some_and(|&bounds| !dpvs_iw4::bounds_culled(bounds, planes))
            && let Some(slot) = out.get_mut(generated.surface)
        {
            *slot = 1;
        }
    }
}

/// Traverse each destination's native cell graph from its exit cell, using
/// the camera frustum transformed and narrowed by the ancestor door windings.
/// The output is a union of physical visibility; exact draws retain view tags.
impl PortalCullFrame<'_> {
    fn admit_views(
        &mut self,
        host: &WorldDpvs,
        layers: &[WorldAtlasLayer],
        view: &mut render_frame::PortalView,
        pvs: &render_scene::LockPvsView,
        vis_data: &mut DpvsVisData<'_>,
        aabb_stats: &mut AabbCullStats,
    ) -> (Vec<PortalVisibility>, Vec<usize>) {
        if view.shared {
            // Native room walks cannot establish visibility between overlapping maps.
            let mut reports = Vec::new();
            for index in 0..view.worlds.len() {
                if !view.is_selected(index) || view.moves[index].is_none() {
                    continue;
                }
                let Some(planes) =
                    view.clip_planes_in(index, pvs.eye.to_array(), &pvs.frustum_planes)
                else {
                    continue;
                };
                let mut surfaces = vec![0; vis_data.surface_vis.len()];
                let mut smodels = vec![0; vis_data.smodel_vis.len()];
                let mut admit = |dpvs: &WorldDpvs,
                                 surface_base: usize,
                                 surface_end: usize,
                                 smodel_base: usize| {
                    let mut local = DpvsVisData {
                        surface_vis: &mut surfaces[surface_base..surface_end],
                        smodel_vis: &mut smodels[smodel_base..],
                    };
                    admit_visible_cells(dpvs, |_| true, &planes, &mut local, aabb_stats);
                    let mut sky_planes = planes.clone();
                    if sky_planes.len() > 1 {
                        sky_planes.remove(1);
                    }
                    add_sky_surfaces_dpvs(
                        &dpvs.sky_start_surfs,
                        &dpvs.surface_bounds,
                        &sky_planes,
                        local.surface_vis,
                    );
                };
                admit(host, 0, vis_data.surface_vis.len(), 0);
                for layer in layers {
                    admit(
                        &layer.dpvs,
                        layer.surface_base,
                        layer.surface_base + layer.surface_count,
                        layer.smodel_base,
                    );
                }
                for (out, &value) in vis_data.surface_vis.iter_mut().zip(&surfaces) {
                    *out |= value;
                }
                for (out, &value) in vis_data.smodel_vis.iter_mut().zip(&smodels) {
                    *out |= value;
                }
                let eye = view
                    .eye_in(index, pvs.eye.to_array())
                    .unwrap_or(pvs.eye.to_array());
                reports.push(PortalVisibility {
                    view: index,
                    depth: view.view_depth(index).unwrap_or(0),
                    max_range: None,
                    world: view.worlds[index],
                    eye,
                    seed: eye,
                    cell: None,
                    planes,
                    visible_cells: (0..host.cell_count).collect(),
                    visible_surfaces: surfaces.iter().filter(|&&v| v != 0).count(),
                    visible_smodels: smodels.iter().filter(|&&v| v != 0).count(),
                });
                view.visibility[index] = Some(render_frame::portal::PortalDrawVisibility {
                    surface_base: 0,
                    surfaces: surfaces.into(),
                    smodel_base: 0,
                    smodels: smodels.into(),
                    bmodels: Default::default(),
                });
            }
            return (reports, (0..host.cell_count).collect());
        }
        let surface_vis = &mut *vis_data.surface_vis;
        let smodel_vis = &mut *vis_data.smodel_vis;
        let scratch = &mut *self.scratch;
        let cell_cache = self.cells.as_mut_slice();
        let mut reports = Vec::new();
        let mut root_cells = Vec::new();
        for index in 0..view.worlds.len() {
            if !view.is_selected(index) {
                continue;
            }
            let world = view.worlds[index];
            let (dpvs, surface_base, surface_end, smodel_base) = if world == 0 {
                (host, 0, surface_vis.len(), 0)
            } else {
                let Some(layer) = layers.iter().find(|layer| layer.world == world) else {
                    continue;
                };
                (
                    &layer.dpvs,
                    layer.surface_base,
                    layer.surface_base + layer.surface_count,
                    layer.smodel_base,
                )
            };
            let cells = cell_cache
                .get_mut(index)
                .and_then(Option::take)
                .or_else(|| walk_portal_cells(dpvs, view, index, pvs, scratch));
            let Some(PortalCellVisibility {
                depth,
                max_range,
                planes,
                eye: virtual_eye,
                seed,
                cell,
                visible_cells,
                ..
            }) = cells
            else {
                continue;
            };
            if index == view.camera_world {
                root_cells.extend_from_slice(&visible_cells);
            }
            let (Some(surfaces), Some(smodels)) = (
                surface_vis.get_mut(surface_base..surface_end),
                smodel_vis.get_mut(smodel_base..),
            ) else {
                continue;
            };
            let mut local_surface = vec![0; surfaces.len()];
            let mut local_smodel = vec![0; smodels.len()];
            admit_visible_cells(
                dpvs,
                |cell| visible_cells.binary_search(&cell).is_ok(),
                &planes,
                &mut DpvsVisData {
                    surface_vis: &mut local_surface,
                    smodel_vis: &mut local_smodel,
                },
                aabb_stats,
            );
            let mut sky_planes = planes.clone();
            if sky_planes.len() > 1 {
                sky_planes.remove(1);
            }
            add_sky_surfaces_dpvs(
                &dpvs.sky_start_surfs,
                &dpvs.surface_bounds,
                &sky_planes,
                &mut local_surface,
            );
            let visible_surfaces = local_surface.iter().filter(|&&v| v != 0).count();
            let visible_smodels = local_smodel.iter().filter(|&&v| v != 0).count();
            for (out, &visible) in surfaces.iter_mut().zip(&local_surface) {
                *out |= visible;
            }
            for (out, &visible) in smodels.iter_mut().zip(&local_smodel) {
                *out |= visible;
            }
            // The union above only enumerates retained candidates. Submission uses
            // this view's own native membership, never another view's union.
            view.visibility[index] = Some(render_frame::portal::PortalDrawVisibility {
                surface_base,
                surfaces: local_surface.into(),
                smodel_base,
                smodels: local_smodel.into(),
                bmodels: Default::default(),
            });
            reports.push(PortalVisibility {
                view: index,
                depth,
                max_range,
                world,
                eye: virtual_eye,
                seed,
                cell,
                planes,
                visible_cells,
                visible_surfaces,
                visible_smodels,
            });
        }
        (reports, root_cells)
    }
}

pub(crate) fn log_script_model_gaps_once(
    props: Query<&ScriptModelGameObject>,
    mut done: Local<bool>,
) {
    if *done || props.is_empty() {
        return;
    }
    *done = true;
    let mut names: Vec<&str> = props.iter().map(|prop| prop.0.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    diag::info!(
        World,
        "script_model gameobjects after _gameobjects::main: {} props kept, tags={:?}",
        props.iter().count(),
        names
    );
}

pub(crate) fn log_dpvs_stats_once(stats: Res<DpvsFrameStats>, mut done: Local<bool>) {
    if *done || stats.unculled_triangles == 0 {
        return;
    }
    *done = true;
    diag::info!(
        World,
        "dpvs frame: eye_cell={:?} unresolved={} visible_cells={} portals_frustum_skipped={} \
         child_planes_unported={} aabb_nodes={} planes_dropped={} \
         surf_rejected_by_bounds={} surf_unbounded={} smodel_rejected_by_bounds={} \
         smodel_unbounded={} draw_decals={} bsp_visible={} bsp_admitted={} bsp_runs={} \
         bsp_input_gap={} bsp_ranges_unavailable={} keys={} ordinal_refused={} g0_world={} rebinds={} surfaces={} \
         tris={}/{} (lightmapped={} fallback={}) single_cell={} batches={} rewrite_bytes={} \
         vis_chg={} sky_surf={} sky_mesh={} sky_vis={} sky_drawn={} sky_admit={} \
         aspect={:.3} fov_deg={:.1}",
        stats.eye_cell,
        stats.eye_cell_unresolved,
        stats.visible_cells,
        stats.portals_frustum_skipped,
        stats.child_planes_unported,
        stats.aabb_nodes_visited,
        stats.aabb_planes_dropped,
        stats.surfaces_rejected_by_bounds,
        stats.surfaces_admitted_unbounded,
        stats.smodels_rejected_by_bounds,
        stats.smodels_admitted_unbounded,
        DRAW_DECALS,
        stats.bsp_visible_surfaces,
        stats.bsp_admitted_surfaces,
        stats.bsp_run_n,
        stats.bsp_input_gap_n,
        stats.bsp_ranges_unavailable,
        stats.keys,
        stats.material_ordinal_refused,
        stats.g0_world_surfs.len(),
        stats.rebinds,
        stats.surfaces,
        stats.triangles,
        stats.unculled_triangles,
        stats.lightmapped_triangles,
        stats.fallback_triangles,
        stats.single_cell,
        stats.submitted_batches,
        stats.rewritten_index_bytes,
        stats.visibility_changes,
        stats.sky_surf_n,
        stats.sky_mesh_n,
        stats.sky_vis_n,
        stats.sky_drawn_n,
        stats.sky_admitted,
        stats.aspect,
        stats.fov_deg,
    );
}

pub(crate) fn add_world_surfaces_frustum_only(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
    frustum: &[[f32; 4]],
    camera_cell_vis_lsb: &[u32],
    camera_vis_all: bool,
    surface_vis: &mut [u8],
    smodel_vis: &mut [u8],
    draw_msb: &mut Vec<u32>,
) -> (AabbCullStats, u32) {
    let mut stats = AabbCullStats::default();
    let mut vis_data = DpvsVisData {
        surface_vis,
        smodel_vis,
    };
    let words = cell_caster_row_words(dpvs.cell_count);
    if draw_msb.len() != words {
        draw_msb.clear();
        draw_msb.resize(words, 0);
    } else {
        draw_msb.fill(0);
    }
    let draw_n = if dpvs.cell_caster_bits.is_empty() {
        for cell in 0..dpvs.cell_count {
            add_one_cell_frustum_only(dpvs, cell, frustum, &mut vis_data, &mut stats);
        }
        dpvs.cell_count as u32
    } else {
        let n = or_caster_rows_for_visible(
            &dpvs.cell_caster_bits,
            dpvs.cell_count,
            camera_cell_vis_lsb,
            camera_vis_all,
            &mut draw_msb[..],
        );
        for cell in 0..dpvs.cell_count {
            if !msb_get(draw_msb, cell) {
                continue;
            }
            add_one_cell_frustum_only(dpvs, cell, frustum, &mut vis_data, &mut stats);
        }
        n
    };
    admit_generated_surfaces(
        dpvs,
        |cell| dpvs.cell_caster_bits.is_empty() || msb_get(draw_msb, cell),
        frustum,
        vis_data.surface_vis,
    );
    (stats, draw_n)
}

impl WorldDrawItems {
    pub(crate) fn append_camera_bsp_range(
        &mut self,
        range: asset_world::CameraSurfRange,
        surface_vis: &mut [u8],
        surfaces: &[asset_world::SurfaceDrawFields],
        draw_surfs: &[GfxDrawSurf],
        surface_materials: &[Option<assets::MaterialIndex>],
    ) -> BspDrawSurfCensus {
        let asset_world::CameraSurfRange { kind, begin, end } = range;
        let scratch = &mut self.bsp_run_scratch;
        let draw_items = &mut self.items;
        let g0_surfs = &mut self.g0_surfs;
        let range_n = end.saturating_sub(begin);
        if end < begin {
            return BspDrawSurfCensus {
                input_gap_n: 1,
                ..BspDrawSurfCensus::default()
            };
        }
        let (Ok(begin_i), Ok(end_i)) = (usize::try_from(begin), usize::try_from(end)) else {
            return BspDrawSurfCensus {
                range_n,
                input_gap_n: range_n.max(1),
                ..BspDrawSurfCensus::default()
            };
        };
        if end_i > surface_vis.len() || end_i > surfaces.len() || end_i > draw_surfs.len() {
            let visible_n = surface_vis
                .get(begin_i..surface_vis.len().min(end_i))
                .map_or(0, |bytes| {
                    bytes.iter().filter(|byte| **byte != 0).count() as u32
                });
            return BspDrawSurfCensus {
                range_n,
                visible_n,
                input_gap_n: range_n.max(1),
                ..BspDrawSurfCensus::default()
            };
        }

        let mut host_input_gap_n = 0u32;
        for surf in begin_i..end_i {
            if surface_vis[surf] == 0 || draw_surfs[surf].packed != 0 {
                continue;
            }
            surface_vis[surf] = 0;
            host_input_gap_n = host_input_gap_n.saturating_add(1);
            if let Ok(surf) = u16::try_from(surf) {
                g0_surfs.push(surf);
            }
        }

        let placeholder = BspDrawSurfRun {
            kind,
            first_surf: 0,
            surf_count: 0,
            draw_surf: GfxDrawSurf::from_packed(0),
            setup_key_changed: false,
        };
        scratch.clear();
        scratch.resize(end_i - begin_i, placeholder);

        let mut census = BspDrawSurfCensus {
            range_n,
            ..BspDrawSurfCensus::default()
        };
        let material = |surf: usize| surface_materials.get(surf).copied().flatten();
        let mut span_begin = begin;
        let mut previous_key = None;
        while (span_begin as usize) < end_i {
            let head = span_begin as usize;
            let visible = surface_vis[head] != 0;
            let span_len = surface_vis[head..end_i]
                .iter()
                .enumerate()
                .position(|(offset, &vis)| {
                    (vis != 0) != visible || (visible && material(head + offset) != material(head))
                })
                .unwrap_or(end_i - head);
            let span_end = span_begin + span_len as u32;
            if visible {
                let output = &mut scratch[census.run_n as usize..];
                let part = add_bsp_draw_surfs_camera(
                    kind,
                    span_begin,
                    span_end,
                    surface_vis,
                    surfaces,
                    draw_surfs,
                    output,
                );
                let emitted = part.run_n.saturating_sub(part.output_overflow_n) as usize;
                if let Some(first) = output.get_mut(..emitted).and_then(|runs| runs.first_mut()) {
                    let key = dpvs_iw4::bsp_draw_surf_setup_key(first.draw_surf);
                    first.setup_key_changed = previous_key.is_some_and(|previous| previous != key);
                    previous_key = Some(dpvs_iw4::bsp_draw_surf_setup_key(
                        output[emitted - 1].draw_surf,
                    ));
                }
                census.visible_n += part.visible_n;
                census.admitted_n += part.admitted_n;
                census.run_n += part.run_n;
                census.input_gap_n += part.input_gap_n;
                census.output_overflow_n += part.output_overflow_n;
            }
            span_begin = span_end;
        }
        census.visible_n = census.visible_n.saturating_add(host_input_gap_n);
        census.input_gap_n = census.input_gap_n.saturating_add(host_input_gap_n);
        let emitted_n = census.run_n.saturating_sub(census.output_overflow_n) as usize;
        for run in scratch.iter().take(emitted_n) {
            draw_items.push(WorldDrawItem {
                key: run.draw_surf.packed,
                surf: run.first_surf,
                run: run.surf_count,
                kind: WorldDrawItemKind::Bsp(run.kind),
                setup_key_changed: run.setup_key_changed,
            });
        }
        census
    }
}

fn add_one_cell_frustum_only(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
    cell: usize,
    frustum: &[[f32; 4]],
    vis_data: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let Some(tree) = dpvs.aabb_trees.get(cell) else {
        return;
    };
    if tree.is_empty() {
        if let Some(root) = dpvs.cell_roots.get(cell)
            && root.count > 0
        {
            admit_cell_root_span(
                &dpvs.sorted_surf_index,
                root.start,
                root.count,
                vis_data,
                stats,
            );
        }
        return;
    }
    let cull_input = AabbTreeCull {
        nodes: tree,
        smodel_indexes: dpvs
            .aabb_smodel_indices
            .get(cell)
            .map_or(&[][..], Vec::as_slice),
        sorted_surf_index: &dpvs.sorted_surf_index,
        surfaces_bounds: &dpvs.surface_bounds,
        smodel_bounds: &dpvs.smodel_bounds,
        draw_decals: DRAW_DECALS,
    };
    add_aabb_tree_surfaces_in_frustum(&cull_input, frustum, vis_data, stats);
}

pub(crate) fn packed_world_colour_key(
    cull: &crate::prepare::scene::world::WorldCull,
    surf: usize,
    baked_material_keys: &[Option<u64>],
    batch_primary_lights: &[u8],
) -> Option<u64> {
    let material_asset_id = cull.surface_materials.get(surf).copied().flatten()?;
    if let Some(word) = cull
        .capture
        .packed_draw_surfs
        .get(surf)
        .copied()
        .filter(|word| word.packed != 0)
    {
        return Some(word.packed);
    }
    let packed = baked_material_keys
        .get(material_asset_id.order())
        .copied()
        .flatten()?;
    let scene_light_index = if surf < cull.surface_primary_lights.len() {
        cull.surface_primary_lights[surf]
    } else {
        cull.surface_batch_ranges
            .get(surf)
            .and_then(|&(batch, _, _)| batch_primary_lights.get(batch).copied())
            .unwrap_or(0)
    };
    Some(crate::assemble::drawsurf::with_scene_light_index(
        packed,
        scene_light_index,
    ))
}

pub(crate) fn append_bmodel_colour_span(
    cull: &mut crate::prepare::scene::world::WorldCull,
    start_surf: u16,
    surface_count: u16,
    already: &mut HashSet<u16>,
    baked_material_keys: &[Option<u64>],
    batch_primary_lights: &[u8],
) -> (u32, u32) {
    let mut added = 0u32;
    let mut refused = 0u32;
    let start = usize::from(start_surf);
    let end = start.saturating_add(usize::from(surface_count));
    for surf in start..end {
        let Ok(surf_u16) = u16::try_from(surf) else {
            continue;
        };
        if !already.insert(surf_u16) {
            continue;
        }
        match packed_world_colour_key(cull, surf, baked_material_keys, batch_primary_lights) {
            Some(packed) => {
                cull.draws.items.push(WorldDrawItem {
                    key: packed,
                    surf: surf_u16,
                    run: 1,
                    kind: WorldDrawItemKind::BModel,
                    setup_key_changed: false,
                });
                added = added.saturating_add(1);
            }
            None => refused = refused.saturating_add(1),
        }
    }
    (added, refused)
}

pub(crate) fn refresh_draw_items_id(cull: &mut crate::prepare::scene::world::WorldCull) {
    let mut draw_items_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(
        &mut draw_items_id,
        cull.draws.items.len() as u64,
    );
    for item in &cull.draws.items {
        if item.kind == WorldDrawItemKind::BModel
            && let Some((model, pose)) =
                crate::assemble::drawsurf::retained_list::bmodel_owner_for_surf(
                    item.surf,
                    &cull.brush_models,
                    &cull.bmodel_world_from_local,
                )
        {
            crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(model));
            for word in pose.to_cols_array() {
                crate::assemble::drawsurf::list::mix_content_id(
                    &mut draw_items_id,
                    u64::from(word.to_bits()),
                );
            }
        }
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, item.key);
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.surf));
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.run));
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            match item.kind {
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitOpaque) => 0,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitTrans) => 1,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Emissive) => 2,
                WorldDrawItemKind::BModel => 3,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Decal) => 4,
            },
        );
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            u64::from(item.setup_key_changed),
        );
    }
    cull.draws.content_id = draw_items_id;
}

#[must_use]
pub(crate) fn draw_item_rebinds(items: &[WorldDrawItem]) -> u32 {
    let mut prev_mat: Option<u16> = None;
    let mut rebinds = 0u32;
    for item in items {
        let mat = ((item.key >> 30) & 0xfff) as u16;
        if prev_mat != Some(mat) {
            rebinds = rebinds.saturating_add(1);
            prev_mat = Some(mat);
        }
    }
    rebinds
}
