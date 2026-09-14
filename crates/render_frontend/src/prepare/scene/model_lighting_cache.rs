use std::collections::{HashMap, HashSet};

use anim_iw4::{
    DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT, DOBJ_RADIUS_PARENT_ROOT, dobj_compute_bounds_radius,
};
use assets::FpvMeshCatalog;
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
    ModelLightingOwner, ModelLightingRequest, ModelLightingRequests, ResolvedModelLighting,
    ResolvedModelLightingTable,
};

#[derive(Resource)]
pub struct WorldModelLightingCache {
    pub handle: u16,

    body_handles: HashMap<ModelLightingOwner, u16>,
    rover: u32,
    alloc_fail: bool,

    mod_frame_count: u32,
    origins: Vec<[f32; 3]>,
    lighting_info: Vec<u16>,

    packed_lighting: Vec<Option<[u8; 4]>>,

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
            rover: 0,
            alloc_fail: false,
            mod_frame_count: 0,
            origins: vec![[0.0; 3]; limit],
            lighting_info: vec![0; limit],
            packed_lighting: vec![None; limit],
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
        key: ModelLightingOwner,
        origin: [f32; 3],
        atlas: &WorldModelLightingAtlas,
        scene: Option<&WorldScene>,
        images: &mut Assets<Image>,
        lookup_fallback: u8,
        allow_moved_reuse: bool,
    ) -> u32 {
        let dims = atlas.dims;
        let base = dims.smodel_entry_limit;
        let current = self.body_handles.get(&key).copied().unwrap_or(0);
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
                &pixel_free_bits[pp],
                &pixel_free_bits[p],
                &mut curr,
            ) {
                Ok(mut glob) => Some(glob.alloc(current, origin, allow_moved_reuse)),
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
                (handle, true, Some(slot))
            }
            ModelLightingCacheAlloc::Failed => {
                self.frame_failed = self.frame_failed.saturating_add(1);
                if !self.warned_fail {
                    diag::info!(
                        World,
                        "model light cache alloc failed (warn {}) — remote body skipped",
                        ModelLightingCacheGlob::warn_index_on_fail()
                    );
                    self.warned_fail = true;
                }
                self.body_handles.remove(&key);
                return 0;
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
            let Some(grid) = scene.light_grid.as_ref() else {
                self.body_handles.remove(&key);
                return 0;
            };
            match assets::sample_light_grid_with_lookup_fallback(
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
                    if let (Some(entry), Some(mut img)) = (
                        ModelLightingTileIndex::from_handle(handle),
                        images.get_mut(&atlas.image),
                    ) {
                        let _ =
                            model_lighting_atlas_write_tile(&mut *img, dims, entry, &sampled.tile);
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
                }
                Err(_) => {
                    if let Some(packed) = self.packed_lighting.get_mut(slot as usize) {
                        *packed = None;
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

    pub fn packed_lighting_for_handle(&self, handle: u32) -> Option<[u8; 4]> {
        let Ok(h) = u16::try_from(handle) else {
            return None;
        };
        lighting_iw4::model_lighting_cache_slot(self.base_index, h)
            .and_then(|slot| self.packed_lighting.get(slot as usize).copied().flatten())
    }
}

pub(crate) fn dobj_lighting_box_half(radii: &[f32], parents: &[u8]) -> Option<[f32; 3]> {
    if radii.is_empty() || radii.len() > DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT {
        return None;
    }
    Some(lighting_query_box_half(dobj_compute_bounds_radius(
        radii, parents,
    )))
}

pub(crate) fn fpv_dobj_lighting_box_half(
    catalog: &FpvMeshCatalog,
    ns: assets::AssetNamespace,
    gun_xmodel: &str,
) -> Option<[f32; 3]> {
    let hands = catalog.hands_in(ns)?;
    let gun = catalog.get(ns, gun_xmodel)?;
    let r_hands = hands.skel.radius?;
    let r_gun = gun.skel.radius?;
    dobj_lighting_box_half(&[r_hands, r_gun], &[DOBJ_RADIUS_PARENT_ROOT, 0])
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
        fpv_dobj_lighting_box_half(&catalog.0, handles.fpv.namespace, &handles.fpv.gun_xmodel)
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
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(cache, atlas, scene, images, requests, resolved, false);
}

pub(crate) fn update_glass_dyn_lighting(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    images: ResMut<Assets<Image>>,
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(cache, atlas, scene, images, requests, resolved, true);
}

pub(crate) fn update_fx_dyn_lighting(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    images: ResMut<Assets<Image>>,
    requests: ResMut<ModelLightingRequests>,
    resolved: ResMut<ResolvedModelLightingTable>,
) {
    drain_model_lighting_requests(cache, atlas, scene, images, requests, resolved, false);
}

fn drain_model_lighting_requests(
    cache: Option<ResMut<WorldModelLightingCache>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    scene: Option<Res<WorldScene>>,
    mut images: ResMut<Assets<Image>>,
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
                request.owner,
                request.origin,
                atlas,
                scene,
                &mut images,
                request.lookup_fallback,
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
