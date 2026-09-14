use bevy::prelude::*;
use net::CgFrameClock;
use render_frame::{SunEffectsFrame, angular_lerp};

use super::command_context::MapSunEffects;
use crate::prepare::scene::camera::FpvLens;
use crate::prepare::scene::view_parms::PreparedSceneView;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SunEffectsFrameInput {
    pub frame: Option<SunEffectsFrame>,
}

#[derive(Default)]
pub(crate) struct SunEffectsCameraTrack {
    world_generation: Option<u64>,
    camera_key: u64,
    origin: Vec3,
    armed: bool,
}

const CAMERA_CUT_ORIGIN: f32 = 512.0;

pub(crate) fn publish_sun_effects_frame(
    mut published: ResMut<SunEffectsFrameInput>,
    mut track: Local<SunEffectsCameraTrack>,
    sun: Option<Res<MapSunEffects>>,
    prepared: Option<Res<PreparedSceneView>>,
    clock: Option<Res<CgFrameClock>>,
    world_generation: Option<Res<frame::WorldGeneration>>,
    cameras: Query<(Entity, &GlobalTransform), (With<Camera3d>, Without<FpvLens>)>,
) {
    let Some(def) = sun.and_then(|sun| sun.def) else {
        *published = SunEffectsFrameInput::default();
        *track = SunEffectsCameraTrack::default();
        return;
    };
    let Some(view) = prepared.filter(|view| view.ready) else {
        *published = SunEffectsFrameInput::default();
        return;
    };
    let generation = world_generation.and_then(|generation| generation.0);
    let (camera_key, origin) = cameras
        .iter()
        .next()
        .map(|(entity, transform)| (entity.to_bits(), transform.translation()))
        .unwrap_or((0, view.eye));
    let origin_delta = if track.armed {
        origin.distance(track.origin)
    } else {
        0.0
    };
    let camera_cut = !track.armed
        || track.world_generation != generation
        || track.camera_key != camera_key
        || origin_delta > CAMERA_CUT_ORIGIN;
    track.world_generation = generation;
    track.camera_key = camera_key;
    track.origin = origin;
    track.armed = true;

    let direction = Vec3::from_array(def.direction);
    let view_dot = direction.dot(view.forward);
    let clip = view.clip_from_world * direction.extend(0.0);
    let behind_camera = !(clip.w.is_finite() && clip.w > 0.0);
    let flare_lerp = angular_lerp(view_dot, def.flare_min_dot, def.flare_max_dot);
    let blind_lerp = if def.blind_max_darken > 0.0 {
        angular_lerp(view_dot, def.blind_min_dot, def.blind_max_dot)
    } else {
        0.0
    };
    let glare_lerp = if def.glare_max_lighten > 0.0 {
        angular_lerp(view_dot, def.glare_min_dot, def.glare_max_dot)
    } else {
        0.0
    };
    let dt_ms = clock
        .map(|clock| clock.frametime())
        .filter(|dt| *dt > 0)
        .unwrap_or(10);
    published.frame = Some(SunEffectsFrame {
        world_generation: generation,
        camera_key,
        camera_cut,
        dt_ms,
        view_dot,
        clip: clip.to_array(),
        viewport_w: view.rt_w.max(1) as f32,
        viewport_h: view.rt_h.max(1) as f32,
        sprite_size: def.sprite_size.max(0.0),
        flare_size: def.flare_min_size + flare_lerp * def.flare_max_size,
        flare_alpha: (flare_lerp * def.flare_max_alpha).clamp(0.0, 1.0),
        flare_fade_in_ms: def.flare_fade_in_ms.max(0),
        flare_fade_out_ms: def.flare_fade_out_ms.max(0),
        blind_goal: (blind_lerp * def.blind_max_darken).clamp(0.0, 1.0),
        blind_fade_in_ms: def.blind_fade_in_ms.max(0),
        blind_fade_out_ms: def.blind_fade_out_ms.max(0),
        glare_goal: (glare_lerp * def.glare_max_lighten).clamp(0.0, 1.0),
        glare_fade_in_ms: def.glare_fade_in_ms.max(0),
        glare_fade_out_ms: def.glare_fade_out_ms.max(0),
        behind_camera,
    });
}
