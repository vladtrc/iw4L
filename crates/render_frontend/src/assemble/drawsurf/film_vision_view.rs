use bevy::prelude::*;

use super::dof::GlowDvars;

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct FilmVisionView {
    pub current: Option<assets::FilmVision>,
    from: hud_iw4::VisionSetVars,
    to: hud_iw4::VisionSetVars,
    result: hud_iw4::VisionSetVars,
    lerp: hud_iw4::VisionSetLerpData,
    last_map: Option<assets::FilmVision>,
    override_vision: Option<assets::FilmVision>,
}

impl Default for FilmVisionView {
    fn default() -> Self {
        Self {
            current: None,
            from: hud_iw4::VisionSetVars::default(),
            to: hud_iw4::VisionSetVars::default(),
            result: hud_iw4::VisionSetVars::default(),
            lerp: hud_iw4::VisionSetLerpData::default(),
            last_map: None,
            override_vision: None,
        }
    }
}

impl FilmVisionView {
    /// Select a preset, or restore the map with None. Call only for a ready view.
    pub fn select(
        &mut self,
        map: Option<assets::FilmVision>,
        preset: Option<assets::FilmVision>,
        now_ms: i32,
        duration_ms: i32,
        allowed: bool,
        script_forced: bool,
    ) {
        if self.last_map.is_none() {
            self.result = pack_film_vision(map.unwrap_or_default());
            self.lerp.style = hud_iw4::VISION_SET_LERP_HOLD;
        }
        let (current, _) = hud_iw4::cg_vision_sets_update(
            now_ms,
            self.from,
            self.to,
            self.lerp,
            self.result,
            allowed,
            script_forced,
        );
        let target = preset.or(map).unwrap_or_default();
        (self.from, self.to, self.lerp) = hud_iw4::cg_vision_set_start(
            now_ms,
            duration_ms,
            hud_iw4::VISION_SET_LERP_TO_SMOOTH,
            self.lerp.style,
            current,
            pack_film_vision(target),
        );
        self.result = if duration_ms <= 0 { self.to } else { current };
        self.last_map = Some(target);
        self.override_vision = preset;
    }
}

pub fn pack_film_vision(vision: assets::FilmVision) -> hud_iw4::VisionSetVars {
    hud_iw4::VisionSetVars {
        r_glow: vision.glow_enable,
        r_glow_bloom_cutoff: vision.glow_bloom_cutoff,
        r_glow_bloom_desaturation: vision.glow_bloom_desaturation,
        r_glow_bloom_intensity0: vision.glow_bloom_intensity,
        r_glow_radius0: vision.glow_radius,
        r_film_enable: vision.enable,
        r_film_brightness: vision.brightness,
        r_film_contrast: vision.contrast,
        r_film_desaturation: vision.desaturation,
        r_film_desaturation_dark: vision.desaturation_dark,
        r_film_invert: vision.invert,
        r_film_light_tint: vision.light_tint,
        r_film_medium_tint: vision.medium_tint,
        r_film_dark_tint: vision.dark_tint,
        ..hud_iw4::VisionSetVars::default()
    }
}

pub fn unpack_film_vision(vars: hud_iw4::VisionSetVars) -> assets::FilmVision {
    assets::FilmVision {
        enable: vars.r_film_enable,
        contrast: vars.r_film_contrast,
        brightness: vars.r_film_brightness,
        desaturation: vars.r_film_desaturation,
        desaturation_dark: vars.r_film_desaturation_dark,
        invert: vars.r_film_invert,
        light_tint: vars.r_film_light_tint,
        medium_tint: vars.r_film_medium_tint,
        dark_tint: vars.r_film_dark_tint,
        glow_enable: vars.r_glow,
        glow_radius: vars.r_glow_radius0,
        glow_bloom_cutoff: vars.r_glow_bloom_cutoff,
        glow_bloom_desaturation: vars.r_glow_bloom_desaturation,
        glow_bloom_intensity: vars.presented_glow_intensity0(),
    }
}

#[must_use]
pub fn presented_film_vision(
    view_ready: bool,
    map_vision: Option<assets::FilmVision>,
) -> Option<assets::FilmVision> {
    if view_ready { map_vision } else { None }
}

#[must_use]
pub fn presented_film_vision_with_glow_tweaks(
    view_ready: bool,
    map_vision: Option<assets::FilmVision>,
    use_tweaks: bool,
    tweaks: lighting_iw4::GlowViewInfo,
) -> Option<assets::FilmVision> {
    if !view_ready {
        return None;
    }
    if !use_tweaks {
        return map_vision;
    }
    let mut vision = map_vision.unwrap_or_default();
    let selected = lighting_iw4::r_select_glow_view_info(
        lighting_iw4::GlowViewInfo {
            enable: vision.glow_enable,
            cutoff: vision.glow_bloom_cutoff,
            desaturation: vision.glow_bloom_desaturation,
            intensity: vision.glow_bloom_intensity,
            radius: vision.glow_radius,
        },
        true,
        tweaks,
    );
    vision.glow_enable = selected.enable;
    vision.glow_bloom_cutoff = selected.cutoff;
    vision.glow_bloom_desaturation = selected.desaturation;
    vision.glow_bloom_intensity = selected.intensity;
    vision.glow_radius = selected.radius;
    Some(vision)
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn presented_film_vision_with_lerp(
    view_ready: bool,
    map_vision: Option<assets::FilmVision>,
    now_ms: i32,
    duration_ms: i32,
    slot: &mut FilmVisionView,
    use_tweaks: bool,
    tweaks: lighting_iw4::GlowViewInfo,
    allowed: bool,
    script_forced: bool,
) -> Option<assets::FilmVision> {
    if !view_ready {
        *slot = FilmVisionView::default();
        return None;
    }
    let Some(map) = map_vision else {
        *slot = FilmVisionView::default();
        return None;
    };
    if slot.last_map != Some(map) {
        let (from, to, lerp) = hud_iw4::cg_vision_set_start(
            now_ms,
            duration_ms,
            hud_iw4::VISION_SET_LERP_TO_LINEAR,
            slot.lerp.style,
            slot.result,
            pack_film_vision(map),
        );
        slot.from = from;
        slot.to = to;
        slot.lerp = lerp;
        slot.last_map = Some(map);
        if slot.lerp.style == hud_iw4::VISION_SET_LERP_HOLD {
            slot.result = slot.to;
        }
    }
    let (vars, lerp) = hud_iw4::cg_vision_sets_update(
        now_ms,
        slot.from,
        slot.to,
        slot.lerp,
        slot.result,
        allowed,
        script_forced,
    );
    slot.lerp = lerp;
    slot.result = vars;
    let mixed = unpack_film_vision(vars);
    presented_film_vision_with_glow_tweaks(true, Some(mixed), use_tweaks, tweaks)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntroStage {
    Waiting,
    Starting,
    Returning,
    Playing,
}

fn intro_stage(phase: sim::MatchPhase, prematch: gamemode_iw4::PrematchStep) -> (IntroStage, i32) {
    if phase != sim::MatchPhase::Warmup {
        return (IntroStage::Playing, 0);
    }
    match prematch {
        gamemode_iw4::PrematchStep::Starting { elapsed_ms } => {
            let return_at = gamemode_iw4::MATCH_START_MS.saturating_sub(2000) + 100;
            if elapsed_ms >= return_at {
                (
                    IntroStage::Returning,
                    elapsed_ms.saturating_sub(return_at) as i32,
                )
            } else {
                (IntroStage::Starting, 0)
            }
        }
        _ => (IntroStage::Waiting, 0),
    }
}

#[derive(Resource, Default)]
struct MatchIntroVision {
    stage: Option<IntroStage>,
}

pub fn register(app: &mut App) {
    app.init_resource::<MatchIntroVision>();
    app.init_resource::<FilmVisionView>().add_systems(
        Update,
        update_film_vision_view
            .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
            .in_set(net::ClientSet::Present),
    );
}

fn update_film_vision_view(
    view: Res<crate::prepare::scene::view_parms::PreparedSceneView>,
    scene: Res<crate::prepare::scene::world::WorldScene>,
    glow: Res<GlowDvars>,
    clock: Res<net::CgFrameClock>,
    mut film: ResMut<FilmVisionView>,
    presented: Res<net::PresentedSnapshot>,
    mut intro: ResMut<MatchIntroVision>,
) {
    if !view.ready {
        intro.stage = None;
    } else {
        let (stage, age_ms) = presented
            .snapshot()
            .map_or((IntroStage::Waiting, 0), |snapshot| {
                intro_stage(snapshot.meta.phase, snapshot.meta.prematch)
            });
        if intro.stage != Some(stage) {
            match stage {
                IntroStage::Waiting | IntroStage::Starting => {
                    match scene.film_visions.get("vision/mpintro.vision") {
                        Some(Ok(preset)) => film.select(
                            scene.film_vision,
                            Some(*preset),
                            clock.time(),
                            0,
                            glow.allowed,
                            glow.allowed_script_forced,
                        ),
                        Some(Err(error)) => diag::warn!(World, "match intro vision: {error:?}"),
                        None => {}
                    }
                }
                IntroStage::Returning => {
                    if intro.stage.is_none()
                        && let Some(Ok(preset)) = scene.film_visions.get("vision/mpintro.vision")
                    {
                        film.select(
                            scene.film_vision,
                            Some(*preset),
                            clock.time() - age_ms,
                            0,
                            glow.allowed,
                            glow.allowed_script_forced,
                        );
                    }
                    film.select(
                        scene.film_vision,
                        None,
                        clock.time() - age_ms,
                        3000,
                        glow.allowed,
                        glow.allowed_script_forced,
                    );
                }
                IntroStage::Playing if intro.stage.is_some() => {
                    let duration = if intro.stage == Some(IntroStage::Returning) {
                        3000
                    } else {
                        0
                    };
                    film.select(
                        scene.film_vision,
                        None,
                        clock.time(),
                        duration,
                        glow.allowed,
                        glow.allowed_script_forced,
                    );
                }
                IntroStage::Playing => {}
            }
            intro.stage = Some(stage);
        }
    }
    let mixed = presented_film_vision_with_lerp(
        view.ready,
        film.override_vision.or(scene.film_vision),
        clock.time(),
        0,
        &mut film,
        glow.use_tweaks,
        glow.tweak_view_info(),
        glow.allowed,
        glow.allowed_script_forced,
    );
    film.current = mixed;
}
