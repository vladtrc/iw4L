use bevy::prelude::*;

use crate::ModelLightingRequest;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModelSurfaceDraw {
    pub surface: u32,
    pub material: u32,
    pub world_from_local: Mat4,
    pub lighting_handle: u32,

    pub pending_lighting: Option<ModelLightingRequest>,

    pub colour_refusal: Option<XModelColourRefusal>,

    pub object_id: u16,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub packed_lighting: Option<[u8; 4]>,

    pub is_scope: bool,

    pub scene_entnum: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XModelColourRefusal {
    CameraFrustum,
}
