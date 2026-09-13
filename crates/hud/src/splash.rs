use std::collections::HashMap;

use assets::{CapturedStringTable, MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    ExprError, ExprHost, Operand, SPLASH_COL_DESCRIPTION, SPLASH_COL_DURATION, SPLASH_COL_MATERIAL,
    SPLASH_COL_MENU, SPLASH_COL_TEXT, SPLASH_SLOT_COUNT, SPLASH_TABLE_NAME, SplashSlot,
    cg_activate_splash, item_run_script_lerp, splash_duration_ms, splash_has_icon,
    splash_replace_optional,
};

use crate::chrome::{ChromeAssets, ChromeFrame, ChromeMenuAnim, execute_chrome_menu_with_anim};
use crate::draw2d::{Draw2dOp, tessellate_fonts};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::sys_milliseconds;

#[derive(Resource, Default)]
pub struct PendingSplash {
    pub key: Option<String>,
    pub optional_number: i32,
}

#[derive(Resource, Default)]
pub(crate) struct SplashSlots {
    pub slots: [SplashSlot; SPLASH_SLOT_COUNT],
}
#[derive(Component)]
pub(crate) struct SplashRaster;

struct SplashExprHost<'a> {
    ms: i32,
    slots: &'a [SplashSlot; SPLASH_SLOT_COUNT],
    table: Option<&'a CapturedStringTable>,
    localize: Option<&'a assets::LocalizeCatalog>,
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
}

impl ExprHost for SplashExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        match index {
            22 => Ok(0),

            26 => Ok(0),
            _ => Err(ExprError::Host("static dvar")),
        }
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
        let translated = self.loc(cell);
        Ok(Operand::Str(splash_replace_optional(
            &translated,
            s.optional_number,
        )))
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
        let translated = self.loc(cell);
        Ok(Operand::Str(splash_replace_optional(
            &translated,
            s.optional_number,
        )))
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

fn hide(pass: &mut HudTessPass) {
    pass.splash = TessJob::Hide;
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
    let (index, slot) = cg_activate_splash(0, row, duration_ms, pending.optional_number, now_ms);
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_splash(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
    mut pending: ResMut<PendingSplash>,
    mut slots: ResMut<SplashSlots>,
    mut received: MessageReader<net::SvcHudSplash>,
) {
    if !surface.is_ready() {
        return;
    }
    for cmd in received.read().filter(|cmd| cmd.slot == 0) {
        pending.key = Some(cmd.key.clone());
        pending.optional_number = cmd.optional;
    }
    let now_ms = sys_milliseconds() as i32;
    let table = catalog
        .as_ref()
        .and_then(|c| c.string_table(SPLASH_TABLE_NAME));
    if pending.key.is_some() && catalog.is_some() && table.is_none() {
        gaps.raise(GapCause::SplashNoTable);
    }
    if let Some(miss) = activate_pending(&mut pending, &mut slots, table, now_ms) {
        gaps.raise(GapCause::SplashKeyMissing { key: miss });
        hide(&mut pass);
        return;
    }
    expire_slots(&mut slots, now_ms);

    let live = slots.slots.iter().find(|s| s.live()).copied();
    let Some(live) = live else {
        if pending.key.is_none() {
            gaps.clear(HudGap::EngineSplash);
        }
        hide(&mut pass);
        return;
    };

    let Some(table) = table else {
        gaps.raise(GapCause::SplashNoTable);
        hide(&mut pass);
        return;
    };
    let menu_name = table.cell(live.row, SPLASH_COL_MENU);
    if menu_name.is_empty() {
        gaps.raise(GapCause::SplashNoMenu {
            name: String::new(),
        });
        hide(&mut pass);
        return;
    }

    let Some(menu) = catalog.as_ref().and_then(|c| c.get(menu_name)) else {
        gaps.raise(GapCause::SplashNoMenu {
            name: menu_name.to_owned(),
        });
        hide(&mut pass);
        return;
    };

    let material = table.cell(live.row, SPLASH_COL_MATERIAL);
    let host = SplashExprHost {
        ms: now_ms,
        slots: &slots.slots,
        table: Some(table),
        localize: strings.as_ref().map(|s| &s.0),
    };
    let lerp = item_run_script_lerp(&menu.on_open, live.start_ms);
    let anim = ChromeMenuAnim {
        scale: lerp.scale.current_or(now_ms, 1.0),
        alpha: lerp.alpha.current_or(now_ms, 1.0),
    };
    let ChromeFrame {
        list,
        coverage: _,
        vis_errors: _,
    } = execute_chrome_menu_with_anim(
        menu,
        &host,
        &surface,
        ChromeAssets {
            catalog: catalog.as_deref(),
            localize: strings.as_ref().map(|s| &s.0),
        },
        anim,
        &mut exprs,
    );

    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    for cmd in &list.cmds {
        let _ = hud_images.get(
            crate::images::HUD_CHROME_NAMESPACE,
            &cmd.material,
            &mut images,
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
    let mut image_missing = false;
    if !material.is_empty()
        && hud_images
            .get(crate::images::HUD_CHROME_NAMESPACE, material, &mut images)
            .is_none()
    {
        image_missing = true;
        gaps.raise(GapCause::SplashImageMissing {
            name: material.to_owned(),
            miss: if hud_images.has_games_root() {
                crate::gaps::ImageMiss::NotDecoded
            } else {
                crate::gaps::ImageMiss::NoGamesRoot
            },
        });
    }

    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        gaps.raise(GapCause::SplashEmptyPaint {
            name: menu_name.to_owned(),
        });
        return;
    }
    if !image_missing {
        gaps.clear(HudGap::EngineSplash);
    }
    pass.splash = TessJob::Quads(quads);
}
