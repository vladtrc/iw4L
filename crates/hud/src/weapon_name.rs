use assets::{BoundWeapons, PreparedLocalizedStrings};

pub(crate) fn localized_weapon_name(
    viewmodel_index: u32,
    weapons: Option<&BoundWeapons<'_>>,
    strings: Option<&PreparedLocalizedStrings>,
) -> Option<String> {
    let Some(weapons) = weapons else {
        diag::warn_once!(Ui, "no weapon catalog is loaded");
        return None;
    };
    let Some(key) = weapons.registry().display_name_key_of(viewmodel_index) else {
        diag::warn_once!(
            Ui,
            "weapon index {viewmodel_index} carries no display-name key"
        );
        return None;
    };

    let Some(strings) = strings else {
        diag::warn_once!(Ui, "no localized string table is loaded");
        return None;
    };
    let text = match weapons.registry().identity_namespace_of(viewmodel_index) {
        Some(namespace) => strings.0.text_in(namespace, key),
        None => strings.0.text(key),
    };
    match text {
        Some(text) => {
            let mut name = text.to_owned();
            for key in weapons
                .registry()
                .attachment_caption_keys_of(viewmodel_index)
            {
                let caption = match weapons.registry().identity_namespace_of(viewmodel_index) {
                    Some(namespace) => strings.0.text_in(namespace, key),
                    None => strings.0.text(key),
                };
                if let Some(caption) = caption {
                    name.push(' ');
                    name.push_str(caption);
                }
            }
            Some(name)
        }
        None => {
            diag::warn_once!(
                Ui,
                "key `{key}` has no row in the loaded language",
                key = key.to_owned()
            );
            None
        }
    }
}
