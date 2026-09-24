use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use frame::ClientSet;

use crate::GameUiFont;
use crate::class_setup::{ClassLoadoutCatalog, ClassSetupScratch};
use crate::class_store::{
    ClassStoreFile, SessionClassStore, load_class_store, save_class_store, sync_host_class_loadouts,
};
use crate::nav::{
    ActivatePulse, Focus, Hover, MenuShellCmd, PointerActivation, drive_control_axes,
    paint_focus_help, paint_selection_bars, play_focus_sound, sync_hover_and_nav,
};
use crate::options::{
    BindingView, OptionsState, drive_options_navigation, sync_display_resolutions,
    sync_options_entry,
};
use crate::retail_menu::{
    RetailMenuStack, ensure_main_open, handle_menu_back, handle_retail_clicks, spawn_retail_shell,
    window_size,
};

use assets::{LocalizeCatalog, MenuCatalog};
use frame::UiPlayMusic;

#[derive(Resource, Clone, Debug, Default)]
pub struct MenuMapList(pub Vec<String>);

#[derive(Resource, Clone, Debug, Default)]
pub struct MenuBackground(pub Option<Handle<Image>>);

#[derive(Resource)]
pub struct PendingMenuBgPixels(pub (u32, u32, Vec<u8>));

#[derive(Resource, Debug, Default)]
pub struct PendingMenuMap(pub Option<String>);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GamePrivacy {
    #[default]
    Private,
    Public,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameLobbyRole {
    #[default]
    Host,
    Member,
}

#[derive(Resource, Debug)]
pub struct GameSetupDraft {
    pub privacy: GamePrivacy,
    pub role: GameLobbyRole,
    pub selected_map: Option<String>,
    pub previewed_map: Option<String>,
    pub map_page: usize,
    pub selected_mode: sim::HostGameModeSelection,
}

impl Default for GameSetupDraft {
    fn default() -> Self {
        Self {
            privacy: GamePrivacy::Private,
            role: GameLobbyRole::Host,
            selected_map: None,
            previewed_map: None,
            map_page: 0,
            selected_mode: sim::HostGameModeSelection::from_env(),
        }
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuEnabled(pub bool);

impl Default for MenuEnabled {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub(crate) struct MenuRoot;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellRevision(pub u64);

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuMapList>()
            .init_resource::<MenuBackground>()
            .init_resource::<PendingMenuMap>()
            .init_resource::<GameSetupDraft>()
            .init_resource::<MenuEnabled>()
            .init_resource::<ClassSetupScratch>()
            .init_resource::<ClassLoadoutCatalog>()
            .init_resource::<frame::GameSettings>()
            .init_resource::<OptionsState>()
            .init_resource::<BindingView>()
            .init_resource::<ShellRevision>()
            .init_resource::<SessionClassStore>()
            .init_resource::<ClassStoreFile>()
            .init_resource::<frame::HostClassLoadouts>()
            .init_resource::<MenuCatalog>()
            .init_resource::<LocalizeCatalog>()
            .init_resource::<RetailMenuStack>()
            .init_resource::<crate::retail_menu::MenuFrontend>()
            .init_resource::<crate::retail_menu::MenuImageCache>()
            .init_resource::<Focus>()
            .init_resource::<Hover>()
            .init_resource::<ActivatePulse>()
            .init_resource::<PointerActivation>()
            .add_message::<MenuShellCmd>()
            .add_message::<crate::UiIntent>()
            .add_systems(Startup, upload_menu_background)
            .add_systems(
                Update,
                (
                    (
                        sync_class_select_shell,
                        tear_down_menu_when_disabled,
                        ensure_main_open,
                        follow_public_join.run_if(resource_exists::<net::PendingMasterMenuAction>),
                        drive_game_map_pages,
                        sync_display_resolutions,
                        sync_options_entry,
                        sync_class_entry,
                        bump_shell_revision,
                        rebuild_menu_ui,
                        crate::render::animate_retail_widgets,
                        crate::nav::sync_pointer_input,
                        sync_hover_and_nav,
                        drive_options_navigation,
                        drive_control_axes,
                        sync_game_map_preview_from_focus,
                    )
                        .chain(),
                    (
                        paint_cac_preview,
                        paint_selection_bars,
                        paint_focus_help,
                        play_focus_sound,
                        handle_menu_back,
                        handle_retail_clicks,
                        drive_ingame_class,
                        crate::options::edit_player_name,
                        crate::class_setup::edit_class_name,
                        crate::class_setup::drive_cac_pages,
                        crate::options::apply_option_intents,
                        crate::class_setup::apply_cac_intents,
                        crate::options::apply_window_settings,
                        load_class_store,
                        sync_host_class_loadouts,
                        save_class_store,
                    )
                        .chain(),
                )
                    .chain()
                    .in_set(ClientSet::Ui),
            );
    }
}

fn sync_class_entry(
    stack: Res<RetailMenuStack>,
    mut scratch: ResMut<ClassSetupScratch>,
    mut focus: ResMut<Focus>,
    mut previous_top: Local<Option<String>>,
) {
    let top = stack.names.last().cloned();
    if top.as_deref() == Some("class_setup") && previous_top.as_deref() != Some("class_setup") {
        scratch.reset_navigation();
        focus.widget = if scratch.slots.is_empty() {
            Some("class_setup/back".into())
        } else {
            Some(format!("class_setup/slot/{}", scratch.selected))
        };
    }
    *previous_top = top;
}

fn map_picker_screen(stack: &RetailMenuStack) -> Option<&str> {
    match stack.names.last().map(String::as_str) {
        Some(name @ ("map_setup" | "game_map_select")) => Some(name),
        _ => None,
    }
}

fn sync_game_map_preview_from_focus(
    focus: Res<Focus>,
    stack: Res<RetailMenuStack>,
    maps: Res<MenuMapList>,
    mut setup: ResMut<GameSetupDraft>,
) {
    let Some(screen) = map_picker_screen(&stack) else {
        return;
    };
    let Some(map) = focus
        .widget
        .as_deref()
        .and_then(|id| id.strip_prefix(screen)?.strip_prefix('/'))
        .filter(|map| maps.0.iter().any(|candidate| candidate == map))
    else {
        return;
    };
    if setup.previewed_map.as_deref() != Some(map) {
        setup.previewed_map = Some(map.to_owned());
    }
}

fn drive_game_map_pages(
    stack: Res<RetailMenuStack>,
    maps: Res<MenuMapList>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cmds: MessageReader<MenuShellCmd>,
    mut setup: ResMut<GameSetupDraft>,
) {
    if map_picker_screen(&stack).is_none() {
        return;
    }
    let mut step = 0isize;
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        step -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        step += 1;
    }
    for cmd in cmds.read() {
        match cmd {
            MenuShellCmd::Nav(crate::NavDir::Left) => step -= 1,
            MenuShellCmd::Nav(crate::NavDir::Right) => step += 1,
            _ => {}
        }
    }
    if step == 0 {
        return;
    }
    let page_count = crate::screens::game_map_page_count(&maps.0);
    if page_count > 0 {
        setup.map_page = (setup.map_page as isize + step).rem_euclid(page_count as isize) as usize;
    }
}

fn follow_public_join(
    mut stack: ResMut<RetailMenuStack>,
    bridge: Option<Res<net::MasterBridge>>,
    mut action: ResMut<net::PendingMasterMenuAction>,
    mut setup: ResMut<GameSetupDraft>,
    mut saw_browser: Local<bool>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    let state = bridge.state();
    if stack.names.last().map(String::as_str) == Some("find_lobbies") {
        *saw_browser = true;
        if let net::MasterBridgeState::Joined { map, mode, .. } = state {
            setup.privacy = GamePrivacy::Public;
            setup.role = GameLobbyRole::Member;
            setup.previewed_map = Some(map.clone());
            setup.selected_map = Some(map);
            if let Some(selection) = sim::HostGameModeSelection::from_token(&mode) {
                setup.selected_mode = selection;
            }
            stack.names.push("game_lobby".into());
        }
        return;
    }
    let in_flow = stack
        .names
        .iter()
        .any(|name| name == "find_lobbies" || name == "game_lobby");
    if in_flow {
        *saw_browser = true;
    }
    let joining = matches!(
        state,
        net::MasterBridgeState::Connecting { .. }
            | net::MasterBridgeState::Joining { .. }
            | net::MasterBridgeState::Failed { .. }
            | net::MasterBridgeState::Closed { .. }
            | net::MasterBridgeState::Left { .. }
    );
    if drop_abandoned_browser_join(*saw_browser, in_flow, joining) && action.0.is_none() {
        action.0 = Some(net::MasterMenuAction::LeaveLobby);
        *saw_browser = false;
    }
}

fn drop_abandoned_browser_join(saw_browser: bool, in_flow: bool, joining: bool) -> bool {
    saw_browser && !in_flow && joining
}

fn upload_menu_background(
    mut commands: Commands,
    pending: Option<Res<PendingMenuBgPixels>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(pending) = pending else {
        return;
    };
    let (width, height, pixels) = pending.0.clone();
    let size = bevy::render::render_resource::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new(
        size,
        bevy::render::render_resource::TextureDimension::D2,
        pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::linear();
    let handle = images.add(image);
    commands.insert_resource(MenuBackground(Some(handle)));
    commands.remove_resource::<PendingMenuBgPixels>();
}

fn sync_class_select_shell(
    overlay: Res<crate::ClassSelectOverlayOpen>,
    highlight: Res<crate::ClassSelectHighlight>,
    mut enabled: ResMut<MenuEnabled>,
    mut stack: ResMut<RetailMenuStack>,
    mut focus: ResMut<Focus>,
    mut owned: Local<bool>,
) {
    if overlay.0 && !*owned {
        stack.names.clear();
        stack.names.push("ingame_class".into());
        focus.widget = Some(format!("class_setup/slot/{}", highlight.0));
        enabled.0 = true;
        *owned = true;
    } else if !overlay.0 && *owned {
        stack.names.clear();
        enabled.0 = false;
        *owned = false;
    }
}

fn tear_down_menu_when_disabled(
    enabled: Res<MenuEnabled>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
) {
    if enabled.0 || !enabled.is_changed() {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct ShellExtras<'w> {
    scratch: Res<'w, ClassSetupScratch>,
    catalog: Res<'w, ClassLoadoutCatalog>,
    maps: Res<'w, MenuMapList>,
    bg: Res<'w, MenuBackground>,
    game_setup: Res<'w, GameSetupDraft>,
    browser: Option<Res<'w, net::MasterBrowser>>,
    master_intent: Option<Res<'w, net::MasterLaunchIntent>>,
    master_bridge: Option<Res<'w, net::MasterBridge>>,
    settings: Res<'w, frame::GameSettings>,
    options: Res<'w, OptionsState>,
    bindings: Res<'w, BindingView>,
    class_phase: Res<'w, crate::ClassSelectPhase>,
    class_status: Res<'w, crate::ClassSelectStatus>,
    class_store: Res<'w, SessionClassStore>,
    class_icons: Res<'w, crate::ClassSelectIconCache>,
}

fn window_layout_key(windows: &Query<&Window, With<PrimaryWindow>>) -> Option<(u32, u32, u32)> {
    windows.single().ok().map(|window| {
        (
            window.resolution.physical_width(),
            window.resolution.physical_height(),
            window.resolution.scale_factor().to_bits(),
        )
    })
}

fn bump_shell_revision(
    extras: ShellExtras,
    stack: Res<RetailMenuStack>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut revision: ResMut<ShellRevision>,
    mut last_window: Local<Option<Option<(u32, u32, u32)>>>,
    mut last_stack: Local<Vec<String>>,
    mut last_bridge: Local<Option<net::MasterBridgeState>>,
    mut last_browser: Local<Option<net::MasterBrowserSnapshot>>,
) {
    let window_key = window_layout_key(&windows);
    let window_changed = last_window.as_ref() != Some(&window_key);
    *last_window = Some(window_key);
    let stack_changed = *last_stack != stack.names;
    if stack_changed {
        last_stack.clone_from(&stack.names);
    }
    let bridge = extras.master_bridge.as_ref().map(|value| value.state());
    let browser = extras.browser.as_ref().map(|value| value.snapshot());
    let external_changed = *last_bridge != bridge
        || *last_browser != browser
        || extras
            .master_intent
            .as_ref()
            .is_some_and(|value| value.is_changed());
    *last_bridge = bridge;
    *last_browser = browser;
    if extras.class_phase.is_changed()
        || extras.class_status.is_changed()
        || extras.class_store.is_changed()
        || extras.class_icons.is_changed()
        || extras.scratch.is_changed()
        || extras.catalog.is_changed()
        || extras.maps.is_changed()
        || extras.bg.is_changed()
        || extras.game_setup.is_changed()
        || stack_changed
        || extras.settings.is_changed()
        || extras.options.is_changed()
        || extras.bindings.is_changed()
        || window_changed
        || external_changed
    {
        revision.0 = revision.0.wrapping_add(1);
    }
}

fn rebuild_menu_ui(
    mut commands: Commands,
    enabled: Res<MenuEnabled>,
    extras: ShellExtras,
    font: Option<Res<GameUiFont>>,
    menus: Res<MenuCatalog>,
    loc: Res<LocalizeCatalog>,
    mut stack: ResMut<RetailMenuStack>,
    mut play_music: MessageWriter<UiPlayMusic>,
    mut paint: crate::retail_menu::RetailPaintCtx,
    revision: Res<ShellRevision>,
    roots: Query<Entity, With<MenuRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last: Local<Option<u64>>,
) {
    if !enabled.0 {
        *last = None;
        return;
    }
    let (win_w, win_h) = window_size(&windows);
    let browser = extras
        .browser
        .as_ref()
        .map(|browser| browser.snapshot())
        .unwrap_or_default();
    let browser_enabled = extras
        .master_intent
        .as_ref()
        .is_some_and(|intent| intent.browser_available());
    let bridge = extras.master_bridge.as_ref().map(|bridge| bridge.state());
    if last.as_ref() == Some(&revision.0) {
        return;
    }
    *last = Some(revision.0);
    let Some(font) = font else {
        diag::warn!(Ui, "menu: GameUiFont missing; shell UI skipped");
        return;
    };
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    diag::info!(
        Ui,
        "menu: spawn retail stack (maps={}, bg={})",
        extras.maps.0.len(),
        extras.bg.0.is_some()
    );
    spawn_retail_shell(
        &mut commands,
        &menus,
        &loc,
        &mut stack,
        &font.0,
        extras.bg.0.clone(),
        &mut paint,
        win_w,
        win_h,
        &mut play_music,
        &extras.maps.0,
        Some(&extras.scratch),
        Some(&extras.catalog),
        Some(&extras.game_setup),
        Some(&extras.settings),
        Some(&extras.options),
        Some(&extras.bindings),
        browser_enabled,
        Some(&browser),
        bridge.as_ref(),
    );
}

fn paint_cac_preview(
    focus: Res<Focus>,
    stack: Res<RetailMenuStack>,
    scratch: Res<ClassSetupScratch>,
    catalog: Res<ClassLoadoutCatalog>,
    mut painted: Query<(&crate::render::PaintedWidget, &mut Visibility)>,
) {
    use crate::screens::CacRevealGroup;
    if !matches!(
        stack.names.last().map(String::as_str),
        Some("class_setup" | "ingame_class")
    ) {
        return;
    }
    let focused = focus.widget.as_deref();
    let slot = focused
        .and_then(crate::screens::class_slot_index_from_id)
        .unwrap_or(scratch.selected);
    let pick = focused
        .and_then(crate::screens::class_pick_index_from_id)
        .unwrap_or_else(|| crate::screens::class_pick_default_index(&scratch, &catalog));
    for (widget, mut visibility) in &mut painted {
        let Some((group, index)) = crate::screens::cac_reveal_target(&widget.id) else {
            continue;
        };
        let wanted = match group {
            CacRevealGroup::Slot => index == slot,
            CacRevealGroup::Pick => index == pick,
            CacRevealGroup::Attachment => {
                index
                    == focused
                        .and_then(|id| id.strip_prefix("class_setup/attachment/"))
                        .and_then(|index| index.parse::<usize>().ok())
                        .map_or(0, |index| index + 1)
            }
        };
        let wanted = if wanted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_ingame_class(
    mut intents: MessageReader<crate::UiIntent>,
    mut store: ResMut<SessionClassStore>,
    mut highlight: ResMut<crate::ClassSelectHighlight>,
    mut phase: ResMut<crate::ClassSelectPhase>,
    mut pending: ResMut<crate::PendingClassEquip>,
    mut status: ResMut<crate::ClassSelectStatus>,
    mut seq: ResMut<net::ActionRequestIds>,
    allowed: Res<crate::ClassChangeAllowed>,
    mut stack: ResMut<RetailMenuStack>,
    mut enabled: ResMut<MenuEnabled>,
    mut waiting: Local<bool>,
) {
    if *waiting && !phase.is_pending() {
        *waiting = false;
        if status.0.is_none()
            && let Some(index) = stack.names.iter().position(|name| name == "ingame_options")
        {
            stack.names.truncate(index);
            enabled.0 = false;
        }
    }
    for intent in intents.read() {
        if let crate::UiIntent::SelectClass(index) = intent
            && allowed.0
            && stack.names.last().map(String::as_str) == Some("ingame_class")
        {
            if let Err(error) = crate::commit_class_equip(
                *index as usize,
                &mut store,
                &mut highlight,
                &mut phase,
                &mut pending,
                &mut status,
                &mut seq,
            ) {
                diag::warn!(Ui, "choose class: {error}");
            } else {
                *waiting = true;
            }
        }
    }
}
