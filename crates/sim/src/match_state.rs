use crate::identities::LifeSequence;

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

mod client_view;
mod events;
mod loadout;
mod snapshot_meta;

pub use client_view::{
    KillcamHud, LinkedWeaponView, LocationSelection, MENU_COMMAND_TAIL, MenuCommand,
    MenuCommandKind, RadarMode, RemoteMissile, ScriptBlur, ScriptDepthOfField, ScriptSeat,
    ViewEffects, VisionChange, is_postfx_dvar,
};
pub use events::{
    EntityEventPayload, EntityEventRecord, EventAudience, EventRecord, PelletFxRecord,
    SIM_EVENT_ROSTER, SimEvent, SimEventRow, UNRELIABLE_SIM_EVENT_COUNT, sim_event_is_reliable,
};
pub use loadout::{
    CLASS_CATALOG_DEATHSTREAKS, CLASS_CATALOG_PERKS, ClassDef, ClassRejectReason, IW4_CAMOS,
    LoadoutSpec, PERSONAL_CLASS_SLOTS, PersonalClass, iw4_camo_index,
};
pub fn class_catalog_perk_name(id: u32) -> Option<&'static str> {
    CLASS_CATALOG_PERKS
        .get(id.checked_sub(1)? as usize)
        .copied()
}

pub use snapshot_meta::{
    ActorOverlay, ClientSnapshotMeta, DroppedItemAmmo, ItemPickupRecord, ScriptDvars, SnapshotMeta,
    TargetBoxDvar,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEndReason {
    ScoreLimit,
    TimeLimit,
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
    pub viewmodel: Option<String>,
    pub actor_overlay: Option<snapshot_meta::ActorOverlay>,
    pub shield: Option<crate::ShieldAttachment>,
    pub weapon_lock: crate::WeaponLock,
    pub lifecycle: ClientLifecycle,
    pub god_mode: bool,
    /// The client's projectiles bounce off everything and fly through the
    /// atlas doors (`projectile_flight`, a cheat).
    pub projectile_flight: bool,
    pub input_receipt: InputReceipt,
    pub loadout: Option<LoadoutSpec>,
    pub equipped_class: Option<crate::ClassId>,
    pub class_refusal: Option<(crate::ClassId, crate::ClassRejectReason)>,

    pub life_sequence: LifeSequence,

    pub item_use_spawn_ms: i32,
    pub item_use_entity: Option<crate::EntityRef>,
    pub item_use_press_ms: i32,

    pub weapon_carry: crate::WeaponCarry,

    pub(crate) dead_since_tick: Option<u32>,

    pub(crate) forced_spawn: Option<crate::SpawnPick>,

    pub(crate) look_at_killer_yaw: i32,

    pub(crate) name: [u8; 16],

    pub kills: i32,
    pub deaths: i32,
    pub score: i32,
    pub kill_streak: i32,
    pub radar: RadarMode,
    pub radar_blocked: bool,
    pub remote_missile: Option<RemoteMissile>,
    pub linked_weapon_view: Option<LinkedWeaponView>,

    pub(crate) ffa_team: Option<u8>,

    pub(crate) client_state_team: i32,

    pub(crate) rank: i32,
    pub(crate) prestige: i32,
    pub(crate) player_card_icon: u32,
    pub(crate) player_card_title: u32,
    pub(crate) player_card_nameplate: u32,
    pub(crate) client_dvars: Vec<(String, String)>,
    pub(crate) shellshock: Option<hud_iw4::ShockParams>,
    pub(crate) view_effects: ViewEffects,
    pub(crate) menu_commands: Vec<MenuCommand>,
    pub(crate) location_selection: Option<LocationSelection>,

    pub(crate) controls: ScriptControls,

    pub(crate) max_health: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptControls {
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
    pub(crate) fn to_snapshot_meta(&self) -> ClientSnapshotMeta {
        ClientSnapshotMeta {
            viewmodel: self.viewmodel.clone(),
            actor_overlay: self.actor_overlay.clone(),
            weapon_carry: self.weapon_carry.clone(),
            shield: self.shield,
            shield_collision: None,
            controls: self.controls,
            weapon_lock: self.weapon_lock,
            killcam_hud: None,
            lifecycle: self.lifecycle,
            god_mode: self.god_mode,
            loadout: self.loadout.clone(),
            equipped_class: self.equipped_class,
            class_refusal: self.class_refusal,
            life_sequence: self.life_sequence,
            item_use_spawn_ms: self.item_use_spawn_ms,
            item_use_entity: self.item_use_entity,
            item_use_press_ms: self.item_use_press_ms,

            score: self.score,
            kills: self.kills,
            deaths: self.deaths,
            kill_streak: self.kill_streak,
            radar: self.radar,
            radar_blocked: self.radar_blocked,
            remote_missile: self.remote_missile,
            linked_weapon_view: self.linked_weapon_view,

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
            view_effects: self.view_effects.clone(),
            menu_commands: self.menu_commands.clone(),
            location_selection: self.location_selection.clone(),
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
