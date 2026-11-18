use bevy::prelude::*;
use frame::{AppScreen, LifeStarted, UiDraw, ViewSubject};
use hud_iw4::{
    HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT, blood_overlay_lerp, get_health_fraction,
    should_draw_blood_overlay,
};
use net::FrameClock;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gpu_list::{PackedList, pack_splatter_alt};
use crate::images::{HudImages, HudSampling};

#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct HudRootVisible(pub Option<i32>);

#[derive(Resource, Default)]
pub(crate) struct BloodOverlayLatch {
    intensity: f32,
    subject: Option<ViewSubject>,
    last_time: Option<i32>,
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct BloodOverlay<'w, 's> {
    latch: ResMut<'w, BloodOverlayLatch>,
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    job: ResMut<'w, BloodGpuJob>,
    started: MessageReader<'w, 's, LifeStarted>,
}

impl BloodOverlay<'_, '_> {
    fn begin_frame(&mut self, client: u32, view: ViewSubject, cg_clock: &FrameClock) {
        *self.job = BloodGpuJob::Idle;
        for ev in self.started.read() {
            if ev.client == client {
                self.latch.intensity = 0.0;
            }
        }
        let now = cg_clock.time();
        if self.latch.subject != Some(view) || self.latch.last_time.is_some_and(|time| now < time) {
            self.latch.intensity = 0.0;
        }
        self.latch.subject = Some(view);
        self.latch.last_time = Some(now);
    }

    fn hide(&mut self) {
        request_hide(&mut self.job, self.latch.packed.is_empty());
    }

    fn reset_intensity(&mut self) {
        self.latch.intensity = 0.0;
    }

    fn render(&mut self, health_frac: f32, cg_clock: &FrameClock, win_w: f32, win_h: f32) {
        let frametime_ms = cg_clock.frametime();
        self.latch.intensity = blood_overlay_lerp(
            self.latch.intensity,
            health_frac,
            frametime_ms,
            HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT,
        );

        if self.latch.intensity == 0.0 {
            self.hide();
            return;
        }

        if gpu_stretch_bits_unchanged(&self.latch, self.latch.intensity, win_w, win_h) {
            *self.job = BloodGpuJob::Show;
            return;
        }

        let binding = match self.hud_images.blood_material_binding() {
            Ok(binding) => binding,
            Err(detail) => {
                diag::warn_once!(Ui, "{detail}");
                self.hide();
                return;
            }
        };

        let Some(color) = self.hud_images.get_sampled_with_sampler(
            crate::images::HUD_CHROME_NAMESPACE,
            binding.assets.color,
            HudSampling::Data,
            Some(binding.color_sampler),
            &mut self.images,
        ) else {
            diag::warn_once!(
                Ui,
                "overlay image `{name}` is {miss}",
                name = binding.assets.color.to_owned(),
                miss = self.hud_images.miss_reason()
            );
            self.hide();
            return;
        };
        let Some(mask) = self.hud_images.get_sampled_with_sampler(
            crate::images::HUD_CHROME_NAMESPACE,
            binding.assets.mask,
            HudSampling::Data,
            Some(binding.mask_sampler),
            &mut self.images,
        ) else {
            diag::warn_once!(
                Ui,
                "overlay image `{name}` is {miss}",
                name = binding.assets.mask.to_owned(),
                miss = self.hud_images.miss_reason()
            );
            self.hide();
            return;
        };

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
                color: [1.0, 1.0, 1.0, self.latch.intensity],
                material: binding.assets.color.into(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw {
                    site: "blood_overlay",
                },
                layer: 0,
            }],
        };
        let Some(quad) = tessellate(&list).0.into_iter().next() else {
            self.hide();
            return;
        };
        self.latch.packed = pack_splatter_alt(&quad, color, mask, binding.state);
        self.latch.last_w = Some(win_w);
        self.latch.last_h = Some(win_h);
        self.latch.last_gpu_intensity = Some(self.latch.intensity);
        *self.job = BloodGpuJob::Write;
    }
}

pub(crate) fn update_blood_overlay(
    cg_clock: Res<FrameClock>,
    screen: Res<AppScreen>,
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    mut overlay: BloodOverlay,
    view: Res<ViewSubject>,
    ui_draw: Option<Res<UiDraw>>,
) {
    overlay.begin_frame(presentation.client_id().0, *view, &cg_clock);
    let in_game = matches!(*screen, AppScreen::InGame);
    let Some(ps) = presentation.player() else {
        overlay.reset_intensity();

        overlay.hide();
        return;
    };
    let health_frac = get_health_fraction(ps.health, ps.max_health, ps.pm_type);
    let in_killcam = view.in_killcam();
    let in_killcam_hud_gate = in_killcam && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE;

    if !in_game {
        overlay.reset_intensity();

        overlay.hide();
        return;
    }

    if !surface.is_ready() {
        overlay.hide();
        return;
    }
    let win_w = surface.width();
    let win_h = surface.height();

    let blood_enabled = true;
    let should_draw_hud = ui_draw.is_some_and(|d| d.0);
    let should_draw = should_draw_blood_overlay(
        blood_enabled,
        should_draw_hud,
        in_killcam_hud_gate,
        ps.pm_type,
    );
    if !should_draw {
        overlay.hide();
        return;
    }

    overlay.render(health_frac, &cg_clock, win_w, win_h);
}
