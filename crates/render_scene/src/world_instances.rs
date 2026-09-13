use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct WorldScriptModelInstance {
    pub id: assets::ScriptModelId,

    pub authority_owner: Option<sim::AuthorityModelOwner>,
    pub current_model: assets::MapXModelAssetKey,
    pub transform: Transform,
    pub lighting_origin: [f32; 3],
    pub dobj_state: assets::dobj::DObjSemanticState,
    pub metadata: assets::ScriptModelMetadata,

    pub gentity_number: Option<u16>,
}

#[derive(Component, Clone, Debug)]
pub struct WorldDynEntInstance {
    pub index: u16,
    pub ty: assets::DynEntType,
    pub current_model: assets::MapXModelAssetKey,
    pub transform: Transform,
    pub lighting_origin: [f32; 3],

    pub phys_preset: Option<assets::OwnedPhysPreset>,

    pub health: i32,

    pub destroy_fx: Option<String>,

    pub dead: bool,
}
