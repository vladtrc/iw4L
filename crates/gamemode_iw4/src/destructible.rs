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
        self.definition().destroyed_state()
    }

    pub const fn initial_body(self) -> VehicleBodyState {
        VehicleBodyState {
            state_index: 0,
            health: self.definition().initial_health(),
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

/// Splash bias explosive damage carries when a destructible declares no scaler
/// of its own.
pub const MP_EXPLOSIVE_DAMAGE_BIAS: f32 = 13.0;

/// Which damage causes a stage reacts to. A rejected cause leaves the stage
/// untouched: no health comes off and no action fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageCauseFilter {
    Any,
    Splash,
    NotSplash,
}

impl DamageCauseFilter {
    pub const fn accepts(self, splash: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Splash => splash,
            Self::NotSplash => !splash,
        }
    }
}

/// A one-shot effect a stage plays as it is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleFx {
    pub tag: &'static str,
    pub name: &'static str,
    /// `false` fires the effect along [`VEHICLE_DEATH_FX_FORWARD`] from the tag
    /// origin instead of along the tag's own axes.
    pub use_tag_angles: bool,
    pub cause: DamageCauseFilter,
}

impl DestructibleFx {
    pub const fn on(tag: &'static str, name: &'static str) -> Self {
        Self {
            tag,
            name,
            use_tag_angles: true,
            cause: DamageCauseFilter::Any,
        }
    }

    pub const fn flat(mut self) -> Self {
        self.use_tag_angles = false;
        self
    }

    pub const fn splash(mut self) -> Self {
        self.cause = DamageCauseFilter::Splash;
        self
    }

    pub const fn not_splash(mut self) -> Self {
        self.cause = DamageCauseFilter::NotSplash;
        self
    }
}

/// An effect a stage keeps replaying while the destructible sits past it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleLoopFx {
    pub tag: &'static str,
    pub name: &'static str,
    pub interval_ms: u32,
}

impl DestructibleLoopFx {
    pub const fn new(tag: &'static str, name: &'static str, interval_ms: u32) -> Self {
        Self {
            tag,
            name,
            interval_ms,
        }
    }
}

/// A part that leaves the body when the stage is left: cap, valve, drawer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructiblePartThrow {
    pub tag: &'static str,
    pub model: &'static str,
    pub velocity: [i32; 3],
}

impl DestructiblePartThrow {
    pub const fn new(tag: &'static str, model: &'static str, velocity: [i32; 3]) -> Self {
        Self {
            tag,
            model,
            velocity,
        }
    }
}

/// Health the stage bleeds on its own once it is left, until the destructible
/// reaches its last stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructibleHealthDrain {
    pub amount: u32,
    pub interval_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestructibleExplode {
    pub range_mp: u32,
    pub damage: (u32, u32),
    pub origin_offset_z: f32,
}

/// One rung of a destructible's staircase.
///
/// `model` and `health` describe the stage as it is *entered*; every action
/// list describes what happens as it is *left*.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToyStage {
    /// `None` keeps whatever model the previous stages left standing.
    pub model: Option<&'static str>,
    pub health: u32,
    pub cause: DamageCauseFilter,
    pub fx: &'static [DestructibleFx],
    pub sounds: &'static [&'static str],
    pub loop_fx: &'static [DestructibleLoopFx],
    pub loop_sounds: &'static [&'static str],
    pub throws: &'static [DestructiblePartThrow],
    pub health_drain: Option<DestructibleHealthDrain>,
    pub explode: Option<DestructibleExplode>,
}

impl ToyStage {
    pub const fn new(health: u32) -> Self {
        Self {
            model: None,
            health,
            cause: DamageCauseFilter::Any,
            fx: &[],
            sounds: &[],
            loop_fx: &[],
            loop_sounds: &[],
            throws: &[],
            health_drain: None,
            explode: None,
        }
    }

    pub const fn terminal(model: &'static str) -> Self {
        Self::new(0).model(model)
    }

    pub const fn model(mut self, model: &'static str) -> Self {
        self.model = Some(model);
        self
    }

    pub const fn splash_only(mut self) -> Self {
        self.cause = DamageCauseFilter::Splash;
        self
    }

    pub const fn fx(mut self, fx: &'static [DestructibleFx]) -> Self {
        self.fx = fx;
        self
    }

    pub const fn sounds(mut self, sounds: &'static [&'static str]) -> Self {
        self.sounds = sounds;
        self
    }

    pub const fn loop_fx(mut self, loop_fx: &'static [DestructibleLoopFx]) -> Self {
        self.loop_fx = loop_fx;
        self
    }

    pub const fn loop_sounds(mut self, loop_sounds: &'static [&'static str]) -> Self {
        self.loop_sounds = loop_sounds;
        self
    }

    pub const fn throws(mut self, throws: &'static [DestructiblePartThrow]) -> Self {
        self.throws = throws;
        self
    }

    pub const fn drain(mut self, amount: u32, interval_ms: u32) -> Self {
        self.health_drain = Some(DestructibleHealthDrain {
            amount,
            interval_ms,
        });
        self
    }

    pub const fn explode(
        mut self,
        range_mp: u32,
        damage: (u32, u32),
        origin_offset_z: f32,
    ) -> Self {
        self.explode = Some(DestructibleExplode {
            range_mp,
            damage,
            origin_offset_z,
        });
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToyDestructibleDefinition {
    /// Stage 0 is the standing prop; the last stage is the husk.
    pub stages: &'static [ToyStage],
    pub splash_scaler: Option<f32>,
}

impl ToyDestructibleDefinition {
    pub const fn destroyed_state(&self) -> u8 {
        (self.stages.len() - 1) as u8
    }

    pub const fn initial_health(&self) -> u32 {
        self.stages[0].health
    }

    pub const fn splash_damage_scaler(&self) -> f32 {
        match self.splash_scaler {
            Some(scaler) => scaler,
            None => MP_EXPLOSIVE_DAMAGE_BIAS,
        }
    }

    pub fn stage(&self, state: u8) -> &'static ToyStage {
        let last = self.stages.len() - 1;
        &self.stages[(state as usize).min(last)]
    }

    /// Actions fire on the way out of a stage, so entering `state` runs the
    /// list belonging to `state - 1`.
    pub fn left_stage(&self, entered: u8) -> Option<&'static ToyStage> {
        let index = (entered as usize).checked_sub(1)?;
        self.stages.get(index)
    }

    /// The model the prop stands in at `state`: the newest one any stage up to
    /// here declared.
    pub fn stage_model(&self, state: u8) -> Option<&'static str> {
        let last = (state as usize).min(self.stages.len() - 1);
        self.stages[..=last]
            .iter()
            .rev()
            .find_map(|stage| stage.model)
    }

    pub fn husk(&self) -> Option<&'static str> {
        self.stage_model(self.destroyed_state())
    }

    pub fn stage_models(&self) -> impl Iterator<Item = &'static str> {
        let stages = self.stages;
        stages.iter().filter_map(|stage| stage.model)
    }

    /// Leaving a stage cuts every running loop effect when that stage anchors
    /// an effect to a tag of its own -- looping or one-shot -- and otherwise
    /// leaves them burning, past the last stage included. So the newest left
    /// stage that anchors anything decides what is still playing: its own loop
    /// effects, which is nothing at all for a stage that only fires a one-shot.
    pub fn active_loop_fx(&self, state: u8) -> &'static [DestructibleLoopFx] {
        let Some(newest) = (state as usize).checked_sub(1) else {
            return &[];
        };
        let newest = newest.min(self.stages.len() - 1);
        self.stages[..=newest]
            .iter()
            .rev()
            .find(|stage| !stage.fx.is_empty() || !stage.loop_fx.is_empty())
            .map(|stage| stage.loop_fx)
            .unwrap_or(&[])
    }

    /// Loop sounds are cut at every transition and only the stage just left
    /// speaks.
    pub fn active_loop_sounds(&self, state: u8) -> &'static [&'static str] {
        match self.left_stage(state) {
            Some(stage) => stage.loop_sounds,
            None => &[],
        }
    }

    /// Tags whose parts have already been thrown off by `state`.
    pub fn hidden_tags(&self, state: u8) -> impl Iterator<Item = &'static str> {
        let thrown = &self.stages[..(state as usize).min(self.stages.len())];
        thrown
            .iter()
            .flat_map(|stage| stage.throws.iter().map(|throw| throw.tag))
    }
}

/// Every loop a destructible can speak, so a client prepares them with the
/// rest of the match audio instead of at the first hiss.
pub fn destructible_loop_sound_aliases() -> impl Iterator<Item = &'static str> {
    TOY_DESTRUCTIBLE_KINDS
        .iter()
        .flat_map(|kind| kind.definition().stages.iter())
        .flat_map(|stage| stage.loop_sounds.iter().copied())
}

pub const TOY_OXYGEN_TANK_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_cap", "props/oxygen_tank01_cap")])
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "distortion/oxygen_tank_leak",
                400,
            )])
            .loop_sounds(&["oxygen_tank_leak_loop"])
            .drain(12, 200),
        ToyStage::new(300)
            .model("machinery_oxygen_tank01_dam")
            .fx(&[DestructibleFx::on("tag_fx", "explosions/oxygen_tank01_explosion").flat()])
            .sounds(&["oxygen_tank_explode"])
            .explode(256, (16, 150), 32.0),
        ToyStage::terminal("machinery_oxygen_tank01_des"),
    ],
    splash_scaler: None,
};

pub const TOY_OXYGEN_TANK_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_cap", "props/oxygen_tank02_cap")])
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "distortion/oxygen_tank_leak",
                400,
            )])
            .loop_sounds(&["oxygen_tank_leak_loop"])
            .drain(12, 200),
        ToyStage::new(300)
            .model("machinery_oxygen_tank02_dam")
            .fx(&[DestructibleFx::on("tag_fx", "explosions/oxygen_tank02_explosion").flat()])
            .sounds(&["oxygen_tank_explode"])
            .explode(256, (16, 150), 32.0),
        ToyStage::terminal("machinery_oxygen_tank02_des"),
    ],
    splash_scaler: None,
};

pub const TOY_PROPANE_TANK02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(50),
        ToyStage::new(350)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "distortion/propane_cap_distortion",
                100,
            )])
            .loop_sounds(&["propanetank02_gas_leak_loop"]),
        ToyStage::new(350)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "fire/propane_capfire_leak",
                100,
            )])
            .sounds(&["propanetank02_flareup_med"])
            .loop_sounds(&["propanetank02_fire_med"])
            .drain(12, 200),
        ToyStage::new(150)
            .fx(&[
                DestructibleFx::on("tag_valve", "fire/propane_valvefire_flareup"),
                DestructibleFx::on("tag_cap", "fire/propane_capfire_flareup"),
            ])
            .loop_fx(&[
                DestructibleLoopFx::new("tag_cap", "fire/propane_capfire", 600),
                DestructibleLoopFx::new("tag_valve", "fire/propane_valvefire", 100),
            ])
            .sounds(&["propanetank02_flareup2_med"])
            .loop_sounds(&["propanetank02_fire_med"])
            .throws(&[
                DestructiblePartThrow::new("tag_cap", "com_propane_tank02_cap", [50, 0, 0]),
                DestructiblePartThrow::new("tag_valve", "com_propane_tank02_valve", [50, 0, 0]),
            ]),
        ToyStage::new(150)
            .fx(&[
                DestructibleFx::on("tag_fx", "fire/propane_small_fire"),
                DestructibleFx::on("tag_fx", "explosions/propane_large_exp_fireball"),
                DestructibleFx::on("tag_fx", "explosions/propane_large_exp").flat(),
            ])
            .sounds(&["propanetank02_explode"])
            .loop_sounds(&["propanetank02_fire_blown_med"])
            .explode(600, (32, 300), 80.0),
        ToyStage::terminal("com_propane_tank02_des"),
    ],
    splash_scaler: Some(5.0),
};

pub const TOY_PROPANE_TANK02_SMALL: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(50),
        ToyStage::new(350)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "distortion/propane_cap_distortion",
                100,
            )])
            .loop_sounds(&["propanetank02_gas_leak_loop"]),
        ToyStage::new(350)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "fire/propane_capfire_leak",
                100,
            )])
            .sounds(&["propanetank02_flareup_med"])
            .loop_sounds(&["propanetank02_fire_med"])
            .drain(12, 200),
        ToyStage::new(200)
            .fx(&[
                DestructibleFx::on("tag_valve", "fire/propane_valvefire_flareup"),
                DestructibleFx::on("tag_cap", "fire/propane_capfire_flareup"),
            ])
            .loop_fx(&[
                DestructibleLoopFx::new("tag_cap", "fire/propane_capfire", 600),
                DestructibleLoopFx::new("tag_valve", "fire/propane_valvefire", 100),
            ])
            .sounds(&["propanetank02_flareup_med"])
            .loop_sounds(&["propanetank02_fire_med"])
            .throws(&[
                DestructiblePartThrow::new("tag_cap", "com_propane_tank02_small_cap", [50, 0, 0]),
                DestructiblePartThrow::new(
                    "tag_valve",
                    "com_propane_tank02_small_valve",
                    [50, 0, 0],
                ),
            ]),
        ToyStage::new(200)
            .fx(&[
                DestructibleFx::on("tag_fx", "fire/propane_small_fire"),
                DestructibleFx::on("tag_fx", "explosions/propane_large_exp").flat(),
            ])
            .sounds(&["propanetank02_explode"])
            .explode(400, (32, 100), 80.0),
        ToyStage::terminal("com_propane_tank02_small_des"),
    ],
    splash_scaler: Some(10.0),
};

pub const TOY_TUBETV_TV1: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on("tag_fx", "explosions/tv_explosion")])
            .sounds(&["tv_shot_burst"])
            .explode(9, (3, 3), 12.0),
        ToyStage::terminal("com_tv1_d"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_TUBETV_TV2: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on("tag_fx", "explosions/tv_explosion")])
            .sounds(&["tv_shot_burst"])
            .explode(9, (3, 3), 12.0),
        ToyStage::terminal("com_tv2_d"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_FLATSCREEN_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/tv_flatscreen_explosion",
            )])
            .sounds(&["tv_shot_burst"])
            .explode(10, (3, 3), 15.0),
        ToyStage::terminal("ma_flatscreen_tv_broken_01"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_FLATSCREEN_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/tv_flatscreen_explosion",
            )])
            .sounds(&["tv_shot_burst"])
            .explode(10, (3, 3), 15.0),
        ToyStage::terminal("ma_flatscreen_tv_broken_02"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_FLATSCREEN_WALLMOUNT_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/tv_flatscreen_explosion",
            )])
            .sounds(&["tv_shot_burst"])
            .explode(10, (3, 3), 15.0),
        ToyStage::terminal("ma_flatscreen_tv_wallmount_broken_01"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_FLATSCREEN_WALLMOUNT_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(1)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/tv_flatscreen_explosion",
            )])
            .sounds(&["tv_shot_burst"])
            .explode(10, (3, 3), 15.0),
        ToyStage::terminal("ma_flatscreen_tv_wallmount_broken_02"),
    ],
    splash_scaler: Some(1.0),
};

pub const TOY_LIGHT_CEILING_FLUORESCENT: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[
                DestructibleFx::on("tag_fx", "misc/light_fluorescent_blowout_runner"),
                DestructibleFx::on("tag_swing_fx", "misc/light_blowout_swinging_runner"),
            ])
            .sounds(&["fluorescent_light_fall", "fluorescent_light_bulb"])
            .explode(64, (40, 80), 80.0),
        ToyStage::terminal("me_lightfluohang_double_destroyed"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_LIGHT_CEILING_FLUORESCENT_SINGLE: ToyDestructibleDefinition =
    ToyDestructibleDefinition {
        stages: &[
            ToyStage::new(150)
                .fx(&[
                    DestructibleFx::on("tag_fx", "misc/light_fluorescent_single_blowout_runner"),
                    DestructibleFx::on("tag_swing_center_fx", "misc/light_blowout_swinging_runner"),
                    DestructibleFx::on(
                        "tag_swing_center_fx_far",
                        "misc/light_blowout_swinging_runner",
                    ),
                ])
                .sounds(&["fluorescent_light_fall", "fluorescent_light_bulb"])
                .explode(64, (40, 80), 80.0),
            ToyStage::terminal("me_lightfluohang_single_destroyed"),
        ],
        splash_scaler: Some(15.0),
    };

pub const TOY_ELECTRICBOX2: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_fx", "props/electricbox4_explode")])
            .sounds(&["exp_fusebox_sparks"])
            .explode(32, (32, 48), 0.0),
        ToyStage::terminal("me_electricbox2_dest"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_ELECTRICBOX4: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_fx", "props/electricbox4_explode")])
            .sounds(&["exp_fusebox_sparks"])
            .explode(32, (32, 48), 0.0),
        ToyStage::terminal("me_electricbox4_dest"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_AIRCONDITIONER: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0).loop_sounds(&["airconditioner_running_loop"]),
        ToyStage::new(300)
            .model("com_ex_airconditioner")
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/airconditioner_ex_explode",
            )])
            .sounds(&["airconditioner_burst"])
            .explode(32, (32, 48), 0.0),
        ToyStage::terminal("com_ex_airconditioner_dam"),
    ],
    splash_scaler: None,
};

pub const TOY_WALL_FAN: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0).loop_sounds(&["wall_fan_fanning"]),
        ToyStage::new(150)
            .model("cs_wallfan1")
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/wallfan_explosion_dmg",
            )])
            .sounds(&["wall_fan_sparks"]),
        ToyStage::new(150)
            .model("cs_wallfan1")
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/wallfan_explosion_des",
            )])
            .sounds(&["wall_fan_break"]),
        ToyStage::terminal("cs_wallfan1_dmg"),
    ],
    splash_scaler: None,
};

pub const TOY_LOCKER_DOUBLE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "props/locker_double_des_03_both",
            )])
            .sounds(&["lockers_double"]),
        ToyStage::terminal("com_locker_double_destroyed"),
    ],
    splash_scaler: None,
};

pub const TOY_FILECABINET: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[DestructibleFx::on("tag_drawer_lower", "props/filecabinet_dam").not_splash()])
            .sounds(&["exp_filecabinet"]),
        ToyStage::new(20)
            .model("com_filecabinetblackclosed_dam")
            .splash_only()
            .fx(&[DestructibleFx::on("tag_drawer_upper", "props/filecabinet_des").splash()])
            .sounds(&["exp_filecabinet"])
            .throws(&[DestructiblePartThrow::new(
                "tag_drawer_upper",
                "com_filecabinetblackclosed_drawer",
                [50, -10, 5],
            )]),
        ToyStage::terminal("com_filecabinetblackclosed_des"),
    ],
    splash_scaler: None,
};

pub const TOY_GAS_STATION_TRASH_BIN_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[
                DestructibleFx::on("tag_fx", "props/garbage_spew_des").splash(),
                DestructibleFx::on("tag_fx", "props/garbage_spew").not_splash(),
            ])
            .explode(1, (10, 20), 80.0),
        ToyStage::terminal("usa_gas_station_trash_bin_01_base"),
    ],
    splash_scaler: None,
};

pub const TOY_GAS_STATION_TRASH_BIN_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[
                DestructibleFx::on("tag_fx_high", "props/garbage_spew_des").splash(),
                DestructibleFx::on("tag_fx_high", "props/garbage_spew").not_splash(),
            ])
            .explode(1, (10, 20), 80.0),
        ToyStage::terminal("usa_gas_station_trash_bin_02_base"),
    ],
    splash_scaler: None,
};

pub const TOY_CEILING_FAN: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0),
        ToyStage::new(150)
            .model("me_fanceil1")
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/ceiling_fan_explosion",
            )])
            .sounds(&["ceiling_fan_sparks"])
            .explode(32, (5, 32), 0.0),
        ToyStage::terminal("me_fanceil1_des"),
    ],
    splash_scaler: None,
};

pub const TOY_TRASHBIN_01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[
                DestructibleFx::on("tag_fx", "props/garbage_spew_des").splash(),
                DestructibleFx::on("tag_fx", "props/garbage_spew").not_splash(),
            ])
            .sounds(&["exp_trashcan_sweet"])
            .explode(1, (10, 20), 80.0),
        ToyStage::terminal("com_trashbin01_dmg"),
    ],
    splash_scaler: None,
};

pub const TOY_TRASHBIN_02: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[
                DestructibleFx::on("tag_fx", "props/garbage_spew_des").splash(),
                DestructibleFx::on("tag_fx", "props/garbage_spew").not_splash(),
            ])
            .sounds(&["exp_trashcan_sweet"])
            .explode(1, (10, 20), 80.0),
        ToyStage::terminal("com_trashbin02_dmg"),
    ],
    splash_scaler: None,
};

pub const TOY_TRANSFORMER_SMALL01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx",
            "smoke/car_damage_whitesmoke",
            400,
        )]),
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx",
            "smoke/car_damage_blacksmoke",
            400,
        )]),
        ToyStage::new(150)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_fx",
                "explosions/transformer_spark_runner",
                500,
            )])
            .loop_sounds(&["transformer_spark_loop"])
            .drain(24, 200),
        ToyStage::new(250)
            .loop_fx(&[
                DestructibleLoopFx::new("tag_fx", "explosions/transformer_spark_runner", 500),
                DestructibleLoopFx::new("tag_fx", "fire/transformer_small_blacksmoke_fire", 400),
            ])
            .sounds(&["transformer01_flareup_med"])
            .loop_sounds(&["transformer_spark_loop"])
            .drain(24, 200),
        ToyStage::new(400)
            .fx(&[
                DestructibleFx::on("tag_fx", "explosions/transformer_explosion").flat(),
                DestructibleFx::on("tag_fx", "fire/firelp_small_pm"),
            ])
            .sounds(&["transformer01_explode"])
            .explode(256, (16, 100), 0.0),
        ToyStage::terminal("utility_transformer_small01_dest"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_TRANSFORMER_RATNEST01: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx",
            "smoke/car_damage_whitesmoke",
            400,
        )]),
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx",
            "smoke/car_damage_blacksmoke",
            400,
        )]),
        ToyStage::new(150)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_sparks",
                "explosions/transformer_spark_runner",
                500,
            )])
            .loop_sounds(&["transformer_spark_loop"])
            .drain(24, 200),
        ToyStage::new(250)
            .loop_fx(&[
                DestructibleLoopFx::new("tag_sparks", "explosions/transformer_spark_runner", 500),
                DestructibleLoopFx::new("tag_fx", "fire/transformer_blacksmoke_fire", 400),
            ])
            .sounds(&["transformer01_flareup_med"])
            .loop_sounds(&["transformer_spark_loop"])
            .drain(24, 200),
        ToyStage::new(400)
            .fx(&[
                DestructibleFx::on("tag_fx", "explosions/transformer_explosion").flat(),
                DestructibleFx::on("tag_fx", "fire/firelp_small_pm"),
            ])
            .sounds(&["transformer01_explode"])
            .explode(256, (16, 100), 0.0),
        ToyStage::terminal("utility_transformer_ratnest01_dest"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_WATER_COLLECTOR: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(220)
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "explosions/water_collector_explosion",
            )])
            .sounds(&["water_collector_splash"])
            .explode(32, (1, 10), 32.0),
        ToyStage::terminal("utility_water_collector_base_dest"),
    ],
    splash_scaler: None,
};

pub const TOY_NEWSPAPER_STAND_RED: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[DestructibleFx::on("tag_door", "props/news_stand_paper_spill").not_splash()])
            .sounds(&["exp_newspaper_box"])
            .explode(64, (0, 0), 80.0),
        ToyStage::new(20)
            .model("com_newspaperbox_red_dam")
            .splash_only()
            .fx(&[DestructibleFx::on("tag_fx", "props/news_stand_explosion").splash()]),
        ToyStage::terminal("com_newspaperbox_red_des"),
    ],
    splash_scaler: None,
};

pub const TOY_NEWSPAPER_STAND_BLUE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(120)
            .fx(&[
                DestructibleFx::on("tag_door", "props/news_stand_paper_spill_shatter").not_splash(),
            ])
            .sounds(&["exp_newspaper_box"])
            .explode(64, (0, 0), 80.0),
        ToyStage::new(20)
            .model("com_newspaperbox_blue_dam")
            .splash_only()
            .fx(&[DestructibleFx::on("tag_fx", "props/news_stand_explosion").splash()]),
        ToyStage::terminal("com_newspaperbox_blue_des"),
    ],
    splash_scaler: None,
};

pub const TOY_CHICKEN_BLACK_WHITE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0).loop_sounds(&["animal_chicken_idle_loop"]),
        ToyStage::new(25)
            .model("chicken_black_white")
            .fx(&[DestructibleFx::on(
                "tag_origin",
                "props/chicken_exp_black_white",
            )])
            .sounds(&["animal_chicken_death"]),
        ToyStage::terminal("chicken_black_white"),
    ],
    splash_scaler: None,
};

pub const TOY_CHICKEN_WHITE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0).loop_sounds(&["animal_chicken_idle_loop"]),
        ToyStage::new(25)
            .model("chicken_white")
            .fx(&[DestructibleFx::on("tag_origin", "props/chicken_exp_white")])
            .sounds(&["animal_chicken_death"]),
        ToyStage::terminal("chicken_white"),
    ],
    splash_scaler: None,
};

pub const TOY_FIREHYDRANT: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(250),
        ToyStage::new(500)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_cap",
                "props/firehydrant_leak",
                100,
            )])
            .loop_sounds(&["firehydrant_spray_loop"])
            .drain(12, 200),
        ToyStage::new(800)
            .fx(&[
                DestructibleFx::on("tag_fx", "props/firehydrant_exp").flat(),
                DestructibleFx::on("tag_fx", "props/firehydrant_spray_10sec").flat(),
            ])
            .sounds(&["firehydrant_burst"])
            .explode(96, (32, 48), 80.0),
        ToyStage::terminal("com_firehydrant_dest"),
    ],
    splash_scaler: Some(11.0),
};

pub const TOY_COPIER: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(250).loop_fx(&[DestructibleLoopFx::new(
            "tag_left_feeder",
            "smoke/car_damage_whitesmoke",
            400,
        )]),
        ToyStage::new(250).loop_fx(&[DestructibleLoopFx::new(
            "tag_left_feeder",
            "smoke/car_damage_blacksmoke",
            400,
        )]),
        ToyStage::new(500)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_fx",
                "props/photocopier_sparks",
                3_000,
            )])
            .loop_sounds(&["copier_spark_loop"])
            .drain(12, 200),
        ToyStage::new(800)
            .fx(&[
                DestructibleFx::on("tag_fx", "props/photocopier_exp").flat(),
                DestructibleFx::on("tag_fx", "props/photocopier_fire"),
            ])
            .sounds(&["copier_exp"])
            .loop_sounds(&["copier_fire_loop"])
            .explode(96, (32, 48), 80.0),
        ToyStage::terminal("prop_photocopier_destroyed"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_GENERATOR: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx2",
            "smoke/generator_damage_whitesmoke",
            400,
        )]),
        ToyStage::new(75).loop_fx(&[DestructibleLoopFx::new(
            "tag_fx2",
            "smoke/generator_damage_blacksmoke",
            400,
        )]),
        ToyStage::new(250)
            .loop_fx(&[
                DestructibleLoopFx::new("tag_fx2", "smoke/generator_damage_blacksmoke", 400),
                DestructibleLoopFx::new("tag_fx4", "explosions/generator_spark_runner", 900),
                DestructibleLoopFx::new("tag_fx3", "explosions/generator_spark_runner", 612),
            ])
            .loop_sounds(&["generator_spark_loop"])
            .drain(24, 200),
        ToyStage::new(400)
            .fx(&[
                DestructibleFx::on("tag_fx", "explosions/generator_explosion").flat(),
                DestructibleFx::on("tag_fx", "fire/generator_des_fire"),
            ])
            .sounds(&["generator01_explode"])
            .explode(128, (16, 50), 0.0),
        ToyStage::terminal("machinery_generator_des"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_GENERATOR_ON: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(0)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_fx2",
                "smoke/generator_exhaust",
                400,
            )])
            .loop_sounds(&["generator_running"]),
        ToyStage::new(150)
            .model("machinery_generator")
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_fx2",
                "smoke/generator_damage_whitesmoke",
                400,
            )])
            .loop_sounds(&["generator_running"]),
        ToyStage::new(75)
            .loop_fx(&[DestructibleLoopFx::new(
                "tag_fx2",
                "smoke/generator_damage_blacksmoke",
                400,
            )])
            .loop_sounds(&["generator_damage_loop"]),
        ToyStage::new(250)
            .loop_fx(&[
                DestructibleLoopFx::new("tag_fx2", "smoke/generator_damage_blacksmoke", 400),
                DestructibleLoopFx::new("tag_fx4", "explosions/generator_spark_runner", 900),
                DestructibleLoopFx::new("tag_fx3", "explosions/generator_spark_runner", 612),
            ])
            .loop_sounds(&["generator_spark_loop", "generator_damage_loop"])
            .drain(24, 200),
        ToyStage::new(400)
            .fx(&[
                DestructibleFx::on("tag_fx", "explosions/generator_explosion").flat(),
                DestructibleFx::on("tag_fx", "fire/generator_des_fire"),
            ])
            .sounds(&["generator01_explode"])
            .explode(128, (16, 50), 0.0),
        ToyStage::terminal("machinery_generator_des"),
    ],
    splash_scaler: Some(15.0),
};

pub const TOY_DT_MIRROR_LARGE: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_fx", "props/mirror_shatter_large")])
            .sounds(&["mirror_shatter"]),
        ToyStage::new(150)
            .model("dt_mirror_large_dam")
            .fx(&[DestructibleFx::on(
                "tag_fx",
                "props/mirror_dt_panel_large_broken",
            )])
            .explode(32, (32, 48), 0.0),
        ToyStage::terminal("dt_mirror_large_des"),
    ],
    splash_scaler: Some(5.0),
};

pub const TOY_DT_MIRROR: ToyDestructibleDefinition = ToyDestructibleDefinition {
    stages: &[
        ToyStage::new(150)
            .fx(&[DestructibleFx::on("tag_fx", "props/mirror_shatter")])
            .sounds(&["mirror_shatter"]),
        ToyStage::new(150)
            .model("dt_mirror_dam")
            .fx(&[DestructibleFx::on("tag_fx", "props/mirror_dt_panel_broken")])
            .explode(32, (32, 48), 0.0),
        ToyStage::terminal("dt_mirror_des"),
    ],
    splash_scaler: Some(5.0),
};

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

/// Walks a toy down its staircase: each stage that runs out of health hands
/// the leftover damage to the next one. A stage that rejects the damage cause
/// keeps everything, so the prop stalls where the script says it should.
pub fn apply_toy_damage(
    def: &ToyDestructibleDefinition,
    mut state: VehicleBodyState,
    mut damage: u32,
    splash: bool,
) -> VehicleBodyState {
    let destroyed_state = def.destroyed_state();
    if damage == 0 || state.state_index >= destroyed_state {
        return state;
    }

    loop {
        if !def.stage(state.state_index).cause.accepts(splash) {
            return state;
        }
        if damage < state.health {
            state.health -= damage;
            return state;
        }
        damage -= state.health;
        state.state_index += 1;
        if state.state_index >= destroyed_state {
            return VehicleBodyState {
                state_index: destroyed_state,
                health: 0,
            };
        }
        state.health = def.stage(state.state_index).health;
    }
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
