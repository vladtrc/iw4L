use std::path::PathBuf;

use asset_core::AssetNamespace;
use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::{AppScreen, GameSettings, OtherGame, UiExecCommand, UiMenuRequest};

use crate::layers::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Game {
    ModernWarfare,
    ModernWarfare2,
    BlackOps,
    BlackOps2,
}

impl Game {
    const ALL: [Self; 4] = [
        Self::ModernWarfare,
        Self::ModernWarfare2,
        Self::BlackOps,
        Self::BlackOps2,
    ];

    fn folder(self) -> OtherGame {
        match self {
            Self::ModernWarfare => OtherGame::ModernWarfare,
            Self::ModernWarfare2 => OtherGame::ModernWarfare2,
            Self::BlackOps => OtherGame::BlackOps,
            Self::BlackOps2 => OtherGame::BlackOps2,
        }
    }

    fn namespace(self) -> Option<AssetNamespace> {
        match self {
            Self::ModernWarfare => None,
            Self::ModernWarfare2 => Some(AssetNamespace::Iw4),
            Self::BlackOps => Some(AssetNamespace::T5),
            Self::BlackOps2 => Some(AssetNamespace::T6),
        }
    }

    fn title(self) -> &'static str {
        self.folder().title()
    }

    fn zombies(self) -> Option<&'static game_api::LibraryMode> {
        session::games::modes(self.namespace()?).zombies()
    }

    fn accent(self) -> Color {
        match self {
            Self::ModernWarfare | Self::ModernWarfare2 => Color::srgb(0.68, 0.82, 0.43),
            Self::BlackOps => Color::srgb(0.83, 0.78, 0.64),
            Self::BlackOps2 => Color::srgb(1.0, 0.65, 0.30),
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "mw" | "modern_warfare" => Some(Self::ModernWarfare),
            "mw2" | "iw4" | "modern_warfare_2" => Some(Self::ModernWarfare2),
            "bo" | "t5" | "black_ops" => Some(Self::BlackOps),
            "bo2" | "t6" | "black_ops_2" => Some(Self::BlackOps2),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Library,
    Game,
    Multiplayer,
    Zombies,
    Host,
    Lobby,
    Browser,
    Communities,
    Installations,
    Settings,
    WorkInProgress,
    Password,
}

#[derive(Resource, Default)]
struct Menu {
    page: Page,
    selected: Option<Game>,
    map_page: usize,
    browser_page: usize,
    settings_tab: usize,
    focus: usize,
    editing_name: bool,
    notice: String,
    wip_mode: String,
    editing_lobby: bool,
    password_setup: bool,
}

impl Menu {
    fn navigate(&mut self, page: Page) {
        self.page = page;
        self.focus = 0;
        self.editing_name = false;
        self.notice.clear();
    }

    fn back(&mut self) {
        if self.page == Page::Host && self.editing_lobby {
            self.editing_lobby = false;
            self.navigate(Page::Lobby);
            return;
        }
        self.navigate(match self.page {
            Page::Game => Page::Library,
            Page::Multiplayer | Page::Zombies | Page::WorkInProgress => Page::Game,
            Page::Host | Page::Browser | Page::Communities => Page::Multiplayer,
            Page::Lobby => Page::Multiplayer,
            Page::Password => {
                if self.password_setup {
                    Page::Lobby
                } else {
                    Page::Browser
                }
            }
            Page::Installations | Page::Settings => {
                if self.selected.is_some() {
                    Page::Game
                } else {
                    Page::Library
                }
            }
            Page::Library => Page::Library,
        });
    }
}

#[derive(Default)]
struct Installation {
    game: Option<Game>,
    root: Option<PathBuf>,
    maps: Vec<String>,
    /// The game's zombie maps whose zones are installed, `ns:zone`.
    zombie_maps: Vec<String>,
}

#[derive(Resource, Default)]
struct Inventory {
    games: Vec<Installation>,
    folders: Option<[String; 5]>,
    task: Option<Task<(Vec<Installation>, Vec<asset_transport::MapPack>)>>,
    generation: u64,
}

impl Inventory {
    fn installation(&self, game: Game) -> Option<&Installation> {
        self.games.iter().find(|entry| entry.game == Some(game))
    }

    fn maps(&self, game: Option<Game>) -> &[String] {
        game.and_then(|game| self.installation(game))
            .map_or(&[], |entry| &entry.maps)
    }
}

#[derive(Resource)]
struct Backdrop {
    fallback: Handle<Image>,
    native: Option<Handle<Image>>,
    key: Option<(PathBuf, bool)>,
    task: Option<Task<Result<assets::T6UiArt, String>>>,
    revision: u64,
}

#[derive(Component)]
struct Root;

#[derive(Component)]
struct ScrollContent;

#[derive(Clone, PartialEq, Eq)]
enum Action {
    Page(Page),
    Select(Game),
    ZombieMap(String),
    Back,
    Home,
    Wip(&'static str),
    Pick(Game),
    Scan,
    Rescan,
    Setting(&'static str),
    SettingsTab(usize),
    Bind(&'static str),
    Map(String),
    MapPage(bool),
    Mode,
    ScoreLimit,
    TimeLimit,
    Create,
    Start,
    Privacy,
    Password,
    PasswordSubmit,
    PasswordCancel,
    Leave,
    Refresh,
    BrowserPage(bool),
    Join(String),
    Community(String),
    ApplyCommunity,
    Quit,
}

#[derive(Component)]
struct MenuButton {
    action: Action,
    order: usize,
    enabled: bool,
    accent: Color,
}

pub(crate) fn register(app: &mut App) {
    app.insert_resource(frame::UnifiedFrontend(true))
        .init_resource::<Menu>()
        .init_resource::<Inventory>()
        .add_systems(Startup, load_backdrop)
        .add_systems(
            Update,
            (
                discover,
                input,
                load_native_art,
                rebuild,
                skin_buttons,
                scroll,
            )
                .chain()
                .in_set(frame::ClientSet::Ui)
                .in_set(super::t6_text::NativeUiPaint),
        );
}

fn load_backdrop(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let image = Image::from_buffer(
        include_bytes!("../assets/launcher-background.png"),
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        bevy::asset::RenderAssetUsages::all(),
    )
    .expect("bundled launcher background must decode");
    commands.insert_resource(Backdrop {
        fallback: images.add(image),
        native: None,
        key: None,
        task: None,
        revision: 0,
    });
}

fn load_native_art(
    mut commands: Commands,
    screen: Res<AppScreen>,
    menu: Res<Menu>,
    inventory: Res<Inventory>,
    dvars: Res<frame::UiMenuDvars>,
    backdrop: Option<ResMut<Backdrop>>,
    mut art: ResMut<crate::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
) {
    if *screen != AppScreen::MainMenu {
        return;
    }
    let Some(mut backdrop) = backdrop else {
        return;
    };
    let key = menu
        .selected
        .filter(|game| *game == Game::BlackOps2)
        .and_then(|game| inventory.installation(game)?.root.clone())
        .map(|root| {
            (
                root,
                menu.page == Page::Zombies
                    || (menu.page == Page::Lobby && dvars.get("ui_gametype") == Some("zclassic")),
            )
        });
    if backdrop.key != key {
        backdrop.native = None;
        backdrop.revision = backdrop.revision.wrapping_add(1);
        backdrop.task = key.as_ref().map(|(root, zombies)| {
            let root = root.clone();
            let zombies = *zombies;
            assets::load_pool().spawn(async move { assets::load_t6_ui_art(&root, zombies) })
        });
        backdrop.key = key;
    }
    let Some(task) = backdrop.task.as_mut() else {
        return;
    };
    let Some(result) = future::block_on(future::poll_once(task)) else {
        return;
    };
    backdrop.task = None;
    match result {
        Ok(prepared) => {
            let publication = prepared.publish();
            commands.insert_resource(publication.clone());
            art.adopt(publication);
            let zombies = backdrop.key.as_ref().is_some_and(|(_, zombies)| *zombies);
            backdrop.native = art.image(
                if zombies {
                    "menu_zm_background_main"
                } else {
                    "menu_mp_background_main"
                },
                &mut images,
            );
            backdrop.revision = backdrop.revision.wrapping_add(1);
        }
        Err(error) => diag::warn!(Zone, "T6 frontend artwork unavailable: {error}"),
    }
}

fn skin_buttons(
    mut commands: Commands,
    menu: Res<Menu>,
    backdrop: Option<Res<Backdrop>>,
    mut art: ResMut<crate::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
    mut buttons: Query<(Entity, &MenuButton, &Interaction, Option<&mut ImageNode>)>,
) {
    if menu.selected != Some(Game::BlackOps2)
        || !backdrop.is_some_and(|backdrop| backdrop.native.is_some())
    {
        return;
    }
    let normal = art.image("menu_button_backing", &mut images);
    let selected = art.image("menu_button_backing_highlight", &mut images);
    for (entity, button, interaction, previous) in &mut buttons {
        let focused =
            button.enabled && (button.order == menu.focus || *interaction != Interaction::None);
        let Some(image) = (if focused {
            selected.as_ref().or(normal.as_ref())
        } else {
            normal.as_ref()
        }) else {
            continue;
        };
        let tint = if focused {
            Color::srgba(0.8, 0.3, 0.04, 0.8)
        } else {
            Color::srgba(0.08, 0.08, 0.08, 0.7)
        };
        if let Some(mut previous) = previous {
            if previous.image != *image {
                previous.image = image.clone();
            }
            if previous.color != tint {
                previous.color = tint;
            }
        } else {
            commands.entity(entity).insert(ImageNode {
                image: image.clone(),
                color: tint,
                image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                ..default()
            });
        }
    }
}

fn discover(
    screen: Res<AppScreen>,
    settings: Res<GameSettings>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut inventory: ResMut<Inventory>,
    mut maps: ResMut<crate::MenuMapList>,
) {
    if *screen != AppScreen::MainMenu {
        return;
    }
    if inventory.task.is_some() {
        let task = inventory.task.as_mut().unwrap();
        if let Some((games, packs)) = future::block_on(future::poll_once(task)) {
            inventory.games = games;
            inventory.task = None;
            inventory.generation = inventory.generation.wrapping_add(1);
            maps.0 = packs;
        }
        return;
    }
    if inventory.folders.as_ref() == Some(&settings.game_folders) {
        return;
    }
    let Some(identity) = identity else { return };
    let root = asset_transport::GamesRoot(identity.games_root.clone());
    let folders = settings.game_folders.clone();
    asset_transport::set_game_folders(
        folders
            .iter()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect(),
    );
    inventory.folders = Some(folders);
    inventory.task = Some(assets::load_pool().spawn(async move {
        let mut packs = asset_transport::list_mp_map_packs(&root);
        let maps: Vec<_> = packs.iter().flat_map(|pack| &pack.maps).cloned().collect();
        let games = Game::ALL
            .into_iter()
            .map(|game| {
                let installed = game
                    .namespace()
                    .and_then(|namespace| {
                        let kind = match namespace {
                            AssetNamespace::Iw4 => asset_transport::ZoneGame::Iw4,
                            AssetNamespace::Iw5 => asset_transport::ZoneGame::Iw5,
                            AssetNamespace::T5 => asset_transport::ZoneGame::T5,
                            AssetNamespace::T6 => asset_transport::ZoneGame::T6,
                        };
                        asset_transport::find_game_install(&root.0, kind)
                    })
                    .or_else(|| {
                        (game == Game::ModernWarfare)
                            .then(|| asset_transport::find_modern_warfare_install(&root.0))
                            .flatten()
                    });
                let game_maps = game
                    .namespace()
                    .map(|ns| {
                        maps.iter()
                            .filter(|map| map.starts_with(&format!("{}:", ns.as_str())))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let zombie_maps: Vec<String> = match (game.namespace(), game.zombies()) {
                    (Some(namespace), Some(mode)) if installed.is_some() => mode
                        .maps
                        .iter()
                        .map(|(zone, _)| format!("{}:{zone}", namespace.as_str()))
                        .filter(|map| asset_transport::find_zone_file(&root, map).is_ok())
                        .collect(),
                    _ => Vec::new(),
                };
                if let (Some(namespace), false) = (game.namespace(), zombie_maps.is_empty()) {
                    packs.push(asset_transport::MapPack {
                        label: format!("{} Zombies", namespace.as_str().to_uppercase()),
                        maps: zombie_maps.clone(),
                    });
                }
                Installation {
                    game: Some(game),
                    root: installed,
                    maps: game_maps,
                    zombie_maps,
                }
            })
            .collect();
        (games, packs)
    }));
}

fn command(exec: &mut MessageWriter<UiExecCommand>, text: impl Into<String>) {
    exec.write(UiExecCommand { text: text.into() });
}

fn setting(key: &str, settings: &mut GameSettings, menu: &mut Menu) {
    match key {
        "resolution" => {
            let options = [
                (960, 540),
                (1280, 720),
                (1600, 900),
                (1920, 1080),
                (2560, 1440),
                (3840, 2160),
            ];
            let index = options
                .iter()
                .position(|&(w, h)| {
                    settings.resolution.width == w && settings.resolution.height == h
                })
                .unwrap_or(0);
            let (w, h) = options[(index + 1) % options.len()];
            settings.resolution = frame::DisplayResolution::new(w, h);
        }
        "fullscreen" => settings.fullscreen = !settings.fullscreen,
        "vsync" => settings.vsync = !settings.vsync,
        "fov" => {
            settings.fov = if settings.fov >= 110.0 {
                65.0
            } else {
                settings.fov + 5.0
            }
        }
        "brightness" => {
            settings.brightness = if settings.brightness >= 0.19 {
                -0.2
            } else {
                settings.brightness + 0.05
            }
        }
        "shadows" => settings.shadows = !settings.shadows,
        "bloom" => settings.bloom = !settings.bloom,
        "dof" => settings.depth_of_field = !settings.depth_of_field,
        "volume" => {
            settings.master_volume = if settings.master_volume >= 0.99 {
                0.0
            } else {
                settings.master_volume + 0.1
            }
        }
        "sensitivity" => {
            settings.sensitivity = if settings.sensitivity >= 15.0 {
                0.5
            } else {
                settings.sensitivity + 0.5
            }
        }
        "invert" => settings.invert_mouse = !settings.invert_mouse,
        "name" => {
            menu.editing_name = !menu.editing_name;
            return;
        }
        "vibration" => settings.pad_vibration = !settings.pad_vibration,
        "reset" => {
            let folders = settings.game_folders.clone();
            let name = settings.player_name.clone();
            *settings = GameSettings::default();
            settings.game_folders = folders;
            settings.player_name = name;
        }
        _ => return,
    }
    settings.sanitize();
    settings.touch();
}

fn activate(
    action: Action,
    menu: &mut Menu,
    settings: &mut GameSettings,
    inventory: &mut Inventory,
    dvars: &mut frame::UiMenuDvars,
    exec: &mut MessageWriter<UiExecCommand>,
    binds: &mut MessageWriter<frame::UiBindRequest>,
    browser: Option<&net::MasterBrowser>,
) {
    match action {
        Action::Page(page) => {
            menu.editing_lobby =
                matches!(page, Page::Host | Page::Zombies) && menu.page == Page::Lobby;
            menu.navigate(page);
            if page == Page::Multiplayer {
                dvars.set("ui_gametype", "dm");
                if let Some(map) = inventory.maps(menu.selected).first() {
                    dvars.set("ui_mapname", map.clone());
                }
            }
            if page == Page::Browser {
                if let Some(browser) = browser {
                    browser.refresh();
                }
            }
        }
        Action::ZombieMap(map) => {
            let changing_lobby = menu.editing_lobby;
            if changing_lobby {
                command(exec, format!("ui_select_map {map}"));
            }
            let Some((namespace, mode)) = map
                .split_once(':')
                .and_then(|(prefix, _)| asset_core::AssetNamespace::from_prefix(prefix))
                .and_then(|namespace| {
                    Some((namespace, session::games::modes(namespace).zombies()?))
                })
            else {
                return;
            };
            dvars.set("ui_mapname", map);
            dvars.set("ui_gametype", mode.gametype);
            dvars.set("ui_game_namespace", namespace.as_str());
            dvars.set("ui_scorelimit", "0");
            dvars.set("ui_timelimit", "0");
            if !changing_lobby {
                command(exec, "ui_create_lobby");
            }
            menu.navigate(Page::Lobby);
        }
        Action::Select(game) => {
            menu.selected = Some(game);
            menu.map_page = 0;
            menu.navigate(Page::Game);
            if let Some(map) = inventory.maps(Some(game)).first() {
                dvars.set("ui_mapname", map.clone());
            }
            dvars.set("ui_gametype", "dm");
            dvars.set(
                "ui_game_namespace",
                game.namespace()
                    .map_or("iw3", |namespace| namespace.as_str()),
            );
            dvars.set("ui_scorelimit", "30");
            dvars.set("ui_timelimit", "10");
            dvars.set("scr_dm_scorelimit", "30");
            dvars.set("scr_dm_timelimit", "10");
            dvars.set("ui_map_error", "0");
            dvars.set("ui_frontend_status", "");
        }
        Action::Back => menu.back(),
        Action::Home => {
            menu.selected = None;
            menu.navigate(Page::Library);
        }
        Action::Wip(mode) => {
            menu.navigate(Page::WorkInProgress);
            menu.wip_mode = mode.into();
        }
        Action::Pick(game) => command(
            exec,
            format!("set ui_pick_game_folder {}", game.folder().key()),
        ),
        Action::Scan => command(exec, "set ui_scan_game_folders 1"),
        Action::Rescan => {
            inventory.folders = None;
            menu.notice = "Scanning installed games…".into();
        }
        Action::Setting(key) => setting(key, settings, menu),
        Action::SettingsTab(tab) => {
            menu.settings_tab = tab;
            menu.focus = 0;
            menu.editing_name = false;
        }
        Action::Bind(key) => {
            binds.write(frame::UiBindRequest {
                command: key.into(),
            });
        }
        Action::Map(map) => {
            if menu.editing_lobby {
                command(exec, format!("ui_lobby_map {map}"));
            } else {
                dvars.set("ui_mapname", map);
            }
        }
        Action::MapPage(next) => {
            let pages = inventory.maps(menu.selected).len().div_ceil(6).max(1);
            menu.map_page = if next {
                (menu.map_page + 1) % pages
            } else {
                (menu.map_page + pages - 1) % pages
            };
        }
        Action::Mode => {
            let modes: &[&str] = if menu.selected == Some(Game::BlackOps2) {
                &["dm", "war"]
            } else {
                &["dm", "war", "dom", "sd", "koth", "sab"]
            };
            let current = dvars.get("ui_gametype").unwrap_or("dm");
            let index = modes.iter().position(|&mode| mode == current).unwrap_or(0);
            let next = modes[(index + 1) % modes.len()];
            if menu.editing_lobby {
                command(exec, format!("ui_select_mode {next}"));
            } else {
                dvars.set("ui_gametype", next);
            }
            for key in ["scorelimit", "timelimit"] {
                let value = dvars
                    .get(&format!("ui_{key}"))
                    .unwrap_or(if key == "timelimit" { "10" } else { "30" })
                    .to_owned();
                dvars.set(
                    &format!("scr_{}_{key}", modes[(index + 1) % modes.len()]),
                    value,
                );
            }
        }
        Action::ScoreLimit => {
            let values = [3, 10, 20, 30, 50, 75, 100];
            let value = dvars
                .get("ui_scorelimit")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(30);
            let index = values.iter().position(|&v| v == value).unwrap_or(0);
            dvars.set(
                "ui_scorelimit",
                values[(index + 1) % values.len()].to_string(),
            );
            let key = format!(
                "scr_{}_scorelimit",
                dvars.get("ui_gametype").unwrap_or("dm")
            );
            dvars.set(&key, values[(index + 1) % values.len()].to_string());
        }
        Action::TimeLimit => {
            let values = [0, 5, 10, 15, 20, 30];
            let value = dvars
                .get("ui_timelimit")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(10);
            let index = values.iter().position(|&v| v == value).unwrap_or(0);
            dvars.set(
                "ui_timelimit",
                values[(index + 1) % values.len()].to_string(),
            );
            let key = format!("scr_{}_timelimit", dvars.get("ui_gametype").unwrap_or("dm"));
            dvars.set(&key, values[(index + 1) % values.len()].to_string());
        }
        Action::Create => {
            command(exec, "ui_create_lobby");
            menu.navigate(Page::Lobby);
        }
        Action::Start => {
            command(exec, "ui_start_match");
            menu.notice = "Starting match…".into();
        }
        Action::Privacy => command(exec, "ui_lobby_privacy"),
        Action::Password => {
            menu.password_setup = true;
            menu.navigate(Page::Password);
            command(exec, "ui_password_open");
        }
        Action::PasswordSubmit => {
            command(
                exec,
                if menu.password_setup {
                    "ui_password_save"
                } else {
                    "ui_password_join"
                },
            );
        }
        Action::PasswordCancel => {
            command(exec, "ui_password_cancel");
            menu.back();
        }
        Action::Leave => {
            command(exec, "ui_leave_lobby");
            menu.navigate(Page::Multiplayer);
        }
        Action::Refresh => {
            if let Some(browser) = browser {
                browser.refresh();
            }
        }
        Action::BrowserPage(next) => {
            let count = browser
                .map(|browser| {
                    browser
                        .snapshot()
                        .adverts
                        .iter()
                        .filter(|advert| {
                            menu.selected.and_then(Game::namespace).is_some_and(|ns| {
                                advert.map.starts_with(&format!("{}:", ns.as_str()))
                            })
                        })
                        .count()
                })
                .unwrap_or(0)
                .div_ceil(6)
                .max(1);
            menu.browser_page = if next {
                (menu.browser_page + 1) % count
            } else {
                (menu.browser_page + count - 1) % count
            };
        }
        Action::Join(id) => {
            command(exec, format!("ui_join_lobby_id {id}"));
            menu.navigate(Page::Lobby);
        }
        Action::Community(choice) => command(exec, format!("set ui_community_server {choice}")),
        Action::ApplyCommunity => command(exec, "set ui_community_apply 1"),
        Action::Quit => command(exec, "quit"),
    }
}

fn input(
    screen: Res<AppScreen>,
    keys: Res<ButtonInput<KeyCode>>,
    hud_input: Option<Res<frame::HudInputView>>,
    capture: Res<frame::UiBindingCapture>,
    mut keyboard: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut requests: MessageReader<UiMenuRequest>,
    mut menu: ResMut<Menu>,
    mut inventory: ResMut<Inventory>,
    mut settings: ResMut<GameSettings>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut exec: MessageWriter<UiExecCommand>,
    mut binds: MessageWriter<frame::UiBindRequest>,
    browser: Option<Res<net::MasterBrowser>>,
    mut buttons: Query<(
        Entity,
        &Interaction,
        &MenuButton,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let typing: Vec<_> = keyboard.read().cloned().collect();
    let requests: Vec<_> = requests.read().cloned().collect();
    let menu_keys: Vec<_> = requests
        .iter()
        .filter_map(|request| match request {
            UiMenuRequest::Key(key) => Some(*key),
            _ => None,
        })
        .collect();
    if *screen != AppScreen::MainMenu {
        if !menu.notice.is_empty() {
            menu.notice.clear();
        }
        return;
    }
    let mut actions = Vec::new();
    for request in requests {
        if let UiMenuRequest::Close(name) = &request
            && name == "game_lobby"
            && matches!(menu.page, Page::Lobby | Page::Host)
        {
            menu.editing_lobby = false;
            menu.navigate(Page::Multiplayer);
        }
        if let UiMenuRequest::Open(name) = &request
            && name == "lobby_password_join"
        {
            menu.password_setup = false;
            menu.navigate(Page::Password);
        }
        if let UiMenuRequest::Close(name) = &request
            && name == "lobby_password_setup"
            && menu.page == Page::Password
        {
            menu.navigate(Page::Lobby);
        }
        if let UiMenuRequest::Open(name) = request {
            if name == "game_lobby" {
                let game = dvars
                    .get("ui_mapname")
                    .and_then(|map| map.split_once(':'))
                    .and_then(|(namespace, _)| {
                        Some((Game::from_key(namespace)?, namespace.to_owned()))
                    });
                if let Some((game, namespace)) = game {
                    menu.selected = Some(game);
                    dvars.set("ui_game_namespace", namespace);
                }
                actions.push(Action::Page(Page::Lobby));
                continue;
            }
            let mut parts = name.split('/');
            if parts.next() != Some("launcher") {
                continue;
            }
            let action = match parts.next() {
                None | Some("home") => Some(Action::Home),
                Some("game") => parts.next().and_then(Game::from_key).map(Action::Select),
                Some("settings") => Some(Action::Page(Page::Settings)),
                Some("installations") => Some(Action::Page(Page::Installations)),
                Some("multiplayer") => Some(Action::Page(Page::Multiplayer)),
                Some("host") => Some(Action::Page(Page::Host)),
                Some("browser") => Some(Action::Page(Page::Browser)),
                Some("campaign") => Some(Action::Wip("Campaign")),
                Some("zombies") => Some(if menu.selected == Some(Game::BlackOps2) {
                    Action::Page(Page::Zombies)
                } else {
                    Action::Wip("Zombies")
                }),
                Some("setting") => match parts.next() {
                    Some("vsync") => Some(Action::Setting("vsync")),
                    Some("fullscreen") => Some(Action::Setting("fullscreen")),
                    Some("volume") => Some(Action::Setting("volume")),
                    Some("resolution") => Some(Action::Setting("resolution")),
                    _ => None,
                },
                _ => None,
            };
            if let Some(action) = action {
                actions.push(action);
            }
        }
    }
    let available: Vec<_> = buttons
        .iter()
        .filter(|(_, _, button, _, _)| button.enabled)
        .map(|(_, _, button, _, _)| (button.order, button.action.clone()))
        .collect();
    let mut available = available;
    available.sort_by_key(|(order, _)| *order);
    let console_open = hud_input.as_ref().is_some_and(|input| input.console_open);
    if !console_open && capture.command.is_none() && !capture.consumed_input {
        if menu.page == Page::Password && dvars.get("ui_password_pending") != Some("1") {
            let mut password = dvars.get("ui_password_input").unwrap_or("").to_owned();
            for event in typing.iter().filter(|event| event.state.is_pressed()) {
                if let bevy::input::keyboard::Key::Character(text) = &event.logical_key {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if password.len() + ch.len_utf8() <= 64 {
                            password.push(ch);
                        }
                    }
                }
            }
            if keys.just_pressed(KeyCode::Backspace)
                || menu_keys.contains(&frame::UiMenuKey::Backspace)
            {
                password.pop();
            }
            dvars.set("ui_password_input", password);
        }
        if menu.editing_name {
            for event in typing.iter().filter(|event| event.state.is_pressed()) {
                if let bevy::input::keyboard::Key::Character(text) = &event.logical_key {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if settings.player_name.chars().count() < 24 {
                            settings.player_name.push(ch);
                        }
                    }
                    settings.touch();
                }
            }
            if keys.just_pressed(KeyCode::Backspace)
                || menu_keys.contains(&frame::UiMenuKey::Backspace)
            {
                settings.player_name.pop();
                settings.touch();
            }
            if keys.just_pressed(KeyCode::Enter)
                || keys.just_pressed(KeyCode::Escape)
                || menu_keys.contains(&frame::UiMenuKey::Enter)
                || menu_keys.contains(&frame::UiMenuKey::Escape)
            {
                if settings.player_name.trim().is_empty() {
                    settings.player_name = "Player".into();
                }
                menu.editing_name = false;
                settings.sanitize();
                settings.touch();
            }
        } else {
            if keys.just_pressed(KeyCode::Escape) || menu_keys.contains(&frame::UiMenuKey::Escape) {
                actions.push(if menu.page == Page::Password {
                    Action::PasswordCancel
                } else if menu.page == Page::Lobby {
                    Action::Leave
                } else {
                    Action::Back
                });
            }
            let direction = if keys.just_pressed(KeyCode::ArrowDown)
                || keys.just_pressed(KeyCode::Tab)
                || menu_keys.contains(&frame::UiMenuKey::Down)
            {
                1
            } else if keys.just_pressed(KeyCode::ArrowUp)
                || menu_keys.contains(&frame::UiMenuKey::Up)
            {
                -1
            } else {
                0
            };
            if direction != 0 && !available.is_empty() {
                let current = available
                    .iter()
                    .position(|(order, _)| *order == menu.focus)
                    .unwrap_or(0);
                let index =
                    (current as isize + direction).rem_euclid(available.len() as isize) as usize;
                menu.focus = available[index].0;
            }
            if (keys.just_pressed(KeyCode::Enter) || menu_keys.contains(&frame::UiMenuKey::Enter))
                && let Some((_, action)) = available.iter().find(|(order, _)| *order == menu.focus)
            {
                actions.push(action.clone());
            }
        }
        for (_, interaction, button, _, _) in &buttons {
            if button.enabled
                && *interaction == Interaction::Pressed
                && mouse.just_pressed(MouseButton::Left)
            {
                actions.push(button.action.clone());
            }
        }
    }
    actions.dedup();
    for action in actions {
        activate(
            action,
            &mut menu,
            &mut settings,
            &mut inventory,
            &mut dvars,
            &mut exec,
            &mut binds,
            browser.as_deref(),
        );
    }
    for (_, interaction, button, mut background, mut border) in &mut buttons {
        let focused =
            button.enabled && (*interaction != Interaction::None || button.order == menu.focus);
        let color = if focused {
            Color::srgba(0.23, 0.26, 0.23, 0.85)
        } else {
            Color::srgba(0.035, 0.045, 0.05, 0.72)
        };
        if background.0 != color {
            background.0 = color;
        }
        let color = if focused {
            button.accent
        } else {
            Color::srgba(0.4, 0.44, 0.45, 0.25)
        };
        if border.left != color {
            *border = BorderColor::all(color);
        }
    }
}

fn label(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    value: impl Into<String>,
    size: f32,
    color: Color,
) {
    parent.spawn((
        Text::new(value.into()),
        game_text_font(font, size),
        TextColor(color),
        Node {
            margin: UiRect::bottom(Val::Px(8.0)),
            ..default()
        },
    ));
}

fn button(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    order: &mut usize,
    text: impl Into<String>,
    action: Action,
    enabled: bool,
    accent: Color,
) {
    let index = *order;
    *order += 1;
    parent
        .spawn((
            Button,
            MenuButton {
                action,
                order: index,
                enabled,
                accent,
            },
            BackgroundColor(Color::srgba(0.035, 0.045, 0.05, 0.72)),
            BorderColor::all(Color::srgba(0.4, 0.44, 0.45, 0.25)),
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(40.0),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                margin: UiRect::bottom(Val::Px(5.0)),
                border: UiRect::left(Val::Px(3.0)),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(text.into()),
                game_text_font(font, 17.0),
                TextColor(if enabled {
                    Color::srgb(0.93, 0.94, 0.92)
                } else {
                    Color::srgb(0.43, 0.46, 0.47)
                }),
            ));
        });
}

fn on(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

fn scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    screen: Res<AppScreen>,
    menu: Res<Menu>,
    mut nodes: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<ScrollContent>>,
    buttons: Query<(&ComputedNode, &UiGlobalTransform, &MenuButton, &ChildOf)>,
    parents: Query<&ChildOf>,
    mut last_focus: Local<usize>,
) {
    let movement: f32 = wheel
        .read()
        .map(|event| match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * 40.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        })
        .sum();
    if *screen != AppScreen::MainMenu {
        return;
    }
    for (node, _, mut position) in &mut nodes {
        let limit = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
        position.0.y = (position.0.y - movement).clamp(0.0, limit);
    }
    if *last_focus == menu.focus {
        return;
    }
    let Some((button_node, button_transform, _, parent)) = buttons
        .iter()
        .find(|(_, _, button, _)| button.order == menu.focus)
    else {
        return;
    };
    if button_node.size().y == 0.0 {
        return;
    }
    *last_focus = menu.focus;
    let mut ancestor = parent.parent();
    loop {
        if let Ok((node, transform, mut position)) = nodes.get_mut(ancestor) {
            let top = transform.translation.y - node.size().y * 0.5;
            let bottom = top + node.size().y;
            let button_top = button_transform.translation.y - button_node.size().y * 0.5;
            let button_bottom = button_top + button_node.size().y;
            let delta = if button_top < top {
                button_top - top
            } else if button_bottom > bottom {
                button_bottom - bottom
            } else {
                0.0
            };
            let limit =
                (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
            position.0.y = (position.0.y + delta * node.inverse_scale_factor()).clamp(0.0, limit);
            break;
        }
        let Ok(parent) = parents.get(ancestor) else {
            break;
        };
        ancestor = parent.parent();
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    screen: Res<AppScreen>,
    menu: Res<Menu>,
    inventory: Res<Inventory>,
    settings: Res<GameSettings>,
    font: Option<Res<GameUiFont>>,
    backdrop: Option<Res<Backdrop>>,
    browser: Option<Res<net::MasterBrowser>>,
    party: Res<frame::UiPartyState>,
    communities: Res<crate::CommunityServers>,
    dvars: Res<frame::UiMenuDvars>,
    binding: Res<frame::UiBindingCapture>,
    roots: Query<Entity, With<Root>>,
    mut last_signature: Local<String>,
    positions: Query<&ScrollPosition, With<ScrollContent>>,
    mut last_page: Local<Option<Page>>,
) {
    if *screen != AppScreen::MainMenu {
        return;
    }
    let (Some(font), Some(backdrop)) = (font, backdrop) else {
        return;
    };
    let snapshot = browser.as_ref().map(|browser| browser.snapshot());
    let signature = format!(
        "{} {:?} {} {} {} {} {:?} {:?} {:?}",
        inventory.generation.wrapping_add(backdrop.revision),
        menu.page,
        snapshot.as_ref().map_or(0, |s| s.generation),
        dvars.get("ui_frontend_status").unwrap_or(""),
        dvars.get("ui_mapname").unwrap_or(""),
        dvars.get("ui_lobby_players").unwrap_or(""),
        binding.command,
        [
            "ui_gametype",
            "ui_scorelimit",
            "ui_timelimit",
            "ui_lobby_privacy",
            "ui_map_error",
            "ui_map_error_reason",
            "ui_folder_error",
            "ui_folder_scan",
            "ui_connection_error",
            "partyend_reason",
            "ui_community_server",
            "ui_community_apply_disabled",
            "ui_community_hint",
            "ui_password_status",
            "ui_password_pending",
            "ui_master_configured"
        ]
        .map(|key| dvars.get(key)),
        (
            menu.selected,
            menu.map_page,
            menu.browser_page,
            menu.settings_tab,
            menu.editing_name,
            &menu.notice,
            &menu.wip_mode,
            dvars.get("ui_password_input").unwrap_or("").chars().count(),
            menu.password_setup,
            menu.editing_lobby,
            (0..18)
                .map(|index| dvars.get(&format!("ui_lobby_member_{index}")).unwrap_or(""))
                .collect::<Vec<_>>()
        )
    );
    if !roots.is_empty()
        && !settings.is_changed()
        && !inventory.is_changed()
        && !party.is_changed()
        && *last_signature == signature
    {
        return;
    }
    *last_signature = signature;
    let scroll_position = if *last_page == Some(menu.page) {
        positions.iter().next().cloned().unwrap_or_default()
    } else {
        ScrollPosition::default()
    };
    *last_page = Some(menu.page);
    for root in &roots {
        commands.entity(root).despawn();
    }
    let game = menu.selected;
    let accent = game.map_or(Color::srgb(0.78, 0.86, 0.69), Game::accent);
    let installed = game
        .and_then(|game| inventory.installation(game))
        .is_some_and(|entry| entry.root.is_some());
    let runnable = installed && game.and_then(Game::namespace).is_some();
    let maps = inventory.maps(game);
    let map = dvars
        .get("ui_mapname")
        .unwrap_or_else(|| maps.first().map_or("No map selected", String::as_str));
    let mut order = 0;
    commands.spawn((Root, super::t6_text::NativeUiRoot(game == Some(Game::BlackOps2) && backdrop.native.is_some()), UiLayer::Shell, UiLayerVisibility, GlobalZIndex(80), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::srgb(0.025,0.03,0.035)))).with_children(|root| {
        root.spawn((ImageNode::new(backdrop.native.clone().unwrap_or_else(|| backdrop.fallback.clone())).with_mode(bevy::ui::widget::NodeImageMode::Stretch), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }));
        root.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.01,0.02,0.025,0.25))));
        root.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(4.5), right: Val::Percent(4.5), top: Val::Px(26.0), flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|bar| {
            label(bar, &font.0, "IW4L   /   GAME LIBRARY", 16.0, accent);
            label(bar, &font.0, format!("{}   •   PC", settings.player_name.to_uppercase()), 14.0, Color::srgb(0.67,0.71,0.73));
        });
        root.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(4.5), right: Val::Percent(4.5), top: Val::Px(78.0), bottom: Val::Px(76.0), flex_direction: FlexDirection::Column, ..default() }).with_children(|body| {
            let title = match menu.page {
                Page::Library => "CHOOSE YOUR GAME".into(),
                Page::Game => game.map_or("SELECT A GAME".into(), |game| game.title().to_uppercase()),
                Page::Multiplayer => "MULTIPLAYER".into(),
                Page::Zombies => "ZOMBIES".into(),
                Page::Host => "MATCH SETUP".into(),
                Page::Lobby => "GAME LOBBY".into(),
                Page::Password => "LOBBY PASSWORD".into(),
                Page::Browser => "FIND LOBBIES".into(),
                Page::Communities => "COMMUNITY SERVERS".into(),
                Page::Installations => "GAME INSTALLATIONS".into(),
                Page::Settings => "SETTINGS".into(),
                Page::WorkInProgress => menu.wip_mode.to_uppercase(),
            };
            label(body, &font.0, title, 46.0, Color::srgb(0.96,0.97,0.95));
            label(body, &font.0, match menu.page {
                Page::Library => "YOUR GAMES. ONE RUNTIME.",
                Page::Installations => "SELECT YOUR GAME INSTALLATION FOLDERS.",
                Page::Settings => "CHANGES APPLY IMMEDIATELY AND ARE SAVED AUTOMATICALLY.",
                _ => game.map_or("IW4L", Game::title),
            }, 13.0, accent);
            body.spawn(Node { flex_grow: 1.0, min_height: Val::Px(0.0), width: Val::Percent(100.0), margin: UiRect::top(Val::Px(20.0)), column_gap: Val::Px(32.0), ..default() }).with_children(|columns| {
                columns.spawn((ScrollContent, scroll_position, Node { width: Val::Percent(if matches!(menu.page, Page::Installations | Page::Settings | Page::Browser) { 68.0 } else { 47.0 }), max_width: Val::Px(780.0), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() })).with_children(|content| {
                    match menu.page {
                        Page::Library => {
                            let mut listed = 0;
                            for game in Game::ALL {
                                let Some(entry) = inventory.installation(game).filter(|entry| entry.root.is_some()) else {
                                    continue;
                                };
                                listed += 1;
                                let zombies = if entry.zombie_maps.is_empty() { String::new() } else { format!("  /  {} ZOMBIES MAPS", entry.zombie_maps.len()) };
                                button(content, &font.0, &mut order, format!("{}\nINSTALLED  /  {} MP MAPS{zombies}", game.title().to_uppercase(), entry.maps.len()), Action::Select(game), true, game.accent());
                            }
                            if inventory.task.is_some() {
                                label(content, &font.0, "Scanning for installed games…", 17.0, accent);
                            } else if listed == 0 {
                                label(content, &font.0, "No games found yet. Choose a folder that holds your Call of Duty installations.", 17.0, accent);
                                button(content, &font.0, &mut order, "SCAN A FOLDER FOR GAMES", Action::Scan, true, accent);
                            }
                            button(content, &font.0, &mut order, "GAME INSTALLATIONS", Action::Page(Page::Installations), true, accent);
                            button(content, &font.0, &mut order, "SETTINGS", Action::Page(Page::Settings), true, accent);
                        }
                        Page::Game => {
                            button(content, &font.0, &mut order, "MULTIPLAYER", Action::Page(Page::Multiplayer), true, accent);
                            button(content, &font.0, &mut order, "CAMPAIGN", Action::Wip("Campaign"), true, accent);
                            if game.and_then(Game::zombies).is_some() { button(content, &font.0, &mut order, "ZOMBIES", Action::Page(Page::Zombies), true, accent); }
                            button(content, &font.0, &mut order, "SETTINGS", Action::Page(Page::Settings), true, accent);
                            button(content, &font.0, &mut order, "GAME INSTALLATIONS", Action::Page(Page::Installations), true, accent);
                            button(content, &font.0, &mut order, "CHANGE GAME", Action::Home, true, accent);
                        }
                        Page::Multiplayer => {
                            button(content, &font.0, &mut order, "CREATE MATCH", Action::Page(Page::Host), runnable, accent);
                            button(content, &font.0, &mut order, "FIND LOBBIES", Action::Page(Page::Browser), runnable, accent);
                            button(content, &font.0, &mut order, "SETTINGS", Action::Page(Page::Settings), true, accent);
                            if !installed { label(content, &font.0, "Choose a valid installation in Game Installations.", 17.0, accent); }
                            else if !runnable { label(content, &font.0, "CoD4 gameplay is not available in this build.\nIts installation and game menus are supported.", 17.0, accent); }
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::Zombies => {
                            if let (Some(namespace), Some(mode)) = (game.and_then(Game::namespace), game.and_then(Game::zombies)) {
                                let owned = game.and_then(|game| inventory.installation(game)).map_or(&[][..], |entry| &entry.zombie_maps[..]);
                                for (zone, name) in mode.maps {
                                    let map = format!("{}:{zone}", namespace.as_str());
                                    let installed = owned.contains(&map);
                                    if installed {
                                        button(content, &font.0, &mut order, *name, Action::ZombieMap(map), true, accent);
                                    }
                                }
                                if owned.is_empty() { label(content, &font.0, "No zombies map of this game is installed.", 17.0, accent); }
                                label(content, &font.0, mode.note, 16.0, Color::WHITE);
                            }
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::WorkInProgress => {
                            label(content, &font.0, "WORK IN PROGRESS", 32.0, accent);
                            label(content, &font.0, "This mode is not playable yet.", 19.0, Color::WHITE);
                            button(content, &font.0, &mut order, "BACK TO GAME", Action::Back, true, accent);
                        }
                        Page::Installations => {
                            button(content, &font.0, &mut order, "SCAN A FOLDER FOR GAMES", Action::Scan, true, accent);
                            if let Some(scan) = dvars.get("ui_folder_scan").filter(|scan| !scan.is_empty()) { label(content, &font.0, scan, 16.0, accent); }
                            for game in Game::ALL {
                                let folder = inventory.installation(game).and_then(|entry| entry.root.as_ref()).map(|root| root.display().to_string()).unwrap_or_else(|| "Not found — choose a folder".into());
                                button(content, &font.0, &mut order, format!("{}\n{}", game.title().to_uppercase(), folder), Action::Pick(game), true, game.accent());
                            }
                            button(content, &font.0, &mut order, "RESCAN INSTALLATIONS", Action::Rescan, inventory.task.is_none(), accent);
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::Settings => {
                            content.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|tabs| {
                                for (index, title) in ["DISPLAY", "AUDIO", "CONTROLS"].into_iter().enumerate() {
                                    button(tabs, &font.0, &mut order, format!("{}{}", if menu.settings_tab == index { "> " } else { "" }, title), Action::SettingsTab(index), true, accent);
                                }
                            });
                            let rows = match menu.settings_tab {
                                0 => vec![
                                    ("resolution", format!("RESOLUTION     {}", settings.resolution)),
                                    ("fullscreen", format!("FULLSCREEN     {}", on(settings.fullscreen))),
                                    ("vsync", format!("VSYNC     {}", on(settings.vsync))),
                                    ("fov", format!("FIELD OF VIEW     {:.0}°", settings.fov)),
                                    ("brightness", format!("BRIGHTNESS     {:+.2}", settings.brightness)),
                                    ("shadows", format!("SHADOWS     {}", on(settings.shadows))),
                                    ("bloom", format!("BLOOM     {}", on(settings.bloom))),
                                    ("dof", format!("DEPTH OF FIELD     {}", on(settings.depth_of_field))),
                                ],
                                1 => vec![("volume", format!("MASTER VOLUME     {:.0}%", settings.master_volume * 100.0))],
                                _ => vec![
                                    ("sensitivity", format!("MOUSE SENSITIVITY     {:.1}", settings.sensitivity)),
                                    ("invert", format!("INVERT MOUSE     {}", on(settings.invert_mouse))),
                                    ("vibration", format!("CONTROLLER VIBRATION     {}", on(settings.pad_vibration))),
                                    ("name", format!("PLAYER NAME     {}{}", settings.player_name, if menu.editing_name { "_" } else { "" })),
                                ],
                            };
                            for (key, text) in rows { button(content, &font.0, &mut order, text, Action::Setting(key), true, accent); }
                            if menu.settings_tab == 2 {
                                for (key, title) in [("+attack", "BIND FIRE"), ("+speed_throw", "BIND AIM"), ("+gostand", "BIND JUMP"), ("+reload", "BIND RELOAD")] { button(content, &font.0, &mut order, title, Action::Bind(key), true, accent); }
                            }
                            button(content, &font.0, &mut order, "RESTORE SETTINGS DEFAULTS", Action::Setting("reset"), true, accent);
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::Host => {
                            label(content, &font.0, "SELECT MAP", 15.0, accent);
                            for map_key in maps.iter().skip(menu.map_page * 6).take(6) {
                                let text = crate::frontend::maps::map_label(map_key);
                                button(content, &font.0, &mut order, format!("{}{}", if map == map_key { "> " } else { "" }, text.to_uppercase()), Action::Map(map_key.clone()), true, accent);
                            }
                            button(content, &font.0, &mut order, "NEXT MAPS  >", Action::MapPage(true), maps.len() > 6, accent);
                            button(content, &font.0, &mut order, "PREVIOUS MAPS  <", Action::MapPage(false), maps.len() > 6, accent);
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::Lobby => {
                            label(content, &font.0, dvars.get("ui_lobby_privacy").unwrap_or("PRIVATE LOBBY"), 18.0, accent);
                            label(content, &font.0, dvars.get("ui_lobby_players").unwrap_or("PLAYERS: 1"), 17.0, Color::WHITE);
                            for index in 0..18 {
                                if let Some(name) = dvars.get(&format!("ui_lobby_member_{index}")).filter(|name| !name.is_empty()) { label(content, &font.0, name, 18.0, Color::srgb(0.83,0.86,0.85)); }
                            }
                            if party.is_host {
                                button(content, &font.0, &mut order, "START MATCH", Action::Start, runnable, accent);
                                button(content, &font.0, &mut order, "CHANGE MAP / RULES", if dvars.get("ui_gametype") == Some("zclassic") { Action::Page(Page::Zombies) } else { Action::Page(Page::Host) }, runnable, accent);
                                button(content, &font.0, &mut order, "SET LOBBY PASSWORD", Action::Password, true, accent);
                                button(content, &font.0, &mut order, "TOGGLE PUBLIC / PRIVATE", Action::Privacy, dvars.get("ui_master_configured") == Some("1"), accent);
                            }
                            button(content, &font.0, &mut order, "LEAVE LOBBY", Action::Leave, true, accent);
                        }
                        Page::Password => {
                            let count = dvars.get("ui_password_input").unwrap_or("").chars().count();
                            label(content, &font.0, format!("PASSWORD     {}_", "•".repeat(count)), 20.0, Color::WHITE);
                            let pending = dvars.get("ui_password_pending") == Some("1");
                            button(content, &font.0, &mut order, if pending { "JOINING…" } else if menu.password_setup { "SAVE PASSWORD" } else { "JOIN LOBBY" }, Action::PasswordSubmit, !pending, accent);
                            button(content, &font.0, &mut order, "CANCEL", Action::PasswordCancel, true, accent);
                            if let Some(status) = dvars.get("ui_password_status").filter(|value| !value.is_empty()) { label(content, &font.0, status, 16.0, accent); }
                        }
                        Page::Communities => {
                            label(content, &font.0, dvars.get("ui_community_current").unwrap_or("Choose a server"), 18.0, accent);
                            for (title, value) in &communities.choices {
                                if !value.is_empty() { button(content, &font.0, &mut order, format!("{}{}", if dvars.get("ui_community_server") == Some(value.as_str()) { "> " } else { "" }, title), Action::Community(value.clone()), !party.in_lobby, accent); }
                            }
                            label(content, &font.0, dvars.get("ui_community_hint").unwrap_or("Applying a server restarts the game and checks its updates."), 16.0, Color::WHITE);
                            button(content, &font.0, &mut order, "APPLY SERVER", Action::ApplyCommunity, dvars.get("ui_community_apply_disabled") == Some("0"), accent);
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                        Page::Browser => {
                            if let Some(snapshot) = &snapshot {
                                label(content, &font.0, &snapshot.community_name, 17.0, accent);
                                if let Some(error) = &snapshot.error { label(content, &font.0, error, 16.0, Color::srgb(1.0,0.6,0.4)); }
                                let filtered: Vec<_> = snapshot.adverts.iter().filter(|advert| game.and_then(Game::namespace).is_some_and(|ns| advert.map.starts_with(&format!("{}:",ns.as_str())))).collect();
                                if filtered.is_empty() { label(content, &font.0, if snapshot.loading { "Finding lobbies…" } else { "No lobbies for this game. Create a public match to host." }, 18.0, Color::WHITE); }
                                for advert in filtered.iter().skip(menu.browser_page*6).take(6) {
                                    button(content, &font.0, &mut order, format!("{}   {}/{}\n{}   /   {}{}", advert.name, advert.players, advert.max_players, crate::frontend::maps::map_label(&advert.map), advert.mode.to_uppercase(), if advert.password_protected { "   •   PASSWORD" } else { "" }), Action::Join(advert.id.to_string()), advert.missing.0 == 0 && !advert.password_protected && !advert.locked, accent);
                                }
                                button(content, &font.0, &mut order, "NEXT LOBBIES  >", Action::BrowserPage(true), filtered.len() > 6, accent);
                            } else { label(content, &font.0, "Choose a community server to find online lobbies.", 18.0, Color::WHITE); }
                            button(content, &font.0, &mut order, "REFRESH", Action::Refresh, browser.is_some(), accent);
                            button(content, &font.0, &mut order, "COMMUNITY SERVERS", Action::Page(Page::Communities), !party.in_lobby, accent);
                            button(content, &font.0, &mut order, "BACK", Action::Back, true, accent);
                        }
                    }
                });
                columns.spawn((Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, padding: UiRect::all(Val::Px(24.0)), align_self: AlignSelf::Start, border: UiRect::top(Val::Px(2.0)), max_width: Val::Px(460.0), ..default() }, BorderColor::all(accent), BackgroundColor(Color::srgba(0.02,0.025,0.03,0.66)))).with_children(|detail| {
                    match menu.page {
                        Page::Host => {
                            label(detail, &font.0, "MATCH RULES", 22.0, accent);
                            button(detail, &font.0, &mut order, format!("MODE     {}", dvars.get("ui_gametype").unwrap_or("dm").to_uppercase()), Action::Mode, true, accent);
                            button(detail, &font.0, &mut order, format!("SCORE LIMIT     {}", dvars.get("ui_scorelimit").unwrap_or("30")), Action::ScoreLimit, true, accent);
                            button(detail, &font.0, &mut order, format!("TIME LIMIT     {} MIN", dvars.get("ui_timelimit").unwrap_or("10")), Action::TimeLimit, true, accent);
                            label(detail, &font.0, crate::frontend::maps::map_label(map), 24.0, Color::WHITE);
                            button(detail, &font.0, &mut order, if menu.editing_lobby { "RETURN TO LOBBY" } else { "CREATE LOBBY" }, if menu.editing_lobby { Action::Page(Page::Lobby) } else { Action::Create }, runnable && !maps.is_empty(), accent);
                        }
                        Page::Password => {
                            label(detail, &font.0, if menu.password_setup { "PROTECT YOUR LOBBY" } else { "ENTER PASSWORD" }, 22.0, accent);
                            label(detail, &font.0, if menu.password_setup { "Type a password, then save. An empty password removes protection." } else { "Type the password provided by the host, then join the lobby." }, 16.0, Color::WHITE);
                        }
                        Page::Lobby => {
                            label(detail, &font.0, "MATCH SETUP", 22.0, accent);
                            label(detail, &font.0, crate::frontend::maps::map_label(map), 24.0, Color::WHITE);
                            let mode = sim::HostGameModeSelection::from_token(dvars.get("ui_gametype").unwrap_or("dm"));
                            label(detail, &font.0, mode.map_or("Multiplayer", |mode| mode.display_name()), 20.0, Color::WHITE);
                            label(detail, &font.0, if party.is_host { "Choose the map and rules, then start the match when your group is ready." } else { "Waiting for the host to start the match. Your group stays together between matches." }, 16.0, Color::srgb(0.7,0.74,0.76));
                        }
                        Page::WorkInProgress => { label(detail, &font.0, "MORE TO COME", 21.0, accent); label(detail, &font.0, "Return to Multiplayer to explore the modes currently available.", 16.0, Color::srgb(0.7,0.74,0.76)); }
                        Page::Settings => {
                            label(detail, &font.0, "PERSONALIZE YOUR GAME", 21.0, accent);
                            label(detail, &font.0, "Select an option to change it.\n\nArrow keys move focus. Enter selects. Esc goes back.\n\nFor controls, select a binding and press the new key.", 16.0, Color::srgb(0.7,0.74,0.76));
                            if let Some(key) = &binding.command { label(detail, &font.0, format!("PRESS A KEY FOR {key}"), 18.0, accent); }
                            if menu.editing_name { label(detail, &font.0, "TYPE YOUR NAME\nENTER TO SAVE", 18.0, accent); }
                        }
                        _ => {
                            label(detail, &font.0, if menu.page == Page::Zombies { "ZOMBIES SURVIVAL" } else { "WELCOME BACK" }, 22.0, accent);
                            label(detail, &font.0, game.map_or("Choose a game from your library.", Game::title), 20.0, Color::WHITE);
                            label(detail, &font.0, if menu.page == Page::Zombies { "Survive the rounds. Earn points for hits and kills, buy weapons and perks, and revive your teammates.\n\nSelect a map to set up your match." } else if game.is_none() { "Select an installation, tune your settings, and enter Multiplayer." } else if installed { "Owned installation detected.\n\nCampaign is Work in Progress." } else { "Add your owned installation in Game Installations to enable Multiplayer." }, 16.0, Color::srgb(0.7,0.74,0.76));
                            if menu.page == Page::Library { label(detail, &font.0, "MODERN WARFARE  /  BLACK OPS", 13.0, accent); }
                        }
                    }
                    let status = dvars.get("ui_frontend_status").filter(|status| !status.is_empty()).unwrap_or(&menu.notice);
                    if !status.is_empty() { label(detail, &font.0, status, 16.0, Color::srgb(1.0,0.68,0.4)); }
                    if dvars.get("ui_map_error") == Some("1") { label(detail, &font.0, dvars.get("ui_map_error_reason").unwrap_or("Map could not load."), 15.0, Color::srgb(1.0,0.6,0.4)); }
                    if let Some(error) = dvars.get("ui_folder_error").filter(|error| !error.is_empty()) { label(detail, &font.0, error, 15.0, Color::srgb(1.0,0.6,0.4)); }
                    if dvars.get("ui_connection_error") == Some("1") { label(detail, &font.0, dvars.get("partyend_reason").unwrap_or("Connection failed."), 15.0, Color::srgb(1.0,0.6,0.4)); }
                });
            });
        });
        root.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(4.5), right: Val::Percent(4.5), bottom: Val::Px(16.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|footer| {
            label(footer, &font.0, "ARROWS NAVIGATE    ENTER SELECT    ESC BACK", 12.0, Color::srgb(0.61,0.66,0.68));
            footer.spawn(Node { width: Val::Px(150.0), ..default() }).with_children(|exit| { button(exit, &font.0, &mut order, "QUIT TO DESKTOP", Action::Quit, true, accent); });
        });
    });
}
