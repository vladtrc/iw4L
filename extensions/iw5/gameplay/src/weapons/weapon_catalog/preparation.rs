use super::{
    attachments::AttachmentLibrary, captured::CapturedWeapon, configuration, iw5_configuration,
};
use asset_game::*;

pub fn hide_tags(capture: &WeaponNativeCapture) -> Vec<String> {
    let scoped = capture
        .get::<CapturedWeapon>()
        .and_then(|capture| capture.configuration)
        .is_some_and(|(_, selection)| selection.scope != 0);
    vec![
        if scoped {
            "tag_sight_on"
        } else {
            "tag_sight_off"
        }
        .to_owned(),
    ]
}

pub fn prepare(content: &mut dyn WeaponPreparationContent) -> ConfigurationPreparationCensus {
    let mut census = ConfigurationPreparationCensus::default();
    let mut prepared = Vec::new();
    for (base_id, selection) in content.candidates(crate::ID) {
        if selection.attachments.is_empty() {
            continue;
        }
        let Some(namespace) = selection.family.as_ref().map(|key| key.namespace) else {
            continue;
        };
        let attachments = content.normalize(namespace, &selection.attachments);
        let selection = WeaponSelection {
            attachments,
            ..selection
        };
        let row = content.draft(base_id).and_then(|base| {
            let library = content.attachments().get::<AttachmentLibrary>(crate::ID)?;
            let native = configuration::resolve_slots(
                &base.attachment_assets,
                library,
                &base.name,
                &selection.attachments,
            )
            .ok()?;
            let mut row = iw5_configuration::compose(&base, library, native, false)?;
            mark(&mut row, base_id, native);
            Some(row)
        });
        match row {
            Some(row) => prepared.push((base_id, selection, row)),
            None => census.refused.push(selection),
        }
    }
    census.prepared = prepared.len();
    for (base_id, selection, mut row) in prepared {
        row.prepared_attachments = selection.attachments.clone();
        content.append(base_id, base_id, Some(selection), row);
    }
    let primary_count = content.len();
    for parent in 1..primary_count {
        let Some(mut primary) = content.draft(parent as u32) else {
            continue;
        };
        let Some((base_id, native)) = primary
            .native_capture
            .get::<CapturedWeapon>()
            .and_then(|capture| capture.configuration)
        else {
            continue;
        };
        let Some(base) = content.draft(base_id) else {
            continue;
        };
        let Some(library) = content.attachments().get::<AttachmentLibrary>(crate::ID) else {
            continue;
        };
        let Some(mut alternate) = iw5_configuration::compose(&base, library, native, true) else {
            continue;
        };
        mark(&mut alternate, base_id, native);
        let underbarrel = native.underbarrel as usize + 5;
        let shared = base
            .attachment_assets
            .get(underbarrel)
            .and_then(Option::as_deref)
            .and_then(|name| library.entries.get(name))
            .is_some_and(|asset| asset.share_ammo_with_alt);
        alternate.facts.ammo_index = if shared {
            if primary.facts.ammo_index != 0 {
                primary.facts.ammo_index
            } else {
                parent as i32
            }
        } else {
            (parent as i32) | 0x1000000
        };
        alternate.facts.clip_index = if shared {
            if primary.facts.clip_index != 0 {
                primary.facts.clip_index
            } else {
                parent as i32
            }
        } else {
            (parent as i32) | 0x1000000
        };
        alternate.prepared_attachments = primary.prepared_attachments.clone();
        alternate.alternate_weapon = None;
        alternate.alternate_index = 0;
        let index = content.append(base_id, base_id, None, alternate);
        primary.alternate_index = index;
        content.replace(parent as u32, primary);
        census.alternates += 1;
    }
    census
}
fn mark(row: &mut WeaponConfigurationDraft, base: u32, native: configuration::AttachmentSelection) {
    let mut capture = row
        .native_capture
        .get::<CapturedWeapon>()
        .expect("IW5 composer preserves captured state")
        .clone();
    capture.configuration = Some((base, native));
    row.native_capture = WeaponNativeCapture::new(capture);
    row.indexed_name = false;
}
