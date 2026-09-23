use std::collections::HashMap;

use assets::{MenuCatalog, MenuItem};
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    COMPASS_ENEMY_FIRING_PING_IMAGE, COMPASS_FRIENDLY_HEIGHT_DEFAULT,
    COMPASS_FRIENDLY_WIDTH_DEFAULT, COMPASS_MAX_RANGE_DEFAULT_MP, COMPASS_PLAYER_HEIGHT_DEFAULT,
    COMPASS_PLAYER_WIDTH_DEFAULT, COMPASS_SIZE_DEFAULT, CompassMapBounds, CompassMapUvWindow,
    RADARJAM_DIST_MAX, RADARJAM_DIST_MIN, cg_compass_fade_alpha, cg_compass_friendly_size,
    cg_compass_player_size, cg_compass_sound_ping_fade, cg_compass_up_yaw_vector,
    cg_radar_jam_intensity, cg_radar_jam_nearest_distance, cg_world_pos_to_compass_partial,
    compass_clamp_offset, compass_map_bounds_from_minimap_corners, compass_partial_map_uv,
    radar_contact_trail_visible,
};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot, WeaponFirePingBus};
use sim::ClientId;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{GpuListLatch, HudTessPass, TessJob};
use crate::images::HudImages;

const MINIMAP_MENU: &str = "minimap_fullscreen";

const OWNER_DRAW_MAP: i32 = 159;

const OWNER_DRAW_PLAYER: i32 = 150;

const OWNER_DRAW_ENEMIES: i32 = 175;

#[derive(Component)]
pub(crate) struct CompassRaster;

#[derive(Resource, Default)]
pub(crate) struct CompassPingLatch {
    actors: HashMap<u32, PingActor>,
    last_time: Option<i32>,
}

struct PingActor {
    begin_fade_ms: i32,
    last_pos: [f32; 2],
}
pub(crate) fn spawn_compass(root: &mut ChildSpawnerCommands) {
    root.spawn((
        CompassRaster,
        GpuListLatch::default(),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
    ));
}

fn hide(pass: &mut HudTessPass) {
    pass.compass = TessJob::Hide;
}

struct DrawableCompass {
    image_name: String,
    bounds: CompassMapBounds,
    max_range: f32,
    north_yaw: f32,
}

struct CatalogCompass<'a> {
    map: (usize, &'a MenuItem),
    player: Option<(usize, &'a MenuItem)>,
    enemies: Option<(usize, &'a MenuItem)>,
}

fn find_owner<'a>(menu: &'a assets::MenuDef, owner_draw: i32) -> Option<(usize, &'a MenuItem)> {
    menu.items
        .iter()
        .enumerate()
        .find(|(_, item)| item.owner_draw == owner_draw)
}

fn catalog_compass(catalog: &MenuCatalog) -> Option<CatalogCompass<'_>> {
    let menu = catalog.get(MINIMAP_MENU)?;
    let map = find_owner(menu, OWNER_DRAW_MAP)?;
    Some(CatalogCompass {
        map,
        player: find_owner(menu, OWNER_DRAW_PLAYER),
        enemies: find_owner(menu, OWNER_DRAW_ENEMIES),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_compass(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    cg_clock: Res<CgFrameClock>,
    compass: Option<Res<assets::SessionCompass>>,
    catalog: Option<Res<MenuCatalog>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut latch: ResMut<CompassPingLatch>,
    mut ping_bus: ResMut<WeaponFirePingBus>,
    mut pass: ResMut<HudTessPass>,
    view: Option<Res<frame::ViewSubject>>,
    local_vars: Res<crate::playercard::UiLocalVars>,
) {
    take_fire_pings(
        &mut ping_bus,
        &presented,
        local.0,
        cg_clock.time(),
        &mut latch,
    );
    let killed_by_showing = (crate::scorebar::sys_milliseconds() as i32)
        .wrapping_sub(local_vars.int("ui_show_killedBy"))
        < 4000;
    if !surface.is_ready() || killed_by_showing || view.is_some_and(|v| v.in_killcam()) {
        hide(&mut pass);
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        hide(&mut pass);
        return;
    };
    let Some(items) = catalog.as_ref().and_then(|c| catalog_compass(c)) else {
        if catalog.is_none() {
            gaps.raise(GapCause::CompassNoCatalog);
        } else {
            gaps.raise(GapCause::CompassNoMapItem);
        }
        hide(&mut pass);
        return;
    };
    let Some(drawable) = resolve(compass.as_deref(), &mut hud_images, &mut gaps) else {
        hide(&mut pass);
        return;
    };

    let uv = compass_partial_map_uv(
        drawable.bounds,
        [ps.origin[0], ps.origin[1]],
        drawable.max_range,
    );
    if uv.half_s <= 0.0 || uv.half_t <= 0.0 {
        hide(&mut pass);
        return;
    }
    let map_rotation = -(ps.viewangles[1] - drawable.north_yaw);

    let [player_w, player_h] = cg_compass_player_size(
        COMPASS_PLAYER_WIDTH_DEFAULT,
        COMPASS_PLAYER_HEIGHT_DEFAULT,
        COMPASS_SIZE_DEFAULT,
    );
    let [ping_w, ping_h] = cg_compass_friendly_size(
        COMPASS_FRIENDLY_WIDTH_DEFAULT,
        COMPASS_FRIENDLY_HEIGHT_DEFAULT,
        COMPASS_SIZE_DEFAULT,
    );

    let player_stem = items.player.and_then(|(_, item)| {
        if item.background.is_empty() {
            None
        } else {
            Some(item.background.as_str())
        }
    });

    let north = cg_compass_up_yaw_vector(ps.viewangles[1]);
    let player_xy = [ps.origin[0], ps.origin[1]];
    let jam_fade = {
        let jammers = presented.snapshot().into_iter().flat_map(|snap| {
            snap.players.iter().filter_map(|(id, _)| {
                if *id == local.0 {
                    return None;
                }
                let other = presented.player(*id)?;
                if other.e_flags & playerstate_iw4::eflags::RADAR_JAM == 0 {
                    return None;
                }
                Some(other.origin)
            })
        });
        let dist = cg_radar_jam_nearest_distance(ps.origin, jammers);
        cg_compass_fade_alpha(
            1.0,
            cg_radar_jam_intensity(dist, RADARJAM_DIST_MIN, RADARJAM_DIST_MAX, false),
        )
    };
    let map_item = items.map.1;
    let local_team = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .map_or(3, |m| m.client_state_team);
    let mut live: Vec<([f32; 2], f32)> = Vec::new();
    for (&id, actor) in &latch.actors {
        let Some(meta) = presented
            .snapshot()
            .and_then(|s| s.meta.for_client(ClientId(id)))
        else {
            continue;
        };
        if local_team == 3
            || meta.client_state_team == 3
            || same_team(local_team, meta.client_state_team)
        {
            continue;
        }
        let Some(alpha) = cg_compass_sound_ping_fade(
            cg_clock.time(),
            actor.begin_fade_ms,
            hud_iw4::COMPASS_SOUND_PING_FADE_TIME_DEFAULT,
        ) else {
            continue;
        };
        let offset = cg_world_pos_to_compass_partial(
            north,
            player_xy,
            actor.last_pos,
            map_item.rect.h * COMPASS_SIZE_DEFAULT,
            drawable.max_range,
        );
        live.push((offset, alpha));
    }

    let mut list = build_compass_list(
        &surface,
        items.map,
        items.player.zip(player_stem),
        items.enemies,
        &drawable.image_name,
        hud_images.map_namespace(),
        uv,
        map_rotation,
        [player_w, player_h],
        [ping_w, ping_h],
        COMPASS_ENEMY_FIRING_PING_IMAGE,
        &live,
        jam_fade,
    );

    let map_ns = hud_images.map_namespace();
    if hud_images
        .get(map_ns, &drawable.image_name, &mut images)
        .is_none()
    {
        hide(&mut pass);
        return;
    }

    let mut fonts = HashMap::new();
    if let Some(snapshot) = presented.snapshot()
        && snapshot.meta.kind.is_team()
        && let Some(font) = catalog
            .as_ref()
            .and_then(|c| c.font(crate::font_overlay::HUD_SMALL_FONT))
    {
        let team = snapshot
            .meta
            .for_client(local.0)
            .map(|m| m.client_state_team)
            .unwrap_or(0);
        let state = &snapshot.meta.objectives;
        let objectives = state
            .flags
            .iter()
            .chain(state.bombs.iter().filter(|b| !b.destroyed).map(|b| &b.view));
        let size = map_item.rect.h * COMPASS_SIZE_DEFAULT;
        for objective in objectives {
            let offset = cg_world_pos_to_compass_partial(
                north,
                player_xy,
                [objective.origin[0], objective.origin[1]],
                size,
                drawable.max_range,
            );
            let width = map_item.rect.w * COMPASS_SIZE_DEFAULT;
            let offset = compass_clamp_offset(offset, [width, size]);
            let x = map_item.rect.x + width * 0.5 + offset[0];
            let y = map_item.rect.y + size * 0.5 + offset[1];
            let r = surface.apply_rect(
                x - 8.0,
                y - 8.0,
                16.0,
                16.0,
                map_item.rect.horz_align as i32,
                map_item.rect.vert_align as i32,
            );
            list.cmds.push(Draw2dCmd {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: [1.0, 1.0, 1.0, jam_fade],
                material: crate::objectives::marker_material(
                    snapshot.meta.kind,
                    state,
                    objective,
                    gamemode_iw4::Team::from_retail_u8(team as u8)
                        .unwrap_or(gamemode_iw4::Team::Free),
                ),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::Objective,
                layer: 1,
            });
        }
        fonts.insert(crate::font_overlay::HUD_SMALL_FONT.to_owned(), font);
        gaps.clear(HudGap::CompassObjectives);
    }
    let (mut quads, _) = crate::draw2d::tessellate_fonts(&list, &fonts);
    if let Some(snapshot) = presented.snapshot() {
        for (id, _) in &snapshot.players {
            if *id == local.0 {
                continue;
            }
            let Some(meta) = snapshot.meta.for_client(*id) else {
                continue;
            };
            if !same_team(local_team, meta.client_state_team) {
                continue;
            }
            let Some(other) = presented.alive_player(*id) else {
                continue;
            };
            let offset = cg_world_pos_to_compass_partial(
                north,
                player_xy,
                [other.origin[0], other.origin[1]],
                map_item.rect.h * COMPASS_SIZE_DEFAULT,
                drawable.max_range,
            );
            quads.push(friendly_quad(
                &surface,
                map_item,
                offset,
                [ping_w, ping_h],
                ps.viewangles[1] - other.viewangles[1],
                jam_fade,
            ));
        }
    }
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    gaps.clear(HudGap::CompassMap);
    pass.compass = TessJob::Quads(quads);
}

fn build_compass_list(
    surface: &crate::surface::Hud2dSurface,
    map: (usize, &MenuItem),
    player: Option<((usize, &MenuItem), &str)>,
    enemies: Option<(usize, &MenuItem)>,
    map_image: &str,
    map_namespace: assets::AssetNamespace,
    uv: CompassMapUvWindow,
    rotation_deg: f32,
    player_size: [f32; 2],
    ping_size: [f32; 2],
    ping_image: &str,
    pings: &[([f32; 2], f32)],
    jam_fade: f32,
) -> Draw2dList {
    let (map_index, map_item) = map;
    let horz = map_item.rect.horz_align as i32;
    let vert = map_item.rect.vert_align as i32;
    let vw = map_item.rect.w * COMPASS_SIZE_DEFAULT;
    let vh = map_item.rect.h * COMPASS_SIZE_DEFAULT;
    let applied = surface.apply_rect(map_item.rect.x, map_item.rect.y, vw, vh, horz, vert);
    let provenance_map = Draw2dProvenance::MenuItem {
        menu: MINIMAP_MENU.into(),
        index: map_index,
    };
    let mut cmds = Vec::new();
    cmds.push(Draw2dCmd {
        material_namespace: map_namespace,
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color: map_item.fore_color,
        material: map_image.into(),
        op: Draw2dOp::RotateSt {
            center_s: uv.center[0],
            center_t: uv.center[1],
            radius_st: uv.radius_st,
            scale_final_s: uv.scale_final_s,
            scale_final_t: uv.scale_final_t,
            deg: rotation_deg,
        },
        provenance: provenance_map,
        layer: 1,
    });
    if let Some(((player_index, player_item), player_image)) = player {
        let cx = map_item.rect.x + vw * 0.5;
        let cy = map_item.rect.y + vh * 0.5;
        let pr = surface.apply_rect(
            cx - player_size[0] * 0.5,
            cy - player_size[1] * 0.5,
            player_size[0],
            player_size[1],
            horz,
            vert,
        );
        cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: pr.x,
            y: pr.y,
            w: pr.w,
            h: pr.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: scale_icon_alpha(player_item.fore_color, jam_fade),
            material: player_image.into(),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::MenuItem {
                menu: MINIMAP_MENU.into(),
                index: player_index,
            },
            layer: 1,
        });
    }
    let ping_prov = match enemies {
        Some((index, _)) => Draw2dProvenance::MenuItem {
            menu: MINIMAP_MENU.into(),
            index,
        },
        None => Draw2dProvenance::OwnerDraw(OWNER_DRAW_ENEMIES),
    };
    for &(offset, alpha) in pings {
        let offset = compass_clamp_offset(offset, [vw, vh]);
        let cx = map_item.rect.x + vw * 0.5 + offset[0];
        let cy = map_item.rect.y + vh * 0.5 + offset[1];
        let pr = surface.apply_rect(
            cx - ping_size[0] * 0.5,
            cy - ping_size[1] * 0.5,
            ping_size[0],
            ping_size[1],
            horz,
            vert,
        );
        cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: pr.x,
            y: pr.y,
            w: pr.w,
            h: pr.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [1.0, 1.0, 1.0, alpha * jam_fade],
            material: ping_image.into(),
            op: Draw2dOp::StretchPic,
            provenance: ping_prov.clone(),
            layer: 1,
        });
    }
    Draw2dList { cmds }
}

fn same_team(local: i32, other: i32) -> bool {
    matches!(local, 1 | 2) && local == other
}

fn friendly_quad(
    surface: &crate::surface::Hud2dSurface,
    map: &MenuItem,
    offset: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    alpha: f32,
) -> crate::draw2d::Draw2dQuad {
    let map_size = [
        map.rect.w * COMPASS_SIZE_DEFAULT,
        map.rect.h * COMPASS_SIZE_DEFAULT,
    ];
    let offset = compass_clamp_offset(offset, map_size);
    let center = [
        map.rect.x + map_size[0] * 0.5 + offset[0],
        map.rect.y + map_size[1] * 0.5 + offset[1],
    ];
    let (sin, cos) = yaw.to_radians().sin_cos();
    let xy = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p| {
        let x = p[0] * size[0];
        let y = p[1] * size[1];
        let r = surface.apply_rect(
            center[0] + x * cos - y * sin,
            center[1] + x * sin + y * cos,
            0.0,
            0.0,
            map.rect.horz_align as i32,
            map.rect.vert_align as i32,
        );
        [r.x, r.y]
    });
    crate::draw2d::Draw2dQuad {
        xy,
        st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color: [1.0, 1.0, 1.0, alpha],
        material: "compassping_friendly_mp".to_owned(),
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        provenance: Draw2dProvenance::OwnerDraw(158),
        layer: 1,
        clip: None,
    }
}

fn scale_icon_alpha(mut rgba: [f32; 4], scale: f32) -> [f32; 4] {
    rgba[3] *= scale;
    rgba
}

fn take_fire_pings(
    bus: &mut WeaponFirePingBus,
    presented: &PresentedSnapshot,
    local: ClientId,
    cg_time_ms: i32,
    latch: &mut CompassPingLatch,
) -> i32 {
    if latch.last_time.is_some_and(|last| cg_time_ms < last) || presented.snapshot().is_none() {
        latch.actors.clear();
    }
    latch.last_time = Some(cg_time_ms);
    latch.actors.retain(|_, actor| {
        cg_compass_sound_ping_fade(
            cg_time_ms,
            actor.begin_fade_ms,
            hud_iw4::COMPASS_SOUND_PING_FADE_TIME_DEFAULT,
        )
        .is_some()
    });
    let n = bus.pings.len() as i32;
    for ping in bus.pings.drain(..) {
        if ping.number == local.0 as i32 {
            continue;
        }
        let Ok(id) = u32::try_from(ping.number) else {
            continue;
        };
        if presented
            .player(ClientId(id))
            .is_some_and(|ps| !radar_contact_trail_visible(ps.perks[0]))
        {
            continue;
        }
        latch.actors.insert(
            id,
            PingActor {
                begin_fade_ms: cg_time_ms,
                last_pos: ping.origin_xy,
            },
        );
    }
    n
}

fn resolve(
    compass: Option<&assets::SessionCompass>,
    hud_images: &mut HudImages,
    gaps: &mut HudPresentationGaps,
) -> Option<DrawableCompass> {
    let compass = compass?;
    let north_yaw = match compass.north_yaw {
        Some(authored) => authored,
        None => 0.0,
    };
    let Some(image_name) = compass.declaration.image.as_deref() else {
        gaps.raise(GapCause::CompassNoImageDeclared);
        return None;
    };
    let map_ns = hud_images.map_namespace();
    hud_images.ensure_rgba(map_ns, image_name);
    if hud_images.rgba(map_ns, image_name).is_none() {
        gaps.raise(GapCause::CompassImageMissing {
            name: image_name.to_owned(),
            miss: hud_images.miss_reason(),
        });
        return None;
    };
    let Some(corners) = compass.corners else {
        gaps.raise(GapCause::CompassNoMinimapCorners);
        return None;
    };
    let Some(bounds) = compass_map_bounds_from_minimap_corners(corners.a, corners.b, north_yaw)
    else {
        gaps.raise(GapCause::CompassCornersDegenerate);
        return None;
    };
    Some(DrawableCompass {
        image_name: image_name.to_owned(),
        bounds,
        max_range: match compass.declaration.max_range {
            Some(range) => range,
            None => COMPASS_MAX_RANGE_DEFAULT_MP,
        },
        north_yaw,
    })
}
