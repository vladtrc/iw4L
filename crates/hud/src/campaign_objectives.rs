use crate::draw2d::{
    Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, TextRunFx, TextRunGlow, tessellate_fonts,
};
use crate::gpu_list::{HudTessPass, TessJob};
use bevy::prelude::*;
use frame::ScopeApp;
use std::collections::HashMap;

#[derive(Component)]
pub(crate) struct CampaignObjectivesRaster;

struct Notice {
    born: i32,
    state: sim::ObjectiveState,
    text: String,
    last_sound_time: i32,
}

impl Notice {
    fn write_time(&self) -> i32 {
        hud_iw4::seh_print_strlen(&self.text).saturating_mul(50)
    }
    fn decay_start(&self) -> i32 {
        self.write_time().max(6000)
    }
    fn alive(&self, now: i32) -> bool {
        now.saturating_sub(self.born) <= self.decay_start().saturating_add(700)
    }
}

#[derive(Resource, Default)]
struct Notices(Vec<Notice>);

pub(crate) fn register(app: &mut App) {
    app.scoped::<Notices>(frame::Timeline::Live).add_systems(
        Update,
        update
            .in_set(frame::InMatch)
            .after(crate::surface::update_hud_surface)
            .before(crate::plugin::flush_campaign_objectives_tess)
            .in_set(frame::LifeFrontPublished),
    );
}

fn resolve(raw: &str, translate: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut parts = Vec::new();
    let mut plain = false;
    let mut start = 0;
    for (at, c) in raw.char_indices().chain(std::iter::once((raw.len(), '\0'))) {
        if !matches!(c, '\x14' | '\x15' | '\0') {
            continue;
        }
        if at > start {
            let part = &raw[start..at];
            parts.push(if plain || !part.bytes().any(|b| b.is_ascii_alphabetic()) {
                part.to_owned()
            } else {
                translate(part)?
            });
        }
        plain = c == '\x15';
        start = at + c.len_utf8();
    }
    let Some((first, args)) = parts.split_first() else {
        return Some(String::new());
    };
    let mut text = first.clone();
    for (at, arg) in args.iter().enumerate() {
        let token = format!("&&{}", at + 1);
        if text.contains(&token) {
            text = text.replace(&token, arg);
        } else {
            text.push_str(arg);
        }
    }
    Some(text)
}

#[derive(bevy::ecs::system::SystemParam)]
struct ObjectiveDisplay<'w> {
    catalog: Option<Res<'w, asset_game::CampaignHudCatalog>>,
    strings: Option<Res<'w, assets::PreparedLocalizedStrings>>,
    clock: Res<'w, net::FrameClock>,
    surface: Res<'w, crate::surface::Hud2dSurface>,
    pass: ResMut<'w, HudTessPass>,
    notices: ResMut<'w, Notices>,
    sound: MessageWriter<'w, frame::UiPlaySound>,
}

fn update(
    mut display: ObjectiveDisplay,
    mut messages: MessageReader<net::SvcObjective>,
    presentation: net::LocalPresentation,
) {
    display.pass.campaign_objectives = TessJob::Hide;
    let now = display.clock.time();
    display.notices.0.retain(|notice| notice.alive(now));
    for message in messages.read() {
        let Some(strings) = display.strings.as_deref() else {
            continue;
        };
        let Some(text) = resolve(&message.0.text, |key| {
            strings.0.text(key).map(str::to_owned)
        }) else {
            diag::warn_once!(
                Ui,
                "objective notification localization unavailable: {key}",
                key = message.0.text.clone()
            );
            continue;
        };
        let born = display
            .notices
            .0
            .last()
            .map_or(now.saturating_add(250), |last| {
                now.saturating_add(250).max(
                    last.born
                        .saturating_add(last.write_time())
                        .saturating_add(150),
                )
            });
        display.notices.0.push(Notice {
            born,
            state: message.0.state,
            text,
            last_sound_time: 0,
        });
    }
    let Some(snapshot) = presentation
        .snapshot()
        .filter(|s| s.meta.objectives.campaign_mode)
    else {
        return;
    };
    let dvars = snapshot.meta.script_dvars(presentation.client_id());
    if !display.surface.is_ready() || dvars.int("cg_draw2d") == Some(0) {
        return;
    }
    let Some(catalog) = display.catalog.as_ref().map(|c| c.0.as_ref()) else {
        return;
    };
    let Some(item) = catalog.get("gamemessages").and_then(|menu| {
        menu.items.iter().find(|i| {
            i.item_type == hud_iw4::ITEM_TYPE_GAME_MESSAGE_WINDOW && i.game_msg_window_index == 0
        })
    }) else {
        diag::warn_once!(Ui, "SP gamemessages window0 is unavailable");
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        6,
        display.surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        return;
    };
    let normalized = hud_iw4::normalized_text_scale(font.pixel_height, item.text_scale);
    let [sx, sy] = display.surface.scale_virtual_to_real();
    let base = dvars
        .string("con_typewritercolorbase")
        .and_then(|text| {
            let values: Option<Vec<f32>> =
                text.split_whitespace().map(|v| v.parse().ok()).collect();
            <[f32; 3]>::try_from(values?).ok()
        })
        .filter(|v| v.iter().all(|v| v.is_finite() && (0.0..=1.).contains(v)))
        .unwrap_or([1.; 3]);
    let mut list = Draw2dList::default();
    let mut y = item.rect.y;
    for notice in &mut display.notices.0 {
        if now < notice.born {
            continue;
        }
        if let Some(sound) = hud_iw4::play_text_fx_pulse_sounds(
            now,
            hud_iw4::seh_print_strlen(&notice.text),
            notice.born,
            50,
            6000,
            &mut notice.last_sound_time,
        ) {
            display.sound.write(frame::UiPlaySound {
                alias: sound.alias().into(),
            });
        }
        let rect = display.surface.apply_rect(
            item.rect.x,
            y,
            0.,
            0.,
            i32::from(item.rect.horz_align),
            i32::from(item.rect.vert_align),
        );
        let glow = match notice.state {
            sim::ObjectiveState::Done => [0., 0.3, 0.8, 1.],
            sim::ObjectiveState::Failed => [0.8, 0., 0., 1.],
            _ => [0., 0.6, 0.18, 1.],
        };
        list.cmds.push(Draw2dCmd {
            x: rect.x,
            y: rect.y + font.pixel_height as f32 * normalized * sy,
            w: normalized * sx,
            h: normalized * sy,
            s0: 0.,
            t0: 0.,
            s1: 1.,
            t1: 1.,
            color: [base[0], base[1], base[2], item.fore_color[3]],
            material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: font_name.into(),
                scale: normalized * sy,
                text: notice.text.clone(),
                loc_key: String::new(),
                style: item.text_style,
                fx: Some(TextRunFx {
                    scene_time: now,
                    fx: hud_iw4::TextPulseFx {
                        birth_time: notice.born,
                        letter_time: 50,
                        decay_start_time: notice.decay_start(),
                        decay_duration: 700,
                    },
                }),
                glow: (!font.glow_material.is_empty()).then(|| TextRunGlow {
                    material: asset_core::AssetRef::bare_name(&font.glow_material).to_owned(),
                    color: glow,
                }),
            },
            provenance: Draw2dProvenance::CgDraw {
                site: "campaign_objective_message",
            },
            layer: 1,
        });
        y += font.pixel_height as f32 * normalized;
    }
    let (quads, _) = tessellate_fonts(&list, &HashMap::from([(font_name.to_owned(), font)]));
    if !quads.is_empty() {
        display.pass.campaign_objectives = TessJob::Quads(quads);
    }
}
