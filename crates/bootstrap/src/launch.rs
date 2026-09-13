use std::path::PathBuf;

use assets::{
    LoadProgress, LoadingPreviewSource, LoadingScreen, MatchLoadRequest, NamespaceSoundIwd,
    NamespaceTrees, decode_menu_background, find_runtime_common_mp, find_zone_file, list_mp_maps,
    load_mp_localized_strings, load_mp_sound_bank, load_ui_menu_catalog,
};
use audio::{SoundBank, SoundIwd};
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
    AppScreen, ClassLoadoutCatalog, LaunchIdentity, LaunchReport, MenuEnabled, MenuFrontend,
    MenuMapList, MenuShotPlan, PendingMenuBgPixels, UiAssetRoot, UiDraw, UiLayer, UiLayers,
};

use crate::args::{AcceptanceLaunch, LaunchMode};
use crate::bench_load;
use crate::plugins::{add_runtime_plugins, add_runtime_plugins_with_role};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Listen,
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
    games: assets::GamesRoot,
    artifacts: PathBuf,
    mode: LaunchMode,
    acceptance: Option<AcceptanceLaunch>,
) {
    diag::info!(Launch, "{}", assets::games_root_report(&games));

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
            run_menu(games, artifacts);
        }
        LaunchMode::Map(zone) => run_map(games, artifacts, zone, acceptance, Role::Listen, None),
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

fn run_export_gltf(games: assets::GamesRoot, artifacts: PathBuf, zone_arg: String) {
    let found = find_zone_file(&games, &zone_arg)
        .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    let game = assets::zone_game_for_path(&found.path)
        .unwrap_or_else(|| fatal("export-gltf: source game could not be identified"));
    if game != assets::ZoneGame::Iw4 {
        fatal(&format!(
            "export-gltf P1a supports native IW4 only; {} is {}",
            found.zone_name,
            game.prefix()
        ));
    }
    let common_mp = find_runtime_common_mp(&games, &found.path).map(|zone| zone.path);

    let prepared = bevy::tasks::futures_lite::future::block_on(assets::load_prepared_match(
        Ok(found.path),
        common_mp,
        assets::LoadProgress::default(),
    ));
    let summary = assets::export_prepared_world_gltf(&artifacts, &found.zone_name, prepared.world)
        .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    diag::announce_stdout(&summary.scene.display().to_string());
    diag::announce_stdout(&summary.report_line());
}

fn run_menu(games: assets::GamesRoot, artifacts: PathBuf) {
    start_perf(None, "menu");
    let ui_games = assets::ui_games_root(&games).unwrap_or_else(|error| {
        let content = assets::games_content_report(&games).join("\n");
        fatal(&format!(
            "Cannot start the IW4 menu: base MW2 Multiplayer assets were not found.\n\n\
             {content}\n\n\
             The menu requires common_mp.ff with IW4 envelope version 0x114. \
             Point IW4L_GAMES or a shortcut beside iw4launcher.exe to the folder containing \
             the base MW2 Multiplayer files. If this is the intended folder, restore its \
             missing base files; DLC maps alone are insufficient.\n\nSearch details: {error}"
        ))
    });
    let (menus, menu_report) = load_ui_menu_catalog(&ui_games);
    for line in &menu_report {
        diag::info!(Launch, "{line}");
    }
    let missing = ["main", "main_text"]
        .into_iter()
        .filter(|name| menus.get(name).is_none())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        fatal(&format!(
            "Cannot start the IW4 menu: {} not loaded from {}.\n\n\
             A missing menu can mean an absent file or a FastFile decoding failure. \
             Include the catalog details below when reporting this error.\n\n{}",
            missing.join(", "),
            ui_games.0.display(),
            menu_report.join("\n")
        ));
    }
    let maps = list_mp_maps(&games);
    diag::info!(
        Launch,
        "menu: {} maps under {}",
        maps.len(),
        games.0.display()
    );
    let config = LaunchConfig {
        role: Role::Listen,
        zone: String::new(),
        games_root: games.0.clone(),
        artifacts,
    };
    let mut app = App::new();
    let namespace_trees = NamespaceTrees::discover(&games);
    let have = content_flags(&namespace_trees);
    let master_intent = if have.0 & net::CONTENT_IW4 == 0 {
        diag::warn!(
            Net,
            "master browser disabled: IW4 common_mp.ff is not installed"
        );
        net::MasterLaunchIntent::disabled()
    } else {
        net::MasterLaunchIntent::browser_from_env(have).unwrap_or_else(|error| {
            diag::warn!(Net, "master browser disabled: {error}");
            net::MasterLaunchIntent::disabled()
        })
    };
    app.insert_resource(master_intent);
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            primary_window: Some(Window {
                title: "iw4l".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        },
    ));
    let menu_bg = match decode_menu_background(&ui_games.0) {
        Ok(Some(v)) => {
            diag::info!(Launch, "menu: background ready");
            Some(v)
        }
        Ok(None) => {
            diag::info!(Launch, "menu: no menu_mp_image in IWD");
            None
        }
        Err(error) => {
            diag::warn!(Launch, "menu: background: {error}");
            None
        }
    };
    let (shell_weapons, cac_tables, cac_report) = assets::load_shell_weapon_registry(&games);
    for line in &cac_report {
        diag::info!(Launch, "{line}");
    }
    let mut class_catalog =
        ClassLoadoutCatalog::from_weapon_registry(&shell_weapons).with_weapon_tables(&cac_tables);
    if let Some(table) = menus.string_table("mp/perkTable.csv") {
        class_catalog = class_catalog.with_perk_table(table);
    }
    diag::info!(
        Launch,
        "CAC menu: primary={} secondary={} lethal={} tactical={} excluded={}",
        class_catalog.primary.len(),
        class_catalog.secondary.len(),
        class_catalog.lethal.len(),
        class_catalog.tactical.len(),
        class_catalog.excluded.len(),
    );
    app.insert_resource(class_catalog);
    match load_mp_localized_strings(&ui_games, "iw4:code_post_gfx_mp") {
        Ok(loc) => {
            diag::info!(Launch, "menu: {} localize keys", loc.len());
            app.insert_resource(loc);
        }
        Err(error) => diag::warn!(Launch, "menu: localize: {error}"),
    }
    app.insert_resource(menus);
    match load_mp_sound_bank(&ui_games, "iw4:code_post_gfx_mp") {
        Ok(loaded) => {
            diag::info!(Launch, "menu: sound bank ready");
            for line in loaded.gap_lines() {
                diag::warn!(Launch, "menu: {line}");
            }
            app.insert_resource(SoundBank(std::sync::Arc::new(loaded.catalog)));
        }
        Err(error) => diag::warn!(Launch, "menu: sound bank: {error}"),
    }

    let (menu_iwd, lines) = NamespaceSoundIwd::open(&namespace_trees);
    for line in lines {
        diag::info!(Launch, "menu: {line}");
    }
    app.insert_resource(SoundIwd(std::sync::Arc::new(menu_iwd)));
    app.insert_resource(launch_identity(&config))
        .insert_resource(MenuMapList(maps))
        .insert_resource(MenuEnabled(true))
        .insert_resource(MenuFrontend {
            game_mode: Some("mp".into()),
        })
        .insert_resource(AppScreen::MainMenu)
        .insert_resource(UiAssetRoot(Some(ui_games.0)))
        .insert_resource(StartupCommands {
            lines: console::startup_commands(),
        })
        .insert_resource(WorldScene::default())
        .insert_resource(ClearColor(Color::srgb(0.04, 0.045, 0.06)))
        .insert_resource({
            let mut layers = UiLayers::default();
            layers.show_only([UiLayer::Shell, UiLayer::Overlay]);
            layers
        });
    add_runtime_plugins(&mut app);
    if let Some(decoded) = menu_bg {
        app.insert_resource(PendingMenuBgPixels(decoded));
    }
    if let Some(plan) = MenuShotPlan::from_env() {
        diag::info!(Launch, "menu-shots: {}", plan.dir.display());
        app.insert_resource(plan);
    } else if let Some(capture) = CaptureRequest::from_env() {
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

fn run_play(
    games: assets::GamesRoot,
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
    run_map(games, artifacts, zone, None, Role::Replay, Some(session));
}

fn run_map(
    games: assets::GamesRoot,
    artifacts: PathBuf,
    zone_arg: String,
    acceptance: Option<AcceptanceLaunch>,
    role: Role,
    playback: Option<ReplayPlayback>,
) {
    let found = find_zone_file(&games, &zone_arg);
    let zone_alias = found.as_ref().ok().and_then(|z| z.alias_note.clone());
    let zone_arg_lc = zone_arg.trim().to_ascii_lowercase();
    let (parsed_game, requested_stem) = assets::split_zone_key(&zone_arg_lc);
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
            .and_then(|z| assets::zone_game_for_path(&z.path))
    });
    let loading_title = assets::map_load_title(&zone_arg_lc, game);
    let namespace_trees = NamespaceTrees::discover(&games);
    let have = content_flags(&namespace_trees);
    let requires = net::content_required_by_map(&zone_arg_lc)
        .unwrap_or_else(|error| fatal(&format!("master content: {error}")));

    let master_intent = if role == Role::Replay {
        net::MasterLaunchIntent::disabled()
    } else if have.0 & net::CONTENT_IW4 == 0 {
        diag::warn!(
            Net,
            "master launch disabled: IW4 common_mp.ff is not installed"
        );
        net::MasterLaunchIntent::disabled()
    } else {
        net::MasterLaunchIntent::from_env_for_map(&zone, have, requires).unwrap_or_else(|error| {
            diag::warn!(Net, "master launch disabled: {error}");
            net::MasterLaunchIntent::disabled()
        })
    };

    let udp_connect = matches!(
        net::UdpLaunchIntent::from_env(),
        net::UdpLaunchIntent::Connect(_)
    );
    let config = LaunchConfig {
        role: if master_intent.is_join() || (role == Role::Listen && udp_connect) {
            Role::Client
        } else {
            role
        },
        zone: zone.clone(),
        games_root: games.0.clone(),
        artifacts,
    };
    start_perf(
        Some(zone.clone()),
        match config.role {
            Role::Listen => "listen",
            Role::Client => "client",
            Role::Replay => "replay",
        },
    );
    let (menus, menu_report) = load_ui_menu_catalog(&games);
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
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            primary_window: Some(Window {
                title: match config.role {
                    Role::Replay => format!("iw4l — play {}", config.zone),
                    Role::Listen => format!("iw4l — {}", config.zone),
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
    let ui_games_root = assets::ui_games_root(&games).ok().map(|root| root.0);
    app.insert_resource(launch_identity(&config))
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
            progress,
            loading_title,
            sim::host_game_mode_kind().display_name().to_owned(),
        ))
        .insert_resource(MenuEnabled(false))
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
        Role::Replay => add_runtime_plugins_with_role(&mut app, net::RuntimeRole::Replay),
        Role::Listen => add_runtime_plugins(&mut app),
        Role::Client => add_runtime_plugins_with_role(&mut app, net::RuntimeRole::Client),
    }
    if let Some(playback) = playback {
        app.insert_resource(playback);
    }
    if let Some(capture) = CaptureRequest::from_env() {
        queue_launch_capture(&mut app, capture);
    }
    let bench = bench_load::enabled();
    if bench {
        bench_load::insert(&mut app, &zone, &config.artifacts);
    }
    app.run();
    let trace = flush_perf();
    if bench {
        bench_load::print_summary(trace);
    }
}

fn content_flags(trees: &NamespaceTrees) -> net::ContentFlags {
    net::content_inventory(
        trees.get(assets::AssetNamespace::Iw4).is_some(),
        trees.get(assets::AssetNamespace::Iw5).is_some(),
        trees.get(assets::AssetNamespace::T5).is_some(),
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
