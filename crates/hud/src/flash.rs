use bevy::prelude::*;
use frame::{AppScreen, LifeStarted};
use hud_iw4::{cg_is_flashbanged, cg_shellshock_flash_blend};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::PackedList;
use crate::images::HudImages;

const FLASH_WHITE_IMAGE: &str = "white";

#[derive(Resource, Default)]
pub(crate) struct FlashWhiteoutLatch {
    pub(crate) saved: PackedList,
    pub(crate) packed: PackedList,
    last_alpha: Option<(f32, f32)>,
    last_w: Option<f32>,
    last_h: Option<f32>,
    last_start: Option<i32>,
    pub(crate) save_sequence: u64,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlashGpuJob {
    #[default]
    Idle,
    Hide,
    Write,
}

fn request_hide(job: &mut FlashGpuJob, packed_empty: bool) {
    if !packed_empty {
        *job = FlashGpuJob::Hide;
    }
}

fn skip_rewrite(latch: &FlashWhiteoutLatch, alpha: (f32, f32), win_w: f32, win_h: f32) -> bool {
    !latch.packed.is_empty()
        && latch.last_alpha == Some(alpha)
        && latch.last_w == Some(win_w)
        && latch.last_h == Some(win_h)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_flash_whiteout(
    cg_clock: Res<CgFrameClock>,
    screen: Res<AppScreen>,
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut latch: ResMut<FlashWhiteoutLatch>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut job: ResMut<FlashGpuJob>,
    mut started: MessageReader<LifeStarted>,
) {
    *job = FlashGpuJob::Idle;
    for ev in started.read() {
        if ev.client == local.0.0 {
            latch.packed = PackedList::default();
            latch.saved = PackedList::default();
            latch.last_alpha = None;
            latch.last_start = None;
        }
    }
    gaps.clear(HudGap::FlashWhiteout);
    if !matches!(*screen, AppScreen::InGame) {
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }
    if !surface.is_ready() {
        request_hide(&mut job, latch.packed.is_empty());
        return;
    }
    let (Some(ps), Some(shock)) = (presented.player(local.0), presented.shellshock(local.0)) else {
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    let remaining = cg_is_flashbanged(
        cg_clock.time(),
        ps.shellshock_time,
        ps.shellshock_duration,
        shock.screen_type,
    );
    let Some((white, shot)) =
        cg_shellshock_flash_blend(remaining, shock.white_fade_ms, shock.shot_fade_ms)
    else {
        latch.last_start = None;
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    if latch.last_start != Some(ps.shellshock_time) {
        latch.last_start = Some(ps.shellshock_time);
        latch.save_sequence = latch.save_sequence.wrapping_add(1);
        latch.last_alpha = None;
    }
    let win_w = surface.width();
    let win_h = surface.height();
    let Some(color) = hud_images.get(
        crate::images::HUD_CHROME_NAMESPACE,
        FLASH_WHITE_IMAGE,
        &mut images,
    ) else {
        gaps.raise(GapCause::FlashWhiteoutImageMissing {
            name: FLASH_WHITE_IMAGE.to_owned(),
            miss: hud_images.miss_reason(),
        });
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    if skip_rewrite(&latch, (white, shot), win_w, win_h) {
        *job = FlashGpuJob::Write;
        return;
    }
    let full_screen = |alpha: f32, site: &'static str| Draw2dList {
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
            color: [1.0, 1.0, 1.0, alpha],
            material: FLASH_WHITE_IMAGE.into(),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw { site },
            layer: 0,
        }],
    };
    let (Some(saved_quad), Some(white_quad)) = (
        tessellate(&full_screen(shot, "flash_saved_screen"))
            .0
            .into_iter()
            .next(),
        tessellate(&full_screen(white, "flash_whiteout"))
            .0
            .into_iter()
            .next(),
    ) else {
        request_hide(&mut job, latch.packed.is_empty());
        return;
    };
    latch.saved = crate::gpu_list::pack_saved_screen(&saved_quad, color.clone());
    latch.packed = crate::gpu_list::pack_modulate(&white_quad, color);
    latch.last_w = Some(win_w);
    latch.last_h = Some(win_h);
    latch.last_alpha = Some((white, shot));
    *job = FlashGpuJob::Write;
}
