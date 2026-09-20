use std::collections::HashMap;

use crate::class_setup::{ClassLoadoutCatalog, ClassSetupScratch, apply_cac_intent};
use crate::menu::{
    GameLobbyRole, GamePrivacy, GameSetupDraft, MenuEnabled, MenuMapList, MenuRoot, PendingMenuMap,
};
use crate::model::{ScreenCmd, UiIntent};
use crate::render::{PaintedWidget, WidgetBehavior, spawn_screen, warm_screen_images};
use crate::screens::{self, Host};
use crate::stack::{self, BackAction};
use crate::{UiLayer, UiLayerVisibility};
use assets::{LocalizeCatalog, MenuCatalog};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use frame::{UiPlayMusic, UiPlaySound, UiStopMusic};

#[derive(Resource, Debug, Default)]
pub struct RetailMenuStack {
    pub names: Vec<String>,
    music_started: bool,
    font_gap_said: bool,
    pub(crate) hover_sound_gap_said: bool,
    visible_exp_gap_said: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MenuFrontend {
    pub game_mode: Option<String>,
}

#[derive(Resource, Debug, Default)]
pub struct MenuImageCache {
    pub handles: HashMap<String, Handle<Image>>,
    pub(crate) sizes: HashMap<String, (u32, u32)>,
    pub(crate) missing: Vec<String>,
}

#[derive(SystemParam)]
pub(crate) struct RetailPaintCtx<'w> {
    games: Option<Res<'w, crate::UiAssetRoot>>,
    images: ResMut<'w, Assets<Image>>,
    cache: ResMut<'w, MenuImageCache>,
    frontend: Res<'w, MenuFrontend>,
    app_screen: Res<'w, frame::AppScreen>,
    identity: Option<Res<'w, frame::LaunchIdentity>>,
    compass: Option<Res<'w, assets::SessionCompass>>,
    presented: Option<Res<'w, net::PresentedSnapshot>>,
    local: Option<Res<'w, net::LocalPresentClient>>,
    team_icons: Option<Res<'w, assets::SessionTeamIcons>>,
    class_store: Res<'w, crate::SessionClassStore>,
    class_phase: Res<'w, crate::ClassSelectPhase>,
    class_status: Res<'w, crate::ClassSelectStatus>,
    class_icons: Res<'w, crate::ClassSelectIconCache>,
    strings: Option<Res<'w, assets::PreparedLocalizedStrings>>,
}

#[derive(SystemParam)]
pub(crate) struct RetailActionWriters<'w> {
    play: MessageWriter<'w, UiPlaySound>,
    play_music: MessageWriter<'w, UiPlayMusic>,
    exit: MessageWriter<'w, AppExit>,
    intents: MessageWriter<'w, UiIntent>,
}

pub(crate) fn spawn_retail_shell(
    commands: &mut Commands,
    catalog: &MenuCatalog,
    loc: &LocalizeCatalog,
    stack: &mut RetailMenuStack,
    font: &Handle<Font>,
    launcher_bg: Option<Handle<Image>>,
    paint: &mut RetailPaintCtx,
    win_w: f32,
    win_h: f32,
    play_music: &mut MessageWriter<UiPlayMusic>,
    maps: &[String],
    classes: Option<&ClassSetupScratch>,
    loadout: Option<&ClassLoadoutCatalog>,
    game_setup: Option<&GameSetupDraft>,
    settings: Option<&frame::GameSettings>,
    options: Option<&crate::OptionsState>,
    bindings: Option<&crate::BindingView>,
    browser_enabled: bool,
    browser: Option<&net::MasterBrowserSnapshot>,
    bridge: Option<&net::MasterBridgeState>,
) {
    if stack.names.is_empty() {
        open_named(catalog, stack, "main", play_music, maps);
    }
    let in_game = matches!(
        *paint.app_screen,
        frame::AppScreen::InGame | frame::AppScreen::ClassSelect
    );
    let loc = if in_game {
        paint.strings.as_deref().map_or(loc, |strings| &strings.0)
    } else {
        loc
    };
    let mut match_info = screens::InGameMenuInfo::default();
    if in_game {
        if let Some(identity) = paint.identity.as_deref() {
            let key = format!(
                "MPUI_{}",
                identity.zone.trim_start_matches("mp_").to_uppercase()
            );
            match_info.map = loc.text(&key).unwrap_or(&identity.zone).to_owned();
            let icons = paint
                .team_icons
                .as_deref()
                .map(|icons| icons.0.clone())
                .filter(|icons| icons.axis.is_some() || icons.allies.is_some())
                .or_else(|| {
                    catalog.string_table("mp/factionTable.csv").map(|table| {
                        assets::team_icons_for_zone(
                            table,
                            catalog.rawfile_text("mp/basemaps.arena"),
                            &identity.zone,
                        )
                    })
                });
            if let (Some(icons), Some(snapshot), Some(local)) = (
                icons,
                paint.presented.as_deref().and_then(|p| p.snapshot()),
                paint.local.as_deref(),
            ) {
                match_info.icon = snapshot.meta.for_client(local.0).and_then(|meta| {
                    match meta.client_state_team {
                        1 => icons.axis,
                        2 => icons.allies,
                        _ => None,
                    }
                });
            }
        }
        match_info.compass = paint
            .compass
            .as_deref()
            .and_then(|c| c.declaration.image.clone());
        if let Some(snapshot) = paint.presented.as_deref().and_then(|p| p.snapshot()) {
            match_info.mode = snapshot.meta.kind.display_name().to_owned();
            let objective = match snapshot.meta.kind.token() {
                "dm" => Some("OBJECTIVES_DM"),
                "dom" => Some("OBJECTIVES_DOM"),
                _ => None,
            };
            if let Some(key) = objective {
                let key = if snapshot.meta.score_limit > 0 {
                    format!("{key}_SCORE")
                } else {
                    key.to_owned()
                };
                match_info.description = loc
                    .text(&key)
                    .map(|text| text.replace("&&1", &snapshot.meta.score_limit.to_string()));
            }
        }
    }
    let host = Host {
        in_game,
        match_info: in_game.then_some(&match_info),
        class_store: Some(&paint.class_store),
        class_pending: paint.class_phase.is_pending(),
        class_status: paint.class_status.0.as_deref(),
        initial_class_select: *paint.app_screen == frame::AppScreen::ClassSelect,
        maps,
        menus: Some(catalog),
        loc: Some(loc),
        classes,
        loadout,
        game_setup,
        settings,
        options,
        bindings,
        browser_enabled,
        browser,
        bridge,
    };
    let screens = screens::resolve_stack(catalog, &stack.names, host);
    if !stack.visible_exp_gap_said {
        if paint.frontend.game_mode.is_none() {
            diag::warn!(
                Ui,
                "menu: visibleExp not evaluated — painting Bound gameMode widgets in order (sp/co/mp share one rect)"
            );
        }
        stack.visible_exp_gap_said = true;
    }
    for (stem, handle) in &paint.class_icons.images {
        if let Some(image) = paint.images.get(handle) {
            paint
                .cache
                .sizes
                .insert(stem.clone(), (image.width(), image.height()));
            paint.cache.handles.insert(stem.clone(), handle.clone());
        }
    }
    let games = paint.games.as_ref().and_then(|g| g.0.as_deref());
    for screen in &screens {
        warm_screen_images(catalog, screen, games, &mut paint.images, &mut paint.cache);
    }
    let has_retail_art = paint.cache.handles.contains_key("mw2_main_background")
        || paint.cache.handles.contains_key("mw2_main_mp_image");
    let underlay = if has_retail_art || in_game {
        None
    } else {
        launcher_bg
    };

    let camera = spawn_menu_camera(commands, in_game);
    let mut root = commands.spawn((
        MenuRoot,
        UiLayer::Shell,
        UiLayerVisibility,
        Visibility::Inherited,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        GlobalZIndex(10_000),
        UiTargetCamera(camera),
    ));
    if let Some(handle) = underlay {
        root.insert(ImageNode::new(handle).with_mode(bevy::ui::widget::NodeImageMode::Stretch));
    }
    let mut used_oxanium = false;
    let live = stack::UiStack::from_screens(&screens);
    let top_capture = live.top_capture_index();
    let paint_start = live.paint_start_index();
    if paint_start > 0 {
        root.remove::<ImageNode>();
    }
    root.with_children(|parent| {
        for (i, screen) in screens.iter().enumerate() {
            if i < paint_start {
                continue;
            }
            if spawn_screen(
                parent,
                loc,
                catalog,
                &paint.cache,
                &paint.frontend,
                screen,
                font,
                win_w,
                win_h,
                top_capture == Some(i),
            ) {
                used_oxanium = true;
            }
        }
    });
    if used_oxanium && !stack.font_gap_said {
        diag::warn!(
            Ui,
            "menu: Font_s atlas missing — painting those labels with Oxanium (typed gap)"
        );
        stack.font_gap_said = true;
    }
}

fn spawn_menu_camera(commands: &mut Commands, in_game: bool) -> Entity {
    let mut camera = commands.spawn((
        Camera2d,
        Camera {
            order: 100,
            clear_color: if in_game {
                ClearColorConfig::None
            } else {
                ClearColorConfig::Custom(Color::srgb(0.04, 0.045, 0.06))
            },
            ..default()
        },
        MenuRoot,
    ));
    if !in_game {
        camera.insert(IsDefaultUiCamera);
    }
    camera.id()
}

pub(crate) fn ensure_main_open(
    enabled: Res<MenuEnabled>,
    catalog: Option<Res<MenuCatalog>>,
    maps: Option<Res<MenuMapList>>,
    mut stack: ResMut<RetailMenuStack>,
    mut play_music: MessageWriter<UiPlayMusic>,
) {
    if !enabled.0 {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let maps = maps.as_ref().map(|m| m.0.as_slice()).unwrap_or(&[]);
    if stack.names.is_empty() {
        open_named(&catalog, &mut stack, "main", &mut play_music, maps);
    }
}

pub(crate) fn sync_frontend_music(
    screen: Res<frame::AppScreen>,
    enabled: Res<MenuEnabled>,
    catalog: Option<Res<MenuCatalog>>,
    maps: Option<Res<MenuMapList>>,
    mut stack: ResMut<RetailMenuStack>,
    mut play_music: MessageWriter<UiPlayMusic>,
    mut stop_music: MessageWriter<UiStopMusic>,
) {
    if !enabled.0
        || matches!(
            *screen,
            frame::AppScreen::InGame | frame::AppScreen::ClassSelect
        )
    {
        if stack.music_started {
            stop_music.write(UiStopMusic);
            stack.music_started = false;
        }
        return;
    }
    if stack.music_started {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let maps = maps.as_ref().map(|m| m.0.as_slice()).unwrap_or(&[]);
    let Some(alias) = screens::resolve_open(&catalog, "main", Host::with_maps(maps))
        .and_then(|screen| screen.bed)
    else {
        return;
    };
    play_music.write(UiPlayMusic { alias });
    stack.music_started = true;
}

fn open_named(
    catalog: &MenuCatalog,
    stack: &mut RetailMenuStack,
    name: &str,
    play_music: &mut MessageWriter<UiPlayMusic>,
    maps: &[String],
) -> bool {
    let Some(screen) = screens::resolve_open(catalog, name, Host::with_maps(maps)) else {
        diag::warn!(Ui, "menu: open `{name}` — not in catalog (typed gap)");
        return false;
    };
    if stack.names.last().map(String::as_str) == Some(screen.id.as_str()) {
        return false;
    }
    stack.names.push(screen.id.clone());
    if !stack.music_started {
        if let Some(alias) = &screen.bed {
            play_music.write(UiPlayMusic {
                alias: alias.clone(),
            });
            stack.music_started = true;
        }
    }
    run_screen_cmds(
        catalog,
        stack,
        &screen.on_open,
        None,
        play_music,
        maps,
        None,
        None,
        None,
        None,
        false,
    )
}

fn close_top(stack: &mut RetailMenuStack) {
    if stack.names.len() > 1 {
        stack.names.pop();
    }
}

fn apply_back(
    catalog: &MenuCatalog,
    stack: &mut RetailMenuStack,
    play_music: &mut MessageWriter<UiPlayMusic>,
    maps: &[String],
) -> bool {
    let top = stack.names.last().cloned();
    let on_back = top
        .as_deref()
        .and_then(|name| screens::resolve_open(catalog, name, Host::with_maps(maps)))
        .map(|s| s.on_back)
        .unwrap_or_default();
    match stack::back_action(&stack.names, &on_back) {
        BackAction::Pop => {
            close_top(stack);
            false
        }
        BackAction::RunOnBack => run_screen_cmds(
            catalog, stack, &on_back, None, play_music, maps, None, None, None, None, false,
        ),
        BackAction::OpenQuitConfirm => open_named(catalog, stack, "quit_confirm", play_music, maps),
    }
}

fn run_screen_cmds(
    catalog: &MenuCatalog,
    stack: &mut RetailMenuStack,
    cmds: &[ScreenCmd],
    play: Option<&mut MessageWriter<UiPlaySound>>,
    play_music: &mut MessageWriter<UiPlayMusic>,
    maps: &[String],
    mut pending: Option<&mut PendingMenuMap>,
    mut game_setup: Option<&mut GameSetupDraft>,
    mut cac: Option<(&mut ClassSetupScratch, &ClassLoadoutCatalog)>,
    mut master_action: Option<&mut net::PendingMasterMenuAction>,
    master_available: bool,
) -> bool {
    let mut play = play;
    let mut quit = false;
    for cmd in cmds {
        match cmd {
            ScreenCmd::Open(target) => {
                quit |= open_named(catalog, stack, target, play_music, maps);
            }
            ScreenCmd::Back => close_top(stack),
            ScreenCmd::CloseAll => {
                if !stack.names.is_empty() {
                    let first = stack.names[0].clone();
                    stack.names.clear();
                    stack.names.push(first);
                }
            }
            ScreenCmd::PlaySound(alias) => {
                if let Some(play) = play.as_mut() {
                    play.write(UiPlaySound {
                        alias: alias.clone(),
                    });
                }
            }
            ScreenCmd::Emit(UiIntent::Quit) => quit = true,
            ScreenCmd::Emit(UiIntent::LoadMap(zone)) => {
                if let Some(pending) = pending.as_mut() {
                    pending.0 = Some(zone.clone());
                } else {
                    diag::warn!(Ui, "menu: UiIntent::LoadMap with no PendingMenuMap");
                }
            }
            ScreenCmd::Emit(UiIntent::RefreshServers) => {
                if let Some(action) = master_action.as_mut() {
                    action.0 = Some(net::MasterMenuAction::Refresh);
                }
            }
            ScreenCmd::Emit(UiIntent::JoinPublicLobby {
                advert_id,
                map,
                mode,
            }) => {
                if !master_available {
                    diag::warn!(Ui, "menu: join requires a configured master");
                    continue;
                }
                let Some(action) = master_action.as_mut() else {
                    diag::warn!(Ui, "menu: join has no master action boundary");
                    continue;
                };
                action.0 = Some(net::MasterMenuAction::Join {
                    advert_id: *advert_id,
                    map: map.clone(),
                    mode: mode.clone(),
                });
            }
            ScreenCmd::Emit(UiIntent::SelectGamePrivacy(public)) => {
                if let Some(setup) = game_setup.as_mut() {
                    if *public == (setup.privacy == GamePrivacy::Public) {
                        continue;
                    }
                    if *public {
                        if !master_available {
                            diag::warn!(Ui, "menu: public lobby requires configured master");
                            continue;
                        }
                        let Some(map) = setup.selected_map.clone() else {
                            diag::warn!(Ui, "menu: cannot publish lobby without a selected map");
                            continue;
                        };
                        let mode = setup.selected_mode.token().to_owned();
                        let Some(action) = master_action.as_mut() else {
                            diag::warn!(Ui, "menu: public lobby has no master action boundary");
                            continue;
                        };
                        action.0 = Some(net::MasterMenuAction::Host { map, mode });
                        setup.privacy = GamePrivacy::Public;
                    } else {
                        if let Some(action) = master_action.as_mut() {
                            action.0 = Some(net::MasterMenuAction::LeaveLobby);
                        }
                        setup.privacy = GamePrivacy::Private;
                    }
                }
            }
            ScreenCmd::Emit(UiIntent::SelectGameMap(map)) => {
                if let Some(setup) = game_setup.as_mut() {
                    setup.selected_map = Some(map.clone());
                    setup.previewed_map = Some(map.clone());
                    if setup.privacy == GamePrivacy::Public
                        && setup.role == GameLobbyRole::Host
                        && stack.names.iter().any(|name| name == "game_lobby")
                    {
                        if let Some(action) = master_action.as_mut() {
                            action.0 = Some(net::MasterMenuAction::UpdateLobby {
                                map: map.clone(),
                                mode: setup.selected_mode.token().to_owned(),
                            });
                        }
                    }
                } else {
                    diag::warn!(Ui, "menu: map selection with no game setup draft");
                }
            }
            ScreenCmd::Emit(UiIntent::SelectGameMapPage(page)) => {
                if let Some(setup) = game_setup.as_mut() {
                    setup.map_page = *page as usize;
                }
            }
            ScreenCmd::Emit(UiIntent::SelectGameMode(mode)) => {
                if let Some(setup) = game_setup.as_mut() {
                    match sim::HostGameModeSelection::from_token(mode) {
                        Some(mode) => {
                            setup.selected_mode = mode;
                            if setup.privacy == GamePrivacy::Public
                                && setup.role == GameLobbyRole::Host
                                && stack.names.iter().any(|name| name == "game_lobby")
                            {
                                if let (Some(map), Some(action)) =
                                    (setup.selected_map.as_ref(), master_action.as_mut())
                                {
                                    action.0 = Some(net::MasterMenuAction::UpdateLobby {
                                        map: map.clone(),
                                        mode: mode.token().to_owned(),
                                    });
                                }
                            }
                        }
                        None => diag::warn!(Ui, "menu: unsupported game mode `{mode}`"),
                    }
                } else {
                    diag::warn!(Ui, "menu: mode selection with no game setup draft");
                }
            }
            ScreenCmd::Emit(UiIntent::CreateLobby { map, public }) => {
                if let Some(setup) = game_setup.as_mut() {
                    setup.role = GameLobbyRole::Host;
                    setup.selected_map = Some(map.clone());
                    setup.previewed_map = Some(map.clone());
                    setup.privacy = if *public {
                        GamePrivacy::Public
                    } else {
                        GamePrivacy::Private
                    };
                }
                if *public {
                    if master_available {
                        if let Some(action) = master_action.as_mut() {
                            let mode = game_setup
                                .as_ref()
                                .map_or("dm", |setup| setup.selected_mode.token());
                            action.0 = Some(net::MasterMenuAction::Host {
                                map: map.clone(),
                                mode: mode.to_owned(),
                            });
                        }
                    } else {
                        diag::warn!(Ui, "menu: public lobby requires configured master");
                    }
                }
            }
            ScreenCmd::Emit(UiIntent::StartLobbyMatch { map, public }) => {
                if *public {
                    if let Some(action) = master_action.as_mut() {
                        let mode = game_setup
                            .as_ref()
                            .map_or("dm", |setup| setup.selected_mode.token());
                        action.0 = Some(net::MasterMenuAction::StartMatch {
                            map: map.clone(),
                            mode: mode.to_owned(),
                        });
                    }
                }
                if !*public {
                    if let Some(pending) = pending.as_mut() {
                        pending.0 = Some(map.clone());
                    }
                }
            }
            ScreenCmd::Emit(UiIntent::VoteToSkip) => {
                if let Some(action) = master_action.as_mut() {
                    action.0 = Some(net::MasterMenuAction::VoteToSkip);
                }
            }
            ScreenCmd::Emit(UiIntent::LeaveLobby) => {
                if let Some(action) = master_action.as_mut() {
                    action.0 = Some(net::MasterMenuAction::LeaveLobby);
                }
            }
            ScreenCmd::Emit(intent)
                if matches!(
                    intent,
                    UiIntent::CacSelectSlot(_)
                        | UiIntent::CacEditRow(_)
                        | UiIntent::CacPick(_)
                        | UiIntent::CacResetClass
                        | UiIntent::CacPickCategory(_)
                        | UiIntent::CacEditAttachments(_)
                        | UiIntent::CacPickAttachment(_)
                        | UiIntent::CacPage(_)
                        | UiIntent::CacBeginRename
                        | UiIntent::CacCommitRename(_)
                        | UiIntent::CacCancelRename
                        | UiIntent::CacCancelEdit
                ) =>
            {
                if let Some((scratch, loadout)) = cac.as_mut() {
                    let _ = apply_cac_intent(scratch, loadout, intent);
                }
            }
            ScreenCmd::Emit(
                UiIntent::SetBinding { .. }
                | UiIntent::BeginBinding { .. }
                | UiIntent::SetSetting { .. }
                | UiIntent::SelectOptionsTab(_)
                | UiIntent::BeginPlayerNameEdit
                | UiIntent::CommitPlayerNameEdit(_)
                | UiIntent::CancelPlayerNameEdit
                | UiIntent::Disconnect
                | UiIntent::SelectClass(_),
            ) => {}
            ScreenCmd::Emit(intent) => {
                diag::warn!(Ui, "menu: UiIntent `{intent:?}` not routed (typed gap)");
            }
        }
    }
    quit
}
pub(crate) fn handle_retail_clicks(
    catalog: Option<Res<MenuCatalog>>,
    maps: Option<Res<MenuMapList>>,
    mut pending: ResMut<PendingMenuMap>,
    mut game_setup: ResMut<GameSetupDraft>,
    mut stack: ResMut<RetailMenuStack>,
    mut writers: RetailActionWriters,
    master_intent: Option<Res<net::MasterLaunchIntent>>,
    mut master_action: Option<ResMut<net::PendingMasterMenuAction>>,
    focus: Res<crate::nav::Focus>,
    pulse: Res<crate::nav::ActivatePulse>,
    pointer_activations: Res<crate::nav::PointerActivation>,
    clicked: Query<
        (&Interaction, &PaintedWidget, &WidgetBehavior),
        (Changed<Interaction>, With<Button>),
    >,
    focused: Query<(&crate::nav::Focusable, &WidgetBehavior), With<Button>>,
    options: Res<crate::OptionsState>,
    classes: Res<ClassSetupScratch>,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let maps = maps.as_ref().map(|m| m.0.as_slice()).unwrap_or(&[]);
    let mut ids: Vec<String> = Vec::new();
    let mut behaviors: Vec<WidgetBehavior> = Vec::new();
    for (interaction, widget, behavior) in &clicked {
        if matches!(*interaction, Interaction::Pressed)
            && action_widget_is_active(&stack, &options, &classes, &widget.id)
        {
            if !ids.contains(&widget.id) {
                ids.push(widget.id.clone());
                behaviors.push(behavior.clone());
            }
        }
    }
    for id in &pointer_activations.0 {
        if ids.contains(id) || !action_widget_is_active(&stack, &options, &classes, id) {
            continue;
        }
        if let Some((_, behavior)) = focused.iter().find(|(widget, _)| widget.id == *id) {
            ids.push(id.clone());
            behaviors.push(behavior.clone());
        }
    }
    if pulse.0 {
        for (widget, behavior) in &focused {
            if crate::nav::focus_matches(&focus, &widget.id) && !ids.contains(&widget.id) {
                ids.push(widget.id.clone());
                behaviors.push(behavior.clone());
            }
        }
    }
    for behavior in behaviors {
        for cmd in &behavior.on_activate {
            if let ScreenCmd::Emit(
                intent @ (UiIntent::SetBinding { .. }
                | UiIntent::BeginBinding { .. }
                | UiIntent::SetSetting { .. }
                | UiIntent::SelectOptionsTab(_)
                | UiIntent::BeginPlayerNameEdit
                | UiIntent::CommitPlayerNameEdit(_)
                | UiIntent::CancelPlayerNameEdit
                | UiIntent::CacSelectSlot(_)
                | UiIntent::CacEditRow(_)
                | UiIntent::CacPick(_)
                | UiIntent::CacResetClass
                | UiIntent::CacPickCategory(_)
                | UiIntent::CacEditAttachments(_)
                | UiIntent::CacPickAttachment(_)
                | UiIntent::CacPage(_)
                | UiIntent::CacBeginRename
                | UiIntent::CacCommitRename(_)
                | UiIntent::CacCancelRename
                | UiIntent::CacCancelEdit
                | UiIntent::Disconnect
                | UiIntent::SelectClass(_)),
            ) = cmd
            {
                writers.intents.write(intent.clone());
            }
        }
        if run_screen_cmds(
            &catalog,
            &mut stack,
            &behavior.on_activate,
            Some(&mut writers.play),
            &mut writers.play_music,
            maps,
            Some(&mut pending),
            Some(&mut game_setup),
            None,
            master_action.as_mut().map(|action| action.as_mut()),
            master_intent
                .as_ref()
                .is_some_and(|intent| intent.browser_available()),
        ) {
            writers.exit.write(AppExit::Success);
        }
    }
}

fn action_widget_is_active(
    stack: &RetailMenuStack,
    options: &crate::OptionsState,
    classes: &ClassSetupScratch,
    id: &str,
) -> bool {
    match stack.names.last().map(String::as_str) {
        Some("options") => crate::options::options_pointer_widget_is_active(options, id),
        Some("class_setup") => crate::class_setup::class_widget_is_active(classes, id),
        _ => true,
    }
}

#[derive(SystemParam)]
pub(crate) struct MenuOccupancy<'w> {
    enabled: ResMut<'w, MenuEnabled>,
    screen: Res<'w, frame::AppScreen>,
}

pub(crate) fn handle_menu_back(
    mut occupancy: MenuOccupancy,
    catalog: Option<Res<MenuCatalog>>,
    maps: Option<Res<MenuMapList>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cmds: MessageReader<crate::nav::MenuShellCmd>,
    mut stack: ResMut<RetailMenuStack>,
    mut play_music: MessageWriter<UiPlayMusic>,
    mut exit: MessageWriter<AppExit>,
    mut options: ResMut<crate::OptionsState>,
    mut bindings: ResMut<crate::BindingView>,
    mut scratch: ResMut<ClassSetupScratch>,
    loadout: Res<ClassLoadoutCatalog>,
    mut focus: ResMut<crate::nav::Focus>,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let maps = maps.as_ref().map(|m| m.0.as_slice()).unwrap_or(&[]);
    let mut back = keys.just_pressed(KeyCode::Escape);
    let mut left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA);
    for cmd in cmds.read() {
        if matches!(cmd, crate::nav::MenuShellCmd::Back) {
            back = true;
        }
        if matches!(cmd, crate::nav::MenuShellCmd::Nav(crate::NavDir::Left)) {
            left = true;
        }
    }
    if !occupancy.enabled.0 {
        if back && *occupancy.screen == frame::AppScreen::InGame {
            stack.names.push("ingame_options".into());
            occupancy.enabled.0 = true;
            focus.widget = Some("ingame_options/choose_class".into());
        }
        return;
    }
    if *occupancy.screen == frame::AppScreen::ClassSelect {
        return;
    }
    if back && stack.names.last().map(String::as_str) == Some("ingame_options") {
        stack.names.pop();
        occupancy.enabled.0 = false;
        focus.widget = None;
        return;
    }
    let class_left = left
        && stack.names.last().map(String::as_str) == Some("class_setup")
        && !scratch.is_picker();
    if !back && !class_left {
        return;
    }
    match stack.names.last().map(String::as_str) {
        Some("options") => {
            if options.name_buffer.take().is_some() {
                options.revision = options.revision.wrapping_add(1);
                focus.widget = Some("options/player_name".into());
                return;
            }
            if bindings.listening.take().is_some() {
                bindings.revision = bindings.revision.wrapping_add(1);
                return;
            }
            if let Some(parent) = options.go_parent() {
                focus.widget = Some(parent);
                return;
            }
        }
        Some("class_setup") => {
            if class_left && scratch.rename_buffer.is_some() {
                return;
            }
            if scratch.rename_buffer.is_some() {
                scratch.cancel_rename();
                focus.widget = Some("class_setup/rename".into());
                return;
            }
            if scratch.editing.is_some() || scratch.editing_attachment.is_some() {
                let target = crate::class_setup::cancel_edit_focus_target(&scratch);
                apply_cac_intent(&mut scratch, &loadout, &UiIntent::CacCancelEdit);
                if let Some(target) = target {
                    focus.widget = Some(target);
                }
                return;
            }
            if scratch.leave_summary() {
                focus.widget = Some(format!("class_setup/slot/{}", scratch.selected));
                return;
            }
            if class_left {
                return;
            }
        }
        _ => {}
    }
    if apply_back(&catalog, &mut stack, &mut play_music, maps) {
        exit.write(AppExit::Success);
    }
}

pub(crate) fn window_size(windows: &Query<&Window, With<PrimaryWindow>>) -> (f32, f32) {
    windows
        .single()
        .map(|w| (w.width().max(1.0), w.height().max(1.0)))
        .unwrap_or((1280.0, 720.0))
}
