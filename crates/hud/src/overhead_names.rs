use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gpu_list::{HudTessPass, TessJob};
use assets::MenuCatalog;
use bevy::prelude::*;
use hud_iw4::*;
use net::{CEntity, CEntityRuntime, CgFrameClock, LocalPresentClient, PresentedSnapshot};

use std::collections::HashMap;

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverheadPosedHead {
    ExactWorld([f32; 3]),
    NoDObjOrHead,
}

#[derive(Resource, Debug, Default)]
pub struct OverheadPosedPlayerFrame {
    by_ent: HashMap<u16, OverheadPosedHead>,
}

impl OverheadPosedPlayerFrame {
    pub fn replace(&mut self, rows: impl IntoIterator<Item = (u16, OverheadPosedHead)>) {
        self.by_ent.clear();
        self.by_ent.extend(rows);
    }

    fn get(&self, entnum: u16) -> Option<OverheadPosedHead> {
        self.by_ent.get(&entnum).copied()
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverheadPosedPlayerFramePublished;

#[derive(Component)]
pub(crate) struct OverheadNamesRaster;

#[derive(Default)]
pub(crate) struct NameMemory {
    last_time: Option<i32>,
    seen: HashMap<u16, (i32, i32, bool)>,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<OverheadPosedPlayerFrame>().add_systems(
        Update,
        update_overhead_names
            .after(OverheadPosedPlayerFramePublished)
            .after(crate::surface::update_hud_surface)
            .before(crate::plugin::flush_overhead_names_tess)
            .before(crate::gaps::report_hud_gaps)
            .in_set(frame::LifeFrontPublished),
    );
}

fn name_color(local_team: i32, target_team: i32) -> [f32; 4] {
    if target_team == 3 {
        [0.65, 0.65, 0.65, 1.0]
    } else if local_team != 0 && local_team == target_team {
        [0.6, 0.8, 0.6, 1.0]
    } else {
        [0.75, 0.25, 0.25, 1.0]
    }
}

// Player-supplied formatting must not override friend/enemy identification.
fn plain_name(name: &str) -> String {
    let mut chars = name.chars().peekable();
    let mut plain = String::new();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else if !c.is_control() {
            // Removing an escape can join another caret and digit ("^^12").
            if c.is_ascii_digit() && plain.ends_with('^') {
                plain.pop();
            } else {
                plain.push(c);
            }
        }
    }
    plain
}

// Intersect the view ray with the presented player's bounds, including crouch/prone.
fn aimed_distance(eye: Vec3, forward: Vec3, origin: Vec3, head: Vec3) -> Option<f32> {
    let mins = origin.min(head) - Vec3::new(15.0, 15.0, 0.0);
    let maxs = origin.max(head) + Vec3::new(15.0, 15.0, 8.0);
    let mut near: f32 = 0.0;
    let mut far = CROSSHAIR_SCAN_DISTANCE;
    for axis in 0..3 {
        if forward[axis].abs() < 1e-6 {
            if eye[axis] < mins[axis] || eye[axis] > maxs[axis] {
                return None;
            }
        } else {
            let a = (mins[axis] - eye[axis]) / forward[axis];
            let b = (maxs[axis] - eye[axis]) / forward[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    (near <= far).then_some(near)
}

#[allow(clippy::too_many_arguments)]
fn update_overhead_names(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    posed: Res<OverheadPosedPlayerFrame>,
    players: Query<(&CEntity, &CEntityRuntime)>,
    cg_clock: Res<CgFrameClock>,
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    prediction: Option<Res<net::ClientPredictionState>>,
    view: Res<frame::ViewSubject>,
    mut pass: ResMut<HudTessPass>,
    mut memory: Local<NameMemory>,
    mut gaps: ResMut<HudPresentationGaps>,
) {
    pass.overhead_names = TessJob::Hide;
    gaps.clear(HudGap::OverheadNames);
    let now = cg_clock.time();
    if memory.last_time.is_some_and(|last| now < last) {
        memory.seen.clear();
    }
    memory.last_time = Some(now);
    let Some(snapshot) = presented.snapshot() else {
        memory.seen.clear();
        return;
    };
    if !surface.is_ready() || view.in_killcam() {
        memory.seen.clear();
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        memory.seen.clear();
        return;
    };
    let Some(local_meta) = snapshot.meta.for_client(local.0) else {
        return;
    };
    if cg_is_flashbanged(
        now,
        ps.shellshock_time,
        ps.shellshock_duration,
        SCREEN_BLEND_FLASHED,
    ) != 0
    {
        memory.seen.clear();
        return;
    }
    let Some((camera, transform)) = cameras.iter().find(|(c, _)| c.is_active) else {
        return;
    };
    let Some(world) = prediction
        .as_ref()
        .filter(|p| p.0.is_armed() && p.0.world().has_world_clip())
        .map(|p| p.0.world())
    else {
        gaps.raise(GapCause::OverheadVisibilityUnavailable);
        return;
    };
    let font_name = ui_get_font_handle(2, surface.scale_virtual_to_real()[1], 1.0);
    let Some(font) = catalog.as_ref().and_then(|c| c.font(font_name)) else {
        gaps.raise(GapCause::OverheadFontUnavailable);
        return;
    };
    let eye = transform.translation();
    let forward = *transform.forward();
    let mut candidates = Vec::new();
    for (identity, runtime) in &players {
        if !runtime.in_next_snap()
            || runtime.next_state.e_type != entity_iw4::ET_PLAYER
            || (runtime.next_state.e_flags & (0x20 | 0x20000)) != 0
        {
            continue;
        }
        let Some(client) = identity.client() else {
            continue;
        };
        if client == local.0 {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(client) else {
            continue;
        };
        if meta.lifecycle != sim::ClientLifecycle::Alive {
            continue;
        }
        let Some(name) = entity_iw4::client_state_name(&meta.name) else {
            continue;
        };
        let origin = Vec3::from_array(runtime.origin);
        let head = match posed.get(identity.number()) {
            Some(OverheadPosedHead::ExactWorld(head)) => Vec3::from_array(head),
            Some(OverheadPosedHead::NoDObjOrHead) => {
                origin + Vec3::Z * (OVERHEAD_ORIGIN_FALLBACK_Z - OVERHEAD_HEAD_LIFT)
            }
            None => {
                gaps.raise(GapCause::OverheadHeadUnavailable {
                    entnum: identity.number(),
                });
                continue;
            }
        };
        if eye.distance_squared(head) > OVERHEAD_MAX_DISTANCE_DEFAULT.powi(2) {
            continue;
        }
        let anchor = head + Vec3::Z * OVERHEAD_HEAD_LIFT;
        let Ok(pixel) = camera.world_to_viewport(transform, anchor) else {
            continue;
        };
        if !pixel.is_finite()
            || pixel.x < 0.0
            || pixel.y < 0.0
            || pixel.x > surface.width()
            || pixel.y > surface.height()
        {
            continue;
        }
        let hit = world.trace_world(
            eye.to_array(),
            head.to_array(),
            [0.0; 3],
            [0.0; 3],
            OVERHEAD_TRACE_MASK,
        );
        let visible = hit.fraction >= 1.0 && hit.startsolid == 0;
        let aim = aimed_distance(eye, forward, origin, head).filter(|_| visible);
        candidates.push((
            identity.number(),
            meta.client_state_team,
            plain_name(name),
            anchor,
            pixel,
            visible,
            aim,
            (meta.rank, meta.prestige, client),
        ));
    }
    let aimed = candidates
        .iter()
        .filter_map(|c| c.6.map(|d| (c.0, d)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|c| c.0);
    memory
        .seen
        .retain(|ent, _| candidates.iter().any(|c| c.0 == *ent));
    let mut list = Draw2dList::default();
    for (ent, team, name, anchor, pixel, visible, _, (rank, prestige, client)) in candidates {
        let friendly = local_meta.client_state_team != 0 && local_meta.client_state_team == team;
        let active = visible && (friendly || aimed == Some(ent));
        if active {
            let seen = memory.seen.entry(ent).or_insert((now, now, friendly));
            if seen.2 != friendly {
                *seen = (now, now, friendly);
            }
            seen.1 = now;
        }
        let Some(&(start, last, was_friendly)) = memory.seen.get(&ent) else {
            continue;
        };
        if was_friendly != friendly {
            memory.seen.remove(&ent);
            continue;
        }
        let fade = if friendly {
            FRIENDLY_NAME_FADE_OUT_DEFAULT_MS
        } else {
            ENEMY_NAME_FADE_MS
        };
        if now.wrapping_sub(last) >= fade {
            memory.seen.remove(&ent);
            continue;
        }
        let alpha = cg_overhead_fade_alpha(
            now,
            start,
            last,
            if friendly { 0 } else { ENEMY_NAME_FADE_MS },
            fade,
        );
        if alpha <= 0.0 {
            continue;
        }
        let distance = cg_overhead_distance_scale(
            eye.to_array(),
            anchor.to_array(),
            OVERHEAD_NEAR_DISTANCE_DEFAULT,
            OVERHEAD_FAR_DISTANCE_DEFAULT,
            OVERHEAD_FAR_SCALE_DEFAULT,
        );
        let scale =
            r_normalized_text_scale(font.pixel_height, OVERHEAD_NAME_SIZE_DEFAULT * distance);
        let rank_scale =
            r_normalized_text_scale(font.pixel_height, OVERHEAD_RANK_SIZE_DEFAULT * distance);
        let x = (pixel.x - crate::chrome::r_text_width(font, &name) as f32 * scale * 0.5).round();
        let mut color = name_color(local_meta.client_state_team, team);
        color[3] = alpha;
        let mut text_runs = vec![(name, x, pixel.y.round(), scale, color)];
        if let Some((icon, level)) = catalog
            .as_ref()
            .and_then(|c| rank_presentation(c, rank, prestige))
        {
            let text_size = font.pixel_height as f32 * scale;
            let icon_size = OVERHEAD_ICON_SIZE_DEFAULT * text_size;
            let level_width = crate::chrome::r_text_width(font, level) as f32 * rank_scale;
            let icon_x = x - level_width - icon_size - 2.0 * distance;
            list.cmds.push(Draw2dCmd {
                x: icon_x,
                y: pixel.y.round() - (icon_size + text_size) * 0.5,
                w: icon_size,
                h: icon_size,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: [1.0, 1.0, 1.0, alpha],
                material: icon.to_owned(),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw {
                    site: "CG_DrawOverheadNames",
                },
                layer: 1,
            });
            text_runs.push((
                level.to_owned(),
                icon_x + icon_size,
                pixel.y.round() + font.pixel_height as f32 * rank_scale * 0.25,
                rank_scale,
                [1.0, 1.0, 1.0, alpha],
            ));
        } else {
            gaps.raise(GapCause::OverheadRankUnavailable { client: client.0 });
        }
        for (text, x, y, scale, color) in text_runs {
            list.cmds.push(Draw2dCmd {
                x,
                y,
                w: scale,
                h: scale,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color,
                material: assets::AssetRef::bare_name(&font.material).to_owned(),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                op: Draw2dOp::TextRun {
                    font: font_name.to_owned(),
                    scale,
                    text,
                    loc_key: String::new(),
                    style: 3,
                    fx: None,
                },
                provenance: Draw2dProvenance::CgDraw {
                    site: "CG_DrawOverheadNames",
                },
                layer: 1,
            });
        }
    }
    let fonts = HashMap::from([(font_name.to_owned(), font)]);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.overhead_names = TessJob::Quads(quads);
    }
}

pub(crate) fn rank_presentation(
    catalog: &MenuCatalog,
    rank: i32,
    prestige: i32,
) -> Option<(&str, &str)> {
    if rank < 0 || prestige < 0 {
        return None;
    }
    let key = rank.to_string();
    let icon = catalog
        .string_table("mp/rankIconTable.csv")?
        .lookup_col(&key, prestige.checked_add(1)?);
    let level = catalog
        .string_table("mp/rankTable.csv")?
        .lookup_col(&key, 14);
    (!icon.is_empty() && !level.is_empty()).then_some((icon, level))
}
