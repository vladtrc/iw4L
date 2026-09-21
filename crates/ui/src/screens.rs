use assets::{LocalizeCatalog, MenuCatalog};

use crate::UiLayer;
use crate::class_icons::{cac_material_iwd_stem, cac_weapon_image, pretty_weapon_name};
use crate::class_setup::{ClassEditRow, ClassLoadoutCatalog, ClassSetupScratch, ClassSlotState};
use crate::menu::{GameLobbyRole, GamePrivacy, GameSetupDraft};
use crate::menu_import;
use crate::model::{
    Content, Enabled, Modality, Rect640, Screen, ScreenCmd, Style, UiIntent, Widget,
    WidgetAnimation,
};
use crate::options::{BindingView, OptionsControlGroup, OptionsDepth, OptionsState, OptionsTab};

#[derive(Clone, Copy, Debug, Default)]
pub struct Host<'a> {
    pub in_game: bool,
    pub match_info: Option<&'a InGameMenuInfo>,
    pub class_store: Option<&'a crate::SessionClassStore>,
    pub class_pending: bool,
    pub class_status: Option<&'a str>,
    pub initial_class_select: bool,
    pub maps: &'a [String],

    pub menus: Option<&'a MenuCatalog>,
    pub loc: Option<&'a LocalizeCatalog>,
    pub classes: Option<&'a ClassSetupScratch>,
    pub loadout: Option<&'a ClassLoadoutCatalog>,
    pub game_setup: Option<&'a GameSetupDraft>,
    pub settings: Option<&'a frame::GameSettings>,
    pub options: Option<&'a OptionsState>,
    pub bindings: Option<&'a BindingView>,
    pub browser_enabled: bool,
    pub browser: Option<&'a net::MasterBrowserSnapshot>,
    pub bridge: Option<&'a net::MasterBridgeState>,
}

impl<'a> Host<'a> {
    pub fn with_maps(maps: &'a [String]) -> Self {
        Self {
            maps,
            in_game: false,
            match_info: None,
            class_store: None,
            class_pending: false,
            class_status: None,
            initial_class_select: false,
            menus: None,
            loc: None,
            classes: None,
            loadout: None,
            game_setup: None,
            settings: None,
            options: None,
            bindings: None,
            browser_enabled: false,
            browser: None,
            bridge: None,
        }
    }
}

pub fn lookup(id: &str, host: Host<'_>) -> Option<Screen> {
    match id {
        "ingame_options" => Some(ingame_options(host.match_info)),
        "ingame_class" => Some(ingame_class(host)),
        "leave_game" => Some(leave_game()),
        "quit_confirm" => Some(quit_confirm()),
        "map_setup" => Some(map_setup(host.maps, host.game_setup)),
        "lobby_game_setup" => Some(lobby_game_setup(host.game_setup, host.browser_enabled)),
        "game_mode_select" => Some(game_mode_select(host.game_setup)),
        "game_map_select" => Some(game_map_select(host.maps, host.game_setup)),
        "game_lobby" => Some(game_lobby(
            host.maps,
            host.game_setup,
            host.bridge,
            host.settings,
        )),
        "find_lobbies" => Some(find_lobbies(
            host.browser_enabled,
            host.browser,
            host.bridge,
        )),
        "class_setup" => Some(class_setup(host)),
        "options" | "options_multi" => Some(options(host)),
        _ => None,
    }
}

pub fn resolve_open<'a>(
    catalog: &'a MenuCatalog,
    name: &str,
    mut host: Host<'a>,
) -> Option<Screen> {
    host.menus = host.menus.or(Some(catalog));
    lookup(name, host).or_else(|| {
        let screen = menu_import::import_open(catalog, name)?;
        if screen.id == "main" {
            Some(rewrite_main(screen, host.maps, host.game_setup))
        } else {
            Some(screen)
        }
    })
}

pub fn resolve_stack<'a>(
    catalog: &'a MenuCatalog,
    names: &[String],
    host: Host<'a>,
) -> Vec<Screen> {
    names
        .iter()
        .filter_map(|name| resolve_open(catalog, name, host))
        .collect()
}

pub fn rewrite_main(mut screen: Screen, maps: &[String], setup: Option<&GameSetupDraft>) -> Screen {
    for widget in &mut screen.widgets {
        if widget.id.ends_with("/button_download_content")
            || widget.id.ends_with("/button_autoupdate")
        {
            widget.id = "main/create_game".into();
            widget.content = Content::Button;
            widget.focusable = true;
            widget.enabled = Enabled::Always;
            widget.style.text_key = "CREATE GAME".into();
            if let Some(map) = selected_game_map(maps, setup) {
                widget.on_activate = vec![
                    ScreenCmd::PlaySound("mouse_click".into()),
                    ScreenCmd::Emit(UiIntent::CreateLobby {
                        map: map.to_owned(),
                        public: false,
                    }),
                    ScreenCmd::Open("game_lobby".into()),
                ];
            } else {
                widget.focusable = false;
                widget.content = Content::Unsupported {
                    reason: "no multiplayer maps discovered".into(),
                };
                widget.style.text_key = "CREATE GAME (NO MAPS FOUND)".into();
                widget.on_activate.clear();
            }
        } else if widget.id.ends_with("/button_xboxlive") {
            widget.id = "main/find_lobbies".into();
            widget.content = Content::Button;
            widget.focusable = true;
            widget.enabled = Enabled::Always;
            widget.style.text_key = "FIND LOBBIES".into();
            widget.on_activate = vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("find_lobbies".into()),
            ];
        } else if widget.id.ends_with("/button_singleplayer") {
            widget.id = "main/classes".into();
            widget.content = Content::Button;
            widget.focusable = true;
            widget.enabled = Enabled::Always;
            widget.style.text_key = "CLASSES".into();
            widget.on_activate = vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("class_setup".into()),
            ];
        } else if widget.id.ends_with("/button_quit") {
            widget.on_activate = vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("quit_confirm".into()),
            ];
        } else if widget.id.ends_with("/button_options") {
            widget.on_activate = vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("options".into()),
            ];
        }
    }
    screen
}

fn selected_game_map<'a>(maps: &'a [String], setup: Option<&GameSetupDraft>) -> Option<&'a str> {
    if let Some(selected) = setup.and_then(|setup| setup.selected_map.as_deref())
        && maps.iter().any(|map| map == selected)
    {
        return maps
            .iter()
            .find(|map| map.as_str() == selected)
            .map(String::as_str);
    }
    maps.iter()
        .find(|map| map.starts_with("iw4:"))
        .map(String::as_str)
}

fn game_map_label(map: &str) -> String {
    map.split_once(':')
        .map_or(map, |(_, zone)| zone)
        .trim_start_matches("mp_")
        .replace('_', " ")
        .to_ascii_uppercase()
}

fn game_preview_stem(map: &str) -> String {
    let zone = map.split_once(':').map_or(map, |(_, zone)| zone);
    format!("preview_{zone}")
}

pub fn game_lobby(
    maps: &[String],
    setup: Option<&GameSetupDraft>,
    bridge: Option<&net::MasterBridgeState>,
    settings: Option<&frame::GameSettings>,
) -> Screen {
    let selected = match bridge {
        Some(net::MasterBridgeState::Hosting { map, .. })
        | Some(net::MasterBridgeState::Joined { map, .. }) => Some(map.as_str()),
        _ => selected_game_map(maps, setup),
    };
    let privacy = setup.map_or(GamePrivacy::Private, |setup| setup.privacy);
    let role = setup.map_or(GameLobbyRole::Host, |setup| setup.role);
    let is_host = role == GameLobbyRole::Host;
    let privacy_label = match privacy {
        GamePrivacy::Private => "PRIVATE LOBBY",
        GamePrivacy::Public => "PUBLIC LOBBY",
    };
    let local_name = settings.map_or("Player", |settings| settings.player_name.as_str());
    let (players, max_players): (Vec<(&str, bool)>, u8) = match bridge {
        Some(net::MasterBridgeState::Hosting {
            identity,
            members,
            member_names,
            max_players,
            ..
        })
        | Some(net::MasterBridgeState::Joined {
            identity,
            members,
            member_names,
            max_players,
            ..
        }) => (
            members
                .iter()
                .map(|member| {
                    (
                        member_names.get(member).map_or("Player", String::as_str),
                        *member == identity.member_id,
                    )
                })
                .collect(),
            *max_players,
        ),
        _ => (vec![(local_name, true)], 18),
    };
    let member_count = players.len();
    let skip_votes = match bridge {
        Some(net::MasterBridgeState::Hosting { skip_votes, .. })
        | Some(net::MasterBridgeState::Joined { skip_votes, .. }) => *skip_votes,
        _ => 0,
    };
    let mode_label = match bridge {
        Some(net::MasterBridgeState::Hosting { mode, .. })
        | Some(net::MasterBridgeState::Joined { mode, .. }) => {
            sim::HostGameModeSelection::from_token(mode)
                .map_or(mode.as_str(), |selection| selection.display_name())
        }
        _ => setup.map_or_else(
            || sim::host_game_mode_kind().display_name(),
            |setup| setup.selected_mode.display_name(),
        ),
    };
    let mut widgets = retail_lobby_background("game_lobby");
    widgets.extend([
        tinted_image(
            "game_lobby/left_fade",
            -107.0,
            0.0,
            280.0,
            480.0,
            "gradient_fadein_fadebottom",
            [1.0, 1.0, 1.0, 0.1],
        ),
        retail_title("game_lobby/title", -43.0, 32.0, 216.0, mode_label),
        label(
            "game_lobby/privacy",
            432.0,
            36.0,
            252.0,
            22.0,
            0.3,
            privacy_label,
        ),
    ]);
    widgets.extend([
        tinted_panel(
            "game_lobby/menu_rule",
            -43.0,
            83.0,
            216.0,
            0.5,
            [1.0, 1.0, 1.0, 0.45],
        ),
        right_label(
            "game_lobby/context_help",
            -43.0,
            204.0,
            216.0,
            34.0,
            0.22,
            " ",
        ),
        tinted_panel(
            "game_lobby/footer",
            -107.0,
            428.0,
            854.0,
            18.0,
            [0.0, 0.0, 0.0, 0.3],
        ),
    ]);
    widgets.push(label(
        "game_lobby/footer_name",
        -43.0,
        428.0,
        216.0,
        18.0,
        0.28,
        local_name,
    ));
    let row_step = (334.0 / member_count.max(1) as f32).min(20.0);
    for (index, (name, local)) in players.iter().enumerate() {
        let y = 66.0 + index as f32 * row_step;
        let row = tinted_panel(
            &format!("game_lobby/member_row/{index}"),
            432.0,
            y,
            252.0,
            row_step - 2.0,
            [0.0, 0.0, 0.0, 0.35],
        );
        widgets.push(row);
        let mut player = label(
            &format!("game_lobby/member/{index}"),
            450.0,
            y,
            226.0,
            row_step - 2.0,
            0.34,
            name,
        );
        player.style.font_enum = 3;
        if *local {
            player.style.fore_color = [1.0, 0.85, 0.35, 1.0];
        }
        widgets.push(player);
    }
    let mut count = right_label(
        "game_lobby/member_count",
        432.0,
        406.0,
        252.0,
        22.0,
        0.5,
        &format!("{member_count}/{max_players} PLAYERS"),
    );
    count.style.font_enum = 9;
    count.style.fore_color = [1.0, 1.0, 1.0, 0.35];
    widgets.push(count);
    if let Some(map) = selected
        && is_host
    {
        widgets.push(retail_button(
            "game_lobby/start",
            -107.0,
            86.0,
            "@MENU_START_GAME_CAPS",
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::StartLobbyMatch {
                    map: map.to_owned(),
                    public: privacy == GamePrivacy::Public,
                }),
            ],
        ));
    }
    widgets.push(retail_button(
        "game_lobby/classes",
        -107.0,
        if is_host { 106.0 } else { 86.0 },
        "@MENU_CREATE_A_CLASS_CAPS",
        vec![
            ScreenCmd::PlaySound("mouse_click".into()),
            ScreenCmd::Open("class_setup".into()),
        ],
    ));
    widgets.push(retail_button(
        "game_lobby/back",
        -107.0,
        400.0,
        "BACK",
        vec![
            ScreenCmd::PlaySound("mouse_click".into()),
            ScreenCmd::Emit(UiIntent::LeaveLobby),
            ScreenCmd::Back,
        ],
    ));
    if is_host {
        widgets.push(retail_button(
            "game_lobby/game_setup",
            -107.0,
            126.0,
            "@MENU_GAME_SETUP_CAPS",
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("lobby_game_setup".into()),
            ],
        ));
    } else if privacy == GamePrivacy::Public {
        widgets.push(retail_button(
            "game_lobby/vote",
            -107.0,
            106.0,
            "@MENU_VOTE_TO_SKIP_CAPS",
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::VoteToSkip),
            ],
        ));
    }
    if privacy == GamePrivacy::Public {
        let vote_status = if member_count > 1 && skip_votes as usize == member_count - 1 {
            "VETO PASSED - HOST MUST CHANGE MAP".to_owned()
        } else {
            format!(
                "VOTE TO SKIP: {skip_votes}/{} MEMBER VOTES",
                member_count.saturating_sub(1)
            )
        };
        widgets.push(label(
            "game_lobby/vote_status",
            -43.0,
            372.0,
            216.0,
            20.0,
            0.24,
            &vote_status,
        ));
    }
    if let Some(map) = selected {
        widgets.push(image(
            "game_lobby/map_preview",
            -43.0,
            244.0,
            216.0,
            122.0,
            &game_preview_stem(map),
        ));
        widgets.push(tinted_panel(
            "game_lobby/map_strip",
            -43.0,
            244.0,
            216.0,
            20.0,
            [0.0, 0.0, 0.0, 0.5],
        ));
        widgets.push(right_label(
            "game_lobby/map",
            -43.0,
            244.0,
            216.0,
            20.0,
            0.34,
            &game_map_label(map),
        ));
        widgets.push(tinted_panel(
            "game_lobby/mode_strip",
            -43.0,
            346.0,
            216.0,
            20.0,
            [0.0, 0.0, 0.0, 0.5],
        ));
        widgets.push(right_label(
            "game_lobby/mode",
            -43.0,
            346.0,
            216.0,
            20.0,
            0.28,
            mode_label,
        ));
    } else {
        widgets.push(label(
            "game_lobby/no_map",
            -43.0,
            244.0,
            216.0,
            20.0,
            0.32,
            "NO IW4 MAPS FOUND",
        ));
    }
    if let Some(net::MasterBridgeState::Failed { error, .. }) = bridge {
        widgets.push(label(
            "game_lobby/error",
            432.0,
            430.0,
            252.0,
            44.0,
            0.24,
            &format!("LOBBY ERROR: {error}"),
        ));
    } else if privacy == GamePrivacy::Public
        && !matches!(
            bridge,
            Some(net::MasterBridgeState::Hosting { .. } | net::MasterBridgeState::Joined { .. })
        )
    {
        widgets.push(label(
            "game_lobby/connecting",
            432.0,
            430.0,
            252.0,
            22.0,
            0.26,
            "CONNECTING TO PUBLIC LOBBY...",
        ));
    }
    for widget in &mut widgets {
        if widget.focusable {
            widget.style.text_scale = 0.32;
        }
        if widget.id == "game_lobby/title" {
            widget.style.text_scale = 0.35;
        }
        widget.help = match widget.id.as_str() {
            "game_lobby/start" => Some("Start the match with these players.".into()),
            "game_lobby/classes" => Some("Create your own custom classes.".into()),
            "game_lobby/game_setup" => Some("Change the map and game mode.".into()),
            "game_lobby/vote" => Some("Vote to skip the current map.".into()),
            "game_lobby/back" => Some("Leave this lobby.".into()),
            _ => None,
        };
    }
    Screen {
        id: "game_lobby".into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Emit(UiIntent::LeaveLobby), ScreenCmd::Back],
    }
}

pub fn lobby_game_setup(setup: Option<&GameSetupDraft>, master_available: bool) -> Screen {
    let privacy = setup.map_or(GamePrivacy::Private, |setup| setup.privacy);

    let mut widgets = retail_popup("lobby_game_setup", 180.0, 126.0, 280.0, 184.0);
    widgets.extend([
        retail_title(
            "lobby_game_setup/title",
            180.0,
            126.0,
            276.0,
            "@MENU_GAME_SETUP_CAPS",
        ),
        retail_button(
            "lobby_game_setup/change_map",
            124.0,
            170.0,
            "@MENU_CHANGE_MAP_CAPS",
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("game_map_select".into()),
            ],
        ),
        retail_button(
            "lobby_game_setup/change_mode",
            124.0,
            194.0,
            &format!(
                "GAME MODE: {}",
                setup.map_or_else(
                    || sim::host_game_mode_kind().display_name(),
                    |setup| setup.selected_mode.display_name(),
                )
            ),
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Open("game_mode_select".into()),
            ],
        ),
    ]);
    if privacy == GamePrivacy::Public || master_available {
        widgets.push(retail_button(
            "lobby_game_setup/privacy",
            124.0,
            218.0,
            if privacy == GamePrivacy::Public {
                "MAKE PRIVATE"
            } else {
                "MAKE PUBLIC"
            },
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::SelectGamePrivacy(privacy == GamePrivacy::Private)),
            ],
        ));
    } else {
        widgets.push(label(
            "lobby_game_setup/privacy_unavailable",
            190.0,
            218.0,
            260.0,
            20.0,
            0.24,
            "MAKE PUBLIC (MASTER NOT CONFIGURED)",
        ));
    }
    widgets.push(retail_button(
        "lobby_game_setup/back",
        124.0,
        258.0,
        "BACK",
        vec![ScreenCmd::PlaySound("mouse_click".into()), ScreenCmd::Back],
    ));
    Screen {
        id: "lobby_game_setup".into(),
        layer: UiLayer::Shell,
        modality: Modality::Overlay,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

pub fn game_mode_select(setup: Option<&GameSetupDraft>) -> Screen {
    let selected = setup.map_or_else(
        || sim::host_game_mode_kind().token(),
        |setup| setup.selected_mode.token(),
    );
    let mut widgets = retail_lobby_background("game_mode_select");
    widgets.extend([
        retail_title(
            "game_mode_select/title",
            40.0,
            28.0,
            300.0,
            "CHOOSE GAME MODE",
        ),
        retail_button(
            "game_mode_select/back",
            40.0,
            414.0,
            "BACK",
            vec![ScreenCmd::PlaySound("mouse_click".into()), ScreenCmd::Back],
        ),
    ]);
    widgets.extend(column_strip(
        "game_mode_select/list",
        40.0,
        70.0,
        568.0,
        112.0,
    ));
    for (index, mode) in sim::HostGameModeSelection::ALL.into_iter().enumerate() {
        let token = mode.token();
        let caption = if token == selected {
            format!("> {}", mode.display_name())
        } else {
            mode.display_name().to_owned()
        };
        widgets.push(retail_button(
            &format!("game_mode_select/{token}"),
            48.0,
            78.0 + index as f32 * 28.0,
            &caption,
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::SelectGameMode(token.into())),
                ScreenCmd::Back,
            ],
        ));
    }
    Screen {
        id: "game_mode_select".into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

const MAPS_PER_PAGE: usize = 16;
const IW4_BASE_MAPS: &[&str] = &[
    "mp_afghan",
    "mp_derail",
    "mp_estate",
    "mp_favela",
    "mp_highrise",
    "mp_invasion",
    "mp_checkpoint",
    "mp_quarry",
    "mp_rundown",
    "mp_rust",
    "mp_boneyard",
    "mp_nightshift",
    "mp_subbase",
    "mp_terminal",
    "mp_underpass",
    "mp_brecourt",
];
const IW4_DLC_MAPS: &[&str] = &[
    "mp_complex",
    "mp_compact",
    "mp_storm",
    "mp_crash",
    "mp_overgrown",
    "mp_abandon",
    "mp_fuel2",
    "mp_strike",
    "mp_trailerpark",
    "mp_vacant",
];

struct GameMapPage<'a> {
    label: String,
    maps: Vec<&'a String>,
}

fn push_map_pages<'a>(pages: &mut Vec<GameMapPage<'a>>, label: &str, maps: Vec<&'a String>) {
    let page_count = maps.len().div_ceil(MAPS_PER_PAGE);
    for (index, chunk) in maps.chunks(MAPS_PER_PAGE).enumerate() {
        pages.push(GameMapPage {
            label: if page_count > 1 {
                format!("{label} {}", index + 1)
            } else {
                label.to_owned()
            },
            maps: chunk.to_vec(),
        });
    }
}

fn game_map_pages(maps: &[String]) -> Vec<GameMapPage<'_>> {
    let mut iw4_base = Vec::new();
    let mut iw4_dlc = Vec::new();
    let mut iw4_other = Vec::new();
    let mut foreign = [("iw5", Vec::new()), ("t5", Vec::new())];
    for map in maps {
        let Some((game, zone)) = map.split_once(':') else {
            continue;
        };
        match game {
            "iw4" if IW4_BASE_MAPS.contains(&zone) => iw4_base.push(map),
            "iw4" if IW4_DLC_MAPS.contains(&zone) => iw4_dlc.push(map),
            "iw4" => iw4_other.push(map),
            _ => {
                if let Some((_, maps)) = foreign.iter_mut().find(|(name, _)| *name == game) {
                    maps.push(map);
                }
            }
        }
    }
    let mut pages = Vec::new();
    push_map_pages(&mut pages, "IW4 BASE", iw4_base);
    push_map_pages(&mut pages, "IW4 DLC", iw4_dlc);
    push_map_pages(&mut pages, "IW4 MAPS", iw4_other);
    for (game, maps) in foreign {
        push_map_pages(
            &mut pages,
            &format!("{} MAPS", game.to_ascii_uppercase()),
            maps,
        );
    }
    pages
}

pub(crate) fn game_map_page_count(maps: &[String]) -> usize {
    game_map_pages(maps).len()
}

fn map_picker(
    id: &str,
    title: &str,
    maps: &[String],
    page_index: usize,
    previewed: Option<&str>,
    activate: impl Fn(&str) -> Vec<ScreenCmd>,
) -> Vec<Widget> {
    let pages = game_map_pages(maps);
    let page_index = page_index.min(pages.len().saturating_sub(1));
    let page = pages.get(page_index);
    let selected = page.and_then(|page| {
        previewed
            .filter(|selected| page.maps.iter().any(|map| map.as_str() == *selected))
            .map(str::to_owned)
            .or_else(|| page.maps.first().map(|map| (*map).clone()))
    });
    let page_maps = page.map_or(&[][..], |page| page.maps.as_slice());
    let row_h = (300.0 / page_maps.len().max(1) as f32).min(18.0);
    let mut widgets = retail_lobby_background(id);
    widgets.extend([
        retail_title(&format!("{id}/title"), 40.0, 28.0, 272.0, title),
        label(
            &format!("{id}/page"),
            48.0,
            72.0,
            226.0,
            20.0,
            0.3,
            page.map_or("NO MAPS", |page| page.label.as_str()),
        ),
        retail_button(
            &format!("{id}/back"),
            40.0,
            414.0,
            "BACK",
            vec![ScreenCmd::PlaySound("mouse_click".into()), ScreenCmd::Back],
        ),
    ]);
    widgets.extend(column_strip(
        &format!("{id}/list"),
        40.0,
        70.0,
        242.0,
        316.0,
    ));
    widgets.extend(column_strip(
        &format!("{id}/preview_panel"),
        300.0,
        70.0,
        308.0,
        320.0,
    ));
    if pages.len() > 1 {
        let previous = if page_index == 0 {
            pages.len() - 1
        } else {
            page_index - 1
        };
        let next = (page_index + 1) % pages.len();
        let mut previous_button = retail_button(
            &format!("{id}/previous_page"),
            40.0,
            388.0,
            "< PREVIOUS",
            vec![ScreenCmd::Emit(UiIntent::SelectGameMapPage(
                previous as u32,
            ))],
        );
        previous_button.rect.w = 120.0;
        previous_button.style.text_align_x = -4.0;
        widgets.push(previous_button);
        let mut next_button = retail_button(
            &format!("{id}/next_page"),
            162.0,
            388.0,
            "NEXT >",
            vec![ScreenCmd::Emit(UiIntent::SelectGameMapPage(next as u32))],
        );
        next_button.rect.w = 120.0;
        next_button.style.text_align_x = -4.0;
        widgets.push(next_button);
    }
    if page_maps.is_empty() {
        widgets.push(label(
            &format!("{id}/empty"),
            48.0,
            94.0,
            226.0,
            22.0,
            0.26,
            "no mp_* maps under IW4L_GAMES",
        ));
    }
    for (index, map) in page_maps.iter().enumerate() {
        let marked = selected.as_deref() == Some(map.as_str());
        let mut choice = retail_button(
            &format!("{id}/{map}"),
            48.0,
            94.0 + index as f32 * row_h,
            &if marked {
                format!("> {}", game_map_label(map))
            } else {
                game_map_label(map)
            },
            activate(map),
        );
        choice.rect.w = 226.0;
        choice.rect.h = (row_h - 1.0).max(8.0);
        choice.style.text_scale = if row_h >= 14.0 { 0.28 } else { 0.22 };
        widgets.push(choice);
    }
    if let Some(map) = selected.as_deref() {
        widgets.push(image(
            &format!("{id}/preview"),
            316.0,
            108.0,
            276.0,
            155.0,
            &game_preview_stem(map),
        ));
        widgets.push(label(
            &format!("{id}/map_name"),
            316.0,
            82.0,
            276.0,
            22.0,
            0.36,
            &game_map_label(map),
        ));
        widgets.push(label(
            &format!("{id}/map_identity"),
            316.0,
            274.0,
            276.0,
            20.0,
            0.26,
            map,
        ));
    }
    widgets
}

fn map_picker_screen(id: &str, widgets: Vec<Widget>) -> Screen {
    Screen {
        id: id.into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

pub fn game_map_select(maps: &[String], setup: Option<&GameSetupDraft>) -> Screen {
    let previewed = setup.and_then(|setup| {
        setup
            .previewed_map
            .as_deref()
            .or(setup.selected_map.as_deref())
    });
    let widgets = map_picker(
        "game_map_select",
        "@MENU_CHOOSE_MAP_CAP",
        maps,
        setup.map_or(0, |setup| setup.map_page),
        previewed,
        |map| {
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::SelectGameMap(map.to_owned())),
                ScreenCmd::Back,
            ]
        },
    );
    map_picker_screen("game_map_select", widgets)
}

pub fn find_lobbies(
    enabled: bool,
    browser: Option<&net::MasterBrowserSnapshot>,
    bridge: Option<&net::MasterBridgeState>,
) -> Screen {
    let joining = match bridge {
        Some(net::MasterBridgeState::Connecting { .. })
        | Some(net::MasterBridgeState::Joining { .. }) => {
            Some(("joining", "JOINING LOBBY...".to_owned()))
        }
        Some(net::MasterBridgeState::Failed { error, .. }) => {
            Some(("join_error", format!("JOIN FAILED: {error}")))
        }
        Some(net::MasterBridgeState::Closed { reason, .. }) => {
            Some(("join_error", format!("SESSION CLOSED: {reason:?}")))
        }
        Some(net::MasterBridgeState::Hosting { .. })
        | Some(net::MasterBridgeState::Joined { .. })
        | None => None,
    };
    let mut widgets = retail_lobby_background("find_lobbies");
    let mut search_again = retail_button(
        "find_lobbies/search_again",
        40.0,
        92.0,
        "SEARCH AGAIN",
        vec![ScreenCmd::Emit(UiIntent::RefreshServers)],
    );
    search_again.focus_order = Some(0);
    let mut back = retail_button(
        "find_lobbies/back",
        40.0,
        414.0,
        "BACK",
        vec![ScreenCmd::Back],
    );
    back.focus_order = Some(u32::MAX);
    widgets.extend([
        retail_title("find_lobbies/title", 40.0, 28.0, 400.0, "FIND LOBBIES"),
        search_again,
        back,
    ]);
    widgets.extend(column_strip(
        "find_lobbies/status_panel",
        40.0,
        132.0,
        568.0,
        148.0,
    ));
    let mut rows = 0;
    if let Some(browser) = browser.filter(|_| enabled) {
        for (index, advert) in browser.adverts.iter().take(LOBBY_ROWS).enumerate() {
            widgets.push(lobby_row(index, advert, joining.is_some()));
            rows += 1;
        }
    }

    let status = joining.or_else(|| {
        if !enabled {
            return Some((
                "config",
                "SET IW4L_MASTER_ADDR AND IW4L_MASTER_SERVER_NAME".to_owned(),
            ));
        }
        let Some(browser) = browser else {
            return Some(("connecting", "CONNECTING TO MASTER...".to_owned()));
        };
        if let Some(error) = &browser.error {
            Some(("error", format!("SERVICE ERROR: {error}")))
        } else if browser.loading {
            Some(("loading", "SEARCHING FOR PUBLIC LOBBIES...".to_owned()))
        } else if browser.adverts.is_empty() {
            Some(("empty", "NO JOINABLE PUBLIC LOBBY FOUND".to_owned()))
        } else {
            None
        }
    });
    if let Some((kind, text)) = status {
        let y = if rows == 0 { 150.0 } else { 288.0 };
        widgets.push(label(
            &format!("find_lobbies/{kind}"),
            52.0,
            y,
            536.0,
            26.0,
            0.28,
            &text,
        ));
    }
    find_lobbies_screen(widgets)
}

const LOBBY_ROWS: usize = 6;
const LOBBY_ROW_H: f32 = 20.0;

fn lobby_row(index: usize, advert: &net::MasterAdvert, joining: bool) -> Widget {
    let refusal = if !advert.missing.is_empty() {
        Some(format!(
            "NEED {}",
            net::content_names(advert.missing).to_ascii_uppercase()
        ))
    } else if advert.locked {
        Some("CLOSED".to_owned())
    } else if advert.players >= advert.max_players {
        Some("FULL".to_owned())
    } else {
        None
    };
    let caption = format!(
        "{} | {} {} | {}/{} | {}",
        advert.name,
        advert.map,
        advert.mode.to_ascii_uppercase(),
        advert.players,
        advert.max_players,
        refusal
            .as_deref()
            .unwrap_or(if advert.in_match { "JOIN LIVE" } else { "JOIN" }),
    );
    let id = format!("find_lobbies/row/{index}");
    let y = 146.0 + index as f32 * LOBBY_ROW_H;
    if refusal.is_some() || joining {
        let mut row = label(&id, 52.0, y, 536.0, LOBBY_ROW_H, 0.25, &caption);
        row.style.fore_color = [0.42, 0.44, 0.46, 1.0];
        return row;
    }
    let mut row = button(
        &id,
        52.0,
        y,
        536.0,
        LOBBY_ROW_H,
        &caption,
        vec![
            ScreenCmd::PlaySound("mouse_click".into()),
            ScreenCmd::Emit(UiIntent::JoinPublicLobby {
                advert_id: advert.id,
                map: advert.map.clone(),
                mode: advert.mode.clone(),
            }),
        ],
    );
    row.style.text_scale = 0.25;
    row.style.font_enum = 3;
    row.style.text_align_mode = 4;
    row.style.background = "menu_button_selection_bar".into();
    row.on_focus = vec![ScreenCmd::PlaySound("mouse_over".into())];
    row.focus_order = Some(index as u32 + 1);
    row
}

fn find_lobbies_screen(widgets: Vec<Widget>) -> Screen {
    Screen {
        id: "find_lobbies".into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

pub fn quit_confirm() -> Screen {
    Screen {
        id: "quit_confirm".into(),
        layer: UiLayer::Shell,
        modality: Modality::Overlay,
        background: None,
        bed: None,
        widgets: vec![
            dim("quit_confirm/dim"),
            label(
                "quit_confirm/title",
                170.0,
                190.0,
                300.0,
                28.0,
                0.4,
                "QUIT?",
            ),
            button(
                "quit_confirm/yes",
                170.0,
                230.0,
                140.0,
                28.0,
                "YES",
                vec![ScreenCmd::Emit(UiIntent::Quit)],
            ),
            button(
                "quit_confirm/no",
                330.0,
                230.0,
                140.0,
                28.0,
                "NO",
                vec![ScreenCmd::Back],
            ),
        ],
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

pub fn map_setup(maps: &[String], setup: Option<&GameSetupDraft>) -> Screen {
    let widgets = map_picker(
        "map_setup",
        "PLAY",
        maps,
        setup.map_or(0, |setup| setup.map_page),
        setup.and_then(|setup| setup.previewed_map.as_deref()),
        |map| {
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::LoadMap(map.to_owned())),
            ]
        },
    );
    map_picker_screen("map_setup", widgets)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayMapLayout {
    pub maps_n: u32,
    pub iw4_n: u32,
    pub iw5_n: u32,
    pub t5_n: u32,
    pub pages_n: u32,
}

pub fn play_map_layout(maps: &[String]) -> PlayMapLayout {
    let cols = assets::group_mp_maps(maps);
    PlayMapLayout {
        maps_n: maps.len() as u32,
        iw4_n: cols[0].len() as u32,
        iw5_n: cols[1].len() as u32,
        t5_n: cols[2].len() as u32,
        pages_n: game_map_page_count(maps) as u32,
    }
}

const MOVEMENT_BINDS: &[(u32, &str)] = &[
    (27, "Forward"),
    (29, "Move Back"),
    (31, "Move Left"),
    (33, "Move Right"),
    (25, "Stand/Jump"),
    (72, "Crouch"),
    (74, "Prone"),
    (59, "Sprint"),
    (47, "Hold Breath"),
    (37, "Turn Left"),
    (39, "Turn Right"),
    (45, "Hold Strafe"),
];

const ACTION_BINDS: &[(u32, &str)] = &[
    (1, "Fire Weapon"),
    (57, "Aim Down the Sight"),
    (51, "Reload"),
    (66, "Switch Weapon"),
    (3, "Melee"),
    (49, "Use"),
    (5, "Throw Frag/Use Equipment"),
    (7, "Throw Special Grenade"),
    (61, "Show Objectives/Scores"),
    (63, "Voice Chat"),
];

const LOOK_BINDS: &[(u32, &str)] = &[
    (41, "Look Up"),
    (43, "Look Down"),
    (71, "Center View"),
    (76, "Toggle Aim Down Sight"),
    (77, "Leave Aim Down Sight"),
];

pub fn options(host: Host<'_>) -> Screen {
    let fallback_settings = frame::GameSettings::default();
    let settings = host.settings.unwrap_or(&fallback_settings);
    let fallback_options = OptionsState::default();
    let state = host.options.unwrap_or(&fallback_options);
    let mut widgets = options_background("options");
    if host.in_game {
        widgets.retain(|widget| {
            !widget.id.starts_with("options/background")
                && !widget.id.starts_with("options/cloud")
                && !widget.id.starts_with("options/glow")
        });
        widgets.insert(0, dim("options/game_dim"));
    }
    widgets.extend(options_tabs(state.tab));
    if state.depth == OptionsDepth::ResolutionPicker {
        widgets.extend(resolution_picker(settings, state));
        return options_screen("options", widgets);
    }
    match state.tab {
        OptionsTab::Video => {
            widgets.push(retail_title(
                "options/page_title",
                248.0,
                28.0,
                240.0,
                "VIDEO",
            ));
            let next_resolution = state
                .display_resolutions
                .iter()
                .position(|value| *value == settings.resolution)
                .map_or(settings.resolution, |i| {
                    state.display_resolutions[(i + 1) % state.display_resolutions.len()]
                });
            let previous_resolution = state
                .display_resolutions
                .iter()
                .position(|value| *value == settings.resolution)
                .map_or(settings.resolution, |i| {
                    state.display_resolutions[(i + state.display_resolutions.len() - 1)
                        % state.display_resolutions.len()]
                });
            widgets.extend([
                cycler(
                    "options/resolution",
                    238.0,
                    56.0,
                    "Resolution",
                    &settings.resolution.to_string(),
                    UiIntent::SetSetting {
                        key: crate::SettingKey::Resolution,
                        value: crate::SettingValue::Resolution(next_resolution),
                    },
                    Some(UiIntent::SetSetting {
                        key: crate::SettingKey::Resolution,
                        value: crate::SettingValue::Resolution(previous_resolution),
                    }),
                    "Choose the output resolution.",
                ),
                cycler(
                    "options/fullscreen",
                    238.0,
                    78.0,
                    "Full Screen",
                    yes_no(settings.fullscreen),
                    UiIntent::SetSetting {
                        key: crate::SettingKey::Fullscreen,
                        value: crate::SettingValue::Bool(!settings.fullscreen),
                    },
                    None,
                    "Switch between a window and borderless full screen.",
                ),
                cycler(
                    "options/vsync",
                    238.0,
                    100.0,
                    "VSync",
                    yes_no(settings.vsync),
                    UiIntent::SetSetting {
                        key: crate::SettingKey::Vsync,
                        value: crate::SettingValue::Bool(!settings.vsync),
                    },
                    None,
                    "Synchronize presentation to the display refresh.",
                ),
            ]);
        }
        OptionsTab::Audio => {
            widgets.push(retail_title(
                "options/page_title",
                248.0,
                28.0,
                240.0,
                "AUDIO",
            ));
            widgets.push(slider(
                "options/volume",
                238.0,
                56.0,
                "Volume",
                settings.master_volume,
                0.0,
                1.0,
                0.05,
                crate::SettingKey::MasterVolume,
                "Master volume for game and menu audio.",
            ));
        }
        OptionsTab::Controls => {
            if state.depth == OptionsDepth::ControlBinds {
                widgets.extend(options_bind_rows(host, state.control_group));
                if let Some(buffer) = &state.name_buffer {
                    widgets.extend(rename_modal(buffer));
                }
                return options_screen("options", widgets);
            }
            widgets.push(retail_title(
                "options/page_title",
                248.0,
                28.0,
                240.0,
                "CONTROLS",
            ));
            for (i, (id, label)) in [
                ("options/movement", "Movement"),
                ("options/actions", "Actions"),
                ("options/look", "Look"),
            ]
            .into_iter()
            .enumerate()
            {
                widgets.push(retail_button(
                    id,
                    238.0,
                    56.0 + i as f32 * 22.0,
                    label,
                    Vec::new(),
                ));
            }
        }
        OptionsTab::Multiplayer => {
            widgets.push(retail_title(
                "options/page_title",
                238.0,
                28.0,
                260.0,
                "MULTIPLAYER",
            ));
            widgets.push(text_edit(
                "options/player_name",
                238.0,
                56.0,
                "Player Name",
                state
                    .name_buffer
                    .as_deref()
                    .unwrap_or(&settings.player_name),
                state.name_buffer.is_some(),
                ScreenCmd::Emit(UiIntent::BeginPlayerNameEdit),
                "Name used by the local player (16 characters maximum).",
            ));
        }
        OptionsTab::Game => {
            widgets.push(retail_title(
                "options/page_title",
                248.0,
                28.0,
                240.0,
                "GAME",
            ));
            widgets.extend([
                slider(
                    "options/sensitivity",
                    238.0,
                    56.0,
                    "Sensitivity",
                    settings.sensitivity,
                    0.1,
                    30.0,
                    0.5,
                    crate::SettingKey::Sensitivity,
                    "Mouse look sensitivity.",
                ),
                cycler(
                    "options/invert_mouse",
                    238.0,
                    78.0,
                    "Look Inversion",
                    yes_no(settings.invert_mouse),
                    UiIntent::SetSetting {
                        key: crate::SettingKey::InvertMouse,
                        value: crate::SettingValue::Bool(!settings.invert_mouse),
                    },
                    None,
                    "Invert vertical mouse look.",
                ),
            ]);
        }
    }
    if let Some(buffer) = &state.name_buffer {
        widgets.extend(rename_modal(buffer));
    }
    options_screen("options", widgets)
}

fn resolution_picker(settings: &frame::GameSettings, state: &OptionsState) -> Vec<Widget> {
    let mut widgets = vec![
        retail_title(
            "options/resolution_title",
            238.0,
            28.0,
            280.0,
            "CHOOSE OPTION",
        ),
        label(
            "options/resolution_current",
            238.0,
            52.0,
            280.0,
            18.0,
            0.25,
            &format!("CURRENT RESOLUTION: {}", settings.resolution),
        ),
    ];
    const COLS: usize = 2;
    for (index, resolution) in state.display_resolutions.iter().copied().enumerate() {
        let col = index % COLS;
        let row = index / COLS;
        let mut choice = retail_button(
            &format!("options/resolution_choice/{index}"),
            238.0 + col as f32 * 132.0,
            78.0 + row as f32 * 20.0,
            &resolution.to_string(),
            vec![ScreenCmd::Emit(UiIntent::SetSetting {
                key: crate::SettingKey::Resolution,
                value: crate::SettingValue::Resolution(resolution),
            })],
        );
        choice.rect.w = 124.0;
        choice.style.text_align_mode = 4;
        choice.style.text_align_x = 0.0;
        choice.style.text_scale = 0.32;
        widgets.push(choice);
    }
    if state.display_resolutions.is_empty() {
        widgets.push(label(
            "options/resolution_gap",
            238.0,
            78.0,
            280.0,
            34.0,
            0.25,
            "NO DISPLAY MODES REPORTED BY THE WINDOW BACKEND",
        ));
    }
    let mut cancel = retail_button(
        "options/resolution_cancel",
        238.0,
        86.0 + state.display_resolutions.len().div_ceil(COLS).max(2) as f32 * 20.0,
        "CANCEL",
        Vec::new(),
    );
    cancel.rect.w = 260.0;
    widgets.push(cancel);
    widgets
}

fn options_bind_rows(host: Host<'_>, group: OptionsControlGroup) -> Vec<Widget> {
    let fallback = BindingView::default();
    let bindings = host.bindings.unwrap_or(&fallback);
    let rows = match group {
        OptionsControlGroup::Movement => MOVEMENT_BINDS,
        OptionsControlGroup::Actions => ACTION_BINDS,
        OptionsControlGroup::Look => LOOK_BINDS,
    };
    let mut widgets = Vec::new();
    widgets.push(retail_title(
        "options/binds/title",
        228.0,
        28.0,
        260.0,
        group.title(),
    ));
    for (i, (command_id, label)) in rows.iter().enumerate() {
        let listening = bindings.listening == Some(*command_id);
        let chord = if listening {
            "PRESS A KEY"
        } else {
            bindings.chord(*command_id)
        };
        widgets.push(bind_control(
            &format!("options/binds/{command_id}"),
            228.0,
            56.0 + i as f32 * 20.0,
            label,
            *command_id,
            chord,
            listening,
        ));
    }
    widgets.push(label(
        "options/binds/help",
        326.0,
        432.0,
        260.0,
        18.0,
        0.25,
        "Press ENTER or CLICK to change",
    ));
    widgets
}

fn options_screen(id: &str, widgets: Vec<Widget>) -> Screen {
    Screen {
        id: id.into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

fn options_background(prefix: &str) -> Vec<Widget> {
    let mut widgets = retail_lobby_background(prefix);
    widgets.extend([
        retail_title(
            &format!("{prefix}/options_title"),
            64.0,
            28.0,
            148.0,
            "OPTIONS",
        ),
        label(
            &format!("{prefix}/back"),
            96.0,
            432.0,
            160.0,
            20.0,
            0.3,
            "BACK · ESC",
        ),
        label(
            &format!("{prefix}/context_help"),
            56.0,
            214.0,
            150.0,
            72.0,
            0.22,
            " ",
        ),
        right_label(
            &format!("{prefix}/version"),
            500.0,
            456.0,
            128.0,
            14.0,
            0.18,
            concat!("IW4L ", env!("CARGO_PKG_VERSION")),
        ),
    ]);
    widgets
}

fn options_tabs(selected: OptionsTab) -> Vec<Widget> {
    let mut widgets = Vec::new();
    for (i, tab) in OptionsTab::ALL.into_iter().enumerate() {
        let mut row = retail_button(
            &format!("options/tab/{}", tab.as_u8()),
            -64.0,
            54.0 + i as f32 * 22.0,
            tab.label(),
            vec![ScreenCmd::Emit(UiIntent::SelectOptionsTab(tab.as_u8()))],
        );
        row.rect.w = 276.0;
        row.style.text_align_x = 0.0;
        if tab == selected {
            row.style.fore_color = [1.0, 1.0, 1.0, 1.0];
        }
        widgets.push(row);
    }
    widgets.push(tinted_panel(
        "options/tab_divider",
        64.0,
        164.0,
        148.0,
        1.0,
        [1.0, 1.0, 1.0, 0.2],
    ));
    widgets
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn cycler(
    id: &str,
    x: f32,
    y: f32,
    label: &str,
    value: &str,
    next: UiIntent,
    previous: Option<UiIntent>,
    help: &str,
) -> Widget {
    let previous = previous.unwrap_or_else(|| next.clone());
    let mut widget = button(
        id,
        x,
        y,
        250.0,
        20.0,
        " ",
        vec![ScreenCmd::Emit(next.clone())],
    );
    widget.content = Content::Cycler {
        label: label.to_owned(),
        value: value.to_owned(),
        previous,
        next,
    };
    widget.help = Some(help.to_owned());
    widget
}

fn slider(
    id: &str,
    x: f32,
    y: f32,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    key: crate::SettingKey,
    help: &str,
) -> Widget {
    let mut widget = button(id, x, y, 250.0, 20.0, " ", Vec::new());
    widget.content = Content::Slider {
        label: label.to_owned(),
        value,
        min,
        max,
        step,
        key,
    };
    widget.help = Some(help.to_owned());
    widget
}

fn text_edit(
    id: &str,
    x: f32,
    y: f32,
    label: &str,
    buffer: &str,
    editing: bool,
    command: ScreenCmd,
    help: &str,
) -> Widget {
    let mut widget = button(id, x, y, 250.0, 20.0, " ", vec![command]);
    widget.content = Content::TextEdit {
        label: label.to_owned(),
        buffer: buffer.to_owned(),
        cursor: buffer.chars().count(),
        editing,
    };
    widget.help = Some(help.to_owned());
    widget
}

fn bind_control(
    id: &str,
    x: f32,
    y: f32,
    label: &str,
    command_id: u32,
    chord: &str,
    listening: bool,
) -> Widget {
    let mut widget = button(
        id,
        x,
        y,
        350.0,
        18.0,
        " ",
        vec![ScreenCmd::Emit(UiIntent::BeginBinding { id: command_id })],
    );
    widget.content = Content::Bind {
        label: label.to_owned(),
        command_id,
        chord: chord.to_owned(),
        listening,
    };
    widget
}

fn rename_modal(buffer: &str) -> Vec<Widget> {
    vec![
        tinted_panel(
            "options/name_modal",
            198.0,
            136.0,
            284.0,
            112.0,
            [0.32, 0.32, 0.32, 0.96],
        ),
        retail_title("options/name_title", 214.0, 144.0, 248.0, "PLAYER NAME"),
        text_edit(
            "options/name_buffer",
            210.0,
            176.0,
            "",
            buffer,
            true,
            ScreenCmd::Emit(UiIntent::CommitPlayerNameEdit(buffer.to_owned())),
            "Type a name; ENTER accepts and ESC cancels.",
        ),
    ]
}

#[derive(Clone, Copy, Default)]
struct CacTables<'a> {
    loadout: Option<&'a ClassLoadoutCatalog>,
    stats: Option<&'a assets::CapturedStringTable>,
    perks: Option<&'a assets::CapturedStringTable>,
    loc: Option<&'a assets::LocalizeCatalog>,
}

impl<'a> CacTables<'a> {
    fn from_host(host: Host<'a>) -> Self {
        let menus = host.menus;
        Self {
            loadout: host.loadout,
            stats: menus.and_then(|c| c.string_table("mp/statsTable.csv")),
            perks: menus.and_then(|c| c.string_table("mp/perkTable.csv")),
            loc: host.loc,
        }
    }

    fn text(&self, key: &str) -> Option<String> {
        let key = key.strip_prefix('@')?;
        if key.is_empty() {
            return None;
        }
        self.loc?.text(key).map(str::to_owned)
    }

    fn weapon(&self, weapon: &str) -> Option<assets::CacWeaponPreview> {
        self.loadout
            .and_then(|catalog| catalog.previews.get(weapon))
            .cloned()
            .or_else(|| assets::weapon_preview(self.stats?, weapon))
    }

    fn perk(&self, reference: &str) -> Option<assets::CacPerkRow> {
        let rows = assets::perk_rows(self.perks?);
        assets::perk_row(&rows, reference).cloned()
    }

    fn weapon_caption(&self, weapon: &str) -> String {
        self.weapon(weapon)
            .and_then(|row| self.text(&row.name_key))
            .unwrap_or_else(|| pretty_weapon_name(weapon))
    }

    fn perk_caption(&self, reference: &str) -> String {
        self.perk(reference)
            .and_then(|row| self.text(&row.name_key))
            .unwrap_or_else(|| pretty_weapon_name(reference))
    }

    fn row_caption(&self, row: ClassEditRow, value: &str) -> String {
        if row.perk_slot().is_some() {
            self.perk_caption(value)
        } else {
            self.weapon_caption(value)
        }
    }

    fn weapon_icon(&self, weapon: &str) -> String {
        self.weapon(weapon)
            .map(|row| row.image)
            .filter(|image| !image.is_empty())
            .map(|image| cac_material_iwd_stem(&image).to_owned())
            .or_else(|| cac_weapon_image(weapon).map(str::to_owned))
            .unwrap_or_default()
    }

    fn perk_icon(&self, reference: &str) -> String {
        self.perk(reference)
            .map(|row| row.image)
            .filter(|image| !image.is_empty())
            .map(|image| cac_material_iwd_stem(&image).to_owned())
            .unwrap_or_default()
    }

    fn folder_caption(&self, folder: crate::class_setup::ClassPickerFolder) -> String {
        match folder.category {
            Some(category) => format!(
                "{}:{}",
                folder.namespace.as_str(),
                self.category_caption(category)
            ),
            None => folder.namespace.as_str().to_owned(),
        }
    }

    fn category_caption(&self, category: assets::CacAuthoredCategory) -> String {
        category
            .loc_key()
            .and_then(|key| self.text(key))
            .unwrap_or_else(|| category.menu_label().to_owned())
    }

    fn edit_row_caption(&self, row: ClassEditRow) -> String {
        self.text(row.loc_key())
            .unwrap_or_else(|| row.label().to_ascii_uppercase())
    }
}

pub fn class_setup(host: Host<'_>) -> Screen {
    let fallback_scratch;
    let scratch = match host.classes {
        Some(scratch) => scratch,
        None => {
            fallback_scratch = ClassSetupScratch::default();
            &fallback_scratch
        }
    };
    let fallback_catalog;
    let catalog = match host.loadout {
        Some(catalog) => catalog,
        None => {
            fallback_catalog = ClassLoadoutCatalog::default();
            &fallback_catalog
        }
    };
    class_setup_from(scratch, catalog, CacTables::from_host(host))
}

pub(crate) fn class_slot_index_from_id(id: &str) -> Option<usize> {
    id.strip_prefix("class_setup/slot/")?.parse().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CacRevealGroup {
    Slot,

    Pick,
    Attachment,
}

pub(crate) fn cac_reveal_target(id: &str) -> Option<(CacRevealGroup, usize)> {
    for (prefix, group) in [
        ("class_setup/slot/preview/", CacRevealGroup::Slot),
        ("class_setup/pick/preview/", CacRevealGroup::Pick),
        (
            "class_setup/attachment/preview/",
            CacRevealGroup::Attachment,
        ),
    ] {
        if let Some(rest) = id.strip_prefix(prefix) {
            return Some((group, rest.split('/').next()?.parse().ok()?));
        }
    }
    None
}

pub(crate) fn class_pick_index_from_id(id: &str) -> Option<usize> {
    let rest = id.strip_prefix("class_setup/pick/")?;
    if rest.contains('/') {
        return None;
    }
    rest.parse().ok()
}

const LIST_X: f32 = -107.0;
const LIST_W: f32 = 280.0;
const LIST_H: f32 = 20.0;
const LIST_TOP: f32 = 56.0;
const LIST_STEP: f32 = 20.0;
const LIST_SCALE: f32 = 0.375;

fn list_row(id: &str, index: usize, caption: &str, on_activate: Vec<ScreenCmd>) -> Widget {
    let mut row = button(
        id,
        LIST_X,
        LIST_TOP + index as f32 * LIST_STEP,
        LIST_W,
        LIST_H,
        caption,
        on_activate,
    );
    row.style.text_scale = LIST_SCALE;
    row.style.font_enum = 3;
    row.style.text_align_mode = 6;
    row.style.text_align_x = -2.0;
    row.style.background = "menu_button_selection_bar".into();
    row.focus_order = Some(index as u32);
    row
}

fn class_setup_from(
    scratch: &ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
    tables: CacTables<'_>,
) -> Screen {
    let mut widgets = retail_lobby_background("class_setup");
    widgets.extend([
        tinted_image(
            "class_setup/left_fade",
            LIST_X,
            0.0,
            280.0,
            480.0,
            "gradient_fadein_fadebottom",
            [1.0, 1.0, 1.0, 0.12],
        ),
        retail_title("class_setup/title", -47.0, 28.0, 240.0, "CREATE A CLASS"),
        button(
            "class_setup/back",
            40.0,
            440.0,
            160.0,
            20.0,
            "BACK - ESC",
            vec![ScreenCmd::Back],
        ),
    ]);

    for (index, slot) in scratch.slots.iter().enumerate() {
        widgets.extend(class_preview_card(index, slot, tables));
    }
    if scratch.summary_active {
        let opened_row = scratch.editing.or(scratch.editing_attachment);
        let visible_rows =
            opened_row.and_then(|row| ClassEditRow::ALL.iter().position(|r| *r == row));
        widgets.extend(class_edit_rows(tables).into_iter().filter(|widget| {
            visible_rows
                .is_none_or(|n| widget.focus_order.is_some_and(|order| (order as usize) < n))
        }));
    } else {
        for (i, slot) in scratch.slots.iter().enumerate() {
            widgets.push(list_row(
                &format!("class_setup/slot/{i}"),
                i,
                &slot.name.to_ascii_uppercase(),
                vec![
                    ScreenCmd::PlaySound("mouse_click".into()),
                    ScreenCmd::Emit(UiIntent::CacSelectSlot(i as u32)),
                ],
            ));
        }
    }
    if let Some(row) = scratch.editing_attachment {
        widgets.retain(|widget| widget.id != "class_setup/back");
        widgets.extend(class_attachment_picker(scratch, catalog, tables, row));
    } else if let Some(row) = scratch.editing {
        widgets.retain(|widget| widget.id != "class_setup/back");
        widgets.extend(class_setup_picker(scratch, catalog, tables, row));
    }
    if let Some(buffer) = scratch.rename_buffer.as_deref() {
        widgets.extend(class_rename_modal(buffer));
    }

    for widget in &mut widgets {
        widget.style.canvas = crate::model::Canvas::Wide;
    }
    Screen {
        id: "class_setup".into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: Vec::new(),
        on_open: Vec::new(),
        on_back: vec![ScreenCmd::Back],
    }
}

fn class_edit_rows(tables: CacTables<'_>) -> Vec<Widget> {
    let mut widgets = Vec::new();
    for (index, row) in ClassEditRow::ALL.into_iter().enumerate() {
        widgets.push(list_row(
            crate::class_setup::edit_row_id(row),
            index,
            &tables.edit_row_caption(row),
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::CacEditRow(row.as_u8())),
            ],
        ));
    }
    let n = ClassEditRow::ALL.len();
    widgets.push(list_row(
        "class_setup/rename",
        n,
        &tables
            .text("@MENU_RENAME_CAPS")
            .unwrap_or_else(|| "RENAME".to_owned()),
        vec![ScreenCmd::Emit(UiIntent::CacBeginRename)],
    ));
    widgets.push(list_row(
        "class_setup/reset",
        n + 1,
        &tables
            .text("@MENU_RESET_CLASS_CAPS")
            .unwrap_or_else(|| "RESET CLASS".to_owned()),
        vec![
            ScreenCmd::PlaySound("mouse_click".into()),
            ScreenCmd::Emit(UiIntent::CacResetClass),
        ],
    ));
    widgets
}

fn class_preview_card(index: usize, slot: &ClassSlotState, tables: CacTables<'_>) -> Vec<Widget> {
    let id = |suffix: &str| format!("class_setup/slot/preview/{index}/{suffix}");
    let mut widgets = vec![
        tinted_panel(&id("bed"), 412.0, 50.0, 252.0, 404.0, [0.0, 0.0, 0.0, 0.35]),
        label(
            &id("name"),
            416.0,
            52.0,
            244.0,
            20.0,
            0.3,
            &slot.name.to_ascii_uppercase(),
        ),
    ];
    let mut gun = |suffix: &str, y: f32, weapon: &str, attachments: &[String]| {
        widgets.push(tinted_panel(
            &id(&format!("{suffix}_bar")),
            412.0,
            y,
            252.0,
            18.0,
            [0.0, 0.0, 0.0, 0.45],
        ));
        widgets.push(label(
            &id(suffix),
            416.0,
            y,
            244.0,
            18.0,
            0.3,
            &tables.weapon_caption(weapon),
        ));
        let stem = tables.weapon_icon(weapon);
        if !stem.is_empty() {
            widgets.push(image(
                &id(&format!("{suffix}_image")),
                436.0,
                y + 20.0,
                204.0,
                62.0,
                &stem,
            ));
        }
        if !attachments.is_empty() {
            widgets.push(label(
                &id(&format!("{suffix}_attachments")),
                416.0,
                y + 82.0,
                244.0,
                14.0,
                0.2,
                &attachments
                    .iter()
                    .map(|attachment| tables.weapon_caption(attachment))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
    };
    gun("primary", 76.0, &slot.primary, &slot.primary_attachments);
    gun(
        "secondary",
        180.0,
        &slot.secondary,
        &slot.secondary_attachments,
    );

    let mut icon_row = |suffix: &str, x: f32, y: f32, w: f32, icon: String, caption: String| {
        if !icon.is_empty() {
            widgets.push(image(
                &id(&format!("{suffix}_image")),
                x,
                y,
                20.0,
                20.0,
                &icon,
            ));
        }
        widgets.push(label(
            &id(suffix),
            x + 22.0,
            y + 2.0,
            w - 22.0,
            16.0,
            0.24,
            &caption,
        ));
    };
    icon_row(
        "lethal",
        414.0,
        288.0,
        124.0,
        tables.weapon_icon(&slot.lethal),
        tables.weapon_caption(&slot.lethal),
    );
    icon_row(
        "tactical",
        540.0,
        288.0,
        124.0,
        tables.weapon_icon(&slot.tactical),
        tables.weapon_caption(&slot.tactical),
    );
    for (i, (suffix, value)) in [
        ("perk1", &slot.perk1),
        ("perk2", &slot.perk2),
        ("perk3", &slot.perk3),
        ("deathstreak", &slot.deathstreak),
    ]
    .into_iter()
    .enumerate()
    {
        icon_row(
            suffix,
            414.0,
            312.0 + i as f32 * 26.0,
            248.0,
            tables.perk_icon(value),
            tables.perk_caption(value),
        );
    }
    for widget in &mut widgets {
        if matches!(widget.content, Content::Image) {
            widget.style.image_contain = true;
        }
    }
    widgets
}

fn class_rename_modal(buffer: &str) -> Vec<Widget> {
    vec![
        tinted_panel(
            "class_setup/rename_modal",
            198.0,
            136.0,
            284.0,
            126.0,
            [0.02, 0.03, 0.05, 0.98],
        ),
        retail_title(
            "class_setup/rename_title",
            214.0,
            146.0,
            248.0,
            "RENAME CLASS",
        ),
        text_edit(
            "class_setup/rename_buffer",
            214.0,
            178.0,
            "",
            buffer,
            true,
            ScreenCmd::Emit(UiIntent::CacCommitRename(buffer.to_owned())),
            "Type a name; ENTER accepts and ESC cancels.",
        ),
        button(
            "class_setup/rename_cancel",
            214.0,
            218.0,
            116.0,
            18.0,
            "CANCEL",
            vec![ScreenCmd::Emit(UiIntent::CacCancelRename)],
        ),
        button(
            "class_setup/rename_accept",
            342.0,
            218.0,
            116.0,
            18.0,
            "ACCEPT",
            vec![ScreenCmd::Emit(UiIntent::CacCommitRename(
                buffer.to_owned(),
            ))],
        ),
    ]
}

struct Popup {
    y: f32,
    rows: usize,

    crumbs: Vec<String>,

    preview_h: f32,
}

impl Popup {
    fn band_h(&self) -> f32 {
        (self.rows.min(crate::class_setup::PICKER_PAGE_SIZE) as f32 * 20.0 + 24.0)
            .max(self.preview_h)
    }

    fn placed(mut self, opened_from_row: usize) -> Self {
        let wanted =
            LIST_TOP + opened_from_row as f32 * LIST_STEP + self.crumbs.len() as f32 * 20.0;
        let highest = LIST_TOP + self.crumbs.len() as f32 * 20.0;
        self.y = wanted.min(432.0 - self.band_h()).max(highest);
        self
    }

    fn chrome(&self, prefix: &str) -> Vec<Widget> {
        let mut widgets = vec![tinted_panel(
            &format!("{prefix}/band"),
            -107.0,
            self.y,
            854.0,
            self.band_h(),
            [0.32, 0.32, 0.32, 1.0],
        )];
        for (depth, crumb) in self.crumbs.iter().enumerate() {
            let y = self.y - (self.crumbs.len() - depth) as f32 * 20.0;
            widgets.push(tinted_panel(
                &format!("{prefix}/crumb_bg/{depth}"),
                LIST_X,
                y,
                LIST_W,
                LIST_H,
                [0.0, 0.0, 0.0, 0.9],
            ));
            let mut item = label(
                &format!("{prefix}/crumb/{depth}"),
                LIST_X,
                y,
                LIST_W,
                LIST_H,
                LIST_SCALE,
                crumb,
            );
            item.style.text_align_mode = 4;
            item.style.text_align_x = 68.0;
            item.style.font_enum = 9;
            item.style.fore_color = [1.0, 1.0, 1.0, 0.65];
            widgets.push(item);
        }
        widgets
    }

    fn row(&self, id: &str, index: usize, caption: &str, on_activate: Vec<ScreenCmd>) -> Widget {
        let mut row = button(
            id,
            LIST_X,
            self.y + 4.0 + index as f32 * 20.0,
            LIST_W,
            LIST_H,
            caption,
            on_activate,
        );
        row.style.text_scale = LIST_SCALE;
        row.style.font_enum = 3;
        row.style.text_align_mode = 4;
        row.style.text_align_x = 68.0;
        row.style.background = "popup_button_selection_bar_short".into();
        row.focus_order = Some(index as u32);
        row
    }

    fn pages(&self, prefix: &str, page: usize) -> Vec<Widget> {
        let pages = self.rows.div_ceil(crate::class_setup::PICKER_PAGE_SIZE);
        if pages <= 1 {
            return Vec::new();
        }
        let y = self.y + self.band_h() - 20.0;
        let mut previous = button(
            &format!("{prefix}/page_prev"),
            4.0,
            y,
            66.0,
            20.0,
            "< PREV",
            vec![ScreenCmd::Emit(UiIntent::CacPage(-1))],
        );
        let mut next = button(
            &format!("{prefix}/page_next"),
            140.0,
            y,
            72.0,
            20.0,
            "NEXT >",
            vec![ScreenCmd::Emit(UiIntent::CacPage(1))],
        );
        for widget in [&mut previous, &mut next] {
            widget.style.font_enum = 3;
            widget.style.text_scale = 0.3;
            widget.focus_order = Some(u32::MAX);
        }
        vec![
            previous,
            label(
                &format!("{prefix}/page_count"),
                78.0,
                y,
                60.0,
                20.0,
                0.3,
                &format!("{} / {pages}", page + 1),
            ),
            next,
        ]
    }
}

fn popup_preview(
    prefix: &str,
    popup: &Popup,
    tables: CacTables<'_>,
    value: &str,
    weapon: bool,
) -> Vec<Widget> {
    let mut widgets = Vec::new();
    let name = if weapon {
        tables.weapon_caption(value)
    } else {
        tables.perk_caption(value)
    };
    let mut title = label(
        &format!("{prefix}/preview_name"),
        268.0,
        popup.y + 6.0,
        200.0,
        20.0,
        LIST_SCALE,
        &name,
    );
    title.style.font_enum = 9;
    title.style.text_wrap = true;
    widgets.push(title);
    let icon = if weapon {
        tables.weapon_icon(value)
    } else {
        tables.perk_icon(value)
    };
    if !icon.is_empty() {
        let firearm = tables.loadout.is_some_and(|catalog| {
            [ClassEditRow::Primary, ClassEditRow::Secondary]
                .into_iter()
                .any(|row| {
                    catalog.options(row).iter().any(|base| {
                        base == value
                            || catalog
                                .attachment_variants(row, base)
                                .iter()
                                .any(|variant| variant == value)
                    })
                })
        });
        let (w, h) = if weapon && firearm {
            (200.0, 100.0)
        } else {
            (64.0, 64.0)
        };
        let mut picture = image(
            &format!("{prefix}/preview_image"),
            268.0,
            popup.y + 18.0,
            w,
            h,
            &icon,
        );
        picture.style.image_contain = true;
        widgets.push(picture);
    }
    let desc_key = if weapon {
        tables.weapon(value).map(|row| row.desc_key)
    } else {
        tables.perk(value).map(|row| row.desc_key)
    };
    if let Some(desc) = desc_key.as_deref().and_then(|key| tables.text(key)) {
        let mut body = label(
            &format!("{prefix}/preview_desc"),
            268.0,
            popup.y + if weapon { 108.0 } else { 84.0 },
            196.0,
            32.0,
            0.3,
            &desc,
        );
        body.style.fore_color = [1.0, 1.0, 1.0, 0.65];
        body.style.font_enum = 3;
        body.style.text_wrap = true;
        widgets.push(body);
    }
    if !weapon {
        return widgets;
    }
    widgets.extend(popup_stat_bars(prefix, popup, tables, value));
    widgets
}

fn popup_stat_bars(prefix: &str, popup: &Popup, tables: CacTables<'_>, value: &str) -> Vec<Widget> {
    let mut widgets = Vec::new();

    for (index, (bar, value)) in tables
        .weapon(value)
        .map(|row| row.bars)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let y = popup.y + 22.0 + index as f32 * 26.0;
        let mut caption = label(
            &format!("{prefix}/bar/{index}/label"),
            555.0,
            y - 16.0,
            120.0,
            14.0,
            0.3,
            &tables
                .text(bar.label_key())
                .unwrap_or_else(|| bar.label_key().trim_start_matches('@').to_owned()),
        );
        caption.style.text_align_mode = 6;
        widgets.push(caption);
        widgets.push(tinted_panel(
            &format!("{prefix}/bar/{index}/track"),
            555.0,
            y,
            120.0,
            2.0,
            [0.0, 0.0, 0.0, 0.85],
        ));
        let fill = 120.0 * (value.clamp(0, 100) as f32) / 100.0;
        if fill > 0.0 {
            widgets.push(tinted_panel(
                &format!("{prefix}/bar/{index}/fill"),
                555.0,
                y,
                fill,
                2.0,
                [0.75, 1.0, 0.7, 0.85],
            ));
        }
    }
    widgets
}

fn class_attachment_picker(
    scratch: &ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
    tables: CacTables<'_>,
    row: ClassEditRow,
) -> Vec<Widget> {
    let weapon = scratch
        .slots
        .get(scratch.selected)
        .map(|slot| slot.row_value(row))
        .unwrap_or("");
    let variants = catalog.attachment_variants(row, weapon);
    let mut crumbs = vec![tables.edit_row_caption(row)];
    if let Some(category) = scratch.picker_category {
        crumbs.push(tables.folder_caption(category));
    }
    crumbs.push(tables.weapon_caption(weapon));
    let popup = Popup {
        y: 0.0,
        rows: variants.len() + 1,
        crumbs,
        preview_h: 184.0,
    }
    .placed(
        ClassEditRow::ALL
            .iter()
            .position(|r| *r == row)
            .unwrap_or(0),
    );
    let mut widgets = popup.chrome("class_setup/attachment");
    if scratch.picker_page == 0 {
        widgets.push(popup.row(
            "class_setup/attachment_none",
            0,
            "No Attachment",
            vec![ScreenCmd::Emit(UiIntent::CacPickAttachment(None))],
        ));
    }
    for (index, variant) in variants.iter().enumerate() {
        let start = scratch.picker_page * crate::class_setup::PICKER_PAGE_SIZE;
        if !(start..start + crate::class_setup::PICKER_PAGE_SIZE).contains(&(index + 1)) {
            continue;
        }
        widgets.push(popup.row(
            &format!("class_setup/attachment/{index}"),
            index + 1 - start,
            &tables.weapon_caption(variant),
            vec![ScreenCmd::Emit(UiIntent::CacPickAttachment(Some(
                variant.clone(),
            )))],
        ));
        widgets.extend(popup_preview(
            &format!("class_setup/attachment/preview/{}", index + 1),
            &popup,
            tables,
            variant,
            true,
        ));
    }
    widgets.push(label(
        "class_setup/attachment/preview/0/none",
        268.0,
        popup.y + 6.0,
        196.0,
        20.0,
        LIST_SCALE,
        "No Attachment",
    ));
    widgets.extend(popup_stat_bars(
        "class_setup/attachment/stats",
        &popup,
        tables,
        weapon,
    ));
    widgets.extend(popup.pages("class_setup/attachment", scratch.picker_page));
    if variants.is_empty() {
        widgets.push(label(
            "class_setup/attachment_gap",
            212.0,
            popup.y + 6.0,
            300.0,
            34.0,
            0.24,
            "NO LOADED COMPLETEDEF VARIANTS FOR THIS WEAPON",
        ));
    }
    widgets.push(hidden_cancel("class_setup/attachment_cancel"));
    widgets
}

fn hidden_cancel(id: &str) -> Widget {
    let mut widget = button(
        id,
        4.0,
        440.0,
        208.0,
        18.0,
        "BACK - ESC",
        vec![
            ScreenCmd::PlaySound("mouse_click".into()),
            ScreenCmd::Emit(UiIntent::CacCancelEdit),
        ],
    );
    widget.style.text_scale = 0.28;
    widget.style.text_align_mode = 6;
    widget.style.text_align_x = -2.0;
    widget
}

fn class_setup_picker(
    scratch: &ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
    tables: CacTables<'_>,
    row: ClassEditRow,
) -> Vec<Widget> {
    let opened_from = ClassEditRow::ALL
        .iter()
        .position(|r| *r == row)
        .unwrap_or(0);
    let mut crumbs = vec![tables.edit_row_caption(row)];
    let categories =
        if ClassLoadoutCatalog::uses_categories(row) && scratch.picker_category.is_none() {
            catalog.categories_for(row)
        } else {
            Vec::new()
        };
    if let Some(category) = scratch.picker_category {
        crumbs.push(tables.folder_caption(category));
    }
    if !categories.is_empty() {
        let popup = Popup {
            y: 0.0,
            rows: categories.len(),
            crumbs,
            preview_h: 0.0,
        }
        .placed(opened_from);
        let mut widgets = popup.chrome("class_setup/pick");
        let start = scratch.picker_page * crate::class_setup::PICKER_PAGE_SIZE;
        for (index, category) in categories
            .iter()
            .enumerate()
            .skip(start)
            .take(crate::class_setup::PICKER_PAGE_SIZE)
        {
            let mut folder_row = popup.row(
                &format!("class_setup/cat/{}", category.slug()),
                index - start,
                &tables.folder_caption(*category),
                vec![
                    ScreenCmd::PlaySound("mouse_click".into()),
                    ScreenCmd::Emit(UiIntent::CacPickCategory(index as u8)),
                ],
            );
            folder_row.rect.w = 380.0;
            widgets.push(folder_row);
        }
        widgets.extend(popup.pages("class_setup/pick", scratch.picker_page));
        widgets.push(hidden_cancel("class_setup/pick_cancel"));
        return widgets;
    }

    let options = scratch.picker_options(catalog, row);
    let weapon_row = row.perk_slot().is_none();
    let popup = Popup {
        y: 0.0,
        rows: options.len().max(1),
        crumbs,
        preview_h: 184.0,
    }
    .placed(opened_from);
    let mut widgets = popup.chrome("class_setup/pick");
    if options.is_empty() {
        widgets.push(label(
            "class_setup/pick_gap",
            212.0,
            popup.y + 6.0,
            300.0,
            34.0,
            0.24,
            "NO CATALOG ROWS LOADED FOR THIS SLOT",
        ));
    }
    for (index, option) in options.iter().enumerate() {
        let start = scratch.picker_page * crate::class_setup::PICKER_PAGE_SIZE;
        if !(start..start + crate::class_setup::PICKER_PAGE_SIZE).contains(&index) {
            continue;
        }
        let caption = tables.row_caption(row, option);
        widgets.push(popup.row(
            &format!("class_setup/pick/{index}"),
            index - start,
            &caption,
            vec![
                ScreenCmd::PlaySound("mouse_click".into()),
                ScreenCmd::Emit(UiIntent::CacPick(option.clone())),
            ],
        ));
    }
    for (index, option) in options.iter().enumerate() {
        let start = scratch.picker_page * crate::class_setup::PICKER_PAGE_SIZE;
        if !(start..start + crate::class_setup::PICKER_PAGE_SIZE).contains(&index) {
            continue;
        }
        widgets.extend(popup_preview(
            &format!("class_setup/pick/preview/{index}"),
            &popup,
            tables,
            option,
            weapon_row,
        ));
    }
    widgets.extend(popup.pages("class_setup/pick", scratch.picker_page));
    widgets.push(hidden_cancel("class_setup/pick_cancel"));
    widgets
}

pub(crate) fn class_pick_default_index(
    scratch: &ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
) -> usize {
    let Some(row) = scratch.editing else {
        return 0;
    };
    let options = scratch.picker_options(catalog, row);
    let selected = scratch
        .slots
        .get(scratch.selected)
        .and_then(|slot| {
            let value = slot.row_value(row);
            options.iter().position(|option| option == value)
        })
        .unwrap_or(0);
    let start = scratch.picker_page * crate::class_setup::PICKER_PAGE_SIZE;
    if (start..start + crate::class_setup::PICKER_PAGE_SIZE).contains(&selected) {
        selected
    } else {
        start
    }
}

fn column_strip(id: &str, x: f32, y: f32, w: f32, h: f32) -> Vec<Widget> {
    let mut widgets = vec![tinted_image(
        &format!("{id}/plate"),
        x,
        y,
        w,
        h,
        "white",
        [1.0, 1.0, 1.0, 0.15],
    )];
    widgets.extend(drop_shadow_frame(id, x, y, w, h));
    widgets
}

fn drop_shadow_frame(id: &str, x: f32, y: f32, w: f32, h: f32) -> Vec<Widget> {
    const S: f32 = 32.0;
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    [
        ("tl", x - S, y - S, S, S),
        ("t", x, y - S, w, S),
        ("tr", x + w, y - S, S, S),
        ("r", x + w, y, S, h),
        ("br", x + w, y + h, S, S),
        ("b", x, y + h, w, S),
        ("bl", x - S, y + h, S, S),
        ("l", x - S, y, S, h),
    ]
    .into_iter()
    .map(|(piece, px, py, pw, ph)| {
        tinted_image(
            &format!("{id}/shadow_{piece}"),
            px,
            py,
            pw,
            ph,
            &format!("drop_shadow_{piece}"),
            BLACK,
        )
    })
    .collect()
}

fn retail_popup(id: &str, x: f32, y: f32, w: f32, h: f32) -> Vec<Widget> {
    let mut widgets = vec![
        dim(&format!("{id}/dim")),
        tinted_image(
            &format!("{id}/plate"),
            x,
            y,
            w,
            h,
            "white",
            [0.5, 0.5, 0.5, 1.0],
        ),
    ];
    widgets.extend(drop_shadow_frame(id, x, y, w, h));
    widgets.push(tinted_image(
        &format!("{id}/title_band"),
        x,
        y,
        w,
        22.0,
        "gradient_fadein",
        [1.0, 1.0, 1.0, 0.25],
    ));
    widgets
}

fn tinted_panel(id: &str, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Widget {
    Widget {
        id: id.into(),
        rect: Rect640 {
            x,
            y,
            w,
            h,
            horz_align: 1,
            vert_align: 1,
        },
        style: Style {
            fore_color: color,
            ..Style::default()
        },
        content: Content::Panel,
        focusable: false,
        enabled: Enabled::Always,
        on_focus: Vec::new(),
        on_activate: Vec::new(),
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

fn retail_lobby_background(prefix: &str) -> Vec<Widget> {
    vec![
        fullscreen_image(&format!("{prefix}/background"), "mw2_main_background"),
        animated_image(
            &format!("{prefix}/cloud_left"),
            -107.0,
            0.0,
            1708.0,
            480.0,
            "mw2_main_cloud_overlay",
            [1.0, 1.0, 1.0, 0.5],
            WidgetAnimation::ScrollX {
                period_seconds: 60.0,
                distance_640: -854.0,
            },
        ),
        animated_image(
            &format!("{prefix}/cloud_right"),
            -961.0,
            0.0,
            1708.0,
            480.0,
            "mw2_main_cloud_overlay",
            [1.0, 1.0, 1.0, 0.5],
            WidgetAnimation::ScrollX {
                period_seconds: 50.0,
                distance_640: 854.0,
            },
        ),
        animated_image(
            &format!("{prefix}/glow_slow"),
            0.0,
            0.0,
            640.0,
            480.0,
            "mockup_bg_glow",
            [1.0, 1.0, 1.0, 0.5],
            WidgetAnimation::PulseAlpha {
                radians_per_second: 1.0 / 1.5,
            },
        ),
        animated_image(
            &format!("{prefix}/glow_fast"),
            0.0,
            0.0,
            640.0,
            480.0,
            "mockup_bg_glow",
            [1.0, 1.0, 1.0, 0.5],
            WidgetAnimation::PulseAlpha {
                radians_per_second: 1.0 / 0.48,
            },
        ),
    ]
}

fn dim(id: &str) -> Widget {
    Widget {
        id: id.into(),
        rect: Rect640 {
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 480.0,
            horz_align: 4,
            vert_align: 4,
        },
        style: Style {
            fore_color: [0.0, 0.0, 0.0, 0.35],
            ..Style::default()
        },
        content: Content::Panel,
        focusable: false,
        enabled: Enabled::Always,
        on_focus: Vec::new(),
        on_activate: Vec::new(),
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

fn label(id: &str, x: f32, y: f32, w: f32, h: f32, scale: f32, text: &str) -> Widget {
    Widget {
        id: id.into(),
        rect: Rect640 {
            x,
            y,
            w,
            h,
            horz_align: 1,
            vert_align: 1,
        },
        style: Style {
            fore_color: [0.92, 0.93, 0.95, 1.0],
            text_scale: scale,
            text_key: text.into(),
            text_align_mode: 4,
            ..Style::default()
        },
        content: Content::Label,
        focusable: false,
        enabled: Enabled::Always,
        on_focus: Vec::new(),
        on_activate: Vec::new(),
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

fn retail_title(id: &str, x: f32, y: f32, w: f32, text: &str) -> Widget {
    let mut widget = label(id, x, y, w, 28.0, 0.5, text);
    widget.style.font_enum = 9;
    widget.style.text_align_mode = 6;
    widget.style.text_align_x = -4.0;
    widget
}

fn retail_button(id: &str, x: f32, y: f32, text: &str, on_activate: Vec<ScreenCmd>) -> Widget {
    let mut widget = button(id, x, y, 336.0, 20.0, text, on_activate);
    widget.style.font_enum = 3;
    widget.style.text_scale = 0.375;
    widget.style.text_align_mode = 6;
    widget.style.text_align_x = -60.0;
    widget.style.background = "menu_button_selection_bar".into();
    widget.on_focus = vec![ScreenCmd::PlaySound("mouse_over".into())];
    widget
}

fn right_label(id: &str, x: f32, y: f32, w: f32, h: f32, scale: f32, text: &str) -> Widget {
    let mut widget = label(id, x, y, w, h, scale, text);
    widget.style.text_align_mode = 6;
    widget.style.text_align_x = -4.0;
    widget
}

fn tinted_image(id: &str, x: f32, y: f32, w: f32, h: f32, stem: &str, color: [f32; 4]) -> Widget {
    let mut widget = image(id, x, y, w, h, stem);
    widget.style.fore_color = color;
    widget
}

fn fullscreen_image(id: &str, stem: &str) -> Widget {
    let mut widget = image(id, 0.0, 0.0, 640.0, 480.0, stem);
    widget.rect.horz_align = 4;
    widget.rect.vert_align = 4;
    widget
}

fn animated_image(
    id: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stem: &str,
    color: [f32; 4],
    animation: WidgetAnimation,
) -> Widget {
    let mut widget = tinted_image(id, x, y, w, h, stem, color);
    if x == 0.0 && y == 0.0 && w == 640.0 && h == 480.0 {
        widget.rect.horz_align = 4;
        widget.rect.vert_align = 4;
    }
    widget.style.animation = animation;
    widget
}

fn image(id: &str, x: f32, y: f32, w: f32, h: f32, stem: &str) -> Widget {
    Widget {
        id: id.into(),
        rect: Rect640 {
            x,
            y,
            w,
            h,
            horz_align: 1,
            vert_align: 1,
        },
        style: Style {
            fore_color: [1.0; 4],
            background: stem.into(),
            ..Style::default()
        },
        content: Content::Image,
        focusable: false,
        enabled: Enabled::Always,
        on_focus: Vec::new(),
        on_activate: Vec::new(),
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

fn button(
    id: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    on_activate: Vec<ScreenCmd>,
) -> Widget {
    Widget {
        id: id.into(),
        rect: Rect640 {
            x,
            y,
            w,
            h,
            horz_align: 1,
            vert_align: 1,
        },
        style: Style {
            fore_color: [0.92, 0.93, 0.95, 1.0],
            text_scale: 0.35,
            text_key: text.into(),
            text_align_mode: 4,
            ..Style::default()
        },
        content: Content::Button,
        focusable: true,
        enabled: Enabled::Always,
        on_focus: Vec::new(),
        on_activate,
        icon: String::new(),
        help: None,
        focus_order: None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct InGameMenuInfo {
    pub mode: String,
    pub description: Option<String>,
    pub map: String,
    pub icon: Option<assets::AssetKey>,
    pub compass: Option<String>,
}

pub fn ingame_options(info: Option<&InGameMenuInfo>) -> Screen {
    let mut widgets = vec![
        dim("ingame_options/dim"),
        retail_title("ingame_options/title", 64.0, 28.0, 148.0, "OPTIONS"),
        label(
            "ingame_options/back",
            96.0,
            432.0,
            160.0,
            20.0,
            0.375,
            "BACK - ESC",
        ),
    ];

    for mut glow in retail_lobby_background("ingame_options")
        .into_iter()
        .filter(|w| w.id.contains("/glow_"))
    {
        glow.rect.horz_align = 4;
        glow.rect.vert_align = 4;
        widgets.insert(1, glow);
    }
    if let Some(info) = info {
        let mut mode = retail_title("ingame_options/mode", 414.0, 28.0, 272.0, &info.mode);
        mode.style.text_align_mode = 4;
        mode.style.text_align_x = 0.0;
        mode.style.text_scale = 0.35;
        widgets.push(mode);
        if let Some(text) = &info.description {
            let mut description = label(
                "ingame_options/description",
                414.0,
                64.0,
                272.0,
                60.0,
                0.375,
                text,
            );
            description.style.font_enum = 3;
            description.style.text_wrap = true;
            description.style.fore_color = [1.0, 1.0, 1.0, 0.75];
            widgets.push(description);
        }
        let mut map = retail_title("ingame_options/map_name", 424.0, 148.0, 240.0, &info.map);
        map.style.text_align_mode = 4;
        map.style.text_align_x = 4.0;
        map.style.text_scale = 0.375;
        widgets.push(map);
        if let Some(icon) = &info.icon {
            widgets.push(tinted_image(
                "ingame_options/team",
                -32.0,
                94.0,
                128.0,
                128.0,
                &icon.display(),
                [1.0, 1.0, 1.0, 0.3],
            ));
        }
        if let Some(compass) = &info.compass {
            widgets.push(image(
                "ingame_options/map",
                424.0,
                171.0,
                240.0,
                240.0,
                compass,
            ));
        }
    }
    for (i, (id, text, cmds)) in [
        (
            "choose_class",
            "Choose Class",
            vec![ScreenCmd::Open("ingame_class".into())],
        ),
        ("change_team", "Change Team", vec![]),
        ("mute_players", "Mute Players", vec![]),
        (
            "options",
            "Options",
            vec![ScreenCmd::Open("options".into())],
        ),
        ("favorites", "Add To Favorites", vec![]),
        ("vote", "Call Vote", vec![]),
        (
            "leave",
            "Leave Game",
            vec![ScreenCmd::Open("leave_game".into())],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut row = retail_button(
            &format!("ingame_options/{id}"),
            -64.0,
            64.0 + i as f32 * 20.0,
            text,
            cmds,
        );
        if row.on_activate.is_empty() {
            row.style.fore_color = [0.4, 0.4, 0.4, 1.0];
            row.focusable = false;
        }
        widgets.push(row);
    }
    Screen {
        id: "ingame_options".into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: vec![],
        on_open: vec![],
        on_back: vec![ScreenCmd::Back],
    }
}

pub fn leave_game() -> Screen {
    let mut widgets = vec![dim("leave_game/dim"), {
        let mut panel = dim("leave_game/panel");
        panel.rect = Rect640 {
            x: 170.0,
            y: 156.0,
            w: 300.0,
            h: 84.0,
            horz_align: 1,
            vert_align: 1,
        };
        panel.style.fore_color = [0.5, 0.5, 0.5, 1.0];
        panel
    }];
    let mut title = retail_title("leave_game/title", 170.0, 156.0, 300.0, "LEAVE GAME?");
    title.rect.h = 24.0;
    title.style.text_scale = 0.375;
    title.style.text_align_mode = 5;
    title.style.text_align_x = 0.0;
    widgets.push(title);
    for (i, (id, text, cmd)) in [
        ("yes", "Yes", ScreenCmd::Emit(UiIntent::Disconnect)),
        ("no", "No", ScreenCmd::Back),
    ]
    .into_iter()
    .enumerate()
    {
        let mut row = retail_button(
            &format!("leave_game/{id}"),
            174.0,
            200.0 + i as f32 * 20.0,
            text,
            vec![cmd],
        );
        row.rect.w = 292.0;
        row.focus_order = Some(if id == "no" { 0 } else { 1 });
        row.style.background = "popup_button_selection_bar".into();
        row.style.text_align_x = -24.0;
        widgets.push(row);
    }
    Screen {
        id: "leave_game".into(),
        layer: UiLayer::Shell,
        modality: Modality::Overlay,
        background: None,
        bed: None,
        widgets,
        focus_overrides: vec![],
        on_open: vec![],
        on_back: vec![ScreenCmd::Back],
    }
}

fn ingame_class(host: Host<'_>) -> Screen {
    let mut screen = ingame_options(host.match_info);
    screen.id = "ingame_class".into();
    screen.widgets.retain(|widget| {
        matches!(
            widget.id.as_str(),
            "ingame_options/dim"
                | "ingame_options/glow_slow"
                | "ingame_options/glow_fast"
                | "ingame_options/team"
                | "ingame_options/back"
        )
    });
    screen.widgets.push(retail_title(
        "ingame_class/title",
        -6.0,
        28.0,
        218.0,
        "CHOOSE CLASS",
    ));
    if let Some(store) = host.class_store {
        let tables = CacTables::from_host(host);
        for (index, slot) in store.slots.iter().enumerate() {
            let mut row = retail_button(
                &format!("class_setup/slot/{index}"),
                -64.0,
                64.0 + index as f32 * 20.0,
                &slot.name,
                vec![ScreenCmd::Emit(UiIntent::SelectClass(index as i32))],
            );
            if slot.lock_reason.is_some() || host.class_pending {
                row.focusable = false;
                row.on_activate.clear();
                row.style.fore_color = [0.4, 0.4, 0.4, 1.0];
            }
            screen.widgets.push(row);
            screen
                .widgets
                .extend(class_preview_card(index, slot, tables));
        }
    }
    if host.initial_class_select {
        screen
            .widgets
            .retain(|widget| widget.id != "ingame_options/back");
        screen.on_back.clear();
    }
    if let Some(status) = host
        .class_status
        .or(host.class_pending.then_some("Choosing class…"))
    {
        screen.widgets.push(label(
            "ingame_class/status",
            32.0,
            404.0,
            340.0,
            28.0,
            0.3,
            status,
        ));
    }
    for widget in &mut screen.widgets {
        if !widget.id.starts_with("ingame_options/") {
            widget.style.canvas = crate::model::Canvas::Wide;
        }
    }

    screen
}
