use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use hud_iw4::{
    CG_OWNERDRAW_MANTLE, HINT_MANTLE_MATERIAL, KEY_UNBOUND, PLATFORM_MANTLE,
    cg_draw_mantle_hint_layout, cg_draw_mantle_hint_visible, default_mp_key_binding,
    mantle_hint_replace_bind, r_normalized_text_scale, ui_get_font_handle, ui_text_height,
    unbound_directive,
};
use net::{LocalPresentClient, PresentedSnapshot};

use crate::chrome::ui_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay;
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

const HUD_FULLSCREEN: &str = "hud_fullscreen";

#[derive(Component)]
pub(crate) struct MantleHintRaster;
pub(crate) fn spawn_mantle_hint(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, MantleHintRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.mantle_hint = TessJob::Hide;
}

fn bind_letter(strings: &PreparedLocalizedStrings) -> Option<String> {
    if let Some(letter) = default_mp_key_binding("+gostand") {
        return Some(letter.to_owned());
    }
    if let Some(letter) = default_mp_key_binding("+moveup") {
        return Some(letter.to_owned());
    }
    let unbound = strings.0.text(KEY_UNBOUND)?;
    Some(unbound_directive(unbound, "+gostand"))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_mantle_hint(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
) {
    let Some(ps) = presented.alive_player(local.0) else {
        hide(&mut pass);
        return;
    };
    if !cg_draw_mantle_hint_visible(ps.mantle_flags) {
        hide(&mut pass);
        return;
    }
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::NoFontCatalog);
        hide(&mut pass);
        return;
    };
    let Some(menu) = catalog.get(HUD_FULLSCREEN) else {
        gaps.raise(GapCause::MantleHintNoItem);
        hide(&mut pass);
        return;
    };
    let Some(item) = menu
        .items
        .iter()
        .find(|it| it.owner_draw == CG_OWNERDRAW_MANTLE)
    else {
        gaps.raise(GapCause::MantleHintNoItem);
        hide(&mut pass);
        return;
    };

    let Some(strings) = strings.as_ref() else {
        gaps.raise(GapCause::NoStringTable);
        hide(&mut pass);
        return;
    };
    let Some(template) = strings.0.text(PLATFORM_MANTLE) else {
        gaps.raise(GapCause::LocalizedRowMissing {
            key: PLATFORM_MANTLE.to_owned(),
        });
        hide(&mut pass);
        return;
    };
    let Some(bind) = bind_letter(strings) else {
        gaps.raise(GapCause::LocalizedRowMissing {
            key: KEY_UNBOUND.to_owned(),
        });
        hide(&mut pass);
        return;
    };
    let text = mantle_hint_replace_bind(template, &bind);

    let font_name = ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        gaps.raise(GapCause::FontMissing {
            name: font_name.to_owned(),
        });
        hide(&mut pass);
        return;
    };

    let nscale = r_normalized_text_scale(font.pixel_height, item.text_scale);
    let length = ui_text_width(font, &text, item.text_scale);
    let height = ui_text_height(item.text_scale);
    let layout = cg_draw_mantle_hint_layout(
        item.rect.x,
        item.rect.y,
        item.rect.w,
        item.rect.h,
        length,
        height,
    );
    let horz = i32::from(item.rect.horz_align);
    let vert = i32::from(item.rect.vert_align);
    let text_applied = surface.apply_rect(layout.text_x, layout.text_y, nscale, nscale, horz, vert);
    let pic_applied = surface.apply_rect(
        layout.pic_x,
        layout.pic_y,
        item.rect.w,
        item.rect.h,
        horz,
        vert,
    );

    let pic_ok = hud_images
        .get(
            crate::images::HUD_CHROME_NAMESPACE,
            HINT_MANTLE_MATERIAL,
            &mut images,
        )
        .is_some();
    if !pic_ok {
        gaps.raise(GapCause::MantleHintImageMissing {
            name: HINT_MANTLE_MATERIAL.to_owned(),
            miss: hud_images.miss_reason(),
        });
        hide(&mut pass);
        return;
    }

    let font_material = assets::AssetRef::bare_name(&font.material).to_owned();
    let list = Draw2dList {
        cmds: vec![
            Draw2dCmd {
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                x: (text_applied.x + 0.5).floor(),
                y: (text_applied.y + 0.5).floor(),
                w: text_applied.w,
                h: text_applied.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: item.fore_color,
                material: font_material.clone(),
                op: Draw2dOp::TextRun {
                    font: font_name.to_owned(),
                    scale: nscale,
                    text: text.clone(),
                    loc_key: PLATFORM_MANTLE.to_owned(),

                    style: item.text_style,
                    fx: None,
                    glow: None,
                },
                provenance: Draw2dProvenance::CgDraw {
                    site: "mantle_hint",
                },
                layer: 1,
            },
            Draw2dCmd {
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                x: (pic_applied.x + 0.5).floor(),
                y: (pic_applied.y + 0.5).floor(),
                w: pic_applied.w,
                h: pic_applied.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: item.fore_color,
                material: HINT_MANTLE_MATERIAL.to_owned(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::OwnerDraw(CG_OWNERDRAW_MANTLE),
                layer: 1,
            },
        ],
    };
    let mut fonts = HashMap::new();
    fonts.insert(font_name.to_owned(), font);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    let tex_ok = hud_images
        .get(
            crate::images::HUD_CHROME_NAMESPACE,
            &font_material,
            &mut images,
        )
        .is_some();
    if quads.is_empty() || !tex_ok {
        if !tex_ok && !quads.is_empty() {
            let image = hud_images
                .zone_image_name(&font_material)
                .map(str::to_owned);
            gaps.raise(GapCause::FontAtlasMissing {
                material: font_material,
                image,
            });
        }
        hide(&mut pass);
        return;
    }
    pass.mantle_hint = TessJob::Quads(quads);
}
