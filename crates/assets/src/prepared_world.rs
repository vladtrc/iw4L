use asset_world::{IntermissionView, WorldDraw, WorldDrawPolicy};
use std::sync::Arc;
#[derive(Clone)]
pub struct PreparedWorld {
    pub source_namespace: Option<asset_core::FamilyId>,
    pub draw: Option<WorldDraw>,
    pub dynamic_light: Option<asset_world::ResolvedLightDef>,
    pub static_model_meshes: Vec<asset_world::ModelMesh>,

    pub static_model_instances: Vec<Option<asset_world::StaticModelPlacement>>,

    pub map_xmodel_scene_assets: asset_world::MapXModelSceneCatalog,

    pub script_model_instances: Vec<asset_world::ScriptModelSceneInstance>,

    pub script_brush_models: Vec<asset_world::ScriptBrushModelPlacement>,

    pub flag_descriptors: Vec<asset_world::FlagDescriptor>,

    pub script_structs: Vec<asset_world::MapScriptStruct>,

    pub dyn_ents: asset_world::DynEntCatalog,

    pub smodel_lighting_samples: Vec<asset_model::SmodelLightingSample>,

    pub light_grid: Option<asset_model::OwnedLightGrid>,

    pub fx: asset_game::FxCatalog,

    pub fx_models: asset_game::FxModelCatalog,

    pub fx_glass: Option<asset_world::FxGlassReset>,

    pub map_content: Arc<asset_core::MapContentDefinition>,

    pub impact_fx: Option<asset_game::OwnedFxImpactTable>,
    pub family_impact_fx: Vec<asset_game::OwnedFxImpactTable>,
    pub reflection_probe_images: Vec<Option<bevy::prelude::Image>>,
    pub intermission_view: Option<IntermissionView>,

    pub exp_fog: Option<asset_world::ExpFog>,
    pub t6_vision: Option<asset_world::T6Vision>,
    pub t6_visions: asset_world::T6VisionCatalog,

    pub film_vision: Option<asset_world::FilmVision>,
    pub film_visions: std::collections::BTreeMap<
        String,
        Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
    >,

    pub createart_name: Option<String>,
    pub min: [f32; 3],
    pub max: [f32; 3],

    pub world_bounds: Option<[f32; 6]>,
    pub policy: WorldDrawPolicy,
}
impl PreparedWorld {
    pub fn empty(policy: WorldDrawPolicy) -> Self {
        Self {
            source_namespace: Some(policy.family),
            draw: Default::default(),
            dynamic_light: Default::default(),
            static_model_meshes: Default::default(),
            static_model_instances: Default::default(),
            map_xmodel_scene_assets: Default::default(),
            script_model_instances: Default::default(),
            script_brush_models: Default::default(),
            flag_descriptors: Default::default(),
            script_structs: Default::default(),
            dyn_ents: Default::default(),
            smodel_lighting_samples: Default::default(),
            light_grid: Default::default(),
            fx: Default::default(),
            fx_models: Default::default(),
            fx_glass: Default::default(),
            map_content: Default::default(),
            impact_fx: Default::default(),
            family_impact_fx: Default::default(),
            reflection_probe_images: Default::default(),
            intermission_view: Default::default(),
            exp_fog: Default::default(),
            t6_vision: Default::default(),
            t6_visions: Default::default(),
            film_vision: Default::default(),
            film_visions: Default::default(),
            createart_name: Default::default(),
            min: Default::default(),
            max: Default::default(),
            world_bounds: Default::default(),
            policy,
        }
    }
}
