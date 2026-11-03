/// Normalized weapon fields a native configuration composer can edit before binding.
#[derive(Clone, Debug)]
pub struct WeaponConfigurationDraft {
    pub name: String,
    pub hide_tags: Vec<String>,
    pub attachment_view_ads_models: Vec<(String, String)>,
    pub attachment_caption_keys: Vec<String>,
    pub indexed_name: bool,
    pub alternate_index: u32,
    pub prepared_attachments: Vec<String>,
    pub namespace: crate::FamilyId,
    pub alternate_weapon: Option<String>,
    pub facts: crate::WeaponBodyFacts,
    pub attachment_assets: Vec<Option<String>>,
    pub native_capture: crate::WeaponNativeCapture,
    pub attachment_view_models: Vec<String>,
    pub attachment_world_models: Vec<String>,
    pub reticle: crate::WeaponReticleAssets,
    pub overlay_material: Option<String>,
    pub overlay_image: Option<String>,
    pub overlay_material_from_slot: bool,
    pub projectile_model: Option<String>,
    pub combat_fx: crate::WeaponCombatFx,
    pub proj_trail: Option<String>,
    pub proj_trail_from_slot: bool,
    pub proj_ignition: Option<String>,
    pub proj_ignition_from_slot: bool,
    pub sounds: crate::WeaponSoundAliases,
    pub sz_xanims: [Option<String>; weapon_iw4::WEAPON_ANIM_SLOTS],
}
impl WeaponConfigurationDraft {
    pub fn new(namespace: crate::FamilyId, name: String) -> Self {
        Self {
            name,
            hide_tags: Vec::new(),
            attachment_view_ads_models: Vec::new(),
            attachment_caption_keys: Vec::new(),
            indexed_name: true,
            alternate_index: 0,
            prepared_attachments: Vec::new(),
            namespace,
            alternate_weapon: Default::default(),
            facts: Default::default(),
            attachment_assets: Default::default(),
            native_capture: Default::default(),
            attachment_view_models: Default::default(),
            attachment_world_models: Default::default(),
            reticle: Default::default(),
            overlay_material: Default::default(),
            overlay_image: Default::default(),
            overlay_material_from_slot: Default::default(),
            projectile_model: Default::default(),
            combat_fx: crate::WeaponCombatFx::empty(namespace),
            proj_trail: Default::default(),
            proj_trail_from_slot: Default::default(),
            proj_ignition: Default::default(),
            proj_ignition_from_slot: Default::default(),
            sounds: Default::default(),
            sz_xanims: std::array::from_fn(|_| None),
        }
    }
}

pub fn overlay_name_is_hud_iris(name: &str) -> bool {
    !name.starts_with("mc/")
}
