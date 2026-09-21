use bevy::picking::{
    events::{Out, Over, Pointer, Press},
    pointer::PointerButton,
};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::model::{Content, ScreenCmd, SettingValue, UiIntent};
use crate::render::{FocusHelpText, SliderControl, WidgetBehavior, WidgetControl, WidgetHelp};
use frame::UiPlaySound;

pub(crate) const UI_PASS_FOCUS: FocusPolicy = FocusPolicy::Pass;

pub(crate) const SELECTION_FILL: Color = Color::srgba(0.0, 0.0, 0.0, 1.0);

#[derive(Resource, Debug, Default, Clone)]
pub struct Focus {
    pub widget: Option<String>,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct Hover {
    pub widget: Option<String>,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct ActivatePulse(pub bool);

#[derive(Resource, Debug, Default)]
pub(crate) struct PointerActivation(pub Vec<String>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDir {
    Up,
    Down,
    Left,
    Right,
}

impl NavDir {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    fn vec(self) -> (f32, f32) {
        match self {
            Self::Up => (0.0, -1.0),
            Self::Down => (0.0, 1.0),
            Self::Left => (-1.0, 0.0),
            Self::Right => (1.0, 0.0),
        }
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum MenuShellCmd {
    Nav(NavDir),
    Accept,
    Back,
}

#[derive(Component, Clone, Debug)]
pub struct Focusable {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,

    pub order: Option<u32>,
}

#[derive(Component)]
pub(crate) struct SelectionBar;

pub(crate) fn spawn_selection_bar(parent: &mut ChildSpawnerCommands, image: Option<Handle<Image>>) {
    let mut bar = parent.spawn((
        SelectionBar,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Visibility::Hidden,
        UI_PASS_FOCUS,
        Pickable::IGNORE,
    ));
    if let Some(handle) = image {
        bar.insert(
            ImageNode::new(handle)
                .with_mode(bevy::ui::widget::NodeImageMode::Stretch)
                .with_color(SELECTION_FILL),
        );
    } else {
        bar.insert(BackgroundColor(SELECTION_FILL));
    }
}

fn pick_ordered(from_id: &str, dir: NavDir, widgets: &[Focusable]) -> Option<String> {
    let mut ranked: Vec<(u32, &str)> = widgets
        .iter()
        .filter_map(|widget| Some((widget.order?, widget.id.as_str())))
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    let at = ranked.iter().position(|(_, id)| *id == from_id)?;
    if ranked.len() < 2 {
        return None;
    }
    let step: isize = match dir {
        NavDir::Up | NavDir::Left => -1,
        NavDir::Down | NavDir::Right => 1,
    };
    let next = (at as isize + step).rem_euclid(ranked.len() as isize) as usize;
    Some(ranked[next].1.to_owned())
}

pub fn pick_nav(from_id: &str, dir: NavDir, widgets: &[Focusable]) -> Option<String> {
    if widgets
        .iter()
        .any(|widget| widget.id == from_id && widget.order.is_some())
    {
        return pick_ordered(from_id, dir, widgets);
    }
    if widgets.len() <= 1 {
        return widgets.first().map(|w| w.id.clone());
    }
    let from = widgets.iter().find(|w| w.id == from_id)?;
    let (fx, fy) = center(from);
    let (dx, dy) = dir.vec();
    let mut best: Option<(f32, f32, &str)> = None;
    for widget in widgets {
        if widget.id == from_id {
            continue;
        }
        let (cx, cy) = center(widget);
        let vx = cx - fx;
        let vy = cy - fy;
        let dist = (vx * vx + vy * vy).sqrt();
        if dist < 1e-4 {
            continue;
        }
        let proj = (vx * dx + vy * dy) / dist;
        if proj < 0.5 {
            continue;
        }
        let along = vx * dx + vy * dy;
        let lat = (vx * (-dy) + vy * dx).abs();
        let score = along + lat * 2.0;
        let better = match best {
            None => true,
            Some((best_score, best_dist, best_id)) => {
                (score, dist, widget.id.as_str()) < (best_score, best_dist, best_id)
            }
        };
        if better {
            best = Some((score, dist, widget.id.as_str()));
        }
    }
    if let Some((_, _, id)) = best {
        return Some(id.to_owned());
    }
    wrap_extreme(from_id, dir, widgets)
}

fn wrap_extreme(from_id: &str, dir: NavDir, widgets: &[Focusable]) -> Option<String> {
    let (dx, dy) = dir.vec();
    let mut pick: Option<(f32, &str)> = None;
    for widget in widgets {
        if widget.id == from_id {
            continue;
        }
        let (cx, cy) = center(widget);
        let key = cx * dx + cy * dy;
        let better = match pick {
            None => true,
            Some((best, best_id)) => (key, widget.id.as_str()) < (best, best_id),
        };
        if better {
            pick = Some((key, widget.id.as_str()));
        }
    }
    pick.map(|(_, id)| id.to_owned())
}

fn center(widget: &Focusable) -> (f32, f32) {
    (widget.x + widget.w * 0.5, widget.y + widget.h * 0.5)
}

pub(crate) fn first_focusable(widgets: &[Focusable]) -> Option<String> {
    if let Some(first) = widgets
        .iter()
        .filter(|widget| widget.order.is_some())
        .min_by_key(|widget| (widget.order, widget.id.clone()))
    {
        return Some(first.id.clone());
    }
    let mut ids: Vec<&Focusable> = widgets.iter().collect();
    ids.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.id.cmp(&b.id))
    });
    ids.first().map(|w| w.id.clone())
}

pub(crate) fn sync_pointer_input(
    mut over: MessageReader<Pointer<Over>>,
    mut out: MessageReader<Pointer<Out>>,
    mut press: MessageReader<Pointer<Press>>,
    widgets: Query<&Focusable>,
    stack: Res<crate::retail_menu::RetailMenuStack>,
    options: Res<crate::OptionsState>,
    classes: Res<crate::ClassSetupScratch>,
    mut focus: ResMut<Focus>,
    mut hover: ResMut<Hover>,
    mut activations: ResMut<PointerActivation>,
) {
    activations.0.clear();
    for event in over.read() {
        let Ok(widget) = widgets.get(event.entity) else {
            continue;
        };
        if pointer_widget_is_active(&stack, &options, &classes, &widget.id) {
            hover.widget = Some(widget.id.clone());
            focus.widget = Some(widget.id.clone());
        }
    }
    for event in out.read() {
        let Ok(widget) = widgets.get(event.entity) else {
            continue;
        };
        if hover.widget.as_deref() == Some(widget.id.as_str()) {
            hover.widget = None;
        }
    }
    for event in press.read() {
        if event.event.button != PointerButton::Primary {
            continue;
        }
        let Ok(widget) = widgets.get(event.entity) else {
            continue;
        };
        if pointer_widget_is_active(&stack, &options, &classes, &widget.id) {
            focus.widget = Some(widget.id.clone());
            if !activations.0.contains(&widget.id) {
                activations.0.push(widget.id.clone());
            }
        }
    }
}

pub(crate) fn sync_hover_and_nav(
    mut focus: ResMut<Focus>,
    mut hover: ResMut<Hover>,
    mut pulse: ResMut<ActivatePulse>,
    mut cmds: MessageReader<MenuShellCmd>,
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Query<(&Interaction, &Focusable), (Changed<Interaction>, With<Button>)>,
    mut pointer_activations: ResMut<PointerActivation>,
    all: Query<&Focusable>,
    controls: Query<(&Focusable, &WidgetControl)>,
    stack: Res<crate::retail_menu::RetailMenuStack>,
    options: Res<crate::OptionsState>,
    classes: Res<crate::ClassSetupScratch>,
) {
    pulse.0 = false;
    let mut hover_now = hover.widget.clone();
    for (interaction, widget) in &hovered {
        if !pointer_widget_is_active(&stack, &options, &classes, &widget.id) {
            continue;
        }
        match *interaction {
            Interaction::Hovered | Interaction::Pressed => {
                hover_now = Some(widget.id.clone());
                focus.widget = Some(widget.id.clone());
                if matches!(*interaction, Interaction::Pressed)
                    && !pointer_activations.0.contains(&widget.id)
                {
                    pointer_activations.0.push(widget.id.clone());
                }
            }
            Interaction::None => {
                if hover_now.as_deref() == Some(widget.id.as_str()) {
                    hover_now = None;
                }
            }
        }
    }
    hover.widget = hover_now;

    let focused_control = focus.widget.as_deref().and_then(|id| {
        controls
            .iter()
            .find(|(widget, _)| widget.id == id)
            .map(|(_, control)| &control.0)
    });
    let captures_all = matches!(
        focused_control,
        Some(
            Content::Bind {
                listening: true,
                ..
            } | Content::TextEdit { editing: true, .. }
        )
    );
    let consumes_horizontal = captures_all
        || matches!(
            focused_control,
            Some(Content::Cycler { .. } | Content::Slider { .. })
        );
    let top = stack.names.last().map(String::as_str);
    let options_browsing = top == Some("options");
    let class_browsing = top == Some("class_setup");
    let class_paging = class_browsing && classes.is_picker();
    let mut dirs = Vec::new();
    let mut accept = false;
    for cmd in cmds.read() {
        match cmd {
            MenuShellCmd::Nav(d)
                if !captures_all
                    && !((options_browsing || class_browsing)
                        && matches!(d, NavDir::Left | NavDir::Right))
                    && (!consumes_horizontal || matches!(d, NavDir::Up | NavDir::Down)) =>
            {
                dirs.push(*d);
            }
            MenuShellCmd::Nav(NavDir::Right)
                if class_browsing && !class_paging && !captures_all =>
            {
                accept = true
            }
            MenuShellCmd::Nav(_) => {}
            MenuShellCmd::Accept => accept = true,
            MenuShellCmd::Back => {}
        }
    }
    if !captures_all && (keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW)) {
        dirs.push(NavDir::Up);
    }
    if !captures_all && (keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS))
    {
        dirs.push(NavDir::Down);
    }
    if !options_browsing
        && !class_browsing
        && !consumes_horizontal
        && (keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA))
    {
        dirs.push(NavDir::Left);
    }
    if !options_browsing && !consumes_horizontal {
        if class_browsing
            && !class_paging
            && (keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD))
        {
            accept = true;
        } else if !class_browsing
            && (keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD))
        {
            dirs.push(NavDir::Right);
        }
    }
    if !captures_all && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space)) {
        accept = true;
    }
    pulse.0 = accept;

    let widgets: Vec<Focusable> = all
        .iter()
        .filter(|widget| keyboard_widget_is_active(&stack, &options, &classes, &widget.id))
        .cloned()
        .collect();
    if widgets.is_empty() {
        focus.widget = None;
        return;
    }
    let current_live = focus.widget.as_ref().is_some_and(|id| {
        widgets.iter().any(|w| w.id == *id)
            || (all.iter().any(|w| w.id == *id)
                && pointer_widget_is_active(&stack, &options, &classes, id))
    });
    if !current_live {
        focus.widget = first_focusable(&widgets);
    }
    for dir in dirs {
        if let Some(from) = focus.widget.clone()
            && let Some(next) = pick_nav(&from, dir, &widgets)
        {
            focus.widget = Some(next);
        }
    }
}

fn keyboard_widget_is_active(
    stack: &crate::retail_menu::RetailMenuStack,
    options: &crate::OptionsState,
    classes: &crate::ClassSetupScratch,
    id: &str,
) -> bool {
    match stack.names.last().map(String::as_str) {
        Some("options") => crate::options::options_widget_is_active(options, id),
        Some("class_setup") => crate::class_setup::class_widget_is_active(classes, id),
        _ => true,
    }
}

fn pointer_widget_is_active(
    stack: &crate::retail_menu::RetailMenuStack,
    options: &crate::OptionsState,
    classes: &crate::ClassSetupScratch,
    id: &str,
) -> bool {
    match stack.names.last().map(String::as_str) {
        Some("options") => crate::options::options_pointer_widget_is_active(options, id),
        Some("class_setup") => crate::class_setup::class_widget_is_active(classes, id),
        _ => true,
    }
}

pub(crate) fn drive_control_axes(
    keys: Res<ButtonInput<KeyCode>>,
    mut shell: MessageReader<MenuShellCmd>,
    focus: Res<Focus>,
    controls: Query<(&Focusable, &WidgetControl)>,
    sliders: Query<(
        &Focusable,
        &SliderControl,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut dragging: Local<Option<String>>,
    mut intents: MessageWriter<UiIntent>,
) {
    let mut horizontal = 0i32;
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        horizontal -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        horizontal += 1;
    }
    for cmd in shell.read() {
        if let MenuShellCmd::Nav(dir) = cmd {
            horizontal += match dir {
                NavDir::Left => -1,
                NavDir::Right => 1,
                _ => 0,
            };
        }
    }
    if horizontal != 0
        && let Some(id) = focus.widget.as_deref()
        && let Some((_, control)) = controls.iter().find(|(widget, _)| widget.id == id)
    {
        match &control.0 {
            Content::Cycler { previous, next, .. } => {
                intents.write(if horizontal < 0 {
                    previous.clone()
                } else {
                    next.clone()
                });
            }
            Content::Slider {
                value,
                min,
                max,
                step,
                key,
                ..
            } => {
                intents.write(UiIntent::SetSetting {
                    key: *key,
                    value: SettingValue::Float(
                        (*value + *step * horizontal.signum() as f32).clamp(*min, *max),
                    ),
                });
            }
            _ => {}
        }
    }
    if !mouse.pressed(MouseButton::Left) {
        *dragging = None;
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        *dragging = windows
            .single()
            .ok()
            .and_then(Window::physical_cursor_position)
            .and_then(|cursor| {
                sliders.iter().find_map(|(widget, _, node, transform)| {
                    let local = transform.try_inverse()?.transform_point2(cursor);
                    let half = node.size() * 0.5;
                    (half.x > 0.0 && local.abs().cmple(half).all()).then(|| widget.id.clone())
                })
            });
    }
    if let Some(id) = dragging.as_deref()
        && let Some((_, slider, node, transform)) =
            sliders.iter().find(|(widget, _, _, _)| widget.id == id)
        && let Ok(window) = windows.single()
        && let Some(cursor) = window.physical_cursor_position()
        && let Some(inverse) = transform.try_inverse()
    {
        let width = node.size().x;
        if width > 0.0 {
            let x = inverse.transform_point2(cursor).x + width * 0.5;
            let fraction = ((x - width * 0.48) / (width * 0.38)).clamp(0.0, 1.0);
            intents.write(UiIntent::SetSetting {
                key: slider.key,
                value: SettingValue::Float(slider.min + fraction * (slider.max - slider.min)),
            });
        }
    }
}

pub(crate) fn paint_focus_help(
    focus: Res<Focus>,
    widgets: Query<(&Focusable, &WidgetHelp)>,
    mut labels: Query<&mut Text, With<FocusHelpText>>,
) {
    let value = focus
        .widget
        .as_deref()
        .and_then(|id| widgets.iter().find(|(widget, _)| widget.id == id))
        .map_or("", |(_, help)| help.0.as_str());
    for mut text in &mut labels {
        if text.0 != value {
            *text = Text::new(value);
        }
    }
}

pub(crate) fn paint_selection_bars(
    focus: Res<Focus>,
    items: Query<(&Focusable, &Children)>,
    mut bars: Query<&mut Visibility, With<SelectionBar>>,
) {
    for (widget, children) in &items {
        let on = focus.widget.as_deref() == Some(widget.id.as_str());
        for child in children {
            if let Ok(mut vis) = bars.get_mut(*child) {
                *vis = if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

pub(crate) fn play_focus_sound(
    mut stack: ResMut<crate::retail_menu::RetailMenuStack>,
    focus: Res<Focus>,
    hits: Query<(&Focusable, &WidgetBehavior), With<Button>>,
    mut play: MessageWriter<UiPlaySound>,
    mut last: Local<Option<String>>,
) {
    let Some(id) = focus.widget.clone() else {
        *last = None;
        return;
    };
    if last.as_ref() == Some(&id) {
        return;
    }
    *last = Some(id.clone());
    let Some((_, behavior)) = hits.iter().find(|(w, _)| w.id == id) else {
        return;
    };
    let aliases: Vec<String> = behavior
        .on_focus
        .iter()
        .filter_map(|cmd| match cmd {
            ScreenCmd::PlaySound(alias) => Some(alias.clone()),
            _ => None,
        })
        .collect();
    for alias in &aliases {
        play.write(UiPlaySound {
            alias: alias.clone(),
        });
    }
    if aliases.is_empty() && !stack.hover_sound_gap_said {
        diag::warn!(
            Ui,
            "menu: focus on `{id}` has no on_focus PlaySound (not inventing mouse_over)"
        );
        stack.hover_sound_gap_said = true;
    }
}

pub(crate) fn focus_matches(focus: &Focus, id: &str) -> bool {
    focus.widget.as_deref() == Some(id)
}
