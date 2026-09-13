use std::sync::Mutex;

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct EntityMarks {
    inner: Mutex<EntityMarkStore>,
}

#[derive(Default)]
pub struct EntityMarkStore {
    pub pending: Vec<EntityMarkRequest>,
    pub attached: Vec<EntityMarkAttachment>,
    pub unsupported_triangles: u64,
    pub unsupported_receivers: u64,
}

pub struct EntityMarkRequest {
    pub entity: u16,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub radius: f32,
    pub material: String,
    pub color: u32,
}

pub struct EntityMarkAttachment {
    pub entity: u16,
    pub model: String,
    pub revision: u32,
    pub bone: usize,
    pub handle: u16,
}

impl EntityMarks {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, EntityMarkStore> {
        self.inner.lock().expect("entity marks")
    }
}
