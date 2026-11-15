use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitLevelCalled;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CampaignLevelCalled(pub String);

pub fn register_script_notify(app: &mut App) {
    app.add_message::<ExitLevelCalled>();
    app.add_message::<CampaignLevelCalled>();
}
