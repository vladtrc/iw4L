use super::{FrameStep, LocalLightWriter, MaterialLocalLightInputs};
use asset_core::FamilyId;
use bevy::math::Vec3;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

pub trait LocalLightProducer: std::fmt::Debug + Send + Sync {
    fn produce(
        &self,
        writer: &mut LocalLightWriter<'_>,
        light: &MaterialLocalLightInputs,
        eye: Vec3,
        time: f32,
    );
}
#[derive(Clone, Copy)]
pub struct MaterialBindingFamily {
    pub prepare_frame: fn(&[u16]) -> Vec<FrameStep>,
    pub prepare_local_light: fn(&[u16]) -> Vec<Arc<dyn LocalLightProducer>>,
}
#[derive(Default)]
pub struct MaterialBindingFamilies(BTreeMap<FamilyId, MaterialBindingFamily>);
impl MaterialBindingFamilies {
    pub fn register(&mut self, id: FamilyId, family: MaterialBindingFamily) {
        assert!(
            !self.0.contains_key(&id),
            "duplicate material binding family"
        );
        self.0.insert(id, family);
    }
    pub fn iter(&self) -> impl Iterator<Item = (FamilyId, &MaterialBindingFamily)> {
        self.0.iter().map(|(&id, family)| (id, family))
    }
}
static FAMILIES: OnceLock<MaterialBindingFamilies> = OnceLock::new();
pub fn install_material_binding_families(
    families: MaterialBindingFamilies,
) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "material binding families already installed")
}
pub fn material_binding_families() -> &'static MaterialBindingFamilies {
    FAMILIES
        .get()
        .expect("builder must install material binding families")
}
pub const REFERENCE_MATERIAL_BINDINGS: MaterialBindingFamily = MaterialBindingFamily {
    prepare_frame: |requested| {
        let mut steps = Vec::new();
        super::sky::prepare(
            &mut steps,
            requested,
            &[
                (18, super::float4_bits([1.0, 0.0, 0.0, 0.0])),
                (19, super::float4_bits([0.0, 1.0, 0.0, 0.0])),
                (20, super::float4_bits([0.0, 0.0, 1.0, 0.0])),
            ],
            &[],
        );
        steps
    },
    prepare_local_light: |_| Vec::new(),
};
