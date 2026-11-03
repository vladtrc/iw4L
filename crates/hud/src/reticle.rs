use assets::PreparedWeapons;
use bevy::prelude::*;
use bevy::ui::Display;
use frame::{LifeStarted, ViewSubject};
use hud_iw4::{
    CG_CROSSHAIR_ALPHA_DEFAULT, CG_CROSSHAIR_ALPHA_MIN_DEFAULT, HipCrosshairGate,
    SCREEN_BLEND_BLURRED, WeaponAdsCrosshairFacts, WeaponReticleFacts, calc_reticle_alpha,
    calc_reticle_spread, hip_crosshair_trans_scale, hip_crosshair_visible, is_flashbanged,
    reticle_draw_size,
};
use movement_iw4::mantle::is_weapon_inactive;
use net::{FrameClock, ViewweaponAim};
use playerstate_iw4::{KillCamMode, ThirdPersonViewInputs, is_third_person_view};
use weapon_iw4::{
    SpreadOverrideState, WeaponSpreadFacts, get_spread_for_weapon, get_viewmodel_weapon_index,
    perk_weap_spread_multiplier, should_apply_view_org_bob,
};

use crate::images::HudImages;
use crate::presentation_scale::{PresentationScale, ScaleClass};
use crate::ui_write::adopt_display;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ReticleQuad {
    Side(u8),

    Center,
}

#[derive(Resource, Default)]
pub(crate) struct ReticleAdsLatch {
    last_frac: f32,
    pub(crate) position_to_ads: bool,
}

pub(crate) fn spawn_reticle(root: &mut ChildSpawnerCommands) {
    for quad in [
        ReticleQuad::Side(0),
        ReticleQuad::Side(1),
        ReticleQuad::Side(2),
        ReticleQuad::Side(3),
        ReticleQuad::Center,
    ] {
        root.spawn((
            quad,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
            UiTransform::IDENTITY,
            ImageNode::default(),
        ));
    }
}

fn hide_all(quads: &mut Query<(&ReticleQuad, &mut Node, &mut ImageNode, &mut UiTransform)>) {
    for (_, mut node, _, _) in quads.iter_mut() {
        adopt_display(&mut node, Display::None);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ReticleRenderer<'w, 's> {
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    ads_latch: ResMut<'w, ReticleAdsLatch>,
    quads: Query<
        'w,
        's,
        (
            &'static ReticleQuad,
            &'static mut Node,
            &'static mut ImageNode,
            &'static mut UiTransform,
        ),
    >,
    cameras: Query<'w, 's, &'static Projection, With<Camera3d>>,
    started: MessageReader<'w, 's, LifeStarted>,
    view: Res<'w, ViewSubject>,
}

impl ReticleRenderer<'_, '_> {
    fn begin_frame(&mut self, client: u32) {
        for ev in self.started.read() {
            if ev.client == client {
                *self.ads_latch = ReticleAdsLatch::default();
            }
        }
    }

    fn track_ads(&mut self, frac: f32) {
        if frac > self.ads_latch.last_frac + 1e-4 {
            self.ads_latch.position_to_ads = true;
        } else if frac + 1e-4 < self.ads_latch.last_frac {
            self.ads_latch.position_to_ads = false;
        }
        self.ads_latch.last_frac = frac;
    }

    fn rendering_third_person(
        &self,
        presentation: &net::LocalPresentation,
        settings: &frame::GameSettings,
        ps: &playerstate_iw4::PlayerState,
    ) -> bool {
        let linked_weapon_view = presentation
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(presentation.client_id()))
            .is_some_and(|meta| meta.linked_weapon_view.is_some());
        (self.view.in_killcam() && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE)
            || (!linked_weapon_view
                && is_third_person_view(ThirdPersonViewInputs {
                    pm_type: ps.pm_type,
                    other_flags: ps.other_flags,
                    link_flags: ps.link_flags,
                    cg_third_person: settings.third_person && !self.view.in_killcam(),
                    in_killcam: self.view.in_killcam(),
                    killcam_mode: KillCamMode::Mode0,
                }))
    }

    fn hide(&mut self) {
        hide_all(&mut self.quads);
    }

    fn render(
        &mut self,
        surface: &crate::surface::Hud2dSurface,
        aim: &ViewweaponAim,
        ps: &playerstate_iw4::PlayerState,
        facts: &asset_game::WeaponHudFacts,
        weapons: &asset_game::WeaponRegistry,
        viewmodel_index: u32,
    ) {
        let Some(assets) = weapons.reticle_of(viewmodel_index) else {
            diag::warn_once!(
                Ui,
                "weapon index {viewmodel_index} authored no reticle material in either slot"
            );
            self.hide();
            return;
        };

        let Some(tan_half) = self.cameras.iter().find_map(|p| match p {
            Projection::Perspective(persp) => Some((persp.fov * 0.5).tan()),
            _ => None,
        }) else {
            self.hide();
            return;
        };
        let reticle_facts = WeaponReticleFacts {
            i_reticle_min_ofs: facts.i_reticle_min_ofs,
            hip_reticle_side_pos: facts.hip_reticle_side_pos,
            i_reticle_side_size: facts.i_reticle_side_size,
        };
        let ads_xf = WeaponAdsCrosshairFacts {
            ads_aim_pitch: facts.ads_aim_pitch,
            ads_crosshair_in_frac: facts.ads_crosshair_in_frac,
            ads_crosshair_out_frac: facts.ads_crosshair_out_frac,
        };
        let spread_facts = WeaponSpreadFacts {
            stand_min: facts.hip_spread_stand_min,
            ducked_min: facts.hip_spread_ducked_min,
            prone_min: facts.hip_spread_prone_min,
            stand_max: facts.hip_spread_stand_max,
            ducked_max: facts.hip_spread_ducked_max,
            prone_max: facts.hip_spread_prone_max,
        };
        let aim_spread = ps.aim_spread_scale;

        let trans_scale = hip_crosshair_trans_scale(
            ps.f_weapon_pos_frac,
            self.ads_latch.position_to_ads,
            &ads_xf,
            tan_half,
        );
        let side_size_v = reticle_draw_size(&reticle_facts, trans_scale);
        let cone = get_spread_for_weapon(
            ps.view_height_current,
            ps.spread_override,
            SpreadOverrideState::from_i32(ps.spread_override_state),
            &spread_facts,
            perk_weap_spread_multiplier(ps.perks[0]),
        );
        let gap_v = calc_reticle_spread(
            cone.min,
            cone.max,
            aim_spread,
            trans_scale,
            tan_half,
            &reticle_facts,
            side_size_v,
        );

        let cook_size = if facts.cooks_grenade && ps.grenade_time_left > 0 {
            (ps.grenade_time_left % 1000) as f32 / 100.0
        } else {
            0.0
        };
        let center_size_v = (assets.center_size as f32 + cook_size) * trans_scale;
        let alpha = calc_reticle_alpha(
            1.0,
            CG_CROSSHAIR_ALPHA_DEFAULT,
            CG_CROSSHAIR_ALPHA_MIN_DEFAULT,
            aim_spread,
        );

        let scale = PresentationScale::from_window(surface.width(), surface.height());
        let factor = scale.factor(ScaleClass::ProjectionBound);
        let cx = scale.width() * 0.5;
        let cy = scale.height() * 0.5;

        let xhair_px = if aim.live {
            (aim.xhair_x * factor, aim.xhair_y * factor)
        } else {
            (0.0, 0.0)
        };

        let weapon_ns = weapons
            .component_namespace_of(viewmodel_index, asset_game::WeaponComponent::Material)
            .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
        let center = resolve_slot(
            "centre",
            assets.center_authored,
            &assets.center_image,
            weapon_ns,
            &mut self.hud_images,
            &mut self.images,
        );
        let side = resolve_slot(
            "side",
            assets.side_authored,
            &assets.side_image,
            weapon_ns,
            &mut self.hud_images,
            &mut self.images,
        );
        let center_handle = center.handle();
        let side_handle = side.handle();
        match material_gap_cause(&center, &side) {
            Some(cause) => diag::warn_once!(Ui, "{}", cause),
            None => (),
        }

        for (quad, mut node, mut image_node, mut xform) in self.quads.iter_mut() {
            let (handle, size_v, offset) = match *quad {
                ReticleQuad::Center => (center_handle.clone(), center_size_v, None),
                ReticleQuad::Side(i) => (side_handle.clone(), side_size_v[0], Some(i)),
            };
            let Some(handle) = handle else {
                adopt_display(&mut node, Display::None);
                continue;
            };
            if size_v <= 0.0 {
                adopt_display(&mut node, Display::None);
                continue;
            }
            let size = size_v * factor;

            let (dx, dy, turns) = match offset {
                None => (0.0, 0.0, 0u8),
                Some(0) => (0.0, -(gap_v[1] * factor), 0),
                Some(1) => (gap_v[0] * factor, 0.0, 1),
                Some(2) => (0.0, gap_v[1] * factor, 2),
                Some(_) => (-(gap_v[0] * factor), 0.0, 3),
            };
            adopt_display(&mut node, Display::Flex);
            node.left = Val::Px(cx + xhair_px.0 + dx - size * 0.5);
            node.top = Val::Px(cy + xhair_px.1 + dy - size * 0.5);
            node.width = Val::Px(size);
            node.height = Val::Px(size);
            xform.rotation = Rot2::degrees(f32::from(turns) * 90.0);
            image_node.image = handle;
            image_node.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        }
    }
}

pub(crate) fn update_reticle(
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    weapons: Option<Res<PreparedWeapons>>,
    mut renderer: ReticleRenderer,
    aim: Res<ViewweaponAim>,
    settings: Res<frame::GameSettings>,
    cg_clock: Res<FrameClock>,
) {
    renderer.begin_frame(presentation.client_id().0);
    if !surface.is_ready() {
        return;
    }
    let Some(ps) = presentation.player() else {
        renderer.hide();
        return;
    };
    if ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
        renderer.hide();
        return;
    }

    renderer.track_ads(ps.f_weapon_pos_frac);

    let viewmodel_index = get_viewmodel_weapon_index(ps);
    let Some(weapons) = weapons.as_ref() else {
        diag::warn_once!(Ui, "no weapon catalog is loaded");
        renderer.hide();
        return;
    };
    let Some(facts) = weapons
        .snapshot_weapon(viewmodel_index)
        .ok()
        .and_then(|weapon| weapon.hud_facts())
    else {
        diag::warn_once!(
            Ui,
            "viewmodel weapon index {viewmodel_index} is not in the catalog"
        );
        renderer.hide();
        return;
    };

    let mantle_inactive = is_weapon_inactive(ps, true);
    let rendering_third_person = renderer.rendering_third_person(&presentation, &settings, ps);
    let gate = HipCrosshairGate {
        rendering_third_person,
        e_flags: ps.e_flags,
        other_flags: ps.other_flags,
        viewmodel_weapon_index: viewmodel_index as i32,
        flashbanged: is_flashbanged(
            cg_clock.time(),
            ps.shellshock_time,
            ps.shellshock_duration,
            presentation
                .shellshock()
                .map_or(SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
        ) != 0,
        draw_hud: true,
        dvars_allow: presentation.snapshot().is_none_or(|snapshot| {
            snapshot
                .meta
                .script_dvars(presentation.client_id())
                .int("cg_drawCrosshair")
                .unwrap_or(1)
                != 0
        }),
        f_weapon_pos_frac: ps.f_weapon_pos_frac,
        cg_draw_gun: true,

        bob_gate: should_apply_view_org_bob(
            false,
            ps.pm_type,
            ps.other_flags,
            ps.link_flags,
            ps.e_flags,
            ps.f_weapon_pos_frac,
            facts.overlay_reticle,
        ),
        weaponstate_primary: ps.weaponstate_primary,
        weaponstate_secondary: ps.weaponstate_secondary,
        last_weapon_hand: ps.last_weapon_hand,
        mantle_weapon_inactive: mantle_inactive,
    };
    if !hip_crosshair_visible(&gate) {
        renderer.hide();
        return;
    }

    renderer.render(
        &surface,
        &aim,
        ps,
        &facts,
        weapons.registry(),
        viewmodel_index,
    );
}

struct SlotResolution {
    slot: &'static str,

    authored: bool,
    image: SlotImage,
}

enum SlotImage {
    Unnamed,

    Drawn(Handle<Image>),

    Missing { name: String, miss: &'static str },
}

impl SlotResolution {
    fn handle(&self) -> Option<Handle<Image>> {
        match &self.image {
            SlotImage::Drawn(handle) => Some(handle.clone()),
            SlotImage::Unnamed | SlotImage::Missing { .. } => None,
        }
    }
}

fn resolve_slot(
    slot: &'static str,
    authored: bool,
    name: &Option<String>,
    namespace: asset_core::FamilyId,
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
) -> SlotResolution {
    let image = match name.as_deref() {
        None => SlotImage::Unnamed,
        Some(name) => match hud_images.get(namespace, name, images) {
            Some(handle) => SlotImage::Drawn(handle),
            None => SlotImage::Missing {
                name: name.to_owned(),
                miss: hud_images.miss_reason(),
            },
        },
    };
    SlotResolution {
        slot,
        authored,
        image,
    }
}

fn material_gap_cause(center: &SlotResolution, side: &SlotResolution) -> Option<String> {
    slot_gap_cause(center).or_else(|| slot_gap_cause(side))
}

fn slot_gap_cause(resolution: &SlotResolution) -> Option<String> {
    if !resolution.authored {
        return None;
    }
    match &resolution.image {
        SlotImage::Drawn(_) => None,
        SlotImage::Unnamed => Some(format!(
            "the {slot} material names no image",
            slot = resolution.slot
        )),
        SlotImage::Missing { name, miss } => Some(format!(
            "{slot} image `{name}` is {miss}",
            slot = resolution.slot,
            name = name.clone(),
            miss = *miss
        )),
    }
}
