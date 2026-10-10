use std::path::PathBuf;

use asset_game::load_ui_menu_catalog;
use asset_transport::{LoadProgress, find_runtime_common_mp, find_zone_file, list_menu_map_packs};
use assets::{LoadingPreviewSource, LoadingScreen, MatchLoadRequest, NamespaceTrees};
use bevy::prelude::*;
use bevy::window::PresentMode;
use render::diag::acceptance::{
    ACCEPTANCE_HEIGHT, ACCEPTANCE_PRESENT_MODE, ACCEPTANCE_WIDTH, AcceptanceRun,
};
use render::diag::capture::{CaptureQueue, CaptureRequest};
use render_frontend::prepare::scene::world::WorldScene;
use replay::{Playback, ReplayPlayback};
use session::StartupCommands;
use ui::{
    AppScreen, LaunchIdentity, LaunchReport, MenuMapList, UiAssetRoot, UiDraw, UiLayer, UiLayers,
};

use crate::args::{AcceptanceLaunch, LaunchMode};
use crate::bench;
use crate::plugins::{add_runtime_plugins, add_runtime_plugins_with_role};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Listen,
    Dedicated,
    Client,
    Replay,
}

struct LaunchConfig {
    role: Role,
    zone: String,
    games_root: PathBuf,
    artifacts: PathBuf,
}

fn launch_report(
    zone: String,
    common_mp: Result<PathBuf, String>,
    zone_ff: Result<PathBuf, String>,
    zone_alias: Option<String>,
) -> LaunchReport {
    LaunchReport {
        zone,
        common_mp,
        zone_ff,
        zone_alias,
        sim_gap: "map loading has not reached clipmap yet",
        prediction_metrics: None,
        world_report: vec!["queued background zone load".into()],
    }
}

fn launch_identity(config: &LaunchConfig) -> LaunchIdentity {
    LaunchIdentity {
        role_label: format!("{:?}", config.role),
        games_root: config.games_root.clone(),
        artifacts: config.artifacts.clone(),
        zone: config.zone.clone(),
    }
}

fn fatal(msg: &str) -> ! {
    diag::exit_launch_error(msg);
}

pub fn launch(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    mode: LaunchMode,
    acceptance: Option<AcceptanceLaunch>,
    cheats: sim::HostCheats,
) {
    asset_transport::set_game_folders(console::stored_game_folders(&artifacts));
    let steam = asset_transport::link_steam_games(&games);
    diag::info!(Launch, "{}", asset_transport::games_root_report(&games));

    if let Some(plan) = crate::frame_owner::prefer_performance_cores() {
        assets::publish_process_cpus(plan.allowed.clone());
        diag::info!(
            Launch,
            "frame owner: cpus {:?} of {:?} allowed (performance cores {:?})",
            plan.chosen,
            plan.allowed,
            plan.performance
        );
    }
    match mode {
        LaunchMode::Menu => {
            if acceptance.is_some() {
                fatal(&format!(
                    "render acceptance requires `iw4l map <zone>` (not menu); maps: {}",
                    render::diag::acceptance::ACCEPTANCE_MAPS.join(", ")
                ));
            }
            run_menu(games, artifacts, cheats, &steam);
        }
        LaunchMode::Map(zone) => run_map(
            games,
            artifacts,
            zone,
            acceptance,
            Role::Listen,
            None,
            cheats,
        ),
        LaunchMode::Serve(zone) => {
            run_map(games, artifacts, zone, None, Role::Dedicated, None, cheats)
        }
        LaunchMode::ExportGltf(zone) => {
            if acceptance.is_some() {
                fatal("render acceptance is not available for export-gltf");
            }
            run_export_gltf(games, artifacts, zone);
        }
        LaunchMode::Play {
            name,
            zone_override,
        } => run_play(games, artifacts, name, zone_override, acceptance),
    }
}

fn run_export_gltf(games: asset_transport::GamesRoot, artifacts: PathBuf, zone_arg: String) {
    let found = find_zone_file(&games, &zone_arg)
        .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    let common_mp = find_runtime_common_mp(&games, &found.path).map(|zone| zone.path);

    let prepared = match bevy::tasks::futures_lite::future::block_on(assets::load_prepared_match(
        Ok(found.path),
        common_mp,
        asset_transport::LoadProgress::default(),
    )) {
        assets::MatchLoadOutcome::Ready(prepared) => prepared,
        assets::MatchLoadOutcome::Canceled => fatal("export-gltf: map walk canceled"),
        assets::MatchLoadOutcome::Refused(reason) => {
            fatal(&format!("export-gltf: map walk refused: {reason}"))
        }
    };
    let summary = assets::export_prepared_world_gltf(
        &artifacts,
        &found.zone_name,
        prepared.world,
        &prepared.materials,
    )
    .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    diag::announce_stdout(&summary.scene.display().to_string());
    diag::announce_stdout(&summary.report_line());
}

fn run_menu(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    cheats: sim::HostCheats,
    _steam: &asset_transport::SteamProbe,
) {
    start_perf(None, "menu");
    let maps = list_menu_map_packs(&games);
    let config = LaunchConfig {
        role: Role::Listen,
        zone: String::new(),
        games_root: games.0.clone(),
        artifacts,
    };
    let trees = NamespaceTrees::discover(&games);
    let have = content_flags(&trees);
    let intent = net::MasterLaunchIntent::browser_from_env(have).unwrap_or_else(|error| {
        diag::warn!(Net, "master browser disabled: {error}");
        net::MasterLaunchIntent::disabled()
    });
    let menus = asset_game::MenuCatalog::default();
    let mut app = App::new();
    app.insert_resource(intent);
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            primary_window: Some(Window {
                title: "IW4L ? Game Library".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        },
    ));
    app.insert_resource(launch_identity(&config))
        .insert_resource(cheats)
        .insert_resource(menus)
        .insert_resource(MenuMapList(maps))
        .insert_resource(AppScreen::MainMenu)
        .insert_resource(StartupCommands {
            lines: console::startup_commands(),
        })
        .insert_resource(WorldScene::default())
        .insert_resource(ClearColor(Color::srgb(0.02, 0.025, 0.03)))
        .insert_resource({
            let mut layers = UiLayers::default();
            layers.show_only([UiLayer::Shell, UiLayer::Overlay]);
            layers
        });
    add_runtime_plugins(&mut app);
    app.init_resource::<MenuAssets>()
        .add_systems(Update, load_menu_assets);
    if let Some(capture) = CaptureRequest::from_env() {
        queue_launch_capture(
            &mut app,
            CaptureRequest {
                exit_after_capture: true,
                ..capture
            },
        );
    }
    app.run();
    let _ = flush_perf();
}

#[derive(Resource, Default)]
struct MenuAssets {
    root: Option<PathBuf>,
    task: Option<bevy::tasks::Task<(asset_game::MenuCatalog, PathBuf)>>,
}

fn load_menu_assets(
    identity: Res<LaunchIdentity>,
    dvars: Res<frame::UiMenuDvars>,
    mut pending: ResMut<MenuAssets>,
    mut catalog: ResMut<asset_game::MenuCatalog>,
    mut commands: Commands,
) {
    if let Some(task) = pending.task.as_mut() {
        if let Some((mut loaded, root)) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(task),
        ) {
            if ui::install_frontend_menus(&mut loaded).is_ok() {
                *catalog = loaded;
                commands.insert_resource(UiAssetRoot(Some(root)));
            }
            pending.task = None;
        }
        return;
    }
    if dvars.get("ui_game_namespace") != Some("iw4") {
        return;
    }
    let games = asset_transport::GamesRoot(identity.games_root.clone());
    let Ok(root) = asset_game::ui_games_root(&games) else {
        return;
    };
    if pending.root.as_ref() == Some(&root.0) {
        return;
    }
    pending.root = Some(root.0.clone());
    pending.task = Some(assets::load_pool().spawn(async move {
        let (catalog, _) = load_ui_menu_catalog(&root);
        (catalog, root.0)
    }));
}

fn run_play(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    name: String,
    zone_override: Option<String>,
    acceptance: Option<AcceptanceLaunch>,
) {
    if acceptance.is_some() {
        fatal("render acceptance requires `iw4l map <zone>` (not play)");
    }
    let playback = Playback::open(&artifacts, &name).unwrap_or_else(|e| {
        fatal(&format!("play: {e}"));
    });
    let zone = zone_override
        .or_else(|| playback.identity().zone_name().map(str::to_owned))
        .unwrap_or_else(|| {
            fatal(&format!(
                "play: recording {} has no zone — re-record on this build, or: make play {name} ZONE=<map>",
                playback.path().display()
            ));
        });
    diag::info!(Launch, "play: {} zone={zone}", playback.path().display());
    let session = ReplayPlayback::new(playback);
    run_map(
        games,
        artifacts,
        zone,
        None,
        Role::Replay,
        Some(session),
        sim::HostCheats::default(),
    );
}

fn run_map(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    zone_arg: String,
    acceptance: Option<AcceptanceLaunch>,
    role: Role,
    playback: Option<ReplayPlayback>,
    cheats: sim::HostCheats,
) {
    let found = find_zone_file(&games, &zone_arg);
    let zone_alias = found.as_ref().ok().and_then(|z| z.alias_note.clone());
    let zone_arg_lc = zone_arg.trim().to_ascii_lowercase();
    let (parsed_game, requested_stem) = asset_transport::split_zone_key(&zone_arg_lc);
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
        Ok(path) => find_runtime_common_mp(&games, path).map(|z| z.path),
        Err(e) => Err(e.clone()),
    };
    let probe = launch_report(zone.clone(), common_mp.clone(), zone_ff.clone(), zone_alias);
    let game = parsed_game.or_else(|| {
        found
            .as_ref()
            .ok()
            .and_then(|z| asset_transport::zone_game_for_path(&z.path))
    });
    let loading_title = asset_transport::map_load_title(&zone_arg_lc, game);
    let namespace_trees = NamespaceTrees::discover(&games);
    let have = content_flags(&namespace_trees);
    let map_key = game.map_or_else(|| zone.clone(), |game| format!("{}:{zone}", game.prefix()));
    let requires = net::content_required_by_map(&map_key)
        .unwrap_or_else(|error| fatal(&format!("master content: {error}")));

    let master_intent = if role == Role::Replay {
        net::MasterLaunchIntent::disabled()
    } else {
        net::MasterLaunchIntent::from_env_for_map(&map_key, have, requires).unwrap_or_else(
            |error| {
                diag::warn!(Net, "master launch disabled: {error}");
                net::MasterLaunchIntent::disabled()
            },
        )
    };

    let joining = master_intent.is_join();
    let config = LaunchConfig {
        role: if joining { Role::Client } else { role },
        zone: zone.clone(),
        games_root: games.0.clone(),
        artifacts,
    };
    start_perf(Some(zone.clone()), role_name(config.role));
    let (mut menus, menu_report) = load_ui_menu_catalog(&games);

    if game == Some(asset_core::ZoneGame::Iw4) {
        ui::install_frontend_menus(&mut menus).unwrap_or_else(|error| fatal(&error));
    } else if let Err(error) = ui::install_frontend_menus(&mut menus) {
        diag::warn!(
            Launch,
            "frontend menus unavailable for {}: {error}",
            game.map(|g| g.prefix()).unwrap_or("unknown")
        );
    }
    for line in &menu_report {
        diag::info!(Launch, "{line}");
    }
    let progress = LoadProgress::default();
    let acceptance_run = acceptance
        .map(|a| AcceptanceRun {
            artifact_dir: a.dir,
            map: zone.clone(),
        })
        .or_else(|| AcceptanceRun::from_env(zone.clone()));
    let present_mode = launch_present_mode(acceptance_run.is_some());
    let mut app = App::new();
    if acceptance_run.is_some() || std::env::var_os("IW4L_PRESENT_MODE").is_some() {
        app.insert_resource(ui::PresentModeOverride(present_mode));
    }
    app.insert_resource(master_intent);
    let dedicated = config.role == Role::Dedicated;
    if dedicated {
        app.insert_resource(bevy::winit::WinitSettings::continuous())
            .insert_resource(frame::Headless);
    }
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            exit_condition: if dedicated {
                bevy::window::ExitCondition::DontExit
            } else {
                bevy::window::ExitCondition::OnAllClosed
            },
            primary_window: (!dedicated).then(|| Window {
                title: match config.role {
                    Role::Replay => format!("iw4l — play {}", config.zone),
                    Role::Listen | Role::Dedicated => format!("iw4l — {}", config.zone),
                    Role::Client => format!("iw4l — join {}", config.zone),
                },
                resolution: (ACCEPTANCE_WIDTH, ACCEPTANCE_HEIGHT).into(),
                present_mode,
                ..default()
            }),
            ..default()
        },
    ));
    if let Ok(path) = &zone_ff {
        app.insert_resource(LoadingPreviewSource {
            path: path.clone(),
            map_name: zone.clone(),
            request_id: 0,
        });
    }
    let ui_games_root = asset_game::ui_games_root(&games).ok().map(|root| root.0);
    app.insert_resource(launch_identity(&config))
        .insert_resource(cheats)
        .insert_resource(probe)
        .insert_resource(menus)
        .insert_resource(MatchLoadRequest {
            request_id: 0,
            load_key: Default::default(),
            zone: zone.clone(),
            zone_ff,
            common_mp,
            progress: progress.clone(),
        })
        .insert_resource(LoadingScreen::new(
            progress.clone(),
            loading_title,
            sim::host_game_mode_kind().display_name().to_owned(),
        ))
        .insert_resource(UiAssetRoot(ui_games_root))
        .insert_resource(StartupCommands {
            lines: console::startup_commands(),
        })
        .insert_resource(WorldScene::default())
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AppScreen::Loading)
        .insert_resource({
            let mut layers = UiLayers::default();
            layers.show_only([UiLayer::Loading, UiLayer::Overlay]);
            layers
        });
    if joining {
        app.world_mut().remove_resource::<MatchLoadRequest>();
        app.world_mut().remove_resource::<LoadingPreviewSource>();
        app.world_mut().remove_resource::<LoadingScreen>();
        app.insert_resource(AppScreen::MainMenu);
    }
    if let Some(run) = acceptance_run {
        diag::info!(
            Launch,
            "render acceptance: dir={} map={} {}x{} {:?}",
            run.artifact_dir.display(),
            run.map,
            ACCEPTANCE_WIDTH,
            ACCEPTANCE_HEIGHT,
            present_mode
        );

        app.insert_resource(UiDraw(false));
        app.insert_resource(run);
    }
    match config.role {
        Role::Replay => add_runtime_plugins_with_role(&mut app, frame::RuntimeRole::Replay),
        Role::Listen | Role::Dedicated => add_runtime_plugins(&mut app),
        Role::Client => add_runtime_plugins_with_role(&mut app, frame::RuntimeRole::Client),
    }
    // The demo, before the playback moves into the world: it names the workload
    // in the bench manifest, and "same demo" is what makes two runs comparable
    // at all. The whole path, not a stem — a clip is `clips/<id>/clip.iw4ldemo`,
    // whose stem is `clip` for every clip ever recorded.
    let demo = playback
        .as_ref()
        .map(|playback| playback.path().display().to_string());
    if let Some(playback) = playback {
        app.insert_resource(playback);
    }
    if let Some(capture) = CaptureRequest::from_env() {
        queue_launch_capture(&mut app, capture);
    }
    bench::announce_runtime(&mut app);
    let bench = bench::enabled();
    if bench {
        bench::insert(
            &mut app,
            &zone,
            demo.as_deref(),
            role_name(config.role),
            &config.artifacts,
            progress.clone(),
        );
    }
    app.run();
    let trace = flush_perf();
    if bench {
        bench::finish(&config.artifacts, trace);
    }
}

fn content_flags(trees: &NamespaceTrees) -> master_protocol::ContentFlags {
    net::content_inventory(
        trees.get(asset_core::AssetNamespace::Iw4).is_some(),
        trees.get(asset_core::AssetNamespace::Iw5).is_some(),
        trees.get(asset_core::AssetNamespace::T5).is_some(),
        trees.get(asset_core::AssetNamespace::T6).is_some(),
    )
}

fn start_perf(zone: Option<String>, role: &str) {
    if let Err(error) = perf::start(perf::RunMetadata {
        zone,
        role: role.to_owned(),
        focus: std::env::var("IW4L_PERF_FOCUS").ok(),
    }) {
        diag::error!(Launch, "perf: start failed: {error}");
    }
}

fn flush_perf() -> Option<PathBuf> {
    match perf::flush() {
        Ok(path) => path,
        Err(error) => {
            diag::error!(Launch, "perf: flush failed: {error}");
            None
        }
    }
}

fn queue_launch_capture(app: &mut App, request: CaptureRequest) {
    app.world_mut()
        .get_resource_mut::<CaptureQueue>()
        .expect("CaptureQueue: RenderPlugin must be added before a launch capture is queued")
        .push(request);
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::Listen => "listen",
        Role::Dedicated => "dedicated",
        Role::Client => "client",
        Role::Replay => "replay",
    }
}

fn launch_present_mode(acceptance: bool) -> PresentMode {
    if acceptance {
        return ACCEPTANCE_PRESENT_MODE;
    }
    match std::env::var("IW4L_PRESENT_MODE").ok().as_deref() {
        Some("AutoNoVsync") => PresentMode::AutoNoVsync,
        Some("Immediate") => PresentMode::Immediate,
        Some("Mailbox") => PresentMode::Mailbox,
        Some("Fifo") => PresentMode::Fifo,
        Some(other) => {
            diag::warn!(Launch, "unknown IW4L_PRESENT_MODE={other}; using Fifo");
            PresentMode::default()
        }
        None => PresentMode::default(),
    }
}
