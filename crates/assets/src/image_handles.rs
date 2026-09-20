use std::ops::Deref;
use std::sync::Arc;

use bevy::prelude::*;

use render_material::MaterialGenerationId;

#[derive(Clone, Debug)]
pub struct RuntimeLightmapHandles {
    pub primary: Option<Handle<Image>>,
    pub secondary: Option<Handle<Image>>,
    pub secondary_b: Option<Handle<Image>>,
    pub ambient_diagnostic: Handle<Image>,
    pub directional_diagnostic: Handle<Image>,
    pub sun_mask_diagnostic: Handle<Image>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeImageHandlesData {
    pub generation_id: MaterialGenerationId,
    pub material_images: Vec<Option<Handle<Image>>>,
    pub material_names: Vec<String>,
    pub reflection_probes: Vec<Option<Handle<Image>>>,
    pub lightmaps: Vec<Option<RuntimeLightmapHandles>>,
    pub model_lighting: Option<Handle<Image>>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct RuntimeImageHandles {
    inner: Arc<RuntimeImageHandlesData>,
}

impl Deref for RuntimeImageHandles {
    type Target = RuntimeImageHandlesData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RuntimeImageHandles {
    pub fn from_pools(
        generation_id: MaterialGenerationId,
        material_images: Vec<Option<Handle<Image>>>,
        material_names: Vec<String>,
        reflection_probes: Vec<Option<Handle<Image>>>,
        lightmaps: Vec<Option<RuntimeLightmapHandles>>,
        model_lighting: Option<Handle<Image>>,
    ) -> Self {
        Self {
            inner: Arc::new(RuntimeImageHandlesData {
                generation_id,
                material_images,
                material_names,
                reflection_probes,
                lightmaps,
                model_lighting,
            }),
        }
    }

    pub fn make_mut(&mut self) -> &mut RuntimeImageHandlesData {
        Arc::make_mut(&mut self.inner)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn live_for(&self, expected: MaterialGenerationId) -> Option<&Self> {
        (self.generation_id == expected).then_some(self)
    }
}
