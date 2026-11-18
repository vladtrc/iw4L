use asset_game::{WeaponPresentationInputs, WeaponPresentationRequirement};
pub fn requirements(inputs: WeaponPresentationInputs<'_>) -> Vec<WeaponPresentationRequirement> {
    if !(inputs.has_weapon)(crate::ID, |name| name.contains("crossbow")) {
        return Vec::new();
    }
    let effect = "weapon/crossbow/fx_trail_crossbow_blink_grn_os";
    vec![
        WeaponPresentationRequirement {
            key: format!("t5:{effect}"),
            source: "t6 crossbow warning",
            available: (inputs.effect_available)("t5", effect),
        },
        WeaponPresentationRequirement {
            key: "t5:wpn_crossbow_alert".into(),
            source: "t6 crossbow warning",
            available: (inputs.sound_available)("t5:wpn_crossbow_alert"),
        },
    ]
}
