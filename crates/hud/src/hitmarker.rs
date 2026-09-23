use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use frame::AppScreen;
use gamemode_iw4::{DAMAGE_FEEDBACK_SHADER, DamageFeedbackPulse, TypeHit, update_damage_feedback};
use hud_iw4::{HE_TYPE_MATERIAL, HudElem, bg_lerp_hud_colors, copy_in_use_prefix};
use net::{AuthorityWorld, CgFrameClock, LocalPresentClient, PendingSvcSounds, PresentedSnapshot};

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::images::HudImages;
use crate::presentation_scale::{HorizontalAlign, PresentationScale, ScaleClass, VerticalAlign};
use crate::ui_write::adopt_display;

#[derive(Resource, Default)]
pub struct PendingHitmarker(pub u32);

#[derive(Resource, Default)]
pub(crate) struct HitmarkerLatch {
    remaining_ms: i32,
    pulse: Option<DamageFeedbackPulse>,
    seen_seq: u64,
    pulse_n: u32,
}
#[derive(Component)]
pub(crate) struct Hitmarker;

struct BankMaterial<'a> {
    elem: &'a HudElem,
    color: [u8; 4],
}

fn bank_material<'a>(
    current: &'a [HudElem],
    archival: &'a [HudElem],
    materials: &sim::HudMaterialCsOccupied,
    cg_time: i32,
) -> Option<BankMaterial<'a>> {
    let current_inuse = copy_in_use_prefix(current);
    let archival_inuse = copy_in_use_prefix(archival);
    let mut vis: Vec<BankMaterial<'a>> = Vec::new();
    push_visible_materials(&mut vis, current_inuse, materials, cg_time);
    push_visible_materials(&mut vis, archival_inuse, materials, cg_time);
    vis.sort_by(|a, b| a.elem.sort.total_cmp(&b.elem.sort));
    vis.pop()
}

fn push_visible_materials<'a>(
    vis: &mut Vec<BankMaterial<'a>>,
    src: &'a [HudElem],
    materials: &sim::HudMaterialCsOccupied,
    cg_time: i32,
) {
    for elem in src {
        if elem.elem_type != HE_TYPE_MATERIAL {
            continue;
        }
        let Some(index) = u8::try_from(elem.material_index).ok() else {
            continue;
        };
        if sim::name_in_occupied(materials, index) != Some(DAMAGE_FEEDBACK_SHADER) {
            continue;
        }
        let color = bg_lerp_hud_colors(elem, cg_time);
        if color[3] == 0 {
            continue;
        }
        vis.push(BankMaterial { elem, color });
    }
}

pub(crate) fn spawn_hitmarker(root: &mut ChildSpawnerCommands) {
    root.spawn((
        Hitmarker,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
        ImageNode::default(),
        FocusPolicy::Pass,
    ));
}

fn hide(marker: &mut Query<(&mut Node, &mut ImageNode), With<Hitmarker>>) {
    for (mut node, _) in marker.iter_mut() {
        adopt_display(&mut node, Display::None);
    }
}

fn stamp_pulse(latch: &mut HitmarkerLatch, pulse: DamageFeedbackPulse) {
    latch.remaining_ms = pulse.fade_ms;
    latch.pulse = Some(pulse);
    latch.pulse_n = latch.pulse_n.saturating_add(1);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_hitmarker(
    cg_clock: Res<CgFrameClock>,
    screen: Res<AppScreen>,
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    authority: Option<Res<AuthorityWorld>>,
    mut pending: ResMut<PendingHitmarker>,
    mut pending_svc: Option<ResMut<PendingSvcSounds>>,
    mut latch: ResMut<HitmarkerLatch>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut marker: Query<(&mut Node, &mut ImageNode), With<Hitmarker>>,
) {
    let cg_time = cg_clock.time();

    if pending.0 > 0 {
        pending.0 = 0;
        if let Some(pulse) = update_damage_feedback(TypeHit::Standard, false, 0.0) {
            stamp_pulse(&mut latch, pulse);
            if let Some(alias) = pulse.sound
                && let Some(ref mut svc) = pending_svc
            {
                svc.push_alias(local.0, false, alias);
            }
        }
    }

    let live_frame = presented
        .player(local.0)
        .is_some_and(|ps| ps.is_live_frame());
    if let Some(world) = authority.as_deref() {
        if live_frame {
            for cue in world.0.damage_feedback_cues() {
                if cue.seq <= latch.seen_seq {
                    continue;
                }
                latch.seen_seq = cue.seq;
                if cue.attacker != local.0 {
                    continue;
                }
                let Some(pulse) = update_damage_feedback(cue.type_hit, false, 0.0) else {
                    continue;
                };
                if let Some(alias) = pulse.sound
                    && let Some(ref mut svc) = pending_svc
                {
                    svc.push_alias(local.0, false, alias);
                }
            }
        }
    }

    if presented.player(local.0).is_none() {
        latch.remaining_ms = 0;
        latch.pulse = None;
        gaps.clear(HudGap::Hitmarker);
        hide(&mut marker);
        return;
    }

    if !matches!(*screen, AppScreen::InGame) {
        latch.remaining_ms = 0;
        latch.pulse = None;
        gaps.clear(HudGap::Hitmarker);
        hide(&mut marker);
        return;
    }

    let frametime_ms = cg_clock.frametime();
    if latch.remaining_ms > 0 {
        latch.remaining_ms = (latch.remaining_ms - frametime_ms).max(0);
    }

    let snapshot = presented.snapshot();
    let bank = snapshot.and_then(|s| {
        let meta = s.meta.for_client(local.0)?;
        bank_material(
            &meta.hud_current,
            &meta.hud_archival,
            &s.meta.hud_materials,
            cg_time,
        )
    });

    if let Some(mat) = bank {
        let alpha = mat.color[3] as f32 / 255.0;
        blit(
            &mut hud_images,
            &mut images,
            &mut gaps,
            &mut marker,
            &surface,
            DAMAGE_FEEDBACK_SHADER,
            mat.elem.x,
            mat.elem.y,
            mat.elem.width as f32,
            mat.elem.height as f32,
            alpha,
            Some(hud_iw4::hud_elem_placement(
                surface.placement(),
                mat.elem,
                cg_time,
                0.0,
                0.0,
            )),
        );
        return;
    }

    let Some(pulse) = latch.pulse else {
        gaps.clear(HudGap::Hitmarker);
        hide(&mut marker);
        return;
    };

    if latch.remaining_ms <= 0 || pulse.fade_ms <= 0 {
        gaps.clear(HudGap::Hitmarker);
        hide(&mut marker);
        return;
    }

    let alpha = (latch.remaining_ms as f32 / pulse.fade_ms as f32).clamp(0.0, 1.0);
    blit(
        &mut hud_images,
        &mut images,
        &mut gaps,
        &mut marker,
        &surface,
        pulse.shader,
        pulse.x as f32,
        pulse.y as f32,
        pulse.width as f32,
        pulse.height as f32,
        alpha,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn blit(
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
    gaps: &mut HudPresentationGaps,
    marker: &mut Query<(&mut Node, &mut ImageNode), With<Hitmarker>>,
    surface: &crate::surface::Hud2dSurface,
    shader: &'static str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    alpha: f32,
    physical: Option<hud_iw4::HudElemPlacement>,
) {
    if !surface.is_ready() {
        gaps.clear(HudGap::Hitmarker);
        hide(marker);
        return;
    }

    let Some(handle) = hud_images.get(crate::images::HUD_CHROME_NAMESPACE, shader, images) else {
        gaps.raise(GapCause::HitmarkerImageMissing {
            name: shader.to_owned(),
            miss: hud_images.miss_reason(),
        });
        hide(marker);
        return;
    };
    gaps.clear(HudGap::Hitmarker);

    let scale = PresentationScale::from_window(surface.width(), surface.height());
    let (left, top, width, height) = if let Some(placed) = physical {
        (placed.x, placed.y, placed.w, placed.h)
    } else {
        let placed = scale.place(
            ScaleClass::ProjectionBound,
            HorizontalAlign::Center,
            VerticalAlign::Center,
            x,
            y,
            w,
            h,
        );
        (placed.left, placed.top, placed.width, placed.height)
    };
    for (mut node, mut image_node) in marker.iter_mut() {
        adopt_display(&mut node, Display::Flex);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        node.width = Val::Px(width);
        node.height = Val::Px(height);
        image_node.image = handle.clone();
        image_node.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
}
