use bevy::prelude::*;

pub mod audio;
pub mod input;
pub mod menu;

pub use audio::{UiPlayMusic, UiPlaySound, UiStopMusic};
pub use input::{UiBindRequest, UiBindingCapture};
pub use menu::{
    HostMatchRules, NativeGameMenu, UiExecCommand, UiMenuDvars, UiMenuKey, UiMenuRequest,
    UiPartyState, UnifiedFrontend,
};

pub fn register_ui_contracts(app: &mut App) {
    app.init_resource::<UiMenuDvars>()
        .init_resource::<UnifiedFrontend>()
        .init_resource::<NativeGameMenu>()
        .init_resource::<UiBindingCapture>()
        .init_resource::<UiPartyState>()
        .add_message::<UiBindRequest>()
        .add_message::<UiPlaySound>()
        .add_message::<UiPlayMusic>()
        .add_message::<UiStopMusic>()
        .add_message::<UiExecCommand>()
        .add_message::<UiMenuRequest>();
}
