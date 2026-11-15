use std::path::PathBuf;

use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeRole {
    Listen,

    Dedicated,

    Client,

    Replay,
}

impl RuntimeRole {
    pub const fn runs_authority(self) -> bool {
        matches!(self, Self::Listen | Self::Dedicated)
    }

    pub const fn runs_client(self) -> bool {
        matches!(self, Self::Listen | Self::Client | Self::Replay)
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Headless;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppScreen {
    #[default]
    MainMenu,
    Loading,

    ClassSelect,
    InGame,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct LoadCinematicPlaying;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct GamePaused;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDraw(pub bool);

impl Default for UiDraw {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct UiPointerFree(pub bool);

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct UiOverlayInput(pub bool);

#[derive(Resource, Debug, Clone, Copy)]
pub struct HudDraw(pub bool);

impl Default for HudDraw {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct LaunchIdentity {
    pub role_label: String,
    pub games_root: PathBuf,
    pub artifacts: PathBuf,
    pub zone: String,
}

#[derive(Resource, Clone, Debug)]
pub struct LaunchReport {
    pub zone: String,
    pub common_mp: Result<PathBuf, String>,
    pub zone_ff: Result<PathBuf, String>,

    pub zone_alias: Option<String>,
    pub sim_gap: &'static str,

    pub prediction_metrics: Option<String>,
    pub world_report: Vec<String>,
}

impl Default for LaunchReport {
    fn default() -> Self {
        Self {
            zone: String::new(),
            common_mp: Err("not requested".into()),
            zone_ff: Err("not requested".into()),
            zone_alias: None,
            sim_gap: "no map load yet",
            prediction_metrics: None,
            world_report: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacWeaponOffer {
    pub key: String,
    pub item_group: Option<String>,

    pub attachments: Vec<String>,
}

impl From<String> for CacWeaponOffer {
    fn from(key: String) -> Self {
        Self {
            key,
            item_group: None,
            attachments: Vec::new(),
        }
    }
}

impl From<&str> for CacWeaponOffer {
    fn from(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            item_group: None,
            attachments: Vec::new(),
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ClassSelectHandoff {
    pub pending: bool,
    pub primary: Vec<CacWeaponOffer>,
    pub secondary: Vec<CacWeaponOffer>,
    pub lethal: Vec<CacWeaponOffer>,
    pub tactical: Vec<CacWeaponOffer>,
    pub excluded: Vec<(String, String)>,
    pub lock_reasons: Vec<Option<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorldStamp(pub Option<u64>);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldGeneration {
    id: std::num::NonZeroU64,
    stamp: WorldStamp,
}
impl WorldGeneration {
    pub fn from_install(request_id: u64) -> Self {
        Self {
            id: std::num::NonZeroU64::new(
                request_id
                    .checked_add(1)
                    .expect("match generation exhausted"),
            )
            .unwrap(),
            stamp: WorldStamp(Some(request_id)),
        }
    }
    pub fn id(self) -> std::num::NonZeroU64 {
        self.id
    }
    pub fn stamp(self) -> WorldStamp {
        self.stamp
    }
}
impl std::ops::Deref for WorldGeneration {
    type Target = WorldStamp;
    fn deref(&self) -> &WorldStamp {
        &self.stamp
    }
}

impl WorldStamp {
    pub fn stamp(self) -> Self {
        self
    }
    pub fn from_install(request_id: u64) -> Self {
        Self(Some(request_id))
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorldProducts(pub Option<u64>);

impl WorldProducts {
    pub fn from_walk(products_id: u64) -> Self {
        Self((products_id != 0).then_some(products_id))
    }

    pub fn same_as(self, other: Self) -> bool {
        self.0.is_some() && self == other
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MatchKey {
    pub session_id: [u8; 16],
    pub match_epoch: u32,
}

impl MatchKey {
    pub const NONE: Self = Self {
        session_id: [0; 16],
        match_epoch: 0,
    };

    pub fn new(session_id: [u8; 16], match_epoch: u32) -> Self {
        Self {
            session_id,
            match_epoch,
        }
    }

    pub fn is_none(self) -> bool {
        self.match_epoch == 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LocalLoadKey {
    pub incarnation: u64,
    pub match_key: MatchKey,
    pub local_load_request_id: u64,
}

impl LocalLoadKey {
    pub fn from_request(request_id: u64, match_key: MatchKey, incarnation: u64) -> Self {
        Self {
            incarnation,
            match_key,
            local_load_request_id: request_id,
        }
    }

    pub fn belongs_to(self, match_key: MatchKey) -> bool {
        !match_key.is_none() && self.match_key == match_key
    }

    pub fn matches_completion(self, other: Self) -> bool {
        self == other
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PresentationContext {
    pub installation: LocalLoadKey,
    pub timeline: u64,
}

impl PresentationContext {
    pub fn new(installation: LocalLoadKey, timeline: u64) -> Self {
        Self {
            installation,
            timeline,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AdmissionKey {
    pub match_key: MatchKey,
    pub member_id: [u8; 16],
    pub connection_id: u64,
    pub bootstrap_id: u32,
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct InstalledMatch {
    pub request_id: u64,

    pub load_key: LocalLoadKey,

    pub zone: String,

    pub spawn_count: usize,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct MapLoadApproved {
    pub request_id: u64,
    pub load_key: LocalLoadKey,
    pub zone: String,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct MapLoadFailed {
    pub request_id: u64,
    pub load_key: LocalLoadKey,
    pub zone: String,
    pub error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeardownReason {
    Disconnect,

    Replaced,

    MatchEnded,
}

impl TeardownReason {
    pub fn keeps_session(self) -> bool {
        matches!(self, TeardownReason::Replaced | TeardownReason::MatchEnded)
    }

    pub fn label(self) -> &'static str {
        match self {
            TeardownReason::Disconnect => "Disconnect",
            TeardownReason::Replaced => "Replaced",
            TeardownReason::MatchEnded => "MatchEnded",
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifeStarted {
    pub client: u32,
    pub life: u32,
    pub reason: LifeStartReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeStartReason {
    BecameAlive,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifeEnded {
    pub client: u32,
    pub life: u32,
    pub cause: LifeEndCause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeEndCause {
    LeftAlive,

    Replaced,

    Dropped,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewSubject {
    #[default]
    None,
    Own {
        client: u32,
    },
    Seat {
        viewer: u32,
        focus: Option<u32>,
    },
}

impl ViewSubject {
    pub fn in_killcam(self) -> bool {
        matches!(self, Self::Seat { .. })
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct HostClassLoadouts {
    pub slots: Vec<HostClassSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostClassSlot {
    pub name: String,
    pub primary: String,
    pub primary_attachments: Vec<String>,
    pub secondary: String,
    pub secondary_attachments: Vec<String>,
    pub lethal: String,
    pub tactical: String,
    pub perks: [String; 3],

    pub deathstreak: String,
    pub camos: [String; 2],
}

impl Default for HostClassLoadouts {
    fn default() -> Self {
        Self {
            slots: crate::showcase_classes()
                .iter()
                .take(5)
                .map(HostClassSlot::from)
                .collect(),
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct HudInputView {
    pub use_key: Option<String>,
    pub binding_keys: std::collections::BTreeMap<String, String>,
    pub console_open: bool,
    pub script_menu_open: bool,
    pub action_slot_keys: [Option<String>; 4],
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalSpawnArmed(pub WorldStamp);

impl LocalSpawnArmed {
    pub fn armed_for(self, generation: WorldStamp) -> bool {
        generation.0.is_some() && self.0 == generation
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadinessState {
    #[default]
    Pending,
    Ready,
    Silent,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldReadiness {
    pub generation: WorldStamp,
    pub state: ReadinessState,
}

impl WorldReadiness {
    pub fn new(generation: WorldStamp, state: ReadinessState) -> Self {
        Self { generation, state }
    }

    pub fn ready_for(self, generation: WorldStamp) -> bool {
        generation.0.is_some()
            && self.generation == generation
            && matches!(self.state, ReadinessState::Ready | ReadinessState::Silent)
    }

    pub fn failed_for(self, generation: WorldStamp) -> bool {
        generation.0.is_some()
            && self.generation == generation
            && self.state == ReadinessState::Failed
    }
}

/// Authority navigation is prepared while the loading screen is still active.
#[derive(bevy::prelude::Resource, Default)]
pub struct BotNavigationReady(pub WorldReadiness);

#[derive(Component)]
pub struct UiCamera;

#[derive(Component, Clone, bevy::render::extract_component::ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub struct DisplayEncodedCamera;
