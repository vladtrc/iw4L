use std::collections::HashMap;

use assets::{MenuCatalog, PreparedLocalizedStrings};
use bevy::prelude::*;
use frame::UiPlaySound;
use hud_iw4::{
    HE_TYPE_MATERIAL, HE_TYPE_PLAYERNAME, HE_TYPE_TEXT, HE_TYPE_VALUE, HE_TYPE_WAYPOINT, HudElem,
    KEY_UNBOUND, WAYPOINT_CONSTANT_SIZE, WAYPOINT_HIDE_OFFSCREEN, bg_lerp_hud_colors,
    copy_in_use_prefix, hud_elem_glow_color, hud_elem_placement, hud_elem_screen_align,
    hudelem_font_ui_enum, hudelem_text_scale, replace_directive, ui_get_font_handle,
    unbound_directive,
};
use net::{CEntity, CEntityRuntime, CgFrameClock, LocalPresentClient, PresentedSnapshot};
use sim::{ClientLifecycle, SnapshotMeta};

use crate::chrome::r_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay;
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::hudelem::resolve_hud_text;
use crate::images::HudImages;

const HE_TYPE_TIMER_DOWN: i32 = 5;
const HE_TYPE_TIMER_UP: i32 = 6;
const HE_TYPE_TIMER_STATIC: i32 = 7;
const HE_TYPE_TENTHS_TIMER_DOWN: i32 = 8;
const HE_TYPE_TENTHS_TIMER_UP: i32 = 9;
const HE_TYPE_TENTHS_TIMER_STATIC: i32 = 10;

const WAYPOINT_ICON_SIZE: f32 = 24.0;
const WAYPOINT_OFFSCREEN_PAD: f32 = 24.0;

const HUDELEM_FLAG_FOREGROUND: i32 = 0x1;
const HUDELEM_FLAG_HIDEWHENDEAD: i32 = 0x2;
const HUDELEM_FLAG_HIDEWHENINMENU: i32 = 0x4;

#[derive(Component)]
pub(crate) struct HudElemsRaster;
pub(crate) fn spawn_hud_elems(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, HudElemsRaster);
}

#[derive(Component)]
pub(crate) struct HudElemsBackRaster;
pub(crate) fn spawn_hud_elems_back(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, HudElemsBackRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.hud_elems = TessJob::Hide;
    pass.hud_elems_back = TessJob::Hide;
}

fn sprintf_g(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value}")
    }
}

fn timer_text(elem: &HudElem, cg_time: i32) -> Option<String> {
    let (ms, tenths, count_down) = match elem.elem_type {
        HE_TYPE_TIMER_DOWN => (elem.time.wrapping_sub(cg_time), false, true),
        HE_TYPE_TIMER_UP => (cg_time.wrapping_sub(elem.time), false, false),
        HE_TYPE_TIMER_STATIC => (elem.time, false, false),
        HE_TYPE_TENTHS_TIMER_DOWN => (elem.time.wrapping_sub(cg_time), true, true),
        HE_TYPE_TENTHS_TIMER_UP => (cg_time.wrapping_sub(elem.time), true, false),
        HE_TYPE_TENTHS_TIMER_STATIC => (elem.time, true, false),
        _ => return None,
    };
    let ms = ms.max(0);
    let unit = if tenths { 100 } else { 1000 };
    let units = if count_down {
        (ms + unit - 1) / unit
    } else {
        ms / unit
    };
    Some(if tenths {
        let seconds = units / 10;
        format!("{}:{:02}.{}", seconds / 60, seconds % 60, units % 10)
    } else {
        format!("{}:{:02}", units / 60, units % 60)
    })
}

fn player_name(meta: &SnapshotMeta, value: f32) -> String {
    let id = value as u32;
    meta.clients
        .iter()
        .find(|(c, _)| c.0 == id)
        .and_then(|(_, row)| entity_iw4::client_state_name(&row.name))
        .map(str::to_owned)
        .unwrap_or_default()
}

pub(crate) fn resolve_directive(
    command: &str,
    strings: &PreparedLocalizedStrings,
    input: &frame::HudInputView,
) -> Option<String> {
    if matches!(command, "+activate" | "+usereload")
        && let Some(letter) = input.use_key.as_deref()
    {
        return Some(letter.to_owned());
    }
    let unbound = strings.0.text(KEY_UNBOUND)?;
    Some(unbound_directive(unbound, command))
}

fn hud_string(
    meta: &SnapshotMeta,
    strings: &PreparedLocalizedStrings,
    input: &frame::HudInputView,
    index: i32,
    gaps: &mut HudPresentationGaps,
) -> String {
    let Some(raw) = sim::hud_string_in_occupied(&meta.hud_strings, index) else {
        return String::new();
    };
    let Some(text) = resolve_hud_text(strings, raw) else {
        gaps.raise(GapCause::LocalizedRowMissing {
            key: raw.to_owned(),
        });
        return String::new();
    };
    replace_directive(&text, |cmd| {
        resolve_directive(cmd, strings, input).unwrap_or_default()
    })
}

fn combine_label(label: String, body: String) -> String {
    if label.contains("&&1") {
        label.replacen("&&1", &body, 1)
    } else {
        label + &body
    }
}

fn elem_text(
    elem: &HudElem,
    meta: &SnapshotMeta,
    strings: &PreparedLocalizedStrings,
    input: &frame::HudInputView,
    cg_time: i32,
    gaps: &mut HudPresentationGaps,
) -> Option<String> {
    let body = match elem.elem_type {
        HE_TYPE_TEXT => hud_string(meta, strings, input, elem.text, gaps),
        HE_TYPE_VALUE => sprintf_g(elem.value),
        HE_TYPE_PLAYERNAME => player_name(meta, elem.value),
        _ => timer_text(elem, cg_time)?,
    };
    let label = hud_string(meta, strings, input, elem.label, gaps);
    Some(combine_label(label, body))
}

fn place_waypoint(
    elem: &HudElem,
    camera: (&Camera, &GlobalTransform),
    entities: &Query<(&CEntity, &CEntityRuntime)>,
    surface: &crate::surface::Hud2dSurface,
) -> Option<(Vec2, f32)> {
    let offset = Vec3::new(elem.x, elem.y, elem.z);
    let world = if elem.target_ent_num == playerstate_iw4::ENTITYNUM_NONE {
        offset
    } else {
        let (_, runtime) = entities.iter().find(|(entity, runtime)| {
            i32::from(entity.number()) == elem.target_ent_num && runtime.in_next_snap()
        })?;
        Vec3::from_array(runtime.origin) + offset
    };
    let (camera, transform) = camera;
    let eye = transform.translation();
    let forward = *transform.forward();
    let depth = (world - eye).dot(forward);
    let bits = elem.value as i32;
    let constant = bits & WAYPOINT_CONSTANT_SIZE != 0;
    let scale = surface.scale_virtual_to_real()[1];
    let (w, h) = (surface.width(), surface.height());
    let mirrored = if depth <= 0.0 {
        world - forward * (2.0 * depth - 1.0)
    } else {
        world
    };
    let pos = camera.world_to_viewport(transform, mirrored).ok()?;
    let onscreen = depth > 0.0 && (0.0..=w).contains(&pos.x) && (0.0..=h).contains(&pos.y);
    if !constant {
        if !onscreen {
            return None;
        }
        let edge = camera
            .world_to_viewport(transform, world + *transform.right() * elem.width as f32)
            .ok()?;
        return Some((pos, (edge - pos).length()));
    }
    let size = WAYPOINT_ICON_SIZE * scale;
    if onscreen {
        return Some((pos, size));
    }
    if bits & WAYPOINT_HIDE_OFFSCREEN != 0 {
        return None;
    }
    let center = Vec2::new(w, h) * 0.5;
    let half = (center - Vec2::splat(WAYPOINT_OFFSCREEN_PAD * scale)).max(Vec2::ONE);
    let dir = pos - center;
    let reach =
        (half.x / dir.x.abs().max(f32::EPSILON)).min(half.y / dir.y.abs().max(f32::EPSILON));
    Some((center + dir * reach, size))
}

fn face_color(color: [u8; 4]) -> [f32; 4] {
    color.map(|c| f32::from(c) / 255.0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_hud_elems(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    input: Res<frame::HudInputView>,
    cg_clock: Res<CgFrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut sound_latch: ResMut<crate::hudelem::HudElemSoundLatch>,
    mut ui_sound: MessageWriter<UiPlaySound>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    entities: Query<(&CEntity, &CEntityRuntime)>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let meta = &snapshot.meta;
    let Some(client) = meta.for_client(local.0) else {
        hide(&mut pass);
        return;
    };
    let dead = client.lifecycle != ClientLifecycle::Alive;
    let mut elems: Vec<&HudElem> = copy_in_use_prefix(&client.hud_current)
        .iter()
        .chain(copy_in_use_prefix(&client.hud_archival).iter())
        .filter(|e| {
            !(e.flags & HUDELEM_FLAG_HIDEWHENINMENU != 0 && input.menu_open
                || e.flags & HUDELEM_FLAG_HIDEWHENDEAD != 0 && dead)
        })
        .collect();
    if elems.is_empty() {
        hide(&mut pass);
        return;
    }
    elems.sort_by(|a, b| a.sort.total_cmp(&b.sort));
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

    let cg_time = cg_clock.time();
    let mut layers: [Vec<Draw2dCmd>; 2] = Default::default();
    let mut fonts = HashMap::new();
    for (index, elem) in elems.iter().enumerate() {
        let color = bg_lerp_hud_colors(elem, cg_time);
        if color[3] == 0 {
            continue;
        }
        let font_scale = hud_iw4::hud_elem_lerp_font_scale(elem, cg_time);
        let font_height =
            hud_iw4::hudelem_em_px(elem.font, font_scale, surface.scale_virtual_to_real()[1]);
        let provenance = Draw2dProvenance::HudElem {
            index: index as i32,
        };
        let cmds = &mut layers[usize::from(elem.flags & HUDELEM_FLAG_FOREGROUND != 0)];

        if matches!(elem.elem_type, HE_TYPE_MATERIAL | HE_TYPE_WAYPOINT) {
            let Some(material) = u8::try_from(elem.material_index)
                .ok()
                .and_then(|i| sim::name_in_occupied(&meta.hud_materials, i))
            else {
                gaps.raise(GapCause::HudElemMaterialUnbound {
                    material_index: elem.material_index,
                });
                continue;
            };
            if hud_images
                .get(crate::images::HUD_CHROME_NAMESPACE, material, &mut images)
                .is_none()
            {
                gaps.raise(GapCause::HudElemImageMissing {
                    name: material.to_owned(),
                    miss: hud_images.miss_reason(),
                });
                continue;
            }
            let placed = if elem.elem_type == HE_TYPE_WAYPOINT {
                let Some(camera) = cameras.iter().find(|(c, _)| c.is_active) else {
                    continue;
                };
                let Some((at, size)) = place_waypoint(elem, camera, &entities, &surface) else {
                    continue;
                };
                hud_iw4::HudElemPlacement {
                    x: at.x - size * 0.5,
                    y: at.y - size * 0.5,
                    w: size,
                    h: size,
                }
            } else {
                hud_elem_placement(surface.placement(), elem, cg_time, 0.0, font_height)
            };
            cmds.push(Draw2dCmd {
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                x: placed.x,
                y: placed.y,
                w: placed.w,
                h: placed.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: face_color(color),
                material: material.to_owned(),
                op: Draw2dOp::StretchPic,
                provenance,
                layer: 1,
            });
            continue;
        }

        let Some(text) = elem_text(elem, meta, strings, &input, cg_time, &mut gaps) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let text_scale = hudelem_text_scale(elem.font, font_scale);
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
        let material = assets::AssetRef::bare_name(&font.material).to_owned();
        if hud_images
            .get(crate::images::HUD_CHROME_NAMESPACE, &material, &mut images)
            .is_none()
        {
            gaps.raise(GapCause::FontAtlasMissing {
                image: hud_images.zone_image_name(&material).map(str::to_owned),
                material,
            });
            continue;
        }
        fonts.insert(font_name.to_owned(), font);
        if let Some(alias) =
            crate::hudelem::hudelem_pulse_sound(elem, &text, cg_time, &mut sound_latch)
        {
            ui_sound.write(UiPlaySound {
                alias: alias.to_owned(),
            });
        }
        let nscale = hud_iw4::r_normalized_text_scale(font.pixel_height, text_scale);
        let (horz_align, vert_align) = hud_elem_screen_align(elem.align_screen);
        let glyph = surface.apply_rect(0.0, 0.0, nscale, nscale, horz_align, vert_align);
        let text_width = r_text_width(font, &text) as f32 * glyph.w;
        let placed =
            hud_elem_placement(surface.placement(), elem, cg_time, text_width, font_height);
        cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: (placed.x + 0.5).floor(),
            y: (placed.text_baseline_y() + 0.5).floor(),
            w: glyph.w,
            h: glyph.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: face_color(color),
            material,
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: nscale,
                text,
                loc_key: String::new(),

                style: crate::draw2d::TEXT_STYLE_HUDELEM,
                fx: crate::hudelem::hudelem_text_fx(elem, cg_time),
                glow: hud_elem_glow_color(elem, color)
                    .and_then(|glow| crate::chrome::text_run_glow(font, glow)),
            },
            provenance,
            layer: 1,
        });
    }

    if layers.iter().all(Vec::is_empty) {
        hide(&mut pass);
        return;
    }

    let mut any_fx = false;
    for fx in layers.iter().flatten().filter_map(|cmd| match &cmd.op {
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
    let [back, front] = layers.map(|cmds| {
        let (quads, _) = tessellate_fonts(&Draw2dList { cmds }, &fonts);
        if quads.is_empty() {
            TessJob::Hide
        } else {
            TessJob::Quads(quads)
        }
    });
    pass.hud_elems_back = back;
    pass.hud_elems = front;
}
