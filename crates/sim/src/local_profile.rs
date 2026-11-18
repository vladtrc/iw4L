use bevy_ecs::prelude::Resource;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalPlayerProfile {
    pub take_cover_warnings: i32,
    pub intel_found: u64,
    pub has_ever_played_sp: bool,
    pub auto_aim: bool,
    pub save_generation: u64,
    pub percent_complete_sp: u8,
    pub percent_complete_mp: u8,
    pub percent_complete_so: u8,
}

impl Default for LocalPlayerProfile {
    fn default() -> Self {
        Self {
            take_cover_warnings: -1,
            intel_found: 0,
            has_ever_played_sp: false,
            auto_aim: true,
            save_generation: 0,
            percent_complete_sp: 0,
            percent_complete_mp: 0,
            percent_complete_so: 0,
        }
    }
}

impl LocalPlayerProfile {
    pub fn request_save(&mut self) {
        self.save_generation = self.save_generation.wrapping_add(1);
    }

    pub fn intel_is_found(&self, number: i32) -> bool {
        (1..=64).contains(&number) && self.intel_found & (1u64 << (number - 1)) != 0
    }

    pub fn set_intel_found(&mut self, number: i32) {
        if (1..=64).contains(&number) {
            self.intel_found |= 1u64 << (number - 1);
        }
    }

    pub fn get_int(&self, name: &str) -> Option<i32> {
        if name.eq_ignore_ascii_case("takeCoverWarnings") {
            Some(self.take_cover_warnings)
        } else if name.eq_ignore_ascii_case("hasEverPlayed_SP") {
            Some(i32::from(self.has_ever_played_sp))
        } else if name.eq_ignore_ascii_case("autoAim") {
            Some(i32::from(self.auto_aim))
        } else {
            self.get(name).map(i32::from)
        }
    }

    pub fn set_int(&mut self, name: &str, value: i32) -> bool {
        if name.eq_ignore_ascii_case("takeCoverWarnings") {
            self.take_cover_warnings = value;
            true
        } else if name.eq_ignore_ascii_case("hasEverPlayed_SP") {
            self.has_ever_played_sp = value != 0;
            true
        } else if name.eq_ignore_ascii_case("autoAim") {
            self.auto_aim = value != 0;
            true
        } else {
            self.set(name, value as u8)
        }
    }

    pub const ALL_FIELDS: [&str; 6] = [
        "percentcompletesp",
        "percentcompletemp",
        "percentcompleteso",
        "takecoverwarnings",
        "haseverplayed_sp",
        "autoaim",
    ];

    pub const FIELDS: [&str; 3] = [
        "percentcompletesp",
        "percentcompletemp",
        "percentcompleteso",
    ];

    pub fn get(&self, name: &str) -> Option<u8> {
        if name.eq_ignore_ascii_case(Self::FIELDS[0]) {
            Some(self.percent_complete_sp)
        } else if name.eq_ignore_ascii_case(Self::FIELDS[1]) {
            Some(self.percent_complete_mp)
        } else if name.eq_ignore_ascii_case(Self::FIELDS[2]) {
            Some(self.percent_complete_so)
        } else {
            None
        }
    }

    pub fn set(&mut self, name: &str, value: u8) -> bool {
        let slot = if name.eq_ignore_ascii_case(Self::FIELDS[0]) {
            &mut self.percent_complete_sp
        } else if name.eq_ignore_ascii_case(Self::FIELDS[1]) {
            &mut self.percent_complete_mp
        } else if name.eq_ignore_ascii_case(Self::FIELDS[2]) {
            &mut self.percent_complete_so
        } else {
            return false;
        };
        *slot = value;
        true
    }
}
