use super::*;

pub trait NativeWeaponCapture {
    fn capture_t6(&mut self, weapon: ext_t6_fastfile::weapon::WeaponView<'_>);
}
impl NativeWeaponCapture for WeaponCatalog {
    fn capture_t6(&mut self, weapon: ext_t6_fastfile::weapon::WeaponView<'_>) {
        use ext_t6_fastfile::weapon::variant as v;
        let Some(name) = weapon.name().filter(|name| !name.is_empty()) else {
            return;
        };
        self.insert_capture(WeaponCaptureInput {
            namespace: crate::ID,
            name: name.to_owned(),
            alternate_weapon: weapon
                .variant_str(v::ALT_WEAPON_NAME)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            weap_def: None,
            display_name_key: weapon
                .variant_str(v::DISPLAY_NAME)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: {
                use ext_t6_fastfile::weapon::def as d;
                let center = weapon
                    .def_loaded_asset_name(d::RETICLE_CENTER)
                    .map(t6_model_name);
                let side = weapon
                    .def_loaded_asset_name(d::RETICLE_SIDE)
                    .map(t6_model_name);
                WeaponReticleAssets {
                    center_authored: center.is_some(),
                    side_authored: side.is_some(),
                    center_image: center.clone(),
                    side_image: side.clone(),
                    center_material: center,
                    side_material: side,
                    center_size: weapon.def_i32(d::RETICLE_CENTER_SIZE),
                    side_size: weapon.def_i32(d::RETICLE_SIDE_SIZE),
                    ..Default::default()
                }
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            reticle_center_slot: None,
            reticle_side_slot: None,
            overlay_material: weapon
                .variant_asset_name(v::OVERLAY_MATERIAL)
                .map(str::to_owned),
            overlay_image: weapon
                .variant_asset_name(v::OVERLAY_MATERIAL)
                .map(str::to_owned),
            overlay_material_slot: None,
            attachment_assets: Vec::new(),
            attached_models: [true, false].map(|view| t6_attached_models(weapon, view)),
            camo_models: WeaponCamoModels::default(),
            skin_parent: None,
            native_capture: WeaponNativeCapture::new(CapturedWeapon::capture(weapon)),
            hud_icon: weapon
                .def_loaded_asset_name(ext_t6_fastfile::weapon::def::HUD_ICON)
                .map(t6_model_name),
            hud_icon_slot: None,
            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: weapon
                .def_loaded_asset_name(ext_t6_fastfile::weapon::def::KILL_ICON)
                .map(t6_model_name),
            kill_icon_slot: None,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel: weapon
                .def_asset_array_name(ext_t6_fastfile::weapon::def::GUN_XMODEL, 0)
                .map(t6_model_name),
            hand_xmodel: None,
            knife_xmodel: None,
            world_model: weapon
                .def_asset_array_name(ext_t6_fastfile::weapon::def::WORLD_MODEL, 0)
                .map(t6_model_name),
            projectile_model: weapon
                .def_asset_name(ext_t6_fastfile::weapon::def::PROJECTILE_MODEL)
                .map(t6_model_name)
                .or_else(|| crate::weapons::weapon_t6::planted_model(name).map(str::to_owned)),
            rocket_model: None,
            sz_xanims: t6_sz_xanims(weapon),
            dual_wield_weapon: None,
            impact_payload: None,
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: t6_sz_xanims_left(weapon),
            hide_tags: Vec::new(),
            sounds: capture_t6_sounds(weapon),
            flame: None,
            combat_fx: WeaponCombatFx {
                view_flash_hint: weapon
                    .def_asset_name(ext_t6_fastfile::weapon::def::VIEW_FLASH_EFFECT)
                    .map(str::to_owned),
                world_flash_hint: weapon
                    .def_asset_name(ext_t6_fastfile::weapon::def::WORLD_FLASH_EFFECT)
                    .map(str::to_owned),
                explosion_hint: weapon
                    .def_asset_name(ext_t6_fastfile::weapon::def::PROJ_EXPLOSION_EFFECT)
                    .map(str::to_owned),
                ..WeaponCombatFx::empty(crate::ID)
            },
            combat_slots: WeaponCaptureFxSlots::default(),
            facts: capture_t6_body_facts(weapon),
        });
    }
}
