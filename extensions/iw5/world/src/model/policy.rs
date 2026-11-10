pub fn classify(name: &str) -> Option<asset_model::ModelKind> {
    asset_model::model_kind(name)
}
pub const POLICY: asset_model::ModelFamily = asset_model::ModelFamily {
    lod: asset_model::REFERENCE_MODEL_LOD_POLICY,
    soldier_hands: asset_model::reference_soldier_hands,
    classify,
    default_hands: "viewmodel_base_viewhands",
};
