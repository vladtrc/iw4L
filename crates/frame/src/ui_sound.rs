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

#[derive(Message, Clone, Debug)]
pub struct UiExecCommand {
    pub text: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum UiMenuRequest {
    Toggle,
    Open(String),
    Close(String),
    Key(UiMenuKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMenuKey {
    Escape,
    Enter,
    Up,
    Down,
}

pub fn register_ui_sound(app: &mut App) {
    app.add_message::<UiPlaySound>()
        .add_message::<UiPlayMusic>()
        .add_message::<UiStopMusic>()
        .add_message::<UiExecCommand>()
        .add_message::<UiMenuRequest>();
}
