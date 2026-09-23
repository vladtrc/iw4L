use std::collections::HashMap;

use assets::{
    MapTeamSettings, MenuCatalog, MenuDef, PreparedLocalizedStrings, SessionTeamSettings,
};
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    ExprHost, Operand, ScorebarStatus, match_time_remaining_ms, scorebar_gametype_loc_key,
};
use net::{LocalPresentClient, PresentedSnapshot};

use crate::chrome::{ChromeAssets, ChromeFrame, execute_chrome_menu};
use crate::draw2d::{Draw2dOp, tessellate_fonts};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{GpuListLatch, HudTessPass, TessJob};
use crate::images::HudImages;
use crate::playercard::UiLocalVars;

#[derive(Component)]
pub(crate) struct ScorebarRaster;
pub(crate) fn spawn_scorebar(root: &mut ChildSpawnerCommands) {
    root.spawn((
        ScorebarRaster,
        GpuListLatch::default(),
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

pub(crate) fn sys_milliseconds() -> u32 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ORIGIN: OnceLock<Instant> = OnceLock::new();

    const UPTIME_BIAS_MS: u32 = 60_000;
    ORIGIN.get_or_init(Instant::now).elapsed().as_millis() as u32 + UPTIME_BIAS_MS
}

struct ScorebarExprHost<'a> {
    ms: i32,
    player_score: i32,
    rank1: i32,
    rank2: i32,
    time_left: i32,
    objectives: &'a sim::ObjectiveMatch,
    objective_time_ms: u32,
    time_limit_minutes: i32,
    localize: Option<&'a assets::LocalizeCatalog>,
    score_limit: i32,
    kind: gamemode_iw4::GameModeKind,
    client_state_team: i32,
    team_scores: [i32; 3],
    ffa_team: Option<u8>,
    menu: &'a MenuDef,
    icons: &'a MapTeamSettings,
    local_vars: &'a UiLocalVars,
}

fn ffa_ui_team(ffa_team: Option<u8>) -> &'static str {
    if ffa_team == Some(1) {
        "opfor"
    } else {
        "marines"
    }
}

impl ExprHost for ScorebarExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, hud_iw4::ExprError> {
        let name = self
            .menu
            .static_dvar_name(index)
            .ok_or(hud_iw4::ExprError::Host("scorebar static dvar name"))?;
        self.dvar_int(name)
    }
    fn dvar_int(&self, name: &str) -> Result<i32, hud_iw4::ExprError> {
        match name.to_ascii_lowercase().as_str() {
            "ui_scorelimit" => Ok(self.score_limit),
            "ui_timelimit" => Ok(self.time_limit_minutes),
            "ui_halftime" | "ui_overtime" | "splitscreen" => Ok(0),
            "ui_bomb_timer" => {
                let count = self
                    .objectives
                    .bombs
                    .iter()
                    .filter(|b| b.planted_at_ms.is_some() && !b.destroyed)
                    .count();
                Ok(if count == 0 { 0 } else { count as i32 + 1 })
            }
            "ui_bombtimer_a" | "ui_bombtimer_b" => {
                let label = if name.eq_ignore_ascii_case("ui_bombtimer_a") {
                    "A"
                } else {
                    "B"
                };
                Ok(self
                    .objectives
                    .bombs
                    .iter()
                    .find(|b| b.view.label == label && !b.destroyed)
                    .and_then(|b| b.planted_at_ms)
                    .map(|at| {
                        let elapsed = self.objective_time_ms.saturating_sub(at);
                        (gamemode_iw4::dd::BOMB_FUSE_MS
                            .saturating_sub(elapsed)
                            .div_ceil(1000) as i32
                            - 1)
                        .max(0)
                    })
                    .unwrap_or(-1))
            }
            _ => Err(hud_iw4::ExprError::Host("scorebar dvarint")),
        }
    }
    fn localize_string(&self, args: &[Operand]) -> Result<String, hud_iw4::ExprError> {
        let Some(Operand::Str(key)) = args.first() else {
            return Err(hud_iw4::ExprError::Host("locstring template"));
        };
        let template = self
            .localize
            .and_then(|l| l.text(key.trim_start_matches('@')))
            .ok_or(hud_iw4::ExprError::Host("locstring localization"))?;
        let values = args[1..]
            .iter()
            .map(|arg| match arg {
                Operand::Int(v) => Ok(v.to_string()),
                Operand::Float(v) => Ok(format!("{v}")),
                _ => Err(hud_iw4::ExprError::Host("locstring non-numeric insertion")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = String::new();
        let mut rest = template;
        while let Some(at) = rest.find("&&") {
            out.push_str(&rest[..at]);
            let tail = &rest[at + 2..];
            let digit = tail.as_bytes().first().copied();
            if let Some(digit @ b'1'..=b'9') = digit {
                let value = values
                    .get((digit - b'1') as usize)
                    .ok_or(hud_iw4::ExprError::Host("locstring missing insertion"))?;
                out.push_str(value);
                rest = &tail[1..];
            } else {
                out.push_str("&&");
                rest = tail;
            }
        }
        out.push_str(rest);
        Ok(out)
    }
    fn static_dvar_string(&self, index: i32) -> Result<String, hud_iw4::ExprError> {
        let name = self
            .menu
            .static_dvar_name(index)
            .ok_or(hud_iw4::ExprError::Host("static dvar string"))?;
        if name.eq_ignore_ascii_case("g_TeamIcon_Allies") {
            self.icons
                .allies
                .as_ref()
                .map(assets::AssetKey::display)
                .ok_or(hud_iw4::ExprError::Host("g_TeamIcon_Allies"))
        } else if name.eq_ignore_ascii_case("g_TeamIcon_Axis") {
            self.icons
                .axis
                .as_ref()
                .map(assets::AssetKey::display)
                .ok_or(hud_iw4::ExprError::Host("g_TeamIcon_Axis"))
        } else if name.eq_ignore_ascii_case("ui_danger_team") {
            Ok(String::new())
        } else {
            Err(hud_iw4::ExprError::Host("static dvar string"))
        }
    }
    fn team_field(&self, field: &str) -> Result<Operand, hud_iw4::ExprError> {
        if field.eq_ignore_ascii_case("name") {
            Ok(Operand::Str(
                entity_iw4::cg_get_team_name(self.client_state_team).to_owned(),
            ))
        } else if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(
                self.team_scores
                    .get(self.client_state_team as usize)
                    .copied()
                    .unwrap_or(0),
            ))
        } else {
            Err(hud_iw4::ExprError::Host("team field"))
        }
    }
    fn player_field(&self, field: &str) -> Result<Operand, hud_iw4::ExprError> {
        if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(self.player_score))
        } else {
            Err(hud_iw4::ExprError::Host("player field"))
        }
    }
    fn other_team_field(&self, field: &str) -> Result<Operand, hud_iw4::ExprError> {
        if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(
                self.team_scores[if self.client_state_team == 1 { 2 } else { 1 }],
            ))
        } else {
            Err(hud_iw4::ExprError::Host("other team field"))
        }
    }
    fn local_var_string(&self, name: &str) -> Result<Operand, hud_iw4::ExprError> {
        if name.eq_ignore_ascii_case("ui_team") {
            let hosted = self.local_vars.string(name);
            if !hosted.is_empty() {
                return Ok(Operand::Str(hosted));
            }
            return Ok(Operand::Str(ffa_ui_team(self.ffa_team).to_owned()));
        }
        Ok(Operand::Str(self.local_vars.string(name)))
    }
    fn time_left(&self) -> Result<i32, hud_iw4::ExprError> {
        Ok(self.time_left)
    }
    fn score_at_rank(&self, rank: i32) -> Result<i32, hud_iw4::ExprError> {
        match rank {
            1 => Ok(self.rank1),
            2 => Ok(self.rank2),
            _ => Err(hud_iw4::ExprError::Host("score rank")),
        }
    }
    fn gametype_name(&self) -> Result<Operand, hud_iw4::ExprError> {
        match scorebar_gametype_loc_key(self.kind.token()) {
            Some(key) => Ok(Operand::Str(key.to_owned())),
            None => Err(hud_iw4::ExprError::Host("gametype loc")),
        }
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, hud_iw4::ExprError> {
        Err(hud_iw4::ExprError::Host("weapon lock"))
    }
}

fn status_of_item(text_key: &str, text_exp: &str) -> Option<ScorebarStatus> {
    if text_key == "@MPUI_WINNING_CAPS" {
        Some(ScorebarStatus::Winning)
    } else if text_key == "@MPUI_LOSING_CAPS" {
        Some(ScorebarStatus::Losing)
    } else if text_key == "@MPUI_TIED_CAPS" {
        Some(ScorebarStatus::Tied)
    } else if text_exp.contains("op 85") {
        Some(ScorebarStatus::Gametype)
    } else {
        None
    }
}

fn eval_scorebar_status(
    menu: &MenuDef,
    host: &ScorebarExprHost,
    exprs: &mut crate::expr_cache::MenuExprCache,
) -> Result<Option<(ScorebarStatus, usize)>, (usize, String)> {
    let mut found = None;
    for (i, item) in menu.items.iter().enumerate() {
        let Some(status) = status_of_item(&item.text_key, &item.text_exp) else {
            continue;
        };
        match exprs.is_true(&item.vis_exp, host) {
            Ok(true) => found = Some((status, i)),
            Ok(false) => {}
            Err(err) => return Err((i, format!("{err:?}"))),
        }
    }
    Ok(found)
}

fn hide(pass: &mut HudTessPass) {
    pass.scorebar = TessJob::Hide;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_scorebar(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    session_icons: Option<Res<SessionTeamSettings>>,
    local_vars: Res<UiLocalVars>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
    view: Option<Res<frame::ViewSubject>>,
) {
    if !surface.is_ready() {
        return;
    }
    if view.is_some_and(|v| v.in_killcam()) {
        hide(&mut pass);
        return;
    }
    let Some(snap) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let Some(local_meta) = snap.meta.for_client(local.0) else {
        hide(&mut pass);
        return;
    };

    let mut others = [0i32; 18];
    let mut n_others = 0usize;
    for (id, meta) in &snap.meta.clients {
        if *id == local.0 {
            continue;
        }
        if n_others < others.len() {
            others[n_others] = meta.score;
            n_others += 1;
        }
    }
    let remaining_ms = if snap.meta.kind == gamemode_iw4::GameModeKind::Demolition {
        snap.meta.objectives.round_remaining_ms as i32
    } else {
        match_time_remaining_ms(snap.meta.time_limit_ms, snap.meta.match_elapsed_ms)
    };
    let remaining_s = remaining_ms.max(0) / 1000;
    let sys_ms = sys_milliseconds();

    let mut rank_scores = [0i32; 18];
    rank_scores[0] = local_meta.score;
    let mut n_ranks = 1usize;
    for s in others[..n_others].iter() {
        if n_ranks < rank_scores.len() {
            rank_scores[n_ranks] = *s;
            n_ranks += 1;
        }
    }
    rank_scores[..n_ranks].sort_unstable_by(|a, b| b.cmp(a));
    let rank1 = rank_scores[0];
    let rank2 = if n_ranks > 1 { rank_scores[1] } else { 0 };

    let Some(menu) = catalog.as_ref().and_then(|c| c.get("scorebar_hd")) else {
        gaps.raise(GapCause::ScorebarNoCatalog);
        hide(&mut pass);
        return;
    };
    let Some(teams) = session_icons.as_deref() else {
        hide(&mut pass);
        return;
    };
    let host = ScorebarExprHost {
        ms: sys_ms as i32,
        player_score: local_meta.score,
        rank1,
        rank2,
        time_left: remaining_s,
        objectives: &snap.meta.objectives,
        objective_time_ms: snap.tick.0.saturating_mul(sim::MATCH_TICK_MS),
        time_limit_minutes: (snap.meta.time_limit_ms / 60_000) as i32,
        localize: strings.as_ref().map(|s| &s.0),
        score_limit: snap.meta.score_limit,
        kind: snap.meta.kind,
        client_state_team: local_meta.client_state_team,
        team_scores: snap.meta.objectives.scores,
        ffa_team: local_meta.ffa_team,
        menu,
        icons: &teams.0,
        local_vars: &local_vars,
    };

    match eval_scorebar_status(menu, &host, &mut exprs) {
        Ok(_) => {
            gaps.clear(HudGap::MenuVisExp);
        }
        Err((item, err)) => {
            gaps.raise(GapCause::VisExpUneval {
                menu: "scorebar_hd".to_owned(),
                item,
                err: err.clone(),
            });
        }
    }

    let ChromeFrame {
        mut list,
        coverage: _,
        vis_errors,
    } = execute_chrome_menu(
        menu,
        &host,
        &surface,
        ChromeAssets {
            catalog: catalog.as_deref(),
            localize: strings.as_ref().map(|s| &s.0),
        },
        &mut exprs,
    );
    for (item, err) in vis_errors {
        gaps.raise(GapCause::MenuExpression {
            menu: menu.name.clone(),
            item,
            err,
        });
    }
    let mut fonts: HashMap<String, &assets::FontDef> = HashMap::new();
    for cmd in &mut list.cmds {
        if let Ok(mut key) = assets::AssetKey::parse(&cmd.material) {
            // IW4 scorebar expressions append `_fade` to faction icons. T5
            // supplies the base emblem only; translate that authored IW4 variant
            // to the selected T5 team's exact material, without probing sources.
            if key.namespace == assets::AssetNamespace::T5 {
                for icon in [teams.0.allies.as_ref(), teams.0.axis.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if key.namespace == icon.namespace
                        && key.name.strip_suffix("_fade") == Some(icon.name.as_str())
                    {
                        key = icon.clone();
                        break;
                    }
                }
            }
            cmd.material_namespace = key.namespace;
            cmd.material = key.name;
        }
        let _ = hud_images.get(cmd.material_namespace, &cmd.material, &mut images);
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
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.scorebar = TessJob::Quads(quads);
}
