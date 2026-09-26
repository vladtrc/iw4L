use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use frame::ViewSubject;
use hud_iw4::{ExprError, ExprHost, Operand};
use net::{ClientActionInput, LocalPresentClient, PresentedSnapshot};

use crate::chrome::{ChromeAssets, execute_chrome_menu};
use crate::draw2d::{Draw2dList, Draw2dOp, tessellate_fonts};
use crate::font_overlay;
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::sys_milliseconds;

#[derive(Component)]
pub(crate) struct KillcamSkipRaster;
pub(crate) fn spawn_killcam_skip(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, KillcamSkipRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.killcam_skip = TessJob::Hide;
}

pub(crate) fn inherit_shared_vis(menu: &assets::MenuDef) -> (assets::MenuDef, i64) {
    let mut out = menu.clone();
    let mut prev = String::new();
    let mut n = 0i64;
    for item in &mut out.items {
        if item.vis_exp.is_empty() && item.vis_ptr != 0 && !prev.is_empty() {
            item.vis_exp = prev.clone();
            n += 1;
        } else if !item.vis_exp.is_empty() {
            prev = item.vis_exp.clone();
        }
    }
    (out, n)
}

struct KillcamExprHost<'a> {
    menu: Option<&'a assets::MenuDef>,
    ms: i32,
    seated: i32,
    game_ended: i32,
    scores_open: i32,
    dvars: sim::ScriptDvars<'a>,
    kind: gamemode_iw4::GameModeKind,
    localize: Option<&'a assets::LocalizeCatalog>,
}

impl ExprHost for KillcamExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        let name = self
            .menu
            .and_then(|m| m.static_dvar_name(index))
            .ok_or(ExprError::Host("static dvar name"))?;
        self.dvar_int(name)
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
        Ok(0)
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Ok(0)
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        crate::scorebar::gametype_display_name(self.kind, self.localize)
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        Err(ExprError::Host("weapon lock"))
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        if let Some(value) = self.dvars.int(name) {
            Ok(value)
        } else if name.eq_ignore_ascii_case("scr_gameended") {
            Ok(self.game_ended)
        } else if name.eq_ignore_ascii_case("splitscreen") {
            Ok(0)
        } else {
            Err(ExprError::Host("dvarint"))
        }
    }
    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(self.seated)
    }
    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(0)
    }
    fn scoreboard_visible(&self) -> Result<i32, ExprError> {
        Ok(self.scores_open)
    }
    fn menu_is_open(&self, name: &str) -> Result<i32, ExprError> {
        if name.eq_ignore_ascii_case("scoreboard") {
            Ok(self.scores_open)
        } else {
            Ok(0)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_killcam_skip(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: Option<Res<ViewSubject>>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    actions: Option<Res<ClientActionInput>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
) {
    let seated = view.as_deref().is_some_and(|v| v.in_killcam());
    if !seated {
        hide(&mut pass);
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let Some(hud) = snapshot
        .meta
        .for_client(local.0)
        .and_then(|meta| meta.killcam_hud)
    else {
        hide(&mut pass);
        return;
    };
    let game_ended = i32::from(hud.final_kill);

    let scores_open = i32::from(actions.as_ref().is_some_and(|a| a.client.kb.scores.active));

    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::NoFontCatalog);
        hide(&mut pass);
        return;
    };
    let Some(menu) = catalog.get("killcam_fullscreen") else {
        hide(&mut pass);
        return;
    };
    let host = KillcamExprHost {
        menu: Some(menu),
        ms: sys_milliseconds() as i32,
        seated: 1,
        game_ended,
        scores_open,
        dvars: snapshot.meta.script_dvars(local.0),
        kind: snapshot.meta.kind,
        localize: strings.as_ref().map(|s| &s.0),
    };
    let (menu, _) = inherit_shared_vis(menu);
    let frame = execute_chrome_menu(
        &menu,
        &host,
        &surface,
        ChromeAssets {
            catalog: Some(catalog),
            localize: strings.as_ref().map(|s| &s.0),
        },
        &mut exprs,
    );
    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    for cmd in &frame.list.cmds {
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
    let list = Draw2dList {
        cmds: frame.list.cmds,
    };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.killcam_skip = TessJob::Quads(quads);
}
