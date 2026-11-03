use super::attachments::{Attachment, AttachmentLibrary};
use asset_game::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachmentSelection {
    pub scope: u8,
    pub underbarrel: u8,
    pub others: u8,
}

pub fn resolve_slots(
    slots: &[Option<String>],
    library: &AttachmentLibrary,
    base_name: &str,
    names: &[String],
) -> Result<AttachmentSelection, ConfigurationRefusal> {
    let mut selected = AttachmentSelection::default();
    for name in names {
        let mut matches = slots.iter().enumerate().filter(|(_, slot)| {
            slot.as_deref()
                .is_some_and(|native| native.eq_ignore_ascii_case(name))
        });
        let direct = matches.next();
        if matches.next().is_some() {
            return Err(ConfigurationRefusal::Unsupported(format!(
                "`{name}` has more than one native IW5 slot in `{}`",
                base_name
            )));
        }
        let index = if let Some((index, _)) = direct {
            index
        } else {
            let display_key = format!("WEAPON_{}_ATTACHMENT", name.to_ascii_uppercase());
            let mut by_display = slots.iter().enumerate().filter(|(_, slot)| {
                slot.as_deref()
                    .and_then(|native| library.entries.get(native))
                    .and_then(|asset| asset.display_name.as_deref())
                    .is_some_and(|key| key.eq_ignore_ascii_case(&display_key))
            });
            let Some((index, _)) = by_display.next() else {
                return Err(ConfigurationRefusal::NotOffered(name.clone()));
            };
            if by_display.next().is_some() {
                return Err(ConfigurationRefusal::Unsupported(format!(
                    "`{name}` has an ambiguous IW5 display key in `{}`",
                    base_name
                )));
            }
            index
        };
        let native = slots[index].as_deref().expect("matched native slot");
        if !library.entries.contains_key(native) {
            return Err(ConfigurationRefusal::MissingContent(native.to_owned()));
        }
        match index {
            0..=5 => {
                if selected.scope != 0 && selected.scope != (index + 1) as u8 {
                    return Err(ConfigurationRefusal::Incompatible {
                        a: slots[selected.scope as usize - 1]
                            .clone()
                            .unwrap_or(String::new()),
                        b: name.clone(),
                    });
                }
                selected.scope = (index + 1) as u8;
            }
            6..=8 => {
                if selected.underbarrel != 0 && selected.underbarrel != (index - 5) as u8 {
                    return Err(ConfigurationRefusal::Incompatible {
                        a: slots[selected.underbarrel as usize + 5]
                            .clone()
                            .unwrap_or(String::new()),
                        b: name.clone(),
                    });
                }
                selected.underbarrel = (index - 5) as u8;
            }
            9..=12 => selected.others |= 1 << (index - 9),
            _ => unreachable!(),
        }
    }
    Ok(selected)
}

pub fn primary_assets<'a>(
    slots: &[Option<String>],
    library: &'a AttachmentLibrary,
    selection: AttachmentSelection,
) -> Option<Vec<&'a Attachment>> {
    let slot_asset = |index: usize| {
        slots
            .get(index)?
            .as_deref()
            .and_then(|name| library.entries.get(name))
    };
    let mut assets = Vec::with_capacity(3);
    let mut push = |asset| {
        if assets.len() < 3 {
            assets.push(asset);
        } else {
            assets[2] = asset;
        }
    };
    if selection.scope != 0 {
        push(slot_asset(usize::from(selection.scope - 1))?);
    }
    for bit in 0..4 {
        if selection.others & (1 << bit) != 0 {
            push(slot_asset(9 + bit)?);
        }
    }
    if selection.underbarrel != 0 {
        let asset = slot_asset(usize::from(selection.underbarrel) + 5)?;
        if asset.weapon_class == 0
            || asset.ads_settings_main.is_some()
            || (asset.scales.ads_settings_main != 0.0 && asset.scales.ads_settings_main != 1.0)
        {
            push(asset);
        }
    }
    Some(assets)
}

pub fn compile(
    content: &dyn WeaponConfigurationContent,
    family: &WeaponFamily,
    selection: &WeaponSelection,
) -> Result<u32, ConfigurationRefusal> {
    if selection.attachments.is_empty() {
        return compile_authored_weapon_configuration(
            content,
            family,
            joined_weapon_name(&family.key.base, &selection.attachments),
        );
    }
    let base = family
        .base
        .ok_or_else(|| ConfigurationRefusal::MissingContent(format!("{}_mp", family.key.base)))?;
    let name = content.name(base);
    let slots = content
        .attachment_assets(base)
        .filter(|_| content.namespace(base) == Some(crate::ID))
        .ok_or_else(|| ConfigurationRefusal::UnknownFamily(name.to_owned()))?;
    let empty = AttachmentLibrary::default();
    let library = content
        .attachments()
        .get::<AttachmentLibrary>(crate::ID)
        .unwrap_or(&empty);
    let native = resolve_slots(slots, library, name, &selection.attachments)?;
    primary_assets(slots, library, native)
        .ok_or_else(|| ConfigurationRefusal::MissingContent(name.to_owned()))?;
    let id = content.prepared(selection).ok_or_else(|| {
        ConfigurationRefusal::MissingContent(format!(
            "{} {}",
            family.key,
            selection.attachments.join(" ")
        ))
    })?;
    content.native_admission(id)?;
    Ok(id)
}
impl AttachmentSelection {
    pub fn override_candidates(self) -> [u16; 3] {
        let mut candidates = [0; 3];
        let mut count = 0;
        let mut push = |condition| {
            candidates[count.min(2)] = condition;
            count += 1;
        };
        if (1..=6).contains(&self.scope) {
            push(u16::from(self.scope));
        }
        if (1..=3).contains(&self.underbarrel) {
            push(u16::from(self.underbarrel) << 3);
        }
        for bit in 0..4 {
            if self.others & (1 << bit) != 0 {
                push(1 << (5 + bit));
            }
        }
        candidates
    }

    pub fn contains_condition(self, condition: u16) -> bool {
        if condition == 0 {
            return true;
        }
        self.override_candidates().contains(&condition)
    }
}
