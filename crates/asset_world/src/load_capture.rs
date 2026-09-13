use bevy::prelude::Image;

use crate::{
    ClipCollision, DynEntCatalog, ExpFog, FlagDescriptor, FxGlassReset, IntermissionView,
    MapCompassDeclaration, MapUseTrigger, MapXModelSceneCatalog, MinimapCorners, ModelMesh,
    ScriptBrushModelPlacement, ScriptModelSceneInstance, SpawnPoint, StaticModelPlacement,
    WorldDraw,
};
use asset_model::{OwnedLightGrid, SmodelLightingSample};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDrawPolicy {
    pub resolve_specular_env: bool,
    pub lightmap_requires_image: bool,
    pub decode_color_at_convert: bool,
}

impl Default for WorldDrawPolicy {
    fn default() -> Self {
        Self::iw4()
    }
}

impl WorldDrawPolicy {
    pub const fn iw4() -> Self {
        Self {
            resolve_specular_env: true,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
        }
    }

    pub const fn t5() -> Self {
        Self {
            resolve_specular_env: false,
            lightmap_requires_image: true,
            decode_color_at_convert: true,
        }
    }

    pub const fn iw5() -> Self {
        Self {
            resolve_specular_env: false,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
        }
    }
}

#[derive(Default)]
pub struct WorldLoadCapture {
    pub draw: Option<WorldDraw>,
    pub collision: Option<ClipCollision>,
    pub spawns: Vec<SpawnPoint>,
    pub static_model_meshes: Vec<ModelMesh>,
    pub static_model_instances: Vec<Option<StaticModelPlacement>>,
    pub map_xmodel_scene_assets: MapXModelSceneCatalog,
    pub script_model_instances: Vec<ScriptModelSceneInstance>,
    pub script_brush_models: Vec<ScriptBrushModelPlacement>,
    pub map_use_triggers: Vec<MapUseTrigger>,
    pub flag_descriptors: Vec<FlagDescriptor>,
    pub dyn_ents: DynEntCatalog,
    pub smodel_lighting_samples: Vec<SmodelLightingSample>,
    pub light_grid: Option<OwnedLightGrid>,
    pub fx_glass: Option<FxGlassReset>,
    pub reflection_probe_images: Vec<Option<Image>>,
    pub intermission_view: Option<IntermissionView>,
    pub minimap_corners: Option<MinimapCorners>,
    pub north_yaw: Option<f32>,
    pub compass: MapCompassDeclaration,
    pub exp_fog: Option<ExpFog>,
    pub film_vision: Option<crate::FilmVision>,
    pub createart_name: Option<String>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub world_bounds: Option<[f32; 6]>,
    pub policy: WorldDrawPolicy,
}
