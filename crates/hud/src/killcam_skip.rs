use std::collections::HashMap;

use asset_game::MenuCatalog;
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use frame::ViewSubject;
use hud_iw4::{ExprError, ExprHost, Operand};
use net::ClientActionInput;

use crate::chrome::{ChromeAssets, execute_chrome_menu};
use crate::draw2d::{Draw2dList, Draw2dOp, tessellate_fonts};
use crate::font_overlay;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImageLoader;
use crate::scorebar::milliseconds;

#[derive(Component)]
pub(crate) struct KillcamSkipRaster;
pub(crate) fn spawn_killcam_skip(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, KillcamSkipRaster);
}

struct KillcamExprHost<'a> {
    menu: Option<&'a asset_game::MenuDef>,
    ms: i32,
    seated: i32,
    game_ended: i32,
    scores_open: i32,
    dvars: sim::ScriptDvars<'a>,
    kind: gamemode_iw4::GameModeKind,
    localize: Option<&'a asset_game::LocalizeCatalog>,
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct KillcamSkipRenderer<'w> {
    catalog: Option<Res<'w, MenuCatalog>>,
    hud_images: HudImageLoader<'w>,
    pass: ResMut<'w, HudTessPass>,
    exprs: ResMut<'w, crate::expr_cache::MenuExprCache>,
}

impl KillcamSkipRenderer<'_> {
    fn hide(&mut self) {
        self.pass.killcam_skip = TessJob::Hide;
    }

    fn render(
        &mut self,
        surface: &crate::surface::Hud2dSurface,
        meta: &sim::SnapshotMeta,
        client: sim::ClientId,
        strings: Option<Res<PreparedLocalizedStrings>>,
        game_ended: i32,
        scores_open: i32,
    ) {
        let Some(catalog) = self.catalog.as_ref() else {
            diag::warn_once!(Ui, "MenuCatalog is not loaded; Font_s cannot be walked");
            self.hide();
            return;
        };
        let Some(menu) = catalog.get("killcam_fullscreen") else {
            self.hide();
            return;
        };
        let host = KillcamExprHost {
            menu: Some(menu),
            ms: milliseconds() as i32,
            seated: 1,
            game_ended,
            scores_open,
            dvars: meta.script_dvars(client),
            kind: meta.kind,
            localize: strings.as_ref().map(|s| &s.0),
        };
        let frame = execute_chrome_menu(
            menu,
            &host,
            surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            &mut self.exprs,
        );
        let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
        for cmd in &frame.list.cmds {
            let _ = self
                .hud_images
                .load(crate::images::HUD_CHROME_NAMESPACE, &cmd.material);
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
            self.hide();
            return;
        }
        self.pass.killcam_skip = TessJob::Quads(quads);
    }
}

pub(crate) fn update_killcam_skip(
    surface: Res<crate::surface::Hud2dSurface>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: Option<Res<ViewSubject>>,
    presentation: net::LocalPresentation,
    actions: Option<Res<ClientActionInput>>,
    mut renderer: KillcamSkipRenderer,
) {
    let seated = view.as_deref().is_some_and(|v| v.in_killcam());
    if !seated {
        renderer.hide();
        return;
    }
    let Some(snapshot) = presentation.snapshot() else {
        renderer.hide();
        return;
    };
    let Some(hud) = snapshot
        .meta
        .for_client(presentation.client_id())
        .and_then(|meta| meta.killcam_hud)
    else {
        renderer.hide();
        return;
    };
    let game_ended = i32::from(hud.final_kill);

    let scores_open = i32::from(
        crate::scoreboard::recorded_hold(snapshot, presentation.client_id())
            || actions.as_ref().is_some_and(|a| a.client.kb.scores.active),
    );

    if !surface.is_ready() {
        renderer.hide();
        return;
    }
    renderer.render(
        &surface,
        &snapshot.meta,
        presentation.client_id(),
        strings,
        game_ended,
        scores_open,
    );
}
