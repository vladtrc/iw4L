use super::attachments::AttachmentLibrary;
use super::configuration::AttachmentSelection;
use super::iw5_parameters::*;
use super::*;
pub fn compose(
    base: &WeaponConfigurationDraft,
    library: &AttachmentLibrary,
    native: AttachmentSelection,
    alternate: bool,
) -> Option<WeaponConfigurationDraft> {
    use ext_iw5_fastfile::size as sz;
    let slot_asset = |index: usize| {
        base.attachment_assets
            .get(index)?
            .as_deref()
            .and_then(|native| library.entries.get(native))
    };
    let scope = match native.scope {
        0 => None,
        index => Some(slot_asset(usize::from(index) - 1)?),
    };
    let underbarrel = match native.underbarrel {
        0 => None,
        index => Some(slot_asset(usize::from(index) + 5)?),
    };
    let others = (0..4)
        .filter(|bit| native.others & (1 << bit) != 0)
        .map(|bit| slot_asset(9 + bit))
        .collect::<Option<Vec<_>>>()?;

    if base.namespace != crate::ID {
        return None;
    }
    let mut row = base.clone();

    let first = |models: &[Option<String>]| models.first().cloned().flatten();
    let mut view = if scope.is_none() {
        base.attachment_view_models.clone()
    } else {
        Vec::new()
    };
    let mut world = if scope.is_none() {
        base.attachment_world_models.clone()
    } else {
        Vec::new()
    };
    if let Some(scope) = scope {
        view.extend(first(&scope.view_models));
        view.extend(first(&scope.reticle_models));
        world.extend(first(&scope.world_models));
    }
    for asset in underbarrel.into_iter().chain(others.iter().copied()) {
        view.extend(first(&asset.view_models));
        world.extend(first(&asset.world_models));
    }
    row.attachment_view_models = view;
    row.attachment_world_models = world;

    let primary_assets = configuration::primary_assets(&base.attachment_assets, library, native)?;
    let alternate_assets;
    let assets = if alternate {
        let underbarrel = underbarrel.filter(|asset| asset.weapon_class != 0)?;
        row.facts.weap_type = remap_iw5_weap_type(underbarrel.weapon_type);
        row.facts.weap_class = underbarrel.weapon_class;
        row.facts.inventory_type = 3;
        alternate_assets = if underbarrel.share_ammo_with_alt {
            primary_assets
        } else {
            vec![underbarrel]
        };
        &alternate_assets
    } else {
        &primary_assets
    };
    apply_iw5_parameter_blocks(&mut row.facts, assets);
    if let Some(reticle) = assets.iter().find_map(|asset| asset.reticle.as_ref()) {
        row.reticle = reticle.clone();
        row.facts.i_reticle_side_size = reticle.side_size;
    }
    if alternate {
        if let Some(projectile) = iw5_first_block(assets, |asset| asset.projectile) {
            row.facts.explosion_radius = projectile.explosion_radius;
            row.facts.explosion_inner_damage = projectile.explosion_inner_damage;
            row.facts.explosion_outer_damage = projectile.explosion_outer_damage;
            row.facts.projectile_speed = projectile.speed;
            row.facts.projectile_speed_up = projectile.speed_up;
            row.facts.projectile_activate_dist = projectile.activate_distance;
            row.facts.projectile_explosion_type = projectile.explosion_type;
            row.facts.proj_impact_explode = projectile.impact_explode;
            let asset = assets.iter().find(|asset| asset.projectile.is_some())?;
            row.projectile_model = asset.projectile_model.clone();
            row.combat_fx.explosion_hint = asset.projectile_explosion_fx.clone();
            row.proj_trail = asset.projectile_trail_fx.clone();
            row.proj_trail_from_slot = row.proj_trail.is_some();
            row.proj_ignition = asset.projectile_ignition_fx.clone();
            row.proj_ignition_from_slot = row.proj_ignition.is_some();
            row.sounds.proj_explosion = asset.projectile_explosion_sound.clone();
            row.sounds.proj_ignition_sound = asset.projectile_ignition_sound.clone();
        }
    }

    if scope.is_some() || alternate {
        row.overlay_material = None;
        row.overlay_image = None;
        row.overlay_material_from_slot = false;
        row.facts.thermal_scope = false;
        row.facts.overlay_reticle = 0;
        row.facts.overlay_interface = 0;
        row.facts.ads_overlay_width = 0.0;
        row.facts.ads_overlay_height = 0.0;
    }
    if let Some(scope) = assets.iter().find(|asset| asset.overlay.is_some()) {
        if let Some(overlay) = scope
            .overlay
            .as_deref()
            .filter(|name| !name.is_empty() && overlay_name_is_hud_iris(name))
        {
            row.overlay_material = Some(overlay.to_owned());
            row.overlay_image = None;
            row.overlay_material_from_slot = true;
            row.facts.overlay_reticle = scope.overlay_reticle;
            row.facts.thermal_scope = scope.thermal;
            row.facts.ads_overlay_width = scope.width;
            row.facts.ads_overlay_height = scope.height;
        }
    }

    let mut anim_types: Vec<u32> = base
        .native_capture
        .get::<CapturedWeapon>()?
        .anim_overrides
        .iter()
        .map(|row| row.anim_tree_type)
        .collect();
    anim_types.sort_unstable();
    anim_types.dedup();
    for anim_type in anim_types {
        let Some(chosen) = select_anim_override(base, native, anim_type) else {
            continue;
        };
        let Some(slot) = anim_slot(anim_type) else {
            continue;
        };
        if let Some(anim) = if alternate {
            chosen.altmode_anim.clone()
        } else {
            chosen.override_anim.clone()
        } {
            row.sz_xanims[slot] = Some(anim);
        }
        let time = if alternate {
            chosen.alt_time_ms
        } else {
            chosen.anim_time_ms
        };
        if time > 0
            || (alternate && slot == weap_anim::RELOAD_EMPTY && chosen.altmode_anim.is_some())
        {
            if let Some(timer) = iw5_anim_timer(&mut row.facts, slot) {
                *timer = time;
            }
        }
    }

    for (sound_type, target) in [
        (sz::SND_OVERRIDE_TYPE_FIRE, &mut row.sounds.fire),
        (
            sz::SND_OVERRIDE_TYPE_PLAYER_FIRE,
            &mut row.sounds.fire_player,
        ),
        (
            sz::SND_OVERRIDE_TYPE_PLAYER_AKIMBO,
            &mut row.sounds.fire_player_akimbo,
        ),
        (
            sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT,
            &mut row.sounds.fire_last_player,
        ),
    ] {
        if let Some(sound) = select_sound_override(base, native, sound_type).and_then(|chosen| {
            if alternate {
                chosen.altmode_sound.clone()
            } else {
                chosen.override_sound.clone()
            }
        }) {
            *target = Some(sound);
        }
    }

    for (fx_type, hint) in [
        (1, &mut row.combat_fx.view_flash_hint),
        (2, &mut row.combat_fx.world_flash_hint),
        (3, &mut row.combat_fx.view_shell_eject_hint),
        (4, &mut row.combat_fx.world_shell_eject_hint),
    ] {
        if let Some(fx) = select_fx_override(base, native, fx_type).and_then(|chosen| {
            if alternate {
                chosen.altmode_fx.clone()
            } else {
                chosen.override_fx.clone()
            }
        }) {
            *hint = Some(fx);
        }
    }

    if let Some(reload) = select_reload_override(base, native) {
        row.facts.reload_add_time_ms = reload.reload_add_time_ms;
        row.facts.reload_start_add_time_ms = reload.reload_start_add_time_ms;
    }
    if let Some(notetracks) = select_notetrack_override(base, native) {
        row.sounds.notetrack_sound_map = notetracks.sound_map.clone();
    }
    Some(row)
}
fn select_anim_override(
    base: &WeaponConfigurationDraft,
    selection: AttachmentSelection,
    kind: u32,
) -> Option<&AnimOverride> {
    iw5_best_pair_override(
        &base.native_capture.get::<CapturedWeapon>()?.anim_overrides,
        selection,
        kind,
        |row| (row.attachment1, row.attachment2, row.anim_tree_type),
    )
}
fn select_sound_override(
    base: &WeaponConfigurationDraft,
    selection: AttachmentSelection,
    kind: u32,
) -> Option<&SoundOverride> {
    iw5_best_pair_override(
        &base.native_capture.get::<CapturedWeapon>()?.sound_overrides,
        selection,
        kind,
        |row| (row.attachment1, row.attachment2, row.sound_type),
    )
}
fn select_fx_override(
    base: &WeaponConfigurationDraft,
    selection: AttachmentSelection,
    kind: u32,
) -> Option<&FxOverride> {
    iw5_best_pair_override(
        &base.native_capture.get::<CapturedWeapon>()?.fx_overrides,
        selection,
        kind,
        |row| (row.attachment1, row.attachment2, row.fx_type),
    )
}
fn select_reload_override(
    base: &WeaponConfigurationDraft,
    selection: AttachmentSelection,
) -> Option<&ext_iw5_fastfile::ReloadOverride> {
    base.native_capture
        .get::<CapturedWeapon>()?
        .reload_overrides
        .iter()
        .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
}
fn select_notetrack_override(
    base: &WeaponConfigurationDraft,
    selection: AttachmentSelection,
) -> Option<&NotetrackOverride> {
    base.native_capture
        .get::<CapturedWeapon>()?
        .notetrack_overrides
        .iter()
        .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
}
