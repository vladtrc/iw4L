use bevy::prelude::*;

use assets::LoadingScreen;
use frame::AppScreen;

use crate::layers::{ApplyUiLayers, UiDraw, UiLayer, UiLayers};

pub fn layers_for_screen(screen: AppScreen, ui_draw: bool, loading: bool) -> &'static [UiLayer] {
    if loading {
        &[UiLayer::Loading, UiLayer::Overlay]
    } else {
        match (screen, ui_draw) {
            (AppScreen::MainMenu, _) => &[UiLayer::Shell, UiLayer::Overlay],
            (AppScreen::Loading, _) => &[UiLayer::Loading, UiLayer::Overlay],
            (AppScreen::ClassSelect, true) => &[UiLayer::Hud, UiLayer::Overlay],
            (AppScreen::ClassSelect, false) => &[UiLayer::Overlay],
            (AppScreen::InGame, true) => &[UiLayer::Hud, UiLayer::Debug, UiLayer::Overlay],
            (AppScreen::InGame, false) => &[UiLayer::Overlay],
        }
    }
}

pub fn sync_ui_layers(
    screen: Res<AppScreen>,
    ui_draw: Res<UiDraw>,
    loading: Option<Res<LoadingScreen>>,
    mut layers: ResMut<UiLayers>,
    menu: Res<crate::MenuEnabled>,
) {
    let set = layers_for_screen(*screen, ui_draw.0, loading.is_some());
    layers.show_only(
        set.iter()
            .copied()
            .chain((menu.0 && *screen == AppScreen::InGame).then_some(UiLayer::Shell)),
    );
}

pub(crate) fn register_screen_systems(app: &mut App) {
    app.init_resource::<AppScreen>()
        .add_systems(Startup, sync_ui_layers)
        .add_systems(PostUpdate, sync_ui_layers.before(ApplyUiLayers));
}
