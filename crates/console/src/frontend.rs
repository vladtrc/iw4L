use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{UiMenuDvars, UiMenuRequest, UiPartyState};
use ui::frontend::maps::{map_label, map_preview, pack_maps};
use ui::frontend::rules::{MATCH_CONFIG, host_rules, seed_rules, selected_game};

use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};

const PAGE_SIZE: usize = 10;
const PACK_SLOTS: usize = 16;

#[derive(Default)]
pub(crate) struct FrontendState {
    public: bool,
    map_pack: usize,
    map_page: usize,
    map_hover: usize,
    browser_page: usize,
    adverts: Vec<net::MasterAdvert>,
    browser_order: Vec<master_protocol::AdvertId>,
    browser_notice: Option<(String, std::time::Instant)>,
    password_advert: Option<net::MasterAdvert>,
    password_joining: bool,
    lobby_password: String,
    rules_seeded: bool,
    family_inventory: Option<master_protocol::ContentFamilies>,
}

#[derive(SystemParam)]
pub(crate) struct LobbyServices<'w> {
    intent: Option<Res<'w, net::MasterLaunchIntent>>,
    browser: Option<Res<'w, net::MasterBrowser>>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    action: Option<ResMut<'w, net::PendingMasterMenuAction>>,
    menus: hud::MenuState<'w>,
}

impl LobbyServices<'_> {
    fn submit(&mut self, action: net::MasterMenuAction) -> Result<(), String> {
        if !self
            .intent
            .as_ref()
            .is_some_and(|intent| intent.configured())
        {
            return Err("Public lobbies require a configured master".into());
        }
        let pending = self.action.as_mut().ok_or("Lobby service is unavailable")?;
        if pending.0.is_some() {
            return Err("A lobby request is already pending".into());
        }
        pending.0 = Some(action);
        Ok(())
    }
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "set",
        "seta",
        "setfromdvar",
        "ui_create_lobby",
        "ui_leave_lobby",
        "ui_start_match",
        "ui_lobby_privacy",
        "ui_lobby_cheats",
        "ui_asset_family",
        "ui_vote_skip",
        "ui_maps",
        "ui_map_pack",
        "ui_map_hover",
        "ui_select_map",
        "ui_select_mode",
        "ui_password_open",
        "ui_password_save",
        "ui_password_cancel",
        "ui_password_join",
        "ui_browser_refresh",
        "ui_browser_page",
        "ui_join_lobby",
        "ui_join_lobby_selected",
        "ui_join_lobby_id",
    ] {
        if registry.resolve(name).is_none() {
            registry.register(CommandSpec::new(name));
        }
    }
}

fn change_page(page: &mut usize, command: &ConsoleCommand, len: usize) {
    let delta = command
        .args
        .first()
        .and_then(|arg| arg.parse::<isize>().ok())
        .unwrap_or(0);
    *page = page
        .saturating_add_signed(delta)
        .min(len.saturating_sub(1) / PAGE_SIZE);
}

pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    (mut returned, scope, match_scope): (
        MessageReader<StateTransitionEvent<frame::MatchScope>>,
        Res<session::ScopeControl>,
        Res<State<frame::MatchScope>>,
    ),
    mut commands: Commands,
    mut dvars: ResMut<UiMenuDvars>,
    mut party: ResMut<UiPartyState>,
    mut menus: MessageWriter<UiMenuRequest>,
    mut transition: ResMut<session::SessionSwapRequest>,
    (maps, identity): (Res<ui::MenuMapList>, Res<frame::LaunchIdentity>),
    presentation: Option<Res<ui::frontend::maps::MapPresentation>>,
    settings: Res<frame::GameSettings>,
    localize: Option<Res<asset_game::LocalizeCatalog>>,
    catalog: Option<Res<asset_game::MenuCatalog>>,
    mut services: LobbyServices,
    mut cheats: Option<ResMut<sim::HostCheats>>,
    mut state: Local<FrontendState>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
) {
    if services.browser.is_none() {
        dvars.set("ui_master_status", "Master: Not connected");
    }
    let mut returned_from_world = false;
    let mut returned_in_menu = false;
    let mut match_ended = true;
    let mut left_session = false;
    for fact in returned
        .read()
        .filter(|event| event.exited.is_some() && event.entered == Some(frame::MatchScope::Absent))
    {
        returned_from_world |= fact.exited == Some(frame::MatchScope::Live);
        returned_in_menu |= !(fact.exited == Some(frame::MatchScope::Live));
        if fact.exited == Some(frame::MatchScope::Live) {
            match_ended &= scope.exit_reason() == Some(frame::TeardownReason::MatchEnded);
        }
        left_session |= scope.exit_reason() == Some(frame::TeardownReason::Disconnect);
    }
    if returned_from_world {
        commands.remove_resource::<frame::HostMatchRules>();
        commands.remove_resource::<sim::HostGameModeSelection>();
        if match_ended
            && party.in_lobby
            && party.is_host
            && !state.public
            && services.menus.is_some()
        {
            menus.write(UiMenuRequest::RestoreLobby);
            dvars.set("ui_frontend_status", "");
        } else {
            *party = UiPartyState::default();
            state.public = false;
        }
    } else if returned_in_menu && state.public {
        let reason = match services.bridge.as_ref().map(|bridge| bridge.state()) {
            Some(net::MasterBridgeState::Failed { error, .. }) => format!("{error:?}"),
            Some(net::MasterBridgeState::Closed { reason, .. }) => {
                format!("Lobby closed: {reason:?}")
            }
            _ => "Public lobby ended".into(),
        };
        state.public = false;
        if !party.is_host {
            *party = UiPartyState::default();
            menus.write(UiMenuRequest::Close("game_lobby".into()));
        }
        dvars.set("ui_frontend_status", reason);
    } else if returned_in_menu && party.in_lobby && party.is_host {
        if !left_session && services.menus.is_some() {
            menus.write(UiMenuRequest::RestoreLobby);
        } else {
            *party = UiPartyState::default();
        }
    }
    if dvars.get("ui_mapname").is_none()
        && let Some(map) = maps.maps().find(|map| map.starts_with("iw4:"))
    {
        dvars.set("ui_mapname", map.to_owned());
    }
    if dvars.get("ui_gametype").is_none() {
        dvars.set(
            "ui_gametype",
            sim::HostGameModeSelection::launch_default().token(),
        );
    }
    if !state.rules_seeded
        && let Some(config) = catalog.as_ref().and_then(|c| c.rawfile_text(MATCH_CONFIG))
    {
        seed_rules(&mut dvars, config);
        state.rules_seeded = true;
    }
    let have = state
        .family_inventory
        .get_or_insert_with(|| {
            let trees = asset_transport::NamespaceTrees::discover(&asset_transport::GamesRoot(
                identity.games_root.clone(),
            ));
            net::content_inventory(trees.present().map(|(id, _)| id.as_str()))
        })
        .clone();
    let family_settings: Vec<_> = frame::AssetFamilies::settings()
        .into_iter()
        .map(|(id, setting)| (id.as_str(), setting))
        .collect();
    for (family, setting) in &family_settings {
        if dvars.get(setting).is_none() {
            let env = std::env::var(format!("IW4L_ASSETS_{}", family.to_ascii_uppercase())).ok();
            dvars.set(
                setting,
                env.unwrap_or_else(|| {
                    if have.contains_family(family) {
                        "1"
                    } else {
                        "0"
                    }
                    .into()
                }),
            );
        }
    }
    for (family, setting) in &family_settings {
        let selection = services
            .bridge
            .as_ref()
            .and_then(|bridge| bridge.state().families());
        let enabled = selection.map_or_else(
            || dvars.get(setting) == Some("1"),
            |families| families.enabled(family),
        );
        let status = if !have.contains_family(family) {
            "NOT INSTALLED"
        } else if enabled {
            "ENABLED"
        } else {
            "DISABLED"
        };
        dvars.set(
            &format!("ui_assets_{family}"),
            format!("{}: {status}", family.to_ascii_uppercase()),
        );
        dvars.set(
            &format!("ui_assets_{family}_blocked"),
            if !have.contains_family(family) || state.public || !party.is_host {
                "1"
            } else {
                "0"
            },
        );
    }
    let mut browser_page_changed = false;
    for command in events.read() {
        if command.name.starts_with("ui_browser") || command.name.starts_with("ui_join_lobby") {
            state.browser_notice = None;
        }
        let result = (|| -> Result<(), String> {
            match command.name.as_str() {
                "set" | "seta" => {
                    if let [name, values @ ..] = command.args.as_slice() {
                        if values.is_empty() {
                            echo.write(format!("{name} = {}", dvars.get(name).unwrap_or("")));
                        } else {
                            if let Some((family, _)) = family_settings
                                .iter()
                                .find(|(_, setting)| setting.eq_ignore_ascii_case(name))
                            {
                                if state.public
                                    || (party.in_lobby && !party.is_host)
                                    || *match_scope.get() != frame::MatchScope::Absent
                                {
                                    return Err("Change asset families in a private host lobby before starting the match".into());
                                }
                                if values.len() != 1 || !matches!(values[0].as_str(), "0" | "1") {
                                    return Err(format!("{name} must be 0 or 1"));
                                }
                                if values[0] == "1" && !have.contains_family(family) {
                                    return Err(format!(
                                        "Cannot enable {family}: game content is not installed"
                                    ));
                                }
                            }
                            dvars.set(name, values.join(" "));
                        }
                    }
                }
                "setfromdvar" => {
                    if command.args.first().is_some_and(|name| {
                        family_settings
                            .iter()
                            .any(|(_, setting)| setting.eq_ignore_ascii_case(name))
                    }) {
                        return Err("Use set <asset setting> 0|1 to change asset families".into());
                    }
                    if let [name, source] = command.args.as_slice()
                        && let Some(value) = dvars.get(source).map(str::to_owned)
                    {
                        dvars.set(name, value);
                    }
                }
                "ui_create_lobby" => {
                    selected_game(&dvars, &maps)?;
                    state.lobby_password.clear();
                    party.active = true;
                    party.in_lobby = true;
                    party.is_host = true;
                    state.public = false;
                    dvars.set("ui_frontend_status", "");
                    menus.write(UiMenuRequest::Open("game_lobby".into()));
                }
                "ui_leave_lobby" => {
                    if state.public {
                        services.submit(net::MasterMenuAction::LeaveLobby)?;
                    }
                    *party = UiPartyState::default();
                    state.public = false;
                    menus.write(UiMenuRequest::Close("game_lobby".into()));
                }
                "ui_lobby_privacy" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the host can change lobby privacy".into());
                    }
                    if state.public {
                        services.submit(net::MasterMenuAction::LeaveLobby)?;
                    } else {
                        let (map, mode) = selected_game(&dvars, &maps)?;
                        frame::AssetFamilies::from_inventory(&have, &dvars)?.require_map(&map)?;
                        services.submit(net::MasterMenuAction::Host {
                            map,
                            mode: mode.token().into(),
                            password: state.lobby_password.clone(),
                        })?;
                    }
                    state.public = !state.public;
                }
                "ui_asset_family" => {
                    if !party.in_lobby
                        || !party.is_host
                        || state.public
                        || *match_scope.get() != frame::MatchScope::Absent
                    {
                        return Err(
                            "Change asset families in a private host lobby before making it public"
                                .into(),
                        );
                    }
                    let family = command.args.first().ok_or("ui_asset_family <family>")?;
                    let (_, setting) = family_settings
                        .iter()
                        .find(|(name, _)| *name == family.as_str())
                        .ok_or("Unknown optional asset family")?;
                    if !have.contains_family(family) {
                        return Err(format!(
                            "Cannot enable {family}: game content is not installed"
                        ));
                    }
                    let value = if dvars.get(setting) == Some("1") {
                        "0"
                    } else {
                        "1"
                    };
                    dvars.set(setting, value);
                }
                "ui_lobby_cheats" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the host can change cheats".into());
                    }
                    let cheats = cheats.as_mut().ok_or("Cheats are unavailable")?;
                    cheats.0 = !cheats.0;
                }
                "ui_start_match" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the lobby host can start a match".into());
                    }
                    let (map, mode) = selected_game(&dvars, &maps)?;
                    let families = services
                        .bridge
                        .as_ref()
                        .and_then(|bridge| bridge.state().families())
                        .map(Ok)
                        .unwrap_or_else(|| frame::AssetFamilies::from_inventory(&have, &dvars))?;
                    families.require_map(&map)?;
                    commands.insert_resource(host_rules(&dvars));
                    if state.public {
                        services.submit(net::MasterMenuAction::StartMatch {
                            map,
                            mode: mode.token().into(),
                        })?;
                    } else {
                        transition
                            .request_zone(map.clone())
                            .map_err(|error| error.to_string())?;
                        commands.insert_resource(mode);
                        echo.write(format!("menu: starting {map} {}", mode.token()));
                    }
                }
                "ui_vote_skip" => services.submit(net::MasterMenuAction::VoteToSkip)?,
                "ui_maps" => {
                    let map = dvars.get("ui_mapname").unwrap_or("");
                    state.map_pack = maps.pack_of(map).unwrap_or(0);
                    let index = pack_maps(&maps, state.map_pack)
                        .iter()
                        .position(|installed| installed == map)
                        .unwrap_or(0);
                    state.map_page = index / PAGE_SIZE;
                    state.map_hover = index % PAGE_SIZE;
                }
                "ui_map_pack" => {
                    let slot = command
                        .args
                        .first()
                        .and_then(|arg| arg.parse::<usize>().ok())
                        .ok_or("Invalid map page")?;
                    let (pack, page, _) = map_pages(&maps)
                        .into_iter()
                        .nth(slot)
                        .ok_or("Invalid map page")?;
                    state.map_pack = pack;
                    state.map_page = page;
                    state.map_hover = 0;
                }
                "ui_map_hover" => {
                    state.map_hover = command
                        .args
                        .first()
                        .and_then(|s| s.parse().ok())
                        .filter(|row| *row < PAGE_SIZE)
                        .ok_or("Invalid map row")?;
                }
                "ui_select_map" | "ui_select_mode" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the lobby host can change game setup".into());
                    }
                    let (mut map, mut mode) = selected_game(&dvars, &maps)?;
                    let selecting_map = command.name == "ui_select_map";
                    if selecting_map {
                        let row = command
                            .args
                            .first()
                            .and_then(|arg| arg.parse::<usize>().ok())
                            .filter(|row| *row < PAGE_SIZE)
                            .ok_or("Invalid map row")?;
                        map = pack_maps(&maps, state.map_pack)
                            .get(state.map_page * PAGE_SIZE + row)
                            .ok_or("Map is unavailable")?
                            .clone();
                    } else {
                        mode = command
                            .args
                            .first()
                            .and_then(|arg| sim::HostGameModeSelection::from_token(arg))
                            .ok_or("Unsupported game mode")?;
                    }
                    let families = services
                        .bridge
                        .as_ref()
                        .and_then(|bridge| bridge.state().families())
                        .map(Ok)
                        .unwrap_or_else(|| frame::AssetFamilies::from_inventory(&have, &dvars))?;
                    families.require_map(&map)?;
                    if state.public {
                        services.submit(net::MasterMenuAction::UpdateLobby {
                            map: map.clone(),
                            mode: mode.token().into(),
                        })?;
                    }
                    dvars.set("ui_mapname", map);
                    dvars.set("ui_gametype", mode.token());
                    menus.write(UiMenuRequest::Close(
                        if selecting_map {
                            "game_map_select"
                        } else {
                            "game_mode_select"
                        }
                        .into(),
                    ));
                }
                "ui_password_open" => {
                    dvars.set("ui_password_input", state.lobby_password.clone());
                    dvars.set("ui_password_status", "");
                }
                "ui_password_cancel" => {
                    if state.password_joining {
                        services.submit(net::MasterMenuAction::LeaveLobby)?;
                    }
                    state.password_joining = false;
                    dvars.set("ui_password_input", "");
                    state.password_advert = None;
                }
                "ui_password_save" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the host can change the password".into());
                    }
                    let password = dvars.get("ui_password_input").unwrap_or("").to_owned();
                    if password.len() > master_protocol::MAX_PASSWORD_BYTES {
                        return Err("Password must be at most 64 bytes".into());
                    }
                    if state.public {
                        services.submit(net::MasterMenuAction::SetPassword {
                            password: password.clone(),
                        })?;
                    }
                    state.lobby_password = password;
                    dvars.set("ui_password_input", "");
                    menus.write(UiMenuRequest::Close("lobby_password_setup".into()));
                }
                "ui_password_join" => {
                    let advert = state
                        .password_advert
                        .clone()
                        .ok_or("Lobby is no longer available")?;
                    let password = dvars.get("ui_password_input").unwrap_or("").to_owned();
                    if password.len() > master_protocol::MAX_PASSWORD_BYTES {
                        return Err("Password must be at most 64 bytes".into());
                    }
                    if state.password_joining {
                        return Err("A join request is already pending".into());
                    }
                    services.submit(net::MasterMenuAction::Join {
                        advert_id: advert.id,
                        map: advert.map,
                        mode: advert.mode,
                        password,
                    })?;
                    dvars.set("ui_password_input", "");
                    dvars.set("ui_password_status", "Joining...");
                    state.password_joining = true;
                    dvars.set("ui_password_pending", "1");
                }
                "ui_browser_refresh" => {
                    browser_page_changed = true;
                    services.submit(net::MasterMenuAction::Refresh)?;
                    state.browser_page = 0;
                    state.browser_order.clear();
                    dvars.set("ui_browser_status", "Refreshing lobbies...");
                }
                "ui_browser_page" => {
                    browser_page_changed = true;
                    let len = state.browser_order.len();
                    change_page(&mut state.browser_page, command, len);
                }
                "ui_join_lobby" | "ui_join_lobby_id" | "ui_join_lobby_selected" => {
                    let (advert_id, map, mode) = if command.name == "ui_join_lobby_id" {
                        let advert_id = command
                            .args
                            .first()
                            .filter(|id| id.is_ascii())
                            .ok_or("usage: ui_join_lobby_id <hex ID>")?
                            .parse()
                            .map_err(|_| "Invalid lobby ID")?;
                        let (map, mode) = selected_game(&dvars, &maps)?;
                        (advert_id, map, mode.token().into())
                    } else {
                        let id = if command.name == "ui_join_lobby_selected" {
                            dvars
                                .get("ui_browser_join_id")
                                .unwrap_or("")
                                .parse()
                                .map_err(|_| "Lobby is no longer available")?
                        } else {
                            let row = command
                                .args
                                .first()
                                .and_then(|arg| arg.parse::<usize>().ok())
                                .ok_or("Invalid lobby row")?;
                            state
                                .adverts
                                .get(row)
                                .ok_or("Lobby is no longer available")?
                                .id
                        };
                        let advert = services
                            .browser
                            .as_ref()
                            .and_then(|browser| {
                                browser
                                    .snapshot()
                                    .adverts
                                    .into_iter()
                                    .find(|advert| advert.id == id)
                            })
                            .ok_or("Lobby is no longer available")?;
                        if let Some(message) = frame::AssetFamilies(advert.missing.clone())
                            .missing_notice(&master_protocol::ContentFamilies::default())
                        {
                            dvars.set("partyend_reason", message);
                            dvars.set("ui_connection_error", "1");
                            return Ok(());
                        }
                        if advert.locked || advert.players >= advert.max_players {
                            return Err(if advert.locked {
                                "Lobby closed".into()
                            } else {
                                "Lobby is full".into()
                            });
                        }
                        if advert.password_protected {
                            state.password_advert = Some(advert.clone());
                            dvars.set("ui_password_input", "");
                            dvars.set("ui_password_status", "");
                            menus.write(UiMenuRequest::Open("lobby_password_join".into()));
                            return Ok(());
                        }
                        (advert.id, advert.map.clone(), advert.mode.clone())
                    };
                    services.submit(net::MasterMenuAction::Join {
                        password: String::new(),
                        advert_id,
                        map,
                        mode,
                    })?;
                    state.public = true;
                    party.active = true;
                    party.in_lobby = true;
                    party.is_host = false;
                    dvars.set("ui_frontend_status", "Joining lobby...");
                    menus.write(UiMenuRequest::Open("game_lobby".into()));
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            let status = if command.name.starts_with("ui_password") {
                "ui_password_status"
            } else if command.name.starts_with("ui_browser")
                || command.name.starts_with("ui_join_lobby")
            {
                "ui_browser_status"
            } else {
                "ui_frontend_status"
            };
            if status == "ui_browser_status" {
                state.browser_notice = Some((error.clone(), std::time::Instant::now()));
            }
            dvars.set(status, &error);
            echo.write(format!("menu: {error}"));
        }
    }
    if services.bridge.is_none() && !state.password_joining {
        dvars.set("ui_password_pending", "0");
    }
    if state.password_joining
        && let Some(bridge) = services.bridge.as_ref()
    {
        match bridge.state() {
            net::MasterBridgeState::Joined { .. } => {
                state.password_joining = false;
                dvars.set("ui_password_pending", "0");
                state.password_advert = None;
                state.public = true;
                party.active = true;
                party.in_lobby = true;
                party.is_host = false;
                menus.write(UiMenuRequest::Close("lobby_password_join".into()));
                menus.write(UiMenuRequest::Open("game_lobby".into()));
            }
            net::MasterBridgeState::Failed { error, .. } => {
                if error.source.starts_with("Missing game files:") {
                    dvars.set("partyend_reason", &error.source);
                    dvars.set("ui_connection_error", "1");
                    dvars.set("ui_password_pending", "0");
                    menus.write(UiMenuRequest::Close("lobby_password_join".into()));
                }
                dvars.set(
                    "ui_password_status",
                    if error.source.contains("IncorrectPassword") {
                        "Incorrect password. Try again."
                    } else {
                        "Could not join lobby. Please try again."
                    },
                );
                if services.submit(net::MasterMenuAction::LeaveLobby).is_ok() {
                    state.password_joining = false;
                }
            }
            net::MasterBridgeState::Closed { .. } | net::MasterBridgeState::Left { .. } => {
                dvars.set(
                    "ui_password_status",
                    "Lobby closed. Cancel to return to the browser.",
                );
                if services.submit(net::MasterMenuAction::LeaveLobby).is_ok() {
                    state.password_joining = false;
                }
            }
            _ => {}
        }
    }
    let mut lobby_names = vec![settings.player_name.clone()];
    let mut local_lobby_row = Some(0);
    if state.public
        && let Some(bridge) = services.bridge.as_ref()
    {
        match bridge.state() {
            net::MasterBridgeState::Hosting {
                map,
                mode,
                members,
                identity,
                member_names,
                max_players,
                ..
            }
            | net::MasterBridgeState::Joined {
                map,
                mode,
                members,
                identity,
                member_names,
                max_players,
                ..
            } => {
                local_lobby_row = members.iter().position(|id| *id == identity.member_id);
                lobby_names = members
                    .iter()
                    .map(|id| {
                        member_names
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| "Player".into())
                    })
                    .collect();
                if dvars.get("ui_frontend_status") == Some("Joining lobby...") {
                    dvars.set("ui_frontend_status", "");
                }
                dvars.set("ui_mapname", map);
                dvars.set("ui_gametype", mode);
                dvars.set(
                    "ui_lobby_players",
                    format!("PLAYERS: {} / {max_players}", members.len()),
                );
            }
            net::MasterBridgeState::Failed { error, .. } => {
                dvars.set("ui_frontend_status", format!("{error:?}"))
            }
            net::MasterBridgeState::Closed { reason, .. } => {
                dvars.set("ui_frontend_status", format!("Lobby closed: {reason:?}"))
            }
            _ => {}
        }
    } else {
        dvars.set("ui_lobby_players", "PLAYERS: 1 / 18");
    }
    for row in 0..18 {
        let name = lobby_names.get(row);
        dvars.set(
            &format!("ui_lobby_member_{row}"),
            name.map(String::as_str).unwrap_or(""),
        );
        dvars.set(
            &format!("ui_lobby_member_{row}_local"),
            if local_lobby_row == Some(row) {
                "1"
            } else {
                "0"
            },
        );
        dvars.set(
            &format!("ui_lobby_member_{row}_visible"),
            if name.is_some() { "1" } else { "0" },
        );
    }
    dvars.set("ui_lobby_host", if party.is_host { "1" } else { "0" });
    dvars.set("ui_lobby_public", if state.public { "1" } else { "0" });
    dvars.set(
        "ui_lobby_privacy",
        localize
            .as_ref()
            .and_then(|loc| {
                loc.text(if state.public {
                    "MPUI_LOBBY"
                } else {
                    "MPUI_PRIVATE_MATCH_LOBBY"
                })
            })
            .unwrap_or(if state.public {
                "PUBLIC LOBBY"
            } else {
                "PRIVATE LOBBY"
            }),
    );
    dvars.set(
        "ui_lobby_cheats",
        if cheats.as_ref().is_some_and(|cheats| cheats.0) {
            "CHEATS: ON"
        } else {
            "CHEATS: OFF"
        },
    );
    let label = |map: &str| {
        presentation.as_ref().map_or_else(
            || map_label(map),
            |presentation| presentation.label(map, localize.as_deref()),
        )
    };
    let preview_image = |map: &str| {
        presentation.as_ref().map_or_else(
            || map_preview(map),
            |presentation| presentation.preview(map),
        )
    };
    let selected_label = label(dvars.get("ui_mapname").unwrap_or(""));
    dvars.set("ui_map_label", selected_label);
    let preview = preview_image(dvars.get("ui_mapname").unwrap_or(""));
    dvars.set("ui_lobby_preview", preview);
    let mode_label = dvars
        .get("ui_gametype")
        .and_then(sim::HostGameModeSelection::from_token)
        .map_or("", |mode| mode.display_name());
    dvars.set("ui_mode_label", mode_label);
    let pack = pack_maps(&maps, state.map_pack);
    let hovered_map = pack.get(state.map_page * PAGE_SIZE + state.map_hover);
    dvars.set(
        "ui_map_preview_title",
        hovered_map.map_or_else(String::new, |map| label(map)),
    );
    dvars.set(
        "ui_map_preview",
        hovered_map.map_or_else(String::new, |map| preview_image(map)),
    );
    for row in 0..PAGE_SIZE {
        dvars.set(
            &format!("ui_map_{row}"),
            pack.get(state.map_page * PAGE_SIZE + row)
                .map(|map| label(map))
                .unwrap_or(String::new()),
        );
    }
    let pages = map_pages(&maps);
    for slot in 0..PACK_SLOTS {
        let page = pages.get(slot);
        dvars.set(
            &format!("ui_map_pack_{slot}"),
            page.map_or("", |(_, _, label)| label.as_str()),
        );
        dvars.set(
            &format!("ui_map_pack_tint_{slot}"),
            if page
                .is_some_and(|(pack, page, _)| *pack == state.map_pack && *page == state.map_page)
            {
                "1"
            } else {
                "0.45"
            },
        );
    }
    if let Some(browser) = services.browser.as_ref() {
        let snapshot = browser.snapshot();
        let connection = match snapshot.ping_ms {
            Some(ping) => format!("{ping} ms"),
            None if snapshot.error.is_some() => "Unavailable".to_owned(),
            None => "Connecting...".to_owned(),
        };
        dvars.set(
            "ui_master_status",
            format!("Master: {} | {connection}", snapshot.community_name),
        );
        let focused_row = services
            .menus
            .as_ref()
            .and_then(|menus| menus.focused_item_in("find_lobbies"))
            .and_then(|index| catalog.as_ref()?.get("find_lobbies")?.items.get(index))
            .and_then(|item| item.name.strip_prefix("row"))
            .and_then(|row| row.parse::<usize>().ok());
        let focused_id = focused_row
            .and_then(|row| state.adverts.get(row))
            .map(|advert| advert.id);
        state
            .browser_order
            .retain(|id| snapshot.adverts.iter().any(|advert| advert.id == *id));
        for advert in &snapshot.adverts {
            if !state.browser_order.contains(&advert.id) {
                state.browser_order.push(advert.id);
            }
        }
        if !browser_page_changed && let Some(id) = focused_id {
            if let Some(index) = state
                .browser_order
                .iter()
                .position(|candidate| *candidate == id)
            {
                state.browser_page = index / PAGE_SIZE;
                let row = index % PAGE_SIZE;
                if focused_row != Some(row) {
                    menus.write(UiMenuRequest::Focus {
                        menu: "find_lobbies".into(),
                        item: format!("row{row}"),
                    });
                }
            } else {
                menus.write(UiMenuRequest::Focus {
                    menu: "find_lobbies".into(),
                    item: "back".into(),
                });
            }
        }
        state.browser_page = state
            .browser_page
            .min(state.browser_order.len().saturating_sub(1) / PAGE_SIZE);
        state.adverts = state
            .browser_order
            .iter()
            .skip(state.browser_page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .filter_map(|id| {
                snapshot
                    .adverts
                    .iter()
                    .find(|advert| advert.id == *id)
                    .cloned()
            })
            .collect();
        dvars.set(
            "ui_browser_page",
            format!(
                "{} / {}",
                state.browser_page + 1,
                state.browser_order.len().div_ceil(PAGE_SIZE).max(1)
            ),
        );
        if let Some((notice, _)) = state
            .browser_notice
            .as_ref()
            .filter(|(_, since)| since.elapsed().as_secs() < 5)
        {
            dvars.set("ui_browser_status", notice);
        } else if snapshot.error.is_some() {
            dvars.set(
                "ui_browser_status",
                "Could not refresh lobbies. Retrying automatically.",
            );
        } else if !snapshot.loading {
            dvars.set(
                "ui_browser_status",
                if state.adverts.is_empty() {
                    "No lobbies found"
                } else {
                    ""
                },
            );
        }
    }
    for row in 0..PAGE_SIZE {
        let advert = state.adverts.get(row);
        dvars.set(
            &format!("ui_browser_{row}_visible"),
            if advert.is_some() { "1" } else { "0" },
        );
        dvars.set(
            &format!("ui_browser_{row}_id"),
            advert
                .map(|advert| advert.id.to_string())
                .unwrap_or(String::new()),
        );
        let cells = advert
            .map(|advert| {
                let status = if advert.locked {
                    "CLOSED".into()
                } else if !advert.missing.is_empty() {
                    format!(
                        "NEEDS {}",
                        net::content_names(&advert.missing).to_uppercase()
                    )
                } else if advert.players >= advert.max_players {
                    "FULL".into()
                } else if advert.in_match {
                    "IN MATCH".into()
                } else if advert.password_protected {
                    "PASSWORD".into()
                } else {
                    "OPEN".into()
                };
                let mode = sim::HostGameModeSelection::from_token(&advert.mode)
                    .map_or(advert.mode.as_str(), |mode| mode.display_name());
                let map_key = advert
                    .map
                    .split_once(':')
                    .filter(|(pack, _)| *pack == "iw4")
                    .map(|(_, name)| {
                        format!(
                            "MPUI_{}",
                            name.trim_start_matches("mp_").to_ascii_uppercase()
                        )
                    });
                let fallback_map = label(&advert.map);
                let map = map_key
                    .as_ref()
                    .and_then(|key| localize.as_ref()?.text(key))
                    .unwrap_or(&fallback_map);
                let assets = advert
                    .available
                    .iter()
                    .filter(|token| *token != "iw4")
                    .collect::<Vec<_>>()
                    .join(",")
                    .to_uppercase();
                [
                    browser_cell(&advert.name, 22),
                    browser_cell(map, 18),
                    browser_cell(mode, 20),
                    format!("{}/{}", advert.players, advert.max_players),
                    if assets.is_empty() {
                        "—".into()
                    } else {
                        assets
                    },
                    status,
                ]
            })
            .unwrap_or_else(|| std::array::from_fn(|_| String::new()));
        for (key, value) in ["host", "map", "mode", "players", "assets", "state"]
            .into_iter()
            .zip(cells)
        {
            dvars.set(&format!("ui_browser_{row}_{key}"), value);
        }
    }
}

fn browser_cell(text: &str, limit: usize) -> String {
    let plain: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if plain.chars().count() > limit {
        format!("{}...", plain.chars().take(limit - 3).collect::<String>())
    } else {
        plain
    }
}

fn map_pages(maps: &ui::MenuMapList) -> Vec<(usize, usize, String)> {
    maps.0
        .iter()
        .enumerate()
        .flat_map(|(index, pack)| {
            let pages = pack.maps.len().div_ceil(PAGE_SIZE);
            (0..pages).map(move |page| {
                (
                    index,
                    page,
                    if pages > 1 {
                        format!("{} {}", pack.label, page + 1)
                    } else {
                        pack.label.clone()
                    },
                )
            })
        })
        .collect()
}
