#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponItemAmmoFacts {
    pub clip_size: i32,
    pub total_min: i32,
    pub total_max: i32,
    pub clip_percent_min: i32,
    pub clip_percent_max: i32,
    pub shared_ammo: bool,
    pub allow_dual_wield: bool,
}
