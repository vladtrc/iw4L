use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use hud_iw4::{
    HE_TYPE_VALUE, HudElem, bg_lerp_hud_colors, copy_in_use_prefix, hud_elem_lerp_font_scale,
    hud_elem_placement, hud_elem_screen_align, hudelem_font_ui_enum, hudelem_text_scale,
    ui_get_font_handle,
};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};

use crate::chrome::r_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay;
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

#[derive(Component)]
pub(crate) struct ScorePopupRaster;
pub(crate) fn spawn_score_popup(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, ScorePopupRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.score_popup = TessJob::Hide;
}

fn sprintf_g(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value}")
    }
}

struct LiveValue {
    elem: HudElem,
    value: f32,
    color: [u8; 4],
    glow: Option<[f32; 4]>,
    font_scale: f32,
    font: i32,
    fx: Option<crate::draw2d::TextRunFx>,
}

fn live_value<'a>(
    current: &'a [HudElem],
    archival: &'a [HudElem],
    cg_time: i32,
    previous: Option<(&[HudElem], &[HudElem], f32)>,
) -> Option<LiveValue> {
    fn best_value<'a>(current: &'a [HudElem], archival: &'a [HudElem]) -> Option<&'a HudElem> {
        let mut best: Option<&HudElem> = None;
        for elem in copy_in_use_prefix(current)
            .iter()
            .chain(copy_in_use_prefix(archival).iter())
        {
            if elem.elem_type != HE_TYPE_VALUE {
                continue;
            }
            if elem.sort != gamemode_iw4::SCORE_POPUP_SORT {
                continue;
            }
            if best.is_none_or(|b| elem.sort >= b.sort) {
                best = Some(elem);
            }
        }
        best
    }

    let elem = best_value(current, archival)?;
    let color = bg_lerp_hud_colors(elem, cg_time);
    if color[3] == 0 {
        return None;
    }
    let font_scale = previous
        .filter(|_| elem.font_scale_time == 0)
        .and_then(|(current, archival, phase)| {
            let previous = best_value(current, archival)?;
            (previous.font_scale_time == 0)
                .then(|| previous.font_scale + (elem.font_scale - previous.font_scale) * phase)
        })
        .unwrap_or_else(|| hud_elem_lerp_font_scale(elem, cg_time));
    Some(LiveValue {
        elem: *elem,
        value: elem.value,
        color,
        glow: hud_iw4::hud_elem_glow_color(elem, color),
        font_scale,
        font: elem.font,
        fx: crate::hudelem::hudelem_text_fx(elem, cg_time),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_score_popup(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    cg_clock: Res<CgFrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let cg_time = cg_clock.time();
    let previous_hud = presented
        .interpolation_pair()
        .and_then(|(previous, _, phase)| {
            let meta = previous.meta.for_client(local.0)?;
            Some((
                meta.hud_current.as_slice(),
                meta.hud_archival.as_slice(),
                phase,
            ))
        });
    let Some(meta) = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
    else {
        hide(&mut pass);
        return;
    };
    let Some(LiveValue {
        elem,
        value,
        color,
        glow,
        font_scale,
        font: elem_font,
        fx,
    }) = live_value(&meta.hud_current, &meta.hud_archival, cg_time, previous_hud)
    else {
        hide(&mut pass);
        return;
    };
    let alpha = color[3] as f32 / 255.0;

    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::NoFontCatalog);
        hide(&mut pass);
        return;
    };
    let text_scale = hudelem_text_scale(elem_font, font_scale);
    let font_name = ui_get_font_handle(
        hudelem_font_ui_enum(elem_font),
        surface.scale_virtual_to_real()[1],
        text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        gaps.raise(GapCause::FontMissing {
            name: font_name.to_owned(),
        });
        hide(&mut pass);
        return;
    };

    let Some(strings) = strings.as_ref() else {
        gaps.raise(GapCause::NoStringTable);
        hide(&mut pass);
        return;
    };
    let Some(plus) = strings.0.text(gamemode_iw4::SCORE_POPUP_LABEL) else {
        gaps.raise(GapCause::LocalizedRowMissing {
            key: gamemode_iw4::SCORE_POPUP_LABEL.to_owned(),
        });
        hide(&mut pass);
        return;
    };
    let text = format!("{plus}{}", sprintf_g(value));

    let nscale = hud_iw4::r_normalized_text_scale(font.pixel_height, text_scale);
    let (horz, vert) = hud_elem_screen_align(elem.align_screen);
    let glyph = surface.apply_rect(0.0, 0.0, nscale, nscale, horz, vert);
    let text_width = r_text_width(font, &text) as f32 * glyph.w;
    let font_height =
        hud_iw4::hudelem_em_px(elem.font, font_scale, surface.scale_virtual_to_real()[1]);
    let placed = hud_elem_placement(surface.placement(), &elem, cg_time, text_width, font_height);
    let material = assets::AssetRef::bare_name(&font.material).to_owned();
    let face = [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        alpha,
    ];
    let list = Draw2dList {
        cmds: vec![Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: (placed.x + 0.5).floor(),
            y: (placed.text_baseline_y() + 0.5).floor(),
            w: glyph.w,
            h: glyph.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: face,
            material: material.clone(),
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: nscale,
                text: text.clone(),
                loc_key: gamemode_iw4::SCORE_POPUP_LABEL.to_owned(),

                style: crate::draw2d::TEXT_STYLE_HUDELEM,

                fx,
                glow: glow.and_then(|color| crate::chrome::text_run_glow(font, color)),
            },
            provenance: Draw2dProvenance::HudElem { index: 0 },
            layer: 1,
        }],
    };
    let mut fonts = HashMap::new();
    fonts.insert(font_name.to_owned(), font);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    let tex_ok = hud_images
        .get(crate::images::HUD_CHROME_NAMESPACE, &material, &mut images)
        .is_some();
    if quads.is_empty() || !tex_ok {
        if !tex_ok && !quads.is_empty() {
            let image = hud_images.zone_image_name(&material).map(str::to_owned);
            gaps.raise(GapCause::FontAtlasMissing { material, image });
        }
        hide(&mut pass);
        return;
    }
    pass.score_popup = TessJob::Quads(quads);
}
