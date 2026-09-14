use crate::observation::{BotObservation, Contact, KnowledgeSource};

const MEMORY_TTL_TICKS: u32 = 80;

#[derive(Clone, Debug, Default)]
pub struct Memory {
    contacts: Vec<Contact>,
}

impl Memory {
    pub fn ingest(&mut self, obs: &BotObservation) {
        self.contacts
            .retain(|c| obs.tick.saturating_sub(c.seen_tick) <= MEMORY_TTL_TICKS);
        for row in &mut self.contacts {
            if row.source == KnowledgeSource::CurrentlySeen
                && !obs.seen.iter().any(|s| s.id == row.id)
            {
                row.source = KnowledgeSource::LastSeen;
            }
        }
        for seen in &obs.seen {
            if let Some(row) = self.contacts.iter_mut().find(|c| c.id == seen.id) {
                *row = *seen;
            } else {
                self.contacts.push(*seen);
            }
        }
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
