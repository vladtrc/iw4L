use bevy::prelude::*;
use frame::ClientSet;

use assets::LoadingScreen;

use crate::layers::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};
use crate::menu::MenuEnabled;
use frame::AppScreen;

#[derive(Resource, Clone, Debug)]
pub struct GapHud {
    pub title: String,
    pub body: Vec<String>,
}

impl Default for GapHud {
    fn default() -> Self {
        Self {
            title: "iw4l".into(),
            body: vec!["waiting for launch status…".into()],
        }
    }
}

#[derive(Component)]
struct GapHudRoot;

#[derive(Component)]
struct GapTitleText;

#[derive(Component)]
struct GapBodyText;

fn gap_should_show(
    loading: Option<Res<LoadingScreen>>,
    menu: Res<MenuEnabled>,
    screen: Res<AppScreen>,
) -> bool {
    if loading.is_some() || menu.0 {
        return false;
    }
    !matches!(*screen, AppScreen::ClassSelect | AppScreen::InGame)
}

fn spawn_gap_ui(
    mut commands: Commands,
    hud: Res<GapHud>,
    font: Option<Res<GameUiFont>>,
    loading: Option<Res<LoadingScreen>>,
    menu: Res<MenuEnabled>,
    screen: Res<AppScreen>,
    existing: Query<Entity, With<GapHudRoot>>,
) {
    if !existing.is_empty() {
        return;
    }
    if !gap_should_show(loading, menu, screen) {
        return;
    }
    let text_font = font
        .as_ref()
        .map(|f| game_text_font(&f.0, 16.0))
        .unwrap_or_else(|| TextFont {
            font_size: bevy::text::FontSize::Px(16.0),
            ..default()
        });
    let title_font = font
        .as_ref()
        .map(|f| game_text_font(&f.0, 28.0))
        .unwrap_or_else(|| TextFont {
            font_size: bevy::text::FontSize::Px(28.0),
            ..default()
        });

    commands
        .spawn((
            GapHudRoot,
            UiLayer::Debug,
            UiLayerVisibility,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(24.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.06, 0.08)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(hud.title.clone()),
                title_font,
                TextColor(Color::srgb(0.9, 0.9, 0.85)),
                GapTitleText,
            ));
            parent.spawn((
                Text::new(hud.body.join("\n")),
                text_font,
                TextColor(Color::srgb(0.75, 0.78, 0.7)),
                GapBodyText,
            ));
        });
}

fn refresh_gap_ui(
    hud: Res<GapHud>,
    mut titles: Query<&mut Text, (With<GapTitleText>, Without<GapBodyText>)>,
    mut bodies: Query<&mut Text, (With<GapBodyText>, Without<GapTitleText>)>,
) {
    if !hud.is_changed() {
        return;
    }
    for mut text in &mut titles {
        *text = Text::new(hud.title.clone());
    }
    for mut text in &mut bodies {
        *text = Text::new(hud.body.join("\n"));
    }
}

fn despawn_gap_when_hidden(
    mut commands: Commands,
    loading: Option<Res<LoadingScreen>>,
    menu: Res<MenuEnabled>,
    screen: Res<AppScreen>,
    roots: Query<Entity, With<GapHudRoot>>,
) {
    if gap_should_show(loading, menu, screen) {
        return;
    }
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn register_gap_hud_systems(app: &mut App) {
    app.init_resource::<GapHud>().add_systems(
        Update,
        (
            spawn_gap_ui,
            refresh_gap_ui,
            despawn_gap_when_hidden.after(spawn_gap_ui),
        )
            .in_set(ClientSet::Ui),
    );
}
