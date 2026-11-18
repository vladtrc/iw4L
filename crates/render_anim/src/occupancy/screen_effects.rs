use assets::PreparedWeapons;
use bevy::prelude::*;
use frame::{ScreenEffectsDvars, ScreenEffectsView, ViewSubject};
use net::{FrameClock, LocalPresentation};
use playerstate_iw4::{KillCamMode, get_viewmodel_weapon_index};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ScreenEffectsComposer<'w> {
    clock: Res<'w, FrameClock>,
    base: Res<'w, ScreenEffectsDvars>,
    output: ResMut<'w, ScreenEffectsView>,
}

impl ScreenEffectsComposer<'_> {
    fn update(
        &mut self,
        presentation: LocalPresentation,
        view: Res<ViewSubject>,
        weapons: Option<Res<PreparedWeapons>>,
        kick: Res<super::view_kick::SessionViewKick>,
        settings: Res<frame::GameSettings>,
    ) {
        *self.output = ScreenEffectsView::default();
        let Some(ps) = presentation.player() else {
            return;
        };
        let Some(mode) = super::killcam::classify_view(
            &presentation.presented,
            presentation.client_id(),
            view.in_killcam(),
            weapons.as_deref(),
        ) else {
            return;
        };
        let mut dvars = *self.base;
        if let Some(snapshot) = presentation.snapshot() {
            for (name, value) in snapshot.meta.objectives.server_info.iter().chain(
                snapshot
                    .meta
                    .for_client(presentation.client_id())
                    .into_iter()
                    .flat_map(|m| m.client_dvars.iter()),
            ) {
                dvars.apply(name, value);
            }
        }
        let turret_active = ps.e_flags & 0xc00 != 0;
        let turret_scoped = turret_active
            && ps.viewlocked != 0
            && presentation
                .snapshot()
                .and_then(|s| {
                    s.meta
                        .entities
                        .iter()
                        .find(|e| e.number == ps.viewlocked_ent_num)
                })
                .and_then(|e| u32::try_from(e.index).ok())
                .filter(|&w| w != 0)
                .and_then(|w| {
                    weapons
                        .as_ref()?
                        .snapshot_weapon(w)
                        .ok()
                        .and_then(|weapon| weapon.hud_facts())
                })
                .is_some_and(|f| f.thermal_scope);
        let direction = if ps.f_weapon_pos_frac > kick.last_weapon_pos_frac {
            true
        } else if ps.f_weapon_pos_frac < kick.last_weapon_pos_frac {
            false
        } else {
            kick.b_position_to_ads
        };
        let scoped = turret_scoped
            || (!turret_active
                && !super::third_person::presented_is_third_person(
                    &presentation.presented,
                    presentation.client_id(),
                    view.in_killcam(),
                    settings.third_person,
                )
                && weapons
                    .as_ref()
                    .and_then(|w| {
                        let index = get_viewmodel_weapon_index(ps);
                        let f = w
                            .snapshot_weapon(index)
                            .ok()
                            .and_then(|weapon| weapon.hud_facts())?;
                        let zoom = hud_iw4::get_weap_reticle_zoom(
                            ps.f_weapon_pos_frac,
                            direction,
                            &hud_iw4::WeaponAdsOverlayFacts {
                                ads_zoom_in_frac: f.ads_zoom_in_frac,
                                ads_zoom_out_frac: f.ads_zoom_out_frac,
                                overlay_material: u32::from(
                                    w.registry().overlay_material_of(index).is_some(),
                                ),
                                overlay_reticle: f.overlay_reticle,
                                ..default()
                            },
                        );
                        Some(f.thermal_scope && zoom.active)
                    })
                    .unwrap_or(false));
        let thermal = ps.other_flags & 0x400 == 0
            && !matches!(
                mode,
                KillCamMode::Mode1Heli | KillCamMode::Mode3Missile | KillCamMode::Mode6Turret
            )
            && (ps.other_flags & 8 != 0 || scoped);
        let suppressed = view.in_killcam() && mode != KillCamMode::Mode0;
        let shock = presentation.shellshock();
        let shock_blend = shock.map_or(0, |s| {
            hud_iw4::shellshock_blend_time(
                self.clock.time(),
                ps.shellshock_time,
                ps.shellshock_duration,
                s.blur_fade_ms,
                s.blur_blend_ms,
            )
        });
        let flashed = !suppressed
            && dvars.draw_shellshock
            && shock_blend > 0
            && shock.is_some_and(|s| s.screen_type == hud_iw4::SCREEN_BLEND_FLASHED);
        let shock_blend = if dvars.draw_shellshock
            && shock.is_some_and(|s| s.screen_type == hud_iw4::SCREEN_BLEND_BLURRED)
        {
            shock_blend
        } else {
            0
        };
        let instant_thermal = thermal && scoped;
        let thermal_blend = if thermal && !scoped && ps.other_flags & 8 != 0 {
            dvars.thermal_no_scope_ms
        } else {
            0
        };
        *self.output = ScreenEffectsView {
            ready: true,
            default_killcam_view: view.in_killcam()
                && matches!(
                    mode,
                    KillCamMode::Mode1Heli | KillCamMode::Mode3Missile | KillCamMode::Mode6Turret
                ),
            thermal_active: thermal,
            thermal_scoped: scoped,
            instant_thermal,
            suppressed,
            flashed,
            blend_ms: if suppressed || flashed {
                0
            } else {
                shock_blend.max(thermal_blend)
            },
        };
    }
}

pub(super) fn update(
    presentation: LocalPresentation,
    view: Res<ViewSubject>,
    weapons: Option<Res<PreparedWeapons>>,
    kick: Res<super::view_kick::SessionViewKick>,
    settings: Res<frame::GameSettings>,
    mut composer: ScreenEffectsComposer,
) {
    composer.update(presentation, view, weapons, kick, settings);
}
