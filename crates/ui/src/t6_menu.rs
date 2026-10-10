use crate::ClassLoadoutCatalog;
use crate::classes::{
    setup::{ClassEditRow, ClassSlotState},
    store::{decode_slots, encode_slots, write_class_file},
};
use crate::layers::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};
use asset_core::AssetNamespace;
use assets::{PreparedWeapons, SessionMapIdentity};
use bevy::prelude::*;
use frame::{AppScreen, GameSettings, NativeGameMenu, UiExecCommand, UiMenuKey, UiMenuRequest};
use net::{ActionRequestIds, ClientActionInbox, LocalPresentClient, ReliableControlEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Pause,
    Classes,
    Edit,
    Settings,
    Video,
    Audio,
    Controls,
    Bindings,
    Picker,
    Rename,
}

#[derive(Resource, Default)]
struct Menu {
    page: Page,
    focus: usize,
    slot: usize,
    profiles: Vec<ClassSlotState>,
    path: Option<std::path::PathBuf>,
    saved: Option<String>,
    preserve_file: bool,
    save_retry: Option<std::time::Instant>,
    pending: Option<u32>,
    host: bool,
    zombies: bool,
    notice: String,
    picker: Vec<String>,
    pick_row: Option<ClassEditRow>,
    pick_attachment: bool,
    pick_page: usize,
    rename: String,
    binding_active: bool,
}

#[derive(Component)]
struct Root;

#[derive(Component, Clone, Copy)]
struct Choice {
    order: usize,
    action: Action,
}

#[derive(Clone, Copy)]
enum Action {
    Resume,
    Classes,
    Settings,
    Back,
    Edit(usize),
    Weapon(ClassEditRow),
    Attachment(ClassEditRow),
    Equip,
    Volume,
    Fov,
    Sensitivity,
    Invert,
    Vsync,
    Leave,
    EndMatch,
    Video,
    Audio,
    Controls,
    Bindings,
    Bind(usize),
    Pick(usize),
    Previous,
    Next,
    CopyPrevious,
    ClearExtras,
    Rename,
    SaveName,
    Fullscreen,
    Resolution,
    Brightness,
    Shadows,
    Bloom,
    DepthOfField,
    PadSensitivity,
    PadAds,
    PadInvert,
    PadVibration,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<Menu>()
        .init_resource::<crate::t6_art::T6Art>()
        .add_systems(
            Update,
            crate::t6_art::sync_publication.before(super::t6_text::NativeUiPaint),
        )
        .add_systems(
            Update,
            (drive, paint)
                .chain()
                .in_set(frame::ClientSet::Ui)
                .in_set(super::t6_text::NativeUiPaint),
        );
}

fn native(slot: &ClassSlotState) -> bool {
    [&slot.primary, &slot.secondary, &slot.lethal, &slot.tactical]
        .into_iter()
        .all(|key| key.is_empty() || key.starts_with("t6:"))
        && !slot.primary.is_empty()
        && [&slot.perk1, &slot.perk2, &slot.perk3, &slot.deathstreak]
            .into_iter()
            .all(|key| key.is_empty() || key == "specialty_null")
}

fn available(slot: &ClassSlotState, registry: &std::sync::Arc<asset_game::WeaponRegistry>) -> bool {
    if !native(slot) {
        return false;
    }
    let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
    session::ClassWeaponAdmission::prepare(&registry.editor_catalog()).allows(&row)
}

fn load(
    menu: &mut Menu,
    identity: &frame::LaunchIdentity,
    catalog: &ClassLoadoutCatalog,
    registry: &std::sync::Arc<asset_game::WeaponRegistry>,
) {
    if menu.path.is_some()
        || !catalog
            .primary
            .iter()
            .any(|offer| offer.key.starts_with("t6:"))
    {
        return;
    }
    let path = std::env::var_os("IW4L_PROFILE_PATH")
        .map(std::path::PathBuf::from)
        .map(|path| path.with_extension("t6-classes.txt"))
        .unwrap_or_else(|| identity.artifacts.join("profile/t6-classes.txt"));
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.len() <= 65536 => {
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| decode_slots(&text))
            {
                Some(slots) if slots.len() <= 5 && slots.iter().all(native) => {
                    menu.profiles = slots
                }
                _ => {
                    menu.preserve_file = true;
                    menu.notice =
                        "The saved BO2 classes could not be read; the file is preserved.".into();
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            menu.preserve_file = true;
            menu.notice = "The saved BO2 classes could not be read; the file is preserved.".into();
        }
    }
    if menu.profiles.is_empty() {
        let mut base = ClassSlotState::from_host_slot(&frame::HostClassSlot {
            name: "Custom 1".into(),
            primary: String::new(),
            secondary: String::new(),
            primary_attachments: vec![],
            secondary_attachments: vec![],
            lethal: String::new(),
            tactical: String::new(),
            perks: Default::default(),
            deathstreak: String::new(),
            camos: Default::default(),
        });
        let primary: Vec<_> = catalog
            .primary
            .iter()
            .filter(|offer| offer.key.starts_with("t6:"))
            .filter(|offer| {
                base.primary = offer.key.clone();
                available(&base, registry)
            })
            .map(|offer| offer.key.clone())
            .collect();
        for index in 0..5 {
            let Some(key) = primary.get(index % primary.len().max(1)) else {
                break;
            };
            base.primary = key.clone();
            base.name = format!("Custom {}", index + 1);
            menu.profiles.push(base.clone());
        }
    }
    menu.saved = Some(encode_slots(&menu.profiles));
    menu.path = Some(path);
}

const PICK_PAGE: usize = 7;
const BINDS: [(&str, &str); 10] = [
    ("FIRE", "+attack"),
    ("AIM DOWN SIGHTS", "+toggleads_throw"),
    ("RELOAD", "+reload"),
    ("USE", "+activate"),
    ("JUMP", "+gostand"),
    ("SPRINT", "+breath_sprint"),
    ("CROUCH", "+movedown"),
    ("MELEE", "+melee"),
    ("LETHAL EQUIPMENT", "+frag"),
    ("TACTICAL EQUIPMENT", "+smoke"),
];

fn rows(menu: &Menu) -> usize {
    match menu.page {
        Page::Pause => 4 + usize::from(menu.host) - usize::from(menu.zombies),
        Page::Classes => menu.profiles.len() + 1,
        Page::Edit => 11,
        Page::Rename => 2,
        Page::Settings => 5,
        Page::Video => 9,
        Page::Audio => 2,
        Page::Controls => 8,
        Page::Bindings => BINDS.len() + 1,
        Page::Picker => {
            menu.picker
                .len()
                .saturating_sub(menu.pick_page * PICK_PAGE)
                .min(PICK_PAGE)
                + usize::from(menu.pick_page > 0)
                + usize::from((menu.pick_page + 1) * PICK_PAGE < menu.picker.len())
                + 1
        }
    }
}

fn changed_item(
    slot: &ClassSlotState,
    row: ClassEditRow,
    attachment: bool,
    key: String,
) -> ClassSlotState {
    let mut changed = slot.clone();
    if attachment {
        let list = if row == ClassEditRow::Primary {
            &mut changed.primary_attachments
        } else {
            &mut changed.secondary_attachments
        };
        *list = if key.is_empty() { vec![] } else { vec![key] };
    } else {
        changed.set_row(row, key);
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn drive(
    screen: Res<AppScreen>,
    role: Res<frame::RuntimeRole>,
    map: Option<Res<SessionMapIdentity>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    catalog: Res<ClassLoadoutCatalog>,
    weapons: Option<Res<PreparedWeapons>>,
    input: (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<frame::HudInputView>,
        Res<frame::UiBindingCapture>,
        MessageReader<bevy::input::keyboard::KeyboardInput>,
    ),
    mut requests: MessageReader<UiMenuRequest>,
    mut reliable: MessageReader<ReliableControlEvent>,
    mut menu: ResMut<Menu>,
    mut open: ResMut<NativeGameMenu>,
    mut settings: ResMut<GameSettings>,
    choices: Query<(&Interaction, &Choice)>,
    authority: (
        ResMut<ClientActionInbox>,
        ResMut<ActionRequestIds>,
        Res<LocalPresentClient>,
    ),
    mut output: (
        MessageWriter<UiExecCommand>,
        MessageWriter<frame::UiBindRequest>,
    ),
) {
    let (keys, mouse, hud, bindings, mut keyboard) = input;
    let text_events: Vec<_> = keyboard
        .read()
        .filter(|e| e.state.is_pressed())
        .filter_map(|e| e.text.as_ref().map(|t| t.to_string()))
        .collect();
    let (ref mut exec, ref mut bind) = output;
    let (mut actions, mut ids, local) = authority;
    menu.host = matches!(
        *role,
        frame::RuntimeRole::Listen | frame::RuntimeRole::Dedicated
    );
    let messages: Vec<_> = requests.read().cloned().collect();
    let active = *screen == AppScreen::InGame
        && map
            .as_ref()
            .is_some_and(|map| map.namespace == Some(AssetNamespace::T6));
    if !active {
        open.0 = false;
        if menu.pending.is_some() {
            menu.pending = None;
        }
        reliable.clear();
        return;
    }
    menu.zombies = map.as_ref().is_some_and(|map| map.zone.starts_with("zm_"));
    if !menu.zombies {
        if let (Some(identity), Some(weapons)) = (identity.as_deref(), weapons.as_deref()) {
            load(&mut menu, identity, &catalog, weapons.registry());
        }
    }
    for event in reliable.read() {
        match &event.0 {
            sim::SimEvent::ClassAccepted { request_id, .. }
                if menu.pending == Some(*request_id) =>
            {
                menu.pending = None;
                menu.notice = "Loadout accepted. It applies on your next respawn.".into();
                open.0 = false;
            }
            sim::SimEvent::ClassRejected {
                request_id, reason, ..
            } if menu.pending == Some(*request_id) => {
                menu.pending = None;
                menu.notice = format!("Loadout refused: {}", reason.as_str());
            }
            _ => {}
        }
    }
    if menu.binding_active && bindings.command.is_none() {
        menu.binding_active = false;
        menu.notice =
            "Key binding capture finished. Saved bindings are available in the launcher settings."
                .into();
    }
    if bindings.command.is_some() {
        menu.binding_active = true;
    }
    if hud.console_open || bindings.command.is_some() || bindings.consumed_input {
        return;
    }
    if open.0 && menu.page == Page::Rename {
        for text in text_events.iter().chain(messages.iter().filter_map(|m| {
            if let UiMenuRequest::Text(t) = m {
                Some(t)
            } else {
                None
            }
        })) {
            for ch in text.chars().filter(|ch| !ch.is_control() && *ch != ',') {
                if menu.rename.chars().count() < 24 {
                    menu.rename.push(ch);
                }
            }
        }
        if keys.just_pressed(KeyCode::Backspace)
            || messages
                .iter()
                .any(|m| *m == UiMenuRequest::Key(UiMenuKey::Backspace))
        {
            menu.rename.pop();
        }
    }
    let mut navigation: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            UiMenuRequest::Key(key) => Some(*key),
            UiMenuRequest::Toggle => Some(UiMenuKey::Escape),
            _ => None,
        })
        .collect();
    let mut seen = Vec::new();
    navigation.retain(|key| {
        if seen.contains(key) {
            false
        } else {
            seen.push(*key);
            true
        }
    });
    if messages
        .iter()
        .any(|message| matches!(message, UiMenuRequest::Open(name) if name == "t6/pause"))
    {
        open.0 = true;
        menu.page = Page::Pause;
        menu.focus = 0;
    }
    for (code, key) in [
        (KeyCode::Escape, UiMenuKey::Escape),
        (KeyCode::Enter, UiMenuKey::Enter),
        (KeyCode::ArrowUp, UiMenuKey::Up),
        (KeyCode::ArrowDown, UiMenuKey::Down),
        (KeyCode::ArrowLeft, UiMenuKey::Left),
        (KeyCode::ArrowRight, UiMenuKey::Right),
    ] {
        if keys.just_pressed(code) && !navigation.contains(&key) {
            navigation.push(key);
        }
    }
    let mut selected = Vec::new();
    let mut direction = 1.0;
    for key in navigation {
        match key {
            UiMenuKey::Escape => {
                if !open.0 {
                    open.0 = true;
                    menu.page = Page::Pause;
                    menu.focus = 0;
                } else if menu.page == Page::Pause {
                    open.0 = false;
                } else {
                    selected.push(Action::Back);
                }
            }
            UiMenuKey::Up if open.0 => menu.focus = (menu.focus + rows(&menu) - 1) % rows(&menu),
            UiMenuKey::Down if open.0 => menu.focus = (menu.focus + 1) % rows(&menu),
            UiMenuKey::Left | UiMenuKey::Right
                if open.0 && matches!(menu.page, Page::Video | Page::Audio | Page::Controls) =>
            {
                direction = if key == UiMenuKey::Left { -1.0 } else { 1.0 };
                if let Some((_, choice)) = choices
                    .iter()
                    .find(|(_, choice)| choice.order == menu.focus)
                {
                    if !matches!(choice.action, Action::Back | Action::Bindings) {
                        selected.push(choice.action);
                    }
                }
            }
            UiMenuKey::Enter if open.0 => {
                if let Some((_, choice)) = choices
                    .iter()
                    .find(|(_, choice)| choice.order == menu.focus)
                {
                    selected.push(choice.action);
                }
            }
            _ => {}
        }
    }
    if open.0 && mouse.just_pressed(MouseButton::Left) {
        if let Some((_, choice)) = choices
            .iter()
            .find(|(interaction, _)| **interaction == Interaction::Pressed)
        {
            selected.push(choice.action);
        }
    }
    for action in selected {
        match action {
            Action::Resume => open.0 = false,
            Action::Classes => {
                menu.page = Page::Classes;
                menu.focus = 0;
            }
            Action::Settings => {
                menu.page = Page::Settings;
                menu.focus = 0;
            }
            Action::Back => {
                menu.page = match menu.page {
                    Page::Edit => Page::Classes,
                    Page::Picker | Page::Rename => Page::Edit,
                    Page::Video | Page::Audio | Page::Controls => Page::Settings,
                    Page::Bindings => Page::Controls,
                    _ => Page::Pause,
                };
                menu.focus = 0;
            }
            Action::Edit(slot) => {
                menu.slot = slot;
                menu.page = Page::Edit;
                menu.focus = 0;
            }
            Action::Weapon(row) | Action::Attachment(row) => {
                let Some(registry) = weapons.as_ref().map(|weapons| weapons.registry()) else {
                    continue;
                };
                let index = menu.slot;
                let Some(slot) = menu.profiles.get(index).cloned() else {
                    continue;
                };
                let attachment = matches!(action, Action::Attachment(_));
                let candidates: Vec<String> = if attachment {
                    std::iter::once(String::new())
                        .chain(
                            catalog
                                .attachments(row, slot.row_value(row))
                                .iter()
                                .cloned(),
                        )
                        .collect()
                } else {
                    let offers = match row {
                        ClassEditRow::Primary => &catalog.primary,
                        ClassEditRow::Secondary => &catalog.secondary,
                        ClassEditRow::Lethal => &catalog.lethal,
                        _ => &catalog.tactical,
                    };
                    std::iter::once(String::new())
                        .filter(|_| row != ClassEditRow::Primary)
                        .chain(
                            offers
                                .iter()
                                .filter(|offer| offer.key.starts_with("t6:"))
                                .map(|offer| offer.key.clone()),
                        )
                        .collect()
                };
                let editor = registry.editor_catalog();
                let admission = session::ClassWeaponAdmission::prepare(&editor);
                menu.picker = candidates
                    .into_iter()
                    .filter(|key| {
                        let changed = changed_item(&slot, row, attachment, key.clone());
                        let class = session::ClassRow::from(&frame::HostClassSlot::from(&changed));
                        native(&changed) && admission.allows(&class)
                    })
                    .collect();
                menu.pick_row = Some(row);
                menu.pick_attachment = attachment;
                menu.pick_page = 0;
                menu.page = Page::Picker;
                menu.focus = 0;
            }
            Action::Pick(index) => {
                if let (Some(row), Some(key), Some(slot), Some(weapons)) = (
                    menu.pick_row,
                    menu.picker.get(index).cloned(),
                    menu.profiles.get(menu.slot).cloned(),
                    weapons.as_deref(),
                ) {
                    let changed = changed_item(&slot, row, menu.pick_attachment, key);
                    if available(&changed, weapons.registry()) {
                        let index = menu.slot;
                        menu.profiles[index] = changed;
                        menu.page = Page::Edit;
                        menu.focus = 0;
                        if !menu.preserve_file {
                            menu.notice.clear();
                        }
                    } else {
                        menu.notice = "This option is no longer prepared.".into();
                    }
                }
            }
            Action::Previous => {
                menu.pick_page = menu.pick_page.saturating_sub(1);
                menu.focus = 0;
            }
            Action::Next => {
                menu.pick_page += 1;
                menu.focus = 0;
            }
            Action::CopyPrevious => {
                let index = menu.slot;
                let previous = (index + menu.profiles.len() - 1) % menu.profiles.len();
                let name = menu.profiles[index].name.clone();
                menu.profiles[index] = menu.profiles[previous].clone();
                menu.profiles[index].name = name;
                menu.notice = "Copied the previous custom class.".into();
            }
            Action::Rename => {
                menu.rename = menu.profiles[menu.slot].name.clone();
                menu.page = Page::Rename;
                menu.focus = 0;
            }
            Action::SaveName => {
                let name = menu.rename.trim().to_owned();
                if !name.is_empty() {
                    let index = menu.slot;
                    menu.profiles[index].name = name;
                    menu.page = Page::Edit;
                    menu.focus = 0;
                } else {
                    menu.notice = "Enter a class name.".into();
                }
            }
            Action::ClearExtras => {
                let index = menu.slot;
                let mut changed = menu.profiles[index].clone();
                changed.secondary.clear();
                changed.lethal.clear();
                changed.tactical.clear();
                changed.primary_attachments.clear();
                changed.secondary_attachments.clear();
                if weapons
                    .as_deref()
                    .is_some_and(|w| available(&changed, w.registry()))
                {
                    menu.profiles[index] = changed;
                    menu.notice = "Equipment and attachments cleared.".into();
                }
            }
            Action::Video | Action::Audio | Action::Controls | Action::Bindings => {
                menu.page = match action {
                    Action::Video => Page::Video,
                    Action::Audio => Page::Audio,
                    Action::Bindings => Page::Bindings,
                    _ => Page::Controls,
                };
                menu.focus = 0;
            }
            Action::Bind(index) => {
                bind.write(frame::UiBindRequest {
                    command: BINDS[index].1.into(),
                });
                menu.notice = format!(
                    "Press a key or mouse button for {}. ESC cancels.",
                    BINDS[index].0
                );
            }
            Action::Fullscreen => {
                settings.fullscreen = !settings.fullscreen;
                settings.touch();
            }
            Action::Resolution => {
                let sizes = [
                    frame::DisplayResolution::new(960, 540),
                    frame::DisplayResolution::HD,
                    frame::DisplayResolution::new(1920, 1080),
                    frame::DisplayResolution::new(2560, 1440),
                ];
                let index = sizes
                    .iter()
                    .position(|r| *r == settings.resolution)
                    .unwrap_or(0);
                settings.resolution = sizes
                    [(index + if direction < 0.0 { sizes.len() - 1 } else { 1 }) % sizes.len()];
                settings.touch();
            }
            Action::Brightness => {
                settings.brightness = (settings.brightness + direction * 0.02).clamp(-0.2, 0.2);
                settings.touch();
            }
            Action::Shadows => {
                settings.shadows = !settings.shadows;
                settings.touch();
            }
            Action::Bloom => {
                settings.bloom = !settings.bloom;
                settings.touch();
            }
            Action::DepthOfField => {
                settings.depth_of_field = !settings.depth_of_field;
                settings.touch();
            }
            Action::PadSensitivity => {
                settings.pad_sensitivity_preset = (i32::from(settings.pad_sensitivity_preset)
                    + direction as i32)
                    .clamp(1, 10) as u8;
                settings.touch();
            }
            Action::PadAds => {
                settings.pad_ads_sensitivity =
                    (settings.pad_ads_sensitivity + direction * 0.1).clamp(0.5, 1.5);
                settings.touch();
            }
            Action::PadInvert => {
                settings.pad_invert = !settings.pad_invert;
                settings.touch();
            }
            Action::PadVibration => {
                settings.pad_vibration = !settings.pad_vibration;
                settings.touch();
            }

            Action::Equip => {
                if menu.pending.is_some() {
                    continue;
                }
                let Some(slot) = menu.profiles.get(menu.slot) else {
                    continue;
                };
                let Some(registry) = weapons.as_ref().map(|weapons| weapons.registry()) else {
                    continue;
                };
                let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
                match session::loadout::resolve_personal_class(&row, registry) {
                    Ok(loadout) => {
                        let request_id = ids.allocate();
                        match actions.push(
                            local.0,
                            sim::ClientAction::SelectClass {
                                request_id,
                                class_id: sim::ClassId(menu.slot as u32),
                                revision: 1,
                                loadout,
                            },
                        ) {
                            Ok(()) => {
                                menu.pending = Some(request_id);
                                menu.notice = "Waiting for the host...".into();
                            }
                            Err(error) => menu.notice = error.to_string(),
                        }
                    }
                    Err(error) => menu.notice = error,
                }
            }
            Action::Volume => {
                settings.master_volume = (settings.master_volume + direction * 0.1).clamp(0.0, 1.0);
                settings.touch();
            }
            Action::Fov => {
                settings.fov = (settings.fov + direction * 5.0).clamp(65.0, 120.0);
                settings.touch();
            }
            Action::Sensitivity => {
                settings.sensitivity = (settings.sensitivity + direction * 0.5).clamp(0.1, 30.0);
                settings.touch();
            }
            Action::Invert => {
                settings.invert_mouse = !settings.invert_mouse;
                settings.touch();
            }
            Action::Vsync => {
                settings.vsync = !settings.vsync;
                settings.touch();
            }
            Action::Leave => {
                exec.write(UiExecCommand {
                    text: "disconnect".into(),
                });
                open.0 = false;
            }
            Action::EndMatch => {
                if menu.host {
                    exec.write(UiExecCommand {
                        text: "end_match".into(),
                    });
                    open.0 = false;
                }
            }
        }
    }
    if !menu.preserve_file
        && !menu.profiles.is_empty()
        && menu
            .save_retry
            .is_none_or(|time| std::time::Instant::now() >= time)
    {
        let contents = encode_slots(&menu.profiles);
        if menu.saved.as_deref() != Some(contents.as_str()) {
            if let Some(path) = &menu.path {
                match write_class_file(path, &contents) {
                    Ok(()) => {
                        menu.saved = Some(contents);
                        menu.save_retry = None;
                    }
                    Err(error) => {
                        menu.notice = format!("Cannot save loadout: {error}");
                        menu.save_retry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
                    }
                }
            }
        }
    }
}

fn fallback_label(key: &str) -> String {
    if key.is_empty() {
        "None".into()
    } else {
        key.split_once(':')
            .map_or(key, |(_, name)| name)
            .rsplit('/')
            .next()
            .unwrap_or(key)
            .trim_end_matches("_mp")
            .replace('_', " ")
    }
}

fn paint(
    mut commands: Commands,
    menu: Res<Menu>,
    open: Res<NativeGameMenu>,
    settings: Res<GameSettings>,
    catalog: Res<ClassLoadoutCatalog>,
    weapons: Option<Res<PreparedWeapons>>,
    strings: Option<Res<assets::PreparedLocalizedStrings>>,
    font: Res<GameUiFont>,
    roots: Query<Entity, With<Root>>,
    mut previous: Local<String>,
    mut art: ResMut<crate::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
    map: Option<Res<SessionMapIdentity>>,
    capture: Res<frame::UiBindingCapture>,
    generation: Res<frame::WorldGeneration>,
    dvars: Res<frame::UiMenuDvars>,
) {
    art.reset(*generation);
    let signature = format!(
        "{} {:?} {} {} {:?} {} {:?} {} {} {:?}",
        open.0,
        menu.page,
        menu.focus,
        menu.slot,
        menu.profiles,
        menu.notice,
        *settings,
        menu.host,
        menu.pick_page,
        (
            &capture.command,
            &menu.rename,
            *generation,
            catalog.revision,
            BINDS.map(|(_, command)| dvars
                .get(&format!("ui_bind_{command}"))
                .unwrap_or("UNBOUND"))
        )
    );
    if *previous == signature {
        return;
    }
    *previous = signature;
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !open.0 {
        return;
    }
    let label = |key: &str| {
        weapons
            .as_ref()
            .and_then(|weapons| {
                weapons
                    .registry()
                    .resolve_index(key)
                    .ok()
                    .flatten()
                    .and_then(|id| weapons.registry().display_name_key_of(id))
            })
            .and_then(|name| {
                strings
                    .as_ref()?
                    .0
                    .text_in(AssetNamespace::T6, name.trim_start_matches('@'))
            })
            .or_else(|| {
                catalog.previews.get(key).and_then(|preview| {
                    crate::t6_art::localized(&strings.as_ref()?.0, &preview.name_key)
                })
            })
            .map_or_else(|| fallback_label(key), str::to_owned)
    };
    let mut buttons = Vec::new();
    let title = match menu.page {
        Page::Pause => {
            buttons.push(("RESUME GAME".into(), Action::Resume));
            if !menu.zombies {
                buttons.push(("CREATE A CLASS".into(), Action::Classes));
            }
            buttons.push(("SETTINGS".into(), Action::Settings));
            if menu.host {
                buttons.push(("END MATCH / RETURN TO LOBBY".into(), Action::EndMatch));
            }
            buttons.push(("LEAVE MATCH".into(), Action::Leave));
            "PAUSE MENU"
        }
        Page::Classes => {
            for (index, slot) in menu.profiles.iter().enumerate() {
                buttons.push((
                    format!("{} / {}", slot.name, label(&slot.primary)),
                    Action::Edit(index),
                ));
            }
            buttons.push(("BACK".into(), Action::Back));
            "CREATE A CLASS"
        }
        Page::Edit => {
            if let Some(slot) = menu.profiles.get(menu.slot) {
                for row in [
                    ClassEditRow::Primary,
                    ClassEditRow::Secondary,
                    ClassEditRow::Lethal,
                    ClassEditRow::Tactical,
                ] {
                    buttons.push((
                        format!("{} / {}", row.label(), label(slot.row_value(row))),
                        Action::Weapon(row),
                    ));
                }
                buttons.push((
                    format!(
                        "PRIMARY ATTACHMENT / {}",
                        label(slot.primary_attachments.first().map_or("", String::as_str))
                    ),
                    Action::Attachment(ClassEditRow::Primary),
                ));
                buttons.push((
                    format!(
                        "SECONDARY ATTACHMENT / {}",
                        label(
                            slot.secondary_attachments
                                .first()
                                .map_or("", String::as_str)
                        )
                    ),
                    Action::Attachment(ClassEditRow::Secondary),
                ));
            }
            buttons.push(("RENAME CLASS".into(), Action::Rename));
            buttons.push(("COPY PREVIOUS CLASS".into(), Action::CopyPrevious));
            buttons.push(("CLEAR EQUIPMENT & ATTACHMENTS".into(), Action::ClearExtras));
            buttons.push(("USE ON NEXT RESPAWN".into(), Action::Equip));
            buttons.push(("BACK".into(), Action::Back));
            "EDIT LOADOUT"
        }
        Page::Settings => {
            buttons.extend([
                ("VIDEO".into(), Action::Video),
                ("AUDIO".into(), Action::Audio),
                ("MOUSE & CONTROLLER".into(), Action::Controls),
                ("KEY BINDINGS".into(), Action::Bindings),
                ("BACK".into(), Action::Back),
            ]);
            "SETTINGS"
        }
        Page::Video => {
            let on = |v| if v { "ON" } else { "OFF" };
            buttons.extend([
                (
                    format!("RESOLUTION / {}", settings.resolution),
                    Action::Resolution,
                ),
                (
                    format!("FULLSCREEN / {}", on(settings.fullscreen)),
                    Action::Fullscreen,
                ),
                (format!("VSYNC / {}", on(settings.vsync)), Action::Vsync),
                (format!("FIELD OF VIEW / {:.0}", settings.fov), Action::Fov),
                (
                    format!("BRIGHTNESS / {:+.2}", settings.brightness),
                    Action::Brightness,
                ),
                (
                    format!("SHADOWS / {}", on(settings.shadows)),
                    Action::Shadows,
                ),
                (format!("BLOOM / {}", on(settings.bloom)), Action::Bloom),
                (
                    format!("DEPTH OF FIELD / {}", on(settings.depth_of_field)),
                    Action::DepthOfField,
                ),
                ("BACK".into(), Action::Back),
            ]);
            "VIDEO"
        }
        Page::Audio => {
            buttons.extend([
                (
                    format!("MASTER VOLUME / {:.0}%", settings.master_volume * 100.0),
                    Action::Volume,
                ),
                ("BACK".into(), Action::Back),
            ]);
            "AUDIO"
        }
        Page::Controls => {
            let on = |v| if v { "ON" } else { "OFF" };
            buttons.extend([
                (
                    format!("MOUSE SENSITIVITY / {:.1}", settings.sensitivity),
                    Action::Sensitivity,
                ),
                (
                    format!("INVERT MOUSE / {}", on(settings.invert_mouse)),
                    Action::Invert,
                ),
                (
                    format!(
                        "CONTROLLER SENSITIVITY / {}",
                        settings.pad_sensitivity_preset
                    ),
                    Action::PadSensitivity,
                ),
                (
                    format!("CONTROLLER ADS SCALE / {:.1}", settings.pad_ads_sensitivity),
                    Action::PadAds,
                ),
                (
                    format!("INVERT CONTROLLER / {}", on(settings.pad_invert)),
                    Action::PadInvert,
                ),
                (
                    format!("CONTROLLER VIBRATION / {}", on(settings.pad_vibration)),
                    Action::PadVibration,
                ),
                ("KEY BINDINGS".into(), Action::Bindings),
                ("BACK".into(), Action::Back),
            ]);
            "CONTROLS"
        }
        Page::Rename => {
            buttons.extend([
                (format!("{}  |  SAVE NAME", menu.rename), Action::SaveName),
                ("CANCEL".into(), Action::Back),
            ]);
            "RENAME CLASS"
        }
        Page::Bindings => {
            for (index, (name, command)) in BINDS.iter().enumerate() {
                let chord = dvars
                    .get(&format!("ui_bind_{command}"))
                    .unwrap_or("UNBOUND");
                buttons.push((format!("{name} / {chord}"), Action::Bind(index)));
            }
            buttons.push(("BACK".into(), Action::Back));
            "KEY BINDINGS"
        }
        Page::Picker => {
            let start = menu.pick_page * PICK_PAGE;
            for (index, key) in menu.picker.iter().enumerate().skip(start).take(PICK_PAGE) {
                buttons.push((label(key), Action::Pick(index)));
            }
            if menu.pick_page > 0 {
                buttons.push(("PREVIOUS PAGE".into(), Action::Previous));
            }
            if start + PICK_PAGE < menu.picker.len() {
                buttons.push(("NEXT PAGE".into(), Action::Next));
            }
            buttons.push(("BACK".into(), Action::Back));
            if menu.pick_attachment {
                "SELECT ATTACHMENT"
            } else {
                match menu.pick_row {
                    Some(ClassEditRow::Primary) => "SELECT PRIMARY WEAPON",
                    Some(ClassEditRow::Secondary) => "SELECT SECONDARY WEAPON",
                    Some(ClassEditRow::Lethal) => "SELECT LETHAL EQUIPMENT",
                    _ => "SELECT TACTICAL EQUIPMENT",
                }
            }
        }
    };
    let focused = buttons.get(menu.focus).map(|(_, action)| *action);
    let slot = menu.profiles.get(match focused {
        Some(Action::Edit(index)) => index,
        _ => menu.slot,
    });
    let preview_key = match focused {
        Some(Action::Pick(index)) => menu.picker.get(index).map(|key| {
            if menu.pick_attachment && !key.is_empty() {
                slot.zip(menu.pick_row).map_or_else(
                    || key.clone(),
                    |(slot, row)| {
                        crate::classes::setup::attachment_preview_key(slot.row_value(row), key)
                    },
                )
            } else {
                key.clone()
            }
        }),
        Some(Action::Edit(index)) => menu.profiles.get(index).map(|s| s.primary.clone()),
        Some(Action::Weapon(row)) => slot.map(|s| s.row_value(row).to_owned()),
        Some(Action::Attachment(row)) => slot.and_then(|s| {
            let attachment = if row == ClassEditRow::Primary {
                s.primary_attachments.first()
            } else {
                s.secondary_attachments.first()
            }?;
            Some(crate::classes::setup::attachment_preview_key(
                s.row_value(row),
                attachment,
            ))
        }),
        _ => slot.map(|s| s.primary.clone()),
    };
    let preview = preview_key
        .as_ref()
        .and_then(|key| catalog.previews.get(key));
    let image = preview
        .filter(|_| matches!(menu.page, Page::Edit | Page::Classes | Page::Picker))
        .and_then(|p| art.image(&p.image, &mut images));
    let image_width = image
        .as_ref()
        .and_then(|h| images.get(h))
        .map_or(200.0, |image| {
            100.0 * image.texture_descriptor.size.width as f32
                / image.texture_descriptor.size.height.max(1) as f32
        });
    let description = preview
        .and_then(|p| crate::t6_art::localized(&strings.as_ref()?.0, &p.desc_key))
        .unwrap_or("");
    let heading = if matches!(
        menu.page,
        Page::Pause | Page::Settings | Page::Video | Page::Audio | Page::Controls | Page::Bindings
    ) {
        if menu.zombies {
            "ZOMBIES".into()
        } else {
            "MULTIPLAYER".into()
        }
    } else {
        slot.map_or_else(|| "CUSTOM CLASS".into(), |s| s.name.to_uppercase())
    };
    let detail = match menu.page {
        Page::Pause if menu.zombies => {
            "Survive together, buy weapons with points and hold USE near a downed teammate to revive. The match continues while this menu is open."
        }
        Page::Pause => {
            "Select a class for your next respawn, adjust your settings, or return to the lobby. The online match continues while this menu is open."
        }
        Page::Settings | Page::Video | Page::Audio | Page::Controls => {
            "Use LEFT / RIGHT to adjust the selected value. Changes apply immediately and save to your profile."
        }
        Page::Rename => {
            "Type a class name (up to 24 characters). BACKSPACE removes a character. ENTER saves and ESC cancels."
        }
        Page::Bindings => {
            "Select an action, then press a key, mouse button or controller button. ESC cancels capture."
        }
        _ => {
            "Choose a prepared weapon and equipment from your BO2 installation. Select USE ON NEXT RESPAWN to send this class to the host."
        }
    };
    let accent = Color::srgb(1.0, 0.48, 0.12);
    let row_back = art.image("menu_button_backing", &mut images);
    let row_selected = art.image("menu_button_backing_highlight", &mut images);
    commands
        .spawn((
            Root,
            super::t6_text::NativeUiRoot(true),
            UiLayer::Overlay,
            UiLayerVisibility,
            GlobalZIndex(100),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::axes(Val::Percent(5.0), Val::Percent(5.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.74)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(title),
                game_text_font(&font.0, 36.0),
                TextColor(Color::WHITE),
            ));
            root.spawn(Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Percent(4.0),
                ..default()
            })
            .with_children(|body| {
                body.spawn(Node {
                    width: Val::Percent(55.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|panel| {
                    for (order, (label, action)) in buttons.into_iter().enumerate() {
                        let backing = if order == menu.focus { row_selected.as_ref().or(row_back.as_ref()) } else { row_back.as_ref() };
                        panel
                            .spawn((
                                Button,
                                Choice { order, action },
                                ImageNode { image: backing.cloned().unwrap_or_default(), color: if backing.is_none() { Color::NONE } else if order == menu.focus { Color::srgba(0.8, 0.3, 0.04, 0.8) } else { Color::srgba(0.08, 0.08, 0.08, 0.7) }, image_mode: bevy::ui::widget::NodeImageMode::Stretch, ..default() },
                                Node {
                                    min_height: Val::Px(28.0),
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                    border: UiRect::left(Val::Px(3.0)),
                                    ..default()
                                },
                                BorderColor::all(if order == menu.focus {
                                    accent
                                } else {
                                    Color::NONE
                                }),
                                BackgroundColor(if order == menu.focus {
                                    Color::srgba(0.45, 0.18, 0.04, 0.65)
                                } else {
                                    Color::srgba(0.065, 0.075, 0.085, 0.9)
                                }),
                            ))
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(label),
                                    game_text_font(&font.0, 22.0),
                                    TextColor(if order == menu.focus { accent } else { Color::srgb(0.94, 0.94, 0.92) }),
                                ));
                            });
                    }
                });
                body.spawn((
                    Node {
                        width: Val::Percent(41.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(18.0)),
                        row_gap: Val::Px(14.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.04, 0.05, 0.06, 0.85)),
                ))
                .with_children(|detail_panel| {
                    detail_panel.spawn((
                        Text::new(heading),
                        game_text_font(&font.0, 22.0),
                        TextColor(accent),
                    ));
                    if let Some(image) = image {
                        detail_panel.spawn((
                            ImageNode::new(image),
                            Node {
                                width: Val::Px(image_width),
                                max_width: Val::Percent(100.0),
                                height: Val::Px(100.0),
                                ..default()
                            },
                        ));
                    }
                    if let Some(key) = preview_key
                        .as_ref()
                        .filter(|_| matches!(menu.page, Page::Edit | Page::Classes | Page::Picker))
                    {
                        detail_panel.spawn((
                            Text::new(label(key)),
                            game_text_font(&font.0, 20.0),
                            TextColor(Color::WHITE),
                        ));
                        if !description.is_empty() {
                            detail_panel.spawn((
                                Text::new(description),
                                game_text_font(&font.0, 14.0),
                                TextColor(Color::srgb(0.75, 0.78, 0.8)),
                            ));
                        }
                    }
                    detail_panel.spawn((
                        Text::new(detail),
                        game_text_font(&font.0, 14.0),
                        TextColor(Color::srgb(0.75, 0.78, 0.8)),
                    ));
                    if menu.page == Page::Picker {
                        detail_panel.spawn((
                            Text::new(format!(
                                "PAGE {} / {}",
                                menu.pick_page + 1,
                                menu.picker.len().div_ceil(PICK_PAGE).max(1)
                            )),
                            game_text_font(&font.0, 14.0),
                            TextColor(accent),
                        ));
                    }
                    if let Some(map) = &map {
                        detail_panel.spawn((
                            Text::new(map.zone.to_uppercase().replace('_', " ")),
                            game_text_font(&font.0, 14.0),
                            TextColor(Color::srgb(0.5, 0.55, 0.6)),
                        ));
                    }
                });
            });
            root.spawn((
                Text::new(if let Some(command) = &capture.command {
                    format!("WAITING FOR INPUT: {command}   |   ESC CANCEL")
                } else if menu.preserve_file {
                    "Saved BO2 classes are unreadable. Original file preserved; edits are session-only.".into()
                } else if !menu.notice.is_empty() {
                    menu.notice.clone()
                } else {
                    "ARROWS  NAVIGATE / ADJUST     ENTER  SELECT     ESC  BACK".into()
                }),
                game_text_font(&font.0, 14.0),
                TextColor(accent),
            ));
        });
}
