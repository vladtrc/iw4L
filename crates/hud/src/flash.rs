use bevy::prelude::*;
use frame::{AppScreen, LifeStarted};
use hud_iw4::{is_flashbanged, shellshock_flash_blend};
use net::FrameClock;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gpu_list::PackedList;
use crate::images::HudImageLoader;

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
    life_sequence: u64,
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FlashOverlay<'w, 's> {
    latch: ResMut<'w, FlashWhiteoutLatch>,
    hud_images: HudImageLoader<'w>,
    job: ResMut<'w, FlashGpuJob>,
    started: MessageReader<'w, 's, LifeStarted>,
    gpu_frame: ResMut<'w, crate::gpu_list::HudTessGpuFrame>,
}

impl FlashOverlay<'_, '_> {
    fn begin_frame(&mut self, client: u32) {
        *self.job = FlashGpuJob::Idle;
        for ev in self.started.read() {
            if ev.client == client {
                self.latch.packed = PackedList::default();
                self.latch.saved = PackedList::default();
                self.latch.last_alpha = None;
                self.latch.last_start = None;
                self.latch.life_sequence = self.latch.life_sequence.wrapping_add(1);
            }
        }
        self.gpu_frame.shellshock_screen = None;
    }

    fn hide(&mut self) {
        request_hide(&mut self.job, self.latch.packed.is_empty());
    }

    fn render(
        &mut self,
        surface: &crate::surface::Hud2dSurface,
        presentation: &net::LocalPresentation,
        effects: &frame::ScreenEffectsView,
        cg_clock: &FrameClock,
    ) {
        let Some(ps) = presentation.player() else {
            self.hide();
            return;
        };
        let shock = presentation.shellshock();
        if !effects.ready || effects.suppressed {
            self.hide();
            return;
        }
        let blend_ms = effects.blend_ms;
        if blend_ms > 0 {
            self.gpu_frame.shellshock_screen = Some(crate::gpu_list::ShellshockScreen {
                client: presentation.client_id().0,
                view_client: ps.kill_cam_client_num as u32,
                life_sequence: self.latch.life_sequence,
                authoritative_life: presentation
                    .snapshot()
                    .and_then(|snapshot| snapshot.meta.for_client(presentation.client_id()))
                    .map_or(0, |meta| meta.life_sequence.0),
                time_ms: cg_clock.time(),
                blend_ms,
            });
        }
        let Some(shock) = shock.filter(|_| effects.flashed) else {
            self.latch.last_start = None;
            self.hide();
            return;
        };
        let remaining = is_flashbanged(
            cg_clock.time(),
            ps.shellshock_time,
            ps.shellshock_duration,
            shock.screen_type,
        );
        let Some((white, shot)) =
            shellshock_flash_blend(remaining, shock.white_fade_ms, shock.shot_fade_ms)
        else {
            self.latch.last_start = None;
            self.hide();
            return;
        };
        if self.latch.last_start != Some(ps.shellshock_time) {
            self.latch.last_start = Some(ps.shellshock_time);
            self.latch.save_sequence = self.latch.save_sequence.wrapping_add(1);
            self.latch.last_alpha = None;
        }
        let win_w = surface.width();
        let win_h = surface.height();
        let Some(color) = self
            .hud_images
            .load(crate::images::HUD_CHROME_NAMESPACE, FLASH_WHITE_IMAGE)
        else {
            diag::warn_once!(
                Ui,
                "flash white image `{name}` is {miss}",
                name = FLASH_WHITE_IMAGE.to_owned(),
                miss = self.hud_images.miss_reason()
            );
            self.hide();
            return;
        };
        if skip_rewrite(&self.latch, (white, shot), win_w, win_h) {
            *self.job = FlashGpuJob::Write;
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
            self.hide();
            return;
        };
        self.latch.saved = crate::gpu_list::pack_saved_screen(&saved_quad, color.clone());
        self.latch.packed = crate::gpu_list::pack_modulate(&white_quad, color);
        self.latch.last_w = Some(win_w);
        self.latch.last_h = Some(win_h);
        self.latch.last_alpha = Some((white, shot));
        *self.job = FlashGpuJob::Write;
    }
}

pub(crate) fn update_flash_whiteout(
    cg_clock: Res<FrameClock>,
    screen: Res<AppScreen>,
    surface: Res<crate::surface::Hud2dSurface>,
    presentation: net::LocalPresentation,
    mut overlay: FlashOverlay,
    effects: Res<frame::ScreenEffectsView>,
) {
    overlay.begin_frame(presentation.client_id().0);
    if !matches!(*screen, AppScreen::InGame) {
        overlay.hide();
        return;
    }
    if !surface.is_ready() {
        overlay.hide();
        return;
    }
    overlay.render(&surface, &presentation, &effects, &cg_clock);
}
