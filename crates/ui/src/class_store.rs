use bevy::prelude::*;

use crate::class_presets::default_presets;
use crate::class_setup::{ClassSetupScratch, ClassSlotState};
use frame::{HostClassLoadouts, HostClassSlot};

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SessionClassStore {
    pub slots: Vec<ClassSlotState>,

    pub equipped: Option<usize>,
    pub selected: usize,
}

impl Default for SessionClassStore {
    fn default() -> Self {
        Self::from_presets()
    }
}

impl SessionClassStore {
    pub fn from_presets() -> Self {
        Self {
            slots: default_presets()
                .iter()
                .map(ClassSlotState::from_preset)
                .collect(),
            equipped: None,
            selected: 0,
        }
    }

    pub fn commit_slots(&mut self, scratch: &ClassSetupScratch) {
        self.slots = scratch.slots.clone();
        self.selected = scratch.selected;
    }

    pub fn commit_equip(&mut self, selected: usize) {
        if selected < self.slots.len() {
            self.selected = selected;
            self.equipped = Some(selected);
        }
    }

    pub fn equipped_slot(&self) -> Option<&ClassSlotState> {
        self.equipped.and_then(|i| self.slots.get(i))
    }

    pub fn apply_coverage_locks(&mut self, reasons: impl IntoIterator<Item = Option<String>>) {
        for (slot, reason) in self.slots.iter_mut().zip(reasons) {
            slot.lock_reason = reason;
        }
    }
}

impl From<&ClassSlotState> for HostClassSlot {
    fn from(slot: &ClassSlotState) -> Self {
        Self {
            name: slot.name.clone(),
            primary: slot.primary.clone(),
            primary_attachments: slot.primary_attachments.clone(),
            secondary: slot.secondary.clone(),
            secondary_attachments: slot.secondary_attachments.clone(),
            lethal: slot.lethal.clone(),
            tactical: slot.tactical.clone(),
            perks: [slot.perk1.clone(), slot.perk2.clone(), slot.perk3.clone()],
            deathstreak: slot.deathstreak.clone(),
        }
    }
}

pub(crate) fn sync_host_class_loadouts(
    scratch: Res<ClassSetupScratch>,
    mut store: ResMut<SessionClassStore>,
    mut host: ResMut<HostClassLoadouts>,
) {
    if !scratch.is_changed() {
        return;
    }
    store.commit_slots(&scratch);
    host.slots = scratch.slots.iter().map(HostClassSlot::from).collect();
}
