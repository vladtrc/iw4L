use bevy::prelude::Resource;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SkinFailure {
    pub reason: Option<&'static str>,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WorldPresentAck {
    pub at: Option<std::time::Instant>,
}
