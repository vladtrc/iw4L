use std::collections::HashMap;
use std::sync::Mutex;

use crate::CueFailure;

const EVENT_CAPACITY: usize = 8192;
const EVENT_WINDOW_TICKS: u32 = 100;
const FUTURE_TICKS: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AudioEventId {
    pub emitter: u64,
    pub occurrence: AudioOccurrence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioOccurrence {
    Entity {
        domain: net::EntityEventDomain,
        sequence: u32,
        tick: u32,
        ordinal: u16,
    },
    Animation(AnimationMarkerId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnimationMarkerId {
    pub controller: u64,
    pub playback: u64,
    pub node: usize,
    pub cycle: i64,
    pub marker: usize,
    pub hand: u8,
    pub life: u32,
    pub weapon: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AudioEvent {
    pub id: AudioEventId,
    pub tick: u32,
    pub fire_cause: Option<(sim::FireCause, u16)>,
}

impl AudioEvent {
    pub fn from_entity(
        entity: bevy::prelude::Entity,
        event: &net::DispatchedEntityEvent,
        ordinal: u16,
    ) -> Self {
        Self {
            id: AudioEventId {
                emitter: entity.to_bits(),
                occurrence: AudioOccurrence::Entity {
                    domain: event.domain,
                    sequence: event.sequence.0,
                    tick: event.tick.0,
                    ordinal,
                },
            },
            tick: event.tick.0,
            fire_cause: (entity_iw4::entity_event_action(event.event)
                == Ok(entity_iw4::EntityEventAction::WeaponFire))
            .then_some(event.payload.fire_cause)
            .flatten()
            .map(|cause| (cause, ordinal)),
        }
    }

    pub fn from_animation(
        client: sim::ClientId,
        tick: sim::Tick,
        marker: AnimationMarkerId,
    ) -> Self {
        Self {
            id: AudioEventId {
                emitter: u64::from(client.0),
                occurrence: AudioOccurrence::Animation(marker),
            },
            tick: tick.0,
            fire_cause: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EventContext {
    pub local_life: Option<(u32, u32)>,
    pub tick: u32,
    pub killcam: bool,
    pub owner_tick: Option<u32>,
}

impl EventContext {
    fn tick_for(self, id: AudioEventId) -> u32 {
        match id.occurrence {
            AudioOccurrence::Entity {
                domain: net::EntityEventDomain::Predicted,
                ..
            } => self.owner_tick.unwrap_or(self.tick),
            _ => self.tick,
        }
    }
}

#[derive(Default)]
pub(crate) struct EventContextState(Mutex<Option<EventContext>>);

impl EventContextState {
    pub(crate) fn set(&self, context: Option<EventContext>) {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner()) = context;
    }

    pub(crate) fn get(&self) -> Option<EventContext> {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

pub(crate) struct EventJournal {
    available: bool,
    context: Option<EventContext>,
    accepted: HashMap<AudioEventId, u32>,
}

impl EventJournal {
    pub(crate) fn new() -> Self {
        Self {
            available: false,
            context: None,
            accepted: HashMap::with_capacity(EVENT_CAPACITY),
        }
    }

    pub(crate) fn advance(&mut self, context: Option<EventContext>) {
        self.available = context.is_some();
        let Some(mut new) = context else {
            return;
        };
        let Some(old) = self.context else {
            self.context = Some(new);
            return;
        };
        let rewound = new.tick.wrapping_sub(old.tick) >= 1 << 31;
        if new.killcam != old.killcam || (new.killcam && rewound) {
            // Archive playback is another occurrence of the same events.
            // Its clock must rewind, and live-event deduplication must not
            // silence the replay's shot, impacts or explosion.
            self.accepted.clear();
            self.context = Some(new);
            return;
        }
        if rewound {
            new.tick = old.tick;
        }
        self.context = Some(new);
        if new.tick == old.tick && new.owner_tick == old.owner_tick {
            return;
        }
        self.accepted.retain(|id, tick| {
            let age = new.tick_for(*id).wrapping_sub(*tick);
            age < EVENT_WINDOW_TICKS || age >= 1 << 31
        });
    }

    pub(crate) fn current(&self, event: Option<AudioEvent>) -> bool {
        event.is_none_or(|event| {
            self.available
                && self.context.is_some_and(|context| {
                    event.fire_cause.is_none_or(|(cause, _)| {
                        context.local_life.is_none_or(|(client, life)| {
                            cause.client.0 != client || cause.life.0 == life
                        })
                    }) && match event.id.occurrence {
                        AudioOccurrence::Entity { .. } => true,
                        AudioOccurrence::Animation(marker) => {
                            context.local_life.is_some_and(|(client, life)| {
                                u64::from(client) == event.id.emitter && life == marker.life
                            })
                        }
                    }
                })
        })
    }

    pub(crate) fn accept(&mut self, event: Option<AudioEvent>) -> Result<(), CueFailure> {
        let Some(event) = event else {
            return Ok(());
        };
        if !self.available {
            return Err(CueFailure::StaleEvent);
        }
        let Some(context) = self.context else {
            return Err(CueFailure::StaleEvent);
        };
        let id = event.id;
        if !self.current(Some(event)) {
            return Err(CueFailure::StaleEvent);
        }
        let context_tick = context.tick_for(id);
        let age = context_tick.wrapping_sub(event.tick);
        if age >= EVENT_WINDOW_TICKS && age < 1 << 31 {
            return Err(CueFailure::StaleEvent);
        }
        if age >= 1 << 31 && event.tick.wrapping_sub(context_tick) > FUTURE_TICKS {
            return Err(CueFailure::StaleEvent);
        }
        if self.accepted.contains_key(&id) {
            return Err(CueFailure::DuplicateEvent);
        }
        if self.accepted.len() == EVENT_CAPACITY {
            return Err(CueFailure::EventBudget);
        }
        self.accepted.insert(id, event.tick);
        Ok(())
    }
}
