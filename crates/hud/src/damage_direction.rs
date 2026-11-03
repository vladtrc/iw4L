use bevy::prelude::*;
use frame::{LifeStarted, UiDraw, ViewSubject};
use net::FrameClock;

use crate::draw2d::{Draw2dProvenance, Draw2dQuad};
use crate::gpu_list::{HudTessPass, TessJob};

#[derive(Component)]
pub(crate) struct DamageDirectionRaster;

#[derive(Clone, Copy, Default)]
struct Hit {
    time: i32,
    duration: i32,
    yaw: f32,
    stun: bool,
}

#[derive(Resource, Default)]
pub(crate) struct DamageDirections {
    hits: [Hit; 12],
    event: Option<u32>,
    subject: Option<ViewSubject>,
    time: Option<i32>,
    weapon_frac: f32,
    ads_in: bool,
    random: u32,
}

pub(crate) fn spawn(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, DamageDirectionRaster);
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct DamageDirectionDisplay<'w, 's> {
    clock: Res<'w, FrameClock>,
    started: MessageReader<'w, 's, LifeStarted>,
    directions: ResMut<'w, DamageDirections>,
    pass: ResMut<'w, HudTessPass>,
}

impl DamageDirectionDisplay<'_, '_> {
    fn update(
        &mut self,
        surface: Res<crate::surface::Hud2dSurface>,
        presentation: net::LocalPresentation,
        view: Res<ViewSubject>,
        ui_draw: Res<UiDraw>,
        aim: Res<net::ViewweaponAim>,
        weapons: Option<Res<assets::PreparedWeapons>>,
    ) {
        self.pass.damage_direction = TessJob::Hide;
        let now = self.clock.time();
        let reset = self
            .started
            .read()
            .filter(|ev| ev.client == presentation.client_id().0)
            .count()
            != 0
            || self.directions.subject != Some(*view)
            || self.directions.time.is_some_and(|time| now < time);
        if reset {
            *self.directions = DamageDirections::default();
        }
        self.directions.subject = Some(*view);
        self.directions.time = Some(now);
        let Some(ps) = presentation.player() else {
            *self.directions = DamageDirections::default();
            return;
        };
        let dvars = presentation
            .snapshot()
            .map(|snapshot| snapshot.meta.script_dvars(presentation.client_id()));
        if self
            .directions
            .event
            .is_some_and(|event| event != ps.damage_event)
            && ps.damage_count > 0
            && !(ps.damage_yaw == 255 && ps.damage_pitch == 255)
        {
            let duration = dvars
                .and_then(|d| d.int("cg_hudDamageIconTime"))
                .unwrap_or(6000)
                .max(0);
            let slot = self
                .directions
                .hits
                .iter()
                .enumerate()
                .min_by_key(|(_, hit)| hit.time)
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.directions.random = self
                .directions
                .random
                .wrapping_mul(214013)
                .wrapping_add(2531011);
            let jitter = (((self.directions.random >> 16) & 0x7fff) as f32 / 32768.0 - 0.5) * 20.0;
            self.directions.hits[slot] = Hit {
                time: now,
                duration,
                yaw: ps.damage_yaw as f32 * (360.0 / 256.0) + jitter,
                stun: ps.damage_flags & 1 != 0,
            };
        }
        self.directions.event = Some(ps.damage_event);
        if ps.f_weapon_pos_frac != self.directions.weapon_frac {
            self.directions.ads_in = ps.f_weapon_pos_frac > self.directions.weapon_frac;
        }
        self.directions.weapon_frac = ps.f_weapon_pos_frac;
        let scoped = weapons.as_ref().is_some_and(|weapons| {
            let index = playerstate_iw4::get_viewmodel_weapon_index(ps);
            weapons
                .snapshot_weapon(index)
                .ok()
                .and_then(|weapon| weapon.hud_facts())
                .is_some_and(|facts| {
                    hud_iw4::get_weap_reticle_zoom(
                        ps.f_weapon_pos_frac,
                        self.directions.ads_in,
                        &hud_iw4::WeaponAdsOverlayFacts {
                            ads_zoom_in_frac: facts.ads_zoom_in_frac,
                            ads_zoom_out_frac: facts.ads_zoom_out_frac,
                            overlay_material: u32::from(
                                weapons.registry().overlay_material_of(index).is_some(),
                            ),
                            overlay_reticle: facts.overlay_reticle,
                            ..default()
                        },
                    )
                    .active
                })
        });
        let flashed = hud_iw4::is_flashbanged(
            now,
            ps.shellshock_time,
            ps.shellshock_duration,
            presentation
                .shellshock()
                .map_or(hud_iw4::SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
        ) != 0;
        if !surface.is_ready()
            || !ui_draw.0
            || flashed
            || ps.pm_flags & playerstate_iw4::pm_flags::MANTLE != 0
            || (scoped
                && dvars
                    .and_then(|d| d.int("cg_hudDamageIconInScope"))
                    .unwrap_or(0)
                    == 0)
            || dvars
                .and_then(|d| d.int("cg_drawDamageDirection"))
                .unwrap_or(1)
                == 0
            || (view.in_killcam() && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE)
        {
            return;
        }
        let value = |name, default| {
            dvars
                .and_then(|d| d.float(name))
                .filter(|value| value.is_finite())
                .unwrap_or(default)
                .clamp(0.0, 512.0)
        };
        let scale = surface.scale_virtual_to_real();
        let half_width = value("cg_hudDamageIconWidth", 128.0) * scale[0] * 0.5;
        let offset = value("cg_hudDamageIconOffset", 128.0) * scale[1];
        let outer = offset + value("cg_hudDamageIconHeight", 64.0) * scale[1];
        let mut center = [surface.width() * 0.5, surface.height() * 0.5];
        if scoped && aim.live {
            let projection = crate::presentation_scale::PresentationScale::from_window(
                surface.width(),
                surface.height(),
            );
            let factor = projection.factor(crate::presentation_scale::ScaleClass::ProjectionBound);
            center[0] += aim.xhair_x * factor;
            center[1] += aim.xhair_y * factor;
        }
        let mut quads = Vec::new();
        for hit in &self.directions.hits {
            let age = now.saturating_sub(hit.time);
            if age <= 0 || age >= hit.duration {
                continue;
            }
            let (sin, cos) = (ps.viewangles[1] - hit.yaw).to_radians().sin_cos();
            let xy = [
                [half_width, outer],
                [-half_width, outer],
                [-half_width, offset],
                [half_width, offset],
            ]
            .map(|[x, y]| [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]);
            quads.push(Draw2dQuad {
                xy,
                st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                color: [
                    1.0,
                    1.0,
                    1.0,
                    (2.0 - 2.0 * age as f32 / hit.duration as f32).min(1.0),
                ],
                material: if hit.stun {
                    "hit_direction_stun"
                } else {
                    "hit_direction"
                }
                .into(),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                provenance: Draw2dProvenance::CgDraw {
                    site: "damage_direction",
                },
                layer: 0,
                clip: None,
            });
        }
        self.pass.damage_direction = TessJob::Quads(quads);
    }
}

pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    view: Res<ViewSubject>,
    ui_draw: Res<UiDraw>,
    aim: Res<net::ViewweaponAim>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    mut display: DamageDirectionDisplay,
) {
    display.update(surface, presentation, view, ui_draw, aim, weapons);
}
