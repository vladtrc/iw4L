use std::collections::HashMap;

use asset_iw4::{SND_ENTCHANNEL_MAX, snd_entity_channel_matches};
use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct VoiceLease {
    pub channel: u32,
    pub snd_ent: Option<u32>,
}

#[derive(Resource, Debug)]
pub struct VoiceOccupancy {
    counts: [i32; SND_ENTCHANNEL_MAX],
    live: HashMap<Entity, VoiceLease>,
}

impl Default for VoiceOccupancy {
    fn default() -> Self {
        Self {
            counts: [0; SND_ENTCHANNEL_MAX],
            live: HashMap::new(),
        }
    }
}

impl VoiceOccupancy {
    pub fn voice_count(&self, channel: u32) -> i32 {
        self.counts.get(channel as usize).copied().unwrap_or(0)
    }

    pub fn track(&mut self, entity: Entity, lease: VoiceLease) {
        if let Some(count) = self.counts.get_mut(lease.channel as usize) {
            *count = count.saturating_add(1);
        }
        self.live.insert(entity, lease);
    }

    pub fn reclaim(&mut self, entity: Entity) {
        let Some(lease) = self.live.remove(&entity) else {
            return;
        };
        if let Some(count) = self.counts.get_mut(lease.channel as usize) {
            *count = (*count - 1).max(0);
        }
    }

    pub fn take_entity_channel(&mut self, snd_ent: u32, channel: u32) -> Vec<Entity> {
        let mut out = Vec::new();
        self.live.retain(|&entity, lease| {
            let Some(occupant_ent) = lease.snd_ent else {
                return true;
            };
            if snd_entity_channel_matches(occupant_ent, lease.channel, snd_ent, channel) {
                if let Some(count) = self.counts.get_mut(lease.channel as usize) {
                    *count = (*count - 1).max(0);
                }
                out.push(entity);
                false
            } else {
                true
            }
        });
        out
    }
}

pub(crate) fn reclaim_finished_voices(
    mut occupancy: ResMut<VoiceOccupancy>,
    mut removed: RemovedComponents<VoiceLease>,
) {
    for entity in removed.read() {
        occupancy.reclaim(entity);
    }
}
