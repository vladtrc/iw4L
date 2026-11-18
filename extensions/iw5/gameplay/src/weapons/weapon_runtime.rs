use asset_game::*;
pub const POLICY: WeaponRuntimePolicy = WeaponRuntimePolicy {
    presentation_requirements: None,
    alternate_raise_time: None,
    ads_overlay: AdsOverlayConvention::WeightIsFrac,
    appearance: WeaponAppearanceKind::Models,
    attachment_mount_on_root: false,
    hide_mode: FpvHideMode::Surfaces,
    force_projectile_rotation: true,
    projectile_pickup: |_| weapon_iw4::ProjectilePickup::AmmoUse,
    ballistic_blade: |_| false,
    supports: |_| true,
};
