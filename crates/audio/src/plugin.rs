use bevy::prelude::*;

use crate::playback::PlayerSoundPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        crate::backend::register(app);
        app.add_plugins(PlayerSoundPlugin);
        app.add_observer(retire_entity_audio);
        crate::match_set::register(app);
        crate::frontend::register_frontend_audio(app);
        crate::entity_events::register_entity_event_audio(app);
        crate::rumble::register(app);
        crate::script_music::register(app);
        crate::script_mix::register(app);
    }
}

fn retire_entity_audio(
    event: On<Remove<net::CEntity>>,
    identities: Query<&net::CEntity>,
    runtime: Option<Res<crate::AudioRuntime>>,
    mut commands: Commands,
    destructibles: Option<ResMut<crate::destructible_loops::DestructibleSources>>,
) {
    let Ok(identity) = identities.get(event.entity) else {
        return;
    };
    let emitter = u32::from(identity.number());
    if let Some(runtime) = runtime.as_ref() {
        runtime.stop_emitter_loops(emitter);
    }
    let entity = event.entity;
    commands.queue(move |world: &mut World| {
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            entity.remove::<(
                crate::projectile_loops::ProjectileLoop,
                crate::flame_loops::FlameStream,
            )>();
        }
    });
    if let (Some(mut sources), Some(runtime)) = (destructibles, runtime.as_deref()) {
        sources.retire_emitter(emitter, runtime);
    }
}
