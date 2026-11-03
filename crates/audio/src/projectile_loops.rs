use bevy::prelude::*;

use crate::backend::AudioScope;
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{AudioRuntime, SoundBank};

#[derive(Component)]
pub(crate) struct ProjectileLoop {
    weapon: u32,
    pub source: DesiredSource,
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
pub(crate) fn update(
    mut commands: Commands,
    presented: Res<net::PresentedSnapshot>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    bank: Option<Res<SoundBank>>,
    clock: Option<Res<net::FrameClock>>,
    runtime: Res<AudioRuntime>,
    mut entities: Query<
        (
            Entity,
            Option<&net::ProjectileMotion>,
            Option<&net::CEntity>,
            Option<&mut ProjectileLoop>,
        ),
        Or<(With<net::ProjectileMotion>, With<ProjectileLoop>)>,
    >,
) {
    let bound = weapons.as_ref().map(|weapons| weapons.bound());
    let at_time = clock
        .filter(|clock| clock.started())
        .map(|clock| clock.time())
        .unwrap_or_else(|| presented.tick().map(sim::level_time_ms).unwrap_or(0));
    for (entity, motion, identity, current) in &mut entities {
        let (Some(motion), Some(bound)) = (motion, bound.as_ref()) else {
            if current.is_some() {
                commands.entity(entity).remove::<ProjectileLoop>();
            }
            continue;
        };
        let number = identity.map(|identity| identity.number());
        let origin = Some(
            match number.and_then(|number| presented.projectile(number)) {
                Some(projectile) => presented.projectile_origin_at(projectile, at_time),
                None => motion.origin_at(at_time),
            },
        );
        if let Some(mut current) = current {
            if current.weapon == motion.weapon {
                current.source.origin_inches = origin;
                continue;
            }
            commands.entity(entity).remove::<ProjectileLoop>();
        }
        let Some(bank) = &bank else {
            continue;
        };
        let Some((namespace, alias)) = bound.registry().weapon_sound_key(
            motion.weapon,
            asset_game::WeaponSoundSlot::Projectile,
            &bank.0,
        ) else {
            continue;
        };
        let Some(cue) = runtime.source_cue(SourceCueRequest {
            bank: bank.0.clone(),
            namespace,
            alias: alias.into(),
            emitter: number.map(u32::from),
            scope: AudioScope::Match,
            group: None,
        }) else {
            continue;
        };
        commands.entity(entity).insert(ProjectileLoop {
            weapon: motion.weapon,
            source: DesiredSource {
                key: SourceKey {
                    scope: AudioScope::Match,
                    object: entity.to_bits(),
                    slot: 7,
                },
                version: 1,
                cue,
                origin_inches: origin,
                start_frame: runtime.audio_frame(),
                gain: 1.0,
                rate: 1.0,
                audible: true,
            },
        });
    }
}

pub(crate) fn stop(
    event: On<Remove<ProjectileLoop>>,
    loops: Query<&ProjectileLoop>,
    runtime: Res<AudioRuntime>,
) {
    if let Ok(source) = loops.get(event.entity) {
        runtime.stop_source(source.source.key);
    }
}
