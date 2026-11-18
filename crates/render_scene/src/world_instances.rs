use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct WorldScriptModelInstance {
    pub world: asset_core::atlas::WorldId,
    pub id: asset_world::ScriptModelId,

    pub authority_owner: Option<sim::AuthorityModelOwner>,
    pub current_model: asset_world::MapXModelAssetKey,
    pub transform: Transform,
    pub lighting_origin: [f32; 3],
    pub dobj_state: xmodel_runtime::DObjSemanticState,
    pub metadata: asset_world::ScriptModelMetadata,

    pub gentity_number: Option<u16>,
}

impl WorldScriptModelInstance {
    pub fn apply_simulation_pose(
        &mut self,
        content: &sim::SimContent,
        origin: [f32; 3],
        angles: [f32; 3],
        scale: Vec3,
    ) -> Option<Transform> {
        let matrix = content.simulation_model_transform(origin, angles)?;
        self.world = if content.atlas().is_some() {
            asset_core::world_frame::SimPoint::from_slab(origin).world()
        } else {
            asset_core::atlas::WorldId(0)
        };
        let mut pose = Transform::from_matrix(matrix);
        pose.scale = scale;
        Some(pose)
    }

    pub fn lighting_origin_at(&self, pose: &Transform) -> [f32; 3] {
        let local = self
            .transform
            .to_matrix()
            .inverse()
            .transform_point3(Vec3::from_array(self.lighting_origin));
        pose.transform_point(local).to_array()
    }
}

#[derive(Component, Clone, Debug)]
pub struct WorldDynEntInstance {
    pub world: asset_core::atlas::WorldId,
    pub index: u16,
    pub ty: asset_world::DynEntType,
    pub current_model: asset_world::MapXModelAssetKey,
    pub transform: Transform,
    pub lighting_origin: [f32; 3],

    pub phys_preset: Option<asset_world::OwnedPhysPreset>,

    pub health: i32,

    pub destroy_fx: Option<String>,

    pub dead: bool,
}
