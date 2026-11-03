use std::collections::HashMap;

use asset_game::{FontDef, MapTeamSettings, MenuCatalog, SessionTeamSettings};
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use entity_iw4::client_state_name;
use frame::LaunchIdentity;
use gamemode_iw4::{ParsedScores, Score};
use hud_iw4::{ALIGN_CENTER, normalized_text_scale};
use net::{ClientActionInput, MasterBridge, MasterBridgeState, Scoreboard};
use sim::{ClientLifecycle, MatchPhase, Snapshot};

use crate::chrome::ui_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::{HUD_CHROME_NAMESPACE, HudImageLoader};
use crate::overhead_names::rank_presentation;
use crate::surface::Hud2dSurface;

const LIST_X: f32 = 70.0;
const LIST_WIDTH: f32 = 500.0;
const WHITE: [f32; 4] = [1.0; 4];
const MINE: [f32; 4] = [1.0, 0.8, 0.4, 1.0];
const COLUMNS: [(BoardTextColumn, &str); 5] = [
    (
        BoardTextColumn {
            x: LIST_X + 248.0,
            width: 49.0,
            centered: true,
        },
        "CGAME_SB_SCORE",
    ),
    (
        BoardTextColumn {
            x: LIST_X + 297.0,
            width: 44.0,
            centered: true,
        },
        "CGAME_SB_KILLS",
    ),
    (
        BoardTextColumn {
            x: LIST_X + 341.0,
            width: 48.0,
            centered: true,
        },
        "CGAME_SB_ASSISTS",
    ),
    (
        BoardTextColumn {
            x: LIST_X + 389.0,
            width: 47.0,
            centered: true,
        },
        "CGAME_SB_DEATHS",
    ),
    (
        BoardTextColumn {
            x: LIST_X + 436.0,
            width: 40.0,
            centered: true,
        },
        "CGAME_SB_PING",
    ),
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
            name: client_state_name(&meta.name).unwrap_or("").to_owned(),
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

#[derive(Clone, Copy)]
struct BoardTextColumn {
    x: f32,
    width: f32,
    centered: bool,
}

impl BoardTextColumn {
    fn draw(self, draw: &mut BoardDraw<'_>, y: f32, scale: f32, text: &str, color: [f32; 4]) {
        let Self { x, width, centered } = self;

        let scale = if centered {
            scale * (width / ui_text_width(draw.font, text, scale).max(1.0)).min(1.0)
        } else {
            scale
        };
        // Keep color escapes intact while fitting names into their column.
        let mut text = text.to_owned();
        while !text.is_empty() && ui_text_width(draw.font, &text, scale) > width {
            text.pop();
        }
        if text.ends_with('^') {
            text.pop();
        }
        if text.is_empty() {
            return;
        }
        let x = if centered {
            x + (width - ui_text_width(draw.font, &text, scale)) * 0.5
        } else {
            x
        };
        let nscale = normalized_text_scale(draw.font.pixel_height, scale);
        let r = draw.surface.apply_rect(
            x - 320.0,
            y - 240.0,
            nscale,
            nscale,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        draw.cmds.push(Draw2dCmd {
            x: r.x.round(),
            y: r.y.round(),
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: asset_core::AssetRef::bare_name(&draw.font.material).to_owned(),
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
) -> (String, Option<asset_core::AssetKey>, [f32; 4]) {
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScoreboardRenderer<'w> {
    catalog: Option<Res<'w, MenuCatalog>>,
    strings: Option<Res<'w, PreparedLocalizedStrings>>,
    session_icons: Option<Res<'w, SessionTeamSettings>>,
    hud_images: HudImageLoader<'w>,
    pass: ResMut<'w, HudTessPass>,
}

impl ScoreboardRenderer<'_> {
    fn update(
        &mut self,
        surface: Res<Hud2dSurface>,
        presentation: net::LocalPresentation,
        actions: Option<Res<ClientActionInput>>,
        scores: Option<Res<Scoreboard>>,
        identity: Option<Res<LaunchIdentity>>,
        bridge: Option<Res<MasterBridge>>,
    ) {
        self.pass.scoreboard = TessJob::Hide;
        let Some(snap) = presentation.snapshot() else {
            return;
        };
        let down = actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
        if !displayed(down, snap, presentation.client_id()) || !surface.is_ready() {
            return;
        }
        let Some(catalog) = self.catalog.as_deref() else {
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
        let loc = self.strings.as_deref();
        let Some(teams) = self.session_icons.as_deref() else {
            return;
        };
        let icons = &teams.0;
        let local_team = snap
            .meta
            .for_client(presentation.client_id())
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

        for (column, key) in COLUMNS {
            column.draw(&mut draw, 77.0, scale, &localized(loc, key), WHITE);
        }
        let mut y = 82.0;
        for team in teams {
            let (name, icon, back) = team_presentation(icons, team, loc);
            let count = rows.iter().filter(|r| r.score.team == team).count();
            if let Some(icon) = icon {
                draw.picture(LIST_X, y, 28.0, 28.0, &icon.name, WHITE);
                draw.cmds.last_mut().unwrap().material_namespace = icon.namespace;
            }
            BoardTextColumn {
                x: LIST_X + 34.0,
                width: 260.0,
                centered: false,
            }
            .draw(
                &mut draw,
                y + 24.0,
                0.35,
                &if name.is_empty() {
                    format!("( {count} )")
                } else {
                    format!("{name}  ( {count} )")
                },
                WHITE,
            );
            y += 30.0;
            for row in rows.iter().filter(|r| r.score.team == team) {
                let color = if row.score.client == presentation.client_id().0 as i32 {
                    MINE
                } else {
                    WHITE
                };
                draw.picture(LIST_X, y, LIST_WIDTH - 24.0, row_h, "white", back);
                if let Some((icon, level)) =
                    rank_presentation(catalog, row.score.rank, row.prestige)
                {
                    draw.picture(LIST_X, y, row_h, row_h, icon, WHITE);
                    BoardTextColumn {
                        x: LIST_X + row_h + 1.0,
                        width: 22.0,
                        centered: false,
                    }
                    .draw(
                        &mut draw,
                        y + row_h * 0.8,
                        scale * 0.25 / 0.35,
                        level,
                        WHITE,
                    );
                }
                BoardTextColumn {
                    x: LIST_X + 44.0,
                    width: 182.0,
                    centered: false,
                }
                .draw(&mut draw, y + row_h * 0.8, scale, &row.name, color);
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
                for ((column, _), value) in COLUMNS.into_iter().zip(values) {
                    if team == 3 && column.x < LIST_X + 436.0 {
                        continue;
                    }
                    let value = if column.x == LIST_X + 436.0 && value < 0 {
                        String::new()
                    } else {
                        value.to_string()
                    };
                    column.draw(&mut draw, y + row_h * 0.8, scale, &value, color);
                }
                draw.ping(LIST_X + 480.0, y, row_h, row.score.ping);
                y += row_step;
            }
            y += 4.0;
        }
        let server = bridge
            .as_ref()
            .and_then(|b| match b.state() {
                MasterBridgeState::Hosting { name, .. }
                | MasterBridgeState::Joined { name, .. } => Some(name),
                _ => None,
            })
            .unwrap_or_else(|| "IW4L".to_owned());
        BoardTextColumn {
            x: LIST_X,
            width: 365.0,
            centered: false,
        }
        .draw(&mut draw, 455.0, 0.3, &server, WHITE);
        if let Some(identity) = identity.as_ref() {
            BoardTextColumn {
                x: LIST_X + 370.0,
                width: 130.0,
                centered: false,
            }
            .draw(&mut draw, 455.0, 0.3, &identity.zone, WHITE);
        }
        for cmd in &draw.cmds {
            let _ = self.hud_images.load(cmd.material_namespace, &cmd.material);
        }
        let list = Draw2dList { cmds: draw.cmds };
        let fonts = HashMap::from([(HUD_SMALL_FONT.to_owned(), font)]);
        let (quads, _) = tessellate_fonts(&list, &fonts);
        if !quads.is_empty() {
            self.pass.scoreboard = TessJob::Quads(quads);
        }
    }
}

pub(crate) fn update_scoreboard(
    surface: Res<Hud2dSurface>,
    presentation: net::LocalPresentation,
    actions: Option<Res<ClientActionInput>>,
    scores: Option<Res<Scoreboard>>,
    identity: Option<Res<LaunchIdentity>>,
    bridge: Option<Res<MasterBridge>>,
    mut renderer: ScoreboardRenderer,
) {
    renderer.update(surface, presentation, actions, scores, identity, bridge);
}
