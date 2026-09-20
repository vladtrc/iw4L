use std::collections::BTreeMap;

use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
    window::{Monitor, MonitorSelection, PresentMode, PrimaryWindow},
};

use crate::nav::{ActivatePulse, Focus, MenuShellCmd, NavDir, PointerActivation};
use crate::render::PaintedWidget;
use crate::retail_menu::RetailMenuStack;
use crate::{SettingKey, SettingValue, UiIntent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionsTab {
    #[default]
    Video,
    Audio,
    Controls,
    Multiplayer,
    Game,
}

impl OptionsTab {
    pub const ALL: [Self; 5] = [
        Self::Video,
        Self::Audio,
        Self::Controls,
        Self::Multiplayer,
        Self::Game,
    ];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Video,
            1 => Self::Audio,
            2 => Self::Controls,
            3 => Self::Multiplayer,
            4 => Self::Game,
            _ => return None,
        })
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Controls => "Controls",
            Self::Multiplayer => "Multiplayer",
            Self::Game => "Game",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionsDepth {
    #[default]
    Sections,
    SectionRows,
    ControlBinds,
    ResolutionPicker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionsControlGroup {
    #[default]
    Movement,
    Actions,
    Look,
}

impl OptionsControlGroup {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Movement => "MOVEMENT",
            Self::Actions => "ACTIONS",
            Self::Look => "LOOK",
        }
    }

    pub const fn widget_id(self) -> &'static str {
        match self {
            Self::Movement => "options/movement",
            Self::Actions => "options/actions",
            Self::Look => "options/look",
        }
    }

    pub fn from_widget_id(id: &str) -> Option<Self> {
        Some(match id {
            "options/movement" => Self::Movement,
            "options/actions" => Self::Actions,
            "options/look" => Self::Look,
            _ => return None,
        })
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct OptionsState {
    pub tab: OptionsTab,
    pub depth: OptionsDepth,
    pub control_group: OptionsControlGroup,
    pub name_buffer: Option<String>,
    pub name_cursor: usize,

    pub display_resolutions: Vec<frame::DisplayResolution>,
    pub revision: u64,
}

impl OptionsState {
    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn reset_navigation(&mut self) {
        self.tab = OptionsTab::Video;
        self.depth = OptionsDepth::Sections;
        self.control_group = OptionsControlGroup::Movement;
        self.name_buffer = None;
        self.name_cursor = 0;
        self.touch();
    }

    pub(crate) fn first_row_id(&self) -> &'static str {
        match self.tab {
            OptionsTab::Video => "options/resolution",
            OptionsTab::Audio => "options/volume",
            OptionsTab::Controls => OptionsControlGroup::Movement.widget_id(),
            OptionsTab::Multiplayer => "options/player_name",
            OptionsTab::Game => "options/sensitivity",
        }
    }

    pub(crate) fn go_parent(&mut self) -> Option<String> {
        match self.depth {
            OptionsDepth::Sections => None,
            OptionsDepth::SectionRows => {
                self.depth = OptionsDepth::Sections;
                self.touch();
                Some(format!("options/tab/{}", self.tab.as_u8()))
            }
            OptionsDepth::ControlBinds => {
                self.depth = OptionsDepth::SectionRows;
                self.touch();
                Some(self.control_group.widget_id().to_owned())
            }
            OptionsDepth::ResolutionPicker => {
                self.depth = OptionsDepth::SectionRows;
                self.touch();
                Some("options/resolution".to_owned())
            }
        }
    }
}

pub(crate) fn options_widget_is_active(state: &OptionsState, id: &str) -> bool {
    if state.name_buffer.is_some() {
        return id == "options/name_buffer";
    }
    match state.depth {
        OptionsDepth::Sections => id.starts_with("options/tab/"),
        OptionsDepth::SectionRows => match state.tab {
            OptionsTab::Video => matches!(
                id,
                "options/resolution" | "options/fullscreen" | "options/vsync"
            ),
            OptionsTab::Audio => id == "options/volume",
            OptionsTab::Controls => OptionsControlGroup::from_widget_id(id).is_some(),
            OptionsTab::Multiplayer => id == "options/player_name",
            OptionsTab::Game => matches!(id, "options/sensitivity" | "options/invert_mouse"),
        },
        OptionsDepth::ControlBinds => id.starts_with("options/binds/"),
        OptionsDepth::ResolutionPicker => {
            id == "options/resolution_cancel" || id.starts_with("options/resolution_choice/")
        }
    }
}

pub(crate) fn options_pointer_widget_is_active(state: &OptionsState, id: &str) -> bool {
    match state.depth {
        OptionsDepth::ResolutionPicker => options_widget_is_active(state, id),
        _ if state.name_buffer.is_some() => options_widget_is_active(state, id),
        _ => {
            let mut visible = state.clone();
            if visible.depth == OptionsDepth::Sections {
                visible.depth = OptionsDepth::SectionRows;
            }
            id.starts_with("options/tab/") || options_widget_is_active(&visible, id)
        }
    }
}

pub(crate) fn sync_options_entry(
    stack: Res<RetailMenuStack>,
    mut state: ResMut<OptionsState>,
    mut focus: ResMut<Focus>,
    mut previous_top: Local<Option<String>>,
) {
    let top = stack.names.last().cloned();
    if top.as_deref() == Some("options") && previous_top.as_deref() != Some("options") {
        state.reset_navigation();
        focus.widget = Some("options/tab/0".into());
    }
    *previous_top = top;
}

pub(crate) fn sync_display_resolutions(monitors: Query<&Monitor>, mut state: ResMut<OptionsState>) {
    let mut resolutions: Vec<_> = monitors
        .iter()
        .flat_map(|monitor| monitor.video_modes.iter())
        .map(|mode| frame::DisplayResolution::new(mode.physical_size.x, mode.physical_size.y))
        .filter(|mode| mode.width >= 640 && mode.height >= 480)
        .collect();
    resolutions.sort_by_key(|mode| (mode.width, mode.height));
    resolutions.dedup();
    if !resolutions.is_empty() && resolutions != state.display_resolutions {
        state.display_resolutions = resolutions;
        state.touch();
    }
}

pub(crate) fn drive_options_navigation(
    stack: Res<RetailMenuStack>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: MessageReader<MenuShellCmd>,
    mut pulse: ResMut<ActivatePulse>,
    mut state: ResMut<OptionsState>,
    mut focus: ResMut<Focus>,
    settings: Res<frame::GameSettings>,
    pointer_activations: Res<PointerActivation>,
    pressed: Query<(&Interaction, &PaintedWidget), (Changed<Interaction>, With<Button>)>,
) {
    if stack.names.last().map(String::as_str) != Some("options") {
        return;
    }

    if let Some(tab) = focus
        .widget
        .as_deref()
        .and_then(|id| id.strip_prefix("options/tab/"))
        .and_then(|raw| raw.parse::<u8>().ok())
        .and_then(OptionsTab::from_u8)
        && state.tab != tab
    {
        state.tab = tab;
        state.depth = OptionsDepth::Sections;
        state.touch();
    }

    if state.depth == OptionsDepth::Sections
        && focus.widget.as_deref().is_some_and(|id| {
            !id.starts_with("options/tab/") && options_pointer_widget_is_active(&state, id)
        })
    {
        state.depth = OptionsDepth::SectionRows;
        state.touch();
    }

    let mut left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA);
    let mut right = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD);
    for command in commands.read() {
        left |= matches!(command, MenuShellCmd::Nav(NavDir::Left));
        right |= matches!(command, MenuShellCmd::Nav(NavDir::Right));
    }
    if left
        && state.name_buffer.is_none()
        && !matches!(
            focus.widget.as_deref(),
            Some(
                "options/volume"
                    | "options/sensitivity"
                    | "options/fullscreen"
                    | "options/vsync"
                    | "options/invert_mouse"
            )
        )
    {
        if let Some(parent) = state.go_parent() {
            focus.widget = Some(parent);
        }
        return;
    }

    let mut activated = pointer_activations.0.clone();
    for (interaction, widget) in &pressed {
        if matches!(*interaction, Interaction::Pressed) {
            activated.push(widget.id.clone());
        }
    }
    if pulse.0
        && let Some(id) = focus.widget.clone()
        && !activated.contains(&id)
    {
        activated.push(id);
    }
    for id in activated {
        if let Some(tab) = id
            .strip_prefix("options/tab/")
            .and_then(|raw| raw.parse::<u8>().ok())
            .and_then(OptionsTab::from_u8)
        {
            state.tab = tab;
            state.depth = OptionsDepth::SectionRows;
            focus.widget = Some(state.first_row_id().to_owned());
            state.touch();
            pulse.0 = false;
            continue;
        }
        if id == "options/resolution" && state.depth == OptionsDepth::SectionRows {
            state.depth = OptionsDepth::ResolutionPicker;
            focus.widget = Some(if state.display_resolutions.is_empty() {
                "options/resolution_cancel".into()
            } else {
                let index = state
                    .display_resolutions
                    .iter()
                    .position(|resolution| *resolution == settings.resolution)
                    .unwrap_or(0);
                format!("options/resolution_choice/{index}")
            });
            state.touch();
            pulse.0 = false;
            continue;
        }
        if id == "options/resolution_cancel" && state.depth == OptionsDepth::ResolutionPicker {
            state.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/resolution".into());
            state.touch();
            pulse.0 = false;
            continue;
        }
        if state.tab == OptionsTab::Controls
            && state.depth == OptionsDepth::SectionRows
            && let Some(group) = OptionsControlGroup::from_widget_id(&id)
        {
            state.control_group = group;
            state.depth = OptionsDepth::ControlBinds;
            focus.widget = Some(format!("options/binds/{}", first_bind_id(group)));
            state.touch();
            pulse.0 = false;
        }
    }

    if right && state.depth == OptionsDepth::Sections {
        state.depth = OptionsDepth::SectionRows;
        focus.widget = Some(state.first_row_id().to_owned());
        state.touch();
        pulse.0 = false;
    }
}

pub(crate) const fn first_bind_id(group: OptionsControlGroup) -> u32 {
    match group {
        OptionsControlGroup::Movement => 27,
        OptionsControlGroup::Actions => 1,
        OptionsControlGroup::Look => 41,
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct BindingView {
    pub chords: BTreeMap<u32, String>,
    pub listening: Option<u32>,
    pub revision: u64,
}

impl BindingView {
    pub fn chord(&self, command_id: u32) -> &str {
        self.chords
            .get(&command_id)
            .map(String::as_str)
            .unwrap_or("UNBOUND")
    }
}

pub(crate) fn apply_option_intents(
    mut intents: MessageReader<UiIntent>,
    mut options: ResMut<OptionsState>,
    mut bindings: ResMut<BindingView>,
    mut settings: ResMut<frame::GameSettings>,
    mut focus: ResMut<Focus>,
) {
    for intent in intents.read() {
        match intent {
            UiIntent::SelectOptionsTab(raw) => {
                if let Some(tab) = OptionsTab::from_u8(*raw)
                    && tab != options.tab
                {
                    options.tab = tab;
                    options.touch();
                }
            }
            UiIntent::BeginPlayerNameEdit => {
                options.name_buffer = Some(settings.player_name.clone());
                options.name_cursor = settings.player_name.chars().count();
                options.touch();
                focus.widget = Some("options/name_buffer".into());
            }
            UiIntent::CommitPlayerNameEdit(value) => {
                let value: String = value.trim().chars().take(16).collect();
                if !value.is_empty() && settings.player_name != value {
                    settings.player_name = value;
                    settings.touch();
                }
                options.name_buffer = None;
                options.name_cursor = 0;
                options.touch();
                focus.widget = Some("options/player_name".into());
            }
            UiIntent::CancelPlayerNameEdit => {
                options.name_buffer = None;
                options.name_cursor = 0;
                options.touch();
                focus.widget = Some("options/player_name".into());
            }
            UiIntent::BeginBinding { id } => {
                bindings.listening = Some(*id);
                bindings.revision = bindings.revision.wrapping_add(1);
            }
            UiIntent::SetBinding { id, .. } => {
                if bindings.listening == Some(*id) {
                    bindings.listening = None;
                    bindings.revision = bindings.revision.wrapping_add(1);
                }
            }
            UiIntent::SetSetting { key, value } => {
                let changed = match (key, value) {
                    (SettingKey::Resolution, SettingValue::Resolution(value)) => {
                        let changed = replace(&mut settings.resolution, *value);
                        if options.depth == OptionsDepth::ResolutionPicker {
                            options.depth = OptionsDepth::SectionRows;
                            options.touch();
                            focus.widget = Some("options/resolution".into());
                        }
                        changed
                    }
                    (SettingKey::Fullscreen, SettingValue::Bool(value)) => {
                        replace(&mut settings.fullscreen, *value)
                    }
                    (SettingKey::Vsync, SettingValue::Bool(value)) => {
                        replace(&mut settings.vsync, *value)
                    }
                    (SettingKey::MasterVolume, SettingValue::Float(value)) => {
                        let value = value.clamp(0.0, 1.0);
                        replace(&mut settings.master_volume, value)
                    }
                    (SettingKey::Sensitivity, SettingValue::Float(value)) => {
                        let value = value.clamp(0.1, 30.0);
                        replace(&mut settings.sensitivity, value)
                    }
                    (SettingKey::InvertMouse, SettingValue::Bool(value)) => {
                        replace(&mut settings.invert_mouse, *value)
                    }
                    (SettingKey::PlayerName, SettingValue::Text(value)) => {
                        let value: String = value.trim().chars().take(16).collect();
                        !value.is_empty() && replace(&mut settings.player_name, value)
                    }
                    _ => false,
                };
                if changed {
                    settings.touch();
                }
            }
            _ => {}
        }
    }
}

fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

#[derive(Resource)]
pub struct PresentModeOverride(pub PresentMode);

pub(crate) fn apply_window_settings(
    settings: Res<frame::GameSettings>,
    present_override: Option<Res<PresentModeOverride>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut applied: Local<Option<(frame::DisplayResolution, bool, PresentMode)>>,
) {
    let present_mode = present_override.map_or_else(
        || {
            if settings.vsync {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            }
        },
        |mode| mode.0,
    );
    let display = (settings.resolution, settings.fullscreen, present_mode);
    if applied.as_ref() == Some(&display) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    window
        .resolution
        .set_physical_resolution(settings.resolution.width, settings.resolution.height);
    window.mode = if settings.fullscreen {
        bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        bevy::window::WindowMode::Windowed
    };
    window.present_mode = present_mode;
    *applied = Some(display);
}

pub(crate) fn edit_player_name(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut options: ResMut<OptionsState>,
    mut out: MessageWriter<UiIntent>,
) {
    let events: Vec<KeyboardInput> = keyboard.read().cloned().collect();
    let Some(mut buffer) = options.name_buffer.clone() else {
        return;
    };
    let mut changed = false;
    let mut commit = false;
    let mut cancel = false;
    for event in &events {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Character(text)
                if !keys.pressed(KeyCode::ControlLeft) && !keys.pressed(KeyCode::ControlRight) =>
            {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if buffer.chars().count() >= 16 {
                        break;
                    }
                    buffer.push(ch);
                    changed = true;
                }
            }
            Key::Backspace => {
                changed |= buffer.pop().is_some();
            }
            Key::Enter => commit = true,
            Key::Escape => cancel = true,
            _ => {}
        }
    }
    if changed {
        options.name_cursor = buffer.chars().count();
        options.touch();
    }
    if commit {
        out.write(UiIntent::CommitPlayerNameEdit(buffer.clone()));
    } else if cancel {
        out.write(UiIntent::CancelPlayerNameEdit);
    } else if changed {
        options.name_buffer = Some(buffer);
    }
}
