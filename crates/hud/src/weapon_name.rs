use assets::{PreparedLocalizedStrings, PreparedWeapons};

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};

pub(crate) fn localized_weapon_name(
    viewmodel_index: u32,
    weapons: Option<&PreparedWeapons>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
) -> Option<String> {
    let Some(weapons) = weapons else {
        gaps.raise(GapCause::NameNoWeaponCatalog);
        return None;
    };
    let Some(key) = weapons.0.display_name_key_of(viewmodel_index) else {
        gaps.raise(GapCause::NameNoDisplayNameKey { viewmodel_index });
        return None;
    };
    gaps.clear(HudGap::WeaponDisplayName);
    let Some(strings) = strings else {
        gaps.raise(GapCause::NoStringTable);
        return None;
    };
    match strings.0.text(key) {
        Some(text) => {
            gaps.clear(HudGap::LocalizedText);
            Some(text.to_owned())
        }
        None => {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: key.to_owned(),
            });
            None
        }
    }
}
