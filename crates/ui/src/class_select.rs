use bevy::prelude::*;
use frame::{AppScreen, ClientSet, RuntimeRole};
use net::SignonState;

use crate::class_icons::{
    ClassSelectIconCache, UiAssetRoot, cac_weapon_image, pretty_weapon_name,
    warm_class_select_icons,
};
use crate::class_store::SessionClassStore;
use crate::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

const BTN_IDLE: Color = Color::srgba(0.10, 0.12, 0.16, 0.92);
const BTN_HOVER: Color = Color::srgba(0.22, 0.28, 0.36, 0.96);
const BTN_PRESSED: Color = Color::srgba(0.34, 0.42, 0.52, 1.0);
const BTN_PENDING: Color = Color::srgba(0.16, 0.18, 0.22, 0.92);
const ROW_SELECTED: Color = Color::srgba(0.28, 0.36, 0.48, 0.96);
const PANEL_BG: Color = Color::srgba(0.02, 0.03, 0.05, 0.82);
const MUTED: Color = Color::srgb(0.65, 0.68, 0.72);
const WARN: Color = Color::srgb(0.92, 0.62, 0.28);

const GUN_CARD_W: f32 = 150.0;
const GUN_CARD_H: f32 = 76.0;
const SEC_CARD_W: f32 = 100.0;
const SEC_CARD_H: f32 = 50.0;
const EQUIP_ICON: f32 = 32.0;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassSelectOverlayOpen(pub bool);

pub(crate) fn overlay_open_after_screen(
    prev: Option<AppScreen>,
    now: AppScreen,
    replay: bool,
    failed: bool,
    currently_open: bool,
) -> bool {
    if failed || replay {
        return false;
    }
    if matches!(now, AppScreen::ClassSelect) {
        if !matches!(prev, Some(AppScreen::ClassSelect)) {
            return true;
        }
        return currently_open;
    }
    false
}

impl Default for ClassSelectOverlayOpen {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassChangeAllowed(pub bool);

impl Default for ClassChangeAllowed {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassChangeBlockReason(pub Option<String>);

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum ClassSelectPhase {
    Interactive,
    Pending { request_id: u32, class_index: usize },
}

impl Default for ClassSelectPhase {
    fn default() -> Self {
        Self::Interactive
    }
}

impl ClassSelectPhase {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    pub fn pending_request_id(&self) -> Option<u32> {
        match *self {
            Self::Pending { request_id, .. } => Some(request_id),
            Self::Interactive => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassEquipRequest {
    pub request_id: u32,
    pub class_index: usize,
}

#[derive(Resource, Debug, Default)]
pub struct PendingClassEquip(pub Option<ClassEquipRequest>);

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassSelectStatus(pub Option<String>);

#[derive(Resource, Debug, Clone, Copy)]
pub struct ClassSelectHighlight(pub usize);

impl Default for ClassSelectHighlight {
    fn default() -> Self {
        Self(0)
    }
}

#[derive(Component)]
struct ClassSelectRoot;

#[derive(Component)]
struct ClassSelectRetiring(bool);

#[derive(Component)]
struct ClassSelectStatusText;

#[derive(Component, Clone, Copy, Debug)]
enum ClassSelectAction {
    Select(usize),
    Equip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassEquipRefusal {
    AlreadyPending { request_id: u32 },

    UnknownClass(String),

    LockedContent { index: usize, reason: String },
}

impl std::fmt::Display for ClassEquipRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyPending { request_id } => {
                write!(f, "equip already pending (request_id={request_id})")
            }
            Self::UnknownClass(name) => write!(f, "unknown_class: `{name}`"),
            Self::LockedContent { index, reason } => {
                write!(f, "locked_content: preset {index} — {reason}")
            }
        }
    }
}

pub fn class_index_by_name(store: &SessionClassStore, name: &str) -> Option<usize> {
    if let Ok(index) = name.parse::<usize>()
        && index < store.slots.len()
    {
        return Some(index);
    }
    store
        .slots
        .iter()
        .position(|slot| slot.name.eq_ignore_ascii_case(name))
}

pub fn commit_class_equip(
    index: usize,
    store: &mut SessionClassStore,
    highlight: &mut ClassSelectHighlight,
    phase: &mut ClassSelectPhase,
    pending: &mut PendingClassEquip,
    status: &mut ClassSelectStatus,
    seq: &mut net::ActionRequestIds,
) -> Result<u32, ClassEquipRefusal> {
    if let Some(request_id) = phase.pending_request_id() {
        return Err(ClassEquipRefusal::AlreadyPending { request_id });
    }
    let Some(slot) = store.slots.get(index) else {
        return Err(ClassEquipRefusal::UnknownClass(index.to_string()));
    };
    if let Some(reason) = slot.lock_reason.as_deref() {
        let refusal = ClassEquipRefusal::LockedContent {
            index,
            reason: reason.to_owned(),
        };
        status.0 = Some(refusal.to_string());
        return Err(refusal);
    }
    let request_id = seq.allocate();
    highlight.0 = index;
    store.commit_equip(index);
    pending.0 = Some(ClassEquipRequest {
        request_id,
        class_index: index,
    });
    *phase = ClassSelectPhase::Pending {
        request_id,
        class_index: index,
    };
    status.0 = None;
    Ok(request_id)
}

pub fn accept_class_equip(
    phase: &mut ClassSelectPhase,
    overlay: &mut ClassSelectOverlayOpen,
    request_id: u32,
) -> bool {
    match *phase {
        ClassSelectPhase::Pending {
            request_id: pending,
            ..
        } if pending == request_id => {
            *phase = ClassSelectPhase::Interactive;
            overlay.0 = false;
            true
        }
        _ => false,
    }
}

pub fn reject_class_equip(
    phase: &mut ClassSelectPhase,
    status: &mut ClassSelectStatus,
    request_id: u32,
    reason: impl Into<String>,
) -> bool {
    match *phase {
        ClassSelectPhase::Pending {
            request_id: pending,
            ..
        } if pending == request_id => {
            *phase = ClassSelectPhase::Interactive;
            status.0.replace(reason.into());
            true
        }
        _ => false,
    }
}

pub(crate) struct ClassSelectPlugin;

impl Plugin for ClassSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClassSelectOverlayOpen>()
            .init_resource::<ClassChangeAllowed>()
            .init_resource::<ClassChangeBlockReason>()
            .init_resource::<PendingClassEquip>()
            .init_resource::<ClassSelectPhase>()
            .init_resource::<ClassSelectStatus>()
            .init_resource::<ClassSelectHighlight>()
            .init_resource::<ClassSelectIconCache>()
            .init_resource::<UiAssetRoot>()
            .init_resource::<SessionClassStore>()
            .add_systems(
                PostUpdate,
                sync_class_select_overlay_to_screen.before(crate::layers::ApplyUiLayers),
            )
            .add_systems(
                Update,
                (
                    tick_class_select_retiring,
                    warm_class_select_icons.run_if(|e: Res<ClassSelectOverlayOpen>| e.0),
                    tear_down_class_select_when_disabled,
                    rebuild_class_select_ui,
                    update_class_select_hover,
                    handle_class_select_buttons,
                )
                    .chain()
                    .in_set(ClientSet::Ui),
            );
    }
}

fn sync_class_select_overlay_to_screen(
    screen: Res<AppScreen>,
    role: Option<Res<RuntimeRole>>,
    signon: Option<Res<SignonState>>,
    mut overlay: ResMut<ClassSelectOverlayOpen>,
    mut last: Local<Option<AppScreen>>,
) {
    let replay = role.is_some_and(|role| *role == RuntimeRole::Replay);
    let failed = signon.is_some_and(|signon| signon.phase.is_failed());
    let next = overlay_open_after_screen(*last, *screen, replay, failed, overlay.0);
    if overlay.0 != next {
        overlay.0 = next;
    }
    *last = Some(*screen);
}

fn tick_class_select_retiring(
    mut commands: Commands,
    mut retiring: Query<(Entity, &mut ClassSelectRetiring)>,
) {
    for (entity, mut tag) in &mut retiring {
        if tag.0 {
            commands.entity(entity).despawn();
        } else {
            tag.0 = true;
        }
    }
}

fn retire_class_select_root(
    commands: &mut Commands,
    root: Entity,
    image_nodes: &Query<Entity, With<ImageNode>>,
    children: &Query<&Children>,
) {
    strip_image_nodes(commands, root, image_nodes, children);
    commands
        .entity(root)
        .insert((ClassSelectRetiring(false), Visibility::Hidden))
        .remove::<ClassSelectRoot>();
}

fn strip_image_nodes(
    commands: &mut Commands,
    entity: Entity,
    image_nodes: &Query<Entity, With<ImageNode>>,
    children: &Query<&Children>,
) {
    if image_nodes.contains(entity) {
        commands.entity(entity).remove::<ImageNode>();
    }
    let Ok(kids) = children.get(entity) else {
        return;
    };
    for child in kids.iter() {
        strip_image_nodes(commands, child, image_nodes, children);
    }
}

fn tear_down_class_select_when_disabled(
    overlay: Res<ClassSelectOverlayOpen>,
    mut commands: Commands,
    roots: Query<Entity, (With<ClassSelectRoot>, Without<ClassSelectRetiring>)>,
    image_nodes: Query<Entity, With<ImageNode>>,
    children: Query<&Children>,
) {
    if overlay.0 || !overlay.is_changed() {
        return;
    }
    for entity in &roots {
        retire_class_select_root(&mut commands, entity, &image_nodes, &children);
    }
}

fn rebuild_class_select_ui(
    mut commands: Commands,
    overlay: Res<ClassSelectOverlayOpen>,
    highlight: Res<ClassSelectHighlight>,
    phase: Res<ClassSelectPhase>,
    status: Res<ClassSelectStatus>,
    store: Res<SessionClassStore>,
    font: Option<Res<GameUiFont>>,
    icons: Res<ClassSelectIconCache>,
    roots: Query<Entity, (With<ClassSelectRoot>, Without<ClassSelectRetiring>)>,
    image_nodes: Query<Entity, With<ImageNode>>,
    children: Query<&Children>,
    mut last: Local<Option<(bool, usize, bool, usize, bool, Option<String>)>>,
) {
    let key = (
        overlay.0,
        highlight.0,
        icons.warmed,
        store.slots.len(),
        phase.is_pending(),
        status.0.clone(),
    );
    if last.as_ref() == Some(&key) {
        return;
    }
    *last = Some(key);
    if !overlay.0 {
        return;
    }
    let Some(font) = font else {
        diag::warn!(Ui, "class select: GameUiFont missing; overlay skipped");
        return;
    };
    for entity in &roots {
        retire_class_select_root(&mut commands, entity, &image_nodes, &children);
    }
    diag::info!(
        Ui,
        "class select: spawn overlay ({} slots, highlight={}, icons={}, pending={})",
        store.slots.len(),
        highlight.0,
        icons.images.len(),
        phase.is_pending()
    );
    spawn_class_select(
        &mut commands,
        &font.0,
        highlight.0,
        &store,
        &icons,
        phase.is_pending(),
        status.0.as_deref(),
    );
}

fn spawn_class_select(
    commands: &mut Commands,
    font: &Handle<Font>,
    selected: usize,
    store: &SessionClassStore,
    icons: &ClassSelectIconCache,
    pending: bool,
    status: Option<&str>,
) {
    let locked = store
        .slots
        .get(selected)
        .and_then(|s| s.lock_reason.as_deref());
    let subtitle = if pending {
        "WAITING FOR AUTHORITY — class request pending…".to_owned()
    } else if let Some(reason) = locked {
        format!("LOCKED — {reason}")
    } else {
        format!(
            "{} — Equip a class to enter the match.",
            sim::host_game_mode_kind().display_name()
        )
    };
    let equip_label = if pending {
        "Equip pending…".to_owned()
    } else if locked.is_some() {
        "Locked (coverage)".to_owned()
    } else {
        "Equip selected".to_owned()
    };
    commands
        .spawn((
            ClassSelectRoot,
            UiLayer::Shell,
            UiLayerVisibility,
            Visibility::Inherited,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(20_000),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Choose Class"),
                game_text_font(font, 26.0),
                TextColor(Color::srgb(0.92, 0.93, 0.95)),
            ));
            root.spawn((
                Text::new(subtitle),
                game_text_font(font, 13.0),
                TextColor(if locked.is_some() { WARN } else { MUTED }),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));
            if let Some(status) = status {
                root.spawn((
                    ClassSelectStatusText,
                    Text::new(status.to_owned()),
                    game_text_font(font, 13.0),
                    TextColor(WARN),
                ));
            }
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    min_width: Val::Px(520.0),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|list| {
                for (index, slot) in store.slots.iter().enumerate() {
                    spawn_class_row(list, font, index, slot, index == selected, icons);
                }
                spawn_action_button(list, &equip_label, ClassSelectAction::Equip, font, pending);
            });
        });
}

fn spawn_class_row(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    index: usize,
    slot: &crate::ClassSlotState,
    selected: bool,
    icons: &ClassSelectIconCache,
) {
    let mut label = slot.name.to_ascii_uppercase();
    if slot.lock_reason.is_some() {
        label = format!("{label}  [LOCKED]");
    }
    if selected {
        label = format!("> {label}");
    }
    parent
        .spawn((
            Button,
            ClassSelectAction::Select(index),
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                align_items: AlignItems::Stretch,
                ..default()
            },
            BackgroundColor(if selected { ROW_SELECTED } else { BTN_IDLE }),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                game_text_font(font, 16.0),
                TextColor(Color::WHITE),
            ));
            if selected {
                spawn_selected_summary(btn, font, slot, icons);
            }
        });
}

fn spawn_selected_summary(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    slot: &crate::ClassSlotState,
    icons: &ClassSelectIconCache,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::left(Val::Px(8.0)),
            ..default()
        })
        .with_children(|sum| {
            if let Some(handle) = weapon_handle(icons, &slot.primary) {
                spawn_image(sum, handle, GUN_CARD_W, GUN_CARD_H);
            } else {
                sum.spawn((
                    Text::new(pretty_weapon_name(&slot.primary)),
                    game_text_font(font, 14.0),
                    TextColor(MUTED),
                ));
            }
            sum.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|eq| {
                if let Some(handle) = weapon_handle(icons, &slot.secondary) {
                    spawn_image(eq, handle, SEC_CARD_W, SEC_CARD_H);
                }
                if let Some(handle) = weapon_handle(icons, &slot.lethal) {
                    spawn_image(eq, handle, EQUIP_ICON, EQUIP_ICON);
                }
                if let Some(handle) = weapon_handle(icons, &slot.tactical) {
                    spawn_image(eq, handle, EQUIP_ICON, EQUIP_ICON);
                }
            });
            let perk_line = [&slot.perk1, &slot.perk2, &slot.perk3]
                .iter()
                .filter(|p| !p.is_empty() && *p != &"—")
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(" · ");
            if !perk_line.is_empty() {
                sum.spawn((
                    Text::new(perk_line),
                    game_text_font(font, 12.0),
                    TextColor(MUTED),
                ));
            }
        });
}

fn weapon_handle(icons: &ClassSelectIconCache, weapon: &str) -> Option<Handle<Image>> {
    let stem = cac_weapon_image(weapon)?;
    icons.get(stem)
}

fn spawn_image(parent: &mut ChildSpawnerCommands, handle: Handle<Image>, w: f32, h: f32) {
    parent.spawn((
        ImageNode::new(handle).with_mode(bevy::ui::widget::NodeImageMode::Stretch),
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

fn spawn_action_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: ClassSelectAction,
    font: &Handle<Font>,
    pending: bool,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: Val::Percent(100.0),
                margin: UiRect::top(Val::Px(6.0)),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(if pending { BTN_PENDING } else { BTN_IDLE }),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                game_text_font(font, 18.0),
                TextColor(Color::WHITE),
            ));
        });
}

fn update_class_select_hover(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &ClassSelectAction),
        (Changed<Interaction>, With<Button>),
    >,
    highlight: Res<ClassSelectHighlight>,
    phase: Res<ClassSelectPhase>,
) {
    let pending = phase.is_pending();
    for (interaction, mut color, action) in &mut buttons {
        let selected = matches!(action, ClassSelectAction::Select(i) if *i == highlight.0);
        *color = match (*interaction, selected, action, pending) {
            (_, _, ClassSelectAction::Equip, true) => BackgroundColor(BTN_PENDING),
            (Interaction::Pressed, _, _, _) => BackgroundColor(BTN_PRESSED),
            (Interaction::Hovered, _, _, _) => BackgroundColor(BTN_HOVER),
            (_, true, ClassSelectAction::Select(_), _) => BackgroundColor(ROW_SELECTED),
            _ => BackgroundColor(BTN_IDLE),
        };
    }
}

fn handle_class_select_buttons(
    mut highlight: ResMut<ClassSelectHighlight>,
    mut pending: ResMut<PendingClassEquip>,
    mut phase: ResMut<ClassSelectPhase>,
    mut status: ResMut<ClassSelectStatus>,
    mut seq: ResMut<net::ActionRequestIds>,
    mut store: ResMut<SessionClassStore>,
    allowed: Res<ClassChangeAllowed>,
    block: Res<ClassChangeBlockReason>,
    buttons: Query<(&Interaction, &ClassSelectAction), (Changed<Interaction>, With<Button>)>,
) {
    for (interaction, action) in &buttons {
        if !matches!(*interaction, Interaction::Pressed) {
            continue;
        }
        match *action {
            ClassSelectAction::Select(index) => {
                if phase.is_pending() {
                    continue;
                }
                highlight.0 = index;
            }
            ClassSelectAction::Equip => {
                if !allowed.0 {
                    let reason = block
                        .0
                        .clone()
                        .unwrap_or_else(|| "class change not allowed".to_owned());
                    status.0 = Some(reason);
                    continue;
                }
                let index = highlight.0;
                match commit_class_equip(
                    index,
                    &mut store,
                    &mut highlight,
                    &mut phase,
                    &mut pending,
                    &mut status,
                    &mut seq,
                ) {
                    Ok(request_id) => {
                        let name = store
                            .equipped_slot()
                            .map(|s| s.name.as_str())
                            .unwrap_or("?");
                        diag::info!(
                            Ui,
                            "class select: equip preset {index} ({name}) request_id={request_id} -> Pending"
                        );
                    }
                    Err(ClassEquipRefusal::AlreadyPending { .. }) => continue,
                    Err(refusal) => {
                        diag::info!(Ui, "class select: Equip refused — {refusal}");
                    }
                }
            }
        }
    }
}
