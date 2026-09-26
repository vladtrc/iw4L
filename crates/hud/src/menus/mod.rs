mod host;
mod script;

use std::collections::HashMap;

use assets::{MenuCatalog, MenuDef, MenuEvent, PreparedLocalizedStrings, SessionTeamSettings};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use frame::{
    AppScreen, MatchTornDown, RuntimeRole, UiExecCommand, UiMenuKey, UiMenuRequest, UiPlaySound,
};
use net::{
    ActionRequestIds, ClientActionInbox, ClientActionInput, LocalPresentClient, PresentedSnapshot,
};

use crate::chrome::{ChromeAssets, execute_chrome_menu, item_screen_rects};
use crate::draw2d::{Draw2dList, Draw2dOp, tessellate_fonts};
use crate::expr_cache::MenuExprCache;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::playercard::UiLocalVars;
use host::{MenuHost, MenuWorld};
use script::Command;

const WINDOW_DECORATION: i32 = 0x0010_0000;
const MAX_SCRIPT_DEPTH: u32 = 16;
const MAIN_MENU_DVAR: &str = "g_scriptMainMenu";
const FALLBACK_MAIN_MENU: &str = "class";
const SCOREBOARD_MENU: &str = "scoreboard";

#[derive(Clone, Debug, Default)]
struct ItemState {
    hidden: bool,
    fore: Option<[f32; 4]>,
    back: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
struct OpenMenu {
    name: String,
    focus: Option<usize>,
    hover: Option<usize>,
    items: Vec<ItemState>,
}

#[derive(Resource, Default)]
pub struct ScriptMenus {
    stack: Vec<OpenMenu>,
    applied_serial: u32,
    responses: Vec<(String, String)>,
    exec: Vec<String>,
    sounds: Vec<String>,
    reported: std::collections::HashSet<String>,
}

impl ScriptMenus {
    pub fn captures_input(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn open_names(&self) -> Vec<String> {
        self.stack.iter().map(|m| m.name.clone()).collect()
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.stack
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case(name))
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut OpenMenu> {
        let pos = self.position(name)?;
        self.stack.get_mut(pos)
    }
}

fn is_focusable(item: &assets::MenuItem) -> bool {
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

fn item_rect(item: &assets::MenuItem) -> [f32; 4] {
    [item.rect.x, item.rect.y, item.rect.w, item.rect.h]
}

struct Runner<'a, 'w> {
    catalog: &'a MenuCatalog,
    world: &'a MenuWorld<'w>,
    menus: &'a mut ScriptMenus,
    locals: &'a mut UiLocalVars,
    exprs: &'a mut MenuExprCache,
    depth: u32,
}

impl Runner<'_, '_> {
    fn def(&self, name: &str) -> Option<&MenuDef> {
        self.catalog.get(name)
    }

    fn eval(&mut self, menu: &str, dump: &str) -> Option<hud_iw4::Operand> {
        let def = self.catalog.get(menu)?;
        let open = self.menus.open_names();
        let focus_rect = self
            .menus
            .stack
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(menu))
            .and_then(|m| m.focus)
            .and_then(|i| def.items.get(i))
            .map(item_rect);
        let host = MenuHost {
            world: self.world,
            menu: def,
            locals: self.locals,
            open: &open,
            focus_rect,
        };
        match self.exprs.evaluate(dump, &host) {
            Ok(value) => Some(value),
            Err(err) => {
                diag::debug!(Ui, "menu {menu}: expression failed {err:?}");
                None
            }
        }
    }

    fn run_events(&mut self, menu: &str, item: Option<usize>, events: &[MenuEvent]) {
        if self.depth >= MAX_SCRIPT_DEPTH {
            diag::warn!(Ui, "menu {menu}: script nesting too deep");
            return;
        }
        self.depth += 1;
        let mut last_if: Option<bool> = None;
        for event in events {
            match event {
                MenuEvent::Script(text) => {
                    last_if = None;
                    for command in script::parse(text) {
                        self.command(menu, item, command);
                    }
                }
                MenuEvent::If { condition, then } => {
                    let pass = self
                        .eval(menu, condition)
                        .is_some_and(|v| hud_iw4::expr::source_int(&v) != 0);
                    last_if = Some(pass);
                    if pass {
                        self.run_events(menu, item, then);
                    }
                }
                MenuEvent::Else(body) => {
                    if last_if == Some(false) {
                        self.run_events(menu, item, body);
                    }
                    last_if = None;
                }
                MenuEvent::SetLocalVar { kind, name, expr } => {
                    last_if = None;
                    let Some(value) = self.eval(menu, expr) else {
                        continue;
                    };
                    match kind {
                        3 | 4 => self.locals.set_int(name, hud_iw4::expr::source_int(&value)),
                        5 => self
                            .locals
                            .set_float(name, hud_iw4::expr::source_float(&value)),
                        _ => self
                            .locals
                            .set_string(name, hud_iw4::expr::source_str(&value)),
                    }
                }
            }
        }
        self.depth -= 1;
    }

    fn command(&mut self, menu: &str, item: Option<usize>, command: Command) {
        let target = |name: &str| {
            if name.eq_ignore_ascii_case("self") {
                menu.to_owned()
            } else {
                name.to_owned()
            }
        };
        match command {
            Command::Open(name) => self.open(&name),
            Command::Close(name) => self.close(&target(&name)),
            Command::Escape(name) => self.escape(&target(&name)),
            Command::SetFocus(name) => {
                let Some(def) = self.def(menu) else {
                    return;
                };
                if let Some(index) = def
                    .items
                    .iter()
                    .position(|i| i.name.eq_ignore_ascii_case(&name) && is_focusable(i))
                {
                    self.set_focus(menu, index);
                }
            }
            Command::FocusFirst => self.focus_step(menu, 1, true),
            Command::SetItemColor {
                item: name,
                field,
                color,
            } => {
                self.each_item(menu, item, &name, |state| match field.as_str() {
                    "backcolor" => state.back = Some(color),
                    "forecolor" => state.fore = Some(color),
                    _ => {}
                });
            }
            Command::Show(name) => self.each_item(menu, item, &name, |state| state.hidden = false),
            Command::Hide(name) => self.each_item(menu, item, &name, |state| state.hidden = true),
            Command::Play(alias) => self.menus.sounds.push(alias),
            Command::ScriptMenuResponse(response) => {
                self.menus.responses.push((menu.to_owned(), response));
            }
            Command::Exec(text) => self.menus.exec.push(text),
            Command::SetDvar(name, value) => {
                self.menus.exec.push(format!("set {name} \"{value}\""));
            }
            Command::ExecOnDvarIntValue {
                dvar,
                value,
                command,
            } => {
                let current = self
                    .world
                    .dvars
                    .as_ref()
                    .and_then(|d| d.int(&dvar))
                    .unwrap_or(0);
                if current == value {
                    self.menus.exec.push(command);
                }
            }
            Command::ExecOnDvarStringValue {
                dvar,
                value,
                command,
            } => {
                let current = self.world.dvars.as_ref().and_then(|d| d.string(&dvar));
                if current.is_some_and(|c| c.eq_ignore_ascii_case(&value)) {
                    self.menus.exec.push(command);
                }
            }
            Command::Unhandled(name) => {
                diag::debug!(Ui, "menu {menu}: script command `{name}` not run");
            }
        }
    }

    fn each_item(
        &mut self,
        menu: &str,
        current: Option<usize>,
        name: &str,
        mut apply: impl FnMut(&mut ItemState),
    ) {
        let Some(def) = self.catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        if name.eq_ignore_ascii_case("self") {
            if let Some(state) = current.and_then(|i| open.items.get_mut(i)) {
                apply(state);
            }
            return;
        }
        for (index, item) in def.items.iter().enumerate() {
            if item.name.eq_ignore_ascii_case(name)
                && let Some(state) = open.items.get_mut(index)
            {
                apply(state);
            }
        }
    }

    fn open(&mut self, name: &str) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(name) else {
            diag::warn!(Ui, "menu open: `{name}` is not loaded");
            return;
        };
        if let Some(pos) = self.menus.position(name) {
            let open = self.menus.stack.remove(pos);
            self.menus.stack.push(open);
            return;
        }
        self.menus.stack.push(OpenMenu {
            name: def.name.clone(),
            focus: None,
            hover: None,
            items: vec![ItemState::default(); def.items.len()],
        });
        diag::info!(Ui, "menu open: {}", def.name);
        let name = def.name.clone();
        self.run_events(&name, None, &def.handlers.open);
    }

    fn close(&mut self, name: &str) {
        let catalog = self.catalog;
        if self.menus.position(name).is_none() {
            return;
        }
        if let Some(def) = catalog.get(name) {
            self.run_events(name, None, &def.handlers.close);
        }
        if let Some(pos) = self.menus.position(name) {
            self.menus.stack.remove(pos);
            diag::info!(Ui, "menu close: {name}");
        }
    }

    fn close_all(&mut self) {
        while let Some(top) = self.menus.stack.last().map(|m| m.name.clone()) {
            self.close(&top);
        }
    }

    fn escape(&mut self, name: &str) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(name) else {
            return;
        };
        if def.handlers.esc.is_empty() {
            self.close(name);
        } else {
            self.run_events(name, None, &def.handlers.esc);
        }
    }

    fn set_focus(&mut self, menu: &str, index: usize) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        let old = open.focus;
        if old == Some(index) {
            return;
        }
        open.focus = Some(index);
        if let Some(old) = old.and_then(|i| def.items.get(i).map(|item| (i, item))) {
            self.run_events(menu, Some(old.0), &old.1.handlers.leave_focus);
        }
        if let Some(item) = def.items.get(index) {
            if !item.focus_sound.is_empty() {
                self.menus.sounds.push(item.focus_sound.clone());
            }
            self.run_events(menu, Some(index), &item.handlers.focus);
        }
    }

    fn set_hover(&mut self, menu: &str, index: Option<usize>) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        let old = open.hover;
        if old == index {
            return;
        }
        open.hover = index;
        if let Some(old) = old.and_then(|i| def.items.get(i).map(|item| (i, item))) {
            self.run_events(menu, Some(old.0), &old.1.handlers.mouse_exit);
        }
        if let Some(index) = index {
            if let Some(item) = def.items.get(index) {
                self.run_events(menu, Some(index), &item.handlers.mouse_enter);
            }
            self.set_focus(menu, index);
        }
    }

    fn focus_step(&mut self, menu: &str, step: i32, first: bool) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let visible = self.visible_items(menu);
        let candidates: Vec<usize> = (0..def.items.len())
            .filter(|&i| is_focusable(&def.items[i]) && visible.contains(&i))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let current = self.menus.get_mut(menu).and_then(|m| m.focus);
        let next = match (
            first,
            current.and_then(|c| candidates.iter().position(|&i| i == c)),
        ) {
            (false, Some(at)) => {
                let n = candidates.len() as i32;
                candidates[((at as i32 + step).rem_euclid(n)) as usize]
            }
            _ => {
                if step < 0 {
                    candidates[candidates.len() - 1]
                } else {
                    candidates[0]
                }
            }
        };
        self.set_focus(menu, next);
    }

    fn visible_items(&mut self, menu: &str) -> Vec<usize> {
        let Some(def) = self.catalog.get(menu) else {
            return Vec::new();
        };
        let hidden: Vec<bool> = self
            .menus
            .stack
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(menu))
            .map(|m| m.items.iter().map(|s| s.hidden).collect())
            .unwrap_or_default();
        let (def, _) = crate::killcam_skip::inherit_shared_vis(def);
        let open = self.menus.open_names();
        let host = MenuHost {
            world: self.world,
            menu: &def,
            locals: self.locals,
            open: &open,
            focus_rect: None,
        };
        let mut out = Vec::new();
        for (index, item) in def.items.iter().enumerate() {
            if hidden.get(index).copied().unwrap_or(false) {
                continue;
            }
            let enabled = item.disabled_exp.is_empty()
                || matches!(self.exprs.is_true(&item.disabled_exp, &host), Ok(false));
            if enabled && matches!(self.exprs.is_true(&item.vis_exp, &host), Ok(true)) {
                out.push(index);
            }
        }
        out
    }

    fn activate(&mut self, menu: &str, index: usize, accept: bool) {
        let catalog = self.catalog;
        let Some(item) = catalog.get(menu).and_then(|d| d.items.get(index)) else {
            return;
        };
        let events = if accept && !item.handlers.accept.is_empty() {
            &item.handlers.accept
        } else {
            &item.handlers.action
        };
        self.run_events(menu, Some(index), events);
    }
}

fn painted_def(def: &MenuDef, open: &OpenMenu) -> MenuDef {
    let (mut out, _) = crate::killcam_skip::inherit_shared_vis(def);
    for (item, state) in out.items.iter_mut().zip(&open.items) {
        if state.hidden {
            item.vis_exp = String::from("0");
        }
        if let Some(color) = state.fore {
            item.fore_color = color;
        }
        if let Some(color) = state.back {
            item.back_color = color;
        }
    }
    out
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
    mouse: Res<'w, ButtonInput<MouseButton>>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    hud_input: Option<ResMut<'w, frame::HudInputView>>,
    screen: Res<'w, AppScreen>,
    actions: Option<Res<'w, ClientActionInput>>,
    requests: MessageReader<'w, 's, UiMenuRequest>,
    classes: Option<Res<'w, frame::HostClassLoadouts>>,
}

#[derive(Default)]
struct Pressed {
    escape: bool,
    enter: bool,
    up: bool,
    down: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuOutputs<'w> {
    sounds: MessageWriter<'w, UiPlaySound>,
    exec: MessageWriter<'w, UiExecCommand>,
    inbox: Option<ResMut<'w, ClientActionInbox>>,
    ids: Option<ResMut<'w, ActionRequestIds>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_script_menus(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    teams: Option<Res<SessionTeamSettings>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    role: Option<Res<RuntimeRole>>,
    mut input: MenuInputs,
    mut torn: MessageReader<MatchTornDown>,
    mut menus: ResMut<ScriptMenus>,
    mut locals: ResMut<UiLocalVars>,
    mut exprs: ResMut<MenuExprCache>,
    mut out: MenuOutputs,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut pass: ResMut<HudTessPass>,
) {
    pass.script_menus = TessJob::Hide;
    if torn.read().count() > 0 {
        *menus = ScriptMenus::default();
    }
    let requests: Vec<UiMenuRequest> = input.requests.read().cloned().collect();
    let Some(catalog) = catalog.as_deref() else {
        return;
    };
    let snapshot = presented.snapshot();
    let meta = snapshot.and_then(|s| s.meta.for_client(local.0));
    let in_game = matches!(*input.screen, AppScreen::InGame | AppScreen::ClassSelect);
    if !in_game || meta.is_none() {
        if !menus.stack.is_empty() {
            *menus = ScriptMenus::default();
        }
        if let Some(view) = input.hud_input.as_mut()
            && view.script_menu_open
        {
            view.script_menu_open = false;
        }
        return;
    }
    let (Some(snapshot), Some(meta)) = (snapshot, meta) else {
        return;
    };
    let now_ms = snapshot.tick.0.saturating_mul(sim::MATCH_TICK_MS) as i32;
    let world = MenuWorld {
        ms: crate::scorebar::sys_milliseconds() as i32,
        dvars: Some(snapshot.meta.script_dvars(local.0)),
        sv_running: role.is_some_and(|r| r.runs_authority()),
        catalog: Some(catalog),
        localize: strings.as_ref().map(|s| &s.0),
        kind: Some(snapshot.meta.kind),
        team: meta.client_state_team,
        team_scores: snapshot.meta.objectives.scores,
        player_score: meta.score,
        time_left_s: snapshot
            .meta
            .objectives
            .time_left_ms(now_ms)
            .div_euclid(1000),
        teams: teams.as_ref().map(|t| &t.0),
        scores_open: input
            .actions
            .as_ref()
            .is_some_and(|a| a.client.kb.scores.active),
        classes: input.classes.as_deref(),
    };

    let mut runner = Runner {
        catalog,
        world: &world,
        menus: &mut menus,
        locals: &mut locals,
        exprs: &mut exprs,
        depth: 0,
    };

    for command in &meta.menu_commands {
        if command.serial <= runner.menus.applied_serial {
            continue;
        }
        runner.menus.applied_serial = command.serial;
        match &command.kind {
            sim::MenuCommandKind::Open(name) => runner.open(name),
            sim::MenuCommandKind::ClosePopup => {
                if let Some(top) = runner.menus.stack.last().map(|m| m.name.clone()) {
                    runner.close(&top);
                }
            }
            sim::MenuCommandKind::CloseInGame => runner.close_all(),
        }
    }

    let rust_menu = input.hud_input.as_ref().is_some_and(|h| h.menu_open);
    let console_open = input.hud_input.as_ref().is_some_and(|h| h.console_open);
    let mut pressed = Pressed::default();
    for request in requests {
        match request {
            UiMenuRequest::Toggle => pressed.escape = true,
            UiMenuRequest::Open(name) => runner.open(&name),
            UiMenuRequest::Close(name) => runner.close(&name),
            UiMenuRequest::Key(UiMenuKey::Escape) => pressed.escape = true,
            UiMenuRequest::Key(UiMenuKey::Enter) => pressed.enter = true,
            UiMenuRequest::Key(UiMenuKey::Up) => pressed.up = true,
            UiMenuRequest::Key(UiMenuKey::Down) => pressed.down = true,
        }
    }
    let pointer = !rust_menu && !console_open;
    if pointer {
        let keys = &input.keys;
        pressed.escape |= keys.just_pressed(KeyCode::Escape);
        pressed.enter |=
            keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
        pressed.up |= keys.just_pressed(KeyCode::ArrowUp);
        pressed.down |= keys.just_pressed(KeyCode::ArrowDown);
    }
    if !rust_menu {
        handle_input(&mut runner, &input, &surface, &pressed, pointer);
    }

    flush_outputs(&mut menus, &mut out, local.0);
    if let Some(view) = input.hud_input.as_mut() {
        let open = menus.captures_input();
        if view.script_menu_open != open {
            view.script_menu_open = open;
        }
    }

    let scoreboard = crate::scoreboard::displayed(world.scores_open, snapshot, local.0)
        .then(|| catalog.get(SCOREBOARD_MENU))
        .flatten();
    if (menus.stack.is_empty() && scoreboard.is_none()) || rust_menu || !surface.is_ready() {
        return;
    }
    let open_names = menus.open_names();
    let mut list = Draw2dList::default();
    let mut reported = std::mem::take(&mut menus.reported);
    let mut layers: Vec<(String, std::borrow::Cow<MenuDef>, Option<[f32; 4]>)> = Vec::new();
    if let Some(def) = scoreboard {
        layers.push((
            SCOREBOARD_MENU.to_owned(),
            std::borrow::Cow::Borrowed(def),
            None,
        ));
    }
    for open in &menus.stack {
        let Some(def) = catalog.get(&open.name) else {
            continue;
        };
        let painted = painted_def(def, open);
        let focus_rect = open.focus.and_then(|i| painted.items.get(i)).map(item_rect);
        layers.push((
            open.name.clone(),
            std::borrow::Cow::Owned(painted),
            focus_rect,
        ));
    }
    for (name, painted, focus_rect) in &layers {
        let host = MenuHost {
            world: &world,
            menu: painted,
            locals: &locals,
            open: &open_names,
            focus_rect: *focus_rect,
        };
        let frame = execute_chrome_menu(
            painted,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            &mut exprs,
        );
        if frame.coverage.typed_gap > 0 && !reported.contains(name) {
            reported.insert(name.clone());
            let gaps: Vec<String> = frame
                .coverage
                .gap_ids
                .iter()
                .map(|(index, kind)| {
                    let item = &painted.items[*index];
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
                painted.items.len(),
                gaps.join(" ")
            );
        }
        list.cmds.extend(frame.list.cmds);
    }
    menus.reported = reported;
    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    for cmd in &list.cmds {
        let _ = hud_images.get(
            crate::images::HUD_CHROME_NAMESPACE,
            &cmd.material,
            &mut images,
        );
        if let Draw2dOp::TextRun { font, .. } = &cmd.op
            && !fonts.contains_key(font)
            && let Some(def) = catalog.font(font)
        {
            fonts.insert(font.clone(), def);
        }
    }
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.script_menus = TessJob::Quads(quads);
    }
}

fn handle_input(
    runner: &mut Runner<'_, '_>,
    input: &MenuInputs,
    surface: &crate::surface::Hud2dSurface,
    pressed: &Pressed,
    pointer: bool,
) {
    if pressed.escape {
        match runner.menus.stack.last().map(|m| m.name.clone()) {
            Some(top) => runner.escape(&top),
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
    let Some(top) = runner.menus.stack.last().map(|m| m.name.clone()) else {
        return;
    };

    if pressed.down {
        runner.focus_step(&top, 1, false);
    }
    if pressed.up {
        runner.focus_step(&top, -1, false);
    }
    if pressed.enter {
        if let Some(focus) = runner.menus.stack.last().and_then(|m| m.focus) {
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
    let Some(def) = runner.catalog.get(&top) else {
        return;
    };
    let Some(open) = runner.menus.stack.last() else {
        return;
    };
    let painted = painted_def(def, open);
    let names = runner.menus.open_names();
    let host = MenuHost {
        world: runner.world,
        menu: &painted,
        locals: runner.locals,
        open: &names,
        focus_rect: None,
    };
    let rects = item_screen_rects(&painted, &host, surface, runner.exprs);
    let visible = runner.visible_items(&top);
    let hit = rects.iter().rev().find_map(|&(index, [x0, y0, x1, y1])| {
        let inside = cursor.x >= x0 && cursor.x < x1 && cursor.y >= y0 && cursor.y < y1;
        (inside && is_focusable(&def.items[index]) && visible.contains(&index)).then_some(index)
    });
    runner.set_hover(&top, hit);
    if input.mouse.just_pressed(MouseButton::Left)
        && let Some(index) = hit
    {
        runner.activate(&top, index, false);
    }
}

fn flush_outputs(menus: &mut ScriptMenus, out: &mut MenuOutputs, local: sim::ClientId) {
    for alias in menus.sounds.drain(..) {
        out.sounds.write(UiPlaySound { alias });
    }
    for text in menus.exec.drain(..) {
        diag::info!(Ui, "menu exec: {text}");
        out.exec.write(UiExecCommand { text });
    }
    let (Some(inbox), Some(ids)) = (out.inbox.as_mut(), out.ids.as_mut()) else {
        menus.responses.clear();
        return;
    };
    for (menu, response) in menus.responses.drain(..) {
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
        let request_id = ids.allocate();
        match inbox.push(
            local,
            sim::ClientAction::MenuResponse {
                request_id,
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
