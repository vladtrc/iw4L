use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};

use crate::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

pub use assets::{LoadLaneView, LoadProgress, LoadingPreviewSource, LoadingScreen};

#[derive(Component)]
pub(crate) struct LoadingRoot;

#[derive(Component)]
pub(crate) struct LoadingOverlayTitle(String);

#[derive(Component)]
pub(crate) struct LoadingCamera;

#[derive(Component)]
pub(crate) struct OverlayUiCamera;

fn spawn_overlay_ui_camera(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 200,

            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        IsDefaultUiCamera,
        OverlayUiCamera,
    ));
}

pub(crate) fn dismiss_loading_overlay(
    commands: &mut Commands,
    chrome: impl IntoIterator<Item = Entity>,
    overlay_cams: impl IntoIterator<Item = Entity>,
    spawn_console_camera: bool,
) {
    for entity in chrome {
        commands.entity(entity).try_despawn();
    }
    let mut overlay_n = 0usize;
    for entity in overlay_cams {
        overlay_n += 1;
        if !spawn_console_camera {
            commands.entity(entity).try_despawn();
        }
    }
    commands.remove_resource::<LoadingScreen>();
    commands.remove_resource::<LoadingPreviewTask>();
    commands.remove_resource::<LoadingPreviewSource>();
    if spawn_console_camera && overlay_n == 0 {
        spawn_overlay_ui_camera(commands);
    }
}

#[derive(Component)]
pub(crate) struct LoadingLetter(usize);

#[derive(Component)]
pub(crate) struct LoadingStatusList;

#[derive(Component)]
pub(crate) struct LoadingActivity;

#[derive(Component)]
pub(crate) struct LoadingStatusRow;

#[derive(Component)]
pub(crate) struct LoadingStatusLane(u64);

#[derive(Component)]
pub(crate) struct LoadingStatusCaption;

#[derive(Component)]
pub(crate) struct LoadingStatusTime;

#[derive(Resource)]
pub(crate) struct LoadingPreviewTask {
    identity: PreviewIdentity,
    task: Task<Option<(u32, u32, Vec<u8>)>>,
}

#[derive(Clone, PartialEq, Eq)]
struct PreviewIdentity {
    request_id: u64,
    map_name: String,
}

impl PreviewIdentity {
    fn matches(&self, source: &LoadingPreviewSource) -> bool {
        self.request_id == source.request_id && self.map_name == source.map_name
    }
}

const LETTER_STEP: Duration = Duration::from_millis(140);
const LETTER_FONT_PX: f32 = 56.0;
const LETTER_FONT_MIN: f32 = 28.0;
const MODE_FONT_PX: f32 = 22.0;
const STATUS_FONT_PX: f32 = 14.0;
const STATUS_TIME_COL_PX: f32 = 64.0;
const LOADING_CLEAR: Color = Color::srgb(0.08, 0.09, 0.12);
const STATUS_RUNNING: Color = Color::srgb(1.0, 0.72, 0.28);
const STATUS_DONE: Color = Color::srgb(0.62, 0.92, 0.68);

fn loading_text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(1.5),
        color: Color::linear_rgba(0., 0., 0., 0.75),
        ..default()
    }
}

pub(crate) fn spawn_loading_screen(
    mut commands: Commands,
    screen: Option<Res<LoadingScreen>>,
    font: Option<Res<GameUiFont>>,
    zone_ff: Option<Res<LoadingPreviewSource>>,
    existing: Query<(Entity, Option<&LoadingOverlayTitle>), With<LoadingRoot>>,
    cameras: Query<Entity, With<LoadingCamera>>,
    leftover_overlay: Query<Entity, With<OverlayUiCamera>>,
    inflight_preview: Option<Res<LoadingPreviewTask>>,
) {
    let Some(screen) = screen else {
        return;
    };
    let title = screen.title();
    let title_ok = existing
        .iter()
        .any(|(_, spawned)| spawned.is_some_and(|s| s.0 == title));
    if title_ok && leftover_overlay.is_empty() {
        return;
    }
    let Some(font) = font else {
        diag::warn!(Ui, "loading: GameUiFont missing; loading screen skipped");
        return;
    };
    let font = font.0.clone();

    let stale: Vec<Entity> = existing
        .iter()
        .map(|(entity, _)| entity)
        .chain(cameras.iter())
        .chain(leftover_overlay.iter())
        .collect();
    for entity in stale {
        commands.entity(entity).try_despawn();
    }

    commands.spawn((
        Camera2d,
        Camera {
            order: 100,
            clear_color: ClearColorConfig::Custom(LOADING_CLEAR),
            ..default()
        },
        IsDefaultUiCamera,
        LoadingCamera,
    ));

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),

                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(LOADING_CLEAR),
            GlobalZIndex(10_000),
            UiLayer::Loading,
            UiLayerVisibility,
            Visibility::Inherited,
            LoadingRoot,
            LoadingOverlayTitle(title.to_owned()),
        ))
        .with_children(|root| {
            root.spawn(Node {
                width: Val::Percent(78.0),
                margin: UiRect {
                    bottom: Val::Percent(10.0),
                    ..default()
                },
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexEnd,
                column_gap: Val::Px(48.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    width: Val::Percent(48.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|left| {
                    left.spawn((
                        Text::default(),
                        TextLayout::justify(Justify::Left),
                        loading_text_shadow(),
                    ))
                    .with_children(|text| {
                        let letter_px = letter_font_px(screen.title());
                        for (index, letter) in screen.title().chars().enumerate() {
                            text.spawn((
                                TextSpan::new(letter.to_string()),
                                game_text_font(&font, letter_px),
                                TextColor(Color::srgb(0.42, 0.44, 0.47)),
                                LoadingLetter(index),
                            ));
                        }
                    });
                    left.spawn((
                        Text::new(screen.mode_label().to_owned()),
                        game_text_font(&font, MODE_FONT_PX),
                        TextColor(Color::srgb(0.92, 0.93, 0.95)),
                        TextLayout::justify(Justify::Left),
                        loading_text_shadow(),
                    ));
                });
                row.spawn(Node {
                    width: Val::Percent(48.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    justify_content: JustifyContent::FlexEnd,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|right| {
                    right.spawn((
                        LoadingActivity,
                        Text::new("Loading"),
                        game_text_font(&font, STATUS_FONT_PX),
                        TextColor(STATUS_RUNNING),
                        loading_text_shadow(),
                    ));
                    right.spawn((
                        LoadingStatusList,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Stretch,
                            row_gap: Val::Px(2.0),
                            ..default()
                        },
                    ));
                });
            });
        });

    diag::info!(
        Ui,
        "loading: screen spawned (title=`{}`, mode=`{}`)",
        screen.title(),
        screen.mode_label()
    );
    if let Some(source) = zone_ff {
        let same = inflight_preview
            .as_deref()
            .is_some_and(|task| task.identity.matches(&source));
        if !same {
            begin_loading_preview_decode(
                &mut commands,
                source.path.clone(),
                source.map_name.clone(),
                source.request_id,
                screen.progress.clone(),
            );
        }
    }
}

fn begin_loading_preview_decode(
    commands: &mut Commands,
    zone_ff: std::path::PathBuf,
    map_name: String,
    request_id: u64,
    progress: LoadProgress,
) {
    let identity = PreviewIdentity {
        request_id,
        map_name: map_name.clone(),
    };

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let stage = progress.stage("decoding loadscreen image");
        let decoded = match assets::decode_map_preview(&zone_ff, &map_name) {
            Ok(decoded) => decoded,
            Err(error) => {
                diag::warn!(Ui, "loading: preview index: {error}");
                None
            }
        };
        drop(stage);
        decoded
    });
    commands.insert_resource(LoadingPreviewTask { identity, task });
}

pub(crate) fn poll_loading_preview(
    mut commands: Commands,
    mut task: Option<ResMut<LoadingPreviewTask>>,
    mut loading: Option<ResMut<LoadingScreen>>,
    source: Option<Res<LoadingPreviewSource>>,
    mut images: ResMut<Assets<Image>>,
    loading_root: Query<Entity, With<LoadingRoot>>,
) {
    let Some(task) = task.as_deref_mut() else {
        return;
    };
    let Some(decoded) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    let identity = task.identity.clone();
    commands.remove_resource::<LoadingPreviewTask>();
    let Some(source) = source.as_deref() else {
        return;
    };
    if !identity.matches(source) {
        return;
    }
    let Some(loading) = loading.as_deref_mut() else {
        return;
    };
    match (decoded, loading_root.single()) {
        (Some((width, height, pixels)), Ok(root)) => {
            let size = bevy::render::render_resource::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let mut image = Image::new(
                size,
                bevy::render::render_resource::TextureDimension::D2,
                pixels,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            image.sampler = bevy::image::ImageSampler::linear();
            let handle = images.add(image);
            commands
                .entity(root)
                .insert(ImageNode::new(handle).with_mode(bevy::ui::widget::NodeImageMode::Stretch));
            loading.mark_preview_ready();
            diag::info!(Ui, "loading: map preview ready ({width}x{height})");
        }
        (None, _) => {
            diag::info!(Ui, "loading: no map preview in IWD (keeping solid color)");
        }
        (_, Err(_)) => {}
    }
}

pub(crate) fn update_loading_screen(
    mut commands: Commands,
    time: Res<Time>,
    font: Option<Res<GameUiFont>>,
    mut screen: Option<ResMut<LoadingScreen>>,
    mut letters: Query<(&LoadingLetter, &mut TextColor), Without<LoadingStatusCaption>>,
    list: Query<(Entity, Option<&Children>), (With<LoadingStatusList>, Without<LoadingStatusRow>)>,
    rows: Query<
        (Entity, &LoadingStatusLane, &Children),
        (With<LoadingStatusRow>, Without<LoadingStatusList>),
    >,
    mut texts: ParamSet<(
        Query<&mut Text, (With<LoadingStatusCaption>, Without<LoadingActivity>)>,
        Query<&mut Text, (With<LoadingStatusTime>, Without<LoadingActivity>)>,
    )>,
    mut status_color: Query<
        &mut TextColor,
        (
            Without<LoadingLetter>,
            Or<(With<LoadingStatusCaption>, With<LoadingStatusTime>)>,
        ),
    >,
    roots: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    mut activity: Query<
        (&mut Text, &mut TextColor),
        (
            With<LoadingActivity>,
            Without<LoadingLetter>,
            Without<LoadingStatusCaption>,
            Without<LoadingStatusTime>,
        ),
    >,
) {
    let Some(screen) = screen.as_deref_mut() else {
        return;
    };

    screen.tick_elapsed(time.delta());

    let count = letters.iter().count().max(1);
    let step = (screen.elapsed().as_millis() / LETTER_STEP.as_millis()) as usize;
    let active = ping_pong(step, count);
    let inactive = if screen.preview_ready() {
        Color::srgb(0.96, 0.96, 0.97)
    } else {
        Color::srgb(0.42, 0.44, 0.47)
    };
    for (letter, mut color) in &mut letters {
        color.0 = if letter.0 == active {
            Color::srgb(1.0, 0.62, 0.12)
        } else {
            inactive
        };
    }
    let lanes = screen.progress.snapshot_lanes();
    let active = lanes.iter().filter(|lane| lane.in_progress()).count();
    for (mut text, mut color) in &mut activity {
        if let Some(reason) = screen.failure() {
            *text = Text::new(format!("Map load failed\n{reason}"));
            color.0 = Color::srgb(1.0, 0.35, 0.25);
        } else {
            *text = Text::new(format!("Loading · {active} active stages"));
            color.0 = STATUS_RUNNING;
        }
    }
    if let (Ok((list_entity, children)), Some(font)) = (list.single(), font.as_ref()) {
        let child_ids: Vec<Entity> = children.map(|c| c.iter().collect()).unwrap_or_default();
        let mut ordered = Vec::with_capacity(lanes.len());
        for lane in &lanes {
            if let Some(row) = child_ids.iter().copied().find(|&entity| {
                rows.get(entity)
                    .map(|(_, id, _)| id.0 == lane.id)
                    .unwrap_or(false)
            }) {
                paint_status_row(&rows, &mut texts, &mut status_color, row, lane);
                ordered.push(row);
            } else {
                ordered.push(spawn_status_row(&mut commands, &font.0, lane));
            }
        }
        if child_ids != ordered {
            commands.entity(list_entity).replace_children(&ordered);
        }
        for entity in child_ids {
            if !ordered.contains(&entity) {
                commands.entity(entity).try_despawn();
            }
        }
    }

    if screen.is_complete() {
        dismiss_loading_overlay(&mut commands, roots.iter(), std::iter::empty(), true);
        diag::info!(Ui, "loading: screen dismissed");
    }
}

fn paint_status_row(
    rows: &Query<
        (Entity, &LoadingStatusLane, &Children),
        (With<LoadingStatusRow>, Without<LoadingStatusList>),
    >,
    texts: &mut ParamSet<(
        Query<&mut Text, (With<LoadingStatusCaption>, Without<LoadingActivity>)>,
        Query<&mut Text, (With<LoadingStatusTime>, Without<LoadingActivity>)>,
    )>,
    status_color: &mut Query<
        &mut TextColor,
        (
            Without<LoadingLetter>,
            Or<(With<LoadingStatusCaption>, With<LoadingStatusTime>)>,
        ),
    >,
    row: Entity,
    lane: &LoadLaneView,
) {
    let color = if lane.in_progress() {
        STATUS_RUNNING
    } else {
        STATUS_DONE
    };
    let Ok((_, _, kids)) = rows.get(row) else {
        return;
    };
    let kids: Vec<Entity> = kids.iter().collect();
    if let Some(&cap) = kids.first() {
        if let Ok(mut text) = texts.p0().get_mut(cap) {
            *text = Text::new(lane.caption());
        }
        if let Ok(mut paint) = status_color.get_mut(cap) {
            paint.0 = color;
        }
    }
    if let Some(&tm) = kids.get(1) {
        if let Ok(mut text) = texts.p1().get_mut(tm) {
            *text = Text::new(lane.elapsed_ms_label().unwrap_or_default());
        }
        if let Ok(mut paint) = status_color.get_mut(tm) {
            paint.0 = color;
        }
    }
}

fn spawn_status_row(commands: &mut Commands, font: &Handle<Font>, lane: &LoadLaneView) -> Entity {
    let color = if lane.in_progress() {
        STATUS_RUNNING
    } else {
        STATUS_DONE
    };
    let time = lane.elapsed_ms_label().unwrap_or_default();
    let row = commands
        .spawn((
            LoadingStatusRow,
            LoadingStatusLane(lane.id),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Baseline,
                column_gap: Val::Px(12.0),
                ..default()
            },
        ))
        .id();
    commands.entity(row).with_children(|row| {
        row.spawn((
            LoadingStatusCaption,
            Text::new(lane.caption()),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(color),
            TextLayout::justify(Justify::Left),
            loading_text_shadow(),
            Node {
                flex_shrink: 1.0,
                ..default()
            },
        ));
        row.spawn((
            LoadingStatusTime,
            Text::new(time),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(color),
            TextLayout::justify(Justify::Right),
            loading_text_shadow(),
            Node {
                min_width: Val::Px(STATUS_TIME_COL_PX),
                flex_shrink: 0.0,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
        ));
    });
    row
}

fn ping_pong(step: usize, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let period = 2 * (count - 1);
    let phase = step % period;
    if phase < count { phase } else { period - phase }
}

fn letter_font_px(title: &str) -> f32 {
    let n = title.chars().count().max(1) as f32;
    (LETTER_FONT_PX * 10.0 / n).clamp(LETTER_FONT_MIN, LETTER_FONT_PX)
}
