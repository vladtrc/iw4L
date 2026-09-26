use playerstate_iw4::UserCmd;

use crate::identities::MatchPhase;
use crate::world::ClientId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

pub type ActionRequestId = u32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientAction {
    JoinMatch {
        request_id: ActionRequestId,
    },

    LeaveMatch {
        request_id: ActionRequestId,
    },

    SelectClass {
        request_id: ActionRequestId,
        class_id: ClassId,
        revision: u32,
    },

    GiveWeapon {
        request_id: ActionRequestId,
        weapon: u32,
    },

    ChangeWeaponConfiguration {
        request_id: ActionRequestId,
        from: u32,
        to: u32,
    },

    ForceDeath {
        request_id: ActionRequestId,
    },

    SpawnClient {
        request_id: ActionRequestId,
    },

    ForceSpawn {
        request_id: ActionRequestId,
        pick: SpawnPick,
    },

    SpawnIntermission {
        request_id: ActionRequestId,
    },

    SetMatchPhase {
        request_id: ActionRequestId,
        phase: MatchPhase,
    },

    Move {
        request_id: ActionRequestId,
        origin: [f32; 3],
        angles: [f32; 3],
    },

    BeginScriptMoverRotateVelocity {
        request_id: ActionRequestId,
        speed: f32,
    },

    DebugDamage {
        request_id: ActionRequestId,
        amount: i32,
    },

    SetName {
        request_id: ActionRequestId,
        name: [u8; 16],
    },

    UseCopycat {
        request_id: ActionRequestId,
    },

    ChooseDefaultClass {
        request_id: ActionRequestId,
        index: u8,
    },

    MenuResponse {
        request_id: ActionRequestId,
        menu: [u8; MENU_RESPONSE_BYTES],
        response: [u8; MENU_RESPONSE_BYTES],
    },
}

pub const MENU_RESPONSE_BYTES: usize = 48;

pub fn menu_response_field(text: &str) -> Option<[u8; MENU_RESPONSE_BYTES]> {
    let bytes = text.as_bytes();
    if bytes.len() > MENU_RESPONSE_BYTES {
        return None;
    }
    let mut field = [0u8; MENU_RESPONSE_BYTES];
    field[..bytes.len()].copy_from_slice(bytes);
    Some(field)
}

pub fn menu_response_text(field: &[u8; MENU_RESPONSE_BYTES]) -> &str {
    let len = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..len]).unwrap_or("")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnPick {
    Seeded(u64),
    At { origin: [f32; 3], yaw: f32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickInput {
    pub cmds: Vec<(ClientId, UserCmd)>,
    pub actions: Vec<(ClientId, ClientAction)>,
}

impl TickInput {
    pub fn from_cmds(cmds: Vec<(ClientId, UserCmd)>) -> Self {
        Self {
            cmds,
            actions: Vec::new(),
        }
    }

    pub fn canonicalize(&mut self) {
        self.cmds.sort_by_key(|(id, _)| id.0);

        self.actions
            .sort_by_key(|(id, action)| (id.0, action_request_id(action)));
    }
}

pub fn action_request_id(action: &ClientAction) -> ActionRequestId {
    match *action {
        ClientAction::JoinMatch { request_id }
        | ClientAction::LeaveMatch { request_id }
        | ClientAction::SelectClass { request_id, .. }
        | ClientAction::GiveWeapon { request_id, .. }
        | ClientAction::ChangeWeaponConfiguration { request_id, .. }
        | ClientAction::ForceDeath { request_id }
        | ClientAction::SpawnClient { request_id }
        | ClientAction::ForceSpawn { request_id, .. }
        | ClientAction::SpawnIntermission { request_id }
        | ClientAction::SetMatchPhase { request_id, .. }
        | ClientAction::Move { request_id, .. }
        | ClientAction::BeginScriptMoverRotateVelocity { request_id, .. }
        | ClientAction::DebugDamage { request_id, .. }
        | ClientAction::SetName { request_id, .. }
        | ClientAction::UseCopycat { request_id }
        | ClientAction::ChooseDefaultClass { request_id, .. }
        | ClientAction::MenuResponse { request_id, .. } => request_id,
    }
}
