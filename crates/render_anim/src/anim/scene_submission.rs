use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimSceneSubmit;

#[derive(Clone, Debug)]
pub struct AnimDObjSceneModel {
    pub lod: i8,
    pub bone_count: u8,
    pub skel: Arc<assets::ModelSkel>,
}

#[derive(Message, Clone, Debug)]
pub struct AnimDObjSceneSubmission {
    pub render_fx_flags: u32,
    pub has_tree: bool,
    pub origin: [f32; 3],
    pub lighting_origin: [f32; 3],
    pub radius: Option<f32>,
    pub entnum: u32,
    pub quat: Option<[f32; 4]>,
    pub occupy_model_n: u8,
    pub models: Vec<AnimDObjSceneModel>,
    pub hide_part_bits: [u32; 6],
    pub store_skin: bool,
}

#[derive(Resource, Default)]
pub struct AnimDObjSceneSkels {
    by_name: HashMap<String, Arc<assets::ModelSkel>>,
}

impl AnimDObjSceneSkels {
    pub fn model(&mut self, name: &str, skel: &assets::ModelSkel, lod: i8) -> AnimDObjSceneModel {
        let skel = match self.by_name.get(name) {
            Some(skel) => Arc::clone(skel),
            None => {
                let skel = Arc::new(skel.clone());
                self.by_name.insert(name.to_owned(), Arc::clone(&skel));
                skel
            }
        };
        AnimDObjSceneModel {
            lod,
            bone_count: u8::try_from(skel.bones.len()).unwrap_or(u8::MAX),
            skel,
        }
    }
}
