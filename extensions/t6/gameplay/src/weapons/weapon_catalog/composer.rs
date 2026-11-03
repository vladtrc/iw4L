use super::captured::*;
use asset_game::*;
use asset_iw4::size::weap_anim;
use weapon_iw4::{WEAPON_ANIM_SLOTS, weap_anim_extra};
pub fn compose(
    base: &WeaponConfigurationDraft,
    kinds: &[u32],
    name: String,
) -> Option<WeaponConfigurationDraft> {
    let capture = base.native_capture.get::<CapturedWeapon>()?;
    if capture.attachments.is_empty() {
        return None;
    }
    let singles: Vec<&Attachment> = kinds
        .iter()
        .filter_map(|&kind| {
            capture
                .attachments
                .iter()
                .find(|attachment| attachment.kind == kind && attachment.mask == 0)
        })
        .collect();
    let mask = kinds.iter().fold(0u32, |mask, &kind| {
        mask | 1u32.checked_shl(kind).unwrap_or(0)
    });
    let pair = (kinds.len() > 1)
        .then(|| {
            capture
                .attachments
                .iter()
                .find(|attachment| attachment.mask == mask)
        })
        .flatten();
    let mut row = base.clone();
    row.name = name;
    clear_attachments(&mut row);
    row.hide_tags = Vec::new();
    for (side, models) in [
        &mut row.attachment_view_models,
        &mut row.attachment_world_models,
    ]
    .into_iter()
    .enumerate()
    {
        let clip = capture.clip_models[side].as_ref();
        let is_clip = |model: &String| clip == Some(model);
        if singles.iter().any(|a| a.disable_base_attachment) {
            models.retain(|model| is_clip(model));
        }
        if singles.iter().any(|a| a.disable_base_clip) {
            models.retain(|model| !is_clip(model));
        }
        for attachment in &singles {
            models.extend(attachment.models[side].iter().cloned());
        }
    }
    if !singles.is_empty() {
        let overlay = match pair {
            Some(pair) => pair.overlay.clone(),
            None if singles.iter().any(|a| a.overlay.is_none()) => None,
            None => singles
                .iter()
                .filter_map(|a| a.overlay.clone())
                .find(|overlay| base.overlay_material.as_ref() != Some(overlay))
                .or_else(|| base.overlay_material.clone()),
        };
        match &overlay {
            None => row.facts.overlay_reticle = 0,
            Some(_) if row.facts.overlay_reticle == 0 => row.facts.overlay_reticle = 1,
            Some(_) => {}
        }
        row.facts.can_hold_breath = overlay.is_some() && row.facts.weap_class != 11;
        row.overlay_image = overlay.clone();
        row.overlay_material = overlay;
    }
    row.attachment_view_ads_models = singles
        .iter()
        .filter_map(|attachment| attachment.view_ads_model.clone())
        .collect();
    for attachment in singles.iter().copied().chain(pair) {
        for (slot, clip) in attachment.xanims.iter().enumerate() {
            if let Some(clip) = clip
                && base.sz_xanims[slot].as_ref() != Some(clip)
            {
                row.sz_xanims[slot] = Some(clip.clone());
            }
        }
    }
    for stats in kinds
        .iter()
        .filter_map(|&kind| capture.stats.iter().find(|stats| stats.kind == kind))
    {
        apply_t6_attachment_stats(&mut row.facts, stats);
    }
    if singles.is_empty()
        && let Some(bare) = capture
            .attachments
            .iter()
            .find(|a| a.kind == 0 && a.mask == 0)
    {
        row.hide_tags = bare.hide_tags.clone();
    }
    for attachment in &singles {
        row.hide_tags.extend(attachment.hide_tags.iter().cloned());
        if let Some(sound) = &attachment.fire_sound {
            row.sounds.fire = Some(sound.clone());
        }
        if let Some(sound) = &attachment.fire_sound_player {
            row.sounds.fire_player = Some(sound.clone());
        }
        if let Some(alt) = &attachment.alt_weapon {
            row.alternate_weapon = Some(alt.clone());
        }
    }
    Some(row)
}

pub fn compose_alternate(
    base: &WeaponConfigurationDraft,
    config: &WeaponConfigurationDraft,
    alternate: &WeaponConfigurationDraft,
    parent: usize,
) -> Option<(WeaponConfigurationDraft, Option<String>)> {
    let capture = base.native_capture.get::<CapturedWeapon>()?;
    let alt_name = config.alternate_weapon.as_deref()?;
    let attachment = capture.attachments.iter().find(|a| {
        a.mask == 0
            && a.alt_weapon
                .as_deref()
                .is_some_and(|alt| alt.eq_ignore_ascii_case(alt_name))
    })?;
    let shared = capture
        .stats
        .iter()
        .find(|stats| stats.kind == attachment.kind)
        .is_some_and(|stats| stats.shared_ammo);
    let mut alt = alternate.clone();
    alt.name = format!("{}+{}", alt.name, config.name);
    alt.alternate_weapon = Some(config.name.clone());
    alt.attachment_view_models = config.attachment_view_models.clone();
    alt.attachment_world_models = config.attachment_world_models.clone();
    alt.attachment_view_ads_models = config.attachment_view_ads_models.clone();
    alt.hide_tags = config.hide_tags.clone();
    clear_attachments(&mut alt);
    for slot in 0..WEAPON_ANIM_SLOTS {
        if !shared
            && matches!(
                slot,
                weap_anim::RELOAD
                    | weap_anim::RELOAD_EMPTY
                    | weap_anim::RELOAD_START
                    | weap_anim::RELOAD_END
                    | weap_anim_extra::RELOAD_QUICK
                    | weap_anim_extra::RELOAD_QUICK_EMPTY
            )
        {
            continue;
        }
        if alt.sz_xanims[slot] == base.sz_xanims[slot] {
            alt.sz_xanims[slot] = config.sz_xanims[slot].clone();
        }
    }
    let authored = |slot: usize| {
        attachment.xanims[slot]
            .clone()
            .filter(|clip| base.sz_xanims[slot].as_ref() != Some(clip))
    };
    let mut config_raise = config.sz_xanims[weap_anim::ALT_RAISE].clone();
    if alt.sz_xanims[weap_anim::ALT_RAISE].is_none()
        || alt.sz_xanims[weap_anim::ALT_RAISE] == base.sz_xanims[weap_anim::ALT_RAISE]
    {
        alt.sz_xanims[weap_anim::ALT_RAISE] = config_raise.clone();
        if let Some(out) = authored(weap_anim::ALT_DROP) {
            config_raise = Some(out);
        }
    }
    alt.sz_xanims[weap_anim::ALT_DROP] = None;
    alt.facts.alternate_drop_time_ms = 0;
    if alt.facts.alternate_raise_time_ms <= 0 {
        alt.facts.alternate_raise_time_ms = config.facts.alternate_raise_time_ms;
    }
    if shared {
        alt.facts.ammo_index = parent as i32;
        alt.facts.clip_index = parent as i32;
        alt.facts.clip_size = config.facts.clip_size;
        alt.facts.start_ammo = config.facts.start_ammo;
        alt.facts.max_ammo = config.facts.max_ammo;
    }
    Some((alt, config_raise))
}
