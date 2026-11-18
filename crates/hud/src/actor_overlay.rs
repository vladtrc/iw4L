use crate::draw2d::{
    Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, TextRunGlow, tessellate_fonts,
};
use crate::gpu_list::TessJob;
use crate::surface::Hud2dSurface;
use asset_game::{FontDef, MenuCatalog};
use sim::{ActorOverlay, ScriptDvars};
use std::collections::HashMap;

fn color(dvars: ScriptDvars<'_>, key: &str, default: [f32; 4]) -> [f32; 4] {
    let Some(raw) = dvars.string(key) else {
        return default;
    };
    let parsed: Option<Vec<f32>> = raw.split_whitespace().map(|v| v.parse().ok()).collect();
    parsed
        .and_then(|v| <[f32; 4]>::try_from(v).ok())
        .filter(|v| v.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
        .unwrap_or(default)
}

pub(crate) fn draw(
    overlay: &ActorOverlay,
    dvars: ScriptDvars<'_>,
    surface: &Hud2dSurface,
    catalog: &MenuCatalog,
    strings: Option<&assets::PreparedLocalizedStrings>,
) -> TessJob {
    if overlay.name.is_empty() || overlay.name.eq_ignore_ascii_case("none") {
        return TessJob::Hide;
    }
    let text_scale = dvars
        .float("friendlynamefontsize")
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        .unwrap_or(0.3);
    let font_enum = if dvars.int("friendlynamefontobjective").unwrap_or(1) != 0 {
        6
    } else {
        0
    };
    let font_name =
        hud_iw4::ui_get_font_handle(font_enum, surface.scale_virtual_to_real()[1], text_scale);
    let Some(font) = catalog.font(font_name) else {
        diag::warn_once!(Ui, "actor overlay: font {font_name} is unavailable");
        return TessJob::Hide;
    };
    let description =
        if overlay.description.is_empty() || overlay.description.eq_ignore_ascii_case("none") {
            None
        } else {
            let translated = strings.and_then(|strings| strings.0.text(&overlay.description));
            if translated.is_none() {
                diag::warn_once!(
                    Ui,
                    "actor overlay: localization {key} is unavailable",
                    key = overlay.description.clone()
                );
            }
            translated
        };
    let (name_color, glow_color) = if overlay.hostile {
        (
            color(dvars, "hostilenamefontcolor", [1.0, 0.9, 0.9, 0.7]),
            color(dvars, "hostilenamefontglowcolor", [0.6, 0.0, 0.0, 1.0]),
        )
    } else {
        (
            color(dvars, "friendlynamefontcolor", [0.9, 1.0, 0.9, 0.7]),
            color(dvars, "friendlynamefontglowcolor", [0.0, 0.3, 0.0, 1.0]),
        )
    };
    let list = commands(
        overlay,
        description,
        surface,
        font_name,
        font,
        text_scale,
        (name_color, glow_color),
    );
    let fonts = HashMap::from([(font_name.to_owned(), font)]);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        TessJob::Hide
    } else {
        TessJob::Quads(quads)
    }
}

fn commands(
    overlay: &ActorOverlay,
    description: Option<&str>,
    surface: &Hud2dSurface,
    font_name: &str,
    font: &FontDef,
    text_scale: f32,
    colors: ([f32; 4], [f32; 4]),
) -> Draw2dList {
    let (name_color, glow_color) = colors;
    let mut list = Draw2dList::default();
    for (text, key, y, color, glow) in [
        (
            Some(overlay.name.as_str()),
            "",
            -2.0,
            name_color,
            Some(glow_color),
        ),
        (
            description,
            overlay.description.as_str(),
            18.0,
            [1.0, 1.0, 1.0, 0.7],
            None,
        ),
    ] {
        let Some(text) = text else { continue };
        let rect = surface.apply_rect(25.0, y, 0.0, 0.0, 2, 2);
        let normalized = hud_iw4::normalized_text_scale(font.pixel_height, text_scale);
        let [sx, sy] = surface.scale_virtual_to_real();
        list.cmds.push(Draw2dCmd {
            x: rect.x,
            y: rect.y,
            w: normalized * sx,
            h: normalized * sy,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: normalized * sy,
                text: text.to_owned(),
                loc_key: key.to_owned(),
                style: 3,
                fx: None,
                glow: glow
                    .filter(|_| !font.glow_material.is_empty())
                    .map(|color| TextRunGlow {
                        material: asset_core::AssetRef::bare_name(&font.glow_material).to_owned(),
                        color,
                    }),
            },
            provenance: Draw2dProvenance::CgDraw {
                site: "actor_overlay",
            },
            layer: 1,
        });
    }
    list
}
