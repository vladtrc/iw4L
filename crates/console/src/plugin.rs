use bevy::{
    input::ButtonState,
    input::InputSystems,
    input::keyboard::{Key, KeyCode, KeyboardInput, NativeKeyCode},
    input::mouse::{MouseButton, MouseMotion},
    input_focus::{FocusCause, InputFocus, InputFocusSystems},
    picking::{
        events::{Drag, Pointer, Press, Release},
        pointer::PointerButton,
    },
    prelude::*,
    text::{EditableText, FontCx, LayoutCx, TextCursorStyle, TextEdit, TextLayoutInfo},
    ui::{ComputedUiRenderTargetInfo, UiGlobalTransform},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use frame::{AppScreen, HasWorld};
use input_iw4::{SCRIPT_KEYNUM, cl_input_cmd, cl_key_event, command_names, key_up_command_id};
use net::{ClientActionInput, ClientSet, PresentedSnapshot, com_frame_time_msec, key_frame_msec};
use render_frontend::prepare::scene::world::WorldScene;
use ui::{MenuEnabled, UiLayer};

use crate::{
    BINDABLE_KEYS, ConsoleCommand, ConsoleEditor, ConsoleInputState, KeyBinds,
    binds::{BindInputs, host_keynum},
    is_bind_command,
    registry::ConsoleRegistry,
    suggest::{SuggestSpan, SuggestTone, suggestion_spans},
};

const EMBEDDED_FONT: &[u8] = include_bytes!("../assets/FreeMono.otf");
const PROMPT: &str = "> ";
const FONT_SIZE: f32 = 15.0;
const COLOR_BODY: Color = Color::srgb(0.82, 0.92, 0.82);

const COLOR_MATCHED: Color = Color::srgb(0.40, 0.48, 0.42);

const COLOR_REST_SELECTED: Color = Color::srgb(0.95, 0.78, 0.35);

const COLOR_REST_MUTED: Color = Color::srgb(0.48, 0.58, 0.50);
const COLOR_PANEL: Color = Color::srgba(0.02, 0.03, 0.04, 0.96);

const COLOR_SUGGEST_BG: Color = Color::srgb(0.02, 0.03, 0.04);

#[derive(Resource, Clone)]
pub struct ConsoleSettings {
    pub log_capacity: usize,

    pub height: f32,

    pub width: f32,
}

impl Default for ConsoleSettings {
    fn default() -> Self {
        Self {
            log_capacity: 12,
            height: 360.0,
            width: 780.0,
        }
    }
}

#[derive(Resource)]
pub struct ConsoleFont(pub Handle<Font>);

#[derive(Resource, Default)]
pub struct ConsoleCommandQueue(pub std::collections::VecDeque<ConsoleCommand>);

impl ConsoleCommandQueue {
    pub fn push_script(&mut self, script: &str) {
        self.0.extend(ConsoleCommand::parse_script(script));
    }
}

#[derive(Resource, Default)]
pub struct ConsoleDispatch {
    pub paused: bool,

    pub wait_remaining: f32,

    pub wait_world: bool,
    pub wait_world_elapsed: f32,

    pub wait_spawn: bool,
    pub wait_spawn_elapsed: f32,

    pub wait_spawn_admit: bool,

    pub pending_spawn_class: Option<String>,

    pub wait_torn: bool,
    pub wait_torn_elapsed: f32,

    pub wait_ambient: bool,
    pub wait_ambient_elapsed: f32,

    pub wait_move: Option<WaitMovePose>,
    pub wait_move_elapsed: f32,

    pub wait_playing: bool,
    pub wait_playing_elapsed: f32,

    pub quit_jumps: u64,

    pub fifo_jumps: u64,
}

impl ConsoleDispatch {
    pub fn release(&mut self) {
        self.paused = false;
        self.wait_world = false;
        self.wait_spawn = false;
        self.wait_spawn_admit = false;
        self.pending_spawn_class = None;
        self.wait_torn = false;
        self.wait_ambient = false;
        self.wait_move = None;
        self.wait_playing = false;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaitMovePose {
    pub client: sim::ClientId,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

const WAIT_WORLD_TIMEOUT_SECS: f32 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WaitKind {
    Seconds(f32),
    World,
    Spawn,
    Torn,
    Ambient,
}

pub(crate) fn parse_wait_args(args: &[String]) -> WaitKind {
    match args.first().map(String::as_str) {
        Some(s) if s.eq_ignore_ascii_case("world") => WaitKind::World,
        Some(s) if s.eq_ignore_ascii_case("spawn") => WaitKind::Spawn,
        Some(s) if s.eq_ignore_ascii_case("torn") => WaitKind::Torn,
        Some(s) if s.eq_ignore_ascii_case("ambient") => WaitKind::Ambient,
        other => {
            let secs = other
                .and_then(|s| s.trim_end_matches(['s', 'S']).parse::<f32>().ok())
                .unwrap_or(1.0)
                .clamp(0.0, WAIT_WORLD_TIMEOUT_SECS);
            WaitKind::Seconds(secs)
        }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConsoleDispatchSet;

#[derive(Resource, Default)]
pub struct ConsoleState {
    pub open: bool,
    pub editor: ConsoleEditor,
    pub log: Vec<String>,

    pub history: Vec<String>,

    pub history_cursor: Option<usize>,

    pub history_draft: String,

    pub prompt_sel_n: usize,

    pub scroll_sel_n: usize,

    pub scroll_anchor: Option<usize>,

    pub scroll_focus: usize,

    pub scroll_gesture: bool,

    pub clipboard_write: Option<bool>,

    pub copy_source: Option<&'static str>,

    pub last_hit_x: Option<f32>,
    pub last_hit_y: Option<f32>,
    pub last_hit_char: Option<usize>,

    pub pending_feed: Option<String>,

    pub pending_os_paste: bool,

    pub feed_n: usize,

    pub clipboard_read: Option<&'static str>,
}

impl ConsoleState {
    pub fn echo(&mut self, line: impl Into<String>, capacity: usize) {
        self.log.push(line.into());
        if self.log.len() > capacity {
            let drop = self.log.len() - capacity;
            self.log.drain(0..drop);
        }
    }

    fn leave_history_browse(&mut self) {
        self.history_cursor = None;
        self.history_draft.clear();
    }

    fn history_recall(&mut self, older: bool) -> bool {
        if self.history.is_empty() {
            return false;
        }
        match self.history_cursor {
            None => {
                if !older || !self.editor.line.is_empty() {
                    return false;
                }
                self.history_draft = self.editor.line.clone();
                self.history_cursor = Some(self.history.len() - 1);
            }
            Some(index) => {
                if older {
                    if index == 0 {
                        return true;
                    }
                    self.history_cursor = Some(index - 1);
                } else if index + 1 >= self.history.len() {
                    let draft = std::mem::take(&mut self.history_draft);
                    self.history_cursor = None;
                    set_editor_line(&mut self.editor, &draft);
                    return true;
                } else {
                    self.history_cursor = Some(index + 1);
                }
            }
        }
        if let Some(index) = self.history_cursor {
            let line = self.history[index].clone();
            set_editor_line(&mut self.editor, &line);
        }
        true
    }
}

fn set_editor_line(editor: &mut ConsoleEditor, line: &str) {
    editor.line = line.to_owned();
    editor.caret = line.chars().count();
    editor.selected = 0;
}

#[derive(Component)]
struct ConsolePanel;

#[derive(Component)]
pub(crate) struct ConsoleLogText;

#[derive(Component)]
struct ConsoleInputText;

#[derive(Resource, Clone, Copy)]
struct ConsolePrompt(Entity);

#[derive(Resource, Clone, Copy)]
struct ConsoleLog(Entity);

#[derive(Component)]
struct ConsoleSuggestionText;

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleSettings>()
            .init_resource::<ConsoleState>()
            .init_resource::<frame::HudInputView>()
            .init_resource::<ConsoleInputState>()
            .init_resource::<KeyBinds>()
            .init_resource::<ConsoleCommandQueue>()
            .init_resource::<ConsoleDispatch>()
            .init_resource::<ConsoleRegistry>()
            .init_resource::<crate::ConsoleQueue>()
            .init_resource::<crate::ConsoleLine>()
            .init_resource::<crate::weapon_dispatch::WeaponArgCompletions>()
            .init_resource::<crate::feature_dispatch::ReplayProcessExit>()
            .init_resource::<crate::user_settings::PendingMenuBinding>()
            .init_resource::<crate::user_settings::UserSettingsPersistence>()
            .add_message::<ConsoleCommand>()
            .add_systems(Update, apply_ingame_menu_intents.after(ClientSet::Ui))
            .add_systems(
                Startup,
                (setup_console, crate::user_settings::load_user_settings).chain(),
            )
            .add_systems(PreUpdate, feed_console_keyboard.before(InputSystems))
            .add_systems(
                PreUpdate,
                (
                    handle_console_input,
                    handle_scrollback_pointer,
                    copy_console_selection_on_release,
                    isolate_gameplay_input,
                    expire_pressed_inputs,
                    publish_client_action_input,
                    sync_cursor_grab,
                )
                    .chain()
                    .after(InputSystems)
                    .after(InputFocusSystems::Dispatch),
            )
            .add_systems(
                Update,
                (
                    (
                        drain_startup_queue.before(dispatch_console_command),
                        dispatch_console_command.in_set(ConsoleDispatchSet),
                        handle_input_commands,
                        apply_console_os_paste,
                        crate::feature_dispatch::route_replay_commands,
                        crate::feature_dispatch::route_ui_commands,
                        crate::feature_dispatch::route_capture_commands,
                        crate::feature_dispatch::route_state_dump_commands,
                        crate::feature_dispatch::route_debug_feature_commands,
                        crate::feature_dispatch::route_session_commands,
                        crate::feature_dispatch::resume_lifecycle_commands,
                        crate::class_dispatch::route_class_commands,
                        crate::class_dispatch::complete_pending_spawn,
                        crate::weapon_dispatch::clear_weapon_args_on_torn_down,
                        crate::weapon_dispatch::refresh_weapon_arg_completions,
                        crate::weapon_dispatch::route_weapon_commands,
                    )
                        .chain(),
                    (
                        crate::weapon_dispatch::echo_give_results,
                        crate::debug_move::route_debug_move_commands,
                        crate::debug_script_mover::route_debug_script_mover_commands,
                        crate::debug_draw_method::route_debug_draw_method_commands,
                        crate::debug_view_proj::route_view_proj_commands,
                        (
                            crate::debug_dof::route,
                            crate::debug_distortion::route,
                            crate::debug_glow::route,
                        )
                            .chain(),
                        crate::debug_fog::route,
                        crate::debug_smc::route_smc_enable_commands,
                        crate::debug_sm::route_sm_commands,
                        crate::debug_lod::route_lod_ramp_commands,
                        crate::debug_cg_gun::route_cg_gun_commands,
                        crate::debug_cl_yawspeed::route_cl_yawspeed_commands,
                        crate::debug_fx::route_debug_fx_commands,
                        crate::debug_fx_marks::route_fx_mark_commands,
                        crate::user_settings::consume_menu_binding,
                        crate::user_settings::sync_binding_view,
                        crate::user_settings::apply_master_volume,
                        crate::user_settings::sync_player_name,
                        crate::user_settings::save_user_settings,
                        update_console_ui,
                    )
                        .chain(),
                )
                    .chain()
                    .in_set(ClientSet::Diag),
            )
            .add_systems(
                Last,
                (
                    paint_scrollback_selection,
                    crate::feature_dispatch::exit_replay_process,
                )
                    .chain(),
            );
        crate::diagnostics::register_diagnostics_mirror(app);
    }
}

fn drain_startup_queue(
    mut startup: ResMut<crate::ConsoleQueue>,
    mut queue: ResMut<ConsoleCommandQueue>,
) {
    for command in startup.drain() {
        queue.0.push_back(command);
    }
}

fn isolate_gameplay_input(
    console: Res<ConsoleState>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: MessageReader<MouseMotion>,
) {
    if !console.open {
        return;
    }
    keys.reset_all();
    for _ in mouse.read() {}
}

fn publish_client_action_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    binds: Res<KeyBinds>,
    mut scripted: ResMut<ConsoleInputState>,
    console: Res<ConsoleState>,
    menu: Res<MenuEnabled>,
    mut hud_input: ResMut<frame::HudInputView>,
    settings: Res<frame::GameSettings>,
    mut out: ResMut<ClientActionInput>,
) {
    hud_input.menu_open = menu.0;
    if binds.is_changed() || hud_input.use_key.is_none() {
        hud_input.use_key = binds
            .iter()
            .filter(|(button, _)| {
                matches!(
                    binds.binding_name(*button),
                    Some("+activate" | "+usereload")
                )
            })
            .map(|(button, _)| crate::binds::display_button(button).to_uppercase())
            .min();
    }
    if binds.is_changed() || hud_input.action_slot_keys.iter().all(Option::is_none) {
        hud_input.action_slot_keys = core::array::from_fn(|index| {
            binds
                .iter()
                .filter(|(_, id)| *id == 15 + index as u32 * 2)
                .map(|(button, _)| crate::binds::display_button(button).to_uppercase())
                .min()
        });
    }
    out.mouse_x = 0.0;
    out.mouse_y = 0.0;
    out.frame_msec = key_frame_msec(time.delta_secs());
    out.now_msec = com_frame_time_msec(time.elapsed_secs());
    out.sensitivity = settings.sensitivity;
    if out.m_yaw == 0.0 {
        out.m_yaw = 0.022;
    }
    out.m_pitch = if settings.invert_mouse { -0.022 } else { 0.022 };
    if out.fov_scale == 0.0 {
        out.fov_scale = 1.0;
    }

    let now = out.now_msec;
    let frame = out.frame_msec;

    if console.open || menu.0 || keys.just_pressed(KeyCode::Escape) {
        for _ in motion.read() {}
        for key_num in 0..input_iw4::KEY_COUNT {
            if out.client.keys[key_num].down != 0 {
                cl_key_event(&mut out.client, key_num, false, now, frame);
            }
        }
        return;
    }

    let inputs = BindInputs::new(&keys, &mouse_buttons);
    for (button, id) in binds.iter() {
        let key_num = host_keynum(button);
        if key_num >= input_iw4::KEY_COUNT {
            continue;
        }
        out.client.keys[key_num].binding = id;
        if inputs.just_pressed(button) {
            cl_key_event(&mut out.client, key_num, true, now, frame);
        }
        if inputs.just_released(button) {
            cl_key_event(&mut out.client, key_num, false, now, frame);
        }
    }

    let scripted_now: std::collections::BTreeSet<u32> = scripted.ids().collect();
    let down: Vec<u32> = scripted_now
        .difference(&out.scripted_ids)
        .copied()
        .collect();
    let up: Vec<u32> = out
        .scripted_ids
        .difference(&scripted_now)
        .copied()
        .collect();
    for id in down {
        let extra = now.wrapping_sub(frame as i32);
        let extra = if extra == 0 { -(frame as i32) } else { extra };
        cl_input_cmd(&mut out.client, id, SCRIPT_KEYNUM, extra, frame);
    }
    for id in up {
        if let Some(up_id) = key_up_command_id(id) {
            cl_input_cmd(&mut out.client, up_id, SCRIPT_KEYNUM, now, frame);
        }
    }
    out.scripted_ids = scripted_now;

    let (sx, sy) = scripted.take_mouse();
    let (rx, ry) = scripted.mouse_rate().unwrap_or((0.0, 0.0));
    out.mouse_x += sx + rx;
    out.mouse_y += sy + ry;
    for ev in motion.read() {
        out.mouse_x += ev.delta.x;
        out.mouse_y += ev.delta.y;
    }
}

fn sync_cursor_grab(
    console: Res<ConsoleState>,
    menu: Option<Res<MenuEnabled>>,
    screen: Option<Res<AppScreen>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let menu_open = menu.map(|m| m.0).unwrap_or(false);
    let in_game = screen
        .as_ref()
        .is_some_and(|s| matches!(**s, AppScreen::InGame));
    let grab = in_game && !console.open && !menu_open;
    let Ok(mut cursor) = windows.single_mut() else {
        return;
    };
    let want = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if cursor.grab_mode != want {
        cursor.grab_mode = want;
        cursor.visible = !grab;
        let grab_label = if grab { "locked" } else { "none" };
        diag::event!(Input, Debug, "cursor", "cursor grab={grab_label}");
    }
}

fn setup_console(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    settings: Res<ConsoleSettings>,
    mut registry: ResMut<ConsoleRegistry>,
    mut binds: ResMut<KeyBinds>,
    weapon_completions: Res<crate::weapon_dispatch::WeaponArgCompletions>,
) {
    binds.apply_defaults();
    if registry.resolve("hold").is_none() {
        registry.register(
            crate::CommandSpec::new("hold")
                .usage("hold <+input> — keep an input active until release")
                .arg(crate::StaticCompleter::new(command_names())),
        );
    }
    if registry.resolve("press").is_none() {
        registry.register(
            crate::CommandSpec::new("press")
                .usage("press <+input> [seconds] — hold an input, then release it")
                .arg(crate::StaticCompleter::new(command_names())),
        );
    }
    if registry.resolve("mousemove").is_none() {
        registry.register(
            crate::CommandSpec::new("mousemove")
                .usage("mousemove <dx> [dy] — inject one-shot mouse pixels into CL_MouseMove"),
        );
    }
    if registry.resolve("mouserate").is_none() {
        registry.register(
            crate::CommandSpec::new("mouserate")
                .usage("mouserate <dx> [dy] — add mouse pixels every frame until mouserate 0"),
        );
    }
    if registry.resolve("release").is_none() {
        registry.register(
            crate::CommandSpec::new("release")
                .usage("release <+input|all> — stop a held console input")
                .arg(crate::StaticCompleter::new(command_names().chain(["all"]))),
        );
    }
    if registry.resolve("bind").is_none() {
        registry.register(
            crate::CommandSpec::new("bind")
                .usage("bind [key] [command] — set or list key binds")
                .arg(crate::StaticCompleter::new(BINDABLE_KEYS.iter().copied()))
                .arg(crate::StaticCompleter::new(command_names())),
        );
    }
    if registry.resolve("unbind").is_none() {
        registry.register(
            crate::CommandSpec::new("unbind")
                .usage("unbind <key> — clear a key bind")
                .arg(crate::StaticCompleter::new(BINDABLE_KEYS.iter().copied())),
        );
    }
    if registry.resolve("unbindall").is_none() {
        registry.register(
            crate::CommandSpec::new("unbindall").usage("unbindall — clear every key bind"),
        );
    }
    if registry.resolve("binddefaults").is_none() {
        registry.register(
            crate::CommandSpec::new("binddefaults")
                .usage("binddefaults — restore the default control script"),
        );
    }
    if registry.resolve("con_toggle").is_none() {
        registry.register(
            crate::CommandSpec::new("con_toggle")
                .usage("con_toggle — open/close the overlay (debug; not a retail name)"),
        );
    }
    if registry.resolve("con_feed").is_none() {
        registry.register(
            crate::CommandSpec::new("con_feed")
                .usage("con_feed <text> — inject KeyboardInput.text (debug; not OS winit)"),
        );
    }
    if registry.resolve("con_paste").is_none() {
        registry.register(
            crate::CommandSpec::new("con_paste")
                .usage("con_paste — insert OS clipboard into the prompt (debug)"),
        );
    }

    let presets: Vec<String> = ui::default_presets()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    if registry.resolve("class").is_none() {
        registry.register(
            crate::CommandSpec::new("class")
                .usage("class [name|index] — list presets, or select one for spawn")
                .arg(crate::StaticCompleter::new(presets.clone())),
        );
    }
    registry.register(
        crate::CommandSpec::new("mark")
            .usage("mark <label> — log a monotonic engine timestamp for external timing"),
    );
    if registry.resolve("spawn").is_none() {
        registry.register(
            crate::CommandSpec::new("spawn")
                .usage("spawn [name|index] — Equip the selected class and enter the match")
                .arg(crate::StaticCompleter::new(presets)),
        );
    }
    crate::weapon_dispatch::register_weapon_commands(&mut registry, &weapon_completions);
    crate::debug_move::register_debug_move_commands(&mut registry);
    crate::debug_script_mover::register_debug_script_mover_commands(&mut registry);
    crate::debug_draw_method::register_debug_draw_method_commands(&mut registry);
    crate::debug_view_proj::register_view_proj_commands(&mut registry);
    crate::debug_dof::register(&mut registry);
    crate::debug_glow::register(&mut registry);
    crate::debug_distortion::register(&mut registry);
    crate::debug_fog::register(&mut registry);
    crate::debug_smc::register_smc_enable_commands(&mut registry);
    crate::debug_sm::register_sm_commands(&mut registry);
    crate::debug_lod::register_lod_ramp_commands(&mut registry);
    crate::debug_cg_gun::register_cg_gun_commands(&mut registry);
    crate::debug_cl_yawspeed::register_cl_yawspeed_command(&mut registry);
    crate::debug_fx::register_debug_fx_commands(&mut registry);
    crate::debug_fx_marks::register_fx_mark_commands(&mut registry);
    binds.apply_defaults();

    let maps = assets::games_root_from_env()
        .map(|root| assets::list_mp_maps(&root))
        .unwrap_or_default();
    crate::feature_dispatch::register_feature_commands(&mut registry, &maps);
    let font = fonts.add(Font::from_bytes(EMBEDDED_FONT.to_vec()));
    commands.insert_resource(ConsoleFont(font.clone()));

    let mut prompt_entity = None;
    let mut log_entity = None;
    commands
        .spawn((
            ConsolePanel,
            UiLayer::Overlay,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(12),
                width: px(settings.width),
                height: px(settings.height),
                padding: UiRect::all(px(12)),
                ..default()
            },
            BackgroundColor(COLOR_PANEL),
            GlobalZIndex(20_000),
            ZIndex(100),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    right: px(12),
                    top: px(12),
                    bottom: px(36),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::End,
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|clip| {
                    log_entity = Some(
                        clip.spawn((
                            ConsoleLogText,
                            Text::new(""),
                            TextFont {
                                font: font.clone().into(),
                                font_size: FontSize::Px(FONT_SIZE),
                                ..default()
                            },
                            TextColor(COLOR_BODY),
                            TextCursorStyle {
                                color: Color::NONE,
                                selection_color: Color::srgba(0.35, 0.55, 0.85, 0.45),
                                unfocused_selection_color: Color::srgba(0.35, 0.55, 0.85, 0.45),
                                selected_text_color: None,
                            },
                        ))
                        .id(),
                    );
                });

            panel.spawn((
                ConsoleSuggestionText,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    bottom: px(36),
                    padding: UiRect::axes(px(0), px(2)),
                    ..default()
                },
                BackgroundColor(COLOR_SUGGEST_BG),
                Visibility::Hidden,
                Text::new(""),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(FONT_SIZE),
                    ..default()
                },
                TextColor(COLOR_REST_MUTED),
            ));
            panel
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    right: px(12),
                    bottom: px(12),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(PROMPT),
                        TextFont {
                            font: font.clone().into(),
                            font_size: FontSize::Px(FONT_SIZE),
                            ..default()
                        },
                        TextColor(COLOR_BODY),
                    ));
                    prompt_entity = Some(
                        row.spawn((
                            ConsoleInputText,
                            EditableText {
                                allow_newlines: false,
                                visible_lines: Some(1.0),
                                ..default()
                            },
                            TextLayout::no_wrap(),
                            TextFont {
                                font: font.clone().into(),
                                font_size: FontSize::Px(FONT_SIZE),
                                ..default()
                            },
                            TextColor(COLOR_BODY),
                            TextCursorStyle {
                                color: COLOR_BODY,
                                selection_color: Color::srgba(0.35, 0.55, 0.85, 0.45),
                                unfocused_selection_color: Color::srgba(0.35, 0.55, 0.85, 0.2),
                                selected_text_color: None,
                            },
                            Node {
                                flex_grow: 1.0,
                                min_width: px(8),
                                height: px(20),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                        ))
                        .id(),
                    );
                });
        });
    commands.insert_resource(ConsolePrompt(prompt_entity.expect("console prompt entity")));
    commands.insert_resource(ConsoleLog(log_entity.expect("console log entity")));
    crate::debug_move::spawn_showpos_hud(&mut commands, font);
}

fn is_console_input(word: &str) -> bool {
    is_bind_command(word)
}

fn expire_pressed_inputs(time: Res<Time>, mut inputs: ResMut<ConsoleInputState>) {
    inputs.tick(time.delta_secs());
}

fn handle_input_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut inputs: ResMut<ConsoleInputState>,
    mut binds: ResMut<KeyBinds>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
) {
    for command in events.read() {
        match command.name.as_str() {
            name if name.starts_with('+') && is_console_input(name) => {
                inputs.hold(name);
            }
            name if let Some(action) = name.strip_prefix('-') => {
                let plus = format!("+{action}");
                if is_console_input(&plus) {
                    inputs.release(&plus);
                }
            }
            "hold" => match command.args.as_slice() {
                [input] if is_console_input(input) => {
                    inputs.hold(input);
                    console.echo(format!("held: {input}"), settings.log_capacity);
                }
                _ => console.echo("usage: hold <+input>", settings.log_capacity),
            },
            "press" => match command.args.as_slice() {
                [input, rest @ ..] if is_console_input(input) => {
                    let seconds = rest
                        .first()
                        .and_then(|s| s.trim_end_matches(['s', 'S']).parse::<f32>().ok())
                        .unwrap_or(crate::PRESS_SECONDS)
                        .clamp(0.0, 60.0);
                    inputs.press(input, seconds);
                    console.echo(
                        format!("pressed: {input} ({seconds:.2}s)"),
                        settings.log_capacity,
                    );
                }
                _ => console.echo("usage: press <+input> [seconds]", settings.log_capacity),
            },
            "mousemove" => match command.args.as_slice() {
                [dx] => match dx.parse::<f32>() {
                    Ok(x) => {
                        inputs.queue_mouse(x, 0.0);
                        console.echo(format!("mousemove {x} 0"), settings.log_capacity);
                    }
                    Err(_) => console.echo("usage: mousemove <dx> [dy]", settings.log_capacity),
                },
                [dx, dy] => match (dx.parse::<f32>(), dy.parse::<f32>()) {
                    (Ok(x), Ok(y)) => {
                        inputs.queue_mouse(x, y);
                        console.echo(format!("mousemove {x} {y}"), settings.log_capacity);
                    }
                    _ => console.echo("usage: mousemove <dx> [dy]", settings.log_capacity),
                },
                _ => console.echo("usage: mousemove <dx> [dy]", settings.log_capacity),
            },
            "mouserate" => match command.args.as_slice() {
                [dx] => match dx.parse::<f32>() {
                    Ok(x) => {
                        inputs.set_mouse_rate(x, 0.0);
                        console.echo(format!("mouserate {x} 0"), settings.log_capacity);
                    }
                    Err(_) => console.echo("usage: mouserate <dx> [dy]", settings.log_capacity),
                },
                [dx, dy] => match (dx.parse::<f32>(), dy.parse::<f32>()) {
                    (Ok(x), Ok(y)) => {
                        inputs.set_mouse_rate(x, y);
                        console.echo(format!("mouserate {x} {y}"), settings.log_capacity);
                    }
                    _ => console.echo("usage: mouserate <dx> [dy]", settings.log_capacity),
                },
                _ => console.echo("usage: mouserate <dx> [dy]", settings.log_capacity),
            },
            "release" => match command.args.as_slice() {
                [input] if input == "all" => {
                    inputs.clear();
                    console.echo("held inputs cleared", settings.log_capacity);
                }
                [input] if is_console_input(input) => {
                    inputs.release(input);
                    console.echo(format!("released: {input}"), settings.log_capacity);
                }
                _ => console.echo("usage: release <+input|all>", settings.log_capacity),
            },
            "bind" => match command.args.as_slice() {
                [] => {
                    let lines = binds.list_lines();
                    if lines.is_empty() {
                        console.echo("no binds", settings.log_capacity);
                    } else {
                        for line in lines {
                            console.echo(line, settings.log_capacity);
                        }
                    }
                }
                args => match binds.apply_script(&format!(
                    "bind {}",
                    args.iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                )) {
                    echoes if !echoes.is_empty() => {
                        for line in echoes {
                            console.echo(line, settings.log_capacity);
                        }
                    }
                    _ => {}
                },
            },
            "unbind" => {
                for line in binds.apply_script(&format!("unbind {}", command.args.join(" "))) {
                    console.echo(line, settings.log_capacity);
                }
            }
            "unbindall" => {
                binds.clear_all();
                console.echo("unbindall", settings.log_capacity);
            }
            "binddefaults" => {
                binds.apply_defaults();
                console.echo("restored default binds", settings.log_capacity);
            }

            "con_toggle" => {
                console.open = !console.open;
                let line = if console.open {
                    "console open"
                } else {
                    "console closed"
                };
                console.echo(line, settings.log_capacity);
            }
            "con_feed" => {
                let text = command.args.join(" ");
                if text.is_empty() {
                    console.echo("usage: con_feed <text>", settings.log_capacity);
                } else {
                    console.pending_feed = Some(text);
                }
            }
            "con_paste" => {
                console.pending_os_paste = true;
            }
            _ => {}
        }
    }
}

fn feed_console_keyboard(
    mut state: ResMut<ConsoleState>,
    mut writer: MessageWriter<KeyboardInput>,
    window: Query<Entity, With<PrimaryWindow>>,
) {
    state.feed_n = 0;
    let Some(text) = state.pending_feed.take() else {
        return;
    };
    if !state.open {
        return;
    }
    let Ok(window) = window.single() else {
        return;
    };
    state.feed_n = text.chars().count();
    for ch in text.chars() {
        let logical = Key::Character(ch.to_string().into());
        let commit = Some(ch.to_string().into());
        writer.write(KeyboardInput {
            key_code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
            logical_key: logical.clone(),
            state: ButtonState::Pressed,
            text: commit,
            repeat: false,
            window,
        });
        writer.write(KeyboardInput {
            key_code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
            logical_key: logical,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window,
        });
    }
}

fn apply_console_os_paste(
    mut state: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut clipboard: ResMut<Clipboard>,
    mut font_cx: ResMut<FontCx>,
    mut layout_cx: ResMut<LayoutCx>,
    prompt: Option<Res<ConsolePrompt>>,
    mut editables: Query<&mut EditableText, With<ConsoleInputText>>,
) {
    state.clipboard_read = None;
    if !state.pending_os_paste {
        return;
    }
    state.pending_os_paste = false;
    if !state.open {
        state.echo("con_paste: console closed", settings.log_capacity);
        return;
    }
    let Some(entity) = prompt.map(|p| p.0) else {
        return;
    };
    let Ok(mut editable) = editables.get_mut(entity) else {
        return;
    };
    let mut read = clipboard.fetch_text();
    match read.poll_result() {
        Some(Ok(text)) if text.is_empty() => {
            state.clipboard_read = Some("empty");
            state.echo("clipboard empty", settings.log_capacity);
        }
        Some(Ok(text)) => {
            let n = text.chars().count();
            state.clipboard_read = Some("ok");
            editable.queue_edit(TextEdit::Insert(text.into()));
            apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
            sanitize_prompt_newlines(&mut editable);
            sync_editor_from_widget(&mut state.editor, &editable);
            state.echo(format!("pasted {n} chars"), settings.log_capacity);
        }
        Some(Err(error)) => {
            state.clipboard_read = Some("err");
            state.echo(
                format!("clipboard read failed: {error}"),
                settings.log_capacity,
            );
        }
        None => {
            state.clipboard_read = Some("pending");
            state.echo("clipboard read pending", settings.log_capacity);
        }
    }
}

fn handle_console_input(
    keys: Res<ButtonInput<KeyCode>>,
    logical_keys: Res<ButtonInput<Key>>,
    mut events: MessageReader<KeyboardInput>,
    mut state: ResMut<ConsoleState>,
    registry: Res<ConsoleRegistry>,
    settings: Res<ConsoleSettings>,
    mut queue: ResMut<ConsoleCommandQueue>,
    mut clipboard: ResMut<Clipboard>,
    mut font_cx: ResMut<FontCx>,
    mut layout_cx: ResMut<LayoutCx>,
    mut focus: ResMut<InputFocus>,
    prompt: Option<Res<ConsolePrompt>>,
    mut editables: Query<&mut EditableText, With<ConsoleInputText>>,
) {
    let prompt_entity = prompt.map(|p| p.0);
    if keys.just_pressed(KeyCode::Backquote) {
        state.open = !state.open;
        if let Some(entity) = prompt_entity
            && let Ok(mut editable) = editables.get_mut(entity)
        {
            editable.pending_edits.clear();
        }
        sync_prompt_focus(
            state.open,
            prompt_entity,
            &mut focus,
            &mut editables,
            &mut state.editor,
        );
        if !state.open {
            clear_scrollback_selection(&mut state);
        }
        return;
    }
    if !state.open {
        clear_scrollback_selection(&mut state);
        return;
    }

    let Some(prompt_entity) = prompt_entity else {
        return;
    };
    if focus.get() != Some(prompt_entity) {
        focus.set(prompt_entity, FocusCause::Navigated);
    }
    let Ok(mut editable) = editables.get_mut(prompt_entity) else {
        return;
    };

    let control = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || logical_keys.pressed(Key::Control);

    let mut want_close = false;
    let mut want_clear = false;
    let mut want_home = false;
    let mut want_end = false;
    let mut want_kill_word = false;
    let mut want_tab = false;
    let mut want_submit = false;
    let mut arrow_up = false;
    let mut arrow_down = false;

    for event in events.read().filter(|event| event.state.is_pressed()) {
        if control {
            match event.key_code {
                KeyCode::KeyW => {
                    want_kill_word = true;
                    continue;
                }
                KeyCode::KeyU => {
                    want_clear = true;
                    continue;
                }
                KeyCode::KeyA => {
                    want_home = true;
                    continue;
                }
                KeyCode::KeyE => {
                    want_end = true;
                    continue;
                }

                _ => {}
            }
        }

        match &event.logical_key {
            Key::Escape => want_close = true,
            Key::ArrowUp => arrow_up = true,
            Key::ArrowDown => arrow_down = true,
            Key::Tab => want_tab = true,
            Key::Enter => want_submit = true,
            _ => {}
        }
    }

    if want_close {
        state.open = false;
        editable.pending_edits.clear();
        focus.clear();
        return;
    }

    editable
        .pending_edits
        .retain(|edit| !is_widget_edit_console_override(edit));

    if want_clear {
        editable.clear();
        state.editor.line.clear();
        state.editor.caret = 0;
        state.editor.selected = 0;
        state.leave_history_browse();
        return;
    }
    if want_home {
        editable.queue_edit(TextEdit::LineStart(false));
    }
    if want_end {
        editable.queue_edit(TextEdit::LineEnd(false));
    }

    apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
    sanitize_prompt_newlines(&mut editable);
    apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
    sync_editor_from_widget(&mut state.editor, &editable);

    if want_kill_word {
        state.editor.kill_word();
        state.leave_history_browse();
        write_widget_from_editor(&mut editable, &state.editor);
        apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
        sync_editor_from_widget(&mut state.editor, &editable);
    }

    if arrow_up {
        if (state.history_cursor.is_some() || state.editor.line.is_empty())
            && state.history_recall(true)
        {
            write_widget_from_editor(&mut editable, &state.editor);
            apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
        } else {
            let len = console_suggestions(&state, &registry).len();
            if len > 0 {
                state.editor.selected = (state.editor.selected + 1) % len;
            }
        }
    } else if arrow_down {
        if state.history_cursor.is_some() {
            state.history_recall(false);
            write_widget_from_editor(&mut editable, &state.editor);
            apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
        } else {
            let len = console_suggestions(&state, &registry).len();
            if len > 0 {
                state.editor.selected = (state.editor.selected + len - 1) % len;
            }
        }
    }

    if want_tab {
        let suggestions = console_suggestions(&state, &registry);
        if state.editor.accept(&suggestions) {
            state.leave_history_browse();
            write_widget_from_editor(&mut editable, &state.editor);
            apply_prompt_edits(&mut editable, &mut font_cx, &mut layout_cx, &mut clipboard);
            sync_editor_from_widget(&mut state.editor, &editable);
        }
    }

    if want_submit {
        if state.history_cursor.is_none() {
            let suggestions = console_suggestions(&state, &registry);
            if !suggestions.is_empty() {
                state.editor.accept(&suggestions);
            }
        }
        let line = state.editor.take();
        state.leave_history_browse();
        editable.clear();
        if line.trim().is_empty() {
            return;
        }
        state.echo(format!("> {line}"), settings.log_capacity);
        if state.history.last() != Some(&line) {
            state.history.push(line.clone());
            if state.history.len() > 100 {
                state.history.remove(0);
            }
        }
        for command in ConsoleCommand::parse_script(&line) {
            if command.name == "help" {
                for line in registry.help_lines() {
                    state.echo(line, settings.log_capacity);
                }
            } else if command.name == "clear" {
                state.log.clear();
            } else if registry.resolve(&command.name).is_none() {
                state.echo(
                    format!("unknown command `{}`", command.name),
                    settings.log_capacity,
                );
            } else {
                let mut command = command;
                command.interactive = true;
                queue.0.push_back(command);
            }
        }
    }
}

fn copy_console_selection_on_release(
    mut releases: MessageReader<Pointer<Release>>,
    prompt: Query<&EditableText, With<ConsoleInputText>>,
    mut state: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut clipboard: ResMut<Clipboard>,
) {
    state.clipboard_write = None;
    state.copy_source = None;
    if !state.open {
        state.prompt_sel_n = 0;
        clear_scrollback_selection(&mut state);
        return;
    }
    let Ok(editable) = prompt.single() else {
        return;
    };
    state.prompt_sel_n = editable
        .editor()
        .selected_text()
        .map(|s| s.chars().count())
        .unwrap_or(0);
    if let Some(anchor) = state.scroll_anchor {
        state.scroll_sel_n = crate::input::selection_len(anchor, state.scroll_focus);
    } else {
        state.scroll_sel_n = 0;
    }
    let primary_up = releases
        .read()
        .any(|release| release.button == PointerButton::Primary);
    if !primary_up {
        return;
    }
    let scroll_gesture = state.scroll_gesture;
    state.scroll_gesture = false;
    let result = if scroll_gesture {
        let joined = state.log.join("\n");
        let selected = state.scroll_anchor.and_then(|anchor| {
            let slice = crate::input::slice_char_range(&joined, anchor, state.scroll_focus);
            (!slice.is_empty()).then_some(slice.to_owned())
        });
        crate::input::copy_prompt_selection(selected.as_deref(), |text| {
            clipboard.set_text(text).map_err(|error| error.to_string())
        })
    } else {
        crate::input::copy_prompt_selection(editable.editor().selected_text(), |text| {
            clipboard.set_text(text).map_err(|error| error.to_string())
        })
    };
    let source = if scroll_gesture { "scroll" } else { "prompt" };
    match &result {
        crate::input::PromptCopy::Copied => {
            state.clipboard_write = Some(true);
            state.copy_source = Some(source);
        }
        crate::input::PromptCopy::Failed(_) => {
            state.clipboard_write = Some(false);
            state.copy_source = Some(source);
        }
        crate::input::PromptCopy::Ignored => {}
    }
    if let Some(line) = crate::input::copy_prompt_echo(&result) {
        state.echo(line, settings.log_capacity);
    }
}

fn handle_scrollback_pointer(
    mut presses: MessageReader<Pointer<Press>>,
    mut drags: MessageReader<Pointer<Drag>>,
    prompt: Option<Res<ConsolePrompt>>,
    log: Option<Res<ConsoleLog>>,
    mut state: ResMut<ConsoleState>,
    ui_scale: Res<UiScale>,
    log_layout: Query<
        (
            &ComputedNode,
            &ComputedUiRenderTargetInfo,
            &UiGlobalTransform,
            &TextLayoutInfo,
            &ChildOf,
        ),
        With<ConsoleLogText>,
    >,
) {
    if !state.open {
        return;
    }
    let Some(log_entity) = log.map(|l| l.0) else {
        return;
    };
    let prompt_entity = prompt.map(|p| p.0);

    let Ok((node, target, transform, layout, log_parent)) = log_layout.get(log_entity) else {
        return;
    };
    let clip_entity = log_parent.0;
    let on_log = |entity: Entity| entity == log_entity || entity == clip_entity;
    let joined = state.log.join("\n");
    for press in presses.read() {
        if press.button != PointerButton::Primary {
            continue;
        }
        if Some(press.entity) == prompt_entity {
            clear_scrollback_selection(&mut state);
            continue;
        }
        if !on_log(press.entity) {
            continue;
        }
        let Some(local) = pointer_to_text_local(
            transform,
            node,
            target,
            ui_scale.0,
            press.pointer_location.position,
        ) else {
            continue;
        };
        state.scroll_gesture = true;
        match press.count {
            2 => {
                let Some((_, _, ch)) = hit_scrollback_cell(&joined, layout, local) else {
                    continue;
                };
                record_scroll_hit(&mut state, local, ch);
                let (lo, hi) = crate::input::word_bounds(&joined, ch);
                state.scroll_anchor = Some(lo);
                state.scroll_focus = hi;
            }
            n if n >= 3 => {
                let Some((row, _, ch)) = hit_scrollback_cell(&joined, layout, local) else {
                    continue;
                };
                record_scroll_hit(&mut state, local, ch);
                let cells = scrollback_cells(&joined, layout);
                let runs = scrollback_runs(layout);
                match crate::input::line_char_range(&cells, &runs, row) {
                    Some((lo, hi)) => {
                        state.scroll_anchor = Some(lo);
                        state.scroll_focus = hi;
                    }
                    None => continue,
                }
            }
            _ => {
                let Some((_, _, ch)) = hit_scrollback_cell(&joined, layout, local) else {
                    continue;
                };
                record_scroll_hit(&mut state, local, ch);
                state.scroll_anchor = Some(ch);
                state.scroll_focus = ch;
            }
        }
        state.scroll_sel_n = state
            .scroll_anchor
            .map(|anchor| crate::input::selection_len(anchor, state.scroll_focus))
            .unwrap_or(0);
    }
    if !state.scroll_gesture {
        return;
    }
    for drag in drags.read() {
        if drag.button != PointerButton::Primary || !on_log(drag.entity) {
            continue;
        }
        let Some(local) = pointer_to_text_local(
            transform,
            node,
            target,
            ui_scale.0,
            drag.pointer_location.position,
        ) else {
            continue;
        };
        let Some((_, _, ch)) = hit_scrollback_cell(&joined, layout, local) else {
            continue;
        };
        record_scroll_hit(&mut state, local, ch);
        state.scroll_focus = ch;
        state.scroll_sel_n = state
            .scroll_anchor
            .map(|anchor| crate::input::selection_len(anchor, ch))
            .unwrap_or(0);
    }
}

fn paint_scrollback_selection(
    state: Res<ConsoleState>,
    ui_scale: Res<UiScale>,
    mut logs: Query<
        (
            &mut TextLayoutInfo,
            &ComputedNode,
            &ComputedUiRenderTargetInfo,
        ),
        With<ConsoleLogText>,
    >,
) {
    let Ok((mut layout, node, target)) = logs.single_mut() else {
        return;
    };
    layout.selection_rects.clear();
    let Some(anchor) = state.scroll_anchor else {
        return;
    };
    let (lo, hi) = crate::input::ordered_char_range(anchor, state.scroll_focus);
    if lo == hi {
        return;
    }
    let joined = state.log.join("\n");

    let cells = scrollback_cells(&joined, &layout);
    let runs = scrollback_runs(&layout);
    let text_right = runs.iter().map(|run| run.2).fold(0.0_f32, f32::max);
    let panel_right = node.content_box().width() * target.scale_factor() / ui_scale.0;
    let block_right = text_right.max(panel_right);
    layout.selection_rects =
        crate::input::cell_rects_for_char_range(lo, hi, &cells, &runs, block_right)
            .into_iter()
            .map(|(x0, y0, x1, y1)| Rect::new(x0, y0, x1, y1))
            .collect();
}

fn pointer_to_text_local(
    transform: &UiGlobalTransform,
    node: &ComputedNode,
    target: &ComputedUiRenderTargetInfo,
    ui_scale: f32,
    pointer: Vec2,
) -> Option<Vec2> {
    transform.try_inverse().map(|inverse| {
        inverse.transform_point2(pointer * target.scale_factor() / ui_scale)
            - node.content_box().min
    })
}

fn hit_scrollback_cell(
    text: &str,
    layout: &TextLayoutInfo,
    local: Vec2,
) -> Option<(usize, usize, usize)> {
    let cells = scrollback_cells(text, layout);
    let runs = scrollback_runs(layout);
    let (row, col) = crate::input::hit_scroll_cell(&cells, &runs, (local.x, local.y))?;
    let ch = crate::input::cell_char_index(&cells, &runs, row, col)?;
    Some((row, col, ch))
}

fn scrollback_cells(text: &str, layout: &TextLayoutInfo) -> Vec<crate::input::ScrollCell> {
    let mapped = crate::input::map_glyphs_to_chars(text, layout.glyphs.len());
    layout
        .glyphs
        .iter()
        .zip(mapped)
        .map(|(glyph, ch)| (glyph.line_index, ch))
        .collect()
}

fn scrollback_runs(layout: &TextLayoutInfo) -> Vec<crate::input::RunBounds> {
    layout
        .run_geometry
        .iter()
        .map(|run| {
            (
                run.bounds.min.x,
                run.bounds.min.y,
                run.bounds.max.x,
                run.bounds.max.y,
            )
        })
        .collect()
}

fn record_scroll_hit(state: &mut ConsoleState, local: Vec2, ch: usize) {
    state.last_hit_x = Some(local.x);
    state.last_hit_y = Some(local.y);
    state.last_hit_char = Some(ch);
}

fn clear_scrollback_selection(state: &mut ConsoleState) {
    state.scroll_anchor = None;
    state.scroll_focus = 0;
    state.scroll_sel_n = 0;
    state.scroll_gesture = false;
}

fn is_widget_edit_console_override(edit: &TextEdit) -> bool {
    matches!(
        edit,
        TextEdit::Up(_) | TextEdit::Down(_) | TextEdit::SelectAll | TextEdit::CollapseSelection
    )
}

fn apply_prompt_edits(
    editable: &mut EditableText,
    font_cx: &mut FontCx,
    layout_cx: &mut LayoutCx,
    clipboard: &mut Clipboard,
) {
    if editable.pending_edits.is_empty() && editable.pending_paste.is_none() {
        return;
    }
    editable.apply_pending_edits(&mut *font_cx, &mut layout_cx.0, clipboard, |_| true);
}

fn sanitize_prompt_newlines(editable: &mut EditableText) {
    let value = editable.value().to_string();
    if !value.bytes().any(|b| b == b'\n' || b == b'\r') {
        return;
    }
    let cleaned = crate::input::normalize_command_paste(&value);
    editable.clear();
    editable.editor.set_text(&cleaned);
    editable.queue_edit(TextEdit::TextEnd(false));
}

fn sync_editor_from_widget(editor: &mut ConsoleEditor, editable: &EditableText) {
    let line = editable.value().to_string();
    let byte = editable.editor().raw_selection().focus().index();
    editor.caret = line
        .char_indices()
        .take_while(|(index, _)| *index < byte)
        .count();
    editor.line = line;
}

fn write_widget_from_editor(editable: &mut EditableText, editor: &ConsoleEditor) {
    editable.clear();
    editable.editor.set_text(&editor.line);
    editable.queue_edit(TextEdit::TextStart(false));
    for _ in 0..editor.caret {
        editable.queue_edit(TextEdit::Right(false));
    }
}

fn sync_prompt_focus(
    open: bool,
    prompt_entity: Option<Entity>,
    focus: &mut InputFocus,
    editables: &mut Query<&mut EditableText, With<ConsoleInputText>>,
    editor: &mut ConsoleEditor,
) {
    let Some(entity) = prompt_entity else {
        return;
    };
    if open {
        focus.set(entity, FocusCause::Navigated);
        if let Ok(mut editable) = editables.get_mut(entity) {
            write_widget_from_editor(&mut editable, editor);
        }
    } else {
        if focus.get() == Some(entity) {
            focus.clear();
        }
        if let Ok(editable) = editables.get_mut(entity) {
            sync_editor_from_widget(editor, &editable);
        }
    }
}

fn world_is_torn(has_world: Option<&HasWorld>, scene: Option<&WorldScene>) -> bool {
    !has_world.is_some_and(|h| h.0) && !scene.is_some_and(|s| s.spawned)
}

fn promote_interactive_interrupt(queue: &mut ConsoleCommandQueue, dispatch: &mut ConsoleDispatch) {
    if !dispatch.paused {
        return;
    }
    let Some(idx) = queue.0.iter().position(|cmd| {
        cmd.interactive && matches!(cmd.name.as_str(), "quit" | "exit" | "disconnect")
    }) else {
        return;
    };
    let Some(cmd) = queue.0.remove(idx) else {
        return;
    };
    let name = cmd.name.clone();
    queue.0.push_front(cmd);
    dispatch.release();
    dispatch.fifo_jumps = dispatch.fifo_jumps.saturating_add(1);
    if matches!(name.as_str(), "quit" | "exit") {
        dispatch.quit_jumps = dispatch.quit_jumps.saturating_add(1);
    }
    diag::info!(
        Console,
        "fifo: interactive jump `{name}` — released FIFO hold (jumps={})",
        dispatch.fifo_jumps
    );
}

fn abort_script_on_wait_timeout(
    kind: &str,
    detail: &str,
    queue: &mut ConsoleCommandQueue,
    console: &mut ConsoleState,
    capacity: usize,
) {
    let before = queue.0.len();
    queue.0.retain(|cmd| cmd.name == "quit");
    let kept = queue.0.len();
    let dropped = before - kept;
    let msg = format!(
        "{kind}: timed out after {WAIT_WORLD_TIMEOUT_SECS:.0}s ({detail}) — \
         script aborted, {dropped} queued commands dropped, kept quit={kept}"
    );
    diag::warn!(Console, "{msg}");
    console.echo(msg, capacity);
}

fn dispatch_console_command(
    time: Res<Time>,
    registry: Res<ConsoleRegistry>,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut queue: ResMut<ConsoleCommandQueue>,
    mut submitted: MessageWriter<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    scene: Option<Res<WorldScene>>,
    screen: Option<Res<AppScreen>>,
    has_world: Option<Res<HasWorld>>,
    ambient_booted: Option<Res<audio::MapAmbientBooted>>,
    presented: Option<Res<PresentedSnapshot>>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut mark_clock: Local<Option<std::time::Instant>>,
    mut mark_sequence: Local<u64>,
) {
    let mark_epoch = *mark_clock.get_or_insert_with(std::time::Instant::now);
    let capacity = settings.log_capacity;

    let waiting = dispatch.paused
        || dispatch.wait_world
        || dispatch.wait_spawn
        || dispatch.wait_spawn_admit
        || dispatch.wait_torn
        || dispatch.wait_ambient
        || dispatch.wait_move.is_some()
        || dispatch.wait_playing
        || dispatch.wait_remaining > 0.0;
    if waiting
        && let Some(index) = queue.0.iter().position(|cmd| {
            cmd.interactive && matches!(cmd.name.as_str(), "dump" | "clip" | "screenshot")
        })
        && let Some(command) = queue.0.remove(index)
    {
        diag::info!(
            Console,
            "fifo: interactive diagnostic `{}` — hold preserved",
            command.name
        );
        submitted.write(command);
        return;
    }
    promote_interactive_interrupt(&mut queue, &mut dispatch);
    if dispatch.paused {
        return;
    }
    if dispatch.wait_world {
        if scene.as_ref().is_some_and(|s| s.spawned) {
            dispatch.wait_world = false;
            diag::info!(
                Console,
                "wait world: spawned after {:.1}s",
                dispatch.wait_world_elapsed
            );
        } else {
            dispatch.wait_world_elapsed += time.delta_secs();
            if dispatch.wait_world_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_world = false;
                abort_script_on_wait_timeout(
                    "wait world",
                    "spawned=false",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if dispatch.wait_spawn_admit {
        dispatch.wait_spawn_elapsed += time.delta_secs();
        if dispatch.wait_spawn_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
            dispatch.wait_spawn_admit = false;
            dispatch.pending_spawn_class = None;
            abort_script_on_wait_timeout(
                "spawn",
                "class select never allowed",
                &mut queue,
                &mut console,
                capacity,
            );
        } else {
            return;
        }
    }
    if dispatch.wait_spawn {
        if screen
            .as_ref()
            .is_some_and(|s| matches!(**s, AppScreen::InGame))
        {
            dispatch.wait_spawn = false;
            diag::info!(
                Console,
                "wait spawn: InGame after {:.1}s",
                dispatch.wait_spawn_elapsed
            );
        } else {
            dispatch.wait_spawn_elapsed += time.delta_secs();
            if dispatch.wait_spawn_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_spawn = false;
                abort_script_on_wait_timeout(
                    "wait spawn",
                    "screen not InGame",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if dispatch.wait_ambient {
        if ambient_booted.as_ref().is_some_and(|b| b.0) {
            dispatch.wait_ambient = false;
            diag::info!(
                Console,
                "wait ambient: MapAmbientBooted after {:.1}s",
                dispatch.wait_ambient_elapsed
            );
        } else {
            dispatch.wait_ambient_elapsed += time.delta_secs();
            if dispatch.wait_ambient_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_ambient = false;
                abort_script_on_wait_timeout(
                    "wait ambient",
                    "MapAmbientBooted=0",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if dispatch.wait_torn {
        if world_is_torn(has_world.as_deref(), scene.as_deref()) {
            dispatch.wait_torn = false;
            diag::info!(
                Console,
                "wait torn: hold after {:.1}s",
                dispatch.wait_torn_elapsed
            );
        } else {
            dispatch.wait_torn_elapsed += time.delta_secs();
            if dispatch.wait_torn_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_torn = false;
                abort_script_on_wait_timeout(
                    "wait torn",
                    "HasWorld or scene.spawned still set",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if let Some(pose) = dispatch.wait_move {
        let matched = presented
            .as_ref()
            .is_some_and(|p| crate::debug_move::presented_matches_move(p, pose));
        if matched {
            dispatch.wait_move = None;
            diag::info!(
                Console,
                "wait move: presented pose after {:.1}s",
                dispatch.wait_move_elapsed
            );
        } else {
            dispatch.wait_move_elapsed += time.delta_secs();
            if dispatch.wait_move_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_move = None;
                abort_script_on_wait_timeout(
                    "wait move",
                    "presented pose still not the move target",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if dispatch.wait_playing {
        if authority
            .as_ref()
            .is_some_and(|w| w.0.phase() == sim::MatchPhase::Playing)
        {
            dispatch.wait_playing = false;
            diag::info!(
                Console,
                "wait playing: Playing after {:.1}s",
                dispatch.wait_playing_elapsed
            );
        } else {
            dispatch.wait_playing_elapsed += time.delta_secs();
            if dispatch.wait_playing_elapsed >= WAIT_WORLD_TIMEOUT_SECS {
                dispatch.wait_playing = false;
                abort_script_on_wait_timeout(
                    "wait playing",
                    "match phase still not Playing",
                    &mut queue,
                    &mut console,
                    capacity,
                );
            } else {
                return;
            }
        }
    }
    if dispatch.wait_remaining > 0.0 {
        dispatch.wait_remaining = (dispatch.wait_remaining - time.delta_secs()).max(0.0);
        return;
    }
    if let Some(command) = queue.0.pop_front() {
        if registry.resolve(&command.name).is_none() {
            let message = format!("unknown command `{}`", command.name);
            diag::warn!(Console, "{message}");
            console.echo(message, capacity);
            return;
        }
        let sync = !command.background
            && matches!(
                command.name.as_str(),
                "map" | "disconnect" | "demo" | "play" | "spawn"
            );
        if command.background
            && matches!(
                command.name.as_str(),
                "map" | "disconnect" | "demo" | "play" | "spawn"
            )
        {
            diag::info!(Console, "{} &: async — FIFO not blocked", command.name);
        }

        if sync {
            match command.name.as_str() {
                "disconnect" => {
                    dispatch.paused = true;
                    dispatch.wait_torn = true;
                    dispatch.wait_torn_elapsed = 0.0;
                    diag::info!(Console, "disconnect: sync until torn hold");
                }
                "spawn" => {}
                _ => {
                    dispatch.paused = true;
                    dispatch.wait_world = true;
                    dispatch.wait_world_elapsed = 0.0;
                    diag::info!(Console, "{}: sync until scene.spawned", command.name);
                }
            }
        }
        if command.name == "mark" {
            if command.args.len() != 1
                || !command.args[0]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                || command.args[0].is_empty()
            {
                console.echo("usage: mark <label> (letters, digits, _, ., -)", capacity);
                return;
            }
            *mark_sequence += 1;
            let ns = mark_epoch.elapsed().as_nanos();
            let label = &command.args[0];

            let rss = assets::process_resident_bytes()
                .map(|bytes| format!(" rss_mib={}", bytes >> 20))
                .unwrap_or_default();
            let heap = diag::process_live_heap_bytes()
                .map(|bytes| format!(" heap_mib={}", bytes >> 20))
                .unwrap_or_default();
            let line = format!(
                "benchmark-mark: pid={} seq={} ns={ns} label={label}{rss}{heap}",
                std::process::id(),
                *mark_sequence
            );
            perf::benchmark_mark(label, *mark_sequence, ns as u64);
            diag::info!(Console, "{line}");
            println!("{line}");
            console.echo(line, capacity);
            return;
        }
        if command.name == "wait" {
            match parse_wait_args(&command.args) {
                WaitKind::World => {
                    if scene.as_ref().is_some_and(|s| s.spawned) {
                        diag::info!(Console, "wait world: already spawned");
                    } else {
                        dispatch.wait_world = true;
                        dispatch.wait_world_elapsed = 0.0;
                        diag::info!(Console, "wait world: until scene.spawned");
                    }
                }
                WaitKind::Spawn => {
                    if screen
                        .as_ref()
                        .is_some_and(|s| matches!(**s, AppScreen::InGame))
                    {
                        diag::info!(Console, "wait spawn: already InGame");
                    } else {
                        dispatch.wait_spawn = true;
                        dispatch.wait_spawn_elapsed = 0.0;
                        diag::info!(Console, "wait spawn: until AppScreen::InGame");
                    }
                }
                WaitKind::Torn => {
                    if world_is_torn(has_world.as_deref(), scene.as_deref()) {
                        diag::info!(Console, "wait torn: already hold");
                    } else {
                        dispatch.wait_torn = true;
                        dispatch.wait_torn_elapsed = 0.0;
                        diag::info!(Console, "wait torn: until HasWorld=0 and scene.spawned=0");
                    }
                }
                WaitKind::Ambient => {
                    if ambient_booted.as_ref().is_some_and(|b| b.0) {
                        diag::info!(Console, "wait ambient: already MapAmbientBooted");
                    } else {
                        dispatch.wait_ambient = true;
                        dispatch.wait_ambient_elapsed = 0.0;
                        diag::info!(Console, "wait ambient: until MapAmbientBooted");
                    }
                }
                WaitKind::Seconds(secs) => {
                    dispatch.wait_remaining = secs;
                    diag::info!(Console, "wait: {secs:.1}s");
                }
            }
            return;
        }
        submitted.write(command);
    }
}

fn console_suggestions(state: &ConsoleState, registry: &ConsoleRegistry) -> Vec<String> {
    registry.suggestions(&state.editor.line, state.editor.caret)
}

#[allow(clippy::type_complexity)]
fn update_console_ui(
    mut commands: Commands,
    state: Res<ConsoleState>,
    registry: Res<ConsoleRegistry>,
    font: Res<ConsoleFont>,
    mut visibilities: ParamSet<(
        Query<&mut Visibility, (With<ConsolePanel>, Without<ConsoleSuggestionText>)>,
        Query<
            (Entity, &mut Visibility, &mut Text),
            (
                With<ConsoleSuggestionText>,
                Without<ConsoleLogText>,
                Without<ConsoleInputText>,
                Without<ConsolePanel>,
            ),
        >,
    )>,
    children: Query<&Children>,
    mut log_texts: Query<
        &mut Text,
        (
            With<ConsoleLogText>,
            Without<ConsoleInputText>,
            Without<ConsoleSuggestionText>,
        ),
    >,
) {
    for mut visibility in &mut visibilities.p0() {
        let want = if state.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
    }
    if !state.open {
        for (entity, mut visibility, mut text) in &mut visibilities.p1() {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            if !text.is_empty() {
                text.clear();
            }
            if children.get(entity).is_ok_and(|c| !c.is_empty()) {
                commands.entity(entity).despawn_related::<Children>();
            }
        }
        return;
    }

    let suggestions = console_suggestions(&state, &registry);
    for mut text in &mut log_texts {
        **text = state.log.join("\n");
    }

    let (token_start, _, token) = state.editor.token_at_caret();
    let indent_cols = PROMPT.chars().count() + token_start;
    let spans = suggestion_spans(&suggestions, state.editor.selected, &token, indent_cols);
    let font = font.0.clone();
    for (entity, mut visibility, mut text) in &mut visibilities.p1() {
        text.clear();
        commands.entity(entity).despawn_related::<Children>();
        if spans.is_empty() {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Visible;
        commands.entity(entity).with_children(|root| {
            for span in &spans {
                root.spawn(suggest_span_bundle(span, font.clone()));
            }
        });
    }
}

fn suggest_span_bundle(span: &SuggestSpan, font: Handle<Font>) -> impl Bundle {
    (
        TextSpan::new(span.text.clone()),
        TextFont {
            font: font.into(),
            font_size: FontSize::Px(FONT_SIZE),
            ..default()
        },
        TextColor(match span.tone {
            SuggestTone::Indent | SuggestTone::Ellipsis => COLOR_REST_MUTED,
            SuggestTone::Matched => COLOR_MATCHED,
            SuggestTone::RestSelected => COLOR_REST_SELECTED,
            SuggestTone::RestMuted => COLOR_REST_MUTED,
        }),
        TextBackgroundColor(COLOR_SUGGEST_BG),
    )
}

fn apply_ingame_menu_intents(
    mut intents: MessageReader<ui::UiIntent>,
    mut transition: ResMut<session::SessionSwapRequest>,
    mut stack: ResMut<ui::RetailMenuStack>,
) {
    for intent in intents.read() {
        if matches!(intent, ui::UiIntent::Disconnect) {
            match transition.request_menu() {
                Ok(_) => {
                    if let Some(index) =
                        stack.names.iter().position(|name| name == "ingame_options")
                    {
                        stack.names.truncate(index);
                    }
                }
                Err(error) => diag::warn!(Ui, "Leave Game: {error}"),
            }
        }
    }
}
