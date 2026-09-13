use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings, SessionTeamIcons, TeamIcons};
use bevy::prelude::*;
use entity_iw4::client_state_name;
use frame::LaunchIdentity;
use gamemode_iw4::{ParsedScores, Score};
use hud_iw4::{
    ALIGN_VIEWABLE, ExprHost, Operand, match_time_remaining_ms, r_normalized_text_scale,
    scorebar_gametype_loc_key,
};
use net::{ClientActionInput, LocalPresentClient, PresentedSnapshot};
use sim::{ClientSnapshotMeta, Snapshot};

use crate::chrome::{ChromeAssets, execute_chrome_menu};
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::{hud_team_icons, sys_milliseconds};

const TEAM_SPECTATOR: i32 = 3;

const LIST_WIDTH: f32 = 500.0;

const ITEM_HEIGHT: f32 = 18.0;

const TEXT_SCALE: f32 = 0.35;

const NAME_FRAC: f32 = 0.35;

const NUM_FRAC: f32 = 0.1;

const ICON_SKIP_FRAC: f32 = 0.15;

#[derive(Clone, Debug)]
struct ScoreboardRow {
    score: Score,
    name: String,
}
#[derive(Component)]
pub(crate) struct ScoreboardRaster;

pub(crate) fn spawn_scoreboard(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, ScoreboardRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.scoreboard = TessJob::Hide;
}

struct ScoreboardExprHost {
    ms: i32,
    player_score: i32,
    time_left: i32,
    score_limit: i32,
    icons: TeamIcons,
    kind: gamemode_iw4::GameModeKind,
    client_state_team: i32,
    team_scores: [i32; 3],
}

impl ExprHost for ScoreboardExprHost {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, hud_iw4::ExprError> {
        match index {
            16 => Ok(self.score_limit),

            22 => Ok(0),
            17 | 18 | 19 | 21 | 26 | 37 | 38 => Ok(0),
            _ => Err(hud_iw4::ExprError::Host("static dvar")),
        }
    }
    fn static_dvar_string(&self, index: i32) -> Result<String, hud_iw4::ExprError> {
        match index {
            6 => self
                .icons
                .allies
                .clone()
                .ok_or(hud_iw4::ExprError::Host("g_TeamIcon_Allies")),
            7 => self
                .icons
                .axis
                .clone()
                .ok_or(hud_iw4::ExprError::Host("g_TeamIcon_Axis")),
            _ => Err(hud_iw4::ExprError::Host("static dvar string")),
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
    fn local_var_string(&self, _name: &str) -> Result<Operand, hud_iw4::ExprError> {
        Ok(Operand::Str(String::new()))
    }
    fn time_left(&self) -> Result<i32, hud_iw4::ExprError> {
        Ok(self.time_left)
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, hud_iw4::ExprError> {
        Ok(self.player_score)
    }
    fn gametype_name(&self) -> Result<Operand, hud_iw4::ExprError> {
        match scorebar_gametype_loc_key(self.kind.token()) {
            Some(key) => Ok(Operand::Str(key.to_owned())),
            None => Err(hud_iw4::ExprError::Host("gametype loc")),
        }
    }
    fn dvar_int(&self, name: &str) -> Result<i32, hud_iw4::ExprError> {
        if name.eq_ignore_ascii_case("splitscreen") || name.eq_ignore_ascii_case("ui_bomb_timer") {
            Ok(0)
        } else if name.eq_ignore_ascii_case("ui_scorelimit") {
            Ok(self.score_limit)
        } else {
            Err(hud_iw4::ExprError::Host("dvarint"))
        }
    }
}

fn client_score_is_better(a: &Score, b: &Score) -> bool {
    if a.team != b.team && (a.team == TEAM_SPECTATOR || b.team == TEAM_SPECTATOR) {
        return false;
    }
    if a.score > b.score {
        return true;
    }
    if a.score >= b.score {
        return a.deaths < b.deaths;
    }
    false
}

fn sort_scores(rows: &mut [ScoreboardRow]) {
    for i in 1..rows.len() {
        let mut j = i;
        while j > 0 && client_score_is_better(&rows[j].score, &rows[j - 1].score) {
            rows.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn name_for(meta: &ClientSnapshotMeta) -> String {
    match client_state_name(&meta.name) {
        Some(n) => n.to_owned(),
        None => String::new(),
    }
}

fn rows_from_parsed(snap: &Snapshot, parsed: &ParsedScores) -> Vec<ScoreboardRow> {
    let mut rows = Vec::with_capacity(parsed.num);
    for i in 0..parsed.num {
        let score = parsed.scores[i];
        let name = match snap
            .meta
            .clients
            .iter()
            .find(|(id, _)| id.0 as i32 == score.client)
        {
            Some((_, meta)) => name_for(meta),
            None => String::new(),
        };
        rows.push(ScoreboardRow { score, name });
    }
    sort_scores(&mut rows);
    rows
}

fn loc_text(strings: Option<&PreparedLocalizedStrings>, key: &str) -> Option<String> {
    strings.and_then(|s| s.0.text(key)).map(str::to_owned)
}

fn text_cmd(
    x: f32,
    y: f32,
    cmd_w: f32,
    cmd_h: f32,
    material: String,
    text: String,
    color: [f32; 4],
) -> Draw2dCmd {
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (x + 0.5).floor(),
        y: (y + 0.5).floor(),
        w: cmd_w,
        h: cmd_h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material,
        op: Draw2dOp::TextRun {
            font: HUD_SMALL_FONT.to_owned(),
            scale: cmd_w,
            text,
            loc_key: String::new(),

            style: crate::draw2d::TEXT_STYLE_UNREAD,
            fx: None,
        },
        provenance: Draw2dProvenance::CgDraw {
            site: "client_score",
        },
        layer: 2,
    }
}

fn push_cell(
    cmds: &mut Vec<Draw2dCmd>,
    surface: &crate::surface::Hud2dSurface,
    x_virtual: f32,
    y_virtual: f32,
    nscale: f32,
    material: &str,
    text: &str,
    color: [f32; 4],
) {
    if text.is_empty() {
        return;
    }
    let applied = surface.apply_rect(
        x_virtual,
        y_virtual,
        nscale,
        nscale,
        ALIGN_VIEWABLE,
        ALIGN_VIEWABLE,
    );
    cmds.push(text_cmd(
        applied.x,
        applied.y,
        applied.w,
        applied.h,
        material.to_owned(),
        text.to_owned(),
        color,
    ));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_scoreboard(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    actions: Option<Res<ClientActionInput>>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    identity: Option<Res<LaunchIdentity>>,
    session_icons: Option<Res<SessionTeamIcons>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
) {
    let down = actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
    if !down {
        hide(&mut pass);
        return;
    }
    let Some(snap) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let parsed = net::parse_scoreboard_cmd(&net::format_scoreboard_from_snapshot(snap));
    let rows = rows_from_parsed(snap, &parsed);
    if rows.is_empty() {
        hide(&mut pass);
        return;
    }

    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let font = catalog.as_ref().and_then(|c| c.font(HUD_SMALL_FONT));
    let Some(def) = font else {
        hide(&mut pass);
        return;
    };
    let nscale = r_normalized_text_scale(def.pixel_height, TEXT_SCALE);
    let material = assets::AssetRef::bare_name(&def.material).to_owned();
    let _ = hud_images.get(crate::images::HUD_CHROME_NAMESPACE, &material, &mut images);
    let mut fonts = HashMap::new();
    fonts.insert(HUD_SMALL_FONT.to_owned(), def);

    let loc = strings.as_deref();
    let mut cmds = Vec::new();
    let list_x = (640.0 - LIST_WIDTH) * 0.5;
    let header_y = 64.0;
    let name_x = list_x + LIST_WIDTH * ICON_SKIP_FRAC;
    let score_x = name_x + LIST_WIDTH * NAME_FRAC + LIST_WIDTH * 0.05;
    let kills_x = score_x + LIST_WIDTH * NUM_FRAC;
    let deaths_x = kills_x + LIST_WIDTH * NUM_FRAC * 2.0;
    let ping_x = deaths_x + LIST_WIDTH * NUM_FRAC;

    let header = [1.0, 1.0, 1.0, 1.0];

    let mine = [1.0, 0.8, 0.4, 1.0];
    if let Some(text) = loc_text(loc, "CGAME_SB_SCORE") {
        push_cell(
            &mut cmds, &surface, score_x, header_y, nscale, &material, &text, header,
        );
    }
    if let Some(text) = loc_text(loc, "CGAME_SB_KILLS") {
        push_cell(
            &mut cmds, &surface, kills_x, header_y, nscale, &material, &text, header,
        );
    }
    if let Some(text) = loc_text(loc, "CGAME_SB_DEATHS") {
        push_cell(
            &mut cmds, &surface, deaths_x, header_y, nscale, &material, &text, header,
        );
    }
    if let Some(text) = loc_text(loc, "CGAME_SB_PING") {
        push_cell(
            &mut cmds, &surface, ping_x, header_y, nscale, &material, &text, header,
        );
    }

    for (i, row) in rows.iter().enumerate() {
        let y = header_y + ITEM_HEIGHT + ITEM_HEIGHT * (i as f32);
        let color = if row.score.client == local.0.0 as i32 {
            mine
        } else {
            header
        };
        push_cell(
            &mut cmds, &surface, name_x, y, nscale, &material, &row.name, color,
        );
        if row.score.team != TEAM_SPECTATOR {
            push_cell(
                &mut cmds,
                &surface,
                score_x,
                y,
                nscale,
                &material,
                &format!("{}", row.score.score),
                color,
            );
            push_cell(
                &mut cmds,
                &surface,
                kills_x,
                y,
                nscale,
                &material,
                &format!("{}", row.score.kills),
                color,
            );
            push_cell(
                &mut cmds,
                &surface,
                deaths_x,
                y,
                nscale,
                &material,
                &format!("{}", row.score.deaths),
                color,
            );
        }
        push_cell(
            &mut cmds, &surface, ping_x, y, nscale, &material, "—", color,
        );
    }

    let mut chrome_cmds = Vec::new();
    if let Some(menu) = catalog.as_ref().and_then(|c| c.get("scoreboard"))
        && let Some(local_meta) = snap.meta.for_client(local.0)
    {
        let remaining_ms =
            match_time_remaining_ms(snap.meta.time_limit_ms, snap.meta.match_elapsed_ms);
        let remaining_s = remaining_ms.max(0) / 1000;
        let player_score = local_meta.score;
        let host = ScoreboardExprHost {
            ms: sys_milliseconds() as i32,
            player_score,
            time_left: remaining_s,
            score_limit: snap.meta.score_limit,
            icons: hud_team_icons(
                catalog.as_deref(),
                identity.as_deref(),
                session_icons.as_deref(),
            ),
            kind: snap.meta.kind,
            client_state_team: local_meta.client_state_team,
            team_scores: snap.meta.objectives.scores,
        };
        let frame = execute_chrome_menu(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: catalog.as_deref(),
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
        if let Some(cat) = catalog.as_deref() {
            for cmd in &frame.list.cmds {
                let Draw2dOp::TextRun { font, .. } = &cmd.op else {
                    continue;
                };
                if fonts.contains_key(font) {
                    continue;
                }
                if let Some(def) = cat.font(font) {
                    fonts.insert(font.clone(), def);
                }
            }
        }
        chrome_cmds = frame.list.cmds;
    }
    chrome_cmds.extend(cmds);
    let list = Draw2dList { cmds: chrome_cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.scoreboard = TessJob::Quads(quads);
}
