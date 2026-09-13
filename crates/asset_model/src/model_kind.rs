#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    Fpv,
    Soldier,

    WorldWeapon,
}

fn is_t5_world_weapon(name: &str) -> bool {
    name.starts_with("t5_weapon_") && name.ends_with("_world")
}

pub fn model_kind(name: &str) -> Option<ModelKind> {
    if name.starts_with("viewmodel_")
        || name.starts_with("viewhands_")
        || name.contains("_viewmodel")
    {
        Some(ModelKind::Fpv)
    } else if name.starts_with("weapon_") || is_t5_world_weapon(name) {
        Some(ModelKind::WorldWeapon)
    } else if name.starts_with("mp_body_")
        || name.starts_with("head_")
        || name.contains("_mp_body_")
        || name.contains("_mp_head_")
    {
        Some(ModelKind::Soldier)
    } else {
        None
    }
}
