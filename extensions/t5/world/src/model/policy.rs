pub fn classify(name: &str) -> Option<asset_model::ModelKind> {
    if name.starts_with("t5_weapon_") && name.ends_with("_world") {
        return Some(asset_model::ModelKind::WorldWeapon);
    }
    asset_model::model_kind(name)
}
pub const POLICY: asset_model::ModelFamily = asset_model::ModelFamily {
    lod: super::lod::POLICY,
    soldier_hands: asset_model::reference_soldier_hands,
    classify,
    default_hands: "viewhands_usmc",
};
