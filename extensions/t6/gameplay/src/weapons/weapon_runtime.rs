use asset_game::*;
pub const POLICY: WeaponRuntimePolicy = WeaponRuntimePolicy {
    presentation_requirements: Some(super::weapon_dependencies::requirements),
    alternate_raise_time: Some(alternate_raise_time),
    ads_overlay: AdsOverlayConvention::WeightIsFrac,
    appearance: WeaponAppearanceKind::Materials,
    attachment_mount_on_root: true,
    hide_mode: FpvHideMode::Bones,
    force_projectile_rotation: true,
    projectile_pickup: |name| {
        if name == "hatchet" {
            weapon_iw4::ProjectilePickup::ItemTouch
        } else {
            weapon_iw4::ProjectilePickup::ItemUse
        }
    },
    ballistic_blade: |_| false,
    supports: |_| true,
};

fn alternate_raise_time(authored: i32, duration: f32) -> Option<i32> {
    let clip_ms = (duration * 1000.0).round() as i32;
    (authored < clip_ms / 2).then_some(clip_ms)
}
