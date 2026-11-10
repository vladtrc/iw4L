use std::collections::HashMap;

use asset_game::MenuCatalog;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use playerstate_iw4::{breath_hold_time_ms, weap_flags};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImageLoader;

#[derive(Component)]
pub(crate) struct BreathHintRaster;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct BreathHintRenderer<'w> {
    catalog: Option<Res<'w, MenuCatalog>>,
    hud_images: HudImageLoader<'w>,
    pass: ResMut<'w, HudTessPass>,
}

impl BreathHintRenderer<'_> {
    fn hide(&mut self) {
        self.pass.breath_hint = TessJob::Hide;
    }

    fn render(
        &mut self,
        surface: &crate::surface::Hud2dSurface,
        ps: &playerstate_iw4::PlayerState,
        input: &frame::HudInputView,
        strings: Option<Res<PreparedLocalizedStrings>>,
        facts: Option<asset_game::WeaponHudFacts>,
    ) {
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };
        let Some(item) = catalog
            .get("hud_fullscreen")
            .and_then(|m| m.items.iter().find(|i| i.owner_draw == 113))
        else {
            return;
        };
        let font_name = hud_iw4::ui_get_font_handle(
            item.font_enum,
            surface.scale_virtual_to_real()[1],
            item.text_scale,
        );
        let Some(font) = catalog.font(font_name) else {
            return;
        };
        let binding = ["+holdbreath", "+melee_breath", "+breath_sprint"]
            .iter()
            .find_map(|cmd| input.binding_keys.get(*cmd))
            .map(String::as_str)
            .unwrap_or("UNBOUND");
        let template = strings
            .as_ref()
            .and_then(|s| s.0.text("PLATFORM_HOLD_BREATH"));
        let mut text = if ps.weap_flags & weap_flags::HOLD_BREATH != 0 {
            format!(
                "Holding breath  {:.1}s",
                (breath_hold_time_ms(ps) - ps.hold_breath_timer).max(0) as f32 * 0.001
            )
        } else if ps.hold_breath_timer > 0 {
            format!(
                "Recovering breath  {:.1}s",
                ps.hold_breath_timer as f32 * 0.001
            )
        } else {
            template
                .map(|t| hud_iw4::hint_replace_bind(t, binding))
                .unwrap_or_else(|| format!("Hold [{binding}] to steady aim"))
        };
        if facts.is_some_and(|f| f.scope_zoom.is_variable()) {
            let binding = ["+changezoom", "+melee_zoom"]
                .iter()
                .find_map(|cmd| input.binding_keys.get(*cmd))
                .map(String::as_str)
                .unwrap_or("UNBOUND");
            let zoom = facts.expect("visible scope hint").scope_zoom;
            let hint = format!(
                "[{binding}] Change zoom  {}/{}",
                ps.scope_zoom_level + 1,
                zoom.levels()
            );
            if facts.is_some_and(|f| f.can_hold_breath) {
                text.push('\n');
                text.push_str(&hint);
            } else {
                text = hint;
            }
        }
        let nscale = hud_iw4::normalized_text_scale(font.pixel_height, item.text_scale);
        let material = asset_core::AssetRef::bare_name(&font.material).to_owned();
        let list = Draw2dList {
            cmds: text
                .lines()
                .enumerate()
                .map(|(line, text)| {
                    let width = crate::chrome::ui_text_width(font, text, item.text_scale);
                    let rect = surface.apply_rect(
                        item.rect.x - width * 0.5,
                        item.rect.y + line as f32 * font.pixel_height as f32 * nscale,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    Draw2dCmd {
                        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                        x: rect.x,
                        y: rect.y,
                        w: rect.w,
                        h: rect.h,
                        s0: 0.0,
                        t0: 0.0,
                        s1: 1.0,
                        t1: 1.0,
                        color: item.fore_color,
                        material: material.clone(),
                        op: Draw2dOp::TextRun {
                            font: font_name.to_owned(),
                            scale: nscale,
                            text: text.to_owned(),
                            loc_key: "PLATFORM_HOLD_BREATH".to_owned(),
                            style: item.text_style,
                            fx: None,
                            glow: None,
                        },
                        provenance: Draw2dProvenance::CgDraw {
                            site: "breath_hint",
                        },
                        layer: 1,
                    }
                })
                .collect(),
        };
        let mut fonts = HashMap::new();
        fonts.insert(font_name.to_owned(), font);
        let (quads, _) = tessellate_fonts(&list, &fonts);
        if !quads.is_empty()
            && self
                .hud_images
                .load(crate::images::HUD_CHROME_NAMESPACE, &material)
                .is_some()
        {
            self.pass.breath_hint = TessJob::Quads(quads);
        }
    }
}

pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    weapons: Option<Res<PreparedWeapons>>,
    presentation: net::LocalPresentation,
    input: Res<frame::HudInputView>,
    view: Option<Res<frame::ViewSubject>>,
    mut renderer: BreathHintRenderer,
) {
    renderer.hide();
    if !surface.is_ready()
        || input.script_menu_open
        || input.console_open
        || view.as_ref().is_some_and(|v| v.in_killcam())
    {
        return;
    }
    let Some(ps) = presentation.alive_player() else {
        return;
    };
    let weapon = playerstate_iw4::get_viewmodel_weapon_index(ps);
    let facts = weapons.as_ref().and_then(|w| {
        w.snapshot_weapon(weapon)
            .ok()
            .and_then(|weapon| weapon.hud_facts())
    });
    if ps.f_weapon_pos_frac != 1.0
        || !facts.is_some_and(|f| f.can_hold_breath || f.scope_zoom.is_variable())
    {
        return;
    }
    renderer.render(&surface, ps, &input, strings, facts);
}
