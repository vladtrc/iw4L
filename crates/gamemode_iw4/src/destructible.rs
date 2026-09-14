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
    Policecar,
}

impl VehicleDestructibleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MovingTruck => "vehicle_moving_truck",
            Self::Pickup => "vehicle_pickup",
            Self::Policecar => "vehicle_policecar",
        }
    }

    pub fn from_mapents(kind: &str) -> Option<Self> {
        match kind {
            "vehicle_moving_truck" => Some(Self::MovingTruck),
            "vehicle_pickup" => Some(Self::Pickup),
            "vehicle_policecar" => Some(Self::Policecar),
            _ => None,
        }
    }

    pub const fn definition(self) -> &'static VehicleDestructibleDefinition {
        match self {
            Self::MovingTruck => &VEHICLE_MOVING_TRUCK,
            Self::Pickup => &VEHICLE_PICKUP,
            Self::Policecar => &VEHICLE_POLICECAR,
        }
    }

    pub const fn destroyed_state(self) -> u8 {
        match self {
            Self::MovingTruck => VEHICLE_MOVING_TRUCK_DESTROYED_STATE,
            Self::Pickup => VEHICLE_PICKUP_DESTROYED_STATE,
            Self::Policecar => VEHICLE_POLICECAR_DESTROYED_STATE,
        }
    }

    pub const fn initial_body(self) -> VehicleBodyState {
        match self {
            Self::MovingTruck => vehicle_moving_truck_initial_body(),
            Self::Pickup => vehicle_pickup_initial_body(),
            Self::Policecar => vehicle_policecar_initial_body(),
        }
    }

    pub const fn has_body_glass(self) -> bool {
        matches!(self, Self::Pickup | Self::Policecar)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToyDestructibleKind {
    Oxygen01,
    Oxygen02,
    PropaneTank02,
    PropaneTank02Small,
    TubeTv1,
    TubeTv2,
    Flatscreen01,
    Flatscreen02,
    FlatscreenWallmount01,
    FlatscreenWallmount02,
    LightCeilingFluorescent,
    LightCeilingFluorescentSingle,
    Electricbox2,
    Electricbox4,
    Airconditioner,
    WallFan,
    LockerDouble,
    Filecabinet,
    GasStationTrashBin01,
    CeilingFan,
    Trashbin01,
    Trashbin02,
    TransformerSmall01,
    WaterCollector,
    NewspaperStandRed,
    NewspaperStandBlue,
    ChickenBlackWhite,
    ChickenWhite,
    Firehydrant,
    GasStationTrashBin02,
    TransformerRatnest01,
    Copier,
    Generator,
    GeneratorOn,
    DtMirrorLarge,
    DtMirror,
}

pub const TOY_DESTRUCTIBLE_KINDS: &[ToyDestructibleKind] = &[
    ToyDestructibleKind::Oxygen01,
    ToyDestructibleKind::Oxygen02,
    ToyDestructibleKind::PropaneTank02,
    ToyDestructibleKind::PropaneTank02Small,
    ToyDestructibleKind::TubeTv1,
    ToyDestructibleKind::TubeTv2,
    ToyDestructibleKind::Flatscreen01,
    ToyDestructibleKind::Flatscreen02,
    ToyDestructibleKind::FlatscreenWallmount01,
    ToyDestructibleKind::FlatscreenWallmount02,
    ToyDestructibleKind::LightCeilingFluorescent,
    ToyDestructibleKind::LightCeilingFluorescentSingle,
    ToyDestructibleKind::Electricbox2,
    ToyDestructibleKind::Electricbox4,
    ToyDestructibleKind::Airconditioner,
    ToyDestructibleKind::WallFan,
    ToyDestructibleKind::LockerDouble,
    ToyDestructibleKind::Filecabinet,
    ToyDestructibleKind::GasStationTrashBin01,
    ToyDestructibleKind::CeilingFan,
    ToyDestructibleKind::Trashbin01,
    ToyDestructibleKind::Trashbin02,
    ToyDestructibleKind::TransformerSmall01,
    ToyDestructibleKind::WaterCollector,
    ToyDestructibleKind::NewspaperStandRed,
    ToyDestructibleKind::NewspaperStandBlue,
    ToyDestructibleKind::ChickenBlackWhite,
    ToyDestructibleKind::ChickenWhite,
    ToyDestructibleKind::Firehydrant,
    ToyDestructibleKind::GasStationTrashBin02,
    ToyDestructibleKind::TransformerRatnest01,
    ToyDestructibleKind::Copier,
    ToyDestructibleKind::Generator,
    ToyDestructibleKind::GeneratorOn,
    ToyDestructibleKind::DtMirrorLarge,
    ToyDestructibleKind::DtMirror,
];

impl ToyDestructibleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oxygen01 => "toy_oxygen_tank_01",
            Self::Oxygen02 => "toy_oxygen_tank_02",
            Self::PropaneTank02 => "toy_propane_tank02",
            Self::PropaneTank02Small => "toy_propane_tank02_small",
            Self::TubeTv1 => "toy_tubetv_tv1",
            Self::TubeTv2 => "toy_tubetv_tv2",
            Self::Flatscreen01 => "toy_tv_flatscreen_01",
            Self::Flatscreen02 => "toy_tv_flatscreen_02",
            Self::FlatscreenWallmount01 => "toy_tv_flatscreen_wallmount_01",
            Self::FlatscreenWallmount02 => "toy_tv_flatscreen_wallmount_02",
            Self::LightCeilingFluorescent => "toy_light_ceiling_fluorescent",
            Self::LightCeilingFluorescentSingle => "toy_light_ceiling_fluorescent_single",
            Self::Electricbox2 => "toy_electricbox2",
            Self::Electricbox4 => "toy_electricbox4",
            Self::Airconditioner => "toy_airconditioner",
            Self::WallFan => "toy_wall_fan",
            Self::LockerDouble => "toy_locker_double",
            Self::Filecabinet => "toy_filecabinet",
            Self::GasStationTrashBin01 => "toy_usa_gas_station_trash_bin_01",
            Self::CeilingFan => "toy_ceiling_fan",
            Self::Trashbin01 => "toy_trashbin_01",
            Self::Trashbin02 => "toy_trashbin_02",
            Self::TransformerSmall01 => "toy_transformer_small01",
            Self::WaterCollector => "toy_water_collector",
            Self::NewspaperStandRed => "toy_newspaper_stand_red",
            Self::NewspaperStandBlue => "toy_newspaper_stand_blue",
            Self::ChickenBlackWhite => "toy_chicken_black_white",
            Self::ChickenWhite => "toy_chicken_white",
            Self::Firehydrant => "toy_firehydrant",
            Self::GasStationTrashBin02 => "toy_usa_gas_station_trash_bin_02",
            Self::TransformerRatnest01 => "toy_transformer_ratnest01",
            Self::Copier => "toy_copier",
            Self::Generator => "toy_generator",
            Self::GeneratorOn => "toy_generator_on",
            Self::DtMirrorLarge => "toy_dt_mirror_large",
            Self::DtMirror => "toy_dt_mirror",
        }
    }

    pub fn from_mapents(kind: &str) -> Option<Self> {
        match kind {
            "toy_oxygen_tank_01" => Some(Self::Oxygen01),
            "toy_oxygen_tank_02" => Some(Self::Oxygen02),
            "toy_propane_tank02" => Some(Self::PropaneTank02),
            "toy_propane_tank02_small" => Some(Self::PropaneTank02Small),
            "toy_tubetv_tv1" => Some(Self::TubeTv1),
            "toy_tubetv_tv2" => Some(Self::TubeTv2),
            "toy_tv_flatscreen_01" => Some(Self::Flatscreen01),
            "toy_tv_flatscreen_02" => Some(Self::Flatscreen02),
            "toy_tv_flatscreen_wallmount_01" => Some(Self::FlatscreenWallmount01),
            "toy_tv_flatscreen_wallmount_02" => Some(Self::FlatscreenWallmount02),
            "toy_light_ceiling_fluorescent" => Some(Self::LightCeilingFluorescent),
            "toy_light_ceiling_fluorescent_single" => Some(Self::LightCeilingFluorescentSingle),
            "toy_electricbox2" => Some(Self::Electricbox2),
            "toy_electricbox4" => Some(Self::Electricbox4),
            "toy_airconditioner" => Some(Self::Airconditioner),
            "toy_wall_fan" => Some(Self::WallFan),
            "toy_locker_double" => Some(Self::LockerDouble),
            "toy_filecabinet" => Some(Self::Filecabinet),
            "toy_usa_gas_station_trash_bin_01" => Some(Self::GasStationTrashBin01),
            "toy_ceiling_fan" => Some(Self::CeilingFan),
            "toy_trashbin_01" => Some(Self::Trashbin01),
            "toy_trashbin_02" => Some(Self::Trashbin02),
            "toy_transformer_small01" => Some(Self::TransformerSmall01),
            "toy_water_collector" => Some(Self::WaterCollector),
            "toy_newspaper_stand_red" => Some(Self::NewspaperStandRed),
            "toy_newspaper_stand_blue" => Some(Self::NewspaperStandBlue),
            "toy_chicken_black_white" => Some(Self::ChickenBlackWhite),
            "toy_chicken_white" => Some(Self::ChickenWhite),
            "toy_firehydrant" => Some(Self::Firehydrant),
            "toy_usa_gas_station_trash_bin_02" => Some(Self::GasStationTrashBin02),
            "toy_transformer_ratnest01" => Some(Self::TransformerRatnest01),
            "toy_copier" => Some(Self::Copier),
            "toy_generator" => Some(Self::Generator),
            "toy_generator_on" => Some(Self::GeneratorOn),
            "toy_dt_mirror_large" => Some(Self::DtMirrorLarge),
            "toy_dt_mirror" => Some(Self::DtMirror),
            _ => None,
        }
    }

    pub const fn definition(self) -> &'static ToyDestructibleDefinition {
        match self {
            Self::Oxygen01 => &TOY_OXYGEN_TANK_01,
            Self::Oxygen02 => &TOY_OXYGEN_TANK_02,
            Self::PropaneTank02 => &TOY_PROPANE_TANK02,
            Self::PropaneTank02Small => &TOY_PROPANE_TANK02_SMALL,
            Self::TubeTv1 => &TOY_TUBETV_TV1,
            Self::TubeTv2 => &TOY_TUBETV_TV2,
            Self::Flatscreen01 => &TOY_FLATSCREEN_01,
            Self::Flatscreen02 => &TOY_FLATSCREEN_02,
            Self::FlatscreenWallmount01 => &TOY_FLATSCREEN_WALLMOUNT_01,
            Self::FlatscreenWallmount02 => &TOY_FLATSCREEN_WALLMOUNT_02,
            Self::LightCeilingFluorescent => &TOY_LIGHT_CEILING_FLUORESCENT,
            Self::LightCeilingFluorescentSingle => &TOY_LIGHT_CEILING_FLUORESCENT_SINGLE,
            Self::Electricbox2 => &TOY_ELECTRICBOX2,
            Self::Electricbox4 => &TOY_ELECTRICBOX4,
            Self::Airconditioner => &TOY_AIRCONDITIONER,
            Self::WallFan => &TOY_WALL_FAN,
            Self::LockerDouble => &TOY_LOCKER_DOUBLE,
            Self::Filecabinet => &TOY_FILECABINET,
            Self::GasStationTrashBin01 => &TOY_GAS_STATION_TRASH_BIN_01,
            Self::CeilingFan => &TOY_CEILING_FAN,
            Self::Trashbin01 => &TOY_TRASHBIN_01,
            Self::Trashbin02 => &TOY_TRASHBIN_02,
            Self::TransformerSmall01 => &TOY_TRANSFORMER_SMALL01,
            Self::WaterCollector => &TOY_WATER_COLLECTOR,
            Self::NewspaperStandRed => &TOY_NEWSPAPER_STAND_RED,
            Self::NewspaperStandBlue => &TOY_NEWSPAPER_STAND_BLUE,
            Self::ChickenBlackWhite => &TOY_CHICKEN_BLACK_WHITE,
            Self::ChickenWhite => &TOY_CHICKEN_WHITE,
            Self::Firehydrant => &TOY_FIREHYDRANT,
            Self::GasStationTrashBin02 => &TOY_GAS_STATION_TRASH_BIN_02,
            Self::TransformerRatnest01 => &TOY_TRANSFORMER_RATNEST01,
            Self::Copier => &TOY_COPIER,
            Self::Generator => &TOY_GENERATOR,
            Self::GeneratorOn => &TOY_GENERATOR_ON,
            Self::DtMirrorLarge => &TOY_DT_MIRROR_LARGE,
            Self::DtMirror => &TOY_DT_MIRROR,
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

    /// Live model at match start. Unlit flatscreen mapents look like husks;
    /// prefer the `_on_` variant the mapper already used on the same maps.
    pub fn spawn_model(self, mapent: &str) -> &str {
        self.spawn_model_candidates(mapent)
            .first()
            .copied()
            .unwrap_or(mapent)
    }

    pub fn spawn_model_candidates(self, mapent: &str) -> &'static [&'static str] {
        match (self, mapent) {
            (Self::FlatscreenWallmount02, "ma_flatscreen_tv_wallmount_02") => &[
                "ma_flatscreen_tv_on_wallmount_02_static",
                "ma_flatscreen_tv_on_wallmount_02",
            ],
            (Self::Flatscreen02, "ma_flatscreen_tv_02") => &["ma_flatscreen_tv_on_02"],
            _ => &[],
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

pub const VEHICLE_POLICECAR_DESTROYED_STATE: u8 = 5;

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

pub const VEHICLE_POLICECAR: VehicleDestructibleDefinition = VehicleDestructibleDefinition {
    health: [250, 200, 100, 300, 400],
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
    death_sound: "car_explode_police",
    explode_force: (4_000, 5_000),
    explode_range_mp: 250,
    explode_damage: (50, 300),
    earthquake: (0.3, 500),
    death: DestructibleDeathPresentation {
        clip: "vehicle_80s_sedan1_destroy",
        husk: "vehicle_policecar_lapd_destroy",
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
    pub death_fx_tag: Option<&'static str>,
    pub death_sound: &'static str,
    pub explode_force: (u32, u32),
    pub explode_range_mp: u32,

    pub explode_damage: (u32, u32),
    pub explode_origin_offset_z: f32,
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
    death_fx_tag: Some("tag_fx"),
    death_sound: "oxygen_tank_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 150),
    explode_origin_offset_z: 32.0,
    husk: "machinery_oxygen_tank01_des",
};

pub const TOY_OXYGEN_TANK_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_OXYGEN_HEALTH,
    destroyed_state: TOY_OXYGEN_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 64, "allies")),
    cap_fx: Some("props/oxygen_tank02_cap"),
    leak_loop_fx: Some("distortion/oxygen_tank_leak"),
    death_fx: "explosions/oxygen_tank02_explosion",
    death_fx_tag: Some("tag_fx"),
    death_sound: "oxygen_tank_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 150),
    explode_origin_offset_z: 32.0,
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
    death_fx_tag: Some("tag_fx"),
    death_sound: "propanetank02_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 600,
    explode_damage: (32, 300),
    explode_origin_offset_z: 80.0,
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
    death_fx_tag: Some("tag_fx"),
    death_sound: "propanetank02_explode",
    explode_force: (7_000, 8_000),
    explode_range_mp: 400,
    explode_damage: (32, 100),
    explode_origin_offset_z: 80.0,
    husk: "com_propane_tank02_small_DES",
};

pub const TOY_TV_HEALTH: &[u32] = &[1];
pub const TOY_TV_DESTROYED_STATE: u8 = 1;
pub const TOY_TV_DEATH_SOUND: &str = "tv_shot_burst";
pub const TOY_TV_DEATH_FX_TAG: &str = "tag_fx";
pub const TOY_TUBETV_DEATH_FX: &str = "explosions/tv_explosion";
pub const TOY_FLATSCREEN_DEATH_FX: &str = "explosions/tv_flatscreen_explosion";
pub const TOY_TUBETV_EXPLODE_RANGE_MP: u32 = 9;
pub const TOY_FLATSCREEN_EXPLODE_RANGE_MP: u32 = 10;
pub const TOY_TUBETV_EXPLODE_ORIGIN_OFFSET_Z: f32 = 12.0;
pub const TOY_FLATSCREEN_EXPLODE_ORIGIN_OFFSET_Z: f32 = 15.0;

const fn toy_tv(
    husk: &'static str,
    death_fx: &'static str,
    explode_range_mp: u32,
    explode_origin_offset_z: f32,
) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_TV_HEALTH,
        destroyed_state: TOY_TV_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx,
        death_fx_tag: Some(TOY_TV_DEATH_FX_TAG),
        death_sound: TOY_TV_DEATH_SOUND,
        explode_force: (20, 2_000),
        explode_range_mp,
        explode_damage: (3, 3),
        explode_origin_offset_z,
        husk,
    }
}

pub const TOY_TUBETV_TV1: ToyDestructibleDefinition = toy_tv(
    "com_tv1_d",
    TOY_TUBETV_DEATH_FX,
    TOY_TUBETV_EXPLODE_RANGE_MP,
    TOY_TUBETV_EXPLODE_ORIGIN_OFFSET_Z,
);
pub const TOY_TUBETV_TV2: ToyDestructibleDefinition = toy_tv(
    "com_tv2_d",
    TOY_TUBETV_DEATH_FX,
    TOY_TUBETV_EXPLODE_RANGE_MP,
    TOY_TUBETV_EXPLODE_ORIGIN_OFFSET_Z,
);
pub const TOY_FLATSCREEN_01: ToyDestructibleDefinition = toy_tv(
    "ma_flatscreen_tv_broken_01",
    TOY_FLATSCREEN_DEATH_FX,
    TOY_FLATSCREEN_EXPLODE_RANGE_MP,
    TOY_FLATSCREEN_EXPLODE_ORIGIN_OFFSET_Z,
);
pub const TOY_FLATSCREEN_02: ToyDestructibleDefinition = toy_tv(
    "ma_flatscreen_tv_broken_02",
    TOY_FLATSCREEN_DEATH_FX,
    TOY_FLATSCREEN_EXPLODE_RANGE_MP,
    TOY_FLATSCREEN_EXPLODE_ORIGIN_OFFSET_Z,
);
pub const TOY_FLATSCREEN_WALLMOUNT_01: ToyDestructibleDefinition = toy_tv(
    "ma_flatscreen_tv_wallmount_broken_01",
    TOY_FLATSCREEN_DEATH_FX,
    TOY_FLATSCREEN_EXPLODE_RANGE_MP,
    TOY_FLATSCREEN_EXPLODE_ORIGIN_OFFSET_Z,
);
pub const TOY_FLATSCREEN_WALLMOUNT_02: ToyDestructibleDefinition = toy_tv(
    "ma_flatscreen_tv_wallmount_broken_02",
    TOY_FLATSCREEN_DEATH_FX,
    TOY_FLATSCREEN_EXPLODE_RANGE_MP,
    TOY_FLATSCREEN_EXPLODE_ORIGIN_OFFSET_Z,
);

pub const TOY_FLUORESCENT_HEALTH: &[u32] = &[150];
pub const TOY_FLUORESCENT_DESTROYED_STATE: u8 = 1;
pub const TOY_FLUORESCENT_DEATH_SOUND: &str = "fluorescent_light_bulb";
pub const TOY_FLUORESCENT_DEATH_FX: &str = "misc/light_fluorescent_blowout_runner";
pub const TOY_FLUORESCENT_SINGLE_DEATH_FX: &str = "misc/light_fluorescent_single_blowout_runner";
pub const TOY_FLUORESCENT_EXPLODE_RANGE_MP: u32 = 64;
pub const TOY_FLUORESCENT_EXPLODE_DAMAGE: (u32, u32) = (40, 80);

const fn toy_fluorescent(husk: &'static str, death_fx: &'static str) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_FLUORESCENT_HEALTH,
        destroyed_state: TOY_FLUORESCENT_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx,
        death_fx_tag: Some("tag_fx"),
        death_sound: TOY_FLUORESCENT_DEATH_SOUND,
        explode_force: (20, 2_000),
        explode_range_mp: TOY_FLUORESCENT_EXPLODE_RANGE_MP,
        explode_damage: TOY_FLUORESCENT_EXPLODE_DAMAGE,
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_LIGHT_CEILING_FLUORESCENT: ToyDestructibleDefinition = toy_fluorescent(
    "me_lightfluohang_double_destroyed",
    TOY_FLUORESCENT_DEATH_FX,
);
pub const TOY_LIGHT_CEILING_FLUORESCENT_SINGLE: ToyDestructibleDefinition = toy_fluorescent(
    "me_lightfluohang_single_destroyed",
    TOY_FLUORESCENT_SINGLE_DEATH_FX,
);

pub const TOY_ELECTRICBOX_HEALTH: &[u32] = &[150];
pub const TOY_ELECTRICBOX_DESTROYED_STATE: u8 = 1;
pub const TOY_ELECTRICBOX_DEATH_FX: &str = "props/electricbox4_explode";
pub const TOY_ELECTRICBOX_DEATH_SOUND: &str = "exp_fusebox_sparks";
pub const TOY_ELECTRICBOX_EXPLODE_RANGE_MP: u32 = 32;
pub const TOY_ELECTRICBOX_EXPLODE_DAMAGE: (u32, u32) = (32, 48);

const fn toy_electricbox(
    husk: &'static str,
    explode_force: (u32, u32),
) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_ELECTRICBOX_HEALTH,
        destroyed_state: TOY_ELECTRICBOX_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx: TOY_ELECTRICBOX_DEATH_FX,
        death_fx_tag: Some("tag_fx"),
        death_sound: TOY_ELECTRICBOX_DEATH_SOUND,
        explode_force,
        explode_range_mp: TOY_ELECTRICBOX_EXPLODE_RANGE_MP,
        explode_damage: TOY_ELECTRICBOX_EXPLODE_DAMAGE,
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_ELECTRICBOX2: ToyDestructibleDefinition =
    toy_electricbox("me_electricbox2_dest", (1_000, 2_000));
pub const TOY_ELECTRICBOX4: ToyDestructibleDefinition =
    toy_electricbox("me_electricbox4_dest", (20, 2_000));

pub const TOY_AIRCONDITIONER_HEALTH: &[u32] = &[300];
pub const TOY_AIRCONDITIONER_DESTROYED_STATE: u8 = 1;
pub const TOY_AIRCONDITIONER_DEATH_FX: &str = "explosions/airconditioner_ex_explode";
pub const TOY_AIRCONDITIONER_DEATH_SOUND: &str = "airconditioner_burst";

pub const TOY_AIRCONDITIONER: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_AIRCONDITIONER_HEALTH,
    destroyed_state: TOY_AIRCONDITIONER_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_AIRCONDITIONER_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_AIRCONDITIONER_DEATH_SOUND,
    explode_force: (1_000, 2_000),
    explode_range_mp: 32,
    explode_damage: (32, 48),
    explode_origin_offset_z: 0.0,
    husk: "com_ex_airconditioner_dam",
};

pub const TOY_WALL_FAN_HEALTH: &[u32] = &[150, 150];
pub const TOY_WALL_FAN_DESTROYED_STATE: u8 = 2;
pub const TOY_WALL_FAN_DEATH_FX: &str = "explosions/wallfan_explosion_des";
pub const TOY_WALL_FAN_DEATH_SOUND: &str = "wall_fan_break";

pub const TOY_WALL_FAN: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_WALL_FAN_HEALTH,
    destroyed_state: TOY_WALL_FAN_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_WALL_FAN_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_WALL_FAN_DEATH_SOUND,
    explode_force: (0, 0),
    explode_range_mp: 0,
    explode_damage: (0, 0),
    explode_origin_offset_z: 0.0,
    husk: "cs_wallfan1_dmg",
};

pub const TOY_LOCKER_DOUBLE_HEALTH: &[u32] = &[150];
pub const TOY_LOCKER_DOUBLE_DESTROYED_STATE: u8 = 1;
pub const TOY_LOCKER_DOUBLE_DEATH_FX: &str = "props/locker_double_des_03_both";
pub const TOY_LOCKER_DOUBLE_DEATH_SOUND: &str = "lockers_double";

pub const TOY_LOCKER_DOUBLE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_LOCKER_DOUBLE_HEALTH,
    destroyed_state: TOY_LOCKER_DOUBLE_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_LOCKER_DOUBLE_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_LOCKER_DOUBLE_DEATH_SOUND,
    explode_force: (0, 0),
    explode_range_mp: 0,
    explode_damage: (0, 0),
    explode_origin_offset_z: 0.0,
    husk: "com_locker_double_destroyed",
};

pub const TOY_FILECABINET_HEALTH: &[u32] = &[120];
pub const TOY_FILECABINET_DESTROYED_STATE: u8 = 1;
pub const TOY_FILECABINET_DEATH_FX: &str = "props/filecabinet_dam";
pub const TOY_FILECABINET_DEATH_SOUND: &str = "exp_filecabinet";

pub const TOY_FILECABINET: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_FILECABINET_HEALTH,
    destroyed_state: TOY_FILECABINET_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_FILECABINET_DEATH_FX,
    death_fx_tag: Some("tag_drawer_lower"),
    death_sound: TOY_FILECABINET_DEATH_SOUND,
    explode_force: (0, 0),
    explode_range_mp: 0,
    explode_damage: (0, 0),
    explode_origin_offset_z: 0.0,
    husk: "com_filecabinetblackclosed_dam",
};

pub const TOY_GAS_STATION_TRASH_BIN_01_HEALTH: &[u32] = &[120];
pub const TOY_GAS_STATION_TRASH_BIN_01_DESTROYED_STATE: u8 = 1;
pub const TOY_GAS_STATION_TRASH_BIN_01_DEATH_FX: &str = "props/garbage_spew";

pub const TOY_GAS_STATION_TRASH_BIN_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_GAS_STATION_TRASH_BIN_01_HEALTH,
    destroyed_state: TOY_GAS_STATION_TRASH_BIN_01_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_GAS_STATION_TRASH_BIN_01_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: "",
    explode_force: (600, 651),
    explode_range_mp: 1,
    explode_damage: (10, 20),
    explode_origin_offset_z: 0.0,
    husk: "usa_gas_station_trash_bin_01_base",
};

pub const TOY_GAS_STATION_TRASH_BIN_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_GAS_STATION_TRASH_BIN_01_HEALTH,
    destroyed_state: TOY_GAS_STATION_TRASH_BIN_01_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_GAS_STATION_TRASH_BIN_01_DEATH_FX,
    death_fx_tag: Some("tag_fx_high"),
    death_sound: "",
    explode_force: (600, 651),
    explode_range_mp: 1,
    explode_damage: (10, 20),
    explode_origin_offset_z: 0.0,
    husk: "usa_gas_station_trash_bin_02_base",
};

pub const TOY_CEILING_FAN_HEALTH: &[u32] = &[150];
pub const TOY_CEILING_FAN_DESTROYED_STATE: u8 = 1;
pub const TOY_CEILING_FAN_DEATH_FX: &str = "explosions/ceiling_fan_explosion";
pub const TOY_CEILING_FAN_DEATH_SOUND: &str = "ceiling_fan_sparks";

pub const TOY_CEILING_FAN: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_CEILING_FAN_HEALTH,
    destroyed_state: TOY_CEILING_FAN_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_CEILING_FAN_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_CEILING_FAN_DEATH_SOUND,
    explode_force: (1_000, 2_000),
    explode_range_mp: 32,
    explode_damage: (5, 32),
    explode_origin_offset_z: 0.0,
    husk: "me_fanceil1_des",
};

pub const TOY_TRASHBIN_HEALTH: &[u32] = &[120];
pub const TOY_TRASHBIN_DESTROYED_STATE: u8 = 1;
pub const TOY_TRASHBIN_DEATH_FX: &str = "props/garbage_spew";
pub const TOY_TRASHBIN_DEATH_SOUND: &str = "exp_trashcan_sweet";
pub const TOY_TRASHBIN_EXPLODE_RANGE_MP: u32 = 1;
pub const TOY_TRASHBIN_EXPLODE_DAMAGE: (u32, u32) = (10, 20);

const fn toy_trashbin(husk: &'static str, explode_force: (u32, u32)) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_TRASHBIN_HEALTH,
        destroyed_state: TOY_TRASHBIN_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx: TOY_TRASHBIN_DEATH_FX,
        death_fx_tag: Some("tag_fx"),
        death_sound: TOY_TRASHBIN_DEATH_SOUND,
        explode_force,
        explode_range_mp: TOY_TRASHBIN_EXPLODE_RANGE_MP,
        explode_damage: TOY_TRASHBIN_EXPLODE_DAMAGE,
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_TRASHBIN_01: ToyDestructibleDefinition =
    toy_trashbin("com_trashbin01_dmg", (1_300, 1_351));
pub const TOY_TRASHBIN_02: ToyDestructibleDefinition =
    toy_trashbin("com_trashbin02_dmg", (600, 800));

pub const TOY_TRANSFORMER_SMALL01_HEALTH: &[u32] = &[75, 75, 150, 250, 400];
pub const TOY_TRANSFORMER_SMALL01_DESTROYED_STATE: u8 = 5;
pub const TOY_TRANSFORMER_SMALL01_DEATH_FX: &str = "explosions/transformer_explosion";
pub const TOY_TRANSFORMER_SMALL01_DEATH_SOUND: &str = "transformer01_explode";

pub const TOY_TRANSFORMER_SMALL01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_TRANSFORMER_SMALL01_HEALTH,
    destroyed_state: TOY_TRANSFORMER_SMALL01_DESTROYED_STATE,
    health_drain: Some((24, 0.2, 150, "allies")),
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_TRANSFORMER_SMALL01_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_TRANSFORMER_SMALL01_DEATH_SOUND,
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 100),
    explode_origin_offset_z: 0.0,
    husk: "utility_transformer_small01_dest",
};

pub const TOY_TRANSFORMER_RATNEST01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_TRANSFORMER_SMALL01_HEALTH,
    destroyed_state: TOY_TRANSFORMER_SMALL01_DESTROYED_STATE,
    health_drain: Some((24, 0.2, 150, "allies")),
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_TRANSFORMER_SMALL01_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_TRANSFORMER_SMALL01_DEATH_SOUND,
    explode_force: (7_000, 8_000),
    explode_range_mp: 256,
    explode_damage: (16, 100),
    explode_origin_offset_z: 0.0,
    husk: "utility_transformer_ratnest01_dest",
};

pub const TOY_WATER_COLLECTOR_HEALTH: &[u32] = &[220];
pub const TOY_WATER_COLLECTOR_DESTROYED_STATE: u8 = 1;
pub const TOY_WATER_COLLECTOR_DEATH_FX: &str = "explosions/water_collector_explosion";
pub const TOY_WATER_COLLECTOR_DEATH_SOUND: &str = "water_collector_splash";
pub const TOY_WATER_COLLECTOR_EXPLODE_ORIGIN_OFFSET_Z: f32 = 32.0;

pub const TOY_WATER_COLLECTOR: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_WATER_COLLECTOR_HEALTH,
    destroyed_state: TOY_WATER_COLLECTOR_DESTROYED_STATE,
    health_drain: None,
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_WATER_COLLECTOR_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_WATER_COLLECTOR_DEATH_SOUND,
    explode_force: (500, 800),
    explode_range_mp: 32,
    explode_damage: (1, 10),
    explode_origin_offset_z: TOY_WATER_COLLECTOR_EXPLODE_ORIGIN_OFFSET_Z,
    husk: "utility_water_collector_base_dest",
};

pub const TOY_NEWSPAPER_STAND_HEALTH: &[u32] = &[120];
pub const TOY_NEWSPAPER_STAND_DESTROYED_STATE: u8 = 1;
pub const TOY_NEWSPAPER_STAND_RED_DEATH_FX: &str = "props/news_stand_paper_spill";
pub const TOY_NEWSPAPER_STAND_BLUE_DEATH_FX: &str = "props/news_stand_paper_spill_shatter";
pub const TOY_NEWSPAPER_STAND_DEATH_SOUND: &str = "exp_newspaper_box";
pub const TOY_NEWSPAPER_STAND_EXPLODE_RANGE_MP: u32 = 64;
pub const TOY_NEWSPAPER_STAND_EXPLODE_DAMAGE: (u32, u32) = (0, 0);

const fn toy_newspaper_stand(
    husk: &'static str,
    death_fx: &'static str,
    explode_force: (u32, u32),
) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_NEWSPAPER_STAND_HEALTH,
        destroyed_state: TOY_NEWSPAPER_STAND_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx,
        death_fx_tag: Some("tag_door"),
        death_sound: TOY_NEWSPAPER_STAND_DEATH_SOUND,
        explode_force,
        explode_range_mp: TOY_NEWSPAPER_STAND_EXPLODE_RANGE_MP,
        explode_damage: TOY_NEWSPAPER_STAND_EXPLODE_DAMAGE,
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_NEWSPAPER_STAND_RED: ToyDestructibleDefinition = toy_newspaper_stand(
    "com_newspaperbox_red_dam",
    TOY_NEWSPAPER_STAND_RED_DEATH_FX,
    (2_500, 2_501),
);
pub const TOY_NEWSPAPER_STAND_BLUE: ToyDestructibleDefinition = toy_newspaper_stand(
    "com_newspaperbox_blue_dam",
    TOY_NEWSPAPER_STAND_BLUE_DEATH_FX,
    (800, 2_001),
);

pub const TOY_CHICKEN_HEALTH: &[u32] = &[25];
pub const TOY_CHICKEN_DESTROYED_STATE: u8 = 1;
pub const TOY_CHICKEN_BLACK_WHITE_DEATH_FX: &str = "props/chicken_exp_black_white";
pub const TOY_CHICKEN_WHITE_DEATH_FX: &str = "props/chicken_exp_white";
pub const TOY_CHICKEN_DEATH_SOUND: &str = "animal_chicken_death";

const fn toy_chicken(husk: &'static str, death_fx: &'static str) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_CHICKEN_HEALTH,
        destroyed_state: TOY_CHICKEN_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx,
        death_fx_tag: Some("tag_origin"),
        death_sound: TOY_CHICKEN_DEATH_SOUND,
        explode_force: (0, 0),
        explode_range_mp: 0,
        explode_damage: (0, 0),
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_CHICKEN_BLACK_WHITE: ToyDestructibleDefinition =
    toy_chicken("chicken_black_white", TOY_CHICKEN_BLACK_WHITE_DEATH_FX);
pub const TOY_CHICKEN_WHITE: ToyDestructibleDefinition =
    toy_chicken("chicken_white", TOY_CHICKEN_WHITE_DEATH_FX);

pub const TOY_FIREHYDRANT_HEALTH: &[u32] = &[250, 500, 800];
pub const TOY_FIREHYDRANT_DESTROYED_STATE: u8 = 3;
pub const TOY_FIREHYDRANT_DEATH_FX: &str = "props/firehydrant_exp";
pub const TOY_FIREHYDRANT_DEATH_SOUND: &str = "firehydrant_burst";
pub const TOY_FIREHYDRANT_LEAK_LOOP_FX: &str = "props/firehydrant_leak";

pub const TOY_FIREHYDRANT: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_FIREHYDRANT_HEALTH,
    destroyed_state: TOY_FIREHYDRANT_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 0, "")),
    cap_fx: None,
    leak_loop_fx: Some(TOY_FIREHYDRANT_LEAK_LOOP_FX),
    death_fx: TOY_FIREHYDRANT_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_FIREHYDRANT_DEATH_SOUND,
    explode_force: (17_000, 18_000),
    explode_range_mp: 96,
    explode_damage: (32, 48),
    explode_origin_offset_z: 0.0,
    husk: "com_firehydrant_dest",
};

pub const TOY_COPIER_HEALTH: &[u32] = &[250, 250, 500, 800];
pub const TOY_COPIER_DESTROYED_STATE: u8 = 4;
pub const TOY_COPIER_DEATH_FX: &str = "props/photocopier_exp";
pub const TOY_COPIER_DEATH_SOUND: &str = "copier_exp";

pub const TOY_COPIER: ToyDestructibleDefinition = ToyDestructibleDefinition {
    health: TOY_COPIER_HEALTH,
    destroyed_state: TOY_COPIER_DESTROYED_STATE,
    health_drain: Some((12, 0.2, 0, "")),
    cap_fx: None,
    leak_loop_fx: None,
    death_fx: TOY_COPIER_DEATH_FX,
    death_fx_tag: Some("tag_fx"),
    death_sound: TOY_COPIER_DEATH_SOUND,
    explode_force: (7_000, 8_000),
    explode_range_mp: 96,
    explode_damage: (32, 48),
    explode_origin_offset_z: 0.0,
    husk: "prop_photocopier_destroyed",
};

pub const TOY_GENERATOR_HEALTH: &[u32] = &[75, 75, 250, 400];
pub const TOY_GENERATOR_ON_HEALTH: &[u32] = &[150, 75, 250, 400];
pub const TOY_GENERATOR_DESTROYED_STATE: u8 = 4;
pub const TOY_GENERATOR_DEATH_FX: &str = "explosions/generator_explosion";
pub const TOY_GENERATOR_DEATH_SOUND: &str = "generator01_explode";

const fn toy_generator(health: &'static [u32]) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health,
        destroyed_state: TOY_GENERATOR_DESTROYED_STATE,
        health_drain: Some((24, 0.2, 64, "allies")),
        cap_fx: None,
        leak_loop_fx: None,
        death_fx: TOY_GENERATOR_DEATH_FX,
        death_fx_tag: Some("tag_fx"),
        death_sound: TOY_GENERATOR_DEATH_SOUND,
        explode_force: (7_000, 8_000),
        explode_range_mp: 128,
        explode_damage: (16, 50),
        explode_origin_offset_z: 0.0,
        husk: "machinery_generator_des",
    }
}

pub const TOY_GENERATOR: ToyDestructibleDefinition = toy_generator(TOY_GENERATOR_HEALTH);
pub const TOY_GENERATOR_ON: ToyDestructibleDefinition = toy_generator(TOY_GENERATOR_ON_HEALTH);

pub const TOY_DT_MIRROR_HEALTH: &[u32] = &[150, 150];
pub const TOY_DT_MIRROR_DESTROYED_STATE: u8 = 2;
pub const TOY_DT_MIRROR_LARGE_DEATH_FX: &str = "props/mirror_dt_panel_large_broken";
pub const TOY_DT_MIRROR_DEATH_FX: &str = "props/mirror_dt_panel_broken";
pub const TOY_DT_MIRROR_DEATH_SOUND: &str = "mirror_shatter";

const fn toy_dt_mirror(husk: &'static str, death_fx: &'static str) -> ToyDestructibleDefinition {
    ToyDestructibleDefinition {
        health: TOY_DT_MIRROR_HEALTH,
        destroyed_state: TOY_DT_MIRROR_DESTROYED_STATE,
        health_drain: None,
        cap_fx: None,
        leak_loop_fx: None,
        death_fx,
        death_fx_tag: Some("tag_fx"),
        death_sound: TOY_DT_MIRROR_DEATH_SOUND,
        explode_force: (1_000, 2_000),
        explode_range_mp: 32,
        explode_damage: (32, 48),
        explode_origin_offset_z: 0.0,
        husk,
    }
}

pub const TOY_DT_MIRROR_LARGE: ToyDestructibleDefinition =
    toy_dt_mirror("dt_mirror_large_des", TOY_DT_MIRROR_LARGE_DEATH_FX);
pub const TOY_DT_MIRROR: ToyDestructibleDefinition =
    toy_dt_mirror("dt_mirror_des", TOY_DT_MIRROR_DEATH_FX);

pub fn destructible_destroyed_state(kind: &str) -> Option<u8> {
    VehicleDestructibleKind::from_mapents(kind)
        .map(VehicleDestructibleKind::destroyed_state)
        .or_else(|| {
            ToyDestructibleKind::from_mapents(kind).map(ToyDestructibleKind::destroyed_state)
        })
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

pub const fn vehicle_policecar_initial_body() -> VehicleBodyState {
    VehicleBodyState {
        state_index: 0,
        health: VEHICLE_POLICECAR.health[0],
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

pub fn apply_vehicle_policecar_player_bullet(
    state: VehicleBodyState,
    damage: u32,
) -> VehicleBodyState {
    apply_vehicle_player_bullet(
        &VEHICLE_POLICECAR,
        VEHICLE_POLICECAR_DESTROYED_STATE,
        state,
        damage,
    )
}

pub fn destructible_death_presentation(kind: &str) -> Option<DestructibleDeathPresentation> {
    let husk = match kind {
        "vehicle_moving_truck" => "vehicle_moving_truck_dst",
        "vehicle_pickup" => "vehicle_pickup_destroyed",
        "vehicle_policecar" => "vehicle_policecar_lapd_destroy",
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
