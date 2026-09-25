use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use assets::{MenuCatalog, PreparedWeapons};
use bevy::prelude::*;
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};
use std::collections::HashMap;

#[derive(Component)]
pub(crate) struct UseHintRaster;

#[derive(Default)]
pub(crate) struct HintMemory {
    caption: Option<(
        String,
        String,
        Option<(assets::AssetNamespace, String, i32, bool)>,
    )>,
    last_seen: f64,
}

pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<assets::PreparedLocalizedStrings>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    cg_clock: Res<CgFrameClock>,
    input: Res<frame::HudInputView>,
    mut pass: ResMut<HudTessPass>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    weapons: Option<Res<PreparedWeapons>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut memory: Local<HintMemory>,
    view: Option<Res<frame::ViewSubject>>,
) {
    pass.use_hint = TessJob::Hide;
    if !surface.is_ready() || input.menu_open || view.as_ref().is_some_and(|v| v.in_killcam()) {
        memory.caption = None;
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        memory.caption = None;
        return;
    };
    if let Some(catalog) = catalog.as_deref() {
        let previous = presented
            .interpolation_pair()
            .map(|(before, _, phase)| (before, phase));
        let quads = crate::objectives::draw(
            &surface,
            catalog,
            strings.as_ref().map(|s| &s.0),
            snapshot,
            previous,
            local.0,
            cg_clock.time(),
            cameras.iter().find(|(c, _)| c.is_active),
        );
        if !quads.is_empty() {
            pass.use_hint = TessJob::Quads(quads);
        }
    }
    if snapshot.meta.phase != sim::MatchPhase::Playing {
        memory.caption = None;
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        memory.caption = None;
        return;
    };
    let result = (|| {
        if ps.cursor_hint <= 4
            && !snapshot
                .meta
                .map_doors
                .as_ref()
                .is_some_and(|d| d.hints.contains(&local.0))
            && !snapshot
                .meta
                .objectives
                .bombs
                .iter()
                .any(|b| !b.destroyed && b.view.users.contains(&local.0))
        {
            return Ok(None);
        }
        let bind = input
            .use_key
            .as_deref()
            .or_else(|| strings.as_ref()?.0.text(hud_iw4::KEY_UNBOUND))
            .ok_or_else(|| "KEY_UNBOUND localization missing".to_owned())?;
        if let Some(doors) = &snapshot.meta.map_doors
            && doors.hints.contains(&local.0)
        {
            let (text, key) = if doors.unavailable(snapshot.tick.0.saturating_mul(50)) {
                (
                    "Door Switch is Unavailable".to_owned(),
                    "MP_HOLD_DOOR_SWITCH_UNAVAILABLE",
                )
            } else {
                (
                    format!("Hold ^3{bind}^7 to Operate Doors"),
                    "MP_HOLD_TO_OPERATE_DOORS",
                )
            };
            return Ok(Some((text, key.to_owned(), None)));
        }
        if snapshot.meta.kind == gamemode_iw4::GameModeKind::Demolition
            && let Some(site) = snapshot
                .meta
                .objectives
                .bombs
                .iter()
                .find(|b| !b.destroyed && b.view.users.contains(&local.0))
        {
            if site.user == Some(local.0) {
                return Ok(None);
            }
            let key = if site.planted_at_ms.is_some() {
                "PLATFORM_HOLD_TO_DEFUSE_EXPLOSIVES"
            } else {
                "PLATFORM_HOLD_TO_PLANT_EXPLOSIVES"
            };
            let template = strings
                .as_ref()
                .and_then(|s| s.0.text(key))
                .ok_or_else(|| format!("missing {key}"))?;
            let unbound = strings
                .as_ref()
                .and_then(|s| s.0.text(hud_iw4::KEY_UNBOUND))
                .ok_or_else(|| "KEY_UNBOUND localization missing".to_owned())?;
            let text = hud_iw4::replace_directive(template, |cmd| {
                if matches!(cmd, "+activate" | "+usereload") {
                    bind.to_owned()
                } else {
                    hud_iw4::unbound_directive(unbound, cmd)
                }
            })
            .replace("&&1", bind);
            return Ok(Some((text, key.to_owned(), None)));
        }
        let now_ms = snapshot.tick.0.saturating_mul(sim::MATCH_TICK_MS as u32) as i32;
        if let Some(package) = snapshot
            .meta
            .care_packages
            .iter()
            .filter(|package| now_ms >= package.ready_at_ms && now_ms < package.expires_at_ms)
            .filter(|package| {
                ps.origin
                    .iter()
                    .zip(package.origin)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f32>()
                    <= gamemode_iw4::killstreaks::CRATE_USE_RADIUS.powi(2)
            })
            .min_by(|a, b| {
                let distance = |package: &sim::CarePackage| {
                    ps.origin
                        .iter()
                        .zip(package.origin)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f32>()
                };
                distance(a).total_cmp(&distance(b))
            })
        {
            let key = match package.contents {
                gamemode_iw4::killstreaks::CrateContents::Ammo => "MP_AMMO_PICKUP",
                gamemode_iw4::killstreaks::CrateContents::Streak(
                    gamemode_iw4::killstreaks::Killstreak::Uav,
                ) => "MP_UAV_PICKUP",
                gamemode_iw4::killstreaks::CrateContents::Streak(
                    gamemode_iw4::killstreaks::Killstreak::PredatorMissile,
                ) => "MP_PREDATOR_MISSILE_PICKUP",
                gamemode_iw4::killstreaks::CrateContents::Streak(
                    gamemode_iw4::killstreaks::Killstreak::HelicopterFlares,
                ) => "MP_HELICOPTER_FLARES_PICKUP",
                gamemode_iw4::killstreaks::CrateContents::Streak(
                    gamemode_iw4::killstreaks::Killstreak::Airdrop,
                ) => "PLATFORM_GET_KILLSTREAK",
            };
            let localized = strings
                .as_ref()
                .and_then(|s| s.0.text(key))
                .ok_or_else(|| format!("missing {key}"))?;
            let unbound = strings
                .as_ref()
                .and_then(|s| s.0.text(hud_iw4::KEY_UNBOUND))
                .ok_or_else(|| "KEY_UNBOUND localization missing".to_owned())?;
            let hint = hud_iw4::replace_directive(localized, |cmd| {
                if matches!(cmd, "+activate" | "+usereload") {
                    bind.to_owned()
                } else {
                    hud_iw4::unbound_directive(unbound, cmd)
                }
            })
            .replace("&&1", bind);
            let (text, draw_key) = if package.capturer == Some(local.0) {
                let capture_time = if package.owner == local.0 {
                    gamemode_iw4::killstreaks::CRATE_OWNER_USE_MS
                } else {
                    gamemode_iw4::killstreaks::CRATE_OTHER_USE_MS
                };
                let capture_key = "MP_CAPTURING_CRATE";
                let capture_label = strings
                    .as_ref()
                    .and_then(|s| s.0.text(capture_key))
                    .ok_or_else(|| format!("missing {capture_key}"))?;
                let progress =
                    (package.capture_ms.clamp(0, capture_time) * 100 / capture_time.max(1)) as u32;
                (
                    format!("{capture_label} {progress}%"),
                    capture_key.to_owned(),
                )
            } else {
                (hint, key.to_owned())
            };
            return Ok(Some((text, draw_key, None)));
        }
        if ps.cursor_hint <= 4 {
            return Ok(None);
        }
        let weapon = (ps.cursor_hint - 4) as u32;
        let weapons = weapons
            .as_ref()
            .ok_or_else(|| "weapon catalog missing".to_owned())?;
        let strings = strings
            .as_ref()
            .ok_or_else(|| "localized strings missing".to_owned())?;
        let primary_count = ps
            .weapons
            .iter()
            .filter(|&&w| {
                w > 0
                    && weapons
                        .0
                        .facts_of(w as u32)
                        .is_some_and(|f| f.inventory_type == 0)
            })
            .count();
        let offhand = weapons
            .0
            .facts_of(weapon)
            .is_some_and(|f| f.offhand_class != 0);
        let key = if offhand || primary_count < 2 {
            "PLATFORM_PICKUPNEWWEAPON"
        } else {
            "PLATFORM_SWAPWEAPONS"
        };
        let template = strings
            .0
            .text(key)
            .ok_or_else(|| format!("missing {key}"))?;
        let name_key = weapons
            .0
            .display_name_key_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: display name missing"))?;
        let name = strings
            .0
            .text(name_key)
            .ok_or_else(|| format!("missing {name_key}"))?;
        let text = format!("{} {}", template.replace("&&1", bind), name);
        let (image, ratio) = weapons
            .0
            .pickup_icon_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: pickup/hud icon unresolved"))?;
        let namespace = weapons
            .0
            .namespace_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: namespace missing"))?;
        Ok::<_, String>(Some((
            text,
            key.to_owned(),
            Some((
                namespace,
                image.to_owned(),
                ratio,
                ps.cursor_hint_dual_wield != 0,
            )),
        )))
    })();
    match result {
        Ok(Some(caption)) => {
            memory.caption = Some(caption);
            memory.last_seen = time.elapsed_secs_f64();
        }
        Ok(None) => {}
        Err(reason) => {
            gaps.raise(GapCause::CursorHintMissing { reason });
            memory.caption = None;
            return;
        }
    }

    let alpha = (1.0 - (time.elapsed_secs_f64() - memory.last_seen) / 0.1).clamp(0.0, 1.0) as f32;
    let Some((text, key, icon)) = memory.caption.as_ref().filter(|_| alpha > 0.0) else {
        return;
    };
    let Some(catalog) = catalog else {
        gaps.raise(GapCause::NoFontCatalog);
        return;
    };
    let Some(item) = catalog
        .get("hud_fullscreen")
        .and_then(|m| m.items.iter().find(|i| i.owner_draw == 72))
    else {
        gaps.raise(GapCause::CursorHintMissing {
            reason: "hud_fullscreen ownerDraw 72 missing".into(),
        });
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        gaps.raise(GapCause::FontMissing {
            name: font_name.to_owned(),
        });
        return;
    };
    let nscale = hud_iw4::r_normalized_text_scale(font.pixel_height, item.text_scale);

    let width = crate::chrome::ui_text_width(font, text, item.text_scale)
        - if icon.is_some() {
            crate::chrome::ui_text_width(font, " ", item.text_scale)
        } else {
            0.0
        };
    let height = hud_iw4::ui_text_height(item.text_scale);
    let horz = i32::from(item.rect.horz_align);
    let vert = i32::from(item.rect.vert_align);
    let rect = surface.apply_rect(
        -width * 0.5,
        item.rect.y + height * 0.5,
        nscale,
        nscale,
        horz,
        vert,
    );
    let mut color = item.fore_color;
    color[3] *= alpha;
    let mut cmds = vec![Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material: assets::AssetRef::bare_name(&font.material).to_owned(),
        op: Draw2dOp::TextRun {
            font: font_name.into(),
            scale: nscale,
            text: text.clone(),
            loc_key: key.clone(),
            style: item.text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site: "use_hint" },
        layer: 1,
    }];
    if let Some((namespace, image, ratio, dual)) = icon {
        if hud_images.get(*namespace, image, &mut images).is_none() {
            gaps.raise(GapCause::CursorHintMissing {
                reason: format!("image {image}: {:?}", hud_images.miss_reason()),
            });
            return;
        }
        let (sx, sy) = match ratio {
            0 => (1.0, 1.0),
            1 => (2.0, 1.0),
            _ => (2.0, 0.5),
        };
        let (w, h) = (item.rect.w * sx, item.rect.h * sy);
        let y = item.rect.y - h * 0.5 + height * (if *dual { sx } else { 1.0 }) * 1.5;
        let placement = if *dual {
            let offset = (h - w) * 0.5;
            vec![(-h + offset, w), (h - offset, -w)]
        } else {
            vec![(-w * 0.5, w)]
        };
        for (x, w) in placement {
            let rect = surface.apply_rect(x, y, w, h, horz, vert);
            cmds.push(Draw2dCmd {
                material_namespace: *namespace,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color,
                material: image.clone(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw { site: "use_hint" },
                layer: 1,
            });
        }
    }
    let mut fonts = HashMap::new();
    fonts.insert(font_name.to_owned(), font);
    let (mut quads, _) = tessellate_fonts(&Draw2dList { cmds }, &fonts);
    if icon.as_ref().is_some_and(|(_, _, _, dual)| *dual) {
        let start = quads.len().saturating_sub(2);
        for (q, sign) in quads[start..].iter_mut().zip([1.0, -1.0]) {
            let center = [
                (q.xy[0][0] + q.xy[2][0]) * 0.5,
                (q.xy[0][1] + q.xy[2][1]) * 0.5,
            ];
            for point in &mut q.xy {
                let x = point[0] - center[0];
                let y = point[1] - center[1];
                *point = [center[0] - sign * y, center[1] + sign * x];
            }
        }
    }
    if let TessJob::Quads(existing) = &mut pass.use_hint {
        existing.extend(quads);
    } else {
        pass.use_hint = TessJob::Quads(quads);
    }
}
