pub const EXPLODABLE_BARREL_HEALTH: u32 = 150;

pub const EXPLODABLE_BARREL_HEALTH_TABLE: [u32; 1] = [EXPLODABLE_BARREL_HEALTH];

pub const EXPLODABLE_BARREL_DESTROYED_STATE: u8 = 1;

pub const EXPLODABLE_BARREL_EXPLODE_RANGE: u32 = 250;

pub const EXPLODABLE_BARREL_EXPLODE_DAMAGE: (u32, u32) = (1, 250);

pub const EXPLODABLE_BARREL_DEATH_FX: &str = "props/barrelexp";

pub const EXPLODABLE_BARREL_DEATH_SOUND: &str = "explo_metal_rand";

pub const EXPLODABLE_BARREL_HUSK: &str = "com_barrel_piece";

pub const EXPLODABLE_BARREL_BURN_START_FX: &str = "props/barrel_ignite";

pub const EXPLODABLE_BARREL_BURN_LOOP_FX: &str = "props/barrel_fire_top";

pub const EXPLODABLE_BARREL_BURN_DRAIN: u32 = 10;

pub const EXPLODABLE_BARREL_BURN_DRAIN_INTERVAL_MS: u32 = 1000;

pub const EXPLODABLE_BARREL_BURN_LOOP_INTERVAL_MS: u32 = 50;

pub fn approve_explodable_barrel_host_policy() {}
