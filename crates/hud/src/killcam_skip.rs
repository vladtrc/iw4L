use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use frame::ViewSubject;
use hud_iw4::{
    ALIGN_CENTER, ExprError, ExprHost, KEY_UNBOUND, Operand, hudelem_default_text_scale,
    r_normalized_text_scale, replace_directive, ui_get_font_handle, unbound_directive,
};
use killcam_iw4::{LOWER_MESSAGE_ALPHA, LOWER_TEXT_FONT_SIZE, LOWER_TEXT_Y};
use net::{ClientActionInput, LocalPresentClient, PresentedSnapshot};

use crate::chrome::{ChromeAssets, execute_chrome_menu, r_text_width};
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
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

fn inherit_shared_vis(menu: &assets::MenuDef) -> (assets::MenuDef, i64) {
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

fn resolve_directive(
    command: &str,
    strings: &PreparedLocalizedStrings,
    input: &frame::HudInputView,
) -> Option<String> {
    if matches!(command, "+activate" | "+usereload")
        && let Some(letter) = input.use_key.as_deref()
    {
        return Some(letter.to_owned());
    }
    let unbound = strings.0.text(KEY_UNBOUND)?;
    Some(unbound_directive(unbound, command))
}

struct KillcamExprHost {
    ms: i32,
    seated: i32,
    game_ended: i32,
    scores_open: i32,
}

impl ExprHost for KillcamExprHost {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        match index {
            4 => Ok(self.game_ended),
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
        Ok(0)
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Ok(0)
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Ok(Operand::Str(String::from("MPUI_DEATHMATCH")))
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        if name.eq_ignore_ascii_case("scr_gameended") {
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
    input: Res<frame::HudInputView>,
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
    let Some(hud) = presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(local.0))
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
    let host = KillcamExprHost {
        ms: sys_milliseconds() as i32,
        seated: 1,
        game_ended,
        scores_open,
    };

    let mut cmds = Vec::new();
    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    if let Some(menu) = catalog.get("killcam_fullscreen") {
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
        for cmd in &frame.list.cmds {
            let _ = hud_images.get(
                crate::images::HUD_CHROME_NAMESPACE,
                &cmd.material,
                &mut images,
            );
        }
        for cmd in &frame.list.cmds {
            let Draw2dOp::TextRun { font, .. } = &cmd.op else {
                continue;
            };
            if fonts.contains_key(font) {
                continue;
            }
            if let Some(def) = catalog.font(font) {
                fonts.insert(font.clone(), def);
            }
        }
        cmds = frame.list.cmds;
    }

    if let Some(key) =
        killcam_iw4::kc_info_loc_key(hud.time_until_respawn_ms as f32 / 1000.0, hud.final_kill)
    {
        let Some(strings) = strings.as_ref() else {
            gaps.raise(GapCause::NoStringTable);
            hide(&mut pass);
            return;
        };
        let Some(template) = strings.0.text(key) else {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: key.to_owned(),
            });
            hide(&mut pass);
            return;
        };

        let mut bind_letter = None;
        let mut missing_unbound = false;
        let text = replace_directive(template, |cmd| {
            match resolve_directive(cmd, strings, &input) {
                Some(resolved) => {
                    if bind_letter.is_none() {
                        bind_letter = Some(resolved.clone());
                    }
                    resolved
                }
                None => {
                    missing_unbound = true;
                    String::new()
                }
            }
        });
        if missing_unbound {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: KEY_UNBOUND.to_owned(),
            });
            hide(&mut pass);
            return;
        }
        let text_scale = hudelem_default_text_scale(LOWER_TEXT_FONT_SIZE);
        let font_name = ui_get_font_handle(0, surface.scale_virtual_to_real()[1], text_scale);
        let Some(font) = catalog.font(font_name) else {
            gaps.raise(GapCause::FontMissing {
                name: font_name.to_owned(),
            });
            hide(&mut pass);
            return;
        };
        fonts.entry(font_name.to_owned()).or_insert(font);

        let nscale = r_normalized_text_scale(font.pixel_height, text_scale);
        let measured = r_text_width(font, &text) as f32 * nscale;
        let applied = surface.apply_rect(
            -measured / 2.0,
            LOWER_TEXT_Y,
            nscale,
            nscale,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        let material = assets::AssetRef::bare_name(&font.material).to_owned();
        let _ = hud_images.get(crate::images::HUD_CHROME_NAMESPACE, &material, &mut images);
        cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: (applied.x + 0.5).floor(),
            y: (applied.y + 0.5).floor(),
            w: applied.w,
            h: applied.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [1.0, 1.0, 1.0, LOWER_MESSAGE_ALPHA],
            material,
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: nscale,
                text,
                loc_key: key.to_owned(),

                style: crate::draw2d::TEXT_STYLE_UNREAD,
                fx: None,
            },
            provenance: Draw2dProvenance::CgDraw {
                site: "killcam_skip",
            },
            layer: 1,
        });
    }

    let list = Draw2dList { cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.killcam_skip = TessJob::Quads(quads);
}
