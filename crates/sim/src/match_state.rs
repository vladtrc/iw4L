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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CopyCatLoadout {
    pub spec: LoadoutSpec,
    pub in_use: bool,
    pub owner: ClientId,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamHud {
    pub final_kill: bool,

    pub time_until_respawn_ms: i32,

    pub kc_timer_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientSnapshotMeta {
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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapshotMeta {
    pub objectives: crate::ObjectiveMatch,
    pub map_doors: Option<crate::MapDoors>,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthRegenCensus {
    pub old_health: i32,
    pub hurt_time_ms: i32,
    pub very_hurt: bool,

    pub named_sound: Option<&'static str>,
}

/// What the authority did with one client's movement commands. `path_units`
/// sums the distance of every applied move, so a circle or a wall slide still
/// counts where net displacement would not.
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

    pub(crate) health_regen: gamemode_iw4::PlayerHealthRegenState,

    pub(crate) last_named_sound: gamemode_iw4::HealthRegenSound,

    pub(crate) dead_since_tick: Option<u32>,

    pub(crate) forced_spawn: Option<crate::SpawnPick>,

    pub(crate) look_at_killer_yaw: i32,

    pub(crate) name: [u8; 16],

    pub kills: i32,
    pub deaths: i32,
    pub score: i32,

    pub(crate) cur_death_streak: i32,

    pub(crate) attackers_this_life: Vec<(ClientId, i32)>,

    pub(crate) last_kill: Option<(ClientId, i32)>,

    pub(crate) combathigh_until_ms: Option<i32>,

    pub(crate) pistoldeath_this_life: bool,

    pub(crate) laststand_until_ms: Option<i32>,

    pub(crate) copycat_this_life: bool,

    pub(crate) copycat_class_this_life: bool,

    pub(crate) copycat_loadout: Option<CopyCatLoadout>,

    pub(crate) ffa_team: Option<u8>,

    pub(crate) client_state_team: i32,

    pub(crate) rank: i32,
    pub(crate) prestige: i32,
    pub(crate) player_card_icon: u32,
    pub(crate) player_card_title: u32,
    pub(crate) player_card_nameplate: u32,
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

    pub fn copycat_stash_defined(&self) -> bool {
        self.copycat_loadout.is_some()
    }

    pub(crate) fn take_spawn_loadout(&mut self) -> (LoadoutSpec, bool) {
        let own = self.loadout.clone().unwrap_or_default();
        let Some(stash) = self.copycat_loadout.as_ref() else {
            return (own, false);
        };
        if !stash.in_use {
            return (own, false);
        }
        let mut merged = stash.spec.clone();
        merged.class_id = own.class_id;
        merged.revision = own.revision;
        self.loadout = Some(merged.clone());
        (merged, true)
    }

    pub(crate) fn to_snapshot_meta(&self) -> ClientSnapshotMeta {
        ClientSnapshotMeta {
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
        }
    }

    pub(crate) fn adopt_snapshot_meta(&mut self, meta: &ClientSnapshotMeta) {
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

pub const CLASS_CATALOG_STOPPING_POWER: u32 = 1;
pub const CLASS_CATALOG_SLEIGHT_OF_HAND: u32 = 2;
pub const CLASS_CATALOG_COLD_BLOODED: u32 = 3;
pub const CLASS_CATALOG_LIGHTWEIGHT: u32 = 4;
pub const CLASS_CATALOG_SCAVENGER: u32 = 5;
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
