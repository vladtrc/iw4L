use bevy::prelude::*;
use frame::ClientSet;

use assets::{
    LoadProgress, LoadingPreviewSource, MatchLoadRequest, find_runtime_common_mp, find_zone_file,
    list_mp_maps,
};

use crate::class_select::ClassSelectOverlayOpen;
use crate::layers::{UiLayer, UiLayers};
use crate::loading::{
    LoadingCamera, LoadingRoot, LoadingScreen, OverlayUiCamera, dismiss_loading_overlay,
};
use crate::menu::{MenuEnabled, MenuMapList, PendingMenuMap};
use frame::{
    AppScreen, LaunchIdentity, LaunchReport, MapLoadApproved, MatchTornDown, TeardownReason,
};

fn launch_report(
    zone: String,
    common_mp: Result<std::path::PathBuf, String>,
    zone_ff: Result<std::path::PathBuf, String>,
    zone_alias: Option<String>,
) -> LaunchReport {
    LaunchReport {
        zone,
        common_mp,
        zone_ff,
        zone_alias,
        sim_gap: "map loading has not reached clipmap yet",
        prediction_metrics: None,
        world_report: vec!["queued background zone walk".into()],
    }
}

fn resolve_zone(
    games: &assets::GamesRoot,
    requested: &str,
) -> (
    String,
    Result<std::path::PathBuf, String>,
    Result<std::path::PathBuf, String>,
    Option<String>,
    String,
) {
    let found = find_zone_file(games, requested);
    let zone_alias = found.as_ref().ok().and_then(|z| z.alias_note.clone());
    let requested_lc = requested.trim().to_ascii_lowercase();
    let (parsed_game, requested_stem) = assets::split_zone_key(&requested_lc);
    let zone = found
        .as_ref()
        .ok()
        .map(|z| z.zone_name.clone())
        .unwrap_or_else(|| requested_stem.to_owned());
    let zone_ff = found
        .as_ref()
        .map(|z| z.path.clone())
        .map_err(|e| e.clone());
    let common_mp = match &zone_ff {
        Ok(path) => find_runtime_common_mp(games, path).map(|z| z.path),
        Err(e) => Err(e.clone()),
    };
    let game = parsed_game.or_else(|| {
        found
            .as_ref()
            .ok()
            .and_then(|z| assets::zone_game_for_path(&z.path))
    });
    let title = assets::map_load_title(&requested_lc, game);
    (zone, zone_ff, common_mp, zone_alias, title)
}

fn insert_loading_chrome(
    commands: &mut Commands,
    progress: LoadProgress,
    title: String,
    zone: &str,
    zone_ff: Result<std::path::PathBuf, String>,
    request_id: u64,
) {
    commands.insert_resource(LoadingScreen::new(
        progress,
        title,
        sim::host_game_mode_kind().display_name().to_owned(),
    ));
    if let Ok(path) = zone_ff {
        commands.insert_resource(LoadingPreviewSource {
            path,
            map_name: zone.to_owned(),
            request_id,
        });
    }
    commands.insert_resource({
        let mut layers = UiLayers::default();
        layers.show_only([UiLayer::Loading, UiLayer::Overlay]);
        layers
    });
}

pub(crate) fn begin_map_from_menu(
    mut commands: Commands,
    mut pending: ResMut<PendingMenuMap>,
    mut menu_enabled: ResMut<MenuEnabled>,
    mut app_screen: ResMut<AppScreen>,
    mut identity: ResMut<LaunchIdentity>,
    game_setup: Res<crate::menu::GameSetupDraft>,
    mut class_overlay: ResMut<ClassSelectOverlayOpen>,
    mut window: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(requested) = pending.0.take() else {
        return;
    };
    let games = assets::GamesRoot(identity.games_root.clone());
    let (zone, zone_ff, common_mp, zone_alias, loading_title) = resolve_zone(&games, &requested);
    let probe = launch_report(zone.clone(), common_mp.clone(), zone_ff.clone(), zone_alias);
    let progress = LoadProgress::default();

    identity.zone = zone.clone();
    if let Ok(mut window) = window.single_mut() {
        window.title = format!("iw4l — {zone}");
    }

    menu_enabled.0 = false;
    class_overlay.0 = false;
    *app_screen = AppScreen::Loading;
    commands.insert_resource(probe);
    commands.insert_resource(game_setup.selected_mode);
    commands.insert_resource(MatchLoadRequest {
        request_id: 0,
        load_key: frame::LocalLoadKey::from_request(0, frame::MatchKey::NONE, 0),
        zone: zone.clone(),
        zone_ff: zone_ff.clone(),
        common_mp,
        progress: progress.clone(),
    });
    insert_loading_chrome(&mut commands, progress, loading_title, &zone, zone_ff, 0);
    diag::info!(Ui, "menu: loading {zone}");
}

pub(crate) fn begin_load_from_session(
    mut commands: Commands,
    mut approved: MessageReader<MapLoadApproved>,
    mut menu_enabled: ResMut<MenuEnabled>,
    mut app_screen: ResMut<AppScreen>,
    mut class_overlay: ResMut<ClassSelectOverlayOpen>,
    identity: Option<ResMut<LaunchIdentity>>,
    loading: Option<Res<LoadingScreen>>,
    request: Option<Res<MatchLoadRequest>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    mut window: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(fact) = approved.read().last() else {
        return;
    };
    if abort.is_some_and(|abort| abort.0 == fact.request_id) {
        return;
    }
    let Some(mut identity) = identity else {
        return;
    };
    let games = assets::GamesRoot(identity.games_root.clone());
    let (zone, zone_ff, common_mp, zone_alias, loading_title) = resolve_zone(&games, &fact.zone);
    if loading.as_ref().is_some_and(|s| s.title() == loading_title) {
        return;
    }
    let probe = launch_report(zone.clone(), common_mp, zone_ff.clone(), zone_alias);
    let progress = request
        .as_ref()
        .filter(|r| r.request_id == fact.request_id)
        .map(|r| r.progress.clone())
        .unwrap_or_default();

    identity.zone = zone.clone();
    if let Ok(mut window) = window.single_mut() {
        window.title = format!("iw4l — {zone}");
    }
    menu_enabled.0 = false;
    class_overlay.0 = false;
    *app_screen = AppScreen::Loading;
    commands.insert_resource(probe);
    insert_loading_chrome(
        &mut commands,
        progress,
        loading_title,
        &zone,
        zone_ff,
        fact.request_id,
    );
    diag::info!(
        Ui,
        "session: loading overlay `{zone}` (swap #{})",
        fact.request_id
    );
}

pub(crate) fn restore_menu_on_disconnect(
    mut commands: Commands,
    mut torn: MessageReader<MatchTornDown>,
    mut menu_enabled: ResMut<MenuEnabled>,
    mut class_overlay: ResMut<ClassSelectOverlayOpen>,
    mut maps: ResMut<MenuMapList>,
    identity: Option<Res<LaunchIdentity>>,
    mut app_screen: ResMut<AppScreen>,
    chrome: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    overlay_cams: Query<Entity, With<OverlayUiCamera>>,
    mut stack: ResMut<crate::RetailMenuStack>,
) {
    if !torn
        .read()
        .any(|fact| fact.reason == TeardownReason::Disconnect)
    {
        return;
    }
    class_overlay.0 = false;
    if let Some(index) = stack.names.iter().position(|name| name == "ingame_options") {
        stack.names.truncate(index);
    }
    *app_screen = AppScreen::MainMenu;
    menu_enabled.0 = true;

    dismiss_loading_overlay(&mut commands, chrome.iter(), overlay_cams.iter(), false);
    if maps.0.is_empty() {
        if let Some(identity) = identity {
            maps.0 = list_mp_maps(&assets::GamesRoot(identity.games_root.clone()));
        }
    }
    diag::info!(Ui, "session: main menu enabled after disconnect");
}

pub(crate) fn register_menu_load_systems(app: &mut App) {
    app.add_systems(
        Update,
        begin_map_from_menu
            .run_if(resource_exists::<LaunchIdentity>)
            .in_set(ClientSet::Ui),
    )
    .add_systems(
        Update,
        crate::retail_menu::sync_frontend_music
            .after(begin_map_from_menu)
            .in_set(ClientSet::Ui),
    )
    .add_systems(
        Update,
        (
            begin_load_from_session,
            restore_menu_on_disconnect.after(begin_load_from_session),
        )
            .after(frame::SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}
