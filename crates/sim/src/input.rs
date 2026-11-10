use playerstate_iw4::UserCmd;

use crate::identities::MatchPhase;
use crate::world::ClientId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientAction {
    JoinMatch,

    InputCommand {
        command: u32,
    },

    SoundDone {
        entity: u16,
        alias_index: u16,
    },

    LeaveMatch,

    SelectClass {
        class_id: ClassId,
        loadout: crate::PersonalClass,
    },

    GiveWeapon {
        weapon: u32,
        model: u8,
    },

    ChangeWeaponCamo {
        weapon: u32,
        model: u8,
    },

    ChangeWeaponConfiguration {
        from: u32,
        to: u32,
    },

    ForceDeath,

    SpawnClient,

    ForceSpawn {
        pick: SpawnPick,
    },

    SpawnIntermission,

    SetMatchPhase {
        phase: MatchPhase,
    },

    Move {
        origin: [f32; 3],
        angles: [f32; 3],
    },

    BeginScriptMoverRotateVelocity {
        speed: f32,
    },

    ToggleGod,

    ToggleProjectileFlight,

    EndGracePeriod,

    DebugDamage {
        amount: i32,
    },

    SetProfile {
        profile: PlayerProfile,
    },

    SetName {
        name: [u8; 16],
    },

    Chat {
        team_only: bool,
        text: [u8; crate::chat::CHAT_BYTES],
    },

    UseCopycat,

    ActionSlot {
        slot: u8,
    },

    ChooseDefaultClass {
        index: u8,
    },

    MenuResponse {
        menu: [u8; MENU_RESPONSE_BYTES],
        response: [u8; MENU_RESPONSE_BYTES],
    },

    ResupplyAmmo,

    GiveKillstreak {
        name: [u8; MENU_RESPONSE_BYTES],
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerProfile {
    pub title: u32,
    pub emblem: u32,
    pub killstreaks: [u32; 3],
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

pub use playerstate_iw4::CommandSequence;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCommand {
    pub client: ClientId,
    pub command: UserCmd,
    pub sequence: Option<CommandSequence>,
}

impl PlayerCommand {
    pub fn unsequenced(client: ClientId, command: UserCmd) -> Self {
        Self {
            client,
            command,
            sequence: None,
        }
    }

    pub fn sequenced(client: ClientId, command: UserCmd, sequence: CommandSequence) -> Self {
        Self {
            client,
            command,
            sequence: Some(sequence),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickInput {
    pub cmds: Vec<PlayerCommand>,
    pub actions: Vec<(ClientId, ClientAction)>,
    pub shot_samples: Vec<((ClientId, i32), crate::ShotSampleProvenance)>,
}

impl TickInput {
    pub fn from_cmds(cmds: Vec<(ClientId, UserCmd)>) -> Self {
        Self {
            cmds: cmds
                .into_iter()
                .map(|(client, command)| PlayerCommand::unsequenced(client, command))
                .collect(),
            actions: Vec::new(),
            shot_samples: Vec::new(),
        }
    }

    pub fn canonicalize(&mut self) {
        self.cmds.sort_by_key(|input| input.client.0);
        self.shot_samples
            .sort_by_key(|((id, time), _)| (id.0, *time));

        self.actions.sort_by_key(|(id, _)| id.0);
    }
}
