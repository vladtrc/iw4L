use crate::ambient::{MapAmbient, MapEmitter};
use crate::clip_store::{ClipStore, clip_keys_for_alias};
use crate::pcm::PcmAudio;
use crate::playback::{MissingAliasGaps, SharedPlayAssets, SoundBank};
use assets::AssetNamespace;
use bevy::prelude::*;
use net::PresentedSnapshot;

pub(crate) const RADIATION_DOOR_ALIASES: &[&str] = &[
    "evt_hydraulic_switch",
    "evt_hydraulic_start",
    "evt_hydraulic_loop",
    "evt_hydraulic_open",
    "evt_hydraulic_close",
];

#[derive(Component)]
pub(crate) struct DoorLoop;

pub(crate) fn update(
    mut commands: Commands,
    presented: Res<PresentedSnapshot>,
    mut playing: Query<(Entity, &mut MapEmitter), With<DoorLoop>>,
    bank: Option<Res<SoundBank>>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pcm: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut gaps: ResMut<MissingAliasGaps>,
) {
    let doors = presented.snapshot().and_then(|s| s.meta.map_doors.as_ref());
    let elapsed = presented.snapshot().and_then(|s| {
        doors.and_then(|d| {
            d.started_at
                .map(|at| s.tick.0.saturating_mul(50).saturating_sub(at))
        })
    });

    let moving = elapsed.is_some_and(|ms| ms < 9000);
    let fade = elapsed.map_or(0.0, |ms| {
        if ms < 500 {
            ms as f32 / 500.0
        } else if ms < 8000 {
            1.0
        } else {
            ((9000.0 - ms as f32) / 1000.0).max(0.0)
        }
    });
    if !moving {
        for (entity, _) in &playing {
            commands.entity(entity).try_despawn();
        }
        return;
    }
    if !playing.is_empty() {
        if let Some(bank) = bank.as_ref()
            && let Some(row) = bank
                .0
                .sound_in(AssetNamespace::T5, "evt_hydraulic_loop")
                .or_else(|| bank.0.sound("evt_hydraulic_loop"))
                .and_then(|s| s.aliases.first())
        {
            for (_, mut emitter) in &mut playing {
                emitter.base_gain = row.vol_min.max(0.0) * fade;
            }
        }
        return;
    }
    let Some(bank) = bank else {
        return;
    };
    let Some(clips) = clips.as_mut() else {
        return;
    };
    let alias = "evt_hydraulic_loop";
    let key = clip_keys_for_alias(&bank.0, AssetNamespace::T5, alias)
        .into_iter()
        .next()
        .or_else(|| {
            clip_keys_for_alias(&bank.0, AssetNamespace::Iw4, alias)
                .into_iter()
                .next()
        });
    let Some(key) = key else {
        gaps.record(alias);
        return;
    };
    clips.request(key.clone());
    let Some(audio) = clips.ready(&key) else {
        return;
    };
    let Ok(audio) = audio else {
        gaps.record(alias);
        return;
    };
    let Some(row) = bank
        .0
        .sound_in(AssetNamespace::T5, alias)
        .or_else(|| bank.0.sound(alias))
        .and_then(|s| s.aliases.first())
    else {
        gaps.record(alias);
        return;
    };
    let Some(curve) = row.volume_falloff.as_ref() else {
        gaps.record(alias);
        return;
    };
    let origin_inches = doors.expect("moving doors").leaves[0].origin;
    commands.spawn((
        DoorLoop,
        MapAmbient,
        MapEmitter {
            origin_inches,
            dist_min: row.dist_min,
            dist_max: row.dist_max,
            knots: shared.intern_curve(&curve.name, &curve.knots),
            base_gain: row.vol_min.max(0.0) * fade,
            pcm: pcm.add(audio),
            live_pan: None,
        },
        Transform::from_translation(Vec3::from_array(origin_inches)),
    ));
}
