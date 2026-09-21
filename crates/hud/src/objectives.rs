use crate::draw2d::{
    Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, Draw2dQuad, tessellate_fonts,
};
use bevy::prelude::*;
use gamemode_iw4::{GameModeKind, Team};

pub(crate) fn draw(
    surface: &crate::surface::Hud2dSurface,
    catalog: &assets::MenuCatalog,
    strings: Option<&assets::LocalizeCatalog>,
    snapshot: &sim::Snapshot,
    local: sim::ClientId,
    camera: Option<(&Camera, &GlobalTransform)>,
) -> Vec<Draw2dQuad> {
    let mode = snapshot.meta.kind;
    if !mode.is_team() {
        return Vec::new();
    }
    let Some(font) = catalog.font("fonts/hudbigfont") else {
        return Vec::new();
    };
    let state = &snapshot.meta.objectives;
    let team = snapshot
        .meta
        .for_client(local)
        .and_then(|m| Team::from_retail_u8(m.client_state_team as u8))
        .unwrap_or(Team::Free);
    let mut list = Draw2dList::default();
    let mut graphics = Vec::new();
    let mut text = |label: String, x: f32, y: f32, scale: f32, color: [f32; 4], physical: bool| {
        let w = crate::chrome::ui_text_width(font, &label, scale);
        let ns = hud_iw4::r_normalized_text_scale(font.pixel_height, scale);
        let rect = surface.apply_rect(x - w * 0.5, y, ns, ns, 0, 0);
        list.cmds.push(Draw2dCmd {
            x: if physical {
                x - w * surface.height() / 480.0 * 0.5
            } else {
                rect.x
            },
            y: if physical { y } else { rect.y },
            w: rect.w,
            h: rect.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: assets::AssetRef::bare_name(&font.material).to_owned(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: "fonts/hudbigfont".to_owned(),
                scale: ns,
                text: label,
                loc_key: String::new(),
                style: crate::draw2d::TEXT_STYLE_HUDELEM,
                fx: None,
            },
            provenance: Draw2dProvenance::Objective,
            layer: 1,
        });
    };
    let white = [1.0; 4];
    if state.match_over || state.round_end_at_ms.is_some() {
        let title = match state.winner {
            Some(w) if w == team => "VICTORY",
            Some(_) => "DEFEAT",
            None => "DRAW",
        };
        text(title.to_owned(), 320.0, 194.0, 0.60, white, false);
        if !state.match_over {
            text(
                "SWITCHING SIDES".to_owned(),
                320.0,
                217.0,
                0.30,
                white,
                false,
            );
        }
    }
    let mut entries: Vec<(&sim::ObjectiveView, bool)> = Vec::new();
    if mode == GameModeKind::Domination {
        entries.extend(state.flags.iter().map(|flag| (flag, false)));
    } else {
        let attacking = state.attackers == team;

        if attacking
            && snapshot.meta.phase == sim::MatchPhase::Playing
            && snapshot
                .meta
                .for_client(local)
                .is_some_and(|m| m.lifecycle == sim::ClientLifecycle::Alive)
        {
            let rect = surface.apply_rect(-140.0, -115.0, 50.0, 50.0, 10, 10);
            graphics.push(quad(
                "hud_suitcase_bomb".into(),
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                [1.0, 1.0, 1.0, 0.75],
            ));
        }
        entries.extend(state.bombs.iter().map(|site| (&site.view, site.destroyed)));
    }
    for (view, destroyed) in &entries {
        let friendly = view.owner == team;

        let color = [1.0; 4];
        if !destroyed && let Some((camera, transform)) = camera {
            let point = Vec3::from_array(view.origin)
                + Vec3::Z
                    * if mode == GameModeKind::Domination {
                        100.0
                    } else {
                        64.0
                    };
            if let Ok(pos) = camera.world_to_viewport(transform, point) {
                let pos = Vec2::new(
                    pos.x.clamp(
                        65.0 * surface.height() / 480.0,
                        surface.width() - 65.0 * surface.height() / 480.0,
                    ),
                    pos.y.clamp(110.0, surface.height() - 70.0),
                );
                let shader = marker_material(mode, state, view, team);
                let size = 28.0 * surface.height() / 480.0;
                graphics.push(quad(
                    shader,
                    pos.x - size * 0.5,
                    pos.y - size,
                    size,
                    size,
                    color,
                ));
            }
        }
        if !destroyed
            && view.users.contains(&local)
            && snapshot.meta.phase == sim::MatchPhase::Playing
        {
            let active = if mode == GameModeKind::Domination {
                !friendly && view.capturing == team
            } else {
                state.bombs.iter().any(|b| {
                    b.view.id == view.id && b.user == Some(local) && b.hold.use_rate != 0.0
                })
            };
            if active {
                let hint = if mode == GameModeKind::Domination {
                    "MP_SECURING_POSITION"
                } else if state
                    .bombs
                    .iter()
                    .any(|b| b.view.id == view.id && b.planted_at_ms.is_some())
                {
                    "MP_DEFUSING_EXPLOSIVE"
                } else {
                    "MP_PLANTING_EXPLOSIVE"
                };
                if let Some(label) = strings.and_then(|s| s.text(hint)) {
                    text(
                        label.to_owned(),
                        320.0,
                        172.2,
                        hud_iw4::hudelem_text_scale(6, 0.6),
                        white,
                        false,
                    );
                }
                let bg = surface.apply_rect(258.0, 172.5, 124.0, 13.0, 0, 0);
                graphics.push(quad(
                    "progress_bar_bg".into(),
                    bg.x,
                    bg.y,
                    bg.w,
                    bg.h,
                    [0.0, 0.0, 0.0, 0.5],
                ));
                let width = (120.0 * view.progress.clamp(0.0, 1.0) + 0.5)
                    .floor()
                    .max(1.0);
                let bar = surface.apply_rect(260.0, 174.5, width, 9.0, 0, 0);
                graphics.push(quad(
                    "progress_bar_fill".into(),
                    bar.x,
                    bar.y,
                    bar.w,
                    bar.h,
                    white,
                ));
            }
        }
    }
    list.cmds.extend(graphics);
    let fonts = std::collections::HashMap::from([("fonts/hudbigfont".to_owned(), font)]);
    tessellate_fonts(&list, &fonts).0
}

fn quad(material: String, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Draw2dCmd {
    Draw2dCmd {
        x,
        y,
        w,
        h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material: material.into(),
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        op: Draw2dOp::StretchPic,
        provenance: Draw2dProvenance::Objective,
        layer: 1,
    }
}

pub(crate) fn marker_material(
    mode: GameModeKind,
    state: &sim::ObjectiveMatch,
    view: &sim::ObjectiveView,
    team: Team,
) -> String {
    let base = if mode == GameModeKind::Domination {
        if view.owner == team {
            "waypoint_defend"
        } else if view.owner == Team::Free {
            "waypoint_captureneutral"
        } else {
            "waypoint_capture"
        }
    } else {
        let planted = state
            .bombs
            .iter()
            .any(|b| b.view.id == view.id && b.planted_at_ms.is_some());
        match (team == state.attackers, planted) {
            (true, false) => "waypoint_target",
            (false, true) => "waypoint_defuse",
            _ => "waypoint_defend",
        }
    };
    format!("{base}_{}", view.label.to_ascii_lowercase())
}
