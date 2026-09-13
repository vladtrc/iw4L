use std::sync::Arc;

use bevy::prelude::*;
use render_material::{PreparedMaterialTable, RuntimeMaterialCatalog};

#[derive(Resource, Clone, Debug, Default)]
pub struct TessMaterials {
    pub catalog: Arc<RuntimeMaterialCatalog>,
    pub prepared: Arc<PreparedMaterialTable>,

    pub material_images: Arc<Vec<Option<Handle<Image>>>>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldPresentFacts {
    pub spawned: bool,
}
