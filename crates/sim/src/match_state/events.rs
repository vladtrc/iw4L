use crate::identities::{DamageSource, EventSequence, LifeSequence};
use crate::input::ClassId;
use crate::world::{ClientId, Tick};

use super::MatchEndReason;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventAudience {
    All,
    AllExcept(ClientId),
    Client(ClientId),

    Clients(Vec<ClientId>),
}

impl EventAudience {
    pub fn projects_to(&self, id: ClientId) -> bool {
        match self {
            Self::All => true,
            Self::AllExcept(excluded) => *excluded != id,
            Self::Client(c) => *c == id,
            Self::Clients(cs) => cs.contains(&id),
        }
    }

    pub fn pair(a: ClientId, b: ClientId) -> Self {
        if a == b {
            Self::Client(a)
        } else {
            Self::Clients(vec![a, b])
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub sequence: EventSequence,
    pub tick: Tick,
    pub audience: EventAudience,
    pub event: SimEvent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityEventPayload {
    pub number: i32,
    pub other_entity_num: i32,
    pub attacker_entity_num: i32,
    pub event_parm: i32,
    pub weapon: u32,

    pub correlation: u32,
    pub fire_cause: Option<crate::FireCause>,

    pub pellet: u16,
    pub segment: u16,
    pub hand: u8,
    pub origin: [f32; 3],
    pub origin2: [f32; 3],
    pub direction: [f32; 3],
    pub surf_type: u8,

    pub surface_flags: u32,
    pub simulation_flags: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityEventRecord {
    pub sequence: EventSequence,
    pub tick: Tick,
    pub audience: EventAudience,
    pub event: entity_iw4::EntityEventKind,
    pub payload: EntityEventPayload,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PelletFxRecord {
    pub attacker: i32,
    pub weapon: u32,

    pub correlation: u32,
    pub fire_cause: Option<crate::FireCause>,

    pub pellet: u16,
    pub segment: u16,

    pub hand: u8,

    pub start: [f32; 3],

    pub end: [f32; 3],

    pub normal: [f32; 3],
    pub surf_type: u8,

    pub surface_flags: u32,

    pub flesh_flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimEvent {
    Spawned {
        class_id: ClassId,
        spawn_index: u32,
        life_sequence: LifeSequence,
    },

    Died {
        victim: ClientId,
        life_sequence: LifeSequence,
        attacker: Option<ClientId>,
        attacker_life: Option<LifeSequence>,
        source: Option<DamageSource>,

        weapon: u32,

        killcam_entity_start_time: i32,
    },

    ScoreChanged {
        client: ClientId,
        score: i32,
        kills: i32,
        deaths: i32,
    },

    MatchEnded {
        reason: MatchEndReason,
    },

    AttackReleased,

    WeaponSwitchRequested {
        weapon: u32,
    },
}

pub const UNRELIABLE_SIM_EVENT_COUNT: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimEventRow {
    pub variant: &'static str,

    pub reliable: bool,

    pub control_fact: &'static str,
}

pub const SIM_EVENT_ROSTER: &[SimEventRow] = &[
    SimEventRow {
        variant: "Spawned",
        reliable: true,
        control_fact: "which authored spawn authority picked and the life sequence it opened; \
                       spawn selection is authority-only state",
    },
    SimEventRow {
        variant: "Died",
        reliable: true,
        control_fact: "the life sequence ending and its cause — attacker, source and killcam \
                       entity birthtime, none of which a client can derive from its own \
                       simulation. The obituary a player *sees* is EV_OBITUARY, not this",
    },
    SimEventRow {
        variant: "ScoreChanged",
        reliable: true,
        control_fact: "the scoreboard row after authority applied it; score is authority state \
                       with no predicted counterpart",
    },
    SimEventRow {
        variant: "MatchEnded",
        reliable: true,
        control_fact: "the phase flip and which limit caused it; the client holds neither the \
                       authoritative clock nor every player's score",
    },
    SimEventRow {
        variant: "AttackReleased",
        reliable: false,
        control_fact: "authority's view of the ATTACK button for remote proxies, whose usercmds \
                       this client never sees. For the local player it is redundant with the \
                       client's own input, which is why it is the one unreliable variant",
    },
    SimEventRow {
        variant: "WeaponSwitchRequested",
        reliable: true,
        control_fact: "authority asks the client to select a weapon; the client owns weapon \
                       selection, so a server-side switch must go through its usercmd to \
                       play the drop and raise",
    },
];

pub fn sim_event_is_reliable(event: &SimEvent) -> bool {
    let variant = match event {
        SimEvent::Spawned { .. } => "Spawned",
        SimEvent::Died { .. } => "Died",
        SimEvent::ScoreChanged { .. } => "ScoreChanged",
        SimEvent::MatchEnded { .. } => "MatchEnded",
        SimEvent::AttackReleased => "AttackReleased",
        SimEvent::WeaponSwitchRequested { .. } => "WeaponSwitchRequested",
    };
    SIM_EVENT_ROSTER
        .iter()
        .find(|row| row.variant == variant)
        .is_some_and(|row| row.reliable)
}
