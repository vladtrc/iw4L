use std::collections::BTreeMap;
use std::collections::HashMap;

use bevy::prelude::Resource;
use sim::{ActionRequestId, ClientAction, ClientId, action_request_id};

use crate::transport::reliable::ActionVerdict;

pub const MAX_REMEMBERED_ACTIONS_PER_CLIENT: usize = 256;

#[derive(Resource, Debug)]
pub struct ActionRequestIds {
    next: ActionRequestId,
}

impl Default for ActionRequestIds {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl ActionRequestIds {
    pub fn allocate(&mut self) -> ActionRequestId {
        let id = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        id
    }

    pub fn reset(&mut self) {
        self.next = 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionAdmission {
    Fresh,

    Pending,

    Repeat(ActionVerdict),

    PayloadMismatch,

    Expired,
}

#[derive(Clone, Debug, Default)]
struct ClientActionMemory {
    admitted: BTreeMap<ActionRequestId, ClientAction>,
    resolved: BTreeMap<ActionRequestId, (ClientAction, ActionVerdict)>,

    forgotten_through: Option<ActionRequestId>,
}

#[derive(Resource, Debug, Default)]
pub struct ClientActionLedger {
    scope: u64,
    clients: HashMap<ClientId, ClientActionMemory>,
}

impl ClientActionLedger {
    pub fn scope(&self) -> u64 {
        self.scope
    }

    pub fn open_scope(&mut self) {
        self.scope = self.scope.wrapping_add(1);
        self.clients.clear();
    }

    pub fn retire_client(&mut self, client: ClientId) {
        self.clients.remove(&client);
    }

    pub fn remembered(&self, client: ClientId) -> usize {
        self.clients
            .get(&client)
            .map(|memory| memory.resolved.len())
            .unwrap_or(0)
    }

    pub fn verdict(&self, client: ClientId, request_id: ActionRequestId) -> Option<ActionVerdict> {
        self.clients
            .get(&client)?
            .resolved
            .get(&request_id)
            .map(|(_, verdict)| *verdict)
    }

    pub fn admit(&mut self, client: ClientId, action: &ClientAction) -> ActionAdmission {
        let request_id = action_request_id(action);
        let memory = self.clients.entry(client).or_default();
        if let Some(payload) = memory.admitted.get(&request_id) {
            return if payload == action {
                ActionAdmission::Pending
            } else {
                ActionAdmission::PayloadMismatch
            };
        }
        if let Some((payload, verdict)) = memory.resolved.get(&request_id) {
            return if payload == action {
                ActionAdmission::Repeat(*verdict)
            } else {
                ActionAdmission::PayloadMismatch
            };
        }
        if memory
            .forgotten_through
            .is_some_and(|watermark| request_id <= watermark)
        {
            return ActionAdmission::Expired;
        }
        memory.admitted.insert(request_id, *action);
        ActionAdmission::Fresh
    }

    pub fn record(&mut self, client: ClientId, action: ClientAction, verdict: ActionVerdict) {
        let request_id = action_request_id(&action);
        let memory = self.clients.entry(client).or_default();
        memory.admitted.remove(&request_id);
        memory.resolved.insert(request_id, (action, verdict));
        while memory.resolved.len() > MAX_REMEMBERED_ACTIONS_PER_CLIENT {
            let Some(oldest) = memory.resolved.keys().next().copied() else {
                break;
            };
            memory.resolved.remove(&oldest);
            memory.forgotten_through = Some(match memory.forgotten_through {
                Some(current) => current.max(oldest),
                None => oldest,
            });
        }
    }
}
