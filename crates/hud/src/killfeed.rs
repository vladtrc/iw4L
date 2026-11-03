use std::collections::{HashMap, HashSet, VecDeque};

use asset_game::MenuCatalog;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use hud_iw4::{
    GAME_MSG_WIN0_FADE_IN_TIME_MS, GAME_MSG_WIN0_FADE_OUT_TIME_MS, GAME_MSG_WIN0_LINE_COUNT,
    GAME_MSG_WIN0_MSG_TIME_MS, GAME_MSG_WIN0_SCROLL_TIME_MS, KILLICON_DIED, gamenotify_line,
    killicon_stretch_uv, killicon_virtual_size, normalized_text_scale, obituary_mod,
    obituary_mod_killicon,
};
use net::FrameClock;

use crate::chrome::text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImageLoader;

#[derive(Clone, Debug)]
enum KillfeedLine {
    Obituary {
        start_ms: i32,
        end_ms: i32,
        icon: String,
        icon_namespace: asset_core::FamilyId,
        attacker: String,
        has_attacker: bool,
        victim: String,
        attacker_team: i32,
        victim_team: i32,

        kill_icon_ratio: i32,

        flip_kill_icon: bool,
    },
    Notify {
        start_ms: i32,
        end_ms: i32,
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

    fn end_ms(&self) -> i32 {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => *end_ms,
        }
    }

    fn fade_out(&mut self, now: i32) {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => {
                *end_ms = (*end_ms).min(now.saturating_add(GAME_MSG_WIN0_FADE_OUT_TIME_MS));
            }
        }
    }
}

struct KillIconPick {
    stem: String,

    namespace: asset_core::FamilyId,
    ratio: i32,
    flip: bool,
}

#[derive(Resource, Default)]
pub(crate) struct KillfeedWindow {
    lines: VecDeque<KillfeedLine>,
    bold: VecDeque<(i32, String)>,
    seen: HashSet<u32>,
}

impl KillfeedWindow {
    fn push(&mut self, line: KillfeedLine, now: i32) {
        self.lines.retain(|line| now < line.end_ms());
        self.lines.push_back(line);
        while self.lines.len() > GAME_MSG_WIN0_LINE_COUNT + 3 {
            self.lines.pop_front();
        }
        if self.lines.len() > GAME_MSG_WIN0_LINE_COUNT {
            let fading = self.lines.len() - GAME_MSG_WIN0_LINE_COUNT - 1;
            self.lines[fading].fade_out(now);
        }
    }
}

const BOLD_LINE_COUNT: usize = 3;
const BOLD_MSG_TIME_MS: i32 = 3000;
const BOLD_TEXT_SCALE: f32 = 0.5;
const BOLD_Y: f32 = 100.0;

#[derive(Component)]
pub(crate) struct KillfeedRaster;

pub(crate) fn spawn_killfeed(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, KillfeedRaster);
}

fn pick_kill_icon(
    payload: &sim::EntityEventPayload,
    weapons: Option<&assets::BoundWeapons<'_>>,
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
    let fallback = KillIconPick {
        stem: KILLICON_DIED.to_owned(),
        namespace: chrome,
        ratio: 0,
        flip: false,
    };
    let Some(reg) = weapons else {
        return fallback;
    };
    let Some(stem) = reg
        .registry()
        .kill_icon_image_of(weapon)
        .or_else(|| reg.registry().kill_icon_of(weapon))
    else {
        return fallback;
    };
    let (ratio, flip) = match reg.row(weapon).and_then(|weapon| weapon.hud_facts()) {
        Some(facts) => (facts.kill_icon_ratio, facts.flip_kill_icon),
        None => (0, false),
    };
    KillIconPick {
        stem: stem.to_owned(),
        namespace: reg
            .registry()
            .component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            .unwrap_or(chrome),
        ratio,
        flip,
    }
}

fn snapshot_client_team(presented: &net::PresentedSnapshot, client: i32) -> i32 {
    if client < 0 {
        return 0;
    }
    presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(sim::ClientId(client as u32)))
        .map_or(0, |meta| meta.client_state_team)
}

pub fn team_name_color(local_team: i32, team: i32, dvars: sim::ScriptDvars<'_>) -> [f32; 4] {
    if !matches!(local_team, 1 | 2) || !matches!(team, 1 | 2) {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let (name, default) = if dvars
        .int("useRelativeTeamColors")
        .is_some_and(|value| value != 0)
    {
        if team == local_team {
            ("g_TeamColor_MyTeam", gamemode_iw4::TEAM_COLOR_MY_TEAM)
        } else {
            ("g_TeamColor_EnemyTeam", gamemode_iw4::TEAM_COLOR_ENEMY_TEAM)
        }
    } else if team == 1 {
        ("g_TeamColor_Axis", gamemode_iw4::TEAM_COLOR_AXIS)
    } else {
        ("g_TeamColor_Allies", gamemode_iw4::TEAM_COLOR_ALLIES)
    };
    let parse = |rgb: &str| {
        let mut values = rgb.split_whitespace().map(str::parse::<f32>);
        let rgb = [
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
        ];
        (values.next().is_none() && rgb.iter().all(|value| value.is_finite())).then(|| {
            [
                rgb[0].clamp(0.0, 1.0),
                rgb[1].clamp(0.0, 1.0),
                rgb[2].clamp(0.0, 1.0),
                1.0,
            ]
        })
    };
    dvars
        .string(name)
        .and_then(parse)
        .or_else(|| parse(default))
        .unwrap_or([1.0; 4])
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

pub(crate) fn obituary(
    obituary: On<net::EntityObituary>,
    mut window: ResMut<KillfeedWindow>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<net::PresentedSnapshot>,
    clock: Res<FrameClock>,
) {
    if obituary.in_killcam {
        return;
    }
    let payload = obituary.event.payload;
    let bound = weapons.as_deref().map(|weapons| weapons.bound());
    let pick = pick_kill_icon(&payload, bound.as_ref());
    let attacker = snapshot_client_name(&presented, payload.attacker_entity_num);
    let victim = snapshot_client_name(&presented, payload.other_entity_num);
    let attacker_team = snapshot_client_team(&presented, payload.attacker_entity_num);
    let victim_team = snapshot_client_team(&presented, payload.other_entity_num);
    let now = clock.time();
    window.push(
        KillfeedLine::Obituary {
            start_ms: now,
            end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
            icon: pick.stem,
            icon_namespace: pick.namespace,
            attacker,
            has_attacker: (0..18).contains(&payload.attacker_entity_num)
                && payload.attacker_entity_num != payload.other_entity_num,
            victim,
            attacker_team,
            victim_team,
            kill_icon_ratio: pick.ratio,
            flip_kill_icon: pick.flip,
        },
        now,
    );
}

fn text_cmd(
    font_name: &str,
    text_style: i32,
    rect: hud_iw4::AppliedRect,
    material: String,
    text: String,
    color: [f32; 4],
    site: &'static str,
) -> Draw2dCmd {
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (rect.x + 0.5).floor(),
        y: (rect.y + 0.5).floor(),
        w: rect.w,
        h: rect.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material,
        op: Draw2dOp::TextRun {
            font: font_name.to_owned(),
            scale: rect.w,
            text,
            loc_key: String::new(),

            style: text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site },
        layer: 1,
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct KillfeedDisplay<'w, 's> {
    clock: Res<'w, FrameClock>,
    notifies: MessageReader<'w, 's, net::SvcGameNotify>,
    window: ResMut<'w, KillfeedWindow>,
    hud_images: HudImageLoader<'w>,
    pass: ResMut<'w, HudTessPass>,
}

impl KillfeedDisplay<'_, '_> {
    fn hide(&mut self) {
        self.pass.killfeed = TessJob::Hide;
    }

    fn update(
        &mut self,
        surface: Res<crate::surface::Hud2dSurface>,
        presentation: net::LocalPresentation,
        catalog: Option<Res<MenuCatalog>>,
        strings: Option<Res<PreparedLocalizedStrings>>,
        view: Option<Res<frame::ViewSubject>>,
    ) {
        if !surface.is_ready() {
            self.hide();
            return;
        }
        if presentation.player().is_none() {
            self.window.lines.clear();
            self.window.bold.clear();
            self.window.seen.clear();

            self.hide();
            return;
        }

        let now = self.clock.time();
        for cmd in self.notifies.read() {
            if cmd.tag == net::SVC_CHAT {
                continue;
            }
            if !self.window.seen.insert(cmd.id) {
                continue;
            }
            let Some(template) = strings
                .as_ref()
                .and_then(|s| crate::hudelem::resolve_hud_text(s, &cmd.key))
            else {
                diag::warn_once!(
                    Ui,
                    "key `{key}` has no row in the loaded language",
                    key = cmd.key.clone()
                );
                continue;
            };
            let args: Vec<String> = cmd
                .name
                .split(sim::HUD_PRINT_ARG_SEPARATOR)
                .map(|arg| {
                    strings
                        .as_ref()
                        .and_then(|s| crate::hudelem::resolve_hud_text(s, arg))
                        .unwrap_or_else(|| arg.to_owned())
                })
                .collect();
            let text = if template.contains("&&2") {
                args.iter().enumerate().fold(template, |line, (i, arg)| {
                    line.replace(&format!("&&{}", i + 1), arg)
                })
            } else {
                gamenotify_line(&template, &args[0])
            };
            if cmd.tag == net::SVC_PRINT_BOLD {
                self.window.bold.push_back((now, text));
                while self.window.bold.len() > BOLD_LINE_COUNT {
                    self.window.bold.pop_front();
                }
                continue;
            }
            self.window.push(
                KillfeedLine::Notify {
                    start_ms: now,
                    end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
                    text,
                    name_empty: cmd.tag == net::SVC_DISCONNECT_NOTIFY && cmd.name.is_empty(),
                },
                now,
            );
        }
        self.window.lines.retain(|line| now < line.end_ms());
        self.window
            .bold
            .retain(|(start, _)| now.saturating_sub(*start) < BOLD_MSG_TIME_MS);
        if self.window.lines.is_empty() && self.window.bold.is_empty() {
            self.hide();
            return;
        }
        if view.is_some_and(|v| v.in_killcam()) {
            self.hide();
            return;
        }

        let names_ok = match self.window.lines.back() {
            None => true,
            Some(KillfeedLine::Obituary {
                has_attacker,
                attacker,
                victim,
                ..
            }) => (!*has_attacker || !attacker.is_empty()) && !victim.is_empty(),
            Some(KillfeedLine::Notify { text, .. }) => !text.is_empty(),
        };
        match self.window.lines.back() {
            None => {}
            Some(KillfeedLine::Obituary { .. }) => {
                if !names_ok {
                    diag::warn_once!(Ui, "clientState.name empty; killfeed icon only");
                }
            }
            Some(KillfeedLine::Notify { name_empty, .. }) => {
                if *name_empty {
                    diag::warn_once!(Ui, "clientState.name empty; gamenotify has no name");
                }
            }
        }

        let local_team =
            snapshot_client_team(&presentation.presented, presentation.client_id().0 as i32);
        let Some(snapshot) = presentation.snapshot() else {
            self.hide();
            return;
        };
        let dvars = snapshot.meta.script_dvars(presentation.client_id());
        let Some(item) = catalog
            .as_ref()
            .and_then(|c| c.get("hud_fullscreen"))
            .and_then(|menu| {
                menu.items.iter().find(|item| {
                    item.item_type == hud_iw4::ITEM_TYPE_GAME_MESSAGE_WINDOW
                        && item.game_msg_window_index == 0
                })
            })
        else {
            self.hide();
            return;
        };
        let font_name = hud_iw4::ui_get_font_handle(
            item.font_enum,
            surface.scale_virtual_to_real()[1],
            item.text_scale,
        );
        let font = catalog.as_ref().and_then(|c| c.font(font_name));
        let (nscale, font_material) = match font {
            Some(def) => (
                normalized_text_scale(def.pixel_height, item.text_scale),
                asset_core::AssetRef::bare_name(&def.material).to_owned(),
            ),
            None => (0.0, String::new()),
        };
        if names_ok && font.is_none() {
            diag::warn_once!(Ui, "clientState.name empty; killfeed icon only");
        }

        let miss = self.hud_images.miss_reason();
        let mut cmds = Vec::new();
        let mut fonts = HashMap::new();
        let font_tex_ok = if let Some(def) = font {
            fonts.insert(font_name.to_owned(), def);
            self.hud_images
                .load(crate::images::HUD_CHROME_NAMESPACE, &font_material)
                .is_some()
        } else {
            false
        };
        if names_ok && font.is_some() && !font_tex_ok {
            let image = self
                .hud_images
                .zone_image_name(&font_material)
                .map(str::to_owned);
            diag::warn_once!(Ui, "{}", {
                let material: String = font_material.clone();
                let image: Option<String> = image;
                match image {
                    Some(image) => {
                        format!("font material `{material}` image `{image}` did not upload")
                    }
                    None => format!("font material `{material}` has no zone atlas"),
                }
            });
        }

        let line_height = (item.text_scale * hud_iw4::GAME_MSG_CHAR_EM + 0.5).floor();
        let icon_spacing = font.map_or(0.0, |def| text_width(def, " ") as f32 * nscale);
        let scroll_offset: f32 = self
            .window
            .lines
            .iter()
            .map(|line| {
                let age = now.saturating_sub(line.start_ms());
                let fraction =
                    (1.0 - age as f32 / GAME_MSG_WIN0_SCROLL_TIME_MS as f32).clamp(0.0, 1.0);
                (line_height * fraction + 0.5).floor()
            })
            .sum();
        for (i, line) in self.window.lines.iter().rev().enumerate() {
            let first_cmd = cmds.len();
            let age = now.saturating_sub(line.start_ms());

            let alpha = (age as f32 / GAME_MSG_WIN0_FADE_IN_TIME_MS as f32).clamp(0.0, 1.0)
                * ((line.end_ms() - now) as f32 / GAME_MSG_WIN0_FADE_OUT_TIME_MS as f32)
                    .clamp(0.0, 1.0)
                * item.fore_color[3];
            let y_virtual = item.rect.y + scroll_offset - line_height * (i as f32 + 1.0);
            match line {
                KillfeedLine::Notify { text, .. } => {
                    if font.is_some() && font_tex_ok {
                        let applied = surface.apply_rect(
                            item.rect.x,
                            y_virtual,
                            nscale,
                            nscale,
                            i32::from(item.rect.horz_align),
                            i32::from(item.rect.vert_align),
                        );
                        cmds.push(text_cmd(
                            font_name,
                            item.text_style,
                            applied,
                            font_material.clone(),
                            text.clone(),
                            [
                                item.fore_color[0],
                                item.fore_color[1],
                                item.fore_color[2],
                                1.0,
                            ],
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
                    attacker_team,
                    victim_team,
                    kill_icon_ratio,
                    flip_kill_icon,
                    ..
                } => {
                    let mut x_virtual = item.rect.x;
                    let line_names = (!*has_attacker || !attacker.is_empty())
                        && !victim.is_empty()
                        && font.is_some()
                        && font_tex_ok;
                    if line_names && *has_attacker {
                        if let Some(def) = font {
                            let attacker_w = text_width(def, attacker) as f32 * nscale;
                            let applied = surface.apply_rect(
                                x_virtual,
                                y_virtual,
                                nscale,
                                nscale,
                                i32::from(item.rect.horz_align),
                                i32::from(item.rect.vert_align),
                            );
                            cmds.push(text_cmd(
                                font_name,
                                item.text_style,
                                applied,
                                font_material.clone(),
                                attacker.clone(),
                                team_name_color(local_team, *attacker_team, dvars),
                                "killfeed_obituary",
                            ));
                            x_virtual += attacker_w + icon_spacing;
                        }
                    }

                    let (icon_vw, icon_vh) = killicon_virtual_size(*kill_icon_ratio);
                    let icon_scale = item.text_scale / hud_iw4::GAME_MSG_WIN0_TEXT_SCALE;
                    let (icon_vw, icon_vh) = (icon_vw * icon_scale, icon_vh * icon_scale);
                    let (s0, s1) = killicon_stretch_uv(*flip_kill_icon);
                    let placed = surface.apply_rect(
                        x_virtual,
                        y_virtual - icon_vh,
                        icon_vw,
                        icon_vh,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    let icon_ok = self.hud_images.load(*icon_namespace, icon).is_some();
                    if !icon_ok {
                        diag::warn_once!(
                            Ui,
                            "killfeed icon `{name}` is {miss}",
                            name = icon.clone()
                        );
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
                        x_virtual += icon_vw + icon_spacing;
                        let applied = surface.apply_rect(
                            x_virtual,
                            y_virtual,
                            nscale,
                            nscale,
                            i32::from(item.rect.horz_align),
                            i32::from(item.rect.vert_align),
                        );
                        cmds.push(text_cmd(
                            font_name,
                            item.text_style,
                            applied,
                            font_material.clone(),
                            victim.clone(),
                            team_name_color(local_team, *victim_team, dvars),
                            "killfeed_obituary",
                        ));
                    }
                }
            }
            for cmd in &mut cmds[first_cmd..] {
                cmd.color[3] *= alpha;
            }
        }
        if let Some(def) = catalog
            .as_ref()
            .and_then(|c| c.font(HUD_SMALL_FONT))
            .filter(|_| !self.window.bold.is_empty())
        {
            let font_material = asset_core::AssetRef::bare_name(&def.material).to_owned();
            fonts.insert(HUD_SMALL_FONT.to_owned(), def);
            let bold_font_tex_ok = self
                .hud_images
                .load(crate::images::HUD_CHROME_NAMESPACE, &font_material)
                .is_some();
            let scale = normalized_text_scale(def.pixel_height, BOLD_TEXT_SCALE);
            for (i, (start, text)) in self
                .window
                .bold
                .iter()
                .enumerate()
                .filter(|_| bold_font_tex_ok)
            {
                let age = now.saturating_sub(*start);
                let alpha = ((BOLD_MSG_TIME_MS - age) as f32 / 500.0).clamp(0.0, 1.0);
                let width = text_width(def, text) as f32 * scale;
                let line_h = def.pixel_height as f32 * scale;
                let applied = surface.apply_rect(
                    -width / 2.0,
                    BOLD_Y + line_h * i as f32,
                    scale,
                    scale,
                    hud_iw4::ALIGN_CENTER,
                    hud_iw4::ALIGN_VIEWABLE,
                );
                cmds.push(text_cmd(
                    HUD_SMALL_FONT,
                    hud_iw4::GAME_MSG_WIN0_TEXT_STYLE,
                    applied,
                    font_material.clone(),
                    text.clone(),
                    [1.0, 1.0, 1.0, alpha],
                    "killfeed_bold_msg",
                ));
            }
        }

        let list = Draw2dList { cmds };
        let (quads, _) = tessellate_fonts(&list, &fonts);
        if quads.is_empty() {
            self.hide();
            return;
        }
        self.pass.killfeed = TessJob::Quads(quads);
    }
}

pub(crate) fn update_killfeed(
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: Option<Res<frame::ViewSubject>>,
    mut display: KillfeedDisplay,
) {
    display.update(surface, presentation, catalog, strings, view);
}
