use asset_core::FamilyId;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Clone, Copy)]
pub struct ModelFamily {
    pub lod: crate::ModelLodPolicy,
    pub soldier_hands: fn(
        &crate::FpvMeshCatalog,
        FamilyId,
        &crate::SoldierKit,
    ) -> Result<Option<crate::FpvHands>, String>,
    pub classify: fn(&str) -> Option<crate::ModelKind>,
    pub default_hands: &'static str,
}
pub const REFERENCE_MODEL_FAMILY: ModelFamily = ModelFamily {
    lod: crate::REFERENCE_MODEL_LOD_POLICY,
    soldier_hands: reference_soldier_hands,
    classify: crate::model_kind,
    default_hands: crate::VIEWHANDS_NAME,
};
#[derive(Default)]
pub struct ModelFamilies(BTreeMap<FamilyId, ModelFamily>);
impl ModelFamilies {
    pub fn iter(&self) -> impl Iterator<Item = (FamilyId, &ModelFamily)> {
        self.0.iter().map(|(&id, policy)| (id, policy))
    }
    pub fn register(&mut self, id: FamilyId, policy: ModelFamily) {
        assert!(!self.0.contains_key(&id), "duplicate model family");
        self.0.insert(id, policy);
    }
}
static FAMILIES: OnceLock<ModelFamilies> = OnceLock::new();
pub fn install_model_families(families: ModelFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "model families already installed")
}
pub fn model_family(id: FamilyId) -> Option<&'static ModelFamily> {
    FAMILIES.get()?.0.get(&id)
}
pub fn classify_model(id: FamilyId, name: &str) -> Option<crate::ModelKind> {
    (model_family(id)?.classify)(name)
}

pub fn reference_soldier_hands(
    catalog: &crate::FpvMeshCatalog,
    family: FamilyId,
    kit: &crate::SoldierKit,
) -> Result<Option<crate::FpvHands>, String> {
    let choice = crate::FpvHands::resolve(catalog, family, Some(kit), None, family);
    Ok(choice.key().is_some().then_some(choice))
}

pub fn model_families() -> &'static ModelFamilies {
    FAMILIES
        .get()
        .expect("builder must install model families before view construction")
}
