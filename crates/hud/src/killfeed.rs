use std::collections::{HashMap, HashSet, VecDeque};

use assets::{MenuCatalog, PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use hud_iw4::{
    GAME_MSG_WIN0_HORZ_ALIGN, GAME_MSG_WIN0_LINE_COUNT, GAME_MSG_WIN0_MSG_TIME_MS,
    GAME_MSG_WIN0_TEXT_SCALE, GAME_MSG_WIN0_VERT_ALIGN, GAME_MSG_WIN0_X, KILLICON_DIED,
    game_msg_win0_line_y, gamenotify_line, killicon_stretch_uv, killicon_virtual_size,
    obituary_mod, obituary_mod_killicon, r_normalized_text_scale,
};
use net::LocalPresentClient;

use crate::chrome::r_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::scorebar::sys_milliseconds;

#[derive(Clone, Debug)]
enum KillfeedLine {
    Obituary {
        start_ms: i32,
        icon: String,
        icon_namespace: assets::AssetNamespace,
        attacker: String,
        has_attacker: bool,
        victim: String,

        kill_icon_ratio: i32,

        flip_kill_icon: bool,
    },
    Notify {
        start_ms: i32,
        text: String,
        name_empty: bool,
    },
}

impl KillfeedLine {
    fn start_ms(&self) -> i32 {
        match self {
            Self::Obituary { start_ms, .. } | Self::Notify { start_ms, .. } => *start_ms,
        }
    }
}

struct KillIconPick {
    stem: String,

    namespace: assets::AssetNamespace,
    ratio: i32,
    flip: bool,
}

#[derive(Resource, Default)]
pub(crate) struct KillfeedWindow {
    lines: VecDeque<KillfeedLine>,
    seen: HashSet<u32>,
}

#[derive(Component)]
pub(crate) struct KillfeedRaster;

pub(crate) fn spawn_killfeed(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, KillfeedRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.killfeed = TessJob::Hide;
}

fn pick_kill_icon(
    payload: &sim::EntityEventPayload,
    weapons: Option<&PreparedWeapons>,
) -> KillIconPick {
    let chrome = crate::images::HUD_CHROME_NAMESPACE;

    if let Some(mod_) = obituary_mod(payload.event_parm) {
        let Some(stem) = obituary_mod_killicon(mod_) else {
            return KillIconPick {
                stem: String::new(),
                namespace: chrome,
                ratio: 0,
                flip: false,
            };
        };
        return KillIconPick {
            stem: stem.to_owned(),
            namespace: chrome,
            ratio: 0,
            flip: false,
        };
    }
    let weapon = payload.event_parm as u32;
    let (ratio, flip) = match weapons.and_then(|reg| reg.0.facts_of(weapon)) {
        Some(facts) => (facts.kill_icon_ratio, facts.flip_kill_icon),
        None => (0, false),
    };
    let (stem, namespace) = if let Some(reg) = weapons {
        let ns = reg.0.namespace_of(weapon).unwrap_or(chrome);
        if let Some(image) = reg.0.kill_icon_image_of(weapon) {
            (image.to_owned(), ns)
        } else if let Some(name) = reg.0.kill_icon_of(weapon) {
            (name.to_owned(), ns)
        } else {
            (KILLICON_DIED.to_owned(), chrome)
        }
    } else {
        (KILLICON_DIED.to_owned(), chrome)
    };
    KillIconPick {
        stem,
        namespace,
        ratio,
        flip,
    }
}

fn snapshot_client_name(presented: &net::PresentedSnapshot, client: i32) -> String {
    if client < 0 {
        return String::new();
    }
    let Some(snap) = presented.snapshot() else {
        return String::new();
    };
    let Some(meta) = snap.meta.for_client(sim::ClientId(client as u32)) else {
        return String::new();
    };
    match entity_iw4::client_state_name(&meta.name) {
        Some(s) => s.to_owned(),
        None => String::new(),
    }
}

pub(crate) fn cg_obituary(
    obituary: On<net::EntityObituary>,
    mut window: ResMut<KillfeedWindow>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<net::PresentedSnapshot>,
) {
    let payload = obituary.event.payload;
    let pick = pick_kill_icon(&payload, weapons.as_deref());
    let attacker = snapshot_client_name(&presented, payload.attacker_entity_num);
    let victim = snapshot_client_name(&presented, payload.number);
    let now = sys_milliseconds() as i32;
    window.lines.push_back(KillfeedLine::Obituary {
        start_ms: now,
        icon: pick.stem,
        icon_namespace: pick.namespace,
        attacker,
        has_attacker: (0..18).contains(&payload.attacker_entity_num)
            && payload.attacker_entity_num != payload.number,
        victim,
        kill_icon_ratio: pick.ratio,
        flip_kill_icon: pick.flip,
    });
    while window.lines.len() > GAME_MSG_WIN0_LINE_COUNT {
        window.lines.pop_front();
    }
}

fn text_cmd(
    x: f32,
    y: f32,
    cmd_w: f32,
    cmd_h: f32,
    material: String,
    text: String,
    site: &'static str,
) -> Draw2dCmd {
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (x + 0.5).floor(),
        y: (y + 0.5).floor(),
        w: cmd_w,
        h: cmd_h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color: [1.0, 1.0, 1.0, 1.0],
        material,
        op: Draw2dOp::TextRun {
            font: HUD_SMALL_FONT.to_owned(),
            scale: cmd_w,
            text,
            loc_key: String::new(),

            style: crate::draw2d::TEXT_STYLE_UNREAD,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site },
        layer: 1,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_killfeed(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut window: ResMut<KillfeedWindow>,
    mut pass: ResMut<HudTessPass>,
    mut notifies: MessageReader<net::SvcGameNotify>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    if presented.player(local.0).is_none() {
        window.lines.clear();
        window.seen.clear();
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }

    let now = sys_milliseconds() as i32;
    for cmd in notifies.read() {
        if !window.seen.insert(cmd.id) {
            continue;
        }
        let Some(template) = strings
            .as_ref()
            .and_then(|s| s.0.text(&cmd.key).map(str::to_owned))
        else {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: cmd.key.clone(),
            });
            continue;
        };
        window.lines.push_back(KillfeedLine::Notify {
            start_ms: now,
            text: gamenotify_line(&template, &cmd.name),
            name_empty: cmd.name.is_empty(),
        });
        while window.lines.len() > GAME_MSG_WIN0_LINE_COUNT {
            window.lines.pop_front();
        }
    }
    window
        .lines
        .retain(|line| now.saturating_sub(line.start_ms()) < GAME_MSG_WIN0_MSG_TIME_MS);
    if window.lines.is_empty() {
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }

    let Some(newest) = window.lines.back() else {
        hide(&mut pass);
        return;
    };
    let names_ok = match newest {
        KillfeedLine::Obituary {
            has_attacker,
            attacker,
            victim,
            ..
        } => (!*has_attacker || !attacker.is_empty()) && !victim.is_empty(),
        KillfeedLine::Notify { text, .. } => !text.is_empty(),
    };
    match newest {
        KillfeedLine::Obituary { .. } => {
            if !names_ok {
                gaps.raise(GapCause::ObituaryNoClientInfo);
            } else {
                gaps.clear(HudGap::Obituary);
            }
        }
        KillfeedLine::Notify { name_empty, .. } => {
            if *name_empty {
                gaps.raise(GapCause::GameNotifyNoClientInfo);
            } else if names_ok {
                gaps.clear(HudGap::Obituary);
            }
        }
    }

    if matches!(newest, KillfeedLine::Obituary { .. })
        && presented
            .snapshot()
            .and_then(|s| s.meta.for_client(local.0))
            .is_some_and(|meta| matches!(meta.client_state_team, 1 | 2))
    {
        gaps.raise(GapCause::ObituaryTeamColors);
    }
    let font = catalog.as_ref().and_then(|c| c.font(HUD_SMALL_FONT));
    let (nscale, font_material) = match font {
        Some(def) => (
            r_normalized_text_scale(def.pixel_height, GAME_MSG_WIN0_TEXT_SCALE),
            assets::AssetRef::bare_name(&def.material).to_owned(),
        ),
        None => (0.0, String::new()),
    };
    if names_ok && font.is_none() {
        gaps.raise(GapCause::ObituaryNoClientInfo);
    }

    let miss = hud_images.miss_reason();
    let mut cmds = Vec::new();
    let mut fonts = HashMap::new();
    let font_tex_ok = if let Some(def) = font {
        fonts.insert(HUD_SMALL_FONT.to_owned(), def);
        hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &font_material,
                &mut images,
            )
            .is_some()
    } else {
        false
    };
    if names_ok && font.is_some() && !font_tex_ok {
        let image = hud_images
            .zone_image_name(&font_material)
            .map(str::to_owned);
        gaps.raise(GapCause::FontAtlasMissing {
            material: font_material.clone(),
            image,
        });
    }

    let mut newest_icon_ok = true;
    for (i, line) in window.lines.iter().rev().enumerate() {
        let first_cmd = cmds.len();
        let age = now.saturating_sub(line.start_ms());

        let alpha = (age as f32 / 250.0).clamp(0.0, 1.0)
            * ((GAME_MSG_WIN0_MSG_TIME_MS - age) as f32 / 500.0).clamp(0.0, 1.0);
        let y_virtual = game_msg_win0_line_y(i);
        match line {
            KillfeedLine::Notify { text, .. } => {
                if font.is_some() && font_tex_ok {
                    let applied = surface.apply_rect(
                        GAME_MSG_WIN0_X,
                        y_virtual,
                        nscale,
                        nscale,
                        GAME_MSG_WIN0_HORZ_ALIGN,
                        GAME_MSG_WIN0_VERT_ALIGN,
                    );
                    cmds.push(text_cmd(
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        text.clone(),
                        "killfeed_game_msg",
                    ));
                }
            }
            KillfeedLine::Obituary {
                icon,
                icon_namespace,
                attacker,
                has_attacker,
                victim,
                kill_icon_ratio,
                flip_kill_icon,
                ..
            } => {
                let mut x_virtual = GAME_MSG_WIN0_X;
                let line_names = (!*has_attacker || !attacker.is_empty())
                    && !victim.is_empty()
                    && font.is_some()
                    && font_tex_ok;
                if line_names && *has_attacker {
                    if let Some(def) = font {
                        let attacker_w = r_text_width(def, attacker) as f32 * nscale;
                        let applied = surface.apply_rect(
                            x_virtual,
                            y_virtual,
                            nscale,
                            nscale,
                            GAME_MSG_WIN0_HORZ_ALIGN,
                            GAME_MSG_WIN0_VERT_ALIGN,
                        );
                        cmds.push(text_cmd(
                            applied.x,
                            applied.y,
                            applied.w,
                            applied.h,
                            font_material.clone(),
                            attacker.clone(),
                            "killfeed_obituary",
                        ));
                        x_virtual += attacker_w + GAME_MSG_WIN0_TEXT_SCALE * 4.0;
                    }
                }

                let (icon_vw, icon_vh) = killicon_virtual_size(*kill_icon_ratio);
                let (s0, s1) = killicon_stretch_uv(*flip_kill_icon);
                let placed = surface.apply_rect(
                    x_virtual,
                    y_virtual - icon_vh,
                    icon_vw,
                    icon_vh,
                    GAME_MSG_WIN0_HORZ_ALIGN,
                    GAME_MSG_WIN0_VERT_ALIGN,
                );
                let icon_ok = hud_images.get(*icon_namespace, icon, &mut images).is_some();
                if i == 0 {
                    newest_icon_ok = icon_ok;
                }
                if !icon_ok {
                    gaps.raise(GapCause::ObituaryKillIconMissing {
                        name: icon.clone(),
                        miss,
                    });
                } else {
                    cmds.push(Draw2dCmd {
                        material_namespace: *icon_namespace,
                        x: (placed.x + 0.5).floor(),
                        y: (placed.y + 0.5).floor(),
                        w: placed.w,
                        h: placed.h,
                        s0,
                        t0: 0.0,
                        s1,
                        t1: 1.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        material: icon.clone(),
                        op: Draw2dOp::StretchPic,
                        provenance: Draw2dProvenance::CgDraw {
                            site: "killfeed_obituary",
                        },
                        layer: 1,
                    });
                }
                if line_names {
                    x_virtual += icon_vw + GAME_MSG_WIN0_TEXT_SCALE * 4.0;
                    let applied = surface.apply_rect(
                        x_virtual,
                        y_virtual,
                        nscale,
                        nscale,
                        GAME_MSG_WIN0_HORZ_ALIGN,
                        GAME_MSG_WIN0_VERT_ALIGN,
                    );
                    cmds.push(text_cmd(
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        victim.clone(),
                        "killfeed_obituary",
                    ));
                }
            }
        }
        for cmd in &mut cmds[first_cmd..] {
            cmd.color[3] *= alpha;
        }
    }

    if !newest_icon_ok {
        hide(&mut pass);
        return;
    }

    let list = Draw2dList { cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.killfeed = TessJob::Quads(quads);
}
