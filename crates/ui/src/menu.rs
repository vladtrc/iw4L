use bevy::prelude::*;
use frame::ClientSet;

use crate::classes::store::{
    ClassStoreFile, load_class_store, save_class_store, sync_host_class_loadouts,
};

#[derive(Resource, Clone, Debug, Default)]
pub struct MenuMapList(pub Vec<asset_transport::MapPack>);

impl MenuMapList {
    pub fn maps(&self) -> impl Iterator<Item = &String> {
        self.0.iter().flat_map(|pack| &pack.maps)
    }

    pub fn contains(&self, map: &str) -> bool {
        self.maps().any(|installed| installed == map)
    }

    pub fn pack_of(&self, map: &str) -> Option<usize> {
        self.0
            .iter()
            .position(|pack| pack.maps.iter().any(|installed| installed == map))
    }
}

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuMapList>()
            .init_resource::<crate::CommunityServers>()
            .init_resource::<crate::barracks::BarracksProfile>()
            .init_resource::<crate::ClassLoadoutCatalog>()
            .init_resource::<frame::GameSettings>()
            .init_resource::<crate::BindingView>()
            .init_resource::<crate::SessionClassStore>()
            .init_resource::<ClassStoreFile>()
            .init_resource::<frame::HostClassLoadouts>()
            .init_resource::<asset_game::MenuCatalog>()
            .init_resource::<asset_game::LocalizeCatalog>()
            .add_systems(
                Update,
                (
                    crate::options::apply_window_settings,
                    load_class_store,
                    crate::barracks::load_profile,
                    crate::barracks::save_profile,
                    sync_host_class_loadouts,
                    save_class_store,
                )
                    .chain()
                    .in_set(ClientSet::Ui),
            );
    }
}

pub fn install_frontend_menus(catalog: &mut asset_game::MenuCatalog) -> Result<(), String> {
    catalog.load_definitions(include_str!("../menus/map_error.json"))?;
    catalog.load_definitions(include_str!("../menus/connection_error.json"))?;
    catalog.load_definitions(include_str!("../menus/frontend.json"))?;
    catalog.load_definitions(include_str!("../menus/classes.json"))?;
    catalog.load_definitions(include_str!("../menus/barracks.json"))?;
    catalog.load_definitions(include_str!("../menus/settings.json"))?;
    catalog.load_definitions(include_str!("../menus/controller.json"))?;
    catalog.load_definitions(include_str!("../menus/game_folders.json"))?;
    catalog.load_definitions(include_str!("../menus/community_servers.json"))?;
    if let Some(item) = catalog
        .menus
        .get_mut("options_community_servers")
        .and_then(|menu| {
            menu.items
                .iter_mut()
                .find(|item| item.name == "community_server")
        })
    {
        item.choices = crate::CommunityServers::default().choices;
    }
    let slider = catalog
        .get("pc_options_video")
        .and_then(|menu| {
            menu.items
                .iter()
                .find(|item| item.name == "video_brightness")
        })
        .cloned();
    for (name, menu) in &mut catalog.menus {
        if name == "pc_options_look"
            && let Some(template) = &slider
            && let Some(y) = menu
                .items
                .iter()
                .find(|item| item.text_key == "@MENU_MOUSE_SENSITIVITY")
                .map(|label| label.rect.y)
            && let Some(item) = menu
                .items
                .iter_mut()
                .find(|item| item.item_type == asset_game::ITEM_TYPE_SLIDER)
        {
            *item = asset_game::MenuItem {
                name: "look_sensitivity".into(),
                dvar: "ui_sensitivity".into(),
                slider: Some(asset_game::MenuSlider {
                    min: 0.1,
                    max: 30.0,
                    step: 0.1,
                    display_range: None,
                    decimals: 1,
                    suffix: String::new(),
                }),
                ..template.clone()
            };
            item.rect.y = y;
        }
        if matches!(name.as_str(), "popup_endgame" | "popup_endgame_ranked") {
            for item in &mut menu.items {
                if item.name == "button_yes" {
                    item.handlers.action = vec![asset_game::MenuEvent::Script(
                        "play mouse_click; close self; exec \"disconnect\";".into(),
                    )];
                }
            }
        }
        if let Some(settings_link) = menu
            .items
            .iter()
            .find(|item| {
                item.item_type == 1
                    && matches!(item.text_key.as_str(), "@MENU_CHAT" | "@MENU_VOICE")
            })
            .cloned()
            && menu
                .items
                .iter()
                .any(|item| item.text_key == "@MENU_RESET_SYSTEM_DEFAULTS")
        {
            let mut multiplayer = settings_link;
            multiplayer.name = "multiplayer_settings".into();
            multiplayer.text_key = "@MENU_MULTIPLAYER_OPTIONS".into();
            multiplayer.rect.y = 88.0;
            multiplayer.vis_exp = "1".into();
            multiplayer.disabled_exp = "0".into();
            multiplayer.handlers.action = vec![asset_game::MenuEvent::Script(
                "play mouse_click; close self; open options_multi;".into(),
            )];
            let mut controller = multiplayer.clone();
            let mut game_folders = multiplayer.clone();
            menu.items.push(multiplayer);
            controller.name = "controller_settings".into();
            controller.text_key = "Controller".into();
            controller.rect.y = 108.0;
            controller.handlers.action = vec![asset_game::MenuEvent::Script(
                "play mouse_click; close self; open options_controller;".into(),
            )];
            menu.items.push(controller);
            game_folders.name = "game_folders_settings".into();
            game_folders.text_key = "Game Folders".into();
            game_folders.rect.y = 128.0;
            game_folders.handlers.action = vec![asset_game::MenuEvent::Script(
                "play mouse_click; close self; open options_game_folders;".into(),
            )];
            let mut community_servers = game_folders.clone();
            menu.items.push(game_folders);
            community_servers.name = "community_servers_settings".into();
            community_servers.text_key = "Community Servers".into();
            community_servers.rect.y = 148.0;
            community_servers.vis_exp = "op 38 s:75695f636f6d6d756e6974795f6d656e75 op 1".into();
            community_servers.handlers.action = vec![asset_game::MenuEvent::Script(
                "play mouse_click; close self; open options_community_servers;".into(),
            )];
            menu.items.push(community_servers);
        }

        let removed_rows: Vec<_> = menu
            .items
            .iter()
            .filter(|item| {
                item.item_type == 1
                    && matches!(
                        item.text_key.as_str(),
                        "@MENU_VOICE" | "@MENU_CHAT" | "@MENU_RESET_SYSTEM_DEFAULTS"
                    )
            })
            .map(|item| (item.rect.x, item.rect.y))
            .collect();
        menu.items.retain(|item| {
            !removed_rows.iter().any(|&(x, y)| {
                item.rect.x == x
                    && item.rect.y == y
                    && item.name != "multiplayer_settings"
                    && item.name != "controller_settings"
                    && item.name != "game_folders_settings"
                    && item.name != "community_servers_settings"
            })
        });
        if name == "pc_options_controls" {
            for item in &mut menu.items {
                if item.rect.x >= 232.0 && item.rect.y > 88.0 {
                    item.rect.y -= 20.0;
                }
            }
        }

        let Some(mode) = name.strip_prefix("settings_quick_") else {
            continue;
        };
        if sim::HostGameModeSelection::from_token(mode).is_none() {
            continue;
        }
        add_rule_rows(&mut menu.items);
        for item in &mut menu.items {
            if item.dvar == "camera_thirdperson" {
                if item.item_type == 12 {
                    item.choices.clear();
                    item.dvar.clear();
                    item.text_key = "Недоступно".into();
                }
                item.disabled_exp = "1".into();
                item.item_type = 0;
                item.static_flags |= 0x100000;
                item.handlers.action.clear();
                item.fore_color[3] = 0.4;
            } else if item.item_type == 1 && !item.dvar.is_empty() {
                item.text_scale = 0.30;
            }
        }
    }
    Ok(())
}

fn add_rule_rows(items: &mut Vec<asset_game::MenuItem>) {
    type Row = (
        &'static str,
        &'static str,
        &'static str,
        Option<Vec<(String, String)>>,
    );
    let pair = |items: &[asset_game::MenuItem], dvar: &str| {
        let button = items
            .iter()
            .position(|item| item.item_type == 1 && item.dvar == dvar)?;
        let value = items
            .iter()
            .position(|item| item.item_type == 12 && item.dvar == dvar)?;
        Some((button, value))
    };
    let counts = |max: u32| -> Vec<(String, String)> {
        (0..=max).map(|n| (n.to_string(), n.to_string())).collect()
    };
    let (Some(gameplay), Some((below_gameplay, _))) = (
        pair(items, "scr_game_onlyheadshots"),
        pair(items, "camera_thirdperson"),
    ) else {
        return;
    };
    let (left_x, top_y) = (items[gameplay.0].rect.x, items[gameplay.0].rect.y);
    let team = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.item_type == 1 && item.rect.x < left_x - 1.0 && item.rect.y >= top_y - 1.0
        })
        .filter_map(|(i, item)| pair(items, &item.dvar).filter(|(button, _)| *button == i))
        .max_by(|a, b| items[a.0].rect.y.total_cmp(&items[b.0].rect.y));
    let team_based = items.iter().any(|item| item.dvar == "scr_team_fftype");
    let mut bot_rows: Vec<Row> = vec![(
        "enemy_bots",
        "Enemy Bots:",
        sim::ENEMY_BOTS_DVAR,
        Some(counts(MAX_RULE_ENEMY_BOTS)),
    )];
    if team_based {
        bot_rows.push((
            "friendly_bots",
            "Friendly Bots:",
            sim::FRIENDLY_BOTS_DVAR,
            Some(counts(MAX_RULE_FRIENDLY_BOTS)),
        ));
    }
    let radar_rows: Vec<Row> = vec![(
        "constant_radar",
        "Constant Radar:",
        sim::CONSTANT_RADAR_DVAR,
        None,
    )];
    let mut columns = vec![(gameplay, below_gameplay, radar_rows)];
    if let Some(team) = team {
        columns.push((team, team.0, bot_rows));
    }
    for ((button, value), last, rows) in columns {
        let mut y = items[last].rect.y + items[last].rect.h;
        for (name, label, dvar, choices) in rows {
            let mut row_button = items[button].clone();
            let mut row_value = items[value].clone();
            let values = choices.as_ref().map_or_else(
                || "0 1".to_owned(),
                |choices| {
                    choices
                        .iter()
                        .map(|(_, value)| value.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            );
            row_button.name = format!("sidenav_button_{name}");
            row_button.text_key = label.into();
            row_button.text_literal = true;
            row_button.dvar = dvar.into();
            row_button.rect.y = y;
            row_button.handlers.action = vec![asset_game::MenuEvent::Script(format!(
                "play mouse_click; exec \"toggle {dvar} {values}\";"
            ))];
            row_value.dvar = dvar.into();
            row_value.rect.y = y;
            row_value.choices = choices.unwrap_or_else(|| items[gameplay.1].choices.clone());
            y += items[last].rect.h;
            items.push(row_button);
            items.push(row_value);
        }
    }
}

pub const MAX_RULE_ENEMY_BOTS: u32 = 9;
pub const MAX_RULE_FRIENDLY_BOTS: u32 = 8;
