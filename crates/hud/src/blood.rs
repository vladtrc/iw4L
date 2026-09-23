use bevy::prelude::*;
use frame::{AppScreen, LifeStarted, UiDraw, ViewSubject};
use hud_iw4::{
    HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT, cg_blood_overlay_lerp, cg_get_health_fraction,
    cg_should_draw_blood_overlay,
};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::KillCamMode;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{PackedList, pack_splatter_alt};
use crate::images::{BLOOD_OVERLAY_COLOR, BLOOD_OVERLAY_MASK, HudImages, HudSampling};

#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct HudRootVisible(pub Option<i32>);

#[derive(Resource, Default)]
pub(crate) struct BloodOverlayLatch {
    intensity: f32,
    pub(crate) packed: PackedList,
    last_gpu_intensity: Option<f32>,
    last_w: Option<f32>,
    last_h: Option<f32>,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BloodGpuJob {
    #[default]
    Idle,
    Hide,
    Show,
    Write,
}

fn request_hide(job: &mut BloodGpuJob, packed_empty: bool) {
    if !packed_empty {
        *job = BloodGpuJob::Hide;
    }
}

fn gpu_stretch_bits_unchanged(
    latch: &BloodOverlayLatch,
    intensity: f32,
    win_w: f32,
    win_h: f32,
) -> bool {
    !latch.packed.is_empty()
        && latch.last_gpu_intensity == Some(intensity)
        && latch.last_w == Some(win_w)
        && latch.last_h == Some(win_h)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_blood_overlay(
    cg_clock: Res<CgFrameClock>,
    screen: Res<AppScreen>,
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut latch: ResMut<BloodOverlayLatch>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut job: ResMut<BloodGpuJob>,
    mut started: MessageReader<LifeStarted>,
    view: Res<ViewSubject>,
    ui_draw: Option<Res<UiDraw>>,
) {
    *job = BloodGpuJob::Idle;
    for ev in started.read() {
        if ev.client == local.0.0 {
            latch.intensity = 0.0;
        }
    }
    let in_game = matches!(*screen, AppScreen::InGame);
    let Some(ps) = presented.player(local.0) else {
        latch.intensity = 0.0;
        gaps.clear(HudGap::BloodOverlay);
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    let health_frac = cg_get_health_fraction(ps.health, ps.max_health, ps.pm_type);
    let in_killcam = view.in_killcam();
    let killcam_mode = KillCamMode::Mode0;
    let in_killcam_hud_gate = in_killcam && killcam_mode != KillCamMode::Mode0;

    if !in_game {
        latch.intensity = 0.0;
        gaps.clear(HudGap::BloodOverlay);
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }

    if !surface.is_ready() {
        gaps.clear(HudGap::BloodOverlay);
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }
    let win_w = surface.width();
    let win_h = surface.height();

    let blood_enabled = true;
    let should_draw_hud = ui_draw.is_some_and(|d| d.0);
    let should_draw = cg_should_draw_blood_overlay(
        blood_enabled,
        should_draw_hud,
        in_killcam_hud_gate,
        ps.pm_type,
    );
    if !should_draw {
        gaps.clear(HudGap::BloodOverlay);
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }

    let frametime_ms = cg_clock.frametime();
    latch.intensity = cg_blood_overlay_lerp(
        latch.intensity,
        health_frac,
        frametime_ms,
        HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT,
    );

    if latch.intensity == 0.0 {
        gaps.clear(HudGap::BloodOverlay);
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }

    if gpu_stretch_bits_unchanged(&latch, latch.intensity, win_w, win_h) {
        gaps.clear(HudGap::BloodOverlay);
        *job = BloodGpuJob::Show;
        return;
    }

    let binding = match hud_images.blood_material_binding() {
        Ok(binding) => binding,
        Err(detail) => {
            gaps.raise(GapCause::BloodOverlayMaterialUnsupported {
                detail: detail.to_owned(),
            });
            request_hide(&mut job, latch.packed.is_empty());
            return;
        }
    };

    let Some(color) = hud_images.get_sampled_with_sampler(
        crate::images::HUD_CHROME_NAMESPACE,
        BLOOD_OVERLAY_COLOR,
        HudSampling::Color,
        Some(binding.color_sampler),
        &mut images,
    ) else {
        gaps.raise(GapCause::BloodOverlayImageMissing {
            name: BLOOD_OVERLAY_COLOR.to_owned(),
            miss: hud_images.miss_reason(),
        });
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    let Some(mask) = hud_images.get_sampled_with_sampler(
        crate::images::HUD_CHROME_NAMESPACE,
        BLOOD_OVERLAY_MASK,
        HudSampling::Data,
        Some(binding.mask_sampler),
        &mut images,
    ) else {
        gaps.raise(GapCause::BloodOverlayImageMissing {
            name: BLOOD_OVERLAY_MASK.to_owned(),
            miss: hud_images.miss_reason(),
        });
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };

    gaps.clear(HudGap::BloodOverlay);
    let list = Draw2dList {
        cmds: vec![Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: 0.0,
            y: 0.0,
            w: win_w,
            h: win_h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [1.0, 1.0, 1.0, latch.intensity],
            material: BLOOD_OVERLAY_COLOR.into(),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw {
                site: "blood_overlay",
            },
            layer: 0,
        }],
    };
    let Some(quad) = tessellate(&list).0.into_iter().next() else {
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    latch.packed = pack_splatter_alt(&quad, color, mask, binding.state);
    latch.last_w = Some(win_w);
    latch.last_h = Some(win_h);
    latch.last_gpu_intensity = Some(latch.intensity);
    *job = BloodGpuJob::Write;
}
