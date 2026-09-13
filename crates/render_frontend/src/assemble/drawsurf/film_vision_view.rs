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
        }
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

pub fn register(app: &mut App) {
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
) {
    let mixed = presented_film_vision_with_lerp(
        view.ready,
        scene.film_vision,
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
