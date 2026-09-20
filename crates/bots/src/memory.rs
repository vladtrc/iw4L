use crate::observation::{BotEvent, BotObservation, Contact, KnowledgeSource, Visibility};

const MEMORY_TTL_TICKS: u32 = 80;
/// How long a contact stays current across ticks where perception never got to
/// it. A completed negative observation ends it at once; this bound only covers
/// "not evaluated", and it never refreshes the remembered position.
const UNSENSED_HOLD_TICKS: u32 = 12;

#[derive(Clone, Debug, Default)]
pub struct Memory {
    contacts: Vec<Contact>,
}

impl Memory {
    /// Folds one observation in and reports the transitions it caused.
    pub fn ingest(&mut self, obs: &BotObservation) -> Vec<BotEvent> {
        let mut events = Vec::new();
        self.contacts
            .retain(|c| obs.tick.saturating_sub(c.seen_tick) <= MEMORY_TTL_TICKS);
        for row in &mut self.contacts {
            if row.source != KnowledgeSource::CurrentlySeen
                || obs.seen.iter().any(|s| s.id == row.id)
            {
                continue;
            }
            // A probe that was not performed is not a negative observation.
            if obs.visibility(row.id) == Visibility::Unknown
                && obs.tick.saturating_sub(row.seen_tick) <= UNSENSED_HOLD_TICKS
            {
                continue;
            }
            row.source = KnowledgeSource::LastSeen;
            events.push(BotEvent::LostSight { id: row.id });
        }
        for seen in &obs.seen {
            if let Some(row) = self.contacts.iter_mut().find(|c| c.id == seen.id) {
                if row.source != KnowledgeSource::CurrentlySeen {
                    events.push(BotEvent::Spotted { id: seen.id });
                }
                *row = *seen;
            } else {
                events.push(BotEvent::Spotted { id: seen.id });
                self.contacts.push(*seen);
            }
        }
        events
    }

    pub fn currently_seen(&self) -> impl Iterator<Item = &Contact> {
        self.contacts
            .iter()
            .filter(|c| c.source == KnowledgeSource::CurrentlySeen)
    }

    pub fn last_seen(&self) -> impl Iterator<Item = &Contact> {
        self.contacts
            .iter()
            .filter(|c| c.source == KnowledgeSource::LastSeen)
    }
}
