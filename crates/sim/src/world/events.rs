use super::{ClientId, Tick};
use crate::identities::EventSequence;
use crate::match_state::{
    EntityEventPayload, EntityEventRecord, EventAudience, EventRecord, SimEvent,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingPlayerCardKind {
    SetSlot {
        source: ClientId,
        slot: i32,
    },
    OpenMenu {
        cs_index: i32,
    },
    Splash {
        key: String,
        slot: i32,
        optional: i32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPlayerCardEvent {
    pub recipient: ClientId,
    pub kind: PendingPlayerCardKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPrint {
    pub recipient: Option<ClientId>,
    pub bold: bool,
    pub chat: bool,
    pub template: String,
    pub arg: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingLocalSound {
    pub recipient: ClientId,
    pub stop: bool,
    pub alias_index: u8,
}

#[derive(Clone, Debug)]
pub(super) struct EventJournal {
    journal: Vec<EventRecord>,
    next_event: EventSequence,
    entity_events: Vec<EntityEventRecord>,
    next_entity_event: EventSequence,
    pellet_fx: Vec<crate::PelletFxRecord>,
    last_pellet_fx: Vec<crate::PelletFxRecord>,
}

impl Default for EventJournal {
    fn default() -> Self {
        Self {
            journal: Vec::new(),
            next_event: EventSequence(1),
            entity_events: Vec::new(),
            next_entity_event: EventSequence(1),
            pellet_fx: Vec::new(),
            last_pellet_fx: Vec::new(),
        }
    }
}
impl EventJournal {
    pub(super) fn journal(&self) -> &[EventRecord] {
        &self.journal
    }
    pub(super) fn clear_tick(&mut self, tick: Tick) {
        self.journal.clear();
        self.entity_events.retain(|record| {
            tick.0
                .saturating_sub(record.tick.0)
                .saturating_mul(crate::MATCH_TICK_MS)
                <= crate::gentity::GENTITY_TEMP_EVENT_LIFETIME_MS as u32
        });
        self.last_pellet_fx = std::mem::take(&mut self.pellet_fx);
    }
    pub(super) fn push(
        &mut self,
        tick: Tick,
        audience: EventAudience,
        event: SimEvent,
        publishes_snapshot: bool,
    ) {
        let sequence = self.next_event;
        self.next_event = sequence.next();
        if publishes_snapshot
            && let SimEvent::Died {
                victim, attacker, ..
            } = &event
        {
            let suicide = match attacker {
                None => 1,
                Some(a) if a == victim => 1,
                Some(_) => 0,
            };
            perf::death(victim.0, attacker.map(|a| a.0), suicide, tick.0);
        }
        self.journal.push(EventRecord {
            sequence,
            tick,
            audience,
            event,
        });
    }

    pub(super) fn push_entity(
        &mut self,
        tick: Tick,
        audience: EventAudience,
        event: entity_iw4::EntityEventKind,
        payload: EntityEventPayload,
    ) {
        let sequence = self.next_entity_event;
        self.next_entity_event = sequence.next();
        self.entity_events.push(EntityEventRecord {
            sequence,
            tick,
            audience,
            event,
            payload,
        });
    }

    pub(super) fn entity_events(&self) -> &[EntityEventRecord] {
        &self.entity_events
    }

    pub(super) fn next_entity_event_sequence(&self) -> EventSequence {
        self.next_entity_event
    }

    pub(super) fn pellet_fx(&self) -> &[crate::PelletFxRecord] {
        &self.pellet_fx
    }

    pub(super) fn last_pellet_fx(&self) -> &[crate::PelletFxRecord] {
        &self.last_pellet_fx
    }

    pub(super) fn push_pellet_fx(&mut self, record: crate::PelletFxRecord) {
        self.pellet_fx.push(record);
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PresentationQueue {
    pub(super) player_cards: Vec<PendingPlayerCardEvent>,
    pub(super) prints: Vec<PendingPrint>,
    pub(super) local_sounds: Vec<PendingLocalSound>,
    pub(super) script_audio: Vec<crate::ScriptAudioCommand>,
}
impl PresentationQueue {
    pub(super) fn reset(&mut self) {
        self.player_cards.clear();
        self.prints.clear();
        self.local_sounds.clear();
        self.script_audio.clear();
    }
}
