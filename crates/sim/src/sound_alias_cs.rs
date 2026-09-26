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

pub const CS_LOCALIZED_STRINGS_SLOTS: usize = 0x200;

pub const HUD_STRING_PLAIN: char = '\u{15}';

pub const HUD_PRINT_ARG_SEPARATOR: char = '\u{16}';

pub type HudStringCsOccupied = Vec<(u16, String)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HudStringCs {
    slots: Vec<String>,
}

impl HudStringCs {
    pub fn index(&mut self, text: &str) -> Option<i32> {
        if text.is_empty() {
            return Some(0);
        }
        if let Some(i) = self.slots.iter().position(|s| s == text) {
            return i32::try_from(i + 1).ok();
        }
        if self.slots.len() + 1 >= CS_LOCALIZED_STRINGS_SLOTS {
            return None;
        }
        self.slots.push(text.to_owned());
        i32::try_from(self.slots.len()).ok()
    }

    pub fn adopt_occupied(&mut self, occupied: &[(u16, String)]) {
        self.slots.clear();
        for (index, text) in occupied {
            let at = usize::from(*index);
            if at == 0 || at >= CS_LOCALIZED_STRINGS_SLOTS {
                continue;
            }
            if self.slots.len() < at {
                self.slots.resize(at, String::new());
            }
            self.slots[at - 1] = text.clone();
        }
    }

    pub fn occupied(&self) -> HudStringCsOccupied {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((u16::try_from(i + 1).ok()?, s.clone())))
            .collect()
    }
}

pub fn hud_string_in_occupied(occupied: &[(u16, String)], index: i32) -> Option<&str> {
    let index = u16::try_from(index).ok()?;
    occupied
        .iter()
        .find(|(i, _)| *i == index)
        .map(|(_, text)| text.as_str())
}
