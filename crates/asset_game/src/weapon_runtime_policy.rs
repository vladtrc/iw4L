#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdsOverlayConvention {
    #[default]
    WeightIsFrac,

    PlayAdsAnim,
}

impl AdsOverlayConvention {
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::WeightIsFrac => "weight_is_frac",
            Self::PlayAdsAnim => "play_ads_anim",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FpvHideMode {
    #[default]
    Surfaces,
    Bones,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponAppearanceKind {
    Models,
    Materials,
}

#[derive(Clone, Copy)]
pub struct WeaponSupportInputs<'a> {
    pub dual_wield: bool,
    pub secondary_model: bool,
    pub left_idle: bool,
    pub gun_model: Option<&'a str>,
}
#[derive(Clone, Copy)]
pub struct WeaponPresentationInputs<'a> {
    pub has_weapon: &'a dyn Fn(asset_core::FamilyId, fn(&str) -> bool) -> bool,
    pub effect_available: &'a dyn Fn(&str, &str) -> bool,
    pub sound_available: &'a dyn Fn(&str) -> bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponPresentationRequirement {
    pub key: String,
    pub source: &'static str,
    pub available: bool,
}
#[derive(Clone, Copy)]
pub struct WeaponRuntimePolicy {
    pub presentation_requirements:
        Option<for<'a> fn(WeaponPresentationInputs<'a>) -> Vec<WeaponPresentationRequirement>>,
    pub alternate_raise_time: Option<fn(i32, f32) -> Option<i32>>,
    pub ads_overlay: AdsOverlayConvention,
    pub appearance: WeaponAppearanceKind,
    pub attachment_mount_on_root: bool,
    pub hide_mode: FpvHideMode,
    pub force_projectile_rotation: bool,
    pub projectile_pickup: fn(&str) -> weapon_iw4::ProjectilePickup,
    pub ballistic_blade: fn(&str) -> bool,
    pub supports: for<'a> fn(WeaponSupportInputs<'a>) -> bool,
}
pub const REFERENCE_WEAPON_RUNTIME: WeaponRuntimePolicy = WeaponRuntimePolicy {
    presentation_requirements: None,
    ads_overlay: AdsOverlayConvention::WeightIsFrac,
    alternate_raise_time: None,
    appearance: WeaponAppearanceKind::Models,
    attachment_mount_on_root: false,
    hide_mode: FpvHideMode::Surfaces,
    force_projectile_rotation: false,
    projectile_pickup: |_| weapon_iw4::ProjectilePickup::AmmoUse,
    ballistic_blade: |_| false,
    supports: |_| true,
};
