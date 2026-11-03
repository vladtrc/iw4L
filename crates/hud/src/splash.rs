use std::collections::HashMap;

use asset_game::{CapturedStringTable, MenuCatalog};
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    ExprError, ExprHost, Operand, SPLASH_COL_DESCRIPTION, SPLASH_COL_DURATION, SPLASH_COL_MATERIAL,
    SPLASH_COL_MENU, SPLASH_COL_TEXT, SPLASH_SLOT_COUNT, SPLASH_TABLE_NAME, SplashSlot,
    activate_splash, item_run_script_lerp, splash_duration_ms, splash_has_icon,
    splash_replace_optional,
};

use crate::chrome::{ChromeAssets, ChromeFrame, execute_chrome_menu_with_anim};
use crate::draw2d::{Draw2dOp, tessellate_fonts};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::milliseconds;

#[derive(Resource, Default)]
pub struct PendingSplash {
    pub key: Option<String>,
    pub optional_number: i32,
    queued: std::collections::VecDeque<(String, i32)>,
}

#[derive(Resource, Default)]
pub(crate) struct SplashSlots {
    pub slots: [SplashSlot; SPLASH_SLOT_COUNT],
}
#[derive(Component)]
pub(crate) struct SplashRaster;

struct SplashExprHost<'a> {
    menu: &'a asset_game::MenuDef,
    ms: i32,
    slots: &'a [SplashSlot; SPLASH_SLOT_COUNT],
    table: Option<&'a CapturedStringTable>,
    catalog: Option<&'a MenuCatalog>,
    localize: Option<&'a asset_game::LocalizeCatalog>,
    input: Option<&'a frame::HudInputView>,
}

impl SplashExprHost<'_> {
    fn slot(&self, slot: i32) -> Option<&SplashSlot> {
        let index = if !(0..=4).contains(&slot) {
            0
        } else {
            slot as usize
        };
        self.slots.get(index)
    }

    fn loc(&self, cell: &str) -> String {
        if cell.is_empty() {
            return String::new();
        }
        let key = if let Some(rest) = cell.strip_prefix('@') {
            rest
        } else {
            cell
        };
        if let Some(table) = self.localize {
            if let Some(text) = table.text(key) {
                return text.to_owned();
            }
        }

        String::from(key)
    }

    fn key_for(&self, command: &str) -> String {
        let bound = self.input.and_then(|input| {
            if matches!(command, "+activate" | "+usereload") {
                return input.use_key.clone();
            }
            let slot = command
                .strip_prefix("+actionslot ")?
                .trim()
                .parse::<usize>()
                .ok()?
                .checked_sub(1)?;
            input.action_slot_keys.get(slot)?.clone()
        });
        bound.unwrap_or_else(|| {
            let unbound = self
                .localize
                .and_then(|t| t.text(hud_iw4::KEY_UNBOUND))
                .unwrap_or(hud_iw4::KEY_UNBOUND);
            hud_iw4::unbound_directive(unbound, command)
        })
    }

    fn cell_text(&self, cell: &str, optional_number: i32) -> String {
        let translated = hud_iw4::replace_directive(&self.loc(cell), |cmd| self.key_for(cmd));
        splash_replace_optional(&translated, optional_number)
    }
}

impl ExprHost for SplashExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        let name = self
            .menu
            .static_dvar_name(index)
            .ok_or(ExprError::Host("static dvar name"))?;
        if name.eq_ignore_ascii_case("splitscreen")
            || name.eq_ignore_ascii_case("camera_thirdPerson")
        {
            return Ok(0);
        }
        Err(ExprError::Host("static dvar"))
    }
    fn team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("team field"))
    }
    fn player_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("player field"))
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    // The menu's onOpen setLocalVar* expressions only read the live slot, so evaluating
    // them on demand gives the value they would have stored at open.
    fn local_var_string(&self, name: &str) -> Result<Operand, ExprError> {
        let Some(var) = self
            .menu
            .on_open_local_vars
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(name))
        else {
            return Ok(Operand::Str(String::new()));
        };
        hud_iw4::expr::evaluate(&var.expr, self)
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("timeleft"))
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("score"))
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("gametype"))
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        Err(ExprError::Host("weapon lock"))
    }
    fn splash_text(&self, slot: i32) -> Result<Operand, ExprError> {
        let Some(s) = self.slot(slot) else {
            return Ok(Operand::Str(String::new()));
        };
        if !s.live() {
            return Ok(Operand::Str(String::new()));
        }
        let cell = match self.table {
            Some(t) => t.cell(s.row, SPLASH_COL_TEXT),
            None => "",
        };
        Ok(Operand::Str(self.cell_text(cell, s.optional_number)))
    }
    fn splash_description(&self, slot: i32) -> Result<Operand, ExprError> {
        let Some(s) = self.slot(slot) else {
            return Ok(Operand::Str(String::new()));
        };
        if !s.live() {
            return Ok(Operand::Str(String::new()));
        }
        let cell = match self.table {
            Some(t) => t.cell(s.row, SPLASH_COL_DESCRIPTION),
            None => "",
        };
        Ok(Operand::Str(self.cell_text(cell, s.optional_number)))
    }
    fn splash_material(&self, slot: i32) -> Result<Operand, ExprError> {
        let Some(s) = self.slot(slot) else {
            return Ok(Operand::Str(String::new()));
        };
        if !s.live() {
            return Ok(Operand::Str(String::new()));
        }
        let cell = match self.table {
            Some(t) => t.cell(s.row, SPLASH_COL_MATERIAL),
            None => "",
        };
        Ok(Operand::Str(String::from(cell)))
    }
    fn splash_has_icon(&self, slot: i32) -> Result<Operand, ExprError> {
        let Some(s) = self.slot(slot) else {
            return Ok(Operand::Int(0));
        };
        if !s.live() {
            return Ok(Operand::Int(0));
        }
        let cell = match self.table {
            Some(t) => t.cell(s.row, SPLASH_COL_MATERIAL),
            None => "",
        };
        Ok(Operand::Int(i32::from(splash_has_icon(cell))))
    }
    fn splash_row_num(&self, slot: i32) -> Result<Operand, ExprError> {
        let Some(s) = self.slot(slot) else {
            return Ok(Operand::Int(0));
        };
        if !s.live() {
            return Ok(Operand::Int(0));
        }
        Ok(Operand::Int(s.row))
    }
    fn table_lookup(
        &self,
        table: &str,
        col0: i32,
        key: &str,
        result_col: i32,
    ) -> Result<Operand, ExprError> {
        let Some(t) = self.catalog.and_then(|c| c.string_table(table)) else {
            return Err(ExprError::Host("string table"));
        };
        Ok(Operand::Str(
            t.lookup_row_in_col(col0, key)
                .map(|row| t.cell(row, result_col).to_owned())
                .unwrap_or(String::new()),
        ))
    }
    fn table_lookup_by_row(&self, table: &str, row: i32, col: i32) -> Result<Operand, ExprError> {
        match self.table {
            Some(t) if table.eq_ignore_ascii_case(SPLASH_TABLE_NAME) => {
                Ok(Operand::Str(String::from(t.cell(row, col))))
            }
            _ => Err(ExprError::Host("splash host table")),
        }
    }
}

pub(crate) fn spawn_splash(root: &mut ChildSpawnerCommands) {
    root.spawn((
        SplashRaster,
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

fn activate_pending(
    pending: &mut PendingSplash,
    slots: &mut SplashSlots,
    table: Option<&CapturedStringTable>,
    now_ms: i32,
) -> Option<String> {
    let key = pending.key.take()?;
    let Some(table) = table else {
        pending.key = Some(key);
        return None;
    };
    let Some(row) = table.lookup_row(&key) else {
        return Some(key);
    };
    let duration_ms = splash_duration_ms(table.cell(row, SPLASH_COL_DURATION));
    let (index, slot) = activate_splash(0, row, duration_ms, pending.optional_number, now_ms);
    slots.slots[index] = slot;
    None
}

fn expire_slots(slots: &mut SplashSlots, now_ms: i32) {
    for slot in &mut slots.slots {
        if slot.expired(now_ms) {
            *slot = SplashSlot::default();
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SplashDisplay<'w, 's> {
    clock: Res<'w, net::FrameClock>,
    received: MessageReader<'w, 's, net::SvcHudSplash>,
    pending: ResMut<'w, PendingSplash>,
    slots: ResMut<'w, SplashSlots>,
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    exprs: ResMut<'w, crate::expr_cache::MenuExprCache>,
    pass: ResMut<'w, HudTessPass>,
}

impl SplashDisplay<'_, '_> {
    fn hide(&mut self) {
        self.pass.splash = TessJob::Hide;
    }

    fn update(
        &mut self,
        surface: Res<crate::surface::Hud2dSurface>,
        catalog: Option<Res<MenuCatalog>>,
        strings: Option<Res<PreparedLocalizedStrings>>,
        input: Option<Res<frame::HudInputView>>,
        presentation: net::LocalPresentation,
    ) {
        if !surface.is_ready() {
            return;
        }
        for cmd in self.received.read().filter(|cmd| cmd.slot == 0) {
            self.pending
                .queued
                .push_back((cmd.key.clone(), cmd.optional));
        }
        let now_ms = if self.clock.started() {
            self.clock.time()
        } else {
            milliseconds() as i32
        };
        expire_slots(&mut self.slots, now_ms);
        if self.pending.key.is_none() && !self.slots.slots.iter().any(|s| s.live()) {
            if let Some((key, optional)) = self.pending.queued.pop_front() {
                self.pending.key = Some(key);
                self.pending.optional_number = optional;
            }
        }
        let table = catalog
            .as_ref()
            .and_then(|c| c.string_table(SPLASH_TABLE_NAME));
        if self.pending.key.is_some() && catalog.is_some() && table.is_none() {
            diag::warn_once!(
                Ui,
                "mp/splashTable.csv was not in the UI-zone StringTable walk"
            );
        }
        if let Some(miss) = activate_pending(&mut self.pending, &mut self.slots, table, now_ms) {
            diag::warn_once!(Ui, "splashTable has no row `{key}`", key = miss);
            self.hide();
            return;
        }

        let live = self.slots.slots.iter().find(|s| s.live()).copied();
        let Some(live) = live else {
            self.hide();
            return;
        };

        let Some(table) = table else {
            diag::warn_once!(
                Ui,
                "mp/splashTable.csv was not in the UI-zone StringTable walk"
            );
            self.hide();
            return;
        };
        let menu_name = table.cell(live.row, SPLASH_COL_MENU);
        if menu_name.is_empty() {
            diag::warn_once!(
                Ui,
                "MenuCatalog has no `{name}` for this splash row",
                name = String::new()
            );
            self.hide();
            return;
        }

        let Some(menu) = catalog.as_ref().and_then(|c| c.get(menu_name)) else {
            diag::warn_once!(
                Ui,
                "MenuCatalog has no `{name}` for this splash row",
                name = menu_name.to_owned()
            );
            self.hide();
            return;
        };

        let mut promotion;
        let menu = if menu_name.eq_ignore_ascii_case("promotion") {
            promotion = menu.clone();
            let rank = presentation
                .snapshot()
                .and_then(|snap| snap.meta.for_client(presentation.client_id()));
            let icon = rank
                .and_then(|meta| {
                    let column = meta.prestige.checked_add(1).filter(|column| *column > 0)?;
                    catalog
                        .as_ref()?
                        .string_table("mp/rankIconTable.csv")
                        .map(|table| table.lookup_col(&meta.rank.to_string(), column))
                })
                .unwrap_or("");
            let title = rank
                .and_then(|meta| {
                    catalog
                        .as_ref()?
                        .string_table("mp/rankTable.csv")
                        .map(|table| table.lookup_col(&meta.rank.to_string(), 5))
                })
                .unwrap_or("");
            for item in &mut promotion.items {
                if item.name.starts_with("promotion_rank_icon") {
                    item.material_exp.clear();
                    item.background = icon.to_owned();
                } else if item.name.starts_with("promotion_rank_text") {
                    item.text_exp.clear();
                    item.text_key = if title.is_empty() {
                        String::new()
                    } else {
                        format!("@{}", title.trim_start_matches('@'))
                    };
                    item.text_literal = false;
                }
            }
            &promotion
        } else {
            menu
        };

        let material = table.cell(live.row, SPLASH_COL_MATERIAL);
        let host = SplashExprHost {
            menu,
            ms: now_ms,
            slots: &self.slots.slots,
            table: Some(table),
            catalog: catalog.as_deref(),
            localize: strings.as_ref().map(|s| &s.0),
            input: input.as_deref(),
        };
        let lerp = item_run_script_lerp(&menu.on_open, live.start_ms);
        for command in &lerp.leftover {
            diag::warn_once!(
                Ui,
                "{menu}: menu script `{command}` is not executed",
                menu = menu_name.to_owned(),
                command = command.clone()
            );
        }
        let anim = lerp.anim(now_ms);
        let ChromeFrame {
            list,
            coverage: _,
            vis_errors,
        } = execute_chrome_menu_with_anim(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: catalog.as_deref(),
                localize: strings.as_ref().map(|s| &s.0),
            },
            anim,
            &mut self.exprs,
        );
        for (item, err) in vis_errors {
            diag::warn_once!(
                Ui,
                "{menu}[{item}] expression: {err}",
                menu = menu_name.to_owned()
            );
        }

        let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
        for cmd in &list.cmds {
            let _ = self.hud_images.get(
                crate::images::HUD_CHROME_NAMESPACE,
                &cmd.material,
                &mut self.images,
            );
        }
        if let Some(cat) = catalog.as_deref() {
            for cmd in &list.cmds {
                if let Draw2dOp::TextRun { font, .. } = &cmd.op {
                    if fonts.contains_key(font) {
                        continue;
                    }
                    if let Some(def) = cat.font(font) {
                        fonts.insert(font.clone(), def);
                    }
                }
            }
        }

        if !material.is_empty()
            && self
                .hud_images
                .get(
                    crate::images::HUD_CHROME_NAMESPACE,
                    material,
                    &mut self.images,
                )
                .is_none()
        {
            diag::warn_once!(
                Ui,
                "splash image `{name}` is {miss}",
                name = material.to_owned(),
                miss = if self.hud_images.has_games_root() {
                    "not a zone atlas and not in the IWD"
                } else {
                    "no games root is set"
                }
            );
        }

        let (quads, _) = tessellate_fonts(&list, &fonts);
        if quads.is_empty() {
            self.hide();
            diag::warn_once!(
                Ui,
                "`{name}` paint emitted no tess quads for a live splash slot",
                name = menu_name.to_owned()
            );
            return;
        }

        self.pass.splash = TessJob::Quads(quads);
    }
}

pub(crate) fn update_splash(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    input: Option<Res<frame::HudInputView>>,
    presentation: net::LocalPresentation,
    mut display: SplashDisplay,
) {
    display.update(surface, catalog, strings, input, presentation);
}
