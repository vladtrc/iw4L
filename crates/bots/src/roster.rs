use bevy::prelude::Resource;
use sim::{ClassId, ClientId};

use crate::brain::DumbBrain;

#[derive(Resource, Debug, Default, Clone)]
pub struct BotClassPool {
    pub ready: bool,
    pub ids: Vec<ClassId>,
}

#[derive(Resource, Debug, Default)]
pub struct BotAddQueue(pub Vec<u32>);

impl BotAddQueue {
    pub fn push(&mut self, count: u32) {
        self.0.push(count.max(1));
    }

    pub fn drain(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct BotHold(pub bool);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BotTpTarget {
    All,
    Id(ClientId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BotTpWhere {
    Absolute {
        origin: [f32; 3],
        yaw: Option<f32>,
        pitch: Option<f32>,
    },

    Above {
        height: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotTpRequest {
    pub target: BotTpTarget,
    pub where_: BotTpWhere,
}

#[derive(Resource, Debug, Default)]
pub struct BotTpQueue(pub Vec<BotTpRequest>);

impl BotTpQueue {
    pub fn push(&mut self, request: BotTpRequest) {
        self.0.push(request);
    }

    pub fn drain(&mut self) -> Vec<BotTpRequest> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Resource, Debug, Default)]
pub struct BotFireQueue(pub Vec<BotTpTarget>);

impl BotFireQueue {
    pub fn push(&mut self, target: BotTpTarget) {
        self.0.push(target);
    }

    pub fn drain(&mut self) -> Vec<BotTpTarget> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Debug)]
pub struct BotSlot {
    pub id: ClientId,
    pub brain: DumbBrain,
    pub joined: bool,
    pub class_requested: bool,
}

#[derive(Resource, Debug)]
pub struct BotRoster {
    pub bots: Vec<BotSlot>,
    pub next_client: u32,
    pub seed: u64,
}

impl Default for BotRoster {
    fn default() -> Self {
        Self {
            bots: Vec::new(),

            next_client: 1,
            seed: 0xb075_0001,
        }
    }
}

impl BotRoster {
    pub fn is_bot(&self, id: ClientId) -> bool {
        self.bots.iter().any(|bot| bot.id == id)
    }

    pub fn add_bots(&mut self, count: u32) -> Vec<ClientId> {
        let mut added = Vec::with_capacity(count as usize);
        for i in 0..count {
            let id = ClientId(self.next_client);
            self.next_client = self.next_client.wrapping_add(1).max(1);
            let brain = DumbBrain::new(self.seed ^ (u64::from(id.0) << 32) ^ u64::from(i));
            self.bots.push(BotSlot {
                id,
                brain,
                joined: false,
                class_requested: false,
            });
            added.push(id);
        }
        added
    }
}
