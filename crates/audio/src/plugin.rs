use bevy::prelude::*;

use crate::playback::PlayerSoundPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        crate::backend::register(app);
        app.add_plugins(PlayerSoundPlugin);
        crate::match_set::register(app);
        crate::frontend::register_frontend_audio(app);
        crate::entity_events::register_entity_event_audio(app);
        crate::weapon_lock::register(app);
        crate::rumble::register(app);
    }
}
