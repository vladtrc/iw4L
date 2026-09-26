use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEnded;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitLevelCalled;

pub fn register_script_notify(app: &mut App) {
    app.add_message::<GameEnded>()
        .add_message::<ExitLevelCalled>();
}
