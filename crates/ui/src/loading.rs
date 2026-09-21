use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};

use crate::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

pub use assets::{LoadProgress, LoadingPreviewSource, LoadingScreen};

use crate::load_table::{LoadRow, activity_line, format_elapsed, project, total_elapsed};

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
pub(crate) struct LoadingStatusCaption;

#[derive(Component)]
pub(crate) struct LoadingStatusCount;

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
/// The two number columns reserve the same width in every row, so a number
/// growing a digit — or a size crossing from MB to GB — moves nothing.
const STATUS_VALUE_COL_PX: f32 = 104.0;
const STATUS_TIME_COL_PX: f32 = 76.0;
/// Every row is the same height, whether it says anything or not.
const STATUS_ROW_PX: f32 = 20.0;
/// At most two stages are shown running. A load opens more than that at once,
/// and a block that grew and shrank would move every row under it.
const STATUS_ACTIVE_ROWS: usize = 2;
/// The table is redrawn about ten times a second; the letters keep their own
/// per-frame animation.
const STATUS_TABLE_PERIOD: Duration = Duration::from_millis(100);
/// Every stage the load can show fits, so the box never has to grow.
const STATUS_TABLE_VH: f32 = 44.0;
const LOADING_CLEAR: Color = Color::srgb(0.08, 0.09, 0.12);
const STATUS_RUNNING: Color = Color::srgb(1.0, 0.72, 0.28);
const STATUS_TOTAL: Color = Color::srgb(0.92, 0.93, 0.95);

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
                // Pinned by its top: the box has a height of its own and fills
                // from the first line down, so a stage opening lands under the
                // rows instead of shoving them up the screen.
                row.spawn(Node {
                    width: Val::Percent(48.0),
                    height: Val::Vh(STATUS_TABLE_VH),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    justify_content: JustifyContent::FlexStart,
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
        let stage = progress.begin(assets::StageId::Preview, None);
        let decoded = match assets::decode_map_preview(&zone_ff, &map_name) {
            Ok(decoded) => decoded,
            Err(error) => {
                diag::warn!(Ui, "loading: preview index: {error}");
                stage.fail();
                return None;
            }
        };
        // A zone with no loadscreen is not a failure — the branch simply had
        // nothing to decode.
        match decoded.is_some() {
            true => stage.done(),
            false => stage.skip(),
        }
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
    load: Option<Res<assets::MapLoadProcess>>,
    mut next_table: Local<Option<std::time::Instant>>,
    mut shown: Local<StatusTable>,
    mut letters: Query<(&LoadingLetter, &mut TextColor), Without<LoadingStatusCell>>,
    list: Query<(Entity, Option<&Children>), With<LoadingStatusList>>,
    mut cells: Query<
        (&LoadingStatusCell, &mut Text, &mut TextColor),
        (Without<LoadingLetter>, Without<LoadingActivity>),
    >,
    roots: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    mut activity: Query<
        (&mut Text, &mut TextColor),
        (
            With<LoadingActivity>,
            Without<LoadingLetter>,
            Without<LoadingStatusCell>,
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

    if screen.is_complete() {
        dismiss_loading_overlay(&mut commands, roots.iter(), std::iter::empty(), true);
        diag::info!(Ui, "loading: screen dismissed");
        return;
    }

    let now = std::time::Instant::now();
    if next_table.is_some_and(|due| now < due) {
        return;
    }
    *next_table = Some(now + STATUS_TABLE_PERIOD);

    // The process is the load; the screen is one way of watching it. When both
    // are here the process wins, because it is the one a new request replaces.
    let progress = load
        .as_deref()
        .map(|process| &process.progress)
        .unwrap_or(&screen.progress);
    let snapshot = progress.snapshot();
    let table = project(&snapshot);

    for (mut text, mut color) in &mut activity {
        let (line, paint) = match screen.failure() {
            Some(reason) => (
                format!("Map load failed\n{reason}"),
                Color::srgb(1.0, 0.35, 0.25),
            ),
            None => (activity_line(&snapshot), STATUS_RUNNING),
        };
        set_text(&mut text, &line);
        color.0 = paint;
    }

    let (Ok((list_entity, children)), Some(font)) = (list.single(), font.as_ref()) else {
        return;
    };
    let mut relayout = false;
    if children.is_none_or(|children| children.is_empty())
        || shown.request != Some(snapshot.request_id)
    {
        for entity in shown.drain() {
            commands.entity(entity).try_despawn();
        }
        shown.request = Some(snapshot.request_id);
        shown.head = Some(spawn_head_row(&mut commands, &font.0));
        relayout = true;
    }

    for (stage, _) in &mut shown.active {
        if stage.is_some_and(|id| !table.running.iter().any(|row| row.id == id)) {
            *stage = None;
        }
    }
    for row in &table.running {
        if shown.active.iter().any(|(stage, _)| *stage == Some(row.id)) {
            continue;
        }
        // A stage whose group finished and then opened another scope goes on
        // being watched where it already is, never drawn twice.
        if shown.ended.iter().any(|(id, _)| *id == row.id) {
            continue;
        }
        if let Some((stage, _)) = shown.active.iter_mut().find(|(stage, _)| stage.is_none()) {
            *stage = Some(row.id);
        } else if shown.active.len() < STATUS_ACTIVE_ROWS {
            let line = StatusLine::Active(shown.active.len());
            let entity = spawn_stage_row(&mut commands, &font.0, line, row);
            shown.active.push((Some(row.id), entity));
            relayout = true;
        }
    }
    for row in &table.ended {
        if shown.ended.iter().any(|(id, _)| *id == row.id) {
            continue;
        }
        let entity = spawn_stage_row(&mut commands, &font.0, StatusLine::Ended(row.id), row);
        shown.ended.push((row.id, entity));
        relayout = true;
    }
    if relayout && let Some(head) = shown.head {
        let mut children = Vec::with_capacity(1 + shown.active.len() + shown.ended.len());
        children.push(head);
        children.extend(shown.active.iter().map(|(_, entity)| *entity));
        children.extend(shown.ended.iter().map(|(_, entity)| *entity));
        commands.entity(list_entity).replace_children(&children);
    }

    // Stages overlap, so this is shorter than their column added up.
    let elapsed = format_elapsed(total_elapsed(&snapshot));

    for (cell, mut text, mut color) in &mut cells {
        let row = match cell.line {
            StatusLine::Head => {
                set_text(
                    &mut text,
                    match cell.column {
                        StatusColumn::Name => "Progress",
                        StatusColumn::Value => "",
                        StatusColumn::Time => &elapsed,
                    },
                );
                continue;
            }
            StatusLine::Active(slot) => shown
                .active
                .get(slot)
                .and_then(|(stage, _)| *stage)
                .and_then(|id| table.running.iter().find(|row| row.id == id)),
            StatusLine::Ended(id) => table
                .ended
                .iter()
                .chain(table.running.iter())
                .find(|row| row.id == id),
        };
        let Some(row) = row else {
            set_text(&mut text, "");
            continue;
        };
        set_text(
            &mut text,
            match cell.column {
                StatusColumn::Name => &row.name,
                StatusColumn::Value => &row.value,
                StatusColumn::Time => &row.time,
            },
        );
        color.0 = row.state.color();
    }
}

/// Rewriting a `Text` that already says this would re-lay out the row.
fn set_text(text: &mut Text, value: &str) {
    if text.0 != value {
        text.0.clear();
        text.0.push_str(value);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusColumn {
    Name,
    Value,
    Time,
}

/// What the list holds right now: its first line, the fixed set of active
/// slots, and every stage that has finished, in the order they finished.
///
/// Both blocks only grow. An active slot outlives the stage in it — it is a
/// place on the screen, not a stage — and a finished row, once drawn, is never
/// moved or respawned.
#[derive(Default)]
pub(crate) struct StatusTable {
    /// A second request is a different load and gets a table of its own.
    request: Option<u64>,
    head: Option<Entity>,
    active: Vec<(Option<assets::StageId>, Entity)>,
    ended: Vec<(assets::StageId, Entity)>,
}

impl StatusTable {
    fn drain(&mut self) -> Vec<Entity> {
        self.head
            .take()
            .into_iter()
            .chain(self.active.drain(..).map(|(_, entity)| entity))
            .chain(self.ended.drain(..).map(|(_, entity)| entity))
            .collect()
    }
}

/// Which line a cell belongs to. An active slot is addressed by its place in
/// the block, because the stage standing in it changes; a finished row is
/// addressed by its stage, because it never moves again.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusLine {
    Head,
    Active(usize),
    Ended(assets::StageId),
}

#[derive(Component)]
pub(crate) struct LoadingStatusCell {
    line: StatusLine,
    column: StatusColumn,
}

/// The one line that is there from the first frame to the last, which is what
/// lets the rows under it be appended to.
fn spawn_head_row(commands: &mut Commands, font: &Handle<Font>) -> Entity {
    let entity = commands
        .spawn((
            LoadingStatusRow,
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..status_row_node()
            },
        ))
        .id();
    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            LoadingStatusCell {
                line: StatusLine::Head,
                column: StatusColumn::Name,
            },
            Text::new("Progress"),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(STATUS_TOTAL),
            status_text_layout(Justify::Left),
            loading_text_shadow(),
            Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..default()
            },
        ));
        for (column, width) in [
            (StatusColumn::Value, STATUS_VALUE_COL_PX),
            (StatusColumn::Time, STATUS_TIME_COL_PX),
        ] {
            parent.spawn((
                LoadingStatusCell {
                    line: StatusLine::Head,
                    column,
                },
                Text::new(""),
                game_text_font(font, STATUS_FONT_PX),
                TextColor(STATUS_TOTAL),
                status_text_layout(Justify::Right),
                loading_text_shadow(),
                status_cell_node(width),
            ));
        }
    });
    entity
}

fn status_row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Px(STATUS_ROW_PX),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        ..default()
    }
}

fn spawn_stage_row(
    commands: &mut Commands,
    font: &Handle<Font>,
    line: StatusLine,
    row: &LoadRow,
) -> Entity {
    let color = row.state.color();
    let entity = commands.spawn((LoadingStatusRow, status_row_node())).id();
    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            LoadingStatusCaption,
            LoadingStatusCell {
                line,
                column: StatusColumn::Name,
            },
            Text::new(row.name.clone()),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(color),
            status_text_layout(Justify::Left),
            loading_text_shadow(),
            Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..default()
            },
        ));
        parent.spawn((
            LoadingStatusCount,
            LoadingStatusCell {
                line,
                column: StatusColumn::Value,
            },
            Text::new(row.value.clone()),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(color),
            status_text_layout(Justify::Right),
            loading_text_shadow(),
            status_cell_node(STATUS_VALUE_COL_PX),
        ));
        parent.spawn((
            LoadingStatusTime,
            LoadingStatusCell {
                line,
                column: StatusColumn::Time,
            },
            Text::new(row.time.clone()),
            game_text_font(font, STATUS_FONT_PX),
            TextColor(color),
            status_text_layout(Justify::Right),
            loading_text_shadow(),
            status_cell_node(STATUS_TIME_COL_PX),
        ));
    });
    entity
}

/// A wrapped cell is a taller box in a row of fixed height, and a taller box
/// centres its first line higher than its neighbours'.
fn status_text_layout(justify: Justify) -> TextLayout {
    TextLayout::new(justify, bevy::text::LineBreak::NoWrap)
}

fn status_cell_node(width: f32) -> Node {
    Node {
        width: Val::Px(width),
        min_width: Val::Px(width),
        flex_shrink: 0.0,
        justify_content: JustifyContent::FlexEnd,
        ..default()
    }
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
