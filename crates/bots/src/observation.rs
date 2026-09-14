use sim::{ClientId, ClientLifecycle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Stand,
    Crouch,
    Prone,
}

impl Stance {
    pub fn from_view_height(height: f32) -> Self {
        if height >= 50.0 {
            Self::Stand
        } else if height >= 30.0 {
            Self::Crouch
        } else {
            Self::Prone
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeSource {
    CurrentlySeen,
    LastSeen,
    PublicMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub id: ClientId,
    pub origin: [f32; 3],
    pub source: KnowledgeSource,
    pub seen_tick: u32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeaponClass {
    #[default]
    Assault,
    Smg,
    Lmg,
    Sniper,
    Shotgun,
}

impl WeaponClass {
    pub fn fight_hold(self) -> f32 {
        match self {
            Self::Sniper => 360.0,
            Self::Lmg => 220.0,
            Self::Assault => 160.0,
            Self::Smg => 112.0,
            Self::Shotgun => 72.0,
        }
    }

    pub fn from_script_name(name: &str) -> Self {
        if name.is_empty() {
            return Self::Assault;
        }
        let stem = crate::unique_loadout::family_stem(name);
        for row in crate::unique_loadout::UNIQUE_WEAPONS {
            if crate::unique_loadout::family_stem(row.key) == stem {
                return Self::from_group(row.group);
            }
        }
        Self::Assault
    }

    fn from_group(group: crate::unique_loadout::UniqueGroup) -> Self {
        use crate::unique_loadout::UniqueGroup;
        match group {
            UniqueGroup::Sniper => Self::Sniper,
            UniqueGroup::Lmg => Self::Lmg,
            UniqueGroup::Shotgun => Self::Shotgun,
            UniqueGroup::Smg | UniqueGroup::MachinePistol | UniqueGroup::Pistol => Self::Smg,
            UniqueGroup::Assault
            | UniqueGroup::Riot
            | UniqueGroup::Projectile
            | UniqueGroup::Cqb
            | UniqueGroup::Special => Self::Assault,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfState {
    pub id: ClientId,
    pub lifecycle: ClientLifecycle,
    pub origin: [f32; 3],
    pub viewangles: [f32; 3],
    pub view_height: f32,
    pub stance: Stance,
    pub health: i32,
    pub weapon: u16,
    pub weapon_class: WeaponClass,
    pub ammo_clip: i32,
    pub ammo_stock: i32,
    pub team: i32,
}

pub const DEFAULT_OBJECTIVE_RADIUS: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeObjective {
    pub origin: [f32; 3],
    pub touching: bool,
    pub use_button: bool,
    pub radius: f32,
}

impl ModeObjective {
    pub fn at(origin: [f32; 3]) -> Self {
        Self {
            origin,
            touching: false,
            use_button: false,
            radius: DEFAULT_OBJECTIVE_RADIUS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotEvent {
    Spotted { id: ClientId },
    LostSight { id: ClientId },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BotObservation {
    pub tick: u32,
    pub time_ms: i32,
    pub self_state: SelfState,
    pub seen: Vec<Contact>,
    pub events: Vec<BotEvent>,
    pub objective: Option<ModeObjective>,
    pub objectives: Vec<ModeObjective>,
}

impl BotObservation {
    pub fn eye(&self) -> [f32; 3] {
        let z = if self.self_state.view_height > 1.0 {
            self.self_state.view_height
        } else {
            60.0
        };
        [
            self.self_state.origin[0],
            self.self_state.origin[1],
            self.self_state.origin[2] + z,
        ]
    }

    pub fn events_since(&self, prev_seen: &[ClientId]) -> Vec<BotEvent> {
        let mut events = Vec::new();
        for contact in &self.seen {
            if !prev_seen.contains(&contact.id) {
                events.push(BotEvent::Spotted { id: contact.id });
            }
        }
        for id in prev_seen {
            if !self.seen.iter().any(|contact| contact.id == *id) {
                events.push(BotEvent::LostSight { id: *id });
            }
        }
        events
    }
}
