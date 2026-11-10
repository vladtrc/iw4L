pub fn classify(name: &str) -> Option<asset_model::ModelKind> {
    asset_model::model_kind(name)
}
pub const POLICY: asset_model::ModelFamily = asset_model::ModelFamily {
    lod: asset_model::REFERENCE_MODEL_LOD_POLICY,
    soldier_hands,
    classify,
    default_hands: "viewmodel_base_viewhands",
};

fn soldier_hands(
    _: &asset_model::FpvMeshCatalog,
    family: asset_core::FamilyId,
    kit: &asset_model::SoldierKit,
) -> Result<Option<asset_model::FpvHands>, String> {
    kit.arms
        .as_ref()
        .map(|name| asset_model::FpvHands::FromKit {
            namespace: family,
            name: name.clone(),
        })
        .map(Some)
        .ok_or_else(|| "T6 soldier kit hands missing".to_owned())
}
