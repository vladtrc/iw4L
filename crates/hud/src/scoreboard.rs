use std::collections::HashMap;

use assets::{
    FontDef, MapTeamSettings, MenuCatalog, PreparedLocalizedStrings, SessionTeamSettings,
};
use bevy::prelude::*;
use entity_iw4::client_state_name;
use frame::LaunchIdentity;
use gamemode_iw4::{ParsedScores, Score};
use hud_iw4::{
    ALIGN_CENTER, match_time_remaining_ms, r_normalized_text_scale, scorebar_gametype_loc_key,
};
use net::{
    CgScores, ClientActionInput, LocalPresentClient, MasterBridge, MasterBridgeState,
    PresentedSnapshot,
};
use sim::{ClientLifecycle, MatchPhase, Snapshot};

use crate::chrome::ui_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::{HUD_CHROME_NAMESPACE, HudImages};
use crate::overhead_names::rank_presentation;
use crate::surface::Hud2dSurface;

const LIST_X: f32 = 70.0;
const LIST_WIDTH: f32 = 500.0;
const WHITE: [f32; 4] = [1.0; 4];
const MINE: [f32; 4] = [1.0, 0.8, 0.4, 1.0];
const COLUMNS: [(f32, f32, &str); 5] = [
    (248.0, 49.0, "CGAME_SB_SCORE"),
    (297.0, 44.0, "CGAME_SB_KILLS"),
    (341.0, 48.0, "CGAME_SB_ASSISTS"),
    (389.0, 47.0, "CGAME_SB_DEATHS"),
    (436.0, 40.0, "CGAME_SB_PING"),
];

#[derive(Clone, Debug)]
struct ScoreboardRow {
    score: Score,
    name: String,
    prestige: i32,
    dead: bool,
}

#[derive(Component)]
pub(crate) struct ScoreboardRaster;

pub(crate) fn spawn_scoreboard(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, ScoreboardRaster);
}

fn rows_from_parsed(snap: &Snapshot, parsed: &ParsedScores) -> Vec<ScoreboardRow> {
    let mut rows = Vec::with_capacity(parsed.num);
    for entry in parsed.scores.iter().take(parsed.num) {
        let Some((_, meta)) = snap
            .meta
            .clients
            .iter()
            .find(|(id, _)| id.0 as i32 == entry.client)
        else {
            continue;
        };
        let mut score = *entry;
        score.score = meta.score;
        score.kills = meta.kills;
        score.deaths = meta.deaths;
        score.team = meta.client_state_team;
        score.rank = meta.rank;
        rows.push(ScoreboardRow {
            score,
            name: client_state_name(&meta.name).unwrap_or_default().to_owned(),
            prestige: meta.prestige,
            dead: matches!(
                meta.lifecycle,
                ClientLifecycle::Dead | ClientLifecycle::RespawnPending
            ),
        });
    }
    rows.sort_by(|a, b| {
        b.score
            .score
            .cmp(&a.score.score)
            .then(a.score.deaths.cmp(&b.score.deaths))
    });
    rows
}

fn localized(strings: Option<&PreparedLocalizedStrings>, key: &str) -> String {
    let key = key.trim_start_matches('@');
    strings
        .and_then(|s| s.0.text(key))
        .unwrap_or(key)
        .to_owned()
}

struct BoardDraw<'a> {
    surface: &'a Hud2dSurface,
    font: &'a FontDef,
    cmds: Vec<Draw2dCmd>,
}

impl BoardDraw<'_> {
    fn picture(&mut self, x: f32, y: f32, w: f32, h: f32, material: &str, color: [f32; 4]) {
        let r = self
            .surface
            .apply_rect(x - 320.0, y - 240.0, w, h, ALIGN_CENTER, ALIGN_CENTER);
        self.cmds.push(Draw2dCmd {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: material.to_owned(),
            material_namespace: HUD_CHROME_NAMESPACE,
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
            layer: 1,
        });
    }

    fn text(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        scale: f32,
        centered: bool,
        text: &str,
        color: [f32; 4],
    ) {
        let scale = if centered {
            scale * (width / ui_text_width(self.font, text, scale).max(1.0)).min(1.0)
        } else {
            scale
        };
        // Keep color escapes intact while fitting names into their column.
        let mut text = text.to_owned();
        while !text.is_empty() && ui_text_width(self.font, &text, scale) > width {
            text.pop();
        }
        if text.ends_with('^') {
            text.pop();
        }
        if text.is_empty() {
            return;
        }
        let x = if centered {
            x + (width - ui_text_width(self.font, &text, scale)) * 0.5
        } else {
            x
        };
        let nscale = r_normalized_text_scale(self.font.pixel_height, scale);
        let r = self.surface.apply_rect(
            x - 320.0,
            y - 240.0,
            nscale,
            nscale,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        self.cmds.push(Draw2dCmd {
            x: r.x.round(),
            y: r.y.round(),
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: assets::AssetRef::bare_name(&self.font.material).to_owned(),
            material_namespace: HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: HUD_SMALL_FONT.to_owned(),
                scale: r.w,
                text,
                loc_key: String::new(),
                style: 3,
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
            layer: 2,
        });
    }

    fn ping(&mut self, x: f32, y: f32, h: f32, ping: i32) {
        if ping < 0 {
            return;
        }
        let bars = (4 - ping / 100).clamp(1, 4);
        let low = [0.0, 0.75, 0.0, 1.0];
        let med = [0.8, 0.8, 0.0, 1.0];
        let high = [0.8, 0.0, 0.0, 1.0];
        let (a, b, t) = if bars < 2 {
            (high, med, bars as f32 / 2.0)
        } else {
            (med, low, (bars - 2) as f32 / 2.0)
        };
        let color = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        self.picture(x, y, 20.0, h, "white", [0.25, 0.25, 0.25, 0.5]);
        for i in 1..=bars {
            let bh = h * 0.7 * i as f32 / 4.0;
            self.picture(
                x + 2.0 + (i - 1) as f32 * 4.0,
                y + h - bh,
                3.0,
                bh,
                "white",
                color,
            );
        }
    }
}

fn team_presentation(
    icons: &MapTeamSettings,
    team: i32,
    strings: Option<&PreparedLocalizedStrings>,
) -> (String, Option<assets::AssetKey>, [f32; 4]) {
    let icon = match team {
        1 => icons.axis.clone(),
        2 => icons.allies.clone(),
        _ => None,
    };
    let name = match team {
        1 => icons.axis_name.as_ref(),
        2 => icons.allies_name.as_ref(),
        _ => None,
    };
    let color = match team {
        1 => icons.axis_color,
        2 => icons.allies_color,
        _ => None,
    }
    .map(|rgb| [rgb[0], rgb[1], rgb[2], 0.5])
    .unwrap_or([0.25, 0.25, 0.25, 0.5]);
    if let Some(name) = name {
        return (
            strings
                .and_then(|s| s.0.text_asset(name))
                .unwrap_or(&name.name)
                .to_owned(),
            icon,
            color,
        );
    }
    let (key, color) = match team {
        0 => ("", [0.76, 0.78, 0.10, 0.5]),
        1 => ("MPUI_AXIS", [0.25, 0.25, 0.25, 0.5]),
        2 => ("MPUI_ALLIES", [0.25, 0.25, 0.25, 0.5]),
        _ => ("CGAME_SPECTATORS", [0.25, 0.25, 0.25, 0.5]),
    };
    let name = if key.is_empty() {
        String::new()
    } else {
        localized(strings, key)
    };
    (name, icon, color)
}

// Only intermission opens the scoreboard; the ended phase alone would cover the final killcam.
pub(crate) fn displayed(down: bool, snap: &Snapshot, local: sim::ClientId) -> bool {
    down || snap.meta.phase == MatchPhase::PostGame
        || snap
            .meta
            .for_client(local)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Intermission)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_scoreboard(
    surface: Res<Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    actions: Option<Res<ClientActionInput>>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    identity: Option<Res<LaunchIdentity>>,
    session_icons: Option<Res<SessionTeamSettings>>,
    scores: Option<Res<CgScores>>,
    bridge: Option<Res<MasterBridge>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut pass: ResMut<HudTessPass>,
) {
    pass.scoreboard = TessJob::Hide;
    let Some(snap) = presented.snapshot() else {
        return;
    };
    let down = actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
    if !displayed(down, snap, local.0) || !surface.is_ready() {
        return;
    }
    let Some(catalog) = catalog.as_deref() else {
        return;
    };
    let Some(font) = catalog.font(HUD_SMALL_FONT) else {
        return;
    };
    let fallback = net::parse_scoreboard_cmd(&net::format_scoreboard_from_snapshot(snap));
    let parsed = scores
        .as_ref()
        .filter(|s| s.cmd.is_some())
        .map_or(&fallback, |s| &s.parsed);
    let rows = rows_from_parsed(snap, parsed);
    if rows.is_empty() {
        return;
    }
    let loc = strings.as_deref();
    let Some(teams) = session_icons.as_deref() else {
        return;
    };
    let icons = &teams.0;
    let local_team = snap
        .meta
        .for_client(local.0)
        .map_or(0, |m| m.client_state_team);
    let order = if local_team == 2 {
        [2, 1, 0, 3]
    } else {
        [1, 2, 0, 3]
    };
    let teams: Vec<_> = order
        .into_iter()
        .filter(|team| rows.iter().any(|r| r.score.team == *team))
        .collect();
    // Fit the entire roster, including team banners, above the server footer.
    let available = 350.0 - teams.len() as f32 * 34.0;
    let row_step = (available / rows.len() as f32).min(20.0);
    let row_h = row_step - 2.0;
    let scale = 0.35 * (row_h / 18.0).min(1.0);
    let mut draw = BoardDraw {
        surface: &surface,
        font,
        cmds: Vec::new(),
    };

    draw.picture(0.0, 24.0, 640.0, 25.0, "white", [0.1, 0.1, 0.1, 0.35]);
    if snap.meta.kind.is_team() {
        for (team, x) in [(2, 32.0), (1, 127.0)] {
            let (_, icon, _) = team_presentation(icons, team, loc);
            if let Some(icon) = icon {
                draw.picture(x, 20.0, 30.0, 30.0, &icon.name, WHITE);
                draw.cmds.last_mut().unwrap().material_namespace = icon.namespace;
            }
            draw.text(
                x + 32.0,
                41.0,
                60.0,
                0.35,
                false,
                &snap.meta.objectives.scores[team as usize].to_string(),
                WHITE,
            );
        }
    }
    let key = scorebar_gametype_loc_key(snap.meta.kind.token()).unwrap_or("MPUI_DD");
    let title = localized(loc, key);
    draw.text(226.0, 41.0, 295.0, 0.35, true, &title, WHITE);
    if snap.meta.score_limit > 0 {
        let score = if snap.meta.kind.is_team() {
            snap.meta
                .objectives
                .scores
                .get(local_team as usize)
                .copied()
                .unwrap_or(0)
        } else {
            snap.meta.for_client(local.0).map_or(0, |m| m.score)
        };
        draw.text(
            226.0,
            58.0,
            295.0,
            0.28,
            true,
            &format!("{score} / {}", snap.meta.score_limit),
            WHITE,
        );
    }
    let remaining_ms = if snap.meta.kind == gamemode_iw4::GameModeKind::Demolition {
        snap.meta.objectives.round_remaining_ms as i32
    } else {
        match_time_remaining_ms(snap.meta.time_limit_ms, snap.meta.match_elapsed_ms)
    };
    let remaining = remaining_ms.max(0) / 1000;
    draw.text(
        558.0,
        41.0,
        60.0,
        0.35,
        true,
        &format!("{}:{:02}", remaining / 60, remaining % 60),
        WHITE,
    );

    for (x, w, key) in COLUMNS {
        draw.text(
            LIST_X + x,
            77.0,
            w,
            scale,
            true,
            &localized(loc, key),
            WHITE,
        );
    }
    let mut y = 82.0;
    for team in teams {
        let (name, icon, back) = team_presentation(icons, team, loc);
        let count = rows.iter().filter(|r| r.score.team == team).count();
        if let Some(icon) = icon {
            draw.picture(LIST_X, y, 28.0, 28.0, &icon.name, WHITE);
            draw.cmds.last_mut().unwrap().material_namespace = icon.namespace;
        }
        draw.text(
            LIST_X + 34.0,
            y + 24.0,
            260.0,
            0.35,
            false,
            &if name.is_empty() {
                format!("( {count} )")
            } else {
                format!("{name}  ( {count} )")
            },
            WHITE,
        );
        y += 30.0;
        for row in rows.iter().filter(|r| r.score.team == team) {
            let color = if row.score.client == local.0.0 as i32 {
                MINE
            } else {
                WHITE
            };
            draw.picture(LIST_X, y, LIST_WIDTH - 24.0, row_h, "white", back);
            if let Some((icon, level)) = rank_presentation(catalog, row.score.rank, row.prestige) {
                draw.picture(LIST_X, y, row_h, row_h, icon, WHITE);
                draw.text(
                    LIST_X + row_h + 1.0,
                    y + row_h * 0.8,
                    22.0,
                    scale * 0.25 / 0.35,
                    false,
                    level,
                    WHITE,
                );
            }
            draw.text(
                LIST_X + 44.0,
                y + row_h * 0.8,
                182.0,
                scale,
                false,
                &row.name,
                color,
            );
            if row.dead {
                draw.picture(LIST_X + 228.0, y, row_h, row_h, "hud_status_dead", WHITE);
            }
            let values = [
                row.score.score,
                row.score.kills,
                row.score.assists,
                row.score.deaths,
                row.score.ping,
            ];
            for ((x, w, _), value) in COLUMNS.into_iter().zip(values) {
                if team == 3 && x < 436.0 {
                    continue;
                }
                let value = if x == 436.0 && value < 0 {
                    String::new()
                } else {
                    value.to_string()
                };
                draw.text(LIST_X + x, y + row_h * 0.8, w, scale, true, &value, color);
            }
            draw.ping(LIST_X + 480.0, y, row_h, row.score.ping);
            y += row_step;
        }
        y += 4.0;
    }
    let server = bridge
        .as_ref()
        .and_then(|b| match b.state() {
            MasterBridgeState::Hosting { name, .. } | MasterBridgeState::Joined { name, .. } => {
                Some(name)
            }
            _ => None,
        })
        .unwrap_or_else(|| "IW4L".to_owned());
    draw.text(LIST_X, 455.0, 365.0, 0.3, false, &server, WHITE);
    if let Some(identity) = identity.as_ref() {
        draw.text(
            LIST_X + 370.0,
            455.0,
            130.0,
            0.3,
            false,
            &identity.zone,
            WHITE,
        );
    }
    for cmd in &draw.cmds {
        let _ = hud_images.get(cmd.material_namespace, &cmd.material, &mut images);
    }
    let list = Draw2dList { cmds: draw.cmds };
    let fonts = HashMap::from([(HUD_SMALL_FONT.to_owned(), font)]);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.scoreboard = TessJob::Quads(quads);
    }
}
