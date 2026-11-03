use bevy::platform::collections::{HashMap, HashSet};

use anim_iw4::{DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT, DOBJ_RADIUS_PARENT_ROOT, compute_bounds_radius};
use asset_model::FpvMeshCatalog;
use bevy::prelude::*;
use lighting_iw4::{
    MODEL_LIGHTING_PIXEL_FREE_BITS_BUFFERS, ModelLightingCacheAlloc, ModelLightingCacheGlob,
    ModelLightingTileIndex, dyn_pixel_free_bits_index, lighting_query_box_half,
    model_lighting_cache_free_bits_words, toggle_dyn_model_lighting_frame,
};

use crate::prepare::scene::camera::FlyCamera;
use crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas;
use crate::prepare::scene::model_lighting_atlas::model_lighting_atlas_write_tile;
use crate::prepare::scene::world::WorldScene;
use render_anim::SessionViewmodel;

pub use render_scene::{
    ModelLightingAtlasTileWrites, ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
    ResolvedModelLighting, ResolvedModelLightingTable,
};

#[derive(Resource)]
pub struct WorldModelLightingCache {
    pub handle: u16,

    body_handles: HashMap<ModelLightingOwner, u16>,
    slot_owner: Vec<Option<ModelLightingOwner>>,
    rover: u32,
    alloc_fail: bool,

    mod_frame_count: u32,
    origins: Vec<[f32; 3]>,
    lighting_info: Vec<u16>,

    packed_lighting: Vec<Option<[u8; 4]>>,

    lighting_sh: std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>,

    pixel_free_bits: [Vec<u32>; MODEL_LIGHTING_PIXEL_FREE_BITS_BUFFERS],
    warned_fail: bool,

    frame_assigned: u32,
    frame_reused: u32,
    frame_failed: u32,

    base_index: u32,

    logged_fpv_atpoint: bool,

    eye_atpoint_path: Option<String>,
    eye_live_corners: Option<u8>,
    eye_tile0: Option<[u8; 4]>,

    eye_picked_raw: Option<u8>,
}

impl WorldModelLightingCache {
    pub fn new(dims: lighting_iw4::ModelLightingAtlasDims) -> Self {
        let limit = dims.xmodel_entry_limit as usize;
        let words = model_lighting_cache_free_bits_words(dims.xmodel_entry_limit);
        let ones = vec![!0u32; words];
        Self {
            handle: 0,
            body_handles: HashMap::new(),
            slot_owner: vec![None; limit],
            rover: 0,
            alloc_fail: false,
            mod_frame_count: 0,
            origins: vec![[0.0; 3]; limit],
            lighting_info: vec![0; limit],
            packed_lighting: vec![None; limit],
            lighting_sh: std::sync::Arc::new(vec![None; limit]),
            pixel_free_bits: [ones.clone(), ones.clone(), ones.clone(), ones],
            warned_fail: false,
            frame_assigned: 0,
            frame_reused: 0,
            frame_failed: 0,
            base_index: dims.smodel_entry_limit,
            logged_fpv_atpoint: false,
            eye_atpoint_path: None,
            eye_live_corners: None,
            eye_tile0: None,
            eye_picked_raw: None,
        }
    }

    pub fn toggle_frame(&mut self) {
        let [b0, b1, b2, b3] = &mut self.pixel_free_bits;
        toggle_dyn_model_lighting_frame(
            &mut [
                b0.as_mut_slice(),
                b1.as_mut_slice(),
                b2.as_mut_slice(),
                b3.as_mut_slice(),
            ],
            &mut self.mod_frame_count,
            &mut self.alloc_fail,
        );
        self.warned_fail = false;
        self.frame_assigned = 0;
        self.frame_reused = 0;
        self.frame_failed = 0;
    }

    fn viewer_fallback(&self, key: ModelLightingOwner) -> u32 {
        if key == ModelLightingOwner::Eye {
            0
        } else {
            u32::from(self.handle)
        }
    }

    pub(crate) fn handle_for(&self, key: ModelLightingOwner) -> u16 {
        self.body_handles.get(&key).copied().unwrap_or(0)
    }

    pub(crate) fn retain_glass(&mut self, live: &HashSet<ModelLightingOwner>) {
        self.body_handles.retain(|owner, _| {
            !matches!(owner, ModelLightingOwner::Glass(_)) || live.contains(owner)
        });
    }

    pub fn alloc_sample_at(
        &mut self,
        request: ModelLightingRequest,
        atlas: &WorldModelLightingAtlas,
        scene: Option<&WorldScene>,
        (images, tile_writes): (&mut Assets<Image>, &mut ModelLightingAtlasTileWrites),
        allow_moved_reuse: bool,
    ) -> u32 {
        let ModelLightingRequest {
            owner: key,
            origin,
            lookup_fallback,
        } = request;
        let dims = atlas.dims;
        let base = dims.smodel_entry_limit;
        let current = self.body_handles.get(&key).copied().unwrap_or(0);
        // Rewriting a held slot in place relies on its tile write reaching the GPU
        // after every earlier frame's draws.
        let owned = lighting_iw4::model_lighting_cache_slot(base, current)
            .and_then(|slot| self.slot_owner.get(slot as usize))
            .is_some_and(|owner| *owner == Some(key));
        let i = dyn_pixel_free_bits_index(self.mod_frame_count, 0);
        let p = dyn_pixel_free_bits_index(self.mod_frame_count, 1);
        let pp = dyn_pixel_free_bits_index(self.mod_frame_count, 2);
        let mut curr = std::mem::take(&mut self.pixel_free_bits[i]);
        let glob_alloc = {
            let Self {
                rover,
                alloc_fail,
                origins,
                lighting_info,
                pixel_free_bits,
                ..
            } = self;
            match ModelLightingCacheGlob::new(
                base,
                dims.xmodel_entry_limit,
                rover,
                alloc_fail,
                origins,
                lighting_info,
                lighting_iw4::ModelLightingFreeFrames {
                    prev_prev: &pixel_free_bits[pp],
                    prev: &pixel_free_bits[p],
                    curr: &mut curr,
                },
            ) {
                Ok(mut glob) => Some(glob.alloc(current, origin, allow_moved_reuse || owned)),
                Err(_) => None,
            }
        };
        self.pixel_free_bits[i] = curr;
        let Some(alloc) = glob_alloc else {
            self.body_handles.remove(&key);
            return 0;
        };

        let (handle, need_sample, slot) = match alloc {
            ModelLightingCacheAlloc::Reused { handle, .. } => {
                self.frame_reused = self.frame_reused.saturating_add(1);
                self.body_handles.insert(key, handle);
                (handle, false, None)
            }
            ModelLightingCacheAlloc::Assigned { handle, slot } => {
                self.frame_assigned = self.frame_assigned.saturating_add(1);
                self.body_handles.insert(key, handle);
                if let Some(owner) = self.slot_owner.get_mut(slot as usize) {
                    *owner = Some(key);
                }
                (handle, true, Some(slot))
            }
            ModelLightingCacheAlloc::Failed => {
                self.frame_failed = self.frame_failed.saturating_add(1);
                if !self.warned_fail {
                    diag::info!(
                        World,
                        "model light cache full (warn {}): {key:?} lit from the viewer's sample this frame \
                         ({} assigned, {} reused so far)",
                        ModelLightingCacheGlob::warn_index_on_fail(),
                        self.frame_assigned,
                        self.frame_reused
                    );
                    self.warned_fail = true;
                }
                self.body_handles.remove(&key);
                return self.viewer_fallback(key);
            }
        };

        if need_sample {
            let Some(slot) = slot else {
                return 0;
            };
            let Some(scene) = scene else {
                self.body_handles.remove(&key);
                return 0;
            };
            let world = asset_core::atlas::WorldLayout.world_at(origin).0;
            let (grid, origin) = if world == 0 {
                (scene.light_grid.as_ref(), origin)
            } else {
                let layer = scene
                    .cull
                    .as_ref()
                    .and_then(|cull| cull.atlas_layers.iter().find(|layer| layer.world == world));
                (
                    layer.and_then(|layer| layer.light_grid.as_ref()),
                    [
                        origin[0]
                            - asset_core::atlas::WorldLayout
                                .origin(asset_core::atlas::WorldId(world))[0],
                        origin[1],
                        origin[2],
                    ],
                )
            };
            let Some(grid) = grid else {
                self.body_handles.remove(&key);
                return 0;
            };
            match asset_model::sample_light_grid_with_lookup_fallback(
                &grid.view(),
                origin,
                lookup_fallback,
            ) {
                Ok(sampled) => {
                    if key == ModelLightingOwner::Eye {
                        self.eye_atpoint_path = Some(format!("{:?}", sampled.path));
                        self.eye_live_corners = Some(sampled.live_corners as u8);
                        self.eye_tile0 = Some([
                            sampled.tile[0],
                            sampled.tile[1],
                            sampled.tile[2],
                            sampled.tile[3],
                        ]);
                        self.eye_picked_raw = Some(sampled.picked_before_remap);
                        if !self.logged_fpv_atpoint {
                            self.logged_fpv_atpoint = true;
                            diag::info!(
                                World,
                                "fpv AtPoint origin={:?} primary={} path={:?} live={} need_sample=1",
                                origin,
                                sampled.picked_primary,
                                sampled.path,
                                sampled.live_corners
                            );
                        }
                    }
                    // Untracked: only `tile_writes` carries the tile to the GPU.
                    if let (Some(entry), Some(img)) = (
                        ModelLightingTileIndex::from_handle(handle),
                        images.get_mut_untracked(&atlas.image),
                    ) && model_lighting_atlas_write_tile(img, dims, entry, &sampled.tile)
                    {
                        tile_writes.push(atlas.image.id(), entry, &sampled.tile);
                    }
                    self.lighting_info[slot as usize] = lighting_iw4::lighting_info_from_bytes(
                        sampled.picked_primary,
                        scene.reflection_probe_for_lighting_origin(origin),
                    );
                    self.packed_lighting[slot as usize] = Some([
                        sampled.compressed[0],
                        sampled.compressed[1],
                        sampled.compressed[2],
                        0xff,
                    ]);
                    std::sync::Arc::make_mut(&mut self.lighting_sh)[slot as usize] =
                        sampled.lighting_sh;
                }
                Err(_) => {
                    if let Some(packed) = self.packed_lighting.get_mut(slot as usize) {
                        *packed = None;
                    }
                    if let Some(sh) =
                        std::sync::Arc::make_mut(&mut self.lighting_sh).get_mut(slot as usize)
                    {
                        *sh = None;
                    }
                    if let Some(owner) = self.slot_owner.get_mut(slot as usize) {
                        *owner = None;
                    }
                    self.body_handles.remove(&key);
                    return 0;
                }
            }
        }

        u32::from(handle)
    }

    pub fn reflection_probe_index_for_handle(&self, handle: u32) -> u8 {
        let Ok(h) = u16::try_from(handle) else {
            return 0;
        };
        lighting_iw4::model_lighting_cache_slot(self.base_index, h)
            .and_then(|slot| self.lighting_info.get(slot as usize).copied())
            .map(lighting_iw4::lighting_info_reflection_probe_index)
            .unwrap_or(0)
    }

    pub fn scene_light_index_for_handle(&self, handle: u32) -> u8 {
        let Ok(h) = u16::try_from(handle) else {
            return 0;
        };
        lighting_iw4::model_lighting_cache_slot(self.base_index, h)
            .and_then(|slot| self.lighting_info.get(slot as usize).copied())
            .map(lighting_iw4::lighting_info_scene_light_index)
            .unwrap_or(0)
    }

    /// Light-grid SH per cache slot, with the handle base that maps a handle to its slot.
    pub fn lighting_sh(&self) -> (u32, std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>) {
        (self.base_index, std::sync::Arc::clone(&self.lighting_sh))
    }

    pub fn packed_lighting_for_handle(&self, handle: u32) -> Option<[u8; 4]> {
        let Ok(h) = u16::try_from(handle) else {
            return None;
        };
        lighting_iw4::model_lighting_cache_slot(self.base_index, h)
            .and_then(|slot| self.packed_lighting.get(slot as usize).copied().flatten())
    }
}

pub(crate) fn lighting_box_half(radii: &[f32], parents: &[u8]) -> Option<[f32; 3]> {
    if radii.is_empty() || radii.len() > DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT {
        return None;
    }
    Some(lighting_query_box_half(compute_bounds_radius(
        radii, parents,
    )))
}

pub(crate) fn fpv_dobj_lighting_box_half(
    catalog: &FpvMeshCatalog,
    hands_index: assets::FpvMeshIndex,
    gun_index: assets::FpvMeshIndex,
) -> Option<[f32; 3]> {
    let hands = catalog.get_at(hands_index.order())?;
    let gun = catalog.get_at(gun_index.order())?;
    let r_hands = hands.skel.radius?;
    let r_gun = gun.skel.radius?;
    lighting_box_half(&[r_hands, r_gun], &[DOBJ_RADIUS_PARENT_ROOT, 0])
}

pub fn viewmodel_lighting_origin(
    origin: [f32; 3],
    view_height_current: f32,
    view_yaw: f32,
    leanf: f32,
) -> [f32; 3] {
    lighting_iw4::viewmodel_lighting_origin(origin, view_height_current, view_yaw, leanf)
}

pub(crate) fn begin_dyn_model_lighting_frame(
    cache: Option<ResMut<WorldModelLightingCache>>,
    mut requests: ResMut<ModelLightingRequests>,
    mut resolved: ResMut<ResolvedModelLightingTable>,
) {
    requests.clear();
    resolved.clear();
    let Some(mut cache) = cache else {
        return;
    };
    cache.toggle_frame();
}

pub fn enqueue_fpv_model_lighting(
    atpoint: Res<render_scene::DynAtPointLookup>,
    fpv_meshes: Option<Res<assets::PreparedFpvMeshes>>,
    session_vm: Option<Res<SessionViewmodel>>,
    cameras: Query<&Transform, With<FlyCamera>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    local: Option<Res<net::LocalPresentClient>>,
    mut requests: ResMut<ModelLightingRequests>,
) {
    if session_vm
        .as_ref()
        .and_then(|session| session.0.as_ref())
        .is_none()
    {
        return;
    }

    let origin = presented
        .as_ref()
        .and_then(|presented| {
            let local = local.as_ref()?;
            let ps = presented.player(local.0)?;
            Some(viewmodel_lighting_origin(
                ps.origin,
                ps.view_height_current,
                ps.viewangles[1],
                ps.leanf,
            ))
        })
        .or_else(|| {
            let cam = cameras.single().ok()?;
            Some([cam.translation.x, cam.translation.y, cam.translation.z])
        });
    let Some(origin) = origin else {
        return;
    };

    let box_half = session_vm.as_ref().and_then(|session| {
        let handles = session.0.as_ref()?;
        let catalog = fpv_meshes.as_ref()?;
        (catalog.0.identity() == handles.catalog_id).then_some(())?;
        fpv_dobj_lighting_box_half(&catalog.0, handles.fpv.hands_index, handles.fpv.gun_index)
    });
    let lookup_fallback = atpoint.fallback(origin, box_half);
    requests.request(ModelLightingRequest {
        owner: ModelLightingOwner::Eye,
        origin,
        lookup_fallback,
    });
}

pub(crate) fn update_dirty_model_lighting(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    images: ResMut<Assets<Image>>,
    tile_writes: ResMut<ModelLightingAtlasTileWrites>,
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(
        cache,
        atlas,
        scene,
        (images, tile_writes),
        requests,
        resolved,
        false,
    );
}

pub(crate) fn update_glass_dyn_lighting(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    images: ResMut<Assets<Image>>,
    tile_writes: ResMut<ModelLightingAtlasTileWrites>,
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(
        cache,
        atlas,
        scene,
        (images, tile_writes),
        requests,
        resolved,
        true,
    );
}

pub(crate) fn update_fx_dyn_lighting(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    images: ResMut<Assets<Image>>,
    tile_writes: ResMut<ModelLightingAtlasTileWrites>,
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(
        cache,
        atlas,
        scene,
        (images, tile_writes),
        requests,
        resolved,
        false,
    );
}

fn drain_model_lighting_requests(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    (mut images, mut tile_writes): (ResMut<Assets<Image>>, ResMut<ModelLightingAtlasTileWrites>),
    mut requests: ResMut<ModelLightingRequests>,
    mut resolved: ResMut<ResolvedModelLightingTable>,
    prune_glass: bool,
) {
    let pending = requests.take_pending();
    let live_glass: HashSet<ModelLightingOwner> = pending
        .iter()
        .map(|request| request.owner)
        .filter(|owner| matches!(owner, ModelLightingOwner::Glass(_)))
        .collect();
    let scene_atlas = scene.as_ref().and_then(|scene| {
        Some(WorldModelLightingAtlas {
            image: scene.model_lighting_image.clone()?,
            dims: scene.model_lighting_dims?,
        })
    });
    let atlas = atlas.as_deref().or(scene_atlas.as_ref());
    let (Some(mut cache), Some(atlas)) = (cache, atlas) else {
        for request in pending {
            resolved.insert_if_absent(request.owner, ResolvedModelLighting::Failed);
        }
        if prune_glass {
            resolved.retain_glass(&live_glass);
        }
        return;
    };
    let scene = scene.as_deref();
    for request in pending {
        let moving_glass = matches!(request.owner, ModelLightingOwner::Glass(_));
        if moving_glass {
            resolved.remove(request.owner);
        }
        resolved.get_or_insert_with(request.owner, || {
            let handle = cache.alloc_sample_at(
                request,
                atlas,
                scene,
                (&mut images, &mut tile_writes),
                moving_glass,
            );
            if handle == 0 {
                if request.owner == ModelLightingOwner::Eye {
                    cache.handle = 0;
                }
                ResolvedModelLighting::Failed
            } else {
                if request.owner == ModelLightingOwner::Eye {
                    cache.handle = handle as u16;
                }
                ResolvedModelLighting::Seated {
                    handle,
                    scene_light_index: cache.scene_light_index_for_handle(handle),
                    reflection_probe_index: cache.reflection_probe_index_for_handle(handle),
                    packed_lighting: cache.packed_lighting_for_handle(handle),
                }
            }
        });
    }
    if prune_glass {
        cache.retain_glass(&live_glass);
        resolved.retain_glass(&live_glass);
    }
}
