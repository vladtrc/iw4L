use core::fmt;

use bevy::prelude::*;
use diag::gap::{self as ledger, Gap as _, GapLedger};
use sim::ClientId;

use crate::client::entities::CLIENT_ENTITY_SLOT_COUNT;

const NET_GAP_COUNT: usize = <NetGap as ledger::Gap>::ALL.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetGap {
    CEntityNumber,

    EntityStateVector,

    EntityEventTarget,

    ScriptNotifyTarget,
}

impl ledger::Gap for NetGap {
    const ALL: &'static [NetGap] = &[
        NetGap::CEntityNumber,
        NetGap::EntityStateVector,
        NetGap::EntityEventTarget,
        NetGap::ScriptNotifyTarget,
    ];

    fn name(self) -> &'static str {
        match self {
            NetGap::CEntityNumber => "centity-number",
            NetGap::EntityStateVector => "entity-state-vector",
            NetGap::EntityEventTarget => "entity-event-target",
            NetGap::ScriptNotifyTarget => "script-notify-target",
        }
    }

    fn is_standing(self) -> bool {
        matches!(self, NetGap::EntityStateVector)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptNotify {
    BeginKillcam,
    SpawnedPlayer,
    AbortKillcam,
    EndedKillcam,
}

impl fmt::Display for ScriptNotify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScriptNotify::BeginKillcam => "BeginKillcam",
            ScriptNotify::SpawnedPlayer => "SpawnedPlayer",
            ScriptNotify::AbortKillcam => "AbortKillcam",
            ScriptNotify::EndedKillcam => "EndedKillcam",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetGapCause {
    ClientOutOfNumberSpace {
        client: ClientId,
    },

    NumberClaimedTwice {
        number: u16,
    },

    EventNumberHasNoEntity {
        number: i32,
    },

    EventNumberOutOfRange {
        number: i32,
    },

    NotifyClientHasNoEntity {
        client: ClientId,
        notify: ScriptNotify,
    },
}

impl ledger::GapCause for NetGapCause {
    type Gap = NetGap;

    fn gap(&self) -> NetGap {
        match self {
            NetGapCause::ClientOutOfNumberSpace { .. } | NetGapCause::NumberClaimedTwice { .. } => {
                NetGap::CEntityNumber
            }
            NetGapCause::EventNumberHasNoEntity { .. }
            | NetGapCause::EventNumberOutOfRange { .. } => NetGap::EntityEventTarget,
            NetGapCause::NotifyClientHasNoEntity { .. } => NetGap::ScriptNotifyTarget,
        }
    }
}

impl fmt::Display for NetGapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetGapCause::ClientOutOfNumberSpace { client } => write!(
                f,
                "client {} is outside the {CLIENT_ENTITY_SLOT_COUNT}-slot entity-number space",
                client.0
            ),
            NetGapCause::NumberClaimedTwice { number } => {
                write!(
                    f,
                    "entity number {number} was claimed twice in one snapshot"
                )
            }
            NetGapCause::EventNumberHasNoEntity { number } => {
                write!(f, "entity number {number} has no live entity")
            }
            NetGapCause::EventNumberOutOfRange { number } => {
                write!(f, "entity number {number} is not a u16 entity number")
            }
            NetGapCause::NotifyClientHasNoEntity { client, notify } => {
                write!(f, "{notify} for client {} has no player entity", client.0)
            }
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct NetIdentityGaps {
    ledger: GapLedger<NetGapCause, NET_GAP_COUNT>,

    reported: String,
}

impl NetIdentityGaps {
    pub fn raise(&mut self, cause: NetGapCause) {
        self.ledger.raise(cause);
    }

    pub fn is_live(&self, gap: NetGap) -> bool {
        self.ledger.is_live(gap)
    }

    pub fn cause(&self, gap: NetGap) -> Option<&NetGapCause> {
        self.ledger.cause(gap)
    }

    pub fn hits(&self, gap: NetGap) -> u64 {
        self.ledger.hits(gap)
    }

    pub fn live(&self) -> impl Iterator<Item = NetGap> + '_ {
        self.ledger.live()
    }

    pub fn count(&self) -> usize {
        self.ledger.count()
    }

    pub(crate) fn report(&mut self) {
        let mut signature = String::new();
        self.ledger
            .write_signature(&mut signature)
            .expect("writing into a String cannot fail");
        if signature == self.reported {
            return;
        }
        self.reported = signature;
        for gap in self.ledger.live() {
            if let Some(cause) = self.ledger.cause(gap) {
                diag::warn!(
                    Net,
                    "net gap {} (x{}): {} — rejected instead of aliasing an entity slot",
                    gap.name(),
                    self.ledger.hits(gap),
                    cause
                );
            }
        }
    }
}
