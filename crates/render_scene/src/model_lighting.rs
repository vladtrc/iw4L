use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelLightingOwner {
    Eye,
    RemoteClient(u16),
    Corpse(Entity),
    ScriptModel(Entity),

    Missile(u32),

    PredictedMissile { owner: u32, weapon: u32 },

    Item(u32),

    FxModel(u16),

    Glass(u16),

    DynEnt(Entity),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLightingRequest {
    pub owner: ModelLightingOwner,
    pub origin: [f32; 3],
    pub lookup_fallback: u8,
}

#[derive(Resource, Debug, Default)]
pub struct ModelLightingRequests {
    pending: Vec<ModelLightingRequest>,
}

impl ModelLightingRequests {
    pub fn request(&mut self, req: ModelLightingRequest) -> ModelLightingRequest {
        self.pending.push(req);
        req
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn take_pending(&mut self) -> Vec<ModelLightingRequest> {
        std::mem::take(&mut self.pending)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedModelLighting {
    Seated {
        handle: u32,
        scene_light_index: u8,
        reflection_probe_index: u8,
        packed_lighting: Option<[u8; 4]>,
    },
    Failed,
}

#[derive(Resource, Debug, Default)]
pub struct ResolvedModelLightingTable {
    by_owner: HashMap<ModelLightingOwner, ResolvedModelLighting>,
}

impl ResolvedModelLightingTable {
    pub fn get(&self, owner: ModelLightingOwner) -> Option<ResolvedModelLighting> {
        self.by_owner.get(&owner).copied()
    }

    pub fn clear(&mut self) {
        self.by_owner.clear();
    }

    pub fn insert_if_absent(&mut self, owner: ModelLightingOwner, value: ResolvedModelLighting) {
        self.by_owner.entry(owner).or_insert(value);
    }

    pub fn get_or_insert_with(
        &mut self,
        owner: ModelLightingOwner,
        compute: impl FnOnce() -> ResolvedModelLighting,
    ) {
        self.by_owner.entry(owner).or_insert_with(compute);
    }
}
