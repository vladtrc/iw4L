use std::collections::HashMap;

use asset_game::MenuCatalog;
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;

use crate::chrome::text_width;
use crate::draw2d::{Draw2dCmd, Draw2dOp, Draw2dProvenance};

const ICON: &str = "objective_onscreen";
const ALPHA: f32 = 0.6;
const FONT_SCALE: f32 = 0.6;
const DETAIL_METRES: f32 = 10.0;
const METRES_PER_UNIT: f32 = 0.0254;
const ICON_SIZE: f32 = 24.0;
const EDGE_PAD: f32 = 24.0;

#[allow(clippy::too_many_arguments)]
pub(crate) fn marker_cmds<'a>(
    snapshot: &sim::Snapshot,
    surface: &crate::surface::Hud2dSurface,
    camera: (&Camera, &GlobalTransform),
    catalog: Option<&'a MenuCatalog>,
    strings: Option<&PreparedLocalizedStrings>,
    fonts: &mut HashMap<String, &'a asset_game::FontDef>,
) -> Vec<Draw2dCmd> {
    let objectives = &snapshot.meta.objectives;
    if objectives.list.is_empty() {
        return Vec::new();
    }
    let (camera, transform) = camera;
    let eye = transform.translation();
    let forward = *transform.forward();
    let scale = surface.scale_virtual_to_real()[1];
    let (w, h) = (surface.width(), surface.height());
    let font = catalog.and_then(|c| c.font(crate::font_overlay::HUD_SMALL_FONT));
    let mut cmds = Vec::new();
    for objective in objectives
        .compass
        .iter()
        .filter(|o| o.state == sim::ObjectiveState::Current)
    {
        let world = Vec3::from_array(objective.origin);
        let depth = (world - eye).dot(forward);
        let mirrored = if depth <= 0.0 {
            world - forward * (2.0 * depth - 1.0)
        } else {
            world
        };
        let Ok(pos) = camera.world_to_viewport(transform, mirrored) else {
            continue;
        };
        let onscreen = depth > 0.0 && (0.0..=w).contains(&pos.x) && (0.0..=h).contains(&pos.y);
        let at = if onscreen {
            pos
        } else {
            let center = Vec2::new(w, h) * 0.5;
            let half = (center - Vec2::splat(EDGE_PAD * scale)).max(Vec2::ONE);
            let dir = pos - center;
            let reach = (half.x / dir.x.abs().max(f32::EPSILON))
                .min(half.y / dir.y.abs().max(f32::EPSILON));
            center + dir * reach
        };
        let size = ICON_SIZE * scale;
        let icon = match objective.icon.as_str() {
            "" | "objective" => ICON,
            icon => icon,
        };
        cmds.push(Draw2dCmd {
            x: at.x - size * 0.5,
            y: at.y - size * 0.5,
            w: size,
            h: size,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [1.0, 1.0, 1.0, ALPHA],
            material: icon.to_owned(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::Objective,
            layer: 1,
        });
        let Some(font) = font else {
            continue;
        };
        let text = label(objective, world.distance(eye), strings);
        if text.is_empty() {
            continue;
        }
        let nscale = hud_iw4::normalized_text_scale(font.pixel_height, FONT_SCALE) * scale;
        let width = text_width(font, &text) as f32 * nscale;
        fonts.insert(crate::font_overlay::HUD_SMALL_FONT.to_owned(), font);
        cmds.push(Draw2dCmd {
            x: (at.x - width * 0.5).floor(),
            y: (at.y + size * 0.5 + font.pixel_height as f32 * nscale).floor(),
            w: nscale,
            h: nscale,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [1.0, 1.0, 1.0, ALPHA],
            material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: crate::font_overlay::HUD_SMALL_FONT.to_owned(),
                scale: nscale,
                text,
                loc_key: String::new(),
                style: crate::draw2d::TEXT_STYLE_HUDELEM,
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::Objective,
            layer: 1,
        });
    }
    cmds
}

fn label(
    objective: &sim::CompassObjective,
    distance: f32,
    strings: Option<&PreparedLocalizedStrings>,
) -> String {
    let localize = |key: &str| {
        strings
            .and_then(|strings| crate::hudelem::resolve_hud_text(strings, key))
            .unwrap_or_default()
    };
    if !objective.label.is_empty() {
        return localize(&objective.label);
    }
    if objective.follows {
        return localize("CGAME_OBJECTIVE_FOLLOWTEXT_DEFAULT");
    }
    let metres = distance * METRES_PER_UNIT;
    if metres < DETAIL_METRES {
        format!("{:.1}m", (metres * 10.0).round() * 0.1)
    } else {
        format!("{}m", metres.round() as i32)
    }
}
