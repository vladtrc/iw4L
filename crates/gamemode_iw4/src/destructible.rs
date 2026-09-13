#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleDeathPresentation {
    pub clip: &'static str,
    pub husk: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleBodyState {
    pub state_index: u8,
    pub health: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleDestructibleKind {
    MovingTruck,
    Pickup,
}

impl VehicleDestructibleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MovingTruck => "vehicle_moving_truck",
            Self::Pickup => "vehicle_pickup",
        }
    }

    pub fn from_mapents(kind: &str) -> Option<Self> {
        match kind {
            "vehicle_moving_truck" => Some(Self::MovingTruck),
            "vehicle_pickup" => Some(Self::Pickup),
            _ => None,
        }
    }

    pub const fn definition(self) -> &'static VehicleDestructibleDefinition {
        match self {
            Self::MovingTruck => &VEHICLE_MOVING_TRUCK,
            Self::Pickup => &VEHICLE_PICKUP,
        }
    }

    pub const fn destroyed_state(self) -> u8 {
        match self {
            Self::MovingTruck => VEHICLE_MOVING_TRUCK_DESTROYED_STATE,
            Self::Pickup => VEHICLE_PICKUP_DESTROYED_STATE,
        }
    }

    pub const fn initial_body(self) -> VehicleBodyState {
        match self {
            Self::MovingTruck => vehicle_moving_truck_initial_body(),
            Self::Pickup => vehicle_pickup_initial_body(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToyDestructibleKind {
    Oxygen01,
    Oxygen02,
    PropaneTank02,
    PropaneTank02Small,
}

impl ToyDestructibleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oxygen01 => "toy_oxygen_tank_01",
            Self::Oxygen02 => "toy_oxygen_tank_02",
            Self::PropaneTank02 => "toy_propane_tank02",
            Self::PropaneTank02Small => "toy_propane_tank02_small",
        }
    }

    pub fn from_mapents(kind: &str) -> Option<Self> {
        match kind {
            "toy_oxygen_tank_01" => Some(Self::Oxygen01),
            "toy_oxygen_tank_02" => Some(Self::Oxygen02),
            "toy_propane_tank02" => Some(Self::PropaneTank02),
            "toy_propane_tank02_small" => Some(Self::PropaneTank02Small),
            _ => None,
        }
    }

    pub const fn definition(self) -> &'static ToyDestructibleDefinition {
        match self {
            Self::Oxygen01 => &TOY_OXYGEN_TANK_01,
            Self::Oxygen02 => &TOY_OXYGEN_TANK_02,
            Self::PropaneTank02 => &TOY_PROPANE_TANK02,
            Self::PropaneTank02Small => &TOY_PROPANE_TANK02_SMALL,
        }
    }

    pub const fn destroyed_state(self) -> u8 {
        self.definition().destroyed_state
    }

    pub const fn initial_body(self) -> VehicleBodyState {
        VehicleBodyState {
            state_index: 0,
            health: self.definition().health[0],
        }
    }
}

pub const VEHICLE_HEALTHDRAIN_AMOUNT: u32 = 15;

pub const VEHICLE_HEALTHDRAIN_INTERVAL_MS: u32 = 250;

pub const VEHICLE_LOOPFX_INTERVAL_MS: u32 = 400;

pub const VEHICLE_DEATH_FX_FORWARD: [f32; 3] = [0.0, 0.0, 100.0];

pub const VEHICLE_HEALTHDRAIN_ACTION_STATE: u8 = 2;

pub const VEHICLE_MOVING_TRUCK_DESTROYED_STATE: u8 = 5;

pub const VEHICLE_PICKUP_DESTROYED_STATE: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleDestructibleDefinition {
    pub health: [u32; 5],
    pub player_only_state: u8,
    pub valid_damage_zone: u8,
    pub loop_fx: [Option<&'static str>; 5],
    pub loop_sound: [Option<&'static str>; 5],
    pub health_drain: Option<(u32, f32, u32, &'static str)>,
    pub death_fx_tag: &'static str,
    pub death_fx: &'static str,
    pub death_sound: &'static str,
    pub explode_force: (u32, u32),

    pub explode_range_mp: u32,

    pub explode_damage: (u32, u32),
    pub earthquake: (f32, u32),
    pub death: DestructibleDeathPresentation,
}

pub const VEHICLE_MOVING_TRUCK: VehicleDestructibleDefinition = VehicleDestructibleDefinition {
    health: [300, 200, 100, 300, 400],
    player_only_state: 3,
    valid_damage_zone: 32,
    loop_fx: [
        Some("smoke/car_damage_whitesmoke"),
        Some("smoke/car_damage_blacksmoke"),
        Some("smoke/car_damage_blacksmoke_fire"),
        None,
        None,
    ],
    loop_sound: [
        None,
        None,
        Some("fire_vehicle_med"),
        Some("fire_vehicle_med"),
        None,
    ],
    health_drain: Some((15, 0.25, 210, "allies")),
    death_fx_tag: "tag_death_fx",
    death_fx: "explosions/vehicle_explosion_medium",
    death_sound: "car_explode",
    explode_force: (4_000, 5_000),
    explode_range_mp: 250,
    explode_damage: (50, 300),
    earthquake: (0.3, 500),
    death: DestructibleDeathPresentation {
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_moving_truck_dst",
    },
};

pub const VEHICLE_PICKUP: VehicleDestructibleDefinition = VehicleDestructibleDefinition {
    health: [300, 200, 100, 300, 400],
    player_only_state: 3,
    valid_damage_zone: 32,
    loop_fx: [
        Some("smoke/car_damage_whitesmoke"),
        Some("smoke/car_damage_blacksmoke"),
        Some("smoke/car_damage_blacksmoke_fire"),
        None,
        None,
    ],
    loop_sound: [
        None,
        None,
        Some("fire_vehicle_med"),
        Some("fire_vehicle_med"),
        None,
    ],
    health_drain: Some((15, 0.25, 210, "allies")),
    death_fx_tag: "tag_death_fx",
    death_fx: "explosions/small_vehicle_explosion",
    death_sound: "car_explode",
    explode_force: (4_000, 5_000),
    explode_range_mp: 250,
    explode_damage: (50, 300),
    earthquake: (0.3, 500),
    death: DestructibleDeathPresentation {
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_pickup_destroyed",
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToyDestructibleDefinition {
    pub health: &'static [u32],
    pub destroyed_state: u8,

    pub health_drain: Option<(u32, f32, u32, &'static str)>,

    pub cap_fx: Option<&'static str>,

    pub leak_loop_fx: Option<&'static str>,
    pub death_fx: &'static str,
    pub death_sound: &'static str,
    pub explode_force: (u32, u32),
    pub explode_range_mp: u32,

    pub explode_damage: (u32, u32),
    pub husk: &'static str,
}

pub const TOY_OXYGEN_HEALTH: &[u32] = &[150, 300];
pub const TOY_OXYGEN_DESTROYED_STATE: u8 = 2;

pub const TOY_OXYGEN_TANK_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_OXYGEN_HEALTH,
    destroyed_state: TOY_OXYGEN_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 64, "allies")),
    cap_fx: Some("props/oxygen_tank01_cap"),
    leak_loop_fx: Some("distortion/oxygen_tank_leak"),
    death_fx: "explosions/oxygen_tank01_explosion",
    death_sound: "oxygen_tank_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 150),
    husk: "machinery_oxygen_tank01_des",
};

pub const TOY_OXYGEN_TANK_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_OXYGEN_HEALTH,
    destroyed_state: TOY_OXYGEN_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 64, "allies")),
    cap_fx: Some("props/oxygen_tank02_cap"),
    leak_loop_fx: Some("distortion/oxygen_tank_leak"),
    death_fx: "explosions/oxygen_tank02_explosion",
    death_sound: "oxygen_tank_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 150),
    husk: "machinery_oxygen_tank02_des",
};

pub const TOY_PROPANE_TANK02_HEALTH: &[u32] = &[50, 350, 350, 150, 150];
pub const TOY_PROPANE_TANK02_DESTROYED_STATE: u8 = 5;

pub const TOY_PROPANE_TANK02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_PROPANE_TANK02_HEALTH,
    destroyed_state: TOY_PROPANE_TANK02_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 300, "allies")),
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: "explosions/propane_large_exp",
    death_sound: "propanetank02_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 600,
    explode_damage: (32, 300),
    husk: "com_propane_tank02_DES",
};

pub const TOY_PROPANE_TANK02_SMALL_HEALTH: &[u32] = &[50, 350, 350, 200, 200];
pub const TOY_PROPANE_TANK02_SMALL_DESTROYED_STATE: u8 = 5;

pub const TOY_PROPANE_TANK02_SMALL: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_PROPANE_TANK02_SMALL_HEALTH,
    destroyed_state: TOY_PROPANE_TANK02_SMALL_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 210, "allies")),
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: "explosions/propane_large_exp",
    death_sound: "propanetank02_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 400,
    explode_damage: (32, 100),
    husk: "com_propane_tank02_small_DES",
};

pub fn destructible_destroyed_state(kind: &str) -> Option<u8> {
    VehicleDestructibleKind::from_mapents(kind).map(VehicleDestructibleKind::destroyed_state)
}

pub const fn vehicle_moving_truck_initial_body() -> VehicleBodyState {
    VehicleBodyState {
        state_index: 0,
        health: VEHICLE_MOVING_TRUCK.health[0],
    }
}

pub const fn vehicle_pickup_initial_body() -> VehicleBodyState {
    VehicleBodyState {
        state_index: 0,
        health: VEHICLE_PICKUP.health[0],
    }
}

pub fn apply_destructible_part_player_bullet(
    health: &[u32],
    destroyed_state: u8,
    mut state: VehicleBodyState,
    mut damage: u32,
) -> VehicleBodyState {
    if damage == 0 || state.state_index >= destroyed_state {
        return state;
    }

    while damage >= state.health {
        damage -= state.health;
        state.state_index += 1;
        if state.state_index >= destroyed_state {
            return VehicleBodyState {
                state_index: destroyed_state,
                health: 0,
            };
        }
        let idx = state.state_index as usize;
        if idx >= health.len() {
            return VehicleBodyState {
                state_index: destroyed_state,
                health: 0,
            };
        }
        state.health = health[idx];
    }
    state.health -= damage;
    state
}

pub fn apply_vehicle_player_bullet(
    def: &VehicleDestructibleDefinition,
    destroyed_state: u8,
    state: VehicleBodyState,
    damage: u32,
) -> VehicleBodyState {
    apply_destructible_part_player_bullet(&def.health, destroyed_state, state, damage)
}

pub fn apply_toy_player_bullet(
    def: &ToyDestructibleDefinition,
    state: VehicleBodyState,
    damage: u32,
) -> VehicleBodyState {
    apply_destructible_part_player_bullet(def.health, def.destroyed_state, state, damage)
}

pub fn toy_healthdrain_arms(previous: u8, next: u8, destroyed_state: u8) -> bool {
    previous == 0 && next > 0 && next < destroyed_state
}

pub fn vehicle_healthdrain_arms(previous: u8, next: u8, destroyed_state: u8) -> bool {
    previous <= VEHICLE_HEALTHDRAIN_ACTION_STATE
        && next > VEHICLE_HEALTHDRAIN_ACTION_STATE
        && next < destroyed_state
}

pub fn vehicle_active_loop_fx(
    def: &VehicleDestructibleDefinition,
    state_index: u8,
    destroyed_state: u8,
) -> Option<&'static str> {
    if state_index == 0 || state_index >= destroyed_state {
        return None;
    }
    let last_left = (state_index as usize - 1).min(def.loop_fx.len().saturating_sub(1));
    def.loop_fx[..=last_left].iter().rev().find_map(|fx| *fx)
}

pub fn vehicle_death_presentation_if_destroyed(
    def: &VehicleDestructibleDefinition,
    state_index: u8,
    destroyed_state: u8,
) -> Option<DestructibleDeathPresentation> {
    (state_index >= destroyed_state).then_some(def.death)
}

pub fn vehicle_death_fx_if_destroyed(
    def: &VehicleDestructibleDefinition,
    state_index: u8,
    destroyed_state: u8,
) -> Option<(&'static str, &'static str)> {
    (state_index >= destroyed_state).then_some((def.death_fx, def.death_sound))
}

pub fn apply_vehicle_moving_truck_player_bullet(
    state: VehicleBodyState,
    damage: u32,
) -> VehicleBodyState {
    apply_vehicle_player_bullet(
        &VEHICLE_MOVING_TRUCK,
        VEHICLE_MOVING_TRUCK_DESTROYED_STATE,
        state,
        damage,
    )
}

pub fn apply_vehicle_pickup_player_bullet(
    state: VehicleBodyState,
    damage: u32,
) -> VehicleBodyState {
    apply_vehicle_player_bullet(
        &VEHICLE_PICKUP,
        VEHICLE_PICKUP_DESTROYED_STATE,
        state,
        damage,
    )
}

pub fn destructible_death_presentation(kind: &str) -> Option<DestructibleDeathPresentation> {
    let husk = match kind {
        "vehicle_moving_truck" => "vehicle_moving_truck_dst",
        "vehicle_pickup" => "vehicle_pickup_destroyed",
        "vehicle_hummer" => "vehicle_hummer_destroyed",
        "vehicle_uaz_open" => "vehicle_uaz_open_dsr",
        _ => return None,
    };
    Some(DestructibleDeathPresentation {
        clip: "vehicle_80s_sedan1_destroy",
        husk,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleGlassPart {
    pub tag: &'static str,
    pub damaged_tag: &'static str,
    pub fx_tag: &'static str,
    pub fx: &'static str,
    pub health: [u32; 2],
}

pub const PICKUP_GLASS_PARTS: [VehicleGlassPart; 6] = [
    VehicleGlassPart {
        tag: "tag_glass_front",
        damaged_tag: "tag_glass_front_d",
        fx_tag: "tag_glass_front_fx",
        fx: "props/car_glass_large",
        health: [40, 60],
    },
    VehicleGlassPart {
        tag: "tag_glass_back",
        damaged_tag: "tag_glass_back_d",
        fx_tag: "tag_glass_back_fx",
        fx: "props/car_glass_large",
        health: [40, 60],
    },
    VehicleGlassPart {
        tag: "tag_glass_left_front",
        damaged_tag: "tag_glass_left_front_d",
        fx_tag: "tag_glass_left_front_fx",
        fx: "props/car_glass_med",
        health: [20, 60],
    },
    VehicleGlassPart {
        tag: "tag_glass_right_front",
        damaged_tag: "tag_glass_right_front_d",
        fx_tag: "tag_glass_right_front_fx",
        fx: "props/car_glass_med",
        health: [20, 60],
    },
    VehicleGlassPart {
        tag: "tag_glass_left_back",
        damaged_tag: "tag_glass_left_back_d",
        fx_tag: "tag_glass_left_back_fx",
        fx: "props/car_glass_med",
        health: [20, 60],
    },
    VehicleGlassPart {
        tag: "tag_glass_right_back",
        damaged_tag: "tag_glass_right_back_d",
        fx_tag: "tag_glass_right_back_fx",
        fx: "props/car_glass_med",
        health: [20, 60],
    },
];
