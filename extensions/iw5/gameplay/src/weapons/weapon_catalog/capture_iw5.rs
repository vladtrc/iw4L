use super::*;

pub trait NativeWeaponCapture {
    fn capture_iw5(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        strings: &ext_iw5_fastfile::ScriptStrings,
        fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
    );
}
impl NativeWeaponCapture for WeaponCatalog {
    fn capture_iw5(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        strings: &ext_iw5_fastfile::ScriptStrings,
        fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
    ) {
        let Some(geometry) = stream.weapon() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = geometry
            .gun_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let hand_xmodel = geometry
            .hand_xmodel_name
            .and_then(|ptr| leftover_cstr_iw5(stream, ptr));
        let world_model = geometry
            .world_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let overlay_material = leftover_iw5_overlay_name(stream, &geometry);
        let overlay_image = None;
        let overlay_material_slot = leftover_iw5_weapdef_overlay_slot(stream, geometry.weap_def);
        let attachment_assets = geometry
            .attachments
            .map(|name| name.and_then(|ptr| leftover_cstr_iw5(stream, ptr)))
            .into();
        let native = CapturedWeapon::capture(stream, strings, &geometry, fx_name_at_slot);
        let sounds = leftover_iw5_sounds(stream, strings, &geometry, &native);
        let mut sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_iw5(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        apply_default_animations(&mut sz_xanims, &native.anim_overrides);
        self.insert_capture(WeaponCaptureInput {
            namespace: crate::ID,
            impact_payload: None,
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| leftover_cstr_iw5(stream, p)),
            reticle_center_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                ext_iw5_fastfile::size::WEAPON_DEF_RETICLE_CENTER_OFF,
                560,
            ),
            reticle_side_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                ext_iw5_fastfile::size::WEAPON_DEF_RETICLE_SIDE_OFF,
                568,
            ),
            name: name.to_owned(),
            weap_def: geometry
                .weap_def
                .map(|slot| asset_material::AssetPointerIdentity {
                    block: slot.block,
                    offset: slot.offset,
                }),
            display_name_key: geometry
                .display_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: leftover_iw5_reticle(stream, geometry.weap_def),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material,
            overlay_image,
            overlay_material_slot,
            attachment_assets,
            attached_models: Default::default(),
            native_capture: WeaponNativeCapture::new(native),
            hud_icon: leftover_iw5_material_name(
                stream,
                geometry.weap_def,
                ext_iw5_fastfile::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),
            hud_icon_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                ext_iw5_fastfile::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),

            pickup_icon: None,
            pickup_icon_slot: leftover_iw5_asset_slot(stream, geometry.weap_def, 508, 808),
            pickup_icon_image: None,
            pickup_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 512, 816)),
            hud_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 504, 800)),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: geometry
                .kill_icon
                .and_then(|mat| leftover_xstring_at_iw5(stream, mat, 0, 0)),
            kill_icon_slot: geometry.kill_icon_slot.map(|slot| {
                asset_material::AssetPointerIdentity {
                    block: slot.block,
                    offset: slot.offset,
                }
            }),
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,
            world_model,
            camo_models: {
                let read = |names: &[Option<ext_iw5_fastfile::Ptr>]| {
                    let mut models = Vec::new();
                    let mut invalid = Vec::new();
                    for (slot, ptr) in names.iter().enumerate().skip(1) {
                        if let Some(ptr) = ptr {
                            match stream.cstr(*ptr) {
                                Ok(name) if !name.is_empty() => {
                                    models.push((slot as u8, name.to_owned()))
                                }
                                Ok(_) => invalid.push(slot as u8),
                                Err(_) => invalid.push(slot as u8),
                            }
                        }
                    }
                    (models, invalid)
                };
                let (view, invalid_view) = read(&geometry.gun_xmodel_names);
                let (world, invalid_world) = read(&geometry.world_model_names);
                WeaponCamoModels {
                    view,
                    world,
                    invalid_view,
                    invalid_world,
                    choices: Vec::new(),
                }
            },
            skin_parent: None,
            projectile_model: geometry
                .projectile_model_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            rocket_model: geometry
                .rocket_model_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            knife_xmodel: geometry
                .knife_xmodel_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            sz_xanims,
            dual_wield_weapon: None,
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: read_hide_tags_iw5(stream, strings, geometry.hide_tags),
            sounds,
            flame: None,
            combat_fx: read_iw5_combat_fx(stream, &geometry, fx_name_at_slot),
            combat_slots: WeaponCaptureFxSlots::default(),
            facts: capture_iw5_body_facts(stream, &geometry),
        });
    }
}

pub(super) fn leftover_iw5_overlay_name(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
) -> Option<String> {
    if let Some(name) = geometry
        .overlay_material_names
        .iter()
        .flatten()
        .filter_map(|&p| leftover_cstr_iw5(stream, p))
        .find(|name| overlay_name_is_hud_iris(name))
        .or_else(|| {
            leftover_iw5_material_name(
                stream,
                geometry.weap_def,
                ext_iw5_fastfile::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
                1352,
            )
        })
        && overlay_name_is_hud_iris(&name)
    {
        return Some(name);
    }
    None
}

pub(super) fn leftover_cstr_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    ptr: ext_iw5_fastfile::Ptr,
) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(super) fn leftover_iw5_weapdef_overlay_slot(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    weap_def: Option<ext_iw5_fastfile::Ptr>,
) -> Option<asset_material::AssetPointerIdentity> {
    leftover_iw5_asset_slot(
        stream,
        weap_def,
        ext_iw5_fastfile::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
        1352,
    )
}

pub(super) fn leftover_iw5_material_name(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    weap_def: Option<ext_iw5_fastfile::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    let mat = match stream.ptr_at(body, field).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    leftover_xstring_at_iw5(stream, mat, 0, 0)
}

pub(super) fn leftover_iw5_asset_slot(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    weap_def: Option<ext_iw5_fastfile::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<asset_material::AssetPointerIdentity> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    match stream.ptr_at(body, field).ok()? {
        ext_iw5_fastfile::ZonePtr::Null => None,
        _ => {
            let cell = body.at(field);
            Some(asset_material::AssetPointerIdentity {
                block: cell.block,
                offset: cell.offset,
            })
        }
    }
}

pub(super) fn leftover_iw5_reticle(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    weap_def: Option<ext_iw5_fastfile::Ptr>,
) -> WeaponReticleAssets {
    use ext_iw5_fastfile::size as sz;
    WeaponReticleAssets {
        center_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        ),
        side_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        ),
        center_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        )
        .is_some(),
        side_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        )
        .is_some(),
        center_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF, 576))
            .unwrap_or(0),
        side_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580))
            .unwrap_or(0),
        ..WeaponReticleAssets::default()
    }
}

pub(super) fn leftover_iw5_sounds(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
    native: &CapturedWeapon,
) -> WeaponSoundAliases {
    use ext_iw5_fastfile::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponSoundAliases::default();
    };
    let mut sounds = WeaponSoundAliases {
        notetrack_convention: NotetrackConvention::SoundMap,
        fire_start: None,
        fire_start_player: None,
        pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PICKUP_OFF, 128),
        pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PICKUP_PLAYER_OFF,
            136,
        ),
        ammo_pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_AMMO_PICKUP_OFF, 144),
        ammo_pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_AMMO_PICKUP_PLAYER_OFF,
            152,
        ),
        pullback: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PULLBACK_OFF, 168),
        pullback_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PULLBACK_PLAYER_OFF,
            176,
        ),
        fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF, 192),
        empty_fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF, 256),
        empty_fire_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF,
            264,
        ),
        melee_swipe: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_SWIPE_OFF, 272),
        melee_swipe_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF,
            280,
        ),
        melee_hit: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_HIT_OFF, 288),
        melee_miss: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_MISS_OFF, 296),
        rechamber: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_OFF, 304),
        rechamber_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF,
            312,
        ),
        reload: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_OFF, 320),
        reload_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
        reload_empty: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF,
            336,
        ),
        reload_empty_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF,
            344,
        ),
        reload_start: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_OFF,
            352,
        ),
        reload_start_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF,
            360,
        ),
        reload_end: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_OFF, 368),
        reload_end_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF,
            376,
        ),
        alt_switch: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_ALT_SWITCH_OFF, 432),
        alt_switch_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_ALT_SWITCH_PLAYER_OFF,
            440,
        ),
        raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RAISE_OFF, 448),
        raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF,
            456,
        ),
        first_raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRST_RAISE_OFF, 464),
        first_raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRST_RAISE_PLAYER_OFF,
            472,
        ),
        putaway: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PUTAWAY_OFF, 480),
        putaway_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF,
            488,
        ),
        detonate: None,
        detonate_player: None,
        proj_explosion: None,
        projectile: leftover_iw5_snd_alias(stream, body, 96, 160),
        proj_ignition_sound: None,
        bounce: core::array::from_fn(|surface| {
            let table = match stream.ptr_at(body, stream.layout(272, 512)).ok()? {
                ext_iw5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
                _ => return None,
            };
            let cell = surface * stream.pointer_bytes();
            leftover_iw5_snd_alias_at(stream, table, cell, cell)
        }),
        notetrack_sound_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF, 48),
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF, 56),
            sz::WEAPON_DEF_NOTE_SOUND_MAP_COUNT,
        ),
        notetrack_rumble_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_KEYS_OFF, 64),
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_VALUES_OFF, 72),
            sz::WEAPON_DEF_NOTE_RUMBLE_MAP_COUNT,
        ),
        fire_player_akimbo: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_AKIMBO_OFF,
            200,
        ),
        fire_loop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LOOP_OFF, 208),
        fire_loop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LOOP_PLAYER_OFF,
            216,
        ),
        fire_stop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_STOP_OFF, 224),
        fire_stop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_STOP_PLAYER_OFF,
            232,
        ),
        fire_last: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LAST_OFF, 240),
        fire_last_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LAST_PLAYER_OFF,
            248,
        ),
        dependency_aliases: native.sound_dependencies(),
        fire_ptr_kind: leftover_iw5_snd_ptr_kind(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF,
            192,
        ),
        reload_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
    };
    apply_default_sounds(&mut sounds, &native.sound_overrides);
    sounds
}

pub(super) fn leftover_xstring_at_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    row: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    match stream.ptr_at(row, stream.layout(x86, x64)) {
        Ok(ext_iw5_fastfile::ZonePtr::Offset(q)) => {
            leftover_cstr_iw5(stream, stream.resolve_alias(q))
        }
        _ => None,
    }
}

pub(super) fn read_iw5_combat_fx(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
) -> WeaponCombatFx {
    use ext_iw5_fastfile::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponCombatFx::empty(crate::ID);
    };
    let name = |x86, x64| match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(ext_iw5_fastfile::ZonePtr::Offset(q)) => fx_name_at_slot(stream.resolve_alias(q)),
        _ => None,
    };
    let view_last_shot_eject_hint = name(sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF, 544);
    let world_last_shot_eject_hint = name(sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF, 552);
    WeaponCombatFx {
        view_flash_hint: name(sz::WEAPON_DEF_VIEW_FLASH_OFF, 112),
        world_flash_hint: name(sz::WEAPON_DEF_WORLD_FLASH_OFF, 120),
        view_shell_eject_hint: name(sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF, 528),
        world_shell_eject_hint: name(sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF, 536),
        last_shot_eject_pair_authored: view_last_shot_eject_hint.is_some()
            && world_last_shot_eject_hint.is_some(),
        view_last_shot_eject_hint,
        world_last_shot_eject_hint,
        ..WeaponCombatFx::empty(crate::ID)
    }
}

pub(super) fn leftover_iw5_snd_alias(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    leftover_iw5_snd_alias_at(stream, body, x86, x64)
}

pub(super) fn leftover_iw5_snd_ptr_kind(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> Option<&'static str> {
    match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(ext_iw5_fastfile::ZonePtr::Null) => Some("null"),
        Ok(ext_iw5_fastfile::ZonePtr::Following) => Some("follow"),
        Ok(ext_iw5_fastfile::ZonePtr::Insert) => Some("insert"),
        Ok(ext_iw5_fastfile::ZonePtr::Offset(_)) => Some("offset"),
        Err(_) => Some("err"),
    }
}

pub(super) fn leftover_iw5_snd_alias_at(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let wrapper = match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    let name_ptr = match stream.ptr_at(wrapper, 0).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(n) => stream.resolve_alias(n),
        _ => wrapper,
    };
    leftover_cstr_iw5(stream, name_ptr)
}

pub(super) fn leftover_iw5_script_string_map(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    body: ext_iw5_fastfile::Ptr,
    keys_off: usize,
    values_off: usize,
    cap: usize,
) -> Vec<(String, String)> {
    ext_iw5_fastfile::ScriptStringMap::at(stream, body, keys_off, values_off)
        .map(|map| iw5_script_string_pairs(stream, strings, map, cap))
        .unwrap_or(Vec::new())
}

pub(super) fn iw5_script_string_pairs(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    map: ext_iw5_fastfile::ScriptStringMap,
    cap: usize,
) -> Vec<(String, String)> {
    map.pairs(stream, cap)
        .filter_map(|(key_id, val_id)| {
            let key = strings.get(stream, key_id).filter(|s| !s.is_empty())?;
            let val = strings
                .get(stream, val_id)
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            Some((key.to_owned(), val.to_owned()))
        })
        .collect()
}

pub(super) fn read_hide_tags_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    arr: Option<ext_iw5_fastfile::Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..32usize {
        let Ok(id) = stream.u16_at(arr, i * 2) else {
            break;
        };
        if id == 0 {
            continue;
        }
        if let Some(name) = strings.get(stream, id)
            && !name.is_empty()
        {
            out.push(name.to_owned());
        }
    }
    out
}
