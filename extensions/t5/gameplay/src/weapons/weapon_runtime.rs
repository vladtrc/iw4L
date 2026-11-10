use asset_game::*;
pub const POLICY: WeaponRuntimePolicy = WeaponRuntimePolicy {
    presentation_requirements: None,
    alternate_raise_time: None,
    ads_overlay: AdsOverlayConvention::PlayAdsAnim,
    appearance: WeaponAppearanceKind::Materials,
    attachment_mount_on_root: false,
    hide_mode: FpvHideMode::Surfaces,
    force_projectile_rotation: true,
    projectile_pickup: |_| weapon_iw4::ProjectilePickup::ItemUse,
    ballistic_blade: |name| name == "knife_ballistic",
    supports,
};
fn supports(input: WeaponSupportInputs<'_>) -> bool {
    if input.dual_wield {
        return input.secondary_model && input.left_idle;
    }
    !input.gun_model.is_some_and(|name| {
        name.ends_with("_dw_rh") || name.ends_with("_dw_lh") || name.ends_with("_lh_viewmodel")
    })
}
