mod host;
mod runtime;
mod script;
mod state;

use runtime::Runner;
use state::OpenMenu;
pub use state::ScriptMenus;

#[derive(bevy::prelude::Resource, Default)]
pub struct FrontendMenus {
    menus: ScriptMenus,
    restore_lobby: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct MenuState<'w> {
    scoped: Option<bevy::prelude::Res<'w, ScriptMenus>>,
    frontend: Option<bevy::prelude::Res<'w, FrontendMenus>>,
}
impl MenuState<'_> {
    pub fn as_ref(&self) -> Option<&ScriptMenus> {
        self.scoped
            .as_deref()
            .or_else(|| self.frontend.as_deref().map(|menus| &menus.menus))
    }
    pub fn as_deref(&self) -> Option<&ScriptMenus> {
        self.as_ref()
    }
    pub fn is_some(&self) -> bool {
        self.as_ref().is_some()
    }
    pub fn is_some_and(&self, predicate: impl FnOnce(&ScriptMenus) -> bool) -> bool {
        self.as_ref().is_some_and(predicate)
    }
    pub fn open_names(&self) -> Vec<String> {
        self.as_ref().map_or_else(Vec::new, ScriptMenus::open_names)
    }
    pub fn focused_item(&self) -> Option<(&str, usize)> {
        self.as_ref().and_then(ScriptMenus::focused_item)
    }
}

use std::collections::HashMap;

use asset_game::{MenuCatalog, MenuDef, SessionTeamSettings};
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use frame::{AppScreen, RuntimeRole, UiExecCommand, UiMenuKey, UiMenuRequest, UiPlaySound};
use net::{ClientActionInbox, ClientActionInput};

use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeGapKind, ChromeItemOverride, ChromeMenuAnim,
    ChromeMenuOverrides, MenuVisOnError, OwnerDrawArgs, OwnerDrawPaint, item_screen_rects,
    push_owner_pic,
};
use crate::draw2d::{Draw2dList, Draw2dOp, tessellate_fonts};
use crate::expr_cache::MenuExprCache;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::playercard::UiLocalVars;
use host::{MenuHost, MenuWorld};
const WINDOW_DECORATION: i32 = 0x0010_0000;
pub(super) const DVAR_FOCUS: i32 = 0x10;
pub(super) const MAX_SCRIPT_DEPTH: u32 = 16;
const MAIN_MENU_DVAR: &str = "g_scriptMainMenu";
const FALLBACK_MAIN_MENU: &str = "class";
const SCOREBOARD_MENU: &str = "scoreboard";

pub(super) fn is_focusable(item: &asset_game::MenuItem) -> bool {
    if item.static_flags & WINDOW_DECORATION != 0 {
        return false;
    }
    let h = &item.handlers;
    item.item_type != 0
        || !h.action.is_empty()
        || !h.accept.is_empty()
        || !h.focus.is_empty()
        || !h.mouse_enter.is_empty()
}

pub(super) fn item_rect(item: &asset_game::MenuItem) -> [f32; 4] {
    [item.rect.x, item.rect.y, item.rect.w, item.rect.h]
}

pub(super) fn choice_index(choices: &[(String, String)], value: &str) -> Option<usize> {
    let number = value.trim().parse::<f32>().ok();
    choices.iter().position(|(_, choice)| {
        choice == value || number.is_some_and(|n| choice.trim().parse::<f32>() == Ok(n))
    })
}

fn item_overrides(def: &MenuDef, open: &OpenMenu) -> Vec<ChromeItemOverride> {
    def.items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let state = open.items.get(index);
            ChromeItemOverride {
                hidden: state.is_some_and(|state| state.hidden),
                fore: if open.focus == Some(index)
                    && (!item.text_key.is_empty() || !item.text_exp.is_empty())
                {
                    def.focus_color
                        .filter(|color| color[3] > 0.0)
                        .or(state.and_then(|state| state.fore))
                } else {
                    state.and_then(|state| state.fore)
                },
                back: state.and_then(|state| state.back),
                ..Default::default()
            }
        })
        .collect()
}

fn custom_class_slot(item: &asset_game::MenuItem) -> Option<usize> {
    fn response(events: &[asset_game::MenuEvent]) -> Option<usize> {
        events.iter().find_map(|event| match event {
            asset_game::MenuEvent::Script(text) => {
                script::parse(text).into_iter().find_map(|cmd| {
                    let script::Command::ScriptMenuResponse(value) = cmd else {
                        return None;
                    };
                    value
                        .strip_prefix("custom")?
                        .parse::<usize>()
                        .ok()?
                        .checked_sub(1)
                        .filter(|&slot| slot < sim::match_state::PERSONAL_CLASS_SLOTS)
                })
            }
            asset_game::MenuEvent::If { then, .. } | asset_game::MenuEvent::Else(then) => {
                response(then)
            }
            _ => None,
        })
    }
    response(&item.handlers.action).or_else(|| response(&item.handlers.accept))
}

fn class_unavailable<'a>(
    menu: &str,
    item: &asset_game::MenuItem,
    dvars: &'a frame::UiMenuDvars,
) -> Option<&'a str> {
    let key = if menu.eq_ignore_ascii_case("class_picker") || menu.eq_ignore_ascii_case("class_samples_picker") {
        let slot = item.name.strip_prefix("choice_")?.parse::<usize>().ok()?;
        format!("ui_class_choice_unavailable_{slot}")
    } else if menu.eq_ignore_ascii_case("iw4l_main") && item.name == "classes"
        || menu.eq_ignore_ascii_case("game_lobby") && item.handlers.action.iter().any(|event| {
            matches!(event, asset_game::MenuEvent::Script(text) if text.contains("ui_class_setup"))
        })
    {
        "ui_class_feature_lock".to_owned()
    } else if menu.to_ascii_lowercase().starts_with("changeclass") {
        let slot = custom_class_slot(item)?;
        format!("ui_class_unavailable_{slot}")
    } else {
        return None;
    };
    dvars.get(&key).filter(|reason| !reason.is_empty())
}

#[derive(Component)]
pub(crate) struct ScriptMenuRaster;

pub(crate) fn spawn_script_menus(root: &mut ChildSpawnerCommands) {
    root.spawn((
        ScriptMenuRaster,
        crate::gpu_list::GpuListLatch::default(),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
    ));
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuInputs<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    overlay: Res<'w, frame::UiOverlayInput>,
    keyboard: MessageReader<'w, 's, bevy::input::keyboard::KeyboardInput>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    hud_input: Option<ResMut<'w, frame::HudInputView>>,
    screen: Res<'w, AppScreen>,
    actions: Option<Res<'w, ClientActionInput>>,
    requests: MessageReader<'w, 's, UiMenuRequest>,
    classes: Option<Res<'w, frame::HostClassLoadouts>>,
    weapons: Option<Res<'w, assets::PreparedWeapons>>,
    compass: Option<Res<'w, assets::SessionCompass>>,
    party: Res<'w, frame::UiPartyState>,
    frontend_strings: Option<Res<'w, asset_game::LocalizeCatalog>>,
    player_data: Option<Res<'w, sim::LocalPlayerData>>,
}

#[derive(Default)]
struct Pressed {
    escape: bool,
    enter: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    home: bool,
    end: bool,
    backspace: bool,
    delete: bool,
    text: Vec<String>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuOutputs<'w> {
    binds: MessageWriter<'w, frame::UiBindRequest>,
    sounds: MessageWriter<'w, UiPlaySound>,
    music: MessageWriter<'w, frame::UiPlayMusic>,
    stop_music: MessageWriter<'w, frame::UiStopMusic>,
    exec: MessageWriter<'w, UiExecCommand>,
    inbox: Option<ResMut<'w, ClientActionInbox>>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScriptMenuRuntime<'w, 's> {
    catalog: Option<Res<'w, MenuCatalog>>,
    strings: Option<Res<'w, PreparedLocalizedStrings>>,
    teams: Option<Res<'w, SessionTeamSettings>>,
    transitions: MessageReader<'w, 's, StateTransitionEvent<frame::MatchScope>>,
    menu_states: (Option<ResMut<'w, ScriptMenus>>, ResMut<'w, FrontendMenus>),
    local_states: (
        Option<ResMut<'w, UiLocalVars>>,
        ResMut<'w, crate::playercard::FrontendLocalVars>,
    ),
    exprs: ResMut<'w, MenuExprCache>,
    dvars: ResMut<'w, frame::UiMenuDvars>,
    binding: ResMut<'w, frame::UiBindingCapture>,
    out: MenuOutputs<'w>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScriptMenuCanvas<'w> {
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    pass: ResMut<'w, HudTessPass>,
}

impl ScriptMenuRuntime<'_, '_> {
    fn update(
        &mut self,
        surface: Res<crate::surface::Hud2dSurface>,
        presentation: net::LocalPresentation,
        role: Option<Res<RuntimeRole>>,
        mut input: MenuInputs,
        canvas: &mut ScriptMenuCanvas,
    ) {
        let requests: Vec<UiMenuRequest> = input.requests.read().cloned().collect();
        let FrontendMenus {
            menus: frontend_menus,
            restore_lobby,
        } = &mut *self.menu_states.1;
        for request in &requests {
            match request {
                UiMenuRequest::RestoreLobby => *restore_lobby = true,
                UiMenuRequest::Close(name) if name == "game_lobby" => *restore_lobby = false,
                _ => {}
            }
        }
        let menus = self.menu_states.0.as_deref_mut().unwrap_or(frontend_menus);
        let locals = self
            .local_states
            .0
            .as_deref_mut()
            .unwrap_or(&mut self.local_states.1.0);
        canvas.hide();
        if self
            .transitions
            .read()
            .any(|event| event.exited == Some(frame::MatchScope::Live))
        {
            self.binding.command = None;
            *menus = ScriptMenus::default();
        }
        let Some(catalog) = self.catalog.as_deref() else {
            return;
        };
        let snapshot = presentation.snapshot();
        let meta = snapshot.and_then(|s| s.meta.for_client(presentation.client_id()));
        let in_game = matches!(*input.screen, AppScreen::InGame | AppScreen::ClassSelect);
        let frontend = *input.screen == AppScreen::MainMenu;
        if (!in_game && !frontend) || (in_game && meta.is_none()) {
            if menus.screen.is_some() {
                self.out.stop_music.write(frame::UiStopMusic);
                self.binding.command = None;
                *menus = ScriptMenus::default();
            }
            if let Some(view) = input.hud_input.as_mut()
                && view.script_menu_open
            {
                view.script_menu_open = false;
            }
            return;
        }
        menus.reconcile(catalog);
        let screen_changed = menus.screen.map(|s| s == AppScreen::MainMenu) != Some(frontend);
        if screen_changed {
            self.binding.command = None;
            *menus = ScriptMenus {
                screen: Some(*input.screen),
                ..default()
            };
            self.out.stop_music.write(frame::UiStopMusic);
        }
        let localize = if in_game {
            self.strings.as_ref().map(|s| &s.0)
        } else {
            input.frontend_strings.as_deref()
        };
        let world = MenuWorld {
            player_data: input.player_data.as_deref(),
            ms: crate::scorebar::milliseconds() as i32,
            in_game,
            party: &input.party,
            dvars: snapshot
                .filter(|_| in_game)
                .map(|s| s.meta.script_dvars(presentation.client_id())),
            sv_running: in_game && role.is_some_and(|r| r.runs_authority()),
            catalog: Some(catalog),
            localize,
            kind: snapshot.map(|s| s.meta.kind),
            team: meta.map_or(0, |m| m.client_state_team),
            team_scores: snapshot.map_or([0; 3], |s| s.meta.objectives.scores),
            player_score: meta.map_or(0, |m| m.score),
            time_left_s: snapshot.map_or(0, |s| {
                let now_ms = s.tick.0.saturating_mul(sim::MATCH_TICK_MS) as i32;
                s.meta.objectives.time_left_ms(now_ms).div_euclid(1000)
            }),
            teams: self.teams.as_ref().map(|t| &t.0),
            scores_open: in_game
                && input
                    .actions
                    .as_ref()
                    .is_some_and(|a| a.client.kb.scores.active),
            classes: input.classes.as_deref(),
            weapons: input.weapons.as_ref().map(|w| w.registry().as_ref()),
            emp_jammed: in_game
                && presentation.player().is_some_and(|ps| {
                    ps.other_flags & playerstate_iw4::other_flags::EMP_JAMMED != 0
                }),
            radar_blocked: in_game
                && presentation
                    .snapshot()
                    .and_then(|snap| snap.meta.for_client(presentation.client_id()))
                    .is_some_and(|meta| meta.radar_blocked),
        };

        let mut runner = Runner {
            catalog,
            world: &world,
            menus,
            locals,
            exprs: &mut self.exprs,
            dvars: &mut self.dvars,
            depth: 0,
            binding: &mut self.binding,
        };

        let main_menu = "iw4l_main";
        if frontend && screen_changed {
            runner.open(main_menu);
            if let Some(def) = catalog.get("main")
                && !def.sound_name.is_empty()
            {
                self.out.music.write(frame::UiPlayMusic {
                    alias: def.sound_name.clone(),
                });
            }
        }

        if frontend && *restore_lobby {
            *restore_lobby = false;
            if input.party.in_lobby && input.party.is_host {
                runner.open("game_lobby");
                diag::info!(Ui, "frontend: private lobby reopened");
            }
        }

        for command in meta
            .into_iter()
            .filter(|_| in_game)
            .flat_map(|m| &m.menu_commands)
        {
            if command.serial <= runner.menus.applied_serial {
                continue;
            }
            runner.menus.applied_serial = command.serial;
            match &command.kind {
                sim::MenuCommandKind::Open(name) => runner.open(name),
                sim::MenuCommandKind::ClosePopup => {
                    if let Some(top) = runner.menus.input_menu().map(|m| m.name.clone()) {
                        runner.close(&top);
                    }
                }
                sim::MenuCommandKind::CloseInGame => runner.close_all(),
            }
        }

        let console_open = input.hud_input.as_ref().is_some_and(|h| h.console_open);
        let mut pressed = Pressed::default();
        for request in requests {
            match request {
                UiMenuRequest::Toggle => pressed.escape = true,
                UiMenuRequest::RestoreLobby => {}
                UiMenuRequest::Open(name) => runner.open(&name),
                UiMenuRequest::Close(name) => runner.close(&name),
                UiMenuRequest::Focus { menu, item } => {
                    if let Some(index) = runner.catalog.get(&menu).and_then(|definition| {
                        definition
                            .items
                            .iter()
                            .position(|candidate| candidate.name == item)
                    }) {
                        runner.set_focus(&menu, index);
                    }
                }
                UiMenuRequest::Key(UiMenuKey::Escape) => pressed.escape = true,
                UiMenuRequest::Key(UiMenuKey::Enter) => pressed.enter = true,
                UiMenuRequest::Key(UiMenuKey::Up) => pressed.up = true,
                UiMenuRequest::Key(UiMenuKey::Down) => pressed.down = true,
                UiMenuRequest::Key(UiMenuKey::Left) => pressed.left = true,
                UiMenuRequest::Key(UiMenuKey::Right) => pressed.right = true,
                UiMenuRequest::Key(UiMenuKey::Home) => pressed.home = true,
                UiMenuRequest::Key(UiMenuKey::End) => pressed.end = true,
                UiMenuRequest::Key(UiMenuKey::Backspace) => pressed.backspace = true,
                UiMenuRequest::Key(UiMenuKey::Delete) => pressed.delete = true,
                UiMenuRequest::Text(text) => pressed.text.push(text),
            }
        }
        // Reopen after a teardown resets the stack; only acknowledgement clears
        // the notice. Opening last also keeps keyboard/mouse focus on the dialog.
        if frontend && runner.dvars.get("ui_connection_error") == Some("1") {
            runner.open("iw4l_connection_error");
        }
        if frontend && runner.dvars.get("ui_map_error") == Some("1") {
            runner.open("iw4l_map_error");
        }
        let pointer = !console_open && !input.overlay.0;
        if pointer {
            let keys = &input.keys;
            pressed.escape |= keys.just_pressed(KeyCode::Escape);
            pressed.enter |=
                keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
            pressed.up |= keys.just_pressed(KeyCode::ArrowUp);
            pressed.down |= keys.just_pressed(KeyCode::ArrowDown);
            pressed.left |= keys.just_pressed(KeyCode::ArrowLeft);
            pressed.right |= keys.just_pressed(KeyCode::ArrowRight);
            pressed.home |= keys.just_pressed(KeyCode::Home);
            pressed.end |= keys.just_pressed(KeyCode::End);
            pressed.backspace |= keys.just_pressed(KeyCode::Backspace);
            pressed.delete |= keys.just_pressed(KeyCode::Delete);
        }
        for event in input.keyboard.read() {
            if pointer
                && event.state.is_pressed()
                && let Some(text) = &event.text
            {
                pressed.text.push(text.to_string());
            }
        }
        if runner.menus.binding_menu.as_ref().is_some_and(|name| {
            runner
                .menus
                .input_menu()
                .is_none_or(|top| &top.name != name)
        }) {
            runner.binding.command = None;
            runner.menus.binding_menu = None;
        }
        handle_input(&mut runner, &input, &surface, &pressed, pointer);

        flush_outputs(
            runner.menus,
            &mut self.out,
            runner.dvars,
            presentation.client_id(),
        );
        if let Some(view) = input.hud_input.as_mut() {
            let open = runner.menus.captures_input();
            if view.script_menu_open != open {
                view.script_menu_open = open;
            }
        }

        if input.overlay.0 && in_game {
            return;
        }

        let scoreboard = snapshot
            .filter(|s| {
                in_game
                    && crate::scoreboard::displayed(world.scores_open, s, presentation.client_id())
            })
            .and_then(|_| catalog.get(SCOREBOARD_MENU));
        canvas.paint(&surface, &mut runner, input.compass.as_deref(), scoreboard);
    }
}

impl ScriptMenuCanvas<'_> {
    fn hide(&mut self) {
        self.pass.script_menus = TessJob::Hide;
    }

    fn paint(
        &mut self,
        surface: &crate::surface::Hud2dSurface,
        runner: &mut Runner<'_, '_>,
        compass: Option<&assets::SessionCompass>,
        scoreboard: Option<&MenuDef>,
    ) {
        let catalog = runner.catalog;
        let world = runner.world;
        let menus = &mut *runner.menus;
        let locals = &*runner.locals;
        let exprs = &mut *runner.exprs;
        let dvars = &*runner.dvars;
        let binding = &*runner.binding;
        let localize = world.localize;
        if (menus.stack.is_empty() && scoreboard.is_none()) || !surface.is_ready() {
            return;
        }
        let mut list = Draw2dList::default();
        let mut reported = std::mem::take(&mut menus.reported);
        let mut layers = Vec::new();
        if let Some(def) = scoreboard {
            layers.push((
                SCOREBOARD_MENU.to_owned(),
                def,
                Vec::new(),
                Vec::new(),
                None,
            ));
        }
        let paint_start = menus
            .stack
            .iter()
            .rposition(|open| {
                catalog
                    .get(&open.name)
                    .is_some_and(|def| def.fullscreen != 0)
            })
            .unwrap_or(0);
        for open in &menus.stack[paint_start..] {
            let Some(def) = catalog.get(&open.name) else {
                continue;
            };
            let mut overrides = item_overrides(def, open);
            let mut slider_parts = Vec::new();
            for (index, (item, state)) in def.items.iter().zip(&mut overrides).enumerate() {
                if let Some(reason) = class_unavailable(&open.name, item, dvars) {
                    if let Some(slot) = custom_class_slot(item) {
                        let name = world
                            .classes
                            .and_then(|classes| classes.slots.get(slot))
                            .map_or("Class", |class| class.name.as_str());
                        state.text = Some(format!("{name} — {reason}"));
                        state.text_literal = Some(true);
                    }
                    state.fore = Some([0.5, 0.5, 0.5, 1.0]);
                }
                if item.item_type == 14 && !item.dvar.is_empty() {
                    state.text = Some(
                        dvars
                            .get(&format!("ui_bind_{}", item.dvar))
                            .unwrap_or("UNBOUND")
                            .to_owned(),
                    );
                    if binding.command.as_deref() == Some(item.dvar.as_str()) {
                        state.text = Some("... (ESC)".to_owned());
                    }
                    state.text_literal = Some(true);

                    state.text_align = Some((10, -8.0, 0.0));
                }
                if item.item_type == 4 && !item.dvar.is_empty() {
                    state.text = Some(dvars.get(&item.dvar).unwrap_or("").to_owned());
                    if item.edit_field.as_ref().is_some_and(|field| field.masked) {
                        state.text =
                            Some("*".repeat(state.text.as_deref().unwrap_or("").chars().count()));
                    }
                    state.text_literal = Some(true);
                }
                if let Some(slider) = &item.slider {
                    let value = dvars
                        .get(&item.dvar)
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(slider.min);
                    let fraction =
                        ((value - slider.min) / (slider.max - slider.min)).clamp(0.0, 1.0);
                    for (x, y, w, h, color) in [
                        (0.0, 9.0, item.rect.w, 2.0, [0.6, 0.6, 0.6, 1.0]),
                        (
                            fraction * (item.rect.w - 6.0),
                            3.0,
                            6.0,
                            14.0,
                            [1.0, 1.0, 1.0, 1.0],
                        ),
                    ] {
                        let mut rect = item.rect;
                        rect.x += x;
                        rect.y += y;
                        rect.w = w;
                        rect.h = h;
                        slider_parts.push((
                            index,
                            ChromeItemOverride {
                                hidden: state.hidden,
                                fore: state.fore,
                                back: Some(color),
                                text: Some(String::new()),
                                rect: Some(rect),
                                style: Some(1),
                                background: Some("white"),
                                ..Default::default()
                            },
                        ));
                    }
                    let displayed = slider
                        .display_range
                        .map_or(value, |[min, max]| min + fraction * (max - min));
                    let decimals = usize::from(slider.decimals.min(6));
                    let mut rect = item.rect;
                    rect.x += item.rect.w + 8.0;
                    rect.w = 36.0;
                    slider_parts.push((
                        index,
                        ChromeItemOverride {
                            hidden: state.hidden,
                            fore: state.fore,
                            back: Some([0.0; 4]),
                            text: Some(format!("{displayed:.decimals$}{}", slider.suffix)),
                            text_literal: Some(true),
                            rect: Some(rect),
                            background: Some(""),
                            ..Default::default()
                        },
                    ));
                    state.background = Some("");
                    state.text = Some(String::new());
                }
                let choices = dvars.choices(&item.dvar).unwrap_or(&item.choices);
                if !choices.is_empty() {
                    let value = dvars.get(&item.dvar).unwrap_or("");
                    state.text = Some(
                        choice_index(choices, value)
                            .map(|at| choices[at].0.clone())
                            .unwrap_or_else(|| value.to_owned()),
                    );
                }
            }
            if let Some(edit) = &menus.editing
                && edit.menu == open.name
                && let Some(item) = def.items.get(edit.item)
                && let Some(state) = overrides.get_mut(edit.item)
            {
                let mut text = if item.edit_field.as_ref().is_some_and(|field| field.masked) {
                    vec!['*'; edit.buffer.len()]
                } else {
                    edit.buffer.clone()
                };
                text.insert(edit.cursor.min(text.len()), '|');
                state.text = Some(text.into_iter().collect());
            }
            let focus_rect = open.focus.and_then(|i| def.items.get(i)).map(item_rect);
            layers.push((open.name.clone(), def, overrides, slider_parts, focus_rect));
        }
        for (name, def, overrides, extra_items, focus_rect) in &layers {
            let host = MenuHost {
                world,
                dvars,
                menu: def,
                locals,
                open: &menus.stack,
                focus_rect: *focus_rect,
            };
            let mut owner_draw = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
                if args.item.owner_draw == 181
                    && let Some(map_namespace) = self.hud_images.map_namespace()
                    && let Some(image) = compass.and_then(|c| c.declaration.image.as_ref())
                {
                    push_owner_pic(
                        &args,
                        image.clone(),
                        map_namespace,
                        args.color,
                        Draw2dOp::StretchPic,
                        frame,
                    );
                    return OwnerDrawPaint::Painted;
                }
                OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw)
            };
            let frame = crate::chrome::ChromeMenuPainter::new(
                def,
                &host,
                surface,
                ChromeAssets {
                    catalog: Some(catalog),
                    localize,
                },
                ChromeMenuAnim::IDENTITY,
                exprs,
            )
            .paint(
                MenuVisOnError::PaintAnyway,
                Some(&mut owner_draw),
                ChromeMenuOverrides {
                    items: overrides,
                    extra_items,
                    skip: None,
                },
            );
            if frame.coverage.typed_gap > 0 && !reported.contains(name) {
                reported.insert(name.clone());
                let gaps: Vec<String> = frame
                    .coverage
                    .gap_ids
                    .iter()
                    .map(|(index, kind)| {
                        let source = if *index < def.items.len() {
                            *index
                        } else {
                            extra_items[*index - def.items.len()].0
                        };
                        let item = &def.items[source];
                        let error = frame
                            .vis_errors
                            .iter()
                            .find(|(i, _)| i == index)
                            .map_or("", |(_, e)| e.as_str());
                        let text = if item.text_exp.is_empty() {
                            item.text_key.clone()
                        } else {
                            format!("{:?}", exprs.evaluate_string(&item.text_exp, &host))
                        };
                        format!("{index}:{}:{kind:?}{error}[{text}]", item.name)
                    })
                    .collect();
                diag::info!(
                    Ui,
                    "menu {}: {} of {} items not painted: {}",
                    name,
                    gaps.len(),
                    def.items.len() + extra_items.len(),
                    gaps.join(" ")
                );
            }
            list.cmds.extend(frame.list.cmds);
        }
        menus.reported = reported;
        let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
        for cmd in &list.cmds {
            let _ =
                self.hud_images
                    .get_native(cmd.material_namespace, &cmd.material, &mut self.images);
            if let Draw2dOp::TextRun { font, .. } = &cmd.op
                && !fonts.contains_key(font)
                && let Some(def) = catalog.font(font)
            {
                fonts.insert(font.clone(), def);
            }
        }
        let (quads, _) = tessellate_fonts(&list, &fonts);
        if !quads.is_empty() {
            self.pass.script_menus = TessJob::Quads(quads);
        }
    }
}

pub(crate) fn update_script_menus(
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    role: Option<Res<RuntimeRole>>,
    input: MenuInputs,
    mut runtime: ScriptMenuRuntime,
    mut canvas: ScriptMenuCanvas,
) {
    runtime.update(surface, presentation, role, input, &mut canvas);
}

fn handle_input(
    runner: &mut Runner<'_, '_>,
    input: &MenuInputs,
    surface: &crate::surface::Hud2dSurface,
    pressed: &Pressed,
    pointer: bool,
) {
    if runner.binding.command.is_some() || runner.binding.consumed_input {
        if pressed.escape {
            runner.binding.command = None;
            runner.binding.consumed_input = true;
        }
        return;
    }
    if let Some(mut edit) = runner.menus.editing.take() {
        if pressed.escape {
            return;
        }
        if pressed.enter {
            if let Some(item) = runner
                .catalog
                .get(&edit.menu)
                .and_then(|def| def.items.get(edit.item))
            {
                runner
                    .dvars
                    .set(&item.dvar, edit.buffer.iter().collect::<String>());
                let value: String = edit.buffer.iter().collect();
                let value = value.replace('\\', "\\\\").replace('"', "\\\"");
                if !item.edit_field.as_ref().is_some_and(|field| field.masked) {
                    runner
                        .menus
                        .exec
                        .push(format!("set {} \"{}\"", item.dvar, value));
                }
                runner.run_events(&edit.menu, Some(edit.item), &item.handlers.accept);
            }
            return;
        }
        if pressed.home {
            edit.cursor = 0;
        }
        if pressed.end {
            edit.cursor = edit.buffer.len();
        }
        if pressed.left {
            edit.cursor = edit.cursor.saturating_sub(1);
        }
        if pressed.right {
            edit.cursor = (edit.cursor + 1).min(edit.buffer.len());
        }
        if pressed.backspace && edit.cursor > 0 {
            edit.cursor -= 1;
            edit.buffer.remove(edit.cursor);
        }
        if pressed.delete && edit.cursor < edit.buffer.len() {
            edit.buffer.remove(edit.cursor);
        }
        for text in &pressed.text {
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                if edit.buffer.len() >= edit.max_chars {
                    break;
                }
                edit.buffer.insert(edit.cursor, ch);
                edit.cursor += 1;
            }
        }
        runner.menus.editing = Some(edit);
        return;
    }
    if pressed.escape {
        match runner.menus.input_menu().map(|m| m.name.clone()) {
            Some(top) => runner.escape(&top),
            None if !runner.world.in_game => runner.open("iw4l_main"),
            None => {
                let main = runner
                    .world
                    .dvars
                    .as_ref()
                    .and_then(|d| d.string(MAIN_MENU_DVAR))
                    .filter(|name| !name.is_empty() && runner.catalog.get(name).is_some())
                    .map(str::to_owned);
                let name = main.unwrap_or_else(|| {
                    diag::warn!(
                        Ui,
                        "togglemenu: {MAIN_MENU_DVAR} names no loaded menu, opening {FALLBACK_MAIN_MENU}"
                    );
                    FALLBACK_MAIN_MENU.to_owned()
                });
                runner.open(&name);
            }
        }
        return;
    }
    let Some(top) = runner.menus.input_menu().map(|m| m.name.clone()) else {
        return;
    };

    if pressed.down {
        runner.focus_nav(&top, 0, 1);
    }
    if pressed.up {
        runner.focus_nav(&top, 0, -1);
    }
    if pressed.left || pressed.right {
        let step = if pressed.left { -1 } else { 1 };
        let focus = runner.menus.input_menu().and_then(|m| m.focus);
        if !focus.is_some_and(|focus| runner.adjust(&top, focus, step)) {
            runner.focus_nav(&top, step, 0);
        }
        return;
    }
    if pressed.enter {
        if let Some(focus) = runner.menus.input_menu().and_then(|m| m.focus) {
            runner.activate(&top, focus, true);
        }
        return;
    }
    if !pointer {
        return;
    }

    let cursor = input
        .windows
        .single()
        .ok()
        .and_then(|w| w.cursor_position());
    let Some(cursor) = cursor else {
        return;
    };
    let moved = runner.menus.cursor.is_some_and(|old| old != cursor);
    runner.menus.cursor = Some(cursor);
    if !input.mouse.pressed(MouseButton::Left) {
        runner.menus.slider_drag = None;
    }
    if !moved && !input.mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(def) = runner.catalog.get(&top) else {
        return;
    };
    let host = MenuHost {
        world: runner.world,
        dvars: runner.dvars,
        menu: def,
        locals: runner.locals,
        open: &runner.menus.stack,
        focus_rect: None,
    };
    let rects = item_screen_rects(def, &host, surface, runner.exprs);
    let visible = runner.visible_items(&top);
    if let Some((menu, index)) = runner.menus.slider_drag.clone() {
        if menu == top && input.mouse.pressed(MouseButton::Left) {
            if let Some((_, [x0, _, x1, _])) = rects.iter().find(|(i, _)| *i == index) {
                runner.slide(&top, index, (cursor.x - x0) / (x1 - x0));
            }
            return;
        }
        runner.menus.slider_drag = None;
    }
    let hit = rects.iter().rev().find_map(|&(index, [x0, y0, x1, y1])| {
        let inside = cursor.x >= x0 && cursor.x < x1 && cursor.y >= y0 && cursor.y < y1;
        (inside && is_focusable(&def.items[index]) && visible.contains(&index)).then_some(index)
    });
    runner.set_hover(&top, hit);
    if input.mouse.just_pressed(MouseButton::Left)
        && let Some(index) = hit
    {
        if let Some((_, [x0, _, x1, _])) = rects.iter().find(|(i, _)| *i == index)
            && runner.slide(&top, index, (cursor.x - x0) / (x1 - x0))
        {
            runner.set_focus(&top, index);
            runner.menus.slider_drag = Some((top.clone(), index));
        } else {
            runner.activate(&top, index, false);
        }
    }
}

fn flush_outputs(
    menus: &mut ScriptMenus,
    out: &mut MenuOutputs,
    dvars: &frame::UiMenuDvars,
    local: sim::ClientId,
) {
    for command in menus.bind_requests.drain(..) {
        out.binds.write(frame::UiBindRequest { command });
    }
    for alias in menus.sounds.drain(..) {
        out.sounds.write(UiPlaySound { alias });
    }
    for text in menus.exec.drain(..) {
        diag::info!(Ui, "menu exec: {text}");
        out.exec.write(UiExecCommand { text });
    }
    let private_host =
        dvars.get("ui_lobby_host") == Some("1") && dvars.get("ui_lobby_public") != Some("1");
    menus.responses.retain(|(menu, response)| {
        let end_game = menu.eq_ignore_ascii_case("popup_endgame")
            || menu.eq_ignore_ascii_case("popup_endgame_ranked");
        if !end_game || !response.eq_ignore_ascii_case("endround") || private_host {
            return true;
        }
        diag::info!(Ui, "menu exec: disconnect");
        out.exec.write(UiExecCommand {
            text: "disconnect".into(),
        });
        false
    });
    let Some(inbox) = out.inbox.as_mut() else {
        menus.responses.clear();
        return;
    };
    for (menu, response) in menus.responses.drain(..) {
        if menu.eq_ignore_ascii_case("changeclass")
            && let Some(slot) = response
                .strip_prefix("custom")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1))
                .filter(|slot| *slot < sim::match_state::PERSONAL_CLASS_SLOTS)
        {
            out.exec.write(UiExecCommand {
                text: format!("spawn {slot} &"),
            });
            continue;
        }
        let (Some(menu_field), Some(response_field)) = (
            sim::menu_response_field(&menu),
            sim::menu_response_field(&response),
        ) else {
            diag::warn!(
                Ui,
                "menu response `{menu}` `{response}` does not fit the wire"
            );
            continue;
        };
        match inbox.push(
            local,
            sim::ClientAction::MenuResponse {
                menu: menu_field,
                response: response_field,
            },
        ) {
            Ok(_) => diag::info!(Ui, "menu response: {menu} {response}"),
            Err(error) => diag::warn!(
                Ui,
                "menu response `{menu}` `{response}` not queued: {error}"
            ),
        }
    }
}
