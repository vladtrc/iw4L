#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Killstreak {
    Uav,
    Airdrop,
    HelicopterFlares,
}

impl Killstreak {
    pub const ALL: [Self; 3] = [Self::Uav, Self::Airdrop, Self::HelicopterFlares];

    #[must_use]
    pub const fn kills(self) -> i32 {
        match self {
            Self::Uav => 3,
            Self::Airdrop => 4,
            Self::HelicopterFlares => 9,
        }
    }

    #[must_use]
    pub const fn weapon(self) -> &'static str {
        match self {
            Self::Uav => "killstreak_uav_mp",
            Self::Airdrop => AIRDROP_MARKER_WEAPON,
            Self::HelicopterFlares => "killstreak_helicopter_flares_mp",
        }
    }

    #[must_use]
    pub const fn pickup_splash(self) -> &'static str {
        match self {
            Self::Uav => "uav_pickup",
            Self::Airdrop => "airdrop_pickup",
            Self::HelicopterFlares => "helicopter_flares_pickup",
        }
    }

    #[must_use]
    pub const fn wire_tag(self) -> u8 {
        match self {
            Self::Uav => 0,
            Self::Airdrop => 1,
            Self::HelicopterFlares => 2,
        }
    }

    #[must_use]
    pub const fn from_wire_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Uav),
            1 => Some(Self::Airdrop),
            2 => Some(Self::HelicopterFlares),
            _ => None,
        }
    }
}

pub const DEFAULT_LOADOUT: [Killstreak; 3] = Killstreak::ALL;

#[must_use]
pub const fn streak_modifier(hardline: bool) -> i32 {
    if hardline { -1 } else { 0 }
}

pub fn earned(
    loadout: &[Killstreak],
    count: i32,
    last_earned: Option<Killstreak>,
    modifier: i32,
) -> impl Iterator<Item = (Killstreak, i32)> + '_ {
    let floor = last_earned.map_or(i32::MIN, Killstreak::kills);
    let mut sorted = [None; 3];
    for (slot, streak) in sorted.iter_mut().zip(loadout) {
        *slot = Some(*streak);
    }
    sorted.sort_unstable_by_key(|s| s.map_or(i32::MAX, Killstreak::kills));
    sorted
        .into_iter()
        .flatten()
        .take_while(move |s| s.kills() + modifier <= count)
        .filter(move |s| s.kills() > floor)
        .map(move |s| (s, (s.kills() + modifier).max(count)))
}

#[must_use]
pub fn is_buzzkill(loadout: &[Killstreak], victim_count: i32, modifier: i32) -> bool {
    loadout
        .iter()
        .any(|s| s.kills() + modifier == victim_count + 1)
}

pub const UAV_DURATION_MS: u32 = 30_000;

pub const UAV_ORBIT_PERIOD_MS: u32 = 60_000;

pub const UAV_MODEL: &str = "vehicle_uav_static_mp";

pub const RADAR_SWEEP_MS: u32 = 4_000;

pub const AIRDROP_MARKER_WEAPON: &str = "airdrop_marker_mp";

pub const LITTLE_BIRD_MODEL: &str = "vehicle_little_bird_armed";

pub const FLYBY_DISTANCE: f32 = 15_000.0;

pub const FLY_HEIGHT_OVER_SITE: f32 = 850.0;

pub const FLYBY_APPROACH_MPH: f32 = 250.0;

pub const FLYBY_SLOW_AFTER_MS: u32 = 2_000;

pub const CRATE_DROP_MS: u32 = 1_000;

pub const CRATE_TIMEOUT_MS: u32 = 90_000;

pub const CRATE_OWNER_USE_MS: i32 = 500;

pub const CRATE_OTHER_USE_MS: i32 = 3_000;

pub const CRATE_USE_RADIUS: f32 = 128.0;

pub const CRATE_FRIENDLY_MODEL: &str = "com_plasticcase_friendly";

pub const CRATE_ENEMY_MODEL: &str = "com_plasticcase_enemy";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrateContents {
    Ammo,
    Streak(Killstreak),
}

impl CrateContents {
    #[must_use]
    pub const fn wire_tag(self) -> u8 {
        match self {
            Self::Ammo => 0xff,
            Self::Streak(s) => s.wire_tag(),
        }
    }

    #[must_use]
    pub const fn from_wire_tag(tag: u8) -> Option<Self> {
        match tag {
            0xff => Some(Self::Ammo),
            _ => match Killstreak::from_wire_tag(tag) {
                Some(s) => Some(Self::Streak(s)),
                None => None,
            },
        }
    }
}

pub const CRATE_WEIGHTS: [(CrateContents, u32); 3] = [
    (CrateContents::Ammo, 17),
    (CrateContents::Streak(Killstreak::Uav), 17),
    (CrateContents::Streak(Killstreak::HelicopterFlares), 5),
];

#[must_use]
pub fn crate_contents(roll: u32) -> CrateContents {
    let total: u32 = CRATE_WEIGHTS.iter().map(|(_, w)| w).sum();
    let mut pick = roll % total;
    for (contents, weight) in CRATE_WEIGHTS {
        if pick < weight {
            return contents;
        }
        pick -= weight;
    }
    CRATE_WEIGHTS[0].0
}

pub const PAVELOW_MODELS: [&str; 2] = ["vehicle_pavelow_opfor", "vehicle_pavelow"];

pub const PAVELOW_MINIGUN: &str = "pavelow_minigun_mp";

pub const PAVELOW_LOOP_MS: u32 = 60_000;

pub const PAVELOW_DEFAULT_MPH: f32 = 60.0;

pub const PAVELOW_BURST: [u32; 2] = [40, 80];

pub const PAVELOW_SHOT_MS: u32 = 100;

pub const PAVELOW_BURST_PAUSE_MS: [u32; 2] = [1_000, 2_000];

pub const PAVELOW_RANGE: f32 = 3_500.0;

pub const PAVELOW_SPAWN_PROTECTION_MS: i32 = 5_000;

pub const MPH_TO_UNITS: f32 = 17.6;
