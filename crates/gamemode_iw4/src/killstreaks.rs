#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Killstreak {
    Uav,
    Airdrop,
    HelicopterFlares,
    PredatorMissile,
}

impl Killstreak {
    pub const ALL: [Self; 4] = [
        Self::Uav,
        Self::Airdrop,
        Self::PredatorMissile,
        Self::HelicopterFlares,
    ];

    #[must_use]
    pub const fn kills(self) -> i32 {
        match self {
            Self::Uav => 3,
            Self::Airdrop => 4,
            Self::HelicopterFlares => 9,
            Self::PredatorMissile => 5,
        }
    }

    #[must_use]
    pub const fn weapon(self) -> &'static str {
        match self {
            Self::Uav => "killstreak_uav_mp",
            Self::Airdrop => AIRDROP_MARKER_WEAPON,
            Self::HelicopterFlares => "killstreak_helicopter_flares_mp",
            Self::PredatorMissile => "killstreak_predator_missile_mp",
        }
    }

    #[must_use]
    pub const fn pickup_splash(self) -> &'static str {
        match self {
            Self::Uav => "uav_pickup",
            Self::Airdrop => "airdrop_pickup",
            Self::HelicopterFlares => "helicopter_flares_pickup",
            Self::PredatorMissile => "predator_missile_pickup",
        }
    }

    #[must_use]
    pub const fn wire_tag(self) -> u8 {
        match self {
            Self::Uav => 0,
            Self::Airdrop => 1,
            Self::HelicopterFlares => 2,
            Self::PredatorMissile => 3,
        }
    }

    #[must_use]
    pub const fn from_wire_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Uav),
            1 => Some(Self::Airdrop),
            2 => Some(Self::HelicopterFlares),
            3 => Some(Self::PredatorMissile),
            _ => None,
        }
    }
}

pub const DEFAULT_LOADOUT: [Killstreak; 4] = Killstreak::ALL;

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
    let mut sorted = [None; Killstreak::ALL.len()];
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

pub const FLYBY_GOAL_SHORT_OF_SITE: f32 = 50.0;

pub const FLYBY_START_JITTER: f32 = 100.0;

pub const FLYBY_END_JITTER: f32 = 150.0;

pub const FLYBY_APPROACH_MPH: f32 = 250.0;

pub const FLYBY_APPROACH_ACCEL_MPH: f32 = 175.0;

pub const FLYBY_SLOW_AFTER_MS: u32 = 2_000;

pub const FLYBY_SLOW_MPH: f32 = 75.0;

pub const FLYBY_SLOW_ACCEL_MPH: f32 = 40.0;

pub const FLYBY_DROP_AFTER_GOAL_MS: u32 = 100;

pub const FLYBY_LEAVE_MPH: f32 = 300.0;

pub const FLYBY_LEAVE_ACCEL_MPH: f32 = 75.0;

pub const FLYBY_YAW_ACCEL_DEG: f32 = 180.0;

pub const CRATE_TAG_GROUND_OFFSET: [f32; 3] = [32.0, 0.0, 5.0];

pub const LITTLE_BIRD_TAG_GROUND: [f32; 3] = [0.0, 0.0, -107.168_78];

pub const LITTLE_BIRD_TAIL_ROTOR: [f32; 3] = [-188.710_14, 11.686_385, -39.676_53];

pub const LITTLE_BIRD_HEALTH: i32 = 500;

pub const LITTLE_BIRD_MAX_PITCH: f32 = 45.0;

pub const LITTLE_BIRD_MAX_ROLL: f32 = 85.0;

// littlebird_mp VehicleDef accel, units/s^2.
pub const LITTLE_BIRD_DEF_ACCEL: f32 = 616.0;

// Not read from the def: CoD4 vehicle defaults.
pub const VEHICLE_MAX_TILT_VEL: f32 = 45.0;

pub const VEHICLE_FAKE_DRAG_MPH: f32 = 60.0;

pub const VEHICLE_FAKE_DRAG_ACCEL: f32 = 100.0;

// Sphere stand-in for the bird's model collision.
pub const LITTLE_BIRD_HIT_CENTER: [f32; 3] = [-50.0, 0.0, -60.0];

pub const LITTLE_BIRD_HIT_RADIUS: f32 = 140.0;

pub const LITTLE_BIRD_DYING_MPH: f32 = 25.0;

pub const LITTLE_BIRD_DYING_ACCEL_MPH: f32 = 5.0;

pub const LITTLE_BIRD_SPIN_DEG: [u32; 2] = [180, 220];

pub const LITTLE_BIRD_CRASH_DELAY_MS: [u32; 2] = [500, 1_500];

pub const LITTLE_BIRD_TAIL_FX: &str = "explosions/aerial_explosion";

pub const LITTLE_BIRD_DEATH_FX: &str = "explosions/helicopter_explosion_cobra_low";

pub const LITTLE_BIRD_CRASH_SOUND: &str = "cobra_helicopter_crash";

pub const CRATE_GRAVITY: f32 = 800.0;

pub const CRATE_LOST_BELOW_SITE: f32 = 3_000.0;

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

pub const CRATE_WEIGHTS: [(CrateContents, u32); 4] = [
    (CrateContents::Ammo, 17),
    (CrateContents::Streak(Killstreak::Uav), 17),
    (CrateContents::Streak(Killstreak::PredatorMissile), 12),
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

pub const PREDATOR_PROJECTILE: &str = "remotemissile_projectile_mp";

pub const PREDATOR_LAUNCH_HEIGHT: f32 = 14_000.0;

pub const PREDATOR_LAUNCH_BACK: f32 = 7_000.0;

pub const PREDATOR_TARGET_AHEAD: f32 = 1_500.0;

pub const PREDATOR_PITCH_RANGE: [f32; 2] = [1.0, 87.0];

pub const PREDATOR_PITCH_RATE: f32 = 15.0;

pub const PREDATOR_YAW_RATE: f32 = 20.0;

pub const PREDATOR_SPEED_RANGE: [f32; 2] = [3_000.0, 6_000.0];

pub const PREDATOR_SPEED_UP: f32 = 2_000.0;

pub const PREDATOR_SPEED_DOWN: f32 = 500.0;

pub const PREDATOR_FOV: f32 = 15.0;

pub const PREDATOR_STATIC_MS: i32 = 500;
