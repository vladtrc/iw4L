use crate::identities::{DamageSource, EventSequence, LifeSequence, MatchPhase};
use crate::input::{ActionRequestId, ClassId};
use crate::world::{ClientId, Tick};
use crate::world_objects::WorldObjectSnapshot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClientLifecycle {
    #[default]
    Connecting,
    ChoosingClass,

    SpawnPending,
    Alive,
    Dead,

    RespawnPending,
    Spectating,

    Intermission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassRejectReason {
    UnknownOrStaleClass,

    NoSpawnAvailable,

    LockedContent,

    UnknownWeaponId,
}

impl ClassRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownOrStaleClass => "unknown_or_stale_class",
            Self::NoSpawnAvailable => "no_spawn_available",
            Self::LockedContent => "locked_content",
            Self::UnknownWeaponId => "unknown_weapon_id",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveRejectReason {
    NotAlive,

    InvalidWeapon,

    UnknownWeaponId,

    UnsupportedWeapon,

    EmptyCombatProfile,
}

impl GiveRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAlive => "not_alive",
            Self::InvalidWeapon => "invalid_weapon",
            Self::UnknownWeaponId => "unknown_weapon_id",
            Self::UnsupportedWeapon => "unsupported_weapon",
            Self::EmptyCombatProfile => "empty_combat_profile",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationChangeRejectReason {
    NotAlive,
    StaleSource,
    InvalidTarget,
    DifferentFamily,
    Busy,
    NoInventorySlot,
    AmmoTableFull,
    SharedAmmoConflict,
}

impl ConfigurationChangeRejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAlive => "not_alive",
            Self::StaleSource => "stale_source",
            Self::InvalidTarget => "invalid_target",
            Self::DifferentFamily => "different_family",
            Self::Busy => "busy",
            Self::NoInventorySlot => "no_inventory_slot",
            Self::AmmoTableFull => "ammo_table_full",
            Self::SharedAmmoConflict => "shared_ammo_conflict",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadoutSpec {
    pub class_id: ClassId,
    pub revision: u32,
    pub primary: u32,
    pub secondary: u32,
    pub primary_attachments: [u32; 4],
    pub secondary_attachments: [u32; 4],
    pub lethal: u32,
    pub tactical: u32,

    pub perks: [u32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventAudience {
    All,
    AllExcept(ClientId),
    Client(ClientId),

    Clients(Vec<ClientId>),
}

impl EventAudience {
    pub fn projects_to(&self, id: ClientId) -> bool {
        match self {
            Self::All => true,
            Self::AllExcept(excluded) => *excluded != id,
            Self::Client(c) => *c == id,
            Self::Clients(cs) => cs.iter().any(|c| *c == id),
        }
    }

    pub fn pair(a: ClientId, b: ClientId) -> Self {
        if a == b {
            Self::Client(a)
        } else {
            Self::Clients(vec![a, b])
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub sequence: EventSequence,
    pub tick: Tick,
    pub audience: EventAudience,
    pub event: SimEvent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityEventPayload {
    pub number: i32,
    pub other_entity_num: i32,
    pub attacker_entity_num: i32,
    pub event_parm: i32,
    pub weapon: u32,

    pub correlation: u32,

    pub pellet: u16,
    pub hand: u8,
    pub origin: [f32; 3],
    pub origin2: [f32; 3],
    pub direction: [f32; 3],
    pub surf_type: u8,

    pub surface_flags: u32,
    pub simulation_flags: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityEventRecord {
    pub sequence: EventSequence,
    pub tick: Tick,
    pub audience: EventAudience,
    pub event: entity_iw4::EntityEventKind,
    pub payload: EntityEventPayload,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PelletFxRecord {
    pub attacker: i32,
    pub weapon: u32,

    pub correlation: u32,

    pub pellet: u16,

    pub hand: u8,

    pub start: [f32; 3],

    pub end: [f32; 3],

    pub normal: [f32; 3],
    pub surf_type: u8,

    pub surface_flags: u32,

    pub flesh_flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimEvent {
    ClassAccepted {
        request_id: ActionRequestId,
        class_id: ClassId,
        revision: u32,
    },
    ClassRejected {
        request_id: ActionRequestId,
        class_id: ClassId,
        revision: u32,
        reason: ClassRejectReason,
    },

    GiveAccepted {
        request_id: ActionRequestId,
        weapon: u32,
    },

    GiveRejected {
        request_id: ActionRequestId,
        weapon: u32,
        reason: GiveRejectReason,
    },

    ConfigurationChangeAccepted {
        request_id: ActionRequestId,
        from: u32,
        to: u32,
    },

    ConfigurationChangeRejected {
        request_id: ActionRequestId,
        from: u32,
        to: u32,
        reason: ConfigurationChangeRejectReason,
    },

    Spawned {
        class_id: ClassId,
        spawn_index: u32,
        life_sequence: LifeSequence,
    },

    Died {
        victim: ClientId,
        life_sequence: LifeSequence,
        attacker: Option<ClientId>,
        attacker_life: Option<LifeSequence>,
        source: Option<DamageSource>,

        weapon: u32,

        killcam_entity_start_time: i32,
    },

    ScoreChanged {
        client: ClientId,
        score: i32,
        kills: i32,
        deaths: i32,
    },

    MatchEnded {
        reason: MatchEndReason,
    },

    AttackReleased,

    WeaponSwitchRequested {
        weapon: u32,
    },
}

pub const UNRELIABLE_SIM_EVENT_COUNT: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimEventRow {
    pub variant: &'static str,

    pub reliable: bool,

    pub control_fact: &'static str,
}

pub const SIM_EVENT_ROSTER: &[SimEventRow] = &[
    SimEventRow {
        variant: "ClassAccepted",
        reliable: true,
        control_fact: "authority's verdict on a class request, carrying the revision the client \
                       must echo; the client proposed it and cannot know the answer",
    },
    SimEventRow {
        variant: "ClassRejected",
        reliable: true,
        control_fact: "the refusal and its reason; without it a client waits forever on a \
                       transaction authority already closed",
    },
    SimEventRow {
        variant: "GiveAccepted",
        reliable: true,
        control_fact: "console/dev give verdict — the held weapon index authority settled on",
    },
    SimEventRow {
        variant: "GiveRejected",
        reliable: true,
        control_fact: "the refusal and its reason, so the console prints why rather than nothing",
    },
    SimEventRow {
        variant: "ConfigurationChangeAccepted",
        reliable: true,
        control_fact: "authority accepted the held weapon's configuration transition",
    },
    SimEventRow {
        variant: "ConfigurationChangeRejected",
        reliable: true,
        control_fact: "authority refused a stale or invalid configuration transition",
    },
    SimEventRow {
        variant: "Spawned",
        reliable: true,
        control_fact: "which authored spawn authority picked and the life sequence it opened; \
                       spawn selection is authority-only state",
    },
    SimEventRow {
        variant: "Died",
        reliable: true,
        control_fact: "the life sequence ending and its cause — attacker, source and killcam \
                       entity birthtime, none of which a client can derive from its own \
                       simulation. The obituary a player *sees* is EV_OBITUARY, not this",
    },
    SimEventRow {
        variant: "ScoreChanged",
        reliable: true,
        control_fact: "the scoreboard row after authority applied it; score is authority state \
                       with no predicted counterpart",
    },
    SimEventRow {
        variant: "MatchEnded",
        reliable: true,
        control_fact: "the phase flip and which limit caused it; the client holds neither the \
                       authoritative clock nor every player's score",
    },
    SimEventRow {
        variant: "AttackReleased",
        reliable: false,
        control_fact: "authority's view of the ATTACK button for remote proxies, whose usercmds \
                       this client never sees. For the local player it is redundant with the \
                       client's own input, which is why it is the one unreliable variant",
    },
    SimEventRow {
        variant: "WeaponSwitchRequested",
        reliable: true,
        control_fact: "authority asks the client to select a weapon; the client owns weapon \
                       selection, so a server-side switch must go through its usercmd to \
                       play the drop and raise",
    },
];

pub fn sim_event_is_reliable(event: &SimEvent) -> bool {
    let variant = match event {
        SimEvent::ClassAccepted { .. } => "ClassAccepted",
        SimEvent::ClassRejected { .. } => "ClassRejected",
        SimEvent::GiveAccepted { .. } => "GiveAccepted",
        SimEvent::GiveRejected { .. } => "GiveRejected",
        SimEvent::ConfigurationChangeAccepted { .. } => "ConfigurationChangeAccepted",
        SimEvent::ConfigurationChangeRejected { .. } => "ConfigurationChangeRejected",
        SimEvent::Spawned { .. } => "Spawned",
        SimEvent::Died { .. } => "Died",
        SimEvent::ScoreChanged { .. } => "ScoreChanged",
        SimEvent::MatchEnded { .. } => "MatchEnded",
        SimEvent::AttackReleased => "AttackReleased",
        SimEvent::WeaponSwitchRequested { .. } => "WeaponSwitchRequested",
    };
    SIM_EVENT_ROSTER
        .iter()
        .find(|row| row.variant == variant)
        .is_some_and(|row| row.reliable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndReason {
    ScoreLimit,
    TimeLimit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptSeat {
    pub spectator_client: i32,
    pub kill_cam_entity: i32,
    pub look_at_entity: i32,
    pub archive_ms: i32,
    pub ps_offset_ms: i32,
    pub length_ms: i32,
}

impl Default for ScriptSeat {
    fn default() -> Self {
        Self {
            spectator_client: -1,
            kill_cam_entity: -1,
            look_at_entity: -1,
            archive_ms: 0,
            ps_offset_ms: 0,
            length_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamHud {
    pub final_kill: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientSnapshotMeta {
    pub weapon_lock: crate::WeaponLock,
    pub killcam_hud: Option<KillcamHud>,
    pub lifecycle: ClientLifecycle,
    pub loadout: Option<LoadoutSpec>,
    pub life_sequence: LifeSequence,

    pub item_use_spawn_ms: i32,
    pub item_use_entity: Option<crate::EntityRef>,

    pub ammo_clip: i32,

    pub ammo_stock: i32,

    pub score: i32,
    pub kills: i32,
    pub deaths: i32,
    pub kill_streak: i32,
    pub radar: RadarMode,
    pub remote_missile: Option<RemoteMissile>,

    pub ammo_by_weapon: Vec<(u32, i32, i32)>,

    pub taped_mag_spent: Vec<u32>,

    pub weapon_shot_count: u8,
    pub burst_latch: bool,
    pub rechamber_pending: bool,

    pub dead_since_tick: Option<u32>,

    pub look_at_killer_yaw: i32,

    pub name: [u8; 16],

    pub hud_archival: Vec<hud_iw4::HudElem>,

    pub hud_current: Vec<hud_iw4::HudElem>,

    pub ffa_team: Option<u8>,

    pub client_state_team: i32,

    pub rank: i32,

    pub prestige: i32,

    pub player_card_icon: u32,

    pub player_card_title: u32,

    pub player_card_nameplate: u32,

    pub client_dvars: Vec<(String, String)>,

    pub shellshock: Option<hud_iw4::ShockParams>,

    pub menu_commands: Vec<MenuCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuCommand {
    pub serial: u32,
    pub kind: MenuCommandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuCommandKind {
    Open(String),
    ClosePopup,
    CloseInGame,
}

pub const MENU_COMMAND_TAIL: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapshotMeta {
    pub objectives: crate::ObjectiveMatch,
    pub phase: MatchPhase,

    pub match_elapsed_ms: u32,
    pub prematch: gamemode_iw4::PrematchStep,

    pub score_limit: i32,

    pub time_limit_ms: u32,

    pub kind: gamemode_iw4::GameModeKind,
    pub clients: Vec<(ClientId, ClientSnapshotMeta)>,

    pub journal: Vec<EventRecord>,

    pub entity_events: Vec<EntityEventRecord>,

    pub pellet_fx: Vec<PelletFxRecord>,

    pub sound_aliases: crate::SoundAliasCsOccupied,

    pub effect_names: crate::EffectNameCsOccupied,

    pub hud_materials: crate::HudMaterialCsOccupied,

    pub hud_strings: crate::HudStringCsOccupied,

    pub rng: RngDebugMeta,

    pub world_objects: WorldObjectSnapshot,

    pub area_entities: Option<crate::AreaEntityWorldSnapshot>,

    pub entity_dobjs: Vec<(
        crate::AuthorityModelOwner,
        xmodel_runtime::DObjSemanticState,
    )>,

    pub entities: Vec<entity_iw4::EntityState>,

    pub script_movers: Vec<crate::ScriptMoverGentity>,

    pub entity_kernel: crate::EntityKernelSnapshot,

    pub corpses: crate::PlayerCorpsePool,

    pub item_ammo: Vec<DroppedItemAmmo>,

    pub item_pickups: Vec<ItemPickupRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadarMode {
    #[default]
    Off,
    Normal,
    Fast,
    Constant,
}

impl RadarMode {
    pub fn wire_tag(self) -> u8 {
        self as u8
    }

    pub fn from_wire_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Off,
            1 => Self::Normal,
            2 => Self::Fast,
            3 => Self::Constant,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RemoteMissile {
    pub projectile: crate::ProjectileId,
    pub entnum: i32,
    pub angles: [f32; 3],
    pub armed: bool,
    pub boosted: bool,
    pub attack: bool,
    pub unlink_at_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DroppedItemAmmo {
    pub entnum: i32,
    pub clip_r: i32,
    pub clip_l: i32,
    pub stock: i32,
    pub scavenger: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemPickupRecord {
    pub picker: i32,
    pub weapon: u32,
    pub from_entnum: i32,
    pub clip_r: i32,
    pub clip_l: i32,
    pub stock: i32,
    pub swapped_entnum: i32,
    pub picker_pm_type: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RngDebugMeta {
    pub root_seed: u64,
    pub scheme: u32,
    pub spawn_draws: u64,
    pub combat_draws: u64,
    pub bot_draws: u64,
}

impl SnapshotMeta {
    pub fn for_client(&self, id: ClientId) -> Option<&ClientSnapshotMeta> {
        self.clients.iter().find(|(c, _)| *c == id).map(|(_, m)| m)
    }

    pub fn events_for(&self, id: ClientId) -> impl Iterator<Item = &SimEvent> {
        self.journal
            .iter()
            .filter(move |record| record.audience.projects_to(id))
            .map(|record| &record.event)
    }

    pub fn script_dvars(&self, id: ClientId) -> ScriptDvars<'_> {
        ScriptDvars {
            client: self
                .for_client(id)
                .map_or(&[], |m| m.client_dvars.as_slice()),
            server: &self.objectives.server_info,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptDvars<'a> {
    client: &'a [(String, String)],
    server: &'a [(String, String)],
}

impl<'a> ScriptDvars<'a> {
    pub fn string(&self, name: &str) -> Option<&'a str> {
        self.client
            .iter()
            .chain(self.server)
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        let value = self.string(name)?.trim();
        value
            .parse::<i32>()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|v| v as i32))
    }

    pub fn text(&self, name: &str, localize: impl Fn(&str) -> Option<String>) -> Option<String> {
        let mut parts = self.string(name)?.split(crate::HUD_PRINT_ARG_SEPARATOR);
        let head = parts.next().unwrap_or_default();
        let Some(key) = head.strip_prefix('@') else {
            return Some(head.to_owned());
        };
        let template = localize(key)?;
        Some(parts.enumerate().fold(template, |line, (i, arg)| {
            line.replace(&format!("&&{}", i + 1), arg)
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthRegenCensus {
    pub old_health: i32,
    pub hurt_time_ms: i32,
    pub very_hurt: bool,

    pub named_sound: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputReceipt {
    pub applied_cmds: u32,
    pub moving_cmds: u32,
    pub path_units: f32,
}

impl InputReceipt {
    pub(crate) fn record(&mut self, moving: bool, from: [f32; 3], to: [f32; 3]) {
        self.applied_cmds = self.applied_cmds.wrapping_add(1);
        if moving {
            self.moving_cmds = self.moving_cmds.wrapping_add(1);
        }
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let step = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if step.is_finite() {
            self.path_units += step;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientMatchState {
    pub weapon_lock: crate::WeaponLock,
    pub lifecycle: ClientLifecycle,
    pub input_receipt: InputReceipt,
    pub loadout: Option<LoadoutSpec>,
    pub life_sequence: LifeSequence,

    pub item_use_spawn_ms: i32,
    pub item_use_entity: Option<crate::EntityRef>,

    pub(crate) ammo_clip: i32,

    pub(crate) ammo_stock: i32,

    pub(crate) ammo_by_weapon: Vec<(u32, i32, i32)>,

    pub(crate) taped_mag_spent: Vec<u32>,

    pub(crate) weapon_shot_count: u8,
    pub(crate) burst_latch: bool,

    pub(crate) rechamber_pending: bool,

    pub(crate) dead_since_tick: Option<u32>,

    pub(crate) forced_spawn: Option<crate::SpawnPick>,

    pub(crate) look_at_killer_yaw: i32,

    pub(crate) name: [u8; 16],

    pub kills: i32,
    pub deaths: i32,
    pub score: i32,
    pub kill_streak: i32,
    pub radar: RadarMode,
    pub remote_missile: Option<RemoteMissile>,

    pub(crate) ffa_team: Option<u8>,

    pub(crate) client_state_team: i32,

    pub(crate) rank: i32,
    pub(crate) prestige: i32,
    pub(crate) player_card_icon: u32,
    pub(crate) player_card_title: u32,
    pub(crate) player_card_nameplate: u32,
    pub(crate) client_dvars: Vec<(String, String)>,
    pub(crate) shellshock: Option<hud_iw4::ShockParams>,
    pub(crate) menu_commands: Vec<MenuCommand>,

    pub(crate) controls: ScriptControls,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ScriptControls {
    pub frozen: bool,
    pub weapons_disabled: bool,
    pub offhands_disabled: bool,
    pub switch_disabled: bool,
    pub jump_disabled: bool,
    pub usability_disabled: bool,
    pub linked: bool,
    pub switch_to: u32,
}

impl ClientMatchState {
    pub(crate) fn ammo_for(&self, weapon: u32) -> (i32, i32) {
        self.ammo_by_weapon
            .iter()
            .find(|(w, _, _)| *w == weapon)
            .map(|(_, c, s)| (*c, *s))
            .unwrap_or((0, 0))
    }

    pub(crate) fn set_ammo(&mut self, weapon: u32, clip: i32, stock: i32) {
        if weapon == 0 {
            return;
        }
        if let Some(row) = self
            .ammo_by_weapon
            .iter_mut()
            .find(|(w, _, _)| *w == weapon)
        {
            row.1 = clip;
            row.2 = stock;
            return;
        }
        self.ammo_by_weapon.push((weapon, clip, stock));
    }

    pub(crate) fn quick_reload_ready(&self, weapon: u32) -> bool {
        !self.taped_mag_spent.contains(&weapon)
    }

    pub(crate) fn set_quick_reload_ready(&mut self, weapon: u32, ready: bool) {
        self.taped_mag_spent.retain(|&w| w != weapon);
        if !ready && weapon != 0 {
            self.taped_mag_spent.push(weapon);
        }
    }

    pub(crate) fn clear_ammo_inventory(&mut self) {
        self.ammo_by_weapon.clear();
        self.taped_mag_spent.clear();
        self.ammo_clip = 0;
        self.ammo_stock = 0;
    }

    pub(crate) fn mirror_held_ammo(&mut self, weapon: u32) {
        let (clip, stock) = self.ammo_for(weapon);
        self.ammo_clip = clip;
        self.ammo_stock = stock;
    }

    pub(crate) fn to_snapshot_meta(&self) -> ClientSnapshotMeta {
        ClientSnapshotMeta {
            weapon_lock: self.weapon_lock,
            killcam_hud: None,
            lifecycle: self.lifecycle,
            loadout: self.loadout.clone(),
            life_sequence: self.life_sequence,
            item_use_spawn_ms: self.item_use_spawn_ms,
            item_use_entity: self.item_use_entity,
            ammo_clip: self.ammo_clip,
            ammo_stock: self.ammo_stock,
            score: self.score,
            kills: self.kills,
            deaths: self.deaths,
            kill_streak: self.kill_streak,
            radar: self.radar,
            remote_missile: self.remote_missile,
            ammo_by_weapon: self.ammo_by_weapon.clone(),
            taped_mag_spent: self.taped_mag_spent.clone(),
            weapon_shot_count: self.weapon_shot_count,
            burst_latch: self.burst_latch,
            rechamber_pending: self.rechamber_pending,
            dead_since_tick: self.dead_since_tick,
            look_at_killer_yaw: self.look_at_killer_yaw,
            name: self.name,
            hud_archival: Vec::new(),
            hud_current: Vec::new(),
            ffa_team: self.ffa_team,
            client_state_team: self.client_state_team,
            rank: self.rank,
            prestige: self.prestige,
            player_card_icon: self.player_card_icon,
            player_card_title: self.player_card_title,
            player_card_nameplate: self.player_card_nameplate,
            client_dvars: self.client_dvars.clone(),
            shellshock: self.shellshock.clone(),
            menu_commands: self.menu_commands.clone(),
        }
    }

    pub(crate) fn push_menu_command(&mut self, kind: MenuCommandKind) {
        let serial = self
            .menu_commands
            .last()
            .map_or(1, |c| c.serial.wrapping_add(1));
        self.menu_commands.push(MenuCommand { serial, kind });
        let excess = self.menu_commands.len().saturating_sub(MENU_COMMAND_TAIL);
        self.menu_commands.drain(..excess);
    }

    pub(crate) fn adopt_snapshot_meta(&mut self, meta: &ClientSnapshotMeta) {
        self.weapon_lock = meta.weapon_lock;
        self.lifecycle = meta.lifecycle;
        self.loadout = meta.loadout.clone();
        self.life_sequence = meta.life_sequence;
        self.item_use_spawn_ms = meta.item_use_spawn_ms;
        self.item_use_entity = meta.item_use_entity;
        self.ammo_clip = meta.ammo_clip;
        self.ammo_stock = meta.ammo_stock;
        self.score = meta.score;
        self.kills = meta.kills;
        self.deaths = meta.deaths;
        self.kill_streak = meta.kill_streak;
        self.radar = meta.radar;
        self.remote_missile = meta.remote_missile;
        self.ammo_by_weapon = meta.ammo_by_weapon.clone();
        self.taped_mag_spent = meta.taped_mag_spent.clone();
        self.weapon_shot_count = meta.weapon_shot_count;
        self.burst_latch = meta.burst_latch;
        self.rechamber_pending = meta.rechamber_pending;
        self.dead_since_tick = meta.dead_since_tick;
        self.look_at_killer_yaw = meta.look_at_killer_yaw;
        self.name = meta.name;
        self.ffa_team = meta.ffa_team;
        self.client_state_team = meta.client_state_team;
        self.rank = meta.rank;
        self.prestige = meta.prestige;
        self.player_card_icon = meta.player_card_icon;
        self.player_card_title = meta.player_card_title;
        self.player_card_nameplate = meta.player_card_nameplate;
        self.client_dvars = meta.client_dvars.clone();
        self.shellshock = meta.shellshock.clone();
        self.menu_commands = meta.menu_commands.clone();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub id: ClassId,
    pub revision: u32,
    pub primary: u32,
    pub secondary: u32,

    pub primary_attachments: [u32; 4],
    pub secondary_attachments: [u32; 4],
    pub lethal: u32,
    pub tactical: u32,
    pub perks: [u32; 3],

    pub deathstreak: String,
    pub locked: bool,
}

impl ClassDef {
    pub fn primary_secondary(id: ClassId, revision: u32, primary: u32, secondary: u32) -> Self {
        Self {
            id,
            revision,
            primary,
            secondary,
            primary_attachments: [0; 4],
            secondary_attachments: [0; 4],
            lethal: 0,
            tactical: 0,
            perks: [0; 3],
            deathstreak: String::new(),
            locked: false,
        }
    }

    pub fn weapon_slot_ids(&self) -> [u32; 4] {
        [self.primary, self.secondary, self.lethal, self.tactical]
    }
}

pub const CLASS_CATALOG_PERKS: [&str; 16] = [
    "specialty_bulletdamage",
    "specialty_fastreload",
    "specialty_coldblooded",
    "specialty_lightweight",
    "specialty_scavenger",
    "specialty_hardline",
    "specialty_heartbreaker",
    "specialty_marathon",
    "specialty_explosivedamage",
    "specialty_extendedmelee",
    "specialty_bulletaccuracy",
    "specialty_bling",
    "specialty_onemanarmy",
    "specialty_localjammer",
    "specialty_detectexplosive",
    "specialty_pistoldeath",
];

#[must_use]
pub fn class_catalog_perk_name(id: u32) -> Option<&'static str> {
    CLASS_CATALOG_PERKS
        .get(id.checked_sub(1)? as usize)
        .copied()
}

pub const CLASS_CATALOG_STOPPING_POWER: u32 = 1;
pub const CLASS_CATALOG_SLEIGHT_OF_HAND: u32 = 2;
pub const CLASS_CATALOG_COLD_BLOODED: u32 = 3;
pub const CLASS_CATALOG_LIGHTWEIGHT: u32 = 4;
pub const CLASS_CATALOG_SCAVENGER: u32 = 5;
pub const CLASS_CATALOG_HARDLINE: u32 = 6;
pub const CLASS_CATALOG_NINJA: u32 = 7;
pub const CLASS_CATALOG_MARATHON: u32 = 8;
pub const CLASS_CATALOG_DANGER_CLOSE: u32 = 9;
pub const CLASS_CATALOG_STEADY_AIM: u32 = 11;
pub const CLASS_CATALOG_BLING: u32 = 12;
pub const CLASS_CATALOG_SCRAMBLER: u32 = 14;

#[must_use]
pub fn class_catalog_has(perks: [u32; 3], catalog_id: u32) -> bool {
    perks.iter().any(|&id| id == catalog_id)
}

#[must_use]
pub fn perk_bits_from_class_catalog(perks: [u32; 3]) -> [u32; 2] {
    let mut bits = [0u32; 2];
    for id in perks {
        bits[0] |= match id {
            CLASS_CATALOG_SLEIGHT_OF_HAND => crate::PERK_FASTRELOAD,
            CLASS_CATALOG_COLD_BLOODED => playerstate_iw4::PERK_COLDBLOODED,
            CLASS_CATALOG_LIGHTWEIGHT => weapon_iw4::PERK_LIGHTWEIGHT_VIEW_BOB_BIT,
            CLASS_CATALOG_SCAVENGER => crate::item::PERK_SCAVENGER,
            CLASS_CATALOG_NINJA => {
                playerstate_iw4::PERK_QUIETER | playerstate_iw4::PERK_HEARTBREAKER
            }
            CLASS_CATALOG_MARATHON => movement_iw4::PERK_MARATHON,
            CLASS_CATALOG_STEADY_AIM => weapon_iw4::PERK_BULLETACCURACY,
            _ => 0,
        };
    }
    bits
}

#[must_use]
pub fn class_catalog_radar_jam_e_flags(perks: [u32; 3]) -> u32 {
    if class_catalog_has(perks, CLASS_CATALOG_SCRAMBLER) {
        playerstate_iw4::eflags::RADAR_JAM
    } else {
        0
    }
}

pub fn perk_table_code_from_class_catalog(id: u32) -> u32 {
    match id {
        1 => 6,
        2 => 2,
        3 => 9,
        4 => 7,
        5 => 3,
        6 => 8,
        7 => 14,
        8 => 1,
        9 => 10,
        10 => 11,
        11 => 12,
        12 => 5,
        13 => 4,
        14 => 13,
        15 => 15,
        16 => 16,
        _ => 0,
    }
}

pub fn perk_slot_from_class_catalog(id: u32) -> Option<usize> {
    match id {
        2 | 5 | 8 | 12 | 13 => Some(0),
        1 | 3 | 4 | 6 | 9 => Some(1),
        7 | 10 | 11 | 14 | 15 | 16 => Some(2),
        _ => None,
    }
}
