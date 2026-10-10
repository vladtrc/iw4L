use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use frame::ClientSet;

use asset_game::{LocalizeCatalog, load_mp_localized_strings};
use asset_transport::{
    GamesRoot, LoadProgress, find_runtime_common_mp, find_zone_file, list_menu_map_packs,
};
use assets::{LoadingPreviewSource, MatchLoadRequest};

use crate::classes::select::ClassSelectOverlayOpen;
use crate::layers::{UiLayer, UiLayers};
use crate::loading::{
    LoadingCamera, LoadingRoot, LoadingScreen, OverlayUiCamera, dismiss_loading_overlay,
};
use crate::menu::MenuMapList;
use frame::{AppScreen, LaunchIdentity, LaunchReport, MapLoadApproved, ReturnedToMenu};

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
    games: &asset_transport::GamesRoot,
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
    let (parsed_game, requested_stem) = asset_transport::split_zone_key(&requested_lc);
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
            .and_then(|z| asset_transport::zone_game_for_path(&z.path))
    });
    let title = asset_transport::map_load_title(&requested_lc, game);
    (zone, zone_ff, common_mp, zone_alias, title)
}

fn insert_loading_chrome(
    commands: &mut Commands,
    progress: LoadProgress,
    title: String,
    zone: &str,
    zone_ff: Result<std::path::PathBuf, String>,
    request_id: u64,
    mode: sim::HostGameModeSelection,
) {
    diag::info!(Ui, "loading: map={zone} mode={}", mode.token());
    commands.insert_resource(LoadingScreen::new(
        progress,
        title,
        mode.display_name().to_owned(),
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

pub(crate) fn begin_load_from_session(
    mut commands: Commands,
    mut approved: MessageReader<MapLoadApproved>,
    mode: Option<Res<sim::HostGameModeSelection>>,
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
    let games = asset_transport::GamesRoot(identity.games_root.clone());
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
        mode.as_deref()
            .copied()
            .unwrap_or_else(sim::HostGameModeSelection::from_env),
    );
    diag::info!(
        Ui,
        "session: loading overlay `{zone}` (swap #{})",
        fact.request_id
    );
}

pub(crate) fn restore_menu_on_return(
    mut commands: Commands,
    mut returned: MessageReader<ReturnedToMenu>,
    mut class_overlay: ResMut<ClassSelectOverlayOpen>,
    mut maps: ResMut<MenuMapList>,
    identity: Option<Res<LaunchIdentity>>,
    mut app_screen: ResMut<AppScreen>,
    chrome: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    overlay_cams: Query<Entity, With<OverlayUiCamera>>,
) {
    if returned.read().count() == 0 {
        return;
    }
    class_overlay.0 = false;
    *app_screen = AppScreen::MainMenu;

    dismiss_loading_overlay(&mut commands, chrome.iter(), overlay_cams.iter(), false);
    if maps.0.is_empty() {
        if let Some(identity) = identity {
            maps.0 = list_menu_map_packs(&asset_transport::GamesRoot(identity.games_root.clone()));
        }
    }
    diag::info!(Ui, "session: main menu enabled");
    diag::lifecycle_boundary("menu_interactive", "");
}

#[derive(Resource)]
struct MenuStringsPrepare(Task<Option<LocalizeCatalog>>);

fn start_menu_strings_prepare(
    loc: Res<LocalizeCatalog>,
    running: Option<Res<MenuStringsPrepare>>,
    root: Option<Res<crate::UiAssetRoot>>,
    loading: (
        Option<Res<assets::MatchLoadBusy>>,
        Option<Res<MatchLoadRequest>>,
        Option<Res<assets::MatchLoadAccepted>>,
        Option<Res<assets::PreparedMatchReady>>,
    ),
    mut commands: Commands,
) {
    if !loc.is_empty() || running.is_some() {
        return;
    }
    let (busy, request, accepted, ready) = loading;
    if busy.is_some_and(|busy| busy.0) || request.is_some() || accepted.is_some() || ready.is_some()
    {
        return;
    }
    let Some(root) = root.as_ref().and_then(|root| root.0.clone()) else {
        return;
    };
    let task = AsyncComputeTaskPool::get().spawn(async move {
        match load_mp_localized_strings(&GamesRoot(root), "iw4:code_post_gfx_mp") {
            Ok(catalog) => Some(catalog),
            Err(error) => {
                diag::warn!(Ui, "menu: localize: {error}");
                None
            }
        }
    });
    commands.insert_resource(MenuStringsPrepare(task));
}

fn install_menu_strings_prepare(
    mut prepare: Option<ResMut<MenuStringsPrepare>>,
    mut loc: ResMut<LocalizeCatalog>,
    mut commands: Commands,
) {
    let Some(prepare) = prepare.as_deref_mut() else {
        return;
    };
    let Some(built) = future::block_on(future::poll_once(&mut prepare.0)) else {
        return;
    };
    commands.remove_resource::<MenuStringsPrepare>();
    let Some(built) = built else {
        return;
    };
    diag::info!(Ui, "menu: {} localize keys resident", built.len());
    *loc = built;
}

pub(crate) fn register_menu_load_systems(app: &mut App) {
    app.add_message::<ReturnedToMenu>().add_systems(
        Update,
        (
            begin_load_from_session,
            restore_menu_on_return.after(begin_load_from_session),
            start_menu_strings_prepare,
            install_menu_strings_prepare.after(start_menu_strings_prepare),
        )
            .after(assets::MapLoadApproval)
            .before(assets::MatchLoadDispatch)
            .in_set(ClientSet::Load),
    );
}
