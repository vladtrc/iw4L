use std::time::Instant;

use bevy::{camera::visibility::VisibilitySystems, prelude::*, ui::UiSystems};

const GAME_UI_FONT_BYTES: &[u8] = include_bytes!("../assets/Oxanium-Regular.ttf");

#[derive(Resource, Clone, Debug)]
pub struct GameUiFont(pub Handle<Font>);

pub(crate) struct GameUiFontPlugin;

impl Plugin for GameUiFontPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_game_ui_font);
    }
}

fn load_game_ui_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let handle = fonts.add(Font::from_bytes(GAME_UI_FONT_BYTES.to_vec()));
    commands.insert_resource(GameUiFont(handle));
}

pub fn game_text_font(font: &Handle<Font>, size_px: f32) -> TextFont {
    TextFont {
        font: font.clone().into(),
        font_size: bevy::text::FontSize::Px(size_px),
        ..default()
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiLayer {
    Hud,

    Debug,

    Shell,

    Loading,

    Overlay,
}

impl UiLayer {
    const COUNT: usize = 5;

    const fn index(self) -> usize {
        match self {
            Self::Hud => 0,
            Self::Debug => 1,
            Self::Shell => 2,
            Self::Loading => 3,
            Self::Overlay => 4,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct UiLayerVisibility;

pub use frame::UiDraw;

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct UiLayers {
    visible: [bool; UiLayer::COUNT],
}

impl Default for UiLayers {
    fn default() -> Self {
        Self {
            visible: [false; UiLayer::COUNT],
        }
    }
}

impl UiLayers {
    pub fn show_only(&mut self, layers: impl IntoIterator<Item = UiLayer>) {
        let mut visible = [false; UiLayer::COUNT];
        for layer in layers {
            visible[layer.index()] = true;
        }
        if self.visible != visible {
            self.visible = visible;
        }
    }

    pub fn is_visible(&self, layer: UiLayer) -> bool {
        self.visible[layer.index()]
    }
}

pub(crate) struct UiLayersPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyUiLayers;

#[derive(Resource, Debug, Default, Clone)]
pub struct UiLayerCpu {
    pub ms: Option<f32>,
    started: Option<Instant>,
}

fn begin_ui_layers(mut cpu: ResMut<UiLayerCpu>) {
    cpu.started = Some(Instant::now());
}

fn end_ui_layers(mut cpu: ResMut<UiLayerCpu>) {
    let Some(started) = cpu.started else {
        return;
    };
    cpu.ms = Some(started.elapsed().as_secs_f32() * 1000.0);
}

impl Plugin for UiLayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiLayers>()
            .init_resource::<UiDraw>()
            .init_resource::<UiLayerCpu>()
            .configure_sets(
                PostUpdate,
                ApplyUiLayers
                    .before(UiSystems::Prepare)
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(
                PostUpdate,
                begin_ui_layers.before(crate::screen::sync_ui_layers),
            )
            .add_systems(
                PostUpdate,
                end_ui_layers
                    .after(apply_ui_layers)
                    .before(UiSystems::Prepare),
            )
            .add_systems(PostUpdate, apply_ui_layers.in_set(ApplyUiLayers));
    }
}

#[allow(clippy::type_complexity)]
fn apply_ui_layers(
    layers: Res<UiLayers>,
    mut roots: Query<(
        &UiLayer,
        Option<&mut Node>,
        Option<&mut Visibility>,
        Option<&UiLayerVisibility>,
        Option<&mut Camera>,
    )>,
) {
    for (layer, node, visibility, layer_vis, camera) in &mut roots {
        let visible = layers.is_visible(*layer);
        if let Some(mut node) = node {
            let want = if visible {
                Display::Flex
            } else {
                Display::None
            };

            if node.display != want {
                node.display = want;
            }
        }

        if layer_vis.is_some()
            && let Some(mut visibility) = visibility
        {
            let want = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != want {
                *visibility = want;
            }
        }
        if let Some(mut camera) = camera {
            if camera.is_active != visible {
                camera.is_active = visible;
            }
        }
    }
}
