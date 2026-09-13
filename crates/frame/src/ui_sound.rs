use bevy::prelude::*;

#[derive(Message, Clone, Debug)]
pub struct UiPlaySound {
    pub alias: String,
}

#[derive(Message, Clone, Debug)]
pub struct UiPlayMusic {
    pub alias: String,
}

#[derive(Message, Clone, Debug, Default)]
pub struct UiStopMusic;

pub fn register_ui_sound(app: &mut App) {
    app.add_message::<UiPlaySound>()
        .add_message::<UiPlayMusic>()
        .add_message::<UiStopMusic>();
}
