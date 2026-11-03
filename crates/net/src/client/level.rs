use bevy::prelude::*;
use std::sync::Arc;

/// Shared match content, installed directly from the prepared session.
#[derive(Resource)]
pub struct ClientLevel {
    pub content: Arc<sim::SimContent>,
    pub schemas: std::collections::BTreeMap<String, Arc<structured_data_iw4::DefinitionSet>>,
    pub fingerprint: crate::ContentFingerprint,
    pub root_seed: u64,
    pub content_digest: u64,
}

impl Default for ClientLevel {
    fn default() -> Self {
        Self {
            content: sim::SimContentBuilder::bootstrap().finish(),
            schemas: Default::default(),
            fingerprint: Default::default(),
            root_seed: 0,
            content_digest: 0,
        }
    }
}

impl ClientLevel {
    pub fn from_world(world: &sim::SimWorld) -> Self {
        Self {
            content: world.content(),
            schemas: world.persistent_data().schemas().clone(),
            fingerprint: crate::ContentFingerprint::from_world(world),
            root_seed: world.root_seed(),
            content_digest: world.content_digest(),
        }
    }
}

#[derive(Resource, Default)]
pub struct ClientCollision(pub sim::CollisionScene);
