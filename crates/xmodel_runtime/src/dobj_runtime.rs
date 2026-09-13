use glam::Mat4;

use std::hash::{Hash, Hasher};

use crate::{
    Attach, CollisionBone, DObj, DObjError, DObjPoseRequest, Local, MaterializeError, ModelPoseSrc,
    PartBits, RetainedModelCapability, XAnimTreeRuntime, collision_dobj_with_controller,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DObjReuseKey {
    pub e_type: i32,
    pub model: i32,
}

pub fn dobj_reuse_matches(cached: DObjReuseKey, current: DObjReuseKey) -> bool {
    cached.e_type == current.e_type && cached.model == current.model
}

pub fn dobj_model_token(parts: &[&str]) -> i32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    (hasher.finish() & 0x7fff_ffff) as i32
}

#[derive(Clone, Debug)]
pub struct DObjAnimRuntime {
    pub dobj: DObj,
    pub tree: XAnimTreeRuntime,
    pub reuse_key: Option<DObjReuseKey>,
}

impl DObjAnimRuntime {
    pub fn compose(
        models: &[(&ModelPoseSrc, Option<Attach>)],
        tree: XAnimTreeRuntime,
        reuse_key: Option<DObjReuseKey>,
    ) -> Result<Self, DObjError> {
        Ok(Self {
            dobj: DObj::build(models)?,
            tree,
            reuse_key,
        })
    }

    pub fn update(&mut self, dtime_seconds: f32) -> Result<(), crate::XAnimTreeError> {
        self.tree.update(dtime_seconds)
    }

    pub fn pose_request(&self) -> DObjPoseRequest {
        DObjPoseRequest::with_tree(self.tree.clone())
    }

    pub fn collision(
        &self,
        models: &[(&RetainedModelCapability, Option<Attach>)],
        world_from_model: Mat4,
        controller: impl FnOnce(&DObj, &PartBits, &mut [Local]),
    ) -> Result<Vec<CollisionBone>, MaterializeError> {
        collision_dobj_with_controller(
            &self.dobj,
            models,
            &self.pose_request(),
            world_from_model,
            controller,
        )
    }
}
