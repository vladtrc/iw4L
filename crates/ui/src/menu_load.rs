use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use frame::ClientSet;

use asset_game::{LocalizeCatalog, load_localized_strings_for_zone};
use asset_transport::{
    GamesRoot, LoadProgress, find_runtime_common_mp, find_zone_file, list_mp_map_packs,
};
use assets::{LoadingPreviewSource, MatchLoadRequest};

use crate::classes::select::ClassSelectOverlayOpen;
use crate::layers::{UiLayer, UiLayers};
use crate::loading::{
    LoadingCamera, LoadingRoot, LoadingScreen, OverlayUiCamera, dismiss_loading_overlay,
};
use crate::menu::MenuMapList;
use frame::{AppScreen, LaunchIdentity, LaunchReport, MapLoadApproved};

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
        sim_gap: "map loading has not reached clipmap yet".into(),
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
    worlds: Vec<assets::LoadingWorld>,

    mode: sim::HostGameModeSelection,
) {
    diag::info!(Ui, "loading: map={zone} mode={}", mode.token());
    commands.insert_resource(
        LoadingScreen::new(progress, title, mode.display_name().to_owned())
            .with_worlds(worlds.iter().map(|world| world.title.clone()).collect()),
    );
    if let Ok(path) = zone_ff {
        commands.insert_resource(LoadingPreviewSource {
            path,
            map_name: zone.to_owned(),
            more: worlds
                .into_iter()
                .skip(1)
                .filter_map(|world| Some((world.zone_ff?, world.zone)))
                .collect(),
        });
    }
    commands.insert_resource({
        let mut layers = UiLayers::default();
        layers.show_only([UiLayer::Loading, UiLayer::Overlay]);
        layers
    });
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuScreenTransition<'w> {
    class_overlay: ResMut<'w, ClassSelectOverlayOpen>,
    app_screen: ResMut<'w, AppScreen>,
}

impl MenuScreenTransition<'_> {
    fn show(&mut self, destination: AppScreen) {
        self.class_overlay.0 = false;
        *self.app_screen = destination;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SessionLoadingOverlay<'w, 's> {
    commands: Commands<'w, 's>,
    menu_screen: MenuScreenTransition<'w>,
    identity: Option<ResMut<'w, LaunchIdentity>>,
    window: Query<'w, 's, &'static mut Window, With<bevy::window::PrimaryWindow>>,
}

impl SessionLoadingOverlay<'_, '_> {
    fn begin(
        mut self,
        mut approved: MessageReader<MapLoadApproved>,
        mode: Option<Res<sim::HostGameModeSelection>>,
        loading: Option<Res<LoadingScreen>>,
        request: Option<Res<MatchLoadRequest>>,
        transition: Res<session::SessionSwapRequest>,
    ) {
        let Some(fact) = approved.read().last() else {
            return;
        };
        if !transition.load_pending() {
            return;
        }
        let Some(mut identity) = self.identity else {
            return;
        };
        let games = asset_transport::GamesRoot(identity.games_root.clone());
        let merged = assets::MergedMap::find(&identity.artifacts, &fact.zone).and_then(Result::ok);
        let requested = merged
            .as_ref()
            .map_or(fact.zone.as_str(), assets::MergedMap::zone);
        let (zone, zone_ff, common_mp, zone_alias, mut loading_title) =
            resolve_zone(&games, requested);
        let worlds = match &merged {
            Some(merged) => {
                loading_title = merged.name.clone();
                merged.loading_worlds(&games)
            }
            None => Vec::new(),
        };
        if loading.as_ref().is_some_and(|s| s.title() == loading_title) {
            return;
        }
        let probe = launch_report(zone.clone(), common_mp, zone_ff.clone(), zone_alias);
        let progress = request
            .as_ref()
            .map(|r| r.progress.clone())
            .unwrap_or_else(LoadProgress::default);

        identity.zone = zone.clone();
        if let Ok(mut window) = self.window.single_mut() {
            window.title = format!("iw4l — {zone}");
        }
        self.menu_screen.show(AppScreen::Loading);
        self.commands.insert_resource(probe);
        insert_loading_chrome(
            &mut self.commands,
            progress,
            loading_title,
            &zone,
            zone_ff,
            worlds,
            mode.as_deref()
                .copied()
                .unwrap_or_else(sim::HostGameModeSelection::launch_default),
        );
        diag::info!(Ui, "session: loading overlay `{zone}`");
    }
}

pub(crate) fn begin_load_from_session(
    overlay: SessionLoadingOverlay,
    approved: MessageReader<MapLoadApproved>,
    mode: Option<Res<sim::HostGameModeSelection>>,
    loading: Option<Res<LoadingScreen>>,
    request: Option<Res<MatchLoadRequest>>,
    transition: Res<session::SessionSwapRequest>,
) {
    overlay.begin(approved, mode, loading, request, transition);
}

pub(crate) fn restore_menu_on_return(
    mut commands: Commands,
    mut returned: MessageReader<StateTransitionEvent<frame::MatchScope>>,
    mut menu_screen: MenuScreenTransition,
    mut maps: ResMut<MenuMapList>,
    identity: Option<Res<LaunchIdentity>>,
    chrome: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    overlay_cams: Query<Entity, With<OverlayUiCamera>>,
) {
    if !returned
        .read()
        .any(|event| event.exited.is_some() && event.entered == Some(frame::MatchScope::Absent))
    {
        return;
    }
    menu_screen.show(AppScreen::MainMenu);

    dismiss_loading_overlay(&mut commands, chrome.iter(), overlay_cams.iter(), false);
    if maps.0.is_empty() {
        if let Some(identity) = identity {
            maps.0 = list_mp_map_packs(&asset_transport::GamesRoot(identity.games_root.clone()));
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
        match load_localized_strings_for_zone(&GamesRoot(root), "iw4:code_post_gfx_mp") {
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
    app.add_systems(
        PreUpdate,
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
