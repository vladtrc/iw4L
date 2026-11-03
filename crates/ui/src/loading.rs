use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};

use crate::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

pub use asset_transport::LoadProgress;
pub use assets::{LoadingPreviewSource, LoadingScreen};

use crate::load_table::{RowState, format_elapsed, project, total_elapsed};

#[derive(Component)]
pub(crate) struct LoadingRoot;

#[derive(Component)]
pub(crate) struct LoadingOverlayTitle(String, u64);

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
        crate::UiCamera,
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
    publication_id: u64,
    map_name: String,
}

impl PreviewIdentity {
    fn matches(&self, source: &LoadingPreviewSource) -> bool {
        self.map_name == source.map_name
    }
}

const LETTER_STEP: Duration = Duration::from_millis(140);
const LETTER_FONT_PX: f32 = 56.0;
const LETTER_FONT_MIN: f32 = 28.0;
const MODE_FONT_PX: f32 = 22.0;
const WORLD_FONT_PX: f32 = 26.0;
const STATUS_FONT_PX: f32 = 14.0;
const STATUS_NUMBER_COL_PX: f32 = 64.0;
const STATUS_NAME_GAP_PX: f32 = 28.0;
const STATUS_UNIT_COL_PX: f32 = 124.0;
/// Every row is the same height, whether it says anything or not.
const STATUS_ROW_PX: f32 = 20.0;
const STATUS_SLOTS: usize = 6;
/// The table is redrawn about ten times a second; the letters keep their own
/// per-frame animation.
const STATUS_TABLE_PERIOD: Duration = Duration::from_millis(100);
const LOADING_CLEAR: Color = Color::srgb(0.08, 0.09, 0.12);
const STATUS_TOTAL: Color = Color::srgb(0.92, 0.93, 0.95);
const STATUS_FAILED: Color = Color::srgb(1.0, 0.35, 0.25);

fn loading_text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(1.5),
        color: Color::linear_rgba(0., 0., 0., 0.75),
        ..default()
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct LoadingPreviewRequest<'w> {
    source: Option<Res<'w, LoadingPreviewSource>>,
    inflight: Option<Res<'w, LoadingPreviewTask>>,
    publication: Option<Res<'w, asset_material::UiImagePublication>>,
}

impl LoadingPreviewRequest<'_> {
    fn publication_id(&self) -> u64 {
        self.publication
            .as_ref()
            .map_or(0, |publication| publication.id())
    }

    fn start(&self, commands: &mut Commands, progress: &LoadProgress) {
        if let Some(source) = self.source.as_deref()
            && let Some(publication) = self.publication.as_deref()
        {
            let same = self.inflight.as_deref().is_some_and(|task| {
                task.identity.matches(source) && task.identity.publication_id == publication.id()
            });
            if !same {
                begin_loading_preview_decode(
                    commands,
                    source,
                    progress.clone(),
                    publication.clone(),
                );
            }
        }
    }
}

pub(crate) fn spawn_loading_screen(
    mut commands: Commands,
    screen: Option<Res<LoadingScreen>>,
    font: Option<Res<GameUiFont>>,
    preview: LoadingPreviewRequest,
    existing: Query<(Entity, Option<&LoadingOverlayTitle>), With<LoadingRoot>>,
    cameras: Query<Entity, With<LoadingCamera>>,
    leftover_overlay: Query<Entity, With<OverlayUiCamera>>,
) {
    let Some(screen) = screen else {
        return;
    };
    let title = screen.title();
    let title_ok = existing
        .iter()
        .any(|(_, spawned)| spawned.is_some_and(|s| s.0 == title));
    let publication_id = preview.publication_id();
    let owner_ok = existing
        .iter()
        .any(|(_, spawned)| spawned.is_some_and(|s| s.1 == publication_id));
    if title_ok && owner_ok && leftover_overlay.is_empty() {
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
        crate::UiCamera,
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
            LoadingOverlayTitle(title.to_owned(), publication_id),
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
                    // A merged map's worlds, stacked upward from world 0 as
                    // the preview's bands are.
                    for world in screen.worlds().iter().rev() {
                        left.spawn((
                            Text::new(world.clone()),
                            game_text_font(&font, WORLD_FONT_PX),
                            TextColor(Color::srgb(0.72, 0.74, 0.77)),
                            TextLayout::justify(Justify::Left),
                            loading_text_shadow(),
                        ));
                    }
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
                        LoadingStatusList,
                        StatusTable::default(),
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
    preview.start(&mut commands, &screen.progress);
}

fn begin_loading_preview_decode(
    commands: &mut Commands,
    source: &LoadingPreviewSource,
    progress: LoadProgress,
    publication: asset_material::UiImagePublication,
) {
    let identity = PreviewIdentity {
        publication_id: publication.id(),
        map_name: source.map_name.clone(),
    };
    let worlds: Vec<(std::path::PathBuf, String)> =
        std::iter::once((source.path.clone(), source.map_name.clone()))
            .chain(source.more.iter().cloned())
            .collect();

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let stage = progress.begin(asset_transport::StageId::Preview, None);
        let mut previews = Vec::with_capacity(worlds.len());
        for (zone_ff, map_name) in &worlds {
            match asset_material::decode_map_preview(&publication, zone_ff, map_name) {
                Ok(decoded) => previews.push(decoded),
                Err(error) => {
                    diag::warn!(Ui, "loading: preview index: {error}");
                    stage.fail();
                    return None;
                }
            }
        }
        let decoded = match previews.len() {
            1 => previews.pop().flatten(),
            _ => stack_previews(&previews),
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

/// A merged map's previews as equal horizontal bands, world 0 at the bottom:
/// each band shows the middle of its world's preview at the size of the
/// largest. A world with no preview leaves its band the loading color.
fn stack_previews(previews: &[Option<(u32, u32, Vec<u8>)>]) -> Option<(u32, u32, Vec<u8>)> {
    let width = previews.iter().flatten().map(|p| p.0).max()?;
    let height = previews.iter().flatten().map(|p| p.1).max()?;
    let bands = previews.len() as u32;
    let clear = LOADING_CLEAR.to_srgba().to_u8_array();
    let mut pixels = clear.repeat((width * height) as usize);
    for y in 0..height {
        let band = (y * bands / height).min(bands - 1);
        let Some((sw, sh, source)) = &previews[(bands - 1 - band) as usize] else {
            continue;
        };
        // The band's row as a fraction of the band, mapped onto the source's
        // middle slice of the band's aspect.
        let band_top = band * height / bands;
        let band_rows = (band + 1) * height / bands - band_top;
        let slice = (f64::from(*sh) / f64::from(bands)).max(1.0);
        let slice_top = (f64::from(*sh) - slice) / 2.0;
        let along = (f64::from(y - band_top) + 0.5) / f64::from(band_rows.max(1));
        let sy = (slice_top + along * slice - 0.5).clamp(0.0, f64::from(sh - 1));
        for x in 0..width {
            let sx = ((f64::from(x) + 0.5) * f64::from(*sw) / f64::from(width) - 0.5)
                .clamp(0.0, f64::from(sw - 1));
            let to = ((y * width + x) * 4) as usize;
            pixels[to..to + 4].copy_from_slice(&bilinear(source, *sw, *sh, sx, sy));
        }
    }
    Some((width, height, pixels))
}

fn bilinear(pixels: &[u8], width: u32, height: u32, x: f64, y: f64) -> [u8; 4] {
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - f64::from(x0), y - f64::from(y0));
    let at = |x: u32, y: u32, c: usize| f64::from(pixels[((y * width + x) * 4) as usize + c]);
    std::array::from_fn(|c| {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        (top * (1.0 - fy) + bottom * fy).round() as u8
    })
}

pub(crate) fn poll_loading_preview(
    mut commands: Commands,
    mut task: Option<ResMut<LoadingPreviewTask>>,
    mut loading: Option<ResMut<LoadingScreen>>,
    source: Option<Res<LoadingPreviewSource>>,
    publication: Option<Res<asset_material::UiImagePublication>>,
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
    if !identity.matches(source)
        || publication
            .as_ref()
            .is_none_or(|publication| publication.id() != identity.publication_id)
    {
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct LoadingScreenView<'w, 's> {
    letters:
        Query<'w, 's, (&'static LoadingLetter, &'static mut TextColor), Without<LoadingStatusCell>>,
    list: Query<
        'w,
        's,
        (Entity, Option<&'static Children>, &'static mut StatusTable),
        With<LoadingStatusList>,
    >,
    cells: Query<
        'w,
        's,
        (
            &'static LoadingStatusCell,
            &'static mut Text,
            &'static mut TextColor,
        ),
        Without<LoadingLetter>,
    >,
}

impl LoadingScreenView<'_, '_> {
    fn animate(&mut self, screen: &LoadingScreen) {
        let count = self.letters.iter().count().max(1);
        let step = (screen.elapsed().as_millis() / LETTER_STEP.as_millis()) as usize;
        let active = ping_pong(step, count);
        let inactive = if screen.preview_ready() {
            Color::srgb(0.96, 0.96, 0.97)
        } else {
            Color::srgb(0.42, 0.44, 0.47)
        };
        for (letter, mut color) in &mut self.letters {
            color.0 = if letter.0 == active {
                Color::srgb(1.0, 0.62, 0.12)
            } else {
                inactive
            };
        }
    }

    fn refresh(
        &mut self,
        commands: &mut Commands,
        screen: &LoadingScreen,
        font: Option<&GameUiFont>,
        load: Option<&assets::MapLoadProcess>,
    ) {
        let (Ok((list_entity, children, mut shown)), Some(font)) = (self.list.single_mut(), font)
        else {
            return;
        };
        let now = std::time::Instant::now();
        if shown.next_refresh.is_some_and(|due| now < due) {
            return;
        }
        shown.next_refresh = Some(now + STATUS_TABLE_PERIOD);

        // The process is the load; the screen is one way of watching it. When both
        // are here the process wins, because it is the one a new request replaces.
        let progress = load
            .map(|process| &process.progress)
            .unwrap_or(&screen.progress);
        let snapshot = progress.snapshot();
        let table = project(&snapshot);

        if children.is_none_or(|children| children.is_empty()) {
            for entity in shown.drain() {
                commands.entity(entity).try_despawn();
            }
            let mut children = Vec::with_capacity(2 + STATUS_SLOTS);
            // Replacing children only detaches old rows, and a detached row stays on
            // screen as its own UI root.
            let head = spawn_metric_row(commands, &font.0, StatusLine::Head);
            let spacer = spawn_spacer_row(commands);
            shown.head = Some(head);
            shown.spacer = Some(spacer);
            children.extend([head, spacer]);
            for slot in 0..STATUS_SLOTS {
                let entity = spawn_metric_row(commands, &font.0, StatusLine::Slot(slot));
                shown.slots.push((None, entity));
                children.push(entity);
            }
            commands.entity(list_entity).replace_children(&children);
        }

        for row in table.running.iter().chain(table.ended.iter()) {
            if shown.slots.iter().any(|(id, _)| *id == Some(row.id)) {
                continue;
            }
            let free = shown.slots.iter().position(|(id, _)| id.is_none());
            let slot = free.or_else(|| {
                shown.slots.iter().position(|(id, _)| {
                    id.and_then(|id| table.row(id))
                        .is_some_and(|held| matches!(held.state, RowState::Done | RowState::Reused))
                })
            });
            let Some(slot) = slot else { continue };
            shown.slots[slot].0 = Some(row.id);
        }

        // Stages overlap, so this is shorter than their column added up.
        let elapsed = format_elapsed(total_elapsed(&snapshot));
        let (head_name, head_paint) = match screen.failure() {
            Some(reason) => (format!("Map load failed — {reason}"), STATUS_FAILED),
            None => ("Loading".to_owned(), STATUS_TOTAL),
        };

        for (cell, mut text, mut color) in &mut self.cells {
            let (value, name, paint) = match cell.line {
                StatusLine::Head => (Some(&elapsed), head_name.as_str(), head_paint),
                StatusLine::Slot(slot) => {
                    match shown
                        .slots
                        .get(slot)
                        .and_then(|(id, _)| *id)
                        .and_then(|id| table.row(id))
                    {
                        Some(row) => (Some(&row.value), row.name.as_str(), row.state.color()),
                        None => (None, "", STATUS_TOTAL),
                    }
                }
            };
            set_text(
                &mut text,
                match cell.column {
                    StatusColumn::Name => name,
                    StatusColumn::Number => value.map_or("", |metric| metric.number.as_str()),
                    StatusColumn::Unit => value.map_or("", |metric| metric.unit.as_str()),
                },
            );
            color.0 = paint;
        }
    }
}

pub(crate) fn update_loading_screen(
    mut commands: Commands,
    font: Option<Res<GameUiFont>>,
    mut screen: Option<ResMut<LoadingScreen>>,
    load: Option<Res<assets::MapLoadProcess>>,
    mut view: LoadingScreenView,
    roots: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
) {
    let Some(screen) = screen.as_deref_mut() else {
        return;
    };

    screen.tick_elapsed();

    view.animate(screen);

    if screen.is_complete() {
        dismiss_loading_overlay(&mut commands, roots.iter(), std::iter::empty(), true);
        diag::info!(Ui, "loading: screen dismissed");
        return;
    }

    view.refresh(&mut commands, screen, font.as_deref(), load.as_deref());
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
    Number,
    Unit,
}

#[derive(Component, Default)]
pub(crate) struct StatusTable {
    next_refresh: Option<std::time::Instant>,
    head: Option<Entity>,
    spacer: Option<Entity>,
    slots: Vec<(Option<asset_transport::StageId>, Entity)>,
}

impl StatusTable {
    fn drain(&mut self) -> Vec<Entity> {
        self.head
            .take()
            .into_iter()
            .chain(self.spacer.take())
            .chain(self.slots.drain(..).map(|(_, entity)| entity))
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusLine {
    Head,
    Slot(usize),
}

#[derive(Component)]
pub(crate) struct LoadingStatusCell {
    line: StatusLine,
    column: StatusColumn,
}

fn status_row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Px(STATUS_ROW_PX),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        ..default()
    }
}

fn spawn_spacer_row(commands: &mut Commands) -> Entity {
    commands.spawn((LoadingStatusRow, status_row_node())).id()
}

fn spawn_metric_row(commands: &mut Commands, font: &Handle<Font>, line: StatusLine) -> Entity {
    let entity = commands.spawn((LoadingStatusRow, status_row_node())).id();
    commands.entity(entity).with_children(|parent| {
        parent
            .spawn(Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                overflow: Overflow::clip_x(),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::right(Val::Px(STATUS_NAME_GAP_PX)),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusCaption,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Name,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
        parent
            .spawn(status_cell_node(
                STATUS_NUMBER_COL_PX,
                JustifyContent::FlexEnd,
            ))
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusCount,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Number,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
        parent
            .spawn(Node {
                padding: UiRect::left(Val::Px(6.0)),
                ..status_cell_node(STATUS_UNIT_COL_PX, JustifyContent::FlexStart)
            })
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusTime,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Unit,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
    });
    entity
}

/// A wrapped cell is a taller box in a row of fixed height, and a taller box
/// centres its first line higher than its neighbours'.
fn status_text_layout() -> TextLayout {
    TextLayout::new(Justify::Left, bevy::text::LineBreak::NoWrap)
}

fn status_cell_node(width: f32, justify: JustifyContent) -> Node {
    Node {
        width: Val::Px(width),
        min_width: Val::Px(width),
        flex_shrink: 0.0,
        justify_content: justify,
        align_items: AlignItems::Center,
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
