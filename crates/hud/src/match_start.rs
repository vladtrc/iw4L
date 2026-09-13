use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use frame::UiPlaySound;
use hud_iw4::{
    ALIGN_CENTER, ALIGN_VIEWABLE, HE_TYPE_PLAYERNAME, HE_TYPE_TEXT, HE_TYPE_VALUE, HudElem,
    bg_lerp_hud_colors, copy_in_use_prefix, hudelem_font_ui_enum, hudelem_text_scale,
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
pub(crate) struct MatchStartRaster;
pub(crate) fn spawn_match_start(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, MatchStartRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.match_start = TessJob::Hide;
}

fn sprintf_g(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value}")
    }
}

fn is_score_popup(elem: &HudElem) -> bool {
    elem.elem_type == HE_TYPE_VALUE && (elem.y - gamemode_iw4::SCORE_POPUP_Y).abs() < 1.0
}

fn vert_align(elem: &HudElem) -> i32 {
    if elem.align_screen == hud_iw4::OUTCOME_ALIGN_SCREEN {
        ALIGN_VIEWABLE
    } else {
        ALIGN_CENTER
    }
}

fn player_name(presented: &PresentedSnapshot, value: f32) -> String {
    let id = value as u32;
    let Some(s) = presented.snapshot() else {
        return String::new();
    };
    let Some((_, row)) = s.meta.clients.iter().find(|(c, _)| c.0 == id) else {
        return String::new();
    };
    match entity_iw4::client_state_name(&row.name) {
        Some(name) => name.to_owned(),
        None => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_match_start(
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
    mut sound_latch: ResMut<crate::hudelem::HudElemSoundLatch>,
    mut ui_sound: MessageWriter<UiPlaySound>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let Some(meta) = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
    else {
        hide(&mut pass);
        return;
    };
    let cg_time = cg_clock.time();
    let elems: Vec<&HudElem> = copy_in_use_prefix(&meta.hud_current)
        .iter()
        .chain(copy_in_use_prefix(&meta.hud_archival).iter())
        .filter(|e| {
            (e.elem_type == HE_TYPE_TEXT
                || e.elem_type == HE_TYPE_VALUE
                || e.elem_type == HE_TYPE_PLAYERNAME)
                && !is_score_popup(e)
        })
        .collect();
    if elems.is_empty() {
        hide(&mut pass);
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::NoFontCatalog);
        hide(&mut pass);
        return;
    };
    let Some(strings) = strings.as_ref() else {
        gaps.raise(GapCause::NoStringTable);
        hide(&mut pass);
        return;
    };

    let mut cmds = Vec::new();
    let mut fonts = HashMap::new();

    for (index, elem) in elems.iter().enumerate() {
        let color = bg_lerp_hud_colors(elem, cg_time);
        if color[3] == 0 {
            continue;
        }
        let text_scale =
            hudelem_text_scale(elem.font, hud_iw4::hud_elem_lerp_font_scale(elem, cg_time));
        let font_name = ui_get_font_handle(
            hudelem_font_ui_enum(elem.font),
            surface.scale_virtual_to_real()[1],
            text_scale,
        );
        let Some(font) = catalog.font(font_name) else {
            gaps.raise(GapCause::FontMissing {
                name: font_name.to_owned(),
            });
            continue;
        };
        fonts.insert(font_name.to_owned(), font);
        let (text, loc_key) = if elem.elem_type == HE_TYPE_VALUE {
            (sprintf_g(elem.value), String::new())
        } else if elem.elem_type == HE_TYPE_PLAYERNAME {
            let name = player_name(&presented, elem.value);
            match gamemode_iw4::loc_key_from_label(elem.label) {
                Some(key) => {
                    let prefix = match strings.0.text(key) {
                        Some(p) => p.to_owned(),
                        None => {
                            gaps.raise(GapCause::LocalizedRowMissing {
                                key: key.to_owned(),
                            });
                            String::new()
                        }
                    };
                    let text = if prefix.is_empty() {
                        name
                    } else {
                        format!("{prefix}{name}")
                    };
                    (text, key.to_owned())
                }
                None => (name, String::new()),
            }
        } else {
            let Some(key) = gamemode_iw4::loc_key_from_label(elem.label) else {
                continue;
            };
            let Some(loc) = strings.0.text(key) else {
                gaps.raise(GapCause::LocalizedRowMissing {
                    key: key.to_owned(),
                });
                continue;
            };
            (loc.to_owned(), key.to_owned())
        };
        if text.is_empty() {
            continue;
        }

        if let Some(alias) =
            crate::hudelem::hudelem_pulse_sound(elem, &text, cg_time, &mut sound_latch)
        {
            ui_sound.write(UiPlaySound {
                alias: alias.to_owned(),
            });
        }
        let nscale = hud_iw4::r_normalized_text_scale(font.pixel_height, text_scale);
        let measured = r_text_width(font, &text) as f32 * nscale;
        let applied = surface.apply_rect(
            elem.x - measured / 2.0,
            elem.y,
            nscale,
            nscale,
            ALIGN_CENTER,
            vert_align(elem),
        );
        let alpha = color[3] as f32 / 255.0;
        let face = [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            alpha,
        ];
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
            color: face,
            material: assets::AssetRef::bare_name(&font.material).to_owned(),
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: nscale,
                text: text.clone(),
                loc_key,

                style: crate::draw2d::TEXT_STYLE_HUDELEM,
                fx: crate::hudelem::hudelem_text_fx(elem, cg_time),
            },
            provenance: Draw2dProvenance::HudElem {
                index: index as i32,
            },
            layer: 1,
        });
    }

    if cmds.is_empty() {
        hide(&mut pass);
        return;
    }

    let mut any_fx = false;
    for fx in cmds.iter().filter_map(|cmd| match &cmd.op {
        Draw2dOp::TextRun { fx: Some(fx), .. } => Some(fx),
        _ => None,
    }) {
        any_fx = true;
        if hud_iw4::fx_decay_tick_count(fx.fx.decay_duration).is_none() {
            gaps.raise(GapCause::TextDecodeFxDecayTooShort {
                fx_decay_duration: fx.fx.decay_duration,
            });
        }
    }
    if any_fx {
        let material = hud_iw4::DECODE_CHARACTERS_MATERIAL;
        if hud_images
            .get(crate::images::HUD_CHROME_NAMESPACE, material, &mut images)
            .is_none()
        {
            gaps.raise(GapCause::TextDecodeFxAtlasMissing {
                material: material.to_owned(),
                image: hud_images.zone_image_name(material).map(str::to_owned),
            });
        }
    }
    let list = Draw2dList { cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    let mut tex_ok = true;
    for cmd in &list.cmds {
        if hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &cmd.material,
                &mut images,
            )
            .is_none()
        {
            tex_ok = false;
            gaps.raise(GapCause::FontAtlasMissing {
                material: cmd.material.clone(),
                image: hud_images.zone_image_name(&cmd.material).map(str::to_owned),
            });
        }
    }
    if quads.is_empty() || !tex_ok {
        hide(&mut pass);
        return;
    }
    pass.match_start = TessJob::Quads(quads);
}
