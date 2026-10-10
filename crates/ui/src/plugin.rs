use bevy::prelude::*;
use frame::{ClientSet, register_ui_contracts};

use crate::classes::equip_txn::register_equip_systems;
use crate::classes::select::ClassSelectPlugin;
use crate::gap_hud::register_gap_hud_systems;
use crate::launch_report::publish_gap_hud;
use crate::layers::{GameUiFontPlugin, UiLayersPlugin};
use crate::loading::{poll_loading_preview, spawn_loading_screen, update_loading_screen};
use crate::menu::MenuPlugin;
use crate::screen::register_screen_systems;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        register_ui_contracts(app);
        app.add_plugins((
            GameUiFontPlugin,
            UiLayersPlugin,
            MenuPlugin,
            ClassSelectPlugin,
        ))
        .add_systems(Startup, spawn_loading_screen)
        .add_systems(
            Update,
            (
                spawn_loading_screen,
                poll_loading_preview,
                update_loading_screen,
            )
                .chain()
                .in_set(ClientSet::Ui),
        )
        .add_systems(Update, publish_gap_hud.in_set(ClientSet::Ui));
        register_screen_systems(app);
        register_equip_systems(app);
        register_gap_hud_systems(app);
        crate::menu_load::register_menu_load_systems(app);
        crate::t6_hud::register(app);
        crate::t6_menu::register(app);
        crate::launcher::register(app);
        crate::t6_text::register(app);
    }
}
