use std::hash::{Hash, Hasher};
use std::sync::Arc;
pub use xmodel_runtime::{ClientAnimSample, overlay_legs_clip};
use xmodel_runtime::{apply_player_anim_goals, apply_player_anim_rates};

use anim_iw4::{DOBJ_RADIUS_PARENT_ROOT, PLAYER_ANIM_RAW_MASK, PlayerAnimValue};
use bevy::prelude::*;

use crate::anim::xmodel_pose::PosedSmodelSurface;

#[path = "remote_animation.rs"]
mod animation;
pub use animation::*;

pub fn occupy_lod_byte(lod: Option<u8>) -> i8 {
    lod.and_then(|lod| i8::try_from(lod).ok()).unwrap_or(-1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrittenSlotLod {
    Missing,

    Invalid,
    Written(u8),
}

pub fn written_slot_lod(slot_lods: &[i8], model: usize) -> WrittenSlotLod {
    match slot_lods.get(model) {
        None => WrittenSlotLod::Missing,
        Some(&lod) => match u8::try_from(lod) {
            Ok(lod) => WrittenSlotLod::Written(lod),
            Err(_) => WrittenSlotLod::Invalid,
        },
    }
}

pub fn resolve_slot_lod(
    slot_lods: &[i8],
    model: usize,
    ramp: impl FnOnce() -> Option<u8>,
) -> Option<u8> {
    match written_slot_lod(slot_lods, model) {
        WrittenSlotLod::Written(lod) => Some(lod),
        WrittenSlotLod::Missing => ramp(),
        WrittenSlotLod::Invalid => None,
    }
}

#[path = "remote_kit.rs"]
mod kit;
pub use kit::*;
pub(crate) use kit::{PreparedRemoteKits, occupy_remote_kit_dobj, select_remote_models};

pub fn validate_remote_tracks(
    dobj: &xmodel_runtime::DObj,
    clips: Option<&PoseClips>,
    body: &asset_model::BodyMeshEntry,
    body_name: &str,
) -> Result<(), String> {
    let Some(clips) = clips else {
        return Ok(());
    };
    let match_clip = clips.torso_clip.as_ref().unwrap_or(&clips.clip);
    let matched = dobj
        .tracks_for(match_clip)
        .iter()
        .filter(|bone| bone.is_some())
        .count()
        + if clips.torso_clip.is_some() {
            dobj.tracks_for(&clips.legs_for_tree)
                .iter()
                .filter(|bone| bone.is_some())
                .count()
        } else {
            0
        };
    if matched > 0 {
        return Ok(());
    }
    let tracks = match_clip
        .tracks
        .iter()
        .take(8)
        .map(|track| track.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let bones = body
        .skel
        .bone_names
        .iter()
        .take(8)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    Err(format!(
        "clip `{}` has {} tracks [{tracks}], none match body `{body_name}` bones [{bones}]",
        match_clip.name,
        match_clip.tracks.len(),
    ))
}

pub fn pose_remote_dobj(
    dobj: &xmodel_runtime::DObj,
    runtime: xmodel_runtime::XAnimTreeRuntime,
    controller: Option<xmodel_runtime::PlayerControllerInput>,
) -> Result<Vec<Mat4>, String> {
    let request = xmodel_runtime::DObjPoseRequest::with_tree(runtime);
    xmodel_runtime::pose_dobj_with_controller(dobj, &request, Mat4::IDENTITY, |dobj, _, locals| {
        if let Some(input) = controller {
            xmodel_runtime::apply_player_controller(dobj, locals, input);
        }
    })
    .map_err(|error| format!("{error:?}"))
}

pub fn remote_player_controller(
    is_corpse: bool,
    view_pitch_deg: f32,
    prone: bool,
    crouch: bool,
) -> Option<xmodel_runtime::PlayerControllerInput> {
    if is_corpse {
        None
    } else {
        Some(xmodel_runtime::PlayerControllerInput {
            view_pitch_deg,
            prone,
            crouch,
            lean_frac: 0.0,
        })
    }
}

pub fn remote_dobj_model_base(
    dobj: &xmodel_runtime::DObj,
    slot: usize,
    attached: &str,
) -> Result<usize, String> {
    dobj.models
        .get(slot)
        .map(|model| model.base)
        .ok_or_else(|| format!("{attached} attached but DObj has no model slot {slot}"))
}

pub struct PendingGunSkin<'a> {
    pub entry: &'a asset_model::WorldWeaponEntry,
    pub base: usize,
}

pub struct RemoteSkinModels<'a> {
    pub body: &'a asset_model::BodyMeshEntry,
    pub head: Option<(&'a asset_model::BodyMeshEntry, usize)>,
    pub gun: Option<PendingGunSkin<'a>>,
    pub head_model: Option<u16>,
    pub gun_model: Option<u16>,
    pub attachments: Vec<(PendingGunSkin<'a>, u16)>,
}

pub fn bind_remote_skin_models<'a>(
    dobj: &xmodel_runtime::DObj,
    models: &RemoteModelSet<'a>,
) -> Result<RemoteSkinModels<'a>, String> {
    let head = match models.head {
        None => None,
        Some(head) => Some((head, remote_dobj_model_base(dobj, 1, "head")?)),
    };
    let gun = match models.gun {
        None => None,
        Some(gun) => Some(PendingGunSkin {
            entry: gun,
            base: remote_dobj_model_base(dobj, models.gun_model_index, "gun")?,
        }),
    };
    let attachments = models
        .attachments
        .iter()
        .map(|&(entry, model)| {
            Ok((
                PendingGunSkin {
                    entry,
                    base: remote_dobj_model_base(dobj, model, "attachment")?,
                },
                model as u16,
            ))
        })
        .collect::<Result<_, String>>()?;
    Ok(RemoteSkinModels {
        body: models.body,
        head,
        gun,
        head_model: head.map(|_| 1),
        gun_model: models.gun.map(|_| models.gun_model_index as u16),
        attachments,
    })
}

pub fn skin_matrices_cover_slot(skin_len: usize, base: usize, bone_n: usize) -> Result<(), String> {
    let need = base.saturating_add(bone_n);
    if skin_len < need {
        return Err(format!(
            "skin matrices {skin_len} shorter than slot base {base} + {bone_n} bones"
        ));
    }
    Ok(())
}

pub fn skin_slot_need(
    skel: &asset_model::ModelSkel,
    skin: &[Mat4],
    base: usize,
) -> Result<(), String> {
    skin_matrices_cover_slot(skin.len(), base, skel.bones.len())
}

pub fn attach_radii(body: Option<f32>, head: Option<f32>, gun: Option<f32>) -> (Vec<f32>, Vec<u8>) {
    let mut radii = Vec::new();
    let mut parents = Vec::new();
    if let Some(radius) = body {
        radii.push(radius);
        parents.push(DOBJ_RADIUS_PARENT_ROOT);
        if let Some(radius) = head {
            radii.push(radius);
            parents.push(0);
        }
        if let Some(radius) = gun {
            radii.push(radius);
            parents.push(0);
        }
    }
    (radii, parents)
}

pub fn radii(
    body: &asset_model::BodyMeshEntry,
    head: Option<&asset_model::BodyMeshEntry>,
    gun: Option<&asset_model::WorldWeaponEntry>,
) -> (Vec<f32>, Vec<u8>) {
    attach_radii(
        body.skel.radius,
        head.and_then(|head| head.skel.radius),
        gun.and_then(|gun| gun.skel.radius),
    )
}

#[derive(Component, Default)]
pub struct RemoteBodyState {
    pub(crate) tree: Option<PersistentRemoteTree>,
    pub(crate) skin: RemoteSkinCache,
}

impl RemoteBodyState {
    pub fn has_tree(&self) -> bool {
        self.tree.is_some()
    }
}

#[derive(Resource, Default)]
pub struct RemoteGeometryRevision(pub u64);

#[derive(Default)]
pub struct RemoteSkinCache {
    last: Option<u64>,
    skinned: Option<CachedSkinnedBody>,
    reskinning: Option<(u64, u64)>,
    pub(crate) last_cache_hit: bool,
}

#[derive(Clone, Default)]
pub struct CpuBodyGeom {
    pub skin_jobs: Vec<render_frame::skinning::PackedSkinJob>,
    pub packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    pub indices: Vec<u32>,
    pub decoded_n: usize,
    pub surfaces: Vec<CpuSurfMeta>,
}

#[derive(Clone)]
pub struct CpuSurfMeta {
    pub index_start: u32,
    pub index_count: u32,
    pub material: Option<asset_core::MaterialKey>,
}

pub struct CpuNamedSurface {
    pub vert_n: usize,
    pub indices: Vec<u32>,
    pub material: Option<asset_core::MaterialKey>,
    pub packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
}

#[derive(Clone)]
pub struct CachedSkinnedBody {
    pub geometry_revision: u64,
    pub layout: u64,
    pub geom: Arc<CpuBodyGeom>,
    pub radii: Vec<f32>,
    pub radius_parents: Vec<u8>,

    pub lods: (Option<u8>, Option<u8>, Option<u8>),
}

pub struct RemoteBodySkinnedItem {
    pub geometry_revision: u64,
    pub client: u32,
    pub origin: [f32; 3],
    pub world_from_local: Mat4,
    pub geom: Arc<CpuBodyGeom>,
    pub radii: Vec<f32>,
    pub radius_parents: Vec<u8>,
}

#[derive(Resource, Default)]
pub struct RemoteBodySkinnedQueue {
    items: Vec<RemoteBodySkinnedItem>,
}

impl RemoteBodySkinnedQueue {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RemoteBodySkinnedItem> {
        self.items.iter()
    }

    pub fn take(&mut self) -> Vec<RemoteBodySkinnedItem> {
        std::mem::take(&mut self.items)
    }
}

impl RemoteSkinCache {
    pub fn has_lods(&self, lods: (Option<u8>, Option<u8>, Option<u8>)) -> bool {
        self.skinned
            .as_ref()
            .is_some_and(|cached| cached.lods == lods)
    }
    pub fn remember_pose_hash(&mut self, hash: u64) -> bool {
        self.last.replace(hash) == Some(hash)
    }
}

pub fn take_unique_geom(pose_hashes: &mut RemoteSkinCache) -> CpuBodyGeom {
    let Some(cached) = pose_hashes.skinned.take() else {
        return CpuBodyGeom::default();
    };
    pose_hashes.reskinning = Some((cached.geometry_revision, cached.layout));
    match Arc::try_unwrap(cached.geom) {
        Ok(mut geom) => {
            geom.packed.clear();
            geom.indices.clear();
            geom.surfaces.clear();
            geom.decoded_n = 0;
            geom
        }
        Err(_) => CpuBodyGeom::default(),
    }
}

pub fn geom_layout(geom: &CpuBodyGeom) -> u64 {
    use std::hash::BuildHasher;
    let mut hasher = bevy::platform::hash::FixedHasher.build_hasher();
    hasher.write_usize(geom.decoded_n);
    hasher.write(bytemuck::cast_slice(&geom.indices));
    for surface in &geom.surfaces {
        (surface.index_start, surface.index_count, &surface.material).hash(&mut hasher);
    }
    for job in &geom.skin_jobs {
        (job.shape.id(), job.destination, job.bones.len()).hash(&mut hasher);
        job.offset.map(f32::to_bits).hash(&mut hasher);
        for range in job.ranges.iter() {
            (range.start, range.count, range.rigid).hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn hash_skin_matrices(skin: &[Mat4]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    skin.len().hash(&mut hasher);
    for matrix in skin {
        for lane in matrix.to_cols_array() {
            lane.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn push_cached_surfaces(
    number: u32,
    transform: &Transform,
    pose_hashes: &mut RemoteSkinCache,
    submit: &mut RemoteBodySkinnedQueue,
) {
    pose_hashes.last_cache_hit = true;
    let cached = pose_hashes
        .skinned
        .as_ref()
        .expect("caller checked contains_key");
    submit.items.push(RemoteBodySkinnedItem {
        geometry_revision: cached.geometry_revision,
        client: number,
        origin: transform.translation.to_array(),
        world_from_local: transform.to_matrix(),
        geom: Arc::clone(&cached.geom),
        radii: cached.radii.clone(),
        radius_parents: cached.radius_parents.clone(),
    });
}

pub(crate) struct AssembledRemoteBody {
    pub(crate) entity: Entity,
    pub(crate) number: u32,
    pub(crate) transform: Transform,
    pub(crate) geom: CpuBodyGeom,
    pub(crate) radii: Vec<f32>,
    pub(crate) radius_parents: Vec<u8>,
    pub(crate) lods: (Option<u8>, Option<u8>, Option<u8>),
}

impl AssembledRemoteBody {
    pub(crate) fn commit(
        self,
        submit: &mut RemoteBodySkinnedQueue,
        pose_hashes: &mut RemoteSkinCache,
        revision: &mut RemoteGeometryRevision,
    ) {
        let Self {
            entity: _,
            number,
            transform,
            geom,
            radii,
            radius_parents,
            lods,
        } = self;
        let layout = geom_layout(&geom);
        let geometry_revision = match pose_hashes.reskinning.take() {
            Some((revision, prior)) if prior == layout => revision,
            _ => {
                revision.0 = revision
                    .0
                    .checked_add(1)
                    .expect("remote geometry revision exhausted");
                revision.0
            }
        };
        let geom = Arc::new(geom);
        pose_hashes.skinned = Some(CachedSkinnedBody {
            geometry_revision,
            layout,
            geom: Arc::clone(&geom),
            radii: radii.clone(),
            radius_parents: radius_parents.clone(),
            lods,
        });
        submit.items.push(RemoteBodySkinnedItem {
            geometry_revision,
            client: number,
            origin: transform.translation.to_array(),
            world_from_local: transform.to_matrix(),
            geom,
            radii,
            radius_parents,
        });
    }
}

pub fn named_surfaces(
    surfaces: Vec<PosedSmodelSurface>,
    keys: &[Option<asset_core::MaterialKey>],
    edges: &[assets::AssetEdge<assets::MaterialSpace>],
) -> Vec<CpuNamedSurface> {
    surfaces
        .into_iter()
        .map(|surface| {
            let material = match edges.get(surface.surface_index) {
                Some(assets::AssetEdge::Bound(_)) => {
                    keys.get(surface.surface_index).cloned().flatten()
                }
                _ => None,
            };
            CpuNamedSurface {
                vert_n: surface.vert_n,
                indices: surface.indices,
                material,
                packed: surface.packed_vertices,
            }
        })
        .collect()
}

pub fn flatten_cpu_geom(surfaces: Vec<CpuNamedSurface>) -> CpuBodyGeom {
    let mut packed = Vec::new();
    let mut indices = Vec::new();
    let mut metas = Vec::with_capacity(surfaces.len());
    packed.reserve(surfaces.iter().map(|s| s.packed.len()).sum());
    indices.reserve(surfaces.iter().map(|s| s.indices.len()).sum());
    let mut decoded_n = 0usize;
    let mut packed_ok = true;
    for surface in surfaces {
        if surface.indices.is_empty() || surface.vert_n == 0 {
            metas.push(CpuSurfMeta {
                index_start: 0,
                index_count: 0,
                material: surface.material,
            });
            continue;
        }
        if packed_ok && surface.packed.len() == surface.vert_n {
            packed.extend_from_slice(&surface.packed);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let vert_base = decoded_n as u32;
        let index_start = indices.len() as u32;
        indices.extend(surface.indices.iter().map(|&index| vert_base + index));
        metas.push(CpuSurfMeta {
            index_start,
            index_count: surface.indices.len() as u32,
            material: surface.material,
        });
        decoded_n = decoded_n.saturating_add(surface.vert_n);
    }
    if !packed_ok {
        packed.clear();
    }
    CpuBodyGeom {
        skin_jobs: Vec::new(),
        packed,
        indices,
        decoded_n,
        surfaces: metas,
    }
}
