use asset_core::FamilyId;
use bevy::prelude::*;
use frame::{UiMenuDvars, UiMenuRequest};
use ui::classes::display::{category_label, class_name, label, localized, preview_image};
use ui::{ClassEditRow, ClassLoadoutCatalog, ClassPickerFolder, SessionClassStore};

use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};

const PAGE_SIZE: usize = 10;

#[derive(Default)]
pub(crate) struct ClassMenuState {
    row: Option<ClassEditRow>,
    game: Option<FamilyId>,
    folder: Option<ClassPickerFolder>,
    page: usize,
    attachments: bool,
    camo: bool,
    samples: bool,
    hover: usize,
}

fn camo_names(catalog: &ClassLoadoutCatalog, weapon: &str) -> Vec<String> {
    let Some(registry) = catalog.resolver.0.as_deref() else {
        return Vec::new();
    };
    let Ok(id) = session::resolve_editor_class_weapon(
        registry,
        weapon,
        &[],
        asset_game::LoadoutRules::default(),
    ) else {
        return Vec::new();
    };
    registry
        .camouflage_choices(id)
        .into_iter()
        .map(|(_, name)| name.to_owned())
        .collect()
}

fn camo_preview(catalog: &ClassLoadoutCatalog, weapon: &str, camo: &str) -> String {
    let Some(registry) = catalog.resolver.0.as_deref() else {
        return String::new();
    };
    session::resolve_editor_class_weapon(registry, weapon, &[], asset_game::LoadoutRules::default())
        .map_or_else(
            |_| String::new(),
            |id| {
                registry
                    .camouflage_preview(id, camo)
                    .unwrap_or_else(String::new)
            },
        )
}

fn localized_camo_label(
    catalog: &ClassLoadoutCatalog,
    loc: &asset_game::LocalizeCatalog,
    weapon: &str,
    camo: &str,
) -> String {
    let label = || {
        if camo.is_empty() {
            return loc.text("MPUI_NONE");
        }
        let registry = catalog.resolver.0.as_deref()?;
        let id = session::resolve_editor_class_weapon(
            registry,
            weapon,
            &[],
            asset_game::LoadoutRules::default(),
        )
        .ok()?;
        loc.text_in(
            registry.identity_namespace_of(id)?,
            registry.camouflage_caption(id, camo)?,
        )
    };
    let mut label = label()
        .map(str::to_owned)
        .unwrap_or_else(|| camo_label(camo));
    if !camo.is_empty()
        && let Some(registry) = catalog.resolver.0.as_deref()
        && let Ok(id) = session::resolve_editor_class_weapon(
            registry,
            weapon,
            &[],
            asset_game::LoadoutRules::default(),
        )
        && let Some(slot) = registry.camouflage_slot(id, camo)
        && let Some((view, world)) = registry.appearance_status(id, slot)
    {
        let unavailable = |status: &asset_game::AppearanceModelStatus| {
            matches!(
                status,
                asset_game::AppearanceModelStatus::DeclaredUnavailable { .. }
            )
        };
        match (unavailable(&view), unavailable(&world)) {
            (true, true) => label.push_str(" (models unavailable)"),
            (true, false) => label.push_str(" (first-person model unavailable)"),
            (false, true) => label.push_str(" (world model unavailable)"),
            _ => {}
        }
    }
    label
}

fn camo_label(camo: &str) -> String {
    if camo.is_empty() {
        return "None".into();
    }
    camo.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn camo_slot(row: ClassEditRow) -> Option<usize> {
    match row {
        ClassEditRow::Primary => Some(0),
        ClassEditRow::Secondary => Some(1),
        _ => None,
    }
}

impl ClassMenuState {
    fn games(&self, catalog: &ClassLoadoutCatalog) -> Vec<FamilyId> {
        let mut games = Vec::new();
        if let Some(row) = self.row {
            for folder in catalog.categories_for(row) {
                if !games.contains(&folder.namespace) {
                    games.push(folder.namespace);
                }
            }
        }
        games
    }

    fn folders(&self, catalog: &ClassLoadoutCatalog) -> Vec<ClassPickerFolder> {
        self.row
            .map(|row| {
                catalog
                    .categories_for(row)
                    .into_iter()
                    .filter(|folder| Some(folder.namespace) == self.game)
                    .collect()
            })
            .unwrap_or(Vec::new())
    }

    fn choices(&self, catalog: &ClassLoadoutCatalog, store: &SessionClassStore) -> Vec<String> {
        if self.samples {
            return ui::showcase_classes()
                .iter()
                .map(|preset| preset.name.to_owned())
                .collect();
        }
        let Some(row) = self.row else {
            return Vec::new();
        };
        if self.camo {
            let Some(slot) = store.slots.get(store.selected) else {
                return Vec::new();
            };
            std::iter::once(String::new())
                .chain(camo_names(catalog, slot.row_value(row)))
                .collect()
        } else if self.attachments {
            let Some(slot) = store.slots.get(store.selected) else {
                return Vec::new();
            };
            std::iter::once(String::new())
                .chain(
                    catalog
                        .attachments(row, slot.row_value(row))
                        .iter()
                        .cloned(),
                )
                .collect()
        } else if let Some(folder) = self.folder {
            catalog.keys_in_category(row, folder)
        } else {
            catalog.options(row)
        }
    }
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "ui_class_setup",
        "ui_class_samples",
        "ui_class_slot",
        "ui_class_preview_slot",
        "ui_class_edit",
        "ui_class_game",
        "ui_class_category",
        "ui_class_pick",
        "ui_class_page",
        "ui_class_attachments",
        "ui_class_camo",
        "ui_class_reset",
        "ui_class_rename",
        "ui_class_hover",
    ] {
        registry.register(CommandSpec::new(name));
    }
}

fn index(command: &ConsoleCommand) -> Result<usize, String> {
    command
        .args
        .first()
        .and_then(|arg| arg.parse().ok())
        .ok_or_else(|| "Invalid class selection".into())
}

fn open_class_setup(
    state: &mut ClassMenuState,
    store: &mut SessionClassStore,
    player_data: &sim::LocalPlayerData,
    menus: &mut MessageWriter<UiMenuRequest>,
) {
    if !player_data.custom_class_available(store.selected) {
        store.selected = 0;
    }
    *state = ClassMenuState::default();
    menus.write(UiMenuRequest::Open("class_setup".into()));
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ClassEditor<'w, 's> {
    state: Local<'s, ClassMenuState>,
    store: ResMut<'w, SessionClassStore>,
    catalog: Res<'w, ClassLoadoutCatalog>,
    player_data: Res<'w, sim::LocalPlayerData>,
    dvars: ResMut<'w, UiMenuDvars>,
}

pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    mut editor: ClassEditor,
    loading: Option<Res<frame::FrontendContentLoading>>,
    menu_state: hud::MenuState,
    loc: (
        Res<asset_game::LocalizeCatalog>,
        Option<Res<assets::PreparedLocalizedStrings>>,
    ),
    mut menus: MessageWriter<UiMenuRequest>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
) {
    let (shell_loc, prepared_loc) = loc;
    let loc = prepared_loc
        .as_ref()
        .map_or(&*shell_loc, |strings| &strings.0);
    if loading.is_none() && menu_state.is_some_and(|menus| menus.is_open("class_loading")) {
        menus.write(UiMenuRequest::Close("class_loading".into()));
        if let Some(reason) = editor.catalog.progression_lock("cac", &editor.player_data) {
            editor.dvars.set("ui_class_status", &reason);
            echo.write(format!("menu: {reason}"));
        } else {
            open_class_setup(
                &mut editor.state,
                &mut editor.store,
                &editor.player_data,
                &mut menus,
            );
        }
    }
    for command in events
        .read()
        .filter(|command| command.name.starts_with("ui_class_"))
    {
        if command.name == "ui_class_setup" && loading.is_some() {
            menus.write(UiMenuRequest::Open("class_loading".into()));
            continue;
        }
        let result = (|| -> Result<(), String> {
            if !matches!(
                command.name.as_str(),
                "ui_class_hover" | "ui_class_preview_slot"
            ) {
                if let Some(reason) = editor.catalog.progression_lock("cac", &editor.player_data) {
                    return Err(reason);
                }
                if !matches!(command.name.as_str(), "ui_class_setup" | "ui_class_slot")
                    && !editor
                        .player_data
                        .custom_class_available(editor.store.selected)
                {
                    return Err("Custom class slot is locked".into());
                }
            }
            match command.name.as_str() {
                "ui_class_setup" => {
                    open_class_setup(
                        &mut editor.state,
                        &mut editor.store,
                        &editor.player_data,
                        &mut menus,
                    );
                }
                "ui_class_samples" => {
                    *editor.state = ClassMenuState {
                        samples: true,
                        ..Default::default()
                    };
                    menus.write(UiMenuRequest::Open("class_samples_picker".into()));
                }
                "ui_class_hover" => editor.state.hover = index(command)?.min(PAGE_SIZE - 1),
                "ui_class_preview_slot" => {
                    let selected = index(command)?;
                    editor
                        .store
                        .slots
                        .get(selected)
                        .ok_or("Class is unavailable")?;
                    editor.store.selected = selected;
                }
                "ui_class_slot" => {
                    let selected = index(command)?;
                    if !editor.player_data.custom_class_available(selected) {
                        return Err("Custom class slot is locked".into());
                    }
                    let slot = editor
                        .store
                        .slots
                        .get(selected)
                        .ok_or("Class is unavailable")?;
                    editor.dvars.set("ui_class_name", &slot.name);
                    editor.store.selected = selected;
                    *editor.state = ClassMenuState::default();
                    menus.write(UiMenuRequest::Open("class_editor".into()));
                }
                "ui_class_edit" | "ui_class_attachments" => {
                    let row = ClassEditRow::ALL
                        .get(index(command)?)
                        .copied()
                        .ok_or("Unknown loadout slot")?;
                    editor.state.row = Some(row);
                    editor.state.game = None;
                    editor.state.folder = None;
                    editor.state.page = 0;
                    editor.state.hover = 0;
                    editor.state.attachments = command.name == "ui_class_attachments";
                    editor.state.camo = false;
                    editor.state.samples = false;
                    if editor.state.attachments
                        && !matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary)
                    {
                        return Err("This slot does not accept attachments".into());
                    }
                    let menu =
                        if !editor.state.attachments && ClassLoadoutCatalog::uses_categories(row) {
                            "class_games"
                        } else {
                            "class_picker"
                        };
                    menus.write(UiMenuRequest::Open(menu.into()));
                }
                "ui_class_camo" => {
                    let row = match index(command)? {
                        0 => ClassEditRow::Primary,
                        1 => ClassEditRow::Secondary,
                        _ => return Err("Unknown weapon slot".into()),
                    };
                    let slot = editor
                        .store
                        .slots
                        .get(editor.store.selected)
                        .ok_or("Class is unavailable")?;
                    if camo_names(&editor.catalog, slot.row_value(row)).is_empty() {
                        return Err("This weapon has no camouflage".into());
                    }
                    *editor.state = ClassMenuState {
                        row: Some(row),
                        camo: true,
                        ..Default::default()
                    };
                    menus.write(UiMenuRequest::Open("class_picker".into()));
                }
                "ui_class_game" => {
                    let games = editor.state.games(&editor.catalog);
                    editor.state.game =
                        Some(*games.get(index(command)?).ok_or("Game is unavailable")?);
                    editor.state.folder = None;
                    editor.state.page = 0;
                    editor.state.hover = 0;
                    let folders = editor.state.folders(&editor.catalog);
                    if folders.len() == 1 && folders[0].category.is_none() {
                        editor.state.folder = Some(folders[0]);
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    } else {
                        menus.write(UiMenuRequest::Open("class_categories".into()));
                    }
                }
                "ui_class_category" => {
                    editor.state.folder = Some(
                        *editor
                            .state
                            .folders(&editor.catalog)
                            .get(index(command)?)
                            .ok_or("Category is unavailable")?,
                    );
                    editor.state.page = 0;
                    editor.state.hover = 0;
                    menus.write(UiMenuRequest::Open("class_picker".into()));
                }
                "ui_class_page" => {
                    let len = editor.state.choices(&editor.catalog, &editor.store).len();
                    let delta = command
                        .args
                        .first()
                        .and_then(|arg| arg.parse::<isize>().ok())
                        .ok_or("Invalid page")?;
                    editor.state.hover = 0;
                    editor.state.page = editor
                        .state
                        .page
                        .saturating_add_signed(delta)
                        .min(len.saturating_sub(1) / PAGE_SIZE);
                }
                "ui_class_pick" if editor.state.samples => {
                    let at = index(command)?;
                    if at >= PAGE_SIZE {
                        return Err("Invalid sample row".into());
                    }
                    let preset = ui::showcase_classes()
                        .get(editor.state.page * PAGE_SIZE + at)
                        .ok_or("Sample is unavailable")?;
                    let candidate = ui::ClassSlotState::from_preset(preset);
                    editor.catalog.validate_class(&candidate)?;
                    editor
                        .catalog
                        .validate_progression(&candidate, &editor.player_data)?;
                    let selected = editor.store.selected;
                    *editor
                        .store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")? = candidate;
                    editor
                        .dvars
                        .set("ui_class_name", &editor.store.slots[selected].name);
                    menus.write(UiMenuRequest::Close("class_samples_picker".into()));
                    *editor.state = ClassMenuState::default();
                }
                "ui_class_pick" => {
                    let row = editor.state.row.ok_or("No loadout slot selected")?;
                    let at = index(command)?;
                    if at >= PAGE_SIZE {
                        return Err("Invalid item row".into());
                    }
                    let choices = editor.state.choices(&editor.catalog, &editor.store);
                    let value = choices
                        .get(editor.state.page * PAGE_SIZE + at)
                        .ok_or("Item is unavailable")?
                        .clone();
                    let selected = editor.store.selected;
                    let slot = editor
                        .store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?;
                    let mut candidate = slot.clone();
                    if editor.state.camo {
                        let at = camo_slot(row).ok_or("This slot takes no camouflage")?;
                        candidate.camos[at] = value;
                    } else if editor.state.attachments {
                        let mut chosen = if row == ClassEditRow::Primary {
                            candidate.primary_attachments.clone()
                        } else {
                            candidate.secondary_attachments.clone()
                        };
                        if value.is_empty() {
                            chosen.clear();
                        } else if let Some(at) = chosen.iter().position(|name| name == &value) {
                            chosen.remove(at);
                        } else {
                            if candidate.loadout_rules().max_attachments == 1 {
                                chosen.clear();
                            }
                            chosen.push(value);
                        }
                        editor
                            .catalog
                            .check_attachments(
                                candidate.row_value(row),
                                &chosen,
                                candidate.loadout_rules(),
                            )
                            .map_err(|error| format!("{error:?}"))?;
                        if row == ClassEditRow::Primary {
                            candidate.primary_attachments = chosen;
                        } else {
                            candidate.secondary_attachments = chosen;
                        }
                    } else {
                        candidate.set_row(row, value);
                        let rules = candidate.loadout_rules();
                        for (weapon, chosen) in [
                            (&candidate.primary, &mut candidate.primary_attachments),
                            (&candidate.secondary, &mut candidate.secondary_attachments),
                        ] {
                            while !chosen.is_empty()
                                && editor
                                    .catalog
                                    .check_attachments(weapon, chosen, rules)
                                    .is_err()
                            {
                                chosen.pop();
                            }
                        }
                        let weapons = [candidate.primary.clone(), candidate.secondary.clone()];
                        for (weapon, camo) in weapons.iter().zip(&mut candidate.camos) {
                            if !camo.is_empty()
                                && !camo_names(&editor.catalog, weapon).contains(camo)
                            {
                                camo.clear();
                            }
                        }
                    }
                    editor.catalog.validate_edit(&candidate, row)?;
                    if let Some(reason) = editor
                        .catalog
                        .progression_lock(candidate.row_value(row), &editor.player_data)
                    {
                        return Err(reason);
                    }
                    if candidate.perk1 == "specialty_bling"
                        && candidate.secondary_attachments.len() > 1
                        && let Some(reason) = editor
                            .catalog
                            .progression_lock("specialty_secondarybling", &editor.player_data)
                    {
                        return Err(reason);
                    }
                    candidate.lock_reason = editor.catalog.validate_class(&candidate).err();
                    if slot.lock_reason.is_none()
                        && let Some(reason) = &candidate.lock_reason
                    {
                        return Err(reason.clone());
                    }
                    *slot = candidate;
                    if editor.state.camo {
                        let camo = camo_slot(row).map_or("", |at| slot.camos[at].as_str());
                        echo.write(format!(
                            "menu: class {} {} camouflage = {}",
                            selected + 1,
                            row.label(),
                            localized_camo_label(&editor.catalog, loc, slot.row_value(row), camo)
                        ));
                    } else {
                        echo.write(format!(
                            "menu: class {} {} = {}",
                            selected + 1,
                            row.label(),
                            slot.row_value(row)
                        ));
                    }
                    for menu in ["class_picker", "class_categories", "class_games"] {
                        menus.write(UiMenuRequest::Close(menu.into()));
                    }
                    let has_camo = !camo_names(&editor.catalog, slot.row_value(row)).is_empty();
                    if !editor.state.attachments
                        && !editor.state.camo
                        && matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary)
                    {
                        editor.state.attachments = true;
                        editor.state.page = 0;
                        editor.state.hover = 0;
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    } else if editor.state.attachments && has_camo {
                        editor.state.attachments = false;
                        editor.state.camo = true;
                        editor.state.page = 0;
                        editor.state.hover = 0;
                        menus.write(UiMenuRequest::Open("class_picker".into()));
                    }
                }
                "ui_class_reset" => {
                    let selected = editor.store.selected;
                    let slot = editor
                        .store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?;
                    let candidate = if let Some(preset) = ui::showcase_classes()
                        .iter()
                        .find(|preset| preset.name == slot.name)
                    {
                        ui::ClassSlotState::from_preset(preset)
                    } else {
                        let registry = editor
                            .catalog
                            .resolver
                            .0
                            .as_deref()
                            .ok_or("Weapon catalog is not ready")?;
                        let mut default = ui::SessionClassStore::from_showcase(0, registry)
                            .slots
                            .into_iter()
                            .nth(selected)
                            .ok_or("No available default class")?;
                        default.name.clone_from(&slot.name);
                        default
                    };
                    editor.catalog.validate_class(&candidate)?;
                    editor
                        .catalog
                        .validate_progression(&candidate, &editor.player_data)?;
                    *slot = candidate;
                    editor.dvars.set("ui_class_name", &slot.name);
                }
                "ui_class_rename" => {
                    let name: String = editor
                        .dvars
                        .get("ui_class_name")
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(20)
                        .collect();
                    if name.is_empty() {
                        return Err(localized(
                            loc,
                            "MENU_IWNET_CREATE_BADNAME",
                            "Enter a class name",
                        ));
                    }
                    let selected = editor.store.selected;
                    editor
                        .store
                        .slots
                        .get_mut(selected)
                        .ok_or("Class is unavailable")?
                        .name = name;
                    menus.write(UiMenuRequest::Close("class_rename".into()));
                }
                _ => {}
            }
            if !matches!(
                command.name.as_str(),
                "ui_class_hover" | "ui_class_preview_slot"
            ) {
                editor.dvars.set("ui_class_status", "");
            }
            Ok(())
        })();
        if let Err(error) = result {
            editor.dvars.set("ui_class_status", &error);
            echo.write(format!("menu: {error}"));
        }
    }
    let feature_lock = loading
        .is_none()
        .then(|| editor.catalog.progression_lock("cac", &editor.player_data))
        .flatten();
    editor.dvars.set(
        "ui_class_available",
        if feature_lock.is_none() { "1" } else { "0" },
    );
    editor.dvars.set(
        "ui_class_feature_lock",
        feature_lock.clone().unwrap_or(String::new()),
    );
    editor.dvars.set(
        "ui_class_feature_label",
        feature_lock.map_or_else(
            || "CLASSES".to_owned(),
            |reason| format!("CLASSES — {reason}"),
        ),
    );
    for at in 0..10 {
        editor.dvars.set(
            &format!("ui_class_slot_{at}"),
            editor
                .store
                .slots
                .get(at)
                .filter(|_| editor.player_data.custom_class_available(at))
                .map(|slot| class_name(&slot.name))
                .unwrap_or(String::new()),
        );
    }
    if let Some(slot) = editor.store.slots.get(editor.store.selected) {
        if editor
            .dvars
            .get("ui_class_status")
            .is_none_or(str::is_empty)
            && let Some(reason) = editor.store.lock_reason(editor.store.selected)
        {
            editor
                .dvars
                .set("ui_class_status", format!("Class unavailable: {reason}"));
        }
        editor.dvars.set("ui_class_title", class_name(&slot.name));
        editor.dvars.set("ui_class_saved_name", &slot.name);
        editor
            .dvars
            .set("ui_class_index", editor.store.selected.to_string());
        for (at, row, weapon) in [
            (0, ClassEditRow::Primary, &slot.primary),
            (1, ClassEditRow::Secondary, &slot.secondary),
        ] {
            let caption = localized(loc, row.loc_key().trim_start_matches('@'), row.label());
            for (kind, key, fallback, available) in [
                (
                    "attachments",
                    "MENU_ATTACHMENTS_CAPS",
                    "Attachments",
                    !editor.catalog.attachments(row, weapon).is_empty(),
                ),
                (
                    "camo",
                    "MENU_CAMO_CAPS",
                    "Camouflage",
                    !camo_names(&editor.catalog, weapon).is_empty(),
                ),
            ] {
                editor.dvars.set(
                    &format!("ui_class_has_{kind}_{at}"),
                    if available { "1" } else { "0" },
                );
                editor.dvars.set(
                    &format!("ui_class_{kind}_button_{at}"),
                    format!("{caption}: {}", localized(loc, key, fallback)),
                );
            }
        }
        for row in ClassEditRow::ALL {
            editor.dvars.set(
                &format!("ui_class_value_{}", row.as_u8()),
                label(slot.row_value(row), &editor.catalog, loc),
            );
            editor.dvars.set(
                &format!("ui_class_image_{}", row.as_u8()),
                preview_image(slot.row_value(row), &editor.catalog),
            );
            editor.dvars.set(
                &format!("ui_class_desc_{}", row.as_u8()),
                editor
                    .catalog
                    .previews
                    .get(slot.row_value(row))
                    .and_then(|preview| loc.text(preview.desc_key.trim_start_matches('@')))
                    .unwrap_or(""),
            );
        }
        for (at, camo) in slot.camos.iter().enumerate() {
            editor.dvars.set(
                &format!("ui_class_camo_image_{at}"),
                camo_preview(
                    &editor.catalog,
                    if at == 0 {
                        &slot.primary
                    } else {
                        &slot.secondary
                    },
                    camo,
                ),
            );
        }
        for (name, selected) in [
            ("primary", &slot.primary_attachments),
            ("secondary", &slot.secondary_attachments),
        ] {
            for at in 0..2 {
                let weapon = if name == "primary" {
                    &slot.primary
                } else {
                    &slot.secondary
                };
                let key = selected
                    .get(at)
                    .map(|attachment| format!("{weapon}+{attachment}"))
                    .unwrap_or(String::new());
                editor.dvars.set(
                    &format!("ui_class_{name}_attachment_image_{at}"),
                    preview_image(&key, &editor.catalog),
                );
                editor.dvars.set(
                    &format!("ui_class_{name}_attachment_name_{at}"),
                    if key.is_empty() {
                        String::new()
                    } else {
                        label(&key, &editor.catalog, loc)
                    },
                );
            }
            editor.dvars.set(
                &format!("ui_class_{name}_attachments"),
                if selected.is_empty() {
                    label("", &editor.catalog, loc)
                } else {
                    selected
                        .iter()
                        .map(|attachment| {
                            label(
                                &format!(
                                    "{}+{attachment}",
                                    if name == "primary" {
                                        &slot.primary
                                    } else {
                                        &slot.secondary
                                    }
                                ),
                                &editor.catalog,
                                loc,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ")
                },
            );
        }
    }
    let games = editor.state.games(&editor.catalog);
    let folders = editor.state.folders(&editor.catalog);
    let choices = editor.state.choices(&editor.catalog, &editor.store);
    editor
        .dvars
        .set("ui_class_game_count", games.len().to_string());
    editor
        .dvars
        .set("ui_class_category_count", folders.len().to_string());
    editor.dvars.set(
        "ui_class_choice_count",
        choices
            .len()
            .saturating_sub(editor.state.page * PAGE_SIZE)
            .min(PAGE_SIZE)
            .to_string(),
    );
    editor.dvars.set(
        "ui_class_more_pages",
        if choices.len() > PAGE_SIZE { "1" } else { "0" },
    );
    let picker_depth = if editor.state.attachments
        || editor.state.camo
        || editor
            .state
            .row
            .is_none_or(|row| !ClassLoadoutCatalog::uses_categories(row))
    {
        1
    } else if editor
        .state
        .folder
        .is_some_and(|folder| folder.category.is_none())
    {
        2
    } else {
        3
    };
    editor
        .dvars
        .set("ui_class_picker_depth", picker_depth.to_string());
    let mut caption = editor.state.row.map_or_else(
        || localized(loc, "MENU_CLASSES", "Classes"),
        |row| localized(loc, row.loc_key(), row.label()),
    );
    if editor.state.attachments {
        caption.push_str(" / ");
        caption.push_str(&localized(loc, "MENU_ATTACHMENTS_CAPS", "Attachments"));
    } else if editor.state.camo {
        caption.push_str(" / ");
        caption.push_str(&localized(loc, "MENU_CAMO_CAPS", "Camouflage"));
    }
    editor.dvars.set("ui_class_caption", caption);
    editor.dvars.set(
        "ui_class_game_label",
        editor
            .state
            .game
            .map(|game| game.as_str().to_uppercase())
            .unwrap_or(String::new()),
    );
    editor.dvars.set(
        "ui_class_category_label",
        editor
            .state
            .folder
            .map(|folder| category_label(folder, loc))
            .unwrap_or(String::new()),
    );
    editor.state.page = editor
        .state
        .page
        .min(choices.len().saturating_sub(1) / PAGE_SIZE);
    for at in 0..PAGE_SIZE {
        let choice_lock = choices
            .get(editor.state.page * PAGE_SIZE + at)
            .and_then(|value| {
                if editor.state.samples {
                    let preset = ui::showcase_classes()
                        .iter()
                        .find(|preset| preset.name == value)?;
                    let candidate = ui::ClassSlotState::from_preset(preset);
                    editor
                        .catalog
                        .validate_class(&candidate)
                        .and_then(|()| {
                            editor
                                .catalog
                                .validate_progression(&candidate, &editor.player_data)
                        })
                        .err()
                } else if !editor.state.attachments && !editor.state.camo {
                    editor.catalog.progression_lock(value, &editor.player_data)
                } else {
                    None
                }
            });
        editor.dvars.set(
            &format!("ui_class_choice_unavailable_{at}"),
            choice_lock.unwrap_or(String::new()),
        );
        editor.dvars.set(
            &format!("ui_class_game_{at}"),
            games
                .get(at)
                .map(|game| game.as_str().to_uppercase())
                .unwrap_or(String::new()),
        );
        editor.dvars.set(
            &format!("ui_class_category_{at}"),
            folders
                .get(at)
                .map(|folder| category_label(*folder, loc))
                .unwrap_or(String::new()),
        );
        editor.dvars.set(
            &format!("ui_class_choice_{at}"),
            choices
                .get(editor.state.page * PAGE_SIZE + at)
                .map(|value| {
                    let slot = editor.store.slots.get(editor.store.selected);
                    if editor.state.samples {
                        let preset = ui::showcase_classes()
                            .iter()
                            .find(|preset| preset.name == value)
                            .expect("sample choice exists");
                        let candidate = ui::ClassSlotState::from_preset(preset);
                        let reason = editor
                            .catalog
                            .validate_class(&candidate)
                            .and_then(|()| {
                                editor
                                    .catalog
                                    .validate_progression(&candidate, &editor.player_data)
                            })
                            .err();
                        return format!(
                            "{}{}",
                            class_name(value),
                            reason.map_or_else(String::new, |reason| format!(" — {reason}"))
                        );
                    }
                    if editor.state.camo {
                        let selected = slot
                            .zip(editor.state.row.and_then(camo_slot))
                            .is_some_and(|(slot, at)| slot.camos[at] == *value);
                        return format!(
                            "{}{}",
                            if selected { "* " } else { "" },
                            localized_camo_label(
                                &editor.catalog,
                                loc,
                                slot.zip(editor.state.row)
                                    .map_or("", |(slot, row)| slot.row_value(row)),
                                value
                            )
                        );
                    }
                    let key = if editor.state.attachments && !value.is_empty() {
                        slot.zip(editor.state.row)
                            .map(|(slot, row)| format!("{}+{value}", slot.row_value(row)))
                            .unwrap_or(String::new())
                    } else {
                        value.clone()
                    };
                    let selected = slot.zip(editor.state.row).is_some_and(|(slot, row)| {
                        if editor.state.attachments {
                            let chosen = if row == ClassEditRow::Primary {
                                &slot.primary_attachments
                            } else {
                                &slot.secondary_attachments
                            };
                            if value.is_empty() {
                                chosen.is_empty()
                            } else {
                                chosen.contains(value)
                            }
                        } else {
                            slot.row_value(row) == value
                        }
                    });
                    let lock = (!editor.state.attachments)
                        .then(|| editor.catalog.progression_lock(value, &editor.player_data))
                        .flatten();
                    format!(
                        "{}{}{}",
                        if selected { "* " } else { "" },
                        label(&key, &editor.catalog, loc),
                        lock.map_or_else(String::new, |reason| format!(" — {reason}"))
                    )
                })
                .unwrap_or(String::new()),
        );
    }
    let title = if editor.state.camo {
        localized(loc, "MENU_CAMO_CAPS", "Camouflage")
    } else if editor.state.attachments {
        localized(loc, "MENU_ATTACHMENTS_CAPS", "Attachments")
    } else if let Some(folder) = editor.state.folder {
        format!(
            "{} / {}",
            folder.namespace.as_str().to_uppercase(),
            category_label(folder, loc)
        )
    } else {
        editor.state.row.map_or_else(
            || localized(loc, "MENU_WEAPON_CLASSES_CAPS", "Classes"),
            |row| localized(loc, row.loc_key(), row.label()),
        )
    };
    editor.dvars.set("ui_class_picker_title", title);
    let preview = choices
        .get(editor.state.page * PAGE_SIZE + editor.state.hover)
        .filter(|_| !editor.state.camo)
        .and_then(|key| {
            let lookup = if editor.state.samples {
                ui::showcase_classes()
                    .iter()
                    .find(|preset| preset.name == key)?
                    .primary
                    .to_owned()
            } else if editor.state.attachments {
                let slot = editor.store.slots.get(editor.store.selected)?;
                format!("{}+{key}", slot.row_value(editor.state.row?))
            } else {
                key.clone()
            };
            let preview = editor.catalog.previews.get(&lookup)?;
            let ns = asset_core::AssetKey::parse(&lookup).ok()?.namespace;
            Some((ns, preview))
        });
    let image = preview
        .filter(|(_, preview)| !preview.image.is_empty())
        .map(|(ns, preview)| {
            if asset_core::AssetKey::parse(&preview.image).is_ok() {
                preview.image.clone()
            } else {
                format!("{}:material/{}", ns.as_str(), preview.image)
            }
        })
        .unwrap_or(String::new());
    let image = if editor.state.camo {
        choices
            .get(editor.state.page * PAGE_SIZE + editor.state.hover)
            .filter(|camo| !camo.is_empty())
            .map_or_else(String::new, |camo| {
                camo_preview(
                    &editor.catalog,
                    editor
                        .store
                        .slots
                        .get(editor.store.selected)
                        .zip(editor.state.row)
                        .map_or("", |(slot, row)| slot.row_value(row)),
                    camo,
                )
            })
    } else {
        image
    };
    editor.dvars.set("ui_class_preview", image);
    let square_preview = editor.state.attachments
        || matches!(
            editor.state.row,
            Some(
                ClassEditRow::Perk1
                    | ClassEditRow::Perk2
                    | ClassEditRow::Perk3
                    | ClassEditRow::Deathstreak
            )
        );
    editor.dvars.set(
        "ui_class_preview_width",
        if square_preview { "100" } else { "200" },
    );
    editor.dvars.set(
        "ui_class_preview_title",
        choices
            .get(editor.state.page * PAGE_SIZE + editor.state.hover)
            .map(|key| {
                let weapon = editor
                    .store
                    .slots
                    .get(editor.store.selected)
                    .zip(editor.state.row)
                    .map_or("", |(slot, row)| slot.row_value(row));
                if editor.state.samples {
                    class_name(key)
                } else if editor.state.camo {
                    localized_camo_label(&editor.catalog, loc, weapon, key)
                } else if editor.state.attachments && !key.is_empty() {
                    label(&format!("{weapon}+{key}"), &editor.catalog, loc)
                } else {
                    label(key, &editor.catalog, loc)
                }
            })
            .unwrap_or(String::new()),
    );
    for (at, bar) in asset_game::CacStatBar::ALL.into_iter().enumerate() {
        let value = preview.and_then(|(_, p)| {
            p.bars
                .iter()
                .find(|(kind, _)| *kind == bar)
                .map(|(_, v)| *v)
        });
        editor.dvars.set(
            &format!("ui_class_stat_{at}"),
            value.unwrap_or(0).clamp(0, 100).to_string(),
        );
        editor.dvars.set(
            &format!("ui_class_stat_visible_{at}"),
            if value.is_some() { "1" } else { "0" },
        );
    }
    editor.dvars.set(
        "ui_class_description",
        if editor.state.samples {
            choices
                .get(editor.state.page * PAGE_SIZE + editor.state.hover)
                .and_then(|name| {
                    ui::showcase_classes()
                        .iter()
                        .find(|preset| preset.name == name)
                })
                .map(|preset| {
                    let slot = ui::ClassSlotState::from_preset(preset);
                    ClassEditRow::ALL
                        .into_iter()
                        .map(|row| {
                            format!(
                                "{}: {}",
                                localized(loc, row.loc_key(), row.label()),
                                label(slot.row_value(row), &editor.catalog, loc)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or(String::new())
        } else {
            preview
                .and_then(|(_, preview)| loc.text(preview.desc_key.trim_start_matches('@')))
                .unwrap_or("")
                .to_owned()
        },
    );
    editor.dvars.set(
        "ui_class_page",
        format!(
            "{} / {}",
            editor.state.page + 1,
            choices.len().max(1).div_ceil(PAGE_SIZE)
        ),
    );
}
