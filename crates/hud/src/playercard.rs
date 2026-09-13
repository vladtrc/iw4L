use std::collections::HashMap;

use assets::{CapturedStringTable, MenuCatalog, PreparedLocalizedStrings};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    ExprError, ExprHost, Operand, PLAYER_CARD_SCRIPT_SLOT_COUNT, PLAYERCARD_KILLED_BY_MENU,
    PLAYERCARD_YOU_KILLED_MENU, PlayerCardData, cg_player_cards_set_script_slot, script_menu_name,
    ui_run_op_get_player_card_info,
};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};

use crate::chrome::{ChromeAssets, ChromeFrame, execute_chrome_menu};
use crate::draw2d::{Draw2dOp, tessellate_fonts};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps, ImageMiss};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::sys_milliseconds;

#[derive(Resource, Default)]
pub(crate) struct UiLocalVars {
    ints: HashMap<String, i32>,
    strings: HashMap<String, String>,
}

impl UiLocalVars {
    pub(crate) fn set_int(&mut self, name: &str, value: i32) {
        self.ints.insert(name.to_owned(), value);
    }

    fn set_string(&mut self, name: &str, value: String) {
        self.strings.insert(name.to_owned(), value);
    }

    pub(crate) fn int(&self, name: &str) -> i32 {
        self.ints.get(name).copied().unwrap_or(0)
    }

    pub(crate) fn string(&self, name: &str) -> String {
        if let Some(s) = self.strings.get(name) {
            return s.clone();
        }
        self.ints
            .get(name)
            .map(|v| v.to_string())
            .unwrap_or_default()
    }
}

#[derive(Resource)]
pub(crate) struct PlayerCardCache {
    slots: [PlayerCardData; PLAYER_CARD_SCRIPT_SLOT_COUNT],
    broadcast: Option<(hud_iw4::SplashSlot, String)>,
}

impl Default for PlayerCardCache {
    fn default() -> Self {
        Self {
            broadcast: None,
            slots: core::array::from_fn(|_| PlayerCardData::default()),
        }
    }
}

impl PlayerCardCache {
    fn set(&mut self, slot: i32, data: PlayerCardData) {
        if let Some(row) = self.slots.get_mut(slot as usize) {
            *row = data;
        }
    }

    fn get(&self, slot: i32) -> Option<&PlayerCardData> {
        self.slots.get(slot as usize)
    }
}

#[derive(Component)]
pub(crate) struct PlayerCardRaster;

#[derive(Clone, Copy)]
struct PlayerCardExprHost<'a> {
    menu: Option<&'a assets::MenuDef>,
    ms: i32,
    in_killcam: bool,
    own_team: i32,
    local_vars: &'a UiLocalVars,
    cache: &'a PlayerCardCache,
    catalog: Option<&'a MenuCatalog>,
}

impl PlayerCardExprHost<'_> {
    fn table(&self, name: &str) -> Option<&CapturedStringTable> {
        self.catalog.and_then(|c| c.string_table(name))
    }
}

impl ExprHost for PlayerCardExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        if let Some(name) = self.menu.and_then(|menu| menu.static_dvar_name(index)) {
            return self.dvar_int(name);
        }
        match index {
            4 => Ok(0),
            _ => Err(ExprError::Host("static dvar")),
        }
    }
    fn team_field(&self, field: &str) -> Result<Operand, ExprError> {
        if field.eq_ignore_ascii_case("name") {
            Ok(Operand::Str(
                entity_iw4::cg_get_team_name(self.own_team).to_owned(),
            ))
        } else if field.eq_ignore_ascii_case("team") {
            Ok(Operand::Int(self.own_team))
        } else if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(0))
        } else {
            Err(ExprError::Host("team field"))
        }
    }
    fn player_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("player field"))
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    fn local_var_string(&self, name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(self.local_vars.string(name)))
    }
    fn local_var_int(&self, name: &str) -> Result<i32, ExprError> {
        Ok(self.local_vars.int(name))
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
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        if name.eq_ignore_ascii_case("hiDef") {
            return Ok(1);
        }
        if ["ui_hide_playercards", "splitscreen", "scr_gameended"]
            .iter()
            .any(|dvar| name.eq_ignore_ascii_case(dvar))
        {
            Ok(0)
        } else {
            Err(ExprError::Host("dvarint"))
        }
    }
    fn table_lookup(
        &self,
        table: &str,
        col0: i32,
        key: &str,
        result_col: i32,
    ) -> Result<Operand, ExprError> {
        let Some(t) = self.table(table) else {
            return Ok(Operand::Str(String::new()));
        };
        match t.lookup_row_in_col(col0, key) {
            Some(row) => Ok(Operand::Str(String::from(t.cell(row, result_col)))),
            None => Ok(Operand::Str(String::new())),
        }
    }
    fn table_lookup_by_row(&self, table: &str, row: i32, col: i32) -> Result<Operand, ExprError> {
        let Some(t) = self.table(table) else {
            return Ok(Operand::Str(String::new()));
        };
        Ok(Operand::Str(String::from(t.cell(row, col))))
    }
    fn player_card_info(&self, field: i32, lookup: i32, slot: i32) -> Result<Operand, ExprError> {
        if lookup != 0 {
            return Err(ExprError::Host("playercard lookup"));
        }
        let Some(data) = self.cache.get(slot) else {
            return Ok(ui_run_op_get_player_card_info(
                &PlayerCardData::default(),
                field,
            ));
        };
        Ok(ui_run_op_get_player_card_info(data, field))
    }
    fn splash_description(&self, slot: i32) -> Result<Operand, ExprError> {
        if slot != 1 {
            return Err(ExprError::Host("playercard splash slot"));
        }
        Ok(Operand::Str(
            self.cache
                .broadcast
                .as_ref()
                .map(|(_, description)| description.clone())
                .unwrap_or_default(),
        ))
    }
    fn get_perk(&self, _name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(String::new()))
    }
    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.in_killcam))
    }
}

struct OpenMenuHost {
    ms: i32,
}

impl ExprHost for OpenMenuHost {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, _index: i32) -> Result<i32, ExprError> {
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
    fn local_var_string(&self, _name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(String::new()))
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
    fn get_perk(&self, _name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(String::new()))
    }
}

pub(crate) fn spawn_playercard(root: &mut ChildSpawnerCommands) {
    root.spawn((
        PlayerCardRaster,
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

fn hide(pass: &mut HudTessPass) {
    pass.playercard = TessJob::Hide;
}

fn apply_card_slots(
    slots: &mut MessageReader<net::SvcCardSlotCmd>,
    cache: &mut PlayerCardCache,
    presented: &PresentedSnapshot,
    cg_time: i32,
) {
    for cmd in slots.read() {
        let data = presented
            .snapshot()
            .and_then(|s| s.meta.for_client(sim::ClientId(cmd.client)))
            .map(|meta| {
                cg_player_cards_set_script_slot(
                    cg_time,
                    &meta.name,
                    meta.client_state_team,
                    meta.rank,
                    meta.prestige,
                    meta.player_card_icon,
                    meta.player_card_title,
                    meta.player_card_nameplate,
                )
            })
            .unwrap_or_default();
        cache.set(cmd.slot, data);
    }
}

fn apply_open_menus(
    menus: &mut MessageReader<net::SvcOpenMenuCmd>,
    local_vars: &mut UiLocalVars,
    catalog: Option<&MenuCatalog>,
    exprs: &mut crate::expr_cache::MenuExprCache,
    ms: i32,
) {
    let host = OpenMenuHost { ms };
    for cmd in menus.read() {
        let Some(name) = script_menu_name(cmd.cs_index) else {
            continue;
        };
        let Some(menu) = catalog.and_then(|c| c.get(name)) else {
            continue;
        };
        for lv in &menu.on_open_local_vars {
            match lv.kind {
                3 | 4 => {
                    if let Ok(value) = exprs.evaluate(&lv.expr, &host) {
                        local_vars.set_int(&lv.name, operand_int(&value));
                    }
                }
                5 => {
                    if let Ok(value) = exprs.evaluate_float(&lv.expr, &host) {
                        local_vars.set_int(&lv.name, value as i32);
                    }
                }
                6 => {
                    if let Ok(value) = exprs.evaluate_string(&lv.expr, &host) {
                        local_vars.set_string(&lv.name, value);
                    }
                }
                _ => {}
            }
        }
    }
}

fn operand_int(value: &Operand) -> i32 {
    match value {
        Operand::Int(v) => *v,
        Operand::Float(v) => *v as i32,
        Operand::Str(s) => s.parse().unwrap_or(0),
    }
}

#[derive(SystemParam)]
pub(crate) struct PlayerCardMessages<'w, 's> {
    card_slots: MessageReader<'w, 's, net::SvcCardSlotCmd>,
    splashes: MessageReader<'w, 's, net::SvcHudSplash>,
    open_menus: MessageReader<'w, 's, net::SvcOpenMenuCmd>,
    sound: MessageWriter<'w, frame::UiPlaySound>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_playercard(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
    mut cache: ResMut<PlayerCardCache>,
    mut local_vars: ResMut<UiLocalVars>,
    mut messages: PlayerCardMessages,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Option<Res<frame::ViewSubject>>,
    cg_clock: Res<CgFrameClock>,
) {
    if !surface.is_ready() {
        return;
    }
    let now_ms = sys_milliseconds() as i32;
    apply_card_slots(
        &mut messages.card_slots,
        &mut cache,
        &presented,
        cg_clock.time(),
    );
    apply_open_menus(
        &mut messages.open_menus,
        &mut local_vars,
        catalog.as_deref(),
        &mut exprs,
        now_ms,
    );

    for cmd in messages.splashes.read().filter(|cmd| cmd.slot == 1) {
        let Some(table) = catalog
            .as_deref()
            .and_then(|c| c.string_table(hud_iw4::SPLASH_TABLE_NAME))
        else {
            gaps.raise(GapCause::SplashNoTable);
            continue;
        };
        let Some(row) = table.lookup_row(&cmd.key) else {
            gaps.raise(GapCause::SplashKeyMissing {
                key: cmd.key.clone(),
            });
            continue;
        };
        let duration = hud_iw4::splash_duration_ms(table.cell(row, hud_iw4::SPLASH_COL_DURATION));
        let (_, slot) = hud_iw4::cg_activate_splash(1, row, duration, cmd.optional, now_ms);
        let key = table
            .cell(row, hud_iw4::SPLASH_COL_DESCRIPTION)
            .trim_start_matches('@');
        let Some(description) = strings.as_deref().and_then(|s| s.0.text(key)) else {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: key.to_owned(),
            });
            continue;
        };
        cache.broadcast = Some((
            slot,
            hud_iw4::splash_replace_optional(description, cmd.optional),
        ));
        local_vars.set_int("callout_update_time", now_ms + 1500);
        messages.sound.write(frame::UiPlaySound {
            alias: "mp_card_slide".into(),
        });
    }
    if cache.broadcast.as_ref().is_some_and(|(slot, _)| {
        now_ms
            > slot
                .start_ms
                .saturating_add(slot.duration_ms)
                .saturating_add(150)
    }) {
        cache.broadcast = None;
    }
    let live = local_vars.int("ui_show_youKilled") != 0
        || local_vars.int("ui_show_killedBy") != 0
        || cache.broadcast.is_some();
    if !live {
        gaps.clear(HudGap::PlayerCard);
        hide(&mut pass);
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::PlayerCardNoCatalog);
        hide(&mut pass);
        return;
    };

    let own_team = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .map(|m| m.client_state_team)
        .unwrap_or(0);
    let host = PlayerCardExprHost {
        menu: None,
        ms: now_ms,
        in_killcam: view.as_deref().is_some_and(|v| v.in_killcam()),
        own_team,
        local_vars: &local_vars,
        cache: &cache,
        catalog: Some(catalog),
    };

    let mut list = crate::draw2d::Draw2dList::default();
    let mut any_menu_missing = false;
    let mut table_miss = false;
    let mut empty_paint = false;
    for menu_name in [PLAYERCARD_YOU_KILLED_MENU, PLAYERCARD_KILLED_BY_MENU]
        .into_iter()
        .chain(cache.broadcast.as_ref().map(|_| "playercard_splash"))
    {
        let Some(menu) = catalog.get(menu_name) else {
            any_menu_missing = true;
            gaps.raise(GapCause::PlayerCardNoMenu {
                name: menu_name.to_owned(),
            });
            continue;
        };
        if host.table("mp/cardTitleTable.csv").is_none() {
            table_miss = true;
            gaps.raise(GapCause::PlayerCardTableMissing {
                name: String::from("mp/cardTitleTable.csv"),
            });
        }
        if host.table("mp/cardIconTable.csv").is_none() {
            table_miss = true;
            gaps.raise(GapCause::PlayerCardTableMissing {
                name: String::from("mp/cardIconTable.csv"),
            });
        }
        let host = PlayerCardExprHost {
            menu: Some(menu),
            ..host
        };
        let ChromeFrame {
            list: mut frame_list,
            coverage: _,
            vis_errors: _,
        } = execute_chrome_menu(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            &mut exprs,
        );
        if menu_name == "playercard_splash"
            && let Some((slot, _)) = &cache.broadcast
        {
            let end = slot.start_ms.saturating_add(slot.duration_ms);
            let (script, start) = if now_ms > end {
                (&menu.on_close_request, end)
            } else {
                (&menu.on_open, slot.start_ms)
            };
            let x = hud_iw4::item_run_script_lerp(script, start)
                .x
                .current_or(now_ms, 0.0);
            for cmd in &mut frame_list.cmds {
                cmd.x += x * surface.scale_virtual_to_real()[0];
            }
        }
        if frame_list.cmds.is_empty() {
            empty_paint = true;
        }
        list.cmds.extend(frame_list.cmds);
    }

    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    let mut image_missing = false;
    for cmd in &list.cmds {
        if hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &cmd.material,
                &mut images,
            )
            .is_none()
        {
            image_missing = true;
            gaps.raise(GapCause::PlayerCardMaterialMissing {
                name: cmd.material.clone(),
                miss: if hud_images.has_games_root() {
                    ImageMiss::NotDecoded
                } else {
                    ImageMiss::NoGamesRoot
                },
            });
        }
        if let Draw2dOp::TextRun { font, .. } = &cmd.op {
            if fonts.contains_key(font) {
                continue;
            }
            if let Some(def) = catalog.font(font) {
                fonts.insert(font.clone(), def);
            }
        }
    }

    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        if !any_menu_missing && !table_miss && empty_paint {
            gaps.raise(GapCause::PlayerCardEmptyPaint {
                name: String::from("playercard"),
            });
        }
        return;
    }
    if !image_missing && !any_menu_missing && !table_miss {
        gaps.clear(HudGap::PlayerCard);
    }
    pass.playercard = TessJob::Quads(quads);
}
