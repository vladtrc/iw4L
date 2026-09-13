pub const CS_SOUNDALIASES_SLOTS: usize = 0x100;

pub type SoundAliasCsOccupied = Vec<(u8, String)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundAliasCs {
    slots: Vec<Option<String>>,
}

impl Default for SoundAliasCs {
    fn default() -> Self {
        Self {
            slots: vec![None; CS_SOUNDALIASES_SLOTS],
        }
    }
}

impl SoundAliasCs {
    pub fn index(&mut self, name: &str) -> u8 {
        if name.is_empty() {
            return 0;
        }
        let mut first_empty = None;
        for i in 1..CS_SOUNDALIASES_SLOTS {
            match self.slots[i].as_deref() {
                Some(existing) if existing == name => return i as u8,
                None if first_empty.is_none() => first_empty = Some(i),
                _ => {}
            }
        }
        let Some(slot) = first_empty else {
            return 0;
        };
        self.slots[slot] = Some(name.to_owned());
        slot as u8
    }

    pub fn name(&self, index: u8) -> Option<&str> {
        self.slots.get(usize::from(index))?.as_deref()
    }

    pub fn occupied(&self) -> SoundAliasCsOccupied {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                let name = slot.as_ref()?;
                let index = u8::try_from(i).ok()?;
                (index != 0).then(|| (index, name.clone()))
            })
            .collect()
    }

    pub fn adopt_occupied(&mut self, occupied: &[(u8, String)]) {
        self.slots = vec![None; CS_SOUNDALIASES_SLOTS];
        for (index, name) in occupied {
            if *index == 0 {
                continue;
            }
            self.slots[usize::from(*index)] = Some(name.clone());
        }
    }
}

pub fn name_in_occupied(occupied: &[(u8, String)], index: u8) -> Option<&str> {
    occupied
        .iter()
        .find(|(i, _)| *i == index)
        .map(|(_, name)| name.as_str())
}

pub type HudMaterialCs = SoundAliasCs;
pub type HudMaterialCsOccupied = SoundAliasCsOccupied;

pub const REQUIRED_HUD_MATERIALS: &[&str] = &["damage_feedback"];

pub type EffectNameCs = SoundAliasCs;
pub type EffectNameCsOccupied = SoundAliasCsOccupied;
