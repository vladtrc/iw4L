use glam::{Mat4, Vec3};

use crate::xanim_tree::{ActiveAdditiveLayer, ActiveXAnimLeaf};
use crate::{
    AnimInstance, Attach, DObj, DObjError, HidePartBits, Local, ModelPoseSrc, PartBits,
    XAnimTreeError, XAnimTreeRuntime,
};
use anim_iw4::xanim_apply_additive;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneCollision {
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub radius_sq: f32,
    pub part_classification: u8,
}

#[derive(Clone, Debug, Default)]
pub struct DObjPoseRequest {
    pub tree: Option<XAnimTreeRuntime>,
    pub requested_parts: Option<PartBits>,
    pub hide_part_bits: HidePartBits,
}

impl DObjPoseRequest {
    pub const fn bind_pose() -> Self {
        Self {
            tree: None,
            requested_parts: None,
            hide_part_bits: HidePartBits::from_words([0; PartBits::WORDS]),
        }
    }

    pub fn with_tree(tree: XAnimTreeRuntime) -> Self {
        Self {
            tree: Some(tree),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct RetainedModelCapability {
    pub key: String,
    pub pose: ModelPoseSrc,
    pub bone_collision: Vec<Option<BoneCollision>>,

    pub contents: Option<u32>,

    pub coll_lod: i16,

    pub coll_surfs: Vec<CollSurfCollision>,

    pub bounds: Option<([f32; 3], [f32; 3])>,

    pub radius: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollTri {
    pub plane: [f32; 4],
    pub svec: [f32; 4],
    pub tvec: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollSurfCollision {
    pub bone: u16,
    pub contents: u32,
    pub surf_flags: u32,
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub tris: Vec<CollTri>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionBone {
    pub bone: u16,
    pub part_classification: u8,
    pub center: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub half_size: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializeError {
    DObj(String),
    AnimatedClipHasNoMatchingTrack,
    NonFiniteBoneTransform { bone: usize },
    BoneIndexOverflow { bone: usize },
    XAnimTree(XAnimTreeError),
}

impl From<DObjError> for MaterializeError {
    fn from(value: DObjError) -> Self {
        Self::DObj(value.to_string())
    }
}

impl From<XAnimTreeError> for MaterializeError {
    fn from(value: XAnimTreeError) -> Self {
        Self::XAnimTree(value)
    }
}

impl RetainedModelCapability {
    pub fn pose(
        &self,
        request: &DObjPoseRequest,
        world_from_model: Mat4,
    ) -> Result<Vec<Mat4>, MaterializeError> {
        self.pose_with_controller(request, world_from_model, |_, _, _| {})
    }

    pub fn pose_with_controller(
        &self,
        request: &DObjPoseRequest,
        world_from_model: Mat4,
        controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
    ) -> Result<Vec<Mat4>, MaterializeError> {
        let dobj = DObj::build(&[(&self.pose, None)])?;
        pose_dobj_with_controller(&dobj, request, world_from_model, controller)
    }

    pub fn collision(
        &self,
        request: &DObjPoseRequest,
        world_from_model: Mat4,
    ) -> Result<Vec<CollisionBone>, MaterializeError> {
        collision_models(&[(self, None)], request, world_from_model)
    }

    pub fn geom_collision(
        &self,
        request: &DObjPoseRequest,
        world_from_model: Mat4,
        contentmask: u32,
    ) -> Result<Vec<CollisionBone>, MaterializeError> {
        if self.coll_surfs.is_empty() {
            return self.collision(request, world_from_model);
        }
        geom_collision_models(&[(self, None)], request, world_from_model, contentmask)
    }

    pub fn bounds_collision_bone(&self, world_from_model: Mat4) -> Option<CollisionBone> {
        let (mid, half) = match self.bounds {
            Some((mid, half)) if half.iter().any(|&h| h > 0.0) => (mid, half),
            _ => {
                let radius = self.radius.filter(|&r| r.is_finite() && r > 0.0)?;
                ([0.0; 3], [radius, radius, radius])
            }
        };
        collision_bone_from_local_box(0, mid, half, world_from_model)
    }
}

pub fn collision_bone_from_local_box(
    bone: u16,
    mid: [f32; 3],
    half: [f32; 3],
    world_from_model: Mat4,
) -> Option<CollisionBone> {
    if !mid.iter().all(|v| v.is_finite()) || !half.iter().all(|h| h.is_finite() && *h >= 0.0) {
        return None;
    }
    if half.iter().all(|&h| h == 0.0) {
        return None;
    }
    let center = world_from_model
        .transform_point3(Vec3::from_array(mid))
        .to_array();
    if !center.iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut axes = [[0.0; 3]; 3];
    let mut half_size = [0.0; 3];
    for axis in 0..3 {
        let direction = world_from_model.transform_vector3(Vec3::AXES[axis]);
        let scale = direction.length();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        axes[axis] = (direction / scale).to_array();
        half_size[axis] = half[axis] * scale;
    }
    Some(CollisionBone {
        bone,
        part_classification: 0,
        center,
        axes,
        half_size,
    })
}

pub fn collision_models(
    models: &[(&RetainedModelCapability, Option<Attach>)],
    request: &DObjPoseRequest,
    world_from_model: Mat4,
) -> Result<Vec<CollisionBone>, MaterializeError> {
    collision_models_with_controller(models, request, world_from_model, |_, _, _| {})
}

pub fn collision_models_with_controller(
    models: &[(&RetainedModelCapability, Option<Attach>)],
    request: &DObjPoseRequest,
    world_from_model: Mat4,
    controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
) -> Result<Vec<CollisionBone>, MaterializeError> {
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let descriptors: Vec<(&ModelPoseSrc, Option<Attach>)> = models
        .iter()
        .map(|(model, attach)| (&model.pose, attach.clone()))
        .collect();
    let dobj = DObj::build(&descriptors)?;
    collision_dobj_with_controller(&dobj, models, request, world_from_model, controller)
}

pub fn collision_dobj_with_controller(
    dobj: &DObj,
    models: &[(&RetainedModelCapability, Option<Attach>)],
    request: &DObjPoseRequest,
    world_from_model: Mat4,
    controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
) -> Result<Vec<CollisionBone>, MaterializeError> {
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let posed = pose_dobj_with_controller(dobj, request, world_from_model, controller)?;
    let mut bones = Vec::new();
    for (slot, (model, _)) in dobj.models.iter().zip(models.iter()) {
        for local in 0..slot.bone_count {
            let Some(collision) = model
                .bone_collision
                .get(local)
                .and_then(|entry| entry.as_ref())
            else {
                continue;
            };
            let bone_index = slot.base + local;
            let Some(world_from_bone) = posed.get(bone_index).copied() else {
                continue;
            };
            let center = world_from_bone
                .transform_point3(Vec3::from_array(collision.midpoint))
                .to_array();
            let mut axes = [[0.0; 3]; 3];
            let mut half_size = [0.0; 3];
            for axis in 0..3 {
                let direction = world_from_bone.transform_vector3(Vec3::AXES[axis]);
                let scale = direction.length();
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(MaterializeError::NonFiniteBoneTransform { bone: bone_index });
                }
                axes[axis] = (direction / scale).to_array();
                half_size[axis] = collision.half_size[axis] * scale;
            }
            let bone = u16::try_from(bone_index)
                .map_err(|_| MaterializeError::BoneIndexOverflow { bone: bone_index })?;
            bones.push(CollisionBone {
                bone,
                part_classification: collision.part_classification,
                center,
                axes,
                half_size,
            });
        }
    }
    Ok(bones)
}

fn geom_collision_models(
    models: &[(&RetainedModelCapability, Option<Attach>)],
    request: &DObjPoseRequest,
    world_from_model: Mat4,
    contentmask: u32,
) -> Result<Vec<CollisionBone>, MaterializeError> {
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let descriptors: Vec<(&ModelPoseSrc, Option<Attach>)> = models
        .iter()
        .map(|(model, attach)| (&model.pose, attach.clone()))
        .collect();
    let dobj = DObj::build(&descriptors)?;
    let posed = pose_dobj_with_controller(&dobj, request, world_from_model, |_, _, _| {})?;
    let mut bones = Vec::new();
    for (slot, (model, _)) in dobj.models.iter().zip(models.iter()) {
        for surf in &model.coll_surfs {
            if surf.contents & contentmask == 0 {
                continue;
            }
            let local = usize::from(surf.bone);
            let bone_index = slot.base + local;
            let Some(world_from_bone) = posed.get(bone_index).copied() else {
                continue;
            };
            let center = world_from_bone
                .transform_point3(Vec3::from_array(surf.midpoint))
                .to_array();
            let mut axes = [[0.0; 3]; 3];
            let mut half_size = [0.0; 3];
            for axis in 0..3 {
                let direction = world_from_bone.transform_vector3(Vec3::AXES[axis]);
                let scale = direction.length();
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(MaterializeError::NonFiniteBoneTransform { bone: bone_index });
                }
                axes[axis] = (direction / scale).to_array();
                half_size[axis] = surf.half_size[axis] * scale;
            }
            let bone = u16::try_from(bone_index)
                .map_err(|_| MaterializeError::BoneIndexOverflow { bone: bone_index })?;
            bones.push(CollisionBone {
                bone,
                part_classification: 0,
                center,
                axes,
                half_size,
            });
        }
    }
    Ok(bones)
}

fn locals_from_leaves(
    dobj: &DObj,
    leaves: &[ActiveXAnimLeaf<'_>],
    requested: &PartBits,
) -> Result<Vec<Local>, MaterializeError> {
    let bindings = leaves
        .iter()
        .map(|leaf| dobj.tracks_for(leaf.clip))
        .collect::<Vec<_>>();
    if !leaves.is_empty() && !bindings.iter().flatten().any(Option::is_some) {
        return Err(MaterializeError::AnimatedClipHasNoMatchingTrack);
    }
    let instances = leaves
        .iter()
        .zip(&bindings)
        .map(|(leaf, tracks)| AnimInstance {
            clip: leaf.clip,
            tracks,
            time: leaf.time,
            weight: leaf.weight,
            parts: leaf.parts,
        })
        .collect::<Vec<_>>();
    Ok(dobj.calc_anim(&instances, requested))
}

fn apply_additive_layers(
    dobj: &DObj,
    requested: &PartBits,
    dest: &mut [Local],
    layers: &[ActiveAdditiveLayer<'_>],
) -> Result<Vec<bool>, MaterializeError> {
    let mut written = vec![false; dest.len()];
    for layer in layers {
        let (mut add, controlled) = if layer.leaves.is_empty() {
            (dobj.bind_locals(), vec![false; dest.len()])
        } else {
            let bindings = layer
                .leaves
                .iter()
                .map(|leaf| dobj.tracks_for(leaf.clip))
                .collect::<Vec<_>>();
            if !bindings.iter().flatten().any(Option::is_some) {
                return Err(MaterializeError::AnimatedClipHasNoMatchingTrack);
            }
            let instances = layer
                .leaves
                .iter()
                .zip(&bindings)
                .map(|(leaf, tracks)| AnimInstance {
                    clip: leaf.clip,
                    tracks,
                    time: leaf.time,
                    weight: leaf.weight,
                    parts: leaf.parts,
                })
                .collect::<Vec<_>>();
            dobj.calc_anim_masked(&instances, requested)
        };
        let inner_written = apply_additive_layers(dobj, requested, &mut add, &layer.inner)?;
        for i in 0..dest.len() {
            if !controlled.get(i).copied().unwrap_or(false)
                && !inner_written.get(i).copied().unwrap_or(false)
            {
                continue;
            }
            let (rot, trans) = xanim_apply_additive(
                dest[i].rotation,
                dest[i].translation,
                add[i].rotation,
                add[i].translation,
                layer.weight,
            );
            dest[i].rotation = rot;
            dest[i].translation = trans;
            written[i] = true;
        }
    }
    Ok(written)
}

pub fn pose_dobj(
    dobj: &DObj,
    request: &DObjPoseRequest,
    world_from_model: Mat4,
) -> Result<Vec<Mat4>, MaterializeError> {
    pose_dobj_with_controller(dobj, request, world_from_model, |_, _, _| {})
}

pub fn pose_dobj_with_controller(
    dobj: &DObj,
    request: &DObjPoseRequest,
    world_from_model: Mat4,
    controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
) -> Result<Vec<Mat4>, MaterializeError> {
    let requested = request.requested_parts.unwrap_or_else(|| dobj.all_parts());
    let mut locals = if let Some(tree) = &request.tree {
        let (leaves, layers) = tree.active_pose()?;
        let mut locals = locals_from_leaves(dobj, &leaves, &requested)?;
        apply_additive_layers(dobj, &requested, &mut locals, &layers)?;
        locals
    } else {
        dobj.bind_locals()
    };
    controller(dobj, &requested, &mut locals);
    dobj.apply_duplicates(&mut locals);
    Ok(dobj.compose(&locals, world_from_model))
}
