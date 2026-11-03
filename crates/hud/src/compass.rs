use asset_game::{MenuCatalog, MenuItem};
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    COMPASS_ENEMY_FIRING_PING_IMAGE, COMPASS_FRIENDLY_HEIGHT_DEFAULT,
    COMPASS_FRIENDLY_WIDTH_DEFAULT, COMPASS_MAX_RANGE_DEFAULT_MP, COMPASS_PLAYER_HEIGHT_DEFAULT,
    COMPASS_PLAYER_WIDTH_DEFAULT, COMPASS_RADAR_LINE_IMAGE, COMPASS_RADAR_PING_FADE_TIME_DEFAULT,
    COMPASS_RADAR_UPDATE_TIME_DEFAULT, COMPASS_SIZE_DEFAULT, CompassMapBounds, CompassMapUvWindow,
    RADARJAM_DIST_MAX, RADARJAM_DIST_MIN, compass_clamp_offset, compass_fade_alpha,
    compass_friendly_size, compass_map_bounds_from_minimap_corners, compass_partial_map_uv,
    compass_player_size, compass_sound_ping_fade, compass_up_yaw_vector,
    radar_contact_trail_visible, radar_jam_intensity, radar_jam_nearest_distance, radar_line,
    radar_line_texture_center_s, radar_lines_surround_point, world_pos_to_compass_partial,
};
use net::{FrameClock, PresentedSnapshot, WeaponFirePingBus};
use sim::ClientId;
use std::collections::HashMap;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};
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
    radar_progress: f32,
    radar_last_ms: Option<i32>,
}

#[derive(Component, Default)]
pub(crate) struct CompassPing(Option<PingActor>);

struct PingActor {
    begin_fade_ms: i32,
    fade_seconds: f32,
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

pub(crate) struct DrawableCompass {
    pub(crate) image_name: String,
    pub(crate) bounds: CompassMapBounds,
    max_range: f32,
    pub(crate) north_yaw: f32,
}

struct CatalogCompass<'a> {
    map: (usize, &'a MenuItem),
    player: Option<(usize, &'a MenuItem)>,
    enemies: Option<(usize, &'a MenuItem)>,
}

fn find_owner<'a>(menu: &'a asset_game::MenuDef, owner_draw: i32) -> Option<(usize, &'a MenuItem)> {
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CompassContacts<'w, 's> {
    clock: Res<'w, FrameClock>,
    latch: ResMut<'w, CompassPingLatch>,
    slots: Res<'w, net::CEntitySlots>,
    actors: Query<'w, 's, (&'static net::CEntity, &'static mut CompassPing)>,
    ping_bus: ResMut<'w, WeaponFirePingBus>,
}

impl CompassContacts<'_, '_> {
    fn receive(&mut self, presentation: &net::LocalPresentation) -> i32 {
        let presented: &PresentedSnapshot = &presentation.presented;
        let local = presentation.client_id();
        let cg_time_ms = self.clock.time();
        let latch: &mut CompassPingLatch = &mut self.latch;
        if presented.snapshot().is_none() {
            for (_, mut actor) in self.actors.iter_mut() {
                actor.0 = None;
            }
            latch.radar_progress = 0.0;
            latch.radar_last_ms = None;
        }
        for (_, mut actor) in self.actors.iter_mut() {
            if actor.0.as_ref().is_some_and(|actor| {
                compass_sound_ping_fade(cg_time_ms, actor.begin_fade_ms, actor.fade_seconds)
                    .is_none()
            }) {
                actor.0 = None;
            }
        }
        let n = self.ping_bus.pings.len() as i32;
        for ping in self.ping_bus.pings.drain(..) {
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
            set_ping(
                &mut self.actors,
                &self.slots,
                id,
                PingActor {
                    begin_fade_ms: cg_time_ms,
                    fade_seconds: hud_iw4::COMPASS_SOUND_PING_FADE_TIME_DEFAULT,
                    last_pos: ping.origin_xy,
                },
            );
        }
        n
    }

    fn radar(
        &mut self,
        presentation: &net::LocalPresentation,
        drawable: &DrawableCompass,
    ) -> Option<[f32; 3]> {
        let presented: &PresentedSnapshot = &presentation.presented;
        let local = presentation.client_id();
        let now_ms = self.clock.time();
        let snapshot = presented.snapshot()?;
        let local_meta = snapshot.meta.for_client(local);
        let local_team = local_meta.map_or(3, |meta| meta.client_state_team);
        let radar = local_meta.map_or(sim::RadarMode::Off, |meta| meta.radar);
        if radar == sim::RadarMode::Off {
            self.latch.radar_progress = 0.0;
            self.latch.radar_last_ms = None;
            return None;
        }
        if radar == sim::RadarMode::Constant {
            self.latch.radar_progress = 0.0;
            self.latch.radar_last_ms = None;
            for (id, _) in &snapshot.players {
                if *id == local {
                    continue;
                }
                let Some(meta) = snapshot.meta.for_client(*id) else {
                    continue;
                };
                if same_team(local_team, meta.client_state_team) {
                    continue;
                }
                let Some(ps) = presented.alive_player(*id) else {
                    continue;
                };
                set_ping(
                    &mut self.actors,
                    &self.slots,
                    id.0,
                    PingActor {
                        begin_fade_ms: now_ms,
                        fade_seconds: COMPASS_RADAR_PING_FADE_TIME_DEFAULT,
                        last_pos: [ps.origin[0], ps.origin[1]],
                    },
                );
            }
            return None;
        }
        let frametime = self
            .latch
            .radar_last_ms
            .map_or(0, |last| now_ms.saturating_sub(last).max(0));
        self.latch.radar_last_ms = Some(now_ms);
        let old = self.latch.radar_progress;
        let sweep = match radar {
            sim::RadarMode::Fast => COMPASS_RADAR_UPDATE_TIME_DEFAULT * 0.5,
            _ => COMPASS_RADAR_UPDATE_TIME_DEFAULT,
        };
        let new = old + frametime as f32 / (sweep * 1000.0);
        let new = new - new.floor();
        self.latch.radar_progress = new;
        let line = radar_line(drawable.bounds, drawable.max_range, new);
        if new < old {
            return Some(line);
        }
        let prev = radar_line(drawable.bounds, drawable.max_range, old);
        for (id, _) in &snapshot.players {
            if *id == local {
                continue;
            }
            let Some(meta) = snapshot.meta.for_client(*id) else {
                continue;
            };
            if same_team(local_team, meta.client_state_team) {
                continue;
            }
            let Some(ps) = presented.alive_player(*id) else {
                continue;
            };
            let pos = [ps.origin[0], ps.origin[1]];
            if !radar_contact_trail_visible(ps.perks[0])
                || !radar_lines_surround_point(prev, line, pos)
            {
                continue;
            }
            set_ping(
                &mut self.actors,
                &self.slots,
                id.0,
                PingActor {
                    begin_fade_ms: now_ms,
                    fade_seconds: COMPASS_RADAR_PING_FADE_TIME_DEFAULT,
                    last_pos: pos,
                },
            );
        }
        Some(line)
    }

    fn visible(
        &self,
        presentation: &net::LocalPresentation,
        map_item: &MenuItem,
        drawable: &DrawableCompass,
        north: [f32; 2],
        player_xy: [f32; 2],
        local_team: i32,
    ) -> Vec<([f32; 2], f32)> {
        let mut live: Vec<([f32; 2], f32)> = Vec::new();
        for (identity, ping) in &self.actors {
            let Some(actor) = &ping.0 else {
                continue;
            };
            let id = u32::from(identity.number());
            let Some(meta) = presentation
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
            let Some(alpha) =
                compass_sound_ping_fade(self.clock.time(), actor.begin_fade_ms, actor.fade_seconds)
            else {
                continue;
            };
            let offset = world_pos_to_compass_partial(
                north,
                player_xy,
                actor.last_pos,
                map_item.rect.h * COMPASS_SIZE_DEFAULT,
                drawable.max_range,
            );
            live.push((offset, alpha));
        }
        live
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CompassRenderer<'w> {
    compass: Option<Res<'w, assets::SessionCompass>>,
    catalog: Option<Res<'w, MenuCatalog>>,
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    pass: ResMut<'w, HudTessPass>,
}

impl CompassRenderer<'_> {
    fn hide(&mut self) {
        self.pass.compass = TessJob::Hide;
    }

    fn update(
        &mut self,
        surface: Res<crate::surface::Hud2dSurface>,
        presentation: net::LocalPresentation,
        mut contacts: CompassContacts,
        view: Option<Res<frame::ViewSubject>>,
        local_vars: Res<crate::playercard::UiLocalVars>,
        hud_input: Option<Res<frame::HudInputView>>,
    ) {
        contacts.receive(&presentation);
        let killed_by_showing = (crate::scorebar::milliseconds() as i32)
            .wrapping_sub(local_vars.int("ui_show_killedBy"))
            < 4000;
        let ui_active = hud_input.is_some_and(|i| i.script_menu_open);
        if !surface.is_ready()
            || killed_by_showing
            || ui_active
            || view.is_some_and(|v| v.in_killcam())
        {
            self.hide();
            return;
        }
        let Some(ps) = presentation.player() else {
            self.hide();
            return;
        };
        if presentation.snapshot().is_some_and(|snapshot| {
            snapshot
                .meta
                .script_dvars(presentation.client_id())
                .int("g_hardcore")
                .unwrap_or(0)
                != 0
                && snapshot
                    .meta
                    .for_client(presentation.client_id())
                    .is_none_or(|meta| meta.radar == sim::RadarMode::Off)
        }) {
            self.hide();
            return;
        }
        let Some(items) = self.catalog.as_ref().and_then(|c| catalog_compass(c)) else {
            if self.catalog.is_none() {
                diag::warn_once!(
                    Ui,
                    "MenuCatalog is not loaded; minimap_fullscreen cannot place"
                );
            } else {
                diag::warn_once!(
                    Ui,
                    "minimap_fullscreen has no ownerDraw 159 minimap_map item"
                );
            }
            self.hide();
            return;
        };
        let Some(drawable) = resolve(self.compass.as_deref(), &mut self.hud_images) else {
            self.hide();
            return;
        };

        let uv = compass_partial_map_uv(
            drawable.bounds,
            [ps.origin[0], ps.origin[1]],
            drawable.max_range,
        );
        if uv.half_s <= 0.0 || uv.half_t <= 0.0 {
            self.hide();
            return;
        }
        let map_rotation = -(ps.viewangles[1] - drawable.north_yaw);

        let [player_w, player_h] = compass_player_size(
            COMPASS_PLAYER_WIDTH_DEFAULT,
            COMPASS_PLAYER_HEIGHT_DEFAULT,
            COMPASS_SIZE_DEFAULT,
        );
        let [ping_w, ping_h] = compass_friendly_size(
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

        let north = compass_up_yaw_vector(ps.viewangles[1]);
        let player_xy = [ps.origin[0], ps.origin[1]];
        let local_team = presentation
            .snapshot()
            .and_then(|s| s.meta.for_client(presentation.client_id()))
            .map_or(3, |m| m.client_state_team);
        let jam_fade = {
            let jammers = presentation.snapshot().into_iter().flat_map(|snap| {
                snap.players.iter().filter_map(|(id, _)| {
                    if *id == presentation.client_id()
                        || snap
                            .meta
                            .for_client(*id)
                            .is_some_and(|m| same_team(local_team, m.client_state_team))
                    {
                        return None;
                    }
                    let other = presentation.presented.alive_player(*id)?;
                    if other.e_flags & playerstate_iw4::eflags::RADAR_JAM == 0 {
                        return None;
                    }
                    Some(other.origin)
                })
            });
            let dist = radar_jam_nearest_distance(ps.origin, jammers);
            compass_fade_alpha(
                1.0,
                radar_jam_intensity(
                    dist,
                    RADARJAM_DIST_MIN,
                    RADARJAM_DIST_MAX,
                    ps.other_flags & playerstate_iw4::other_flags::EMP_JAMMED != 0
                        || presentation
                            .snapshot()
                            .and_then(|snap| snap.meta.for_client(presentation.client_id()))
                            .is_some_and(|meta| meta.radar_blocked),
                ),
            )
        };
        let map_item = items.map.1;
        let radar = contacts
            .radar(&presentation, &drawable)
            .map(|line| radar_line_texture_center_s(line, player_xy, drawable.max_range));
        let live = contacts.visible(
            &presentation,
            map_item,
            &drawable,
            north,
            player_xy,
            local_team,
        );

        let mut list = build_compass_map(
            &surface,
            items.map,
            &drawable.image_name,
            self.hud_images
                .map_namespace()
                .expect("drawable compass family"),
            uv,
            map_rotation,
            radar,
        );
        append_compass_player(
            &mut list,
            &surface,
            map_item,
            items.player.zip(player_stem),
            [player_w, player_h],
            jam_fade,
        );

        let Some(map_ns) = self.hud_images.map_namespace() else {
            return;
        };
        if self
            .hud_images
            .get(map_ns, &drawable.image_name, &mut self.images)
            .is_none()
        {
            self.hide();
            return;
        }

        let mut fonts = HashMap::new();
        if let Some(snapshot) = presentation.snapshot() {
            let team_mode = snapshot.meta.kind.is_team();
            let team = snapshot
                .meta
                .for_client(presentation.client_id())
                .map(|m| m.client_state_team)
                .unwrap_or(0);
            let team =
                gamemode_iw4::Team::from_packed_u8(team as u8).unwrap_or(gamemode_iw4::Team::Free);
            let objectives = snapshot.meta.objectives.compass.iter().filter(|o| {
                o.shows_to(team, presentation.client_id().0)
                    && !o.icon.is_empty()
                    && (team_mode || o.viewer.is_some())
            });
            let size = map_item.rect.h * COMPASS_SIZE_DEFAULT;
            for objective in objectives {
                let offset = world_pos_to_compass_partial(
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
                    material: objective.icon.clone(),
                    material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                    op: Draw2dOp::StretchPic,
                    provenance: Draw2dProvenance::Objective,
                    layer: 1,
                });
            }
            if team_mode
                && let Some(font) = self
                    .catalog
                    .as_ref()
                    .and_then(|c| c.font(crate::font_overlay::HUD_SMALL_FONT))
            {
                fonts.insert(crate::font_overlay::HUD_SMALL_FONT.to_owned(), font);
            }
        }
        list.cmds.extend(enemy_ping_cmds(
            &surface,
            map_item,
            items.enemies,
            [ping_w, ping_h],
            COMPASS_ENEMY_FIRING_PING_IMAGE,
            &live,
            jam_fade,
        ));
        let (mut quads, _) = crate::draw2d::tessellate_fonts(&list, &fonts);
        if let Some(snapshot) = presentation.snapshot() {
            for vehicle in &snapshot.meta.objectives.vehicles {
                let friendly = vehicle.owner == presentation.client_id().0
                    || same_team(local_team, vehicle.team);
                let icon = &vehicle.icons[usize::from(!friendly)];
                if icon.is_empty() || vehicle.size.iter().any(|size| *size <= 0) {
                    continue;
                }
                let offset = world_pos_to_compass_partial(
                    north,
                    player_xy,
                    [vehicle.origin[0], vehicle.origin[1]],
                    map_item.rect.h * COMPASS_SIZE_DEFAULT,
                    drawable.max_range,
                );
                let size = vehicle.size.map(|size| size as f32 * COMPASS_SIZE_DEFAULT);
                let bounds = [map_item.rect.w, map_item.rect.h].map(|n| n * COMPASS_SIZE_DEFAULT);
                let offset = std::array::from_fn(|axis| {
                    let limit = ((bounds[axis] - size[axis]) * 0.5).max(0.0);
                    offset[axis].clamp(-limit, limit)
                });
                let yaw = ps.viewangles[1] - vehicle.yaw;
                let st = crate::chrome::atlas_frame_st(
                    self.catalog.as_deref(),
                    icon,
                    contacts.clock.time(),
                );
                let xy = compass_icon_corners(&surface, map_item, offset, size, yaw);
                quads.push(compass_icon_quad(
                    xy,
                    jam_fade,
                    icon,
                    st,
                    Draw2dProvenance::OwnerDraw(158),
                ));
            }
            for (id, _) in &snapshot.players {
                if *id == presentation.client_id() {
                    continue;
                }
                let Some(meta) = snapshot.meta.for_client(*id) else {
                    continue;
                };
                if !same_team(local_team, meta.client_state_team) {
                    continue;
                }
                let Some(other) = presentation.presented.alive_player(*id) else {
                    continue;
                };
                let offset = world_pos_to_compass_partial(
                    north,
                    player_xy,
                    [other.origin[0], other.origin[1]],
                    map_item.rect.h * COMPASS_SIZE_DEFAULT,
                    drawable.max_range,
                );
                let xy = compass_icon_corners(
                    &surface,
                    map_item,
                    offset,
                    [ping_w, ping_h],
                    ps.viewangles[1] - other.viewangles[1],
                );
                quads.push(compass_icon_quad(
                    xy,
                    jam_fade,
                    "compassping_friendly_mp",
                    [0.0, 0.0, 1.0, 1.0],
                    Draw2dProvenance::OwnerDraw(158),
                ));
            }
        }
        if quads.is_empty() {
            self.hide();
            return;
        }

        self.pass.compass = TessJob::Quads(quads);
    }
}

pub(crate) fn update_compass(
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    contacts: CompassContacts,
    mut renderer: CompassRenderer,
    view: Option<Res<frame::ViewSubject>>,
    local_vars: Res<crate::playercard::UiLocalVars>,
    hud_input: Option<Res<frame::HudInputView>>,
) {
    renderer.update(surface, presentation, contacts, view, local_vars, hud_input);
}

fn build_compass_map(
    surface: &crate::surface::Hud2dSurface,
    map: (usize, &MenuItem),
    map_image: &str,
    map_namespace: asset_core::FamilyId,
    uv: CompassMapUvWindow,
    rotation_deg: f32,
    radar_center_s: Option<f32>,
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
        provenance: provenance_map.clone(),
        layer: 1,
    });
    if let Some(center_s) = radar_center_s {
        cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: applied.x,
            y: applied.y,
            w: applied.w,
            h: applied.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: map_item.fore_color,
            material: COMPASS_RADAR_LINE_IMAGE.into(),
            op: Draw2dOp::RotateSt {
                center_s,
                center_t: 0.0,
                radius_st: 0.5 / hud_iw4::COMPASS_RADAR_LINE_THICKNESS_DEFAULT,
                scale_final_s: 1.0,
                scale_final_t: 1.0,
                deg: rotation_deg,
            },
            provenance: provenance_map,
            layer: 1,
        });
    }
    Draw2dList { cmds }
}

fn append_compass_player(
    list: &mut Draw2dList,
    surface: &crate::surface::Hud2dSurface,
    map_item: &MenuItem,
    player: Option<((usize, &MenuItem), &str)>,
    player_size: [f32; 2],
    jam_fade: f32,
) {
    if let Some(((player_index, player_item), player_image)) = player {
        let horz = map_item.rect.horz_align as i32;
        let vert = map_item.rect.vert_align as i32;
        let vw = map_item.rect.w * COMPASS_SIZE_DEFAULT;
        let vh = map_item.rect.h * COMPASS_SIZE_DEFAULT;
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
        list.cmds.push(Draw2dCmd {
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
}

fn enemy_ping_cmds(
    surface: &crate::surface::Hud2dSurface,
    map_item: &MenuItem,
    enemies: Option<(usize, &MenuItem)>,
    ping_size: [f32; 2],
    ping_image: &str,
    pings: &[([f32; 2], f32)],
    jam_fade: f32,
) -> Vec<Draw2dCmd> {
    let horz = map_item.rect.horz_align as i32;
    let vert = map_item.rect.vert_align as i32;
    let vw = map_item.rect.w * COMPASS_SIZE_DEFAULT;
    let vh = map_item.rect.h * COMPASS_SIZE_DEFAULT;
    let mut cmds = Vec::new();
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
    cmds
}

fn same_team(local: i32, other: i32) -> bool {
    matches!(local, 1 | 2) && local == other
}

fn compass_icon_corners(
    surface: &crate::surface::Hud2dSurface,
    map: &MenuItem,
    offset: [f32; 2],
    size: [f32; 2],
    yaw: f32,
) -> [[f32; 2]; 4] {
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
    [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p| {
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
    })
}

fn compass_icon_quad(
    xy: [[f32; 2]; 4],
    alpha: f32,
    material: &str,
    st: [f32; 4],
    provenance: Draw2dProvenance,
) -> crate::draw2d::Draw2dQuad {
    crate::draw2d::Draw2dQuad {
        xy,
        st: [
            [st[0], st[1]],
            [st[2], st[1]],
            [st[2], st[3]],
            [st[0], st[3]],
        ],
        color: [1.0, 1.0, 1.0, alpha],
        material: material.to_owned(),
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        provenance,
        layer: 1,
        clip: None,
    }
}

fn scale_icon_alpha(mut rgba: [f32; 4], scale: f32) -> [f32; 4] {
    rgba[3] *= scale;
    rgba
}

pub(crate) fn resolve(
    compass: Option<&assets::SessionCompass>,
    hud_images: &mut HudImages,
) -> Option<DrawableCompass> {
    let compass = compass?;
    let north_yaw = match compass.north_yaw {
        Some(authored) => authored,
        None => 0.0,
    };
    let Some(image_name) = compass.declaration.image.as_deref() else {
        diag::warn_once!(
            Ui,
            "the map declared no minimap material or that material names no image"
        );
        return None;
    };
    let map_ns = hud_images.map_namespace()?;
    hud_images.ensure_rgba(map_ns, image_name);
    if hud_images.rgba(map_ns, image_name).is_none() {
        diag::warn_once!(
            Ui,
            "minimap image `{name}` is {miss}",
            name = image_name.to_owned(),
            miss = hud_images.miss_reason()
        );
        return None;
    };
    let Some(corners) = compass.corners else {
        diag::warn_once!(Ui, "MapEnts carries no minimap_corner pair");
        return None;
    };
    let Some(bounds) = compass_map_bounds_from_minimap_corners(corners.a, corners.b, north_yaw)
    else {
        diag::warn_once!(Ui, "the minimap_corner pair spans no area");
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

fn set_ping(
    actors: &mut Query<(&net::CEntity, &mut CompassPing)>,
    slots: &net::CEntitySlots,
    number: u32,
    ping: PingActor,
) {
    if let Some(entity) = u16::try_from(number)
        .ok()
        .and_then(|number| slots.entity_for_number(number))
        && let Ok((_, mut actor)) = actors.get_mut(entity)
    {
        actor.0 = Some(ping);
    }
}
