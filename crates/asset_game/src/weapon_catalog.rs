use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

fn mint_weapon_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, FpvMeshSpace, FxSpace, MaterialSpace,
    ProjectileModelSpace, SoundAliasSpace, TracerSpace, WorldWeaponSpace, XAnimSpace, ZoneOwner,
};
use asset_iw4::size::{SURF_TYPE_NUM, WEAPON_ANIM_COUNT, weap_anim};
use fastfile_iw4::{
    Ptr, ScriptStrings, WeaponIdleCapture, WeaponMovementOfsCapture, ZonePtr, ZoneStream,
};
use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBodyFacts {
    pub body_resolved: bool,

    pub fire_time_ms: i32,

    pub impact_type: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,

    pub fire_delay_ms: i32,

    pub hold_fire_time_ms: i32,

    pub weap_type: i32,

    pub player_anim_type: i32,

    pub weap_class: i32,

    pub offhand_class: i32,

    pub shots_per_fire: i32,

    pub ammo_index: i32,

    pub clip_index: i32,

    pub ammo_counter_clip: i32,

    pub low_ammo_warning_threshold: f32,

    pub hip_spread_stand_min: f32,

    pub hip_spread_ducked_min: f32,

    pub hip_spread_prone_min: f32,

    pub hip_spread_stand_max: f32,

    pub hip_spread_ducked_max: f32,

    pub hip_spread_prone_max: f32,

    pub hip_spread_decay_rate: f32,

    pub hip_spread_fire_add: f32,

    pub hip_spread_turn_add: f32,

    pub hip_spread_move_add: f32,

    pub hip_spread_ducked_decay: f32,

    pub hip_spread_prone_decay: f32,

    pub i_reticle_side_size: i32,

    pub i_reticle_min_ofs: i32,

    pub hip_reticle_side_pos: f32,

    pub ads_aim_pitch: f32,

    pub ads_crosshair_in_frac: f32,

    pub ads_crosshair_out_frac: f32,

    pub ads_spread: f32,

    pub aim_down_sight: bool,

    pub ads_zoom_fov: f32,

    pub ads_dof: Option<[f32; 2]>,

    pub ads_zoom_in_frac: f32,

    pub ads_zoom_out_frac: f32,

    pub no_ads_when_mag_empty: bool,

    pub inherits_perks: bool,

    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub melee_damage: i32,

    pub overlay_reticle: i32,

    pub ads_overlay_width: f32,

    pub ads_overlay_height: f32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub knife_model: u32,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo_at_0x667: Option<bool>,

    pub offhand_hold_is_cancelable_at_0x681: Option<bool>,

    pub move_speed_scale: f32,

    pub ads_move_speed_scale: f32,

    pub sprint_duration_scale: f32,

    pub stance_ofs_at_0x168: [f32; 3],

    pub stance_ofs_at_0x18c: [f32; 3],

    pub night_vision_wear_time: i32,

    pub ads_bob_factor_at_0x330: f32,

    pub ads_view_bob_mult_at_0x334: f32,

    pub movement: WeaponMovementOfsInputs,

    pub idle: WeaponIdleInputs,

    pub clip_size: i32,
    pub penetrate_type: i32,

    pub penetrate_multiplier: f32,

    pub rifle_bullet: bool,
    pub inventory_type: i32,
    pub fire_type: i32,
    pub max_ammo: i32,
    pub damage: i32,
    pub rechamber_time_ms: i32,

    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,
    pub reload_time_ms: i32,

    pub reload_show_rocket_time_ms: i32,
    pub reload_empty_time_ms: i32,
    pub reload_add_time_ms: i32,

    pub reload_empty_add_time_ms: i32,
    pub reload_start_time_ms: i32,

    pub reload_start_add_time_ms: i32,
    pub reload_end_time_ms: i32,

    pub kill_icon_ratio: i32,

    pub flip_kill_icon: bool,

    pub reload_ammo_add: i32,

    pub reload_start_add: i32,

    pub no_partial_reload: bool,

    pub bolt_action: bool,

    pub segmented_reload: bool,

    pub sprint_raise_time_ms: i32,

    pub sprint_loop_time_ms: i32,

    pub sprint_drop_time_ms: i32,
    pub fuse_time_ms: i32,

    pub cook_off_hold: bool,

    pub clip_only: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
    pub start_ammo: i32,

    pub ammo_count_clip_relative: bool,
    pub min_damage: i32,
    pub min_player_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,

    pub kick: WeaponKickFacts,

    pub sway: WeaponSwayFacts,

    pub dual_wield_view_model_offset: f32,

    pub no_dual_wield: bool,
}

impl WeaponBodyFacts {
    pub fn start_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(
            self.start_ammo,
            self.clip_size,
            self.ammo_count_clip_relative,
        )
    }

    pub fn max_ammo_rounds(&self) -> i32 {
        leftover_clip_relative_rounds(self.max_ammo, self.clip_size, self.ammo_count_clip_relative)
    }
}

fn leftover_clip_relative_rounds(count: i32, clip_size: i32, clip_relative: bool) -> i32 {
    if clip_relative && clip_size > 0 && count > 0 {
        count.saturating_mul(clip_size)
    } else {
        count
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponKickFacts {
    pub f_ads_view_kick_center_speed: f32,

    pub f_hip_view_kick_center_speed: f32,
    pub gun_max_pitch: f32,
    pub gun_max_yaw: f32,
    pub ads_gun_kick_reduced_kick_bullets: i32,
    pub ads_gun_kick_reduced_kick_percent: f32,
    pub ads_gun_kick_pitch_min: f32,
    pub ads_gun_kick_pitch_max: f32,
    pub ads_gun_kick_yaw_min: f32,
    pub ads_gun_kick_yaw_max: f32,
    pub ads_gun_kick_accel: f32,
    pub ads_gun_kick_speed_max: f32,
    pub ads_gun_kick_speed_decay: f32,
    pub ads_gun_kick_static_decay: f32,
    pub ads_view_kick_pitch_min: f32,
    pub ads_view_kick_pitch_max: f32,
    pub ads_view_kick_yaw_min: f32,
    pub ads_view_kick_yaw_max: f32,
    pub hip_gun_kick_reduced_kick_bullets: i32,
    pub hip_gun_kick_reduced_kick_percent: f32,
    pub hip_gun_kick_pitch_min: f32,
    pub hip_gun_kick_pitch_max: f32,
    pub hip_gun_kick_yaw_min: f32,
    pub hip_gun_kick_yaw_max: f32,
    pub hip_gun_kick_accel: f32,
    pub hip_gun_kick_speed_max: f32,
    pub hip_gun_kick_speed_decay: f32,
    pub hip_gun_kick_static_decay: f32,
    pub hip_view_kick_pitch_min: f32,
    pub hip_view_kick_pitch_max: f32,
    pub hip_view_kick_yaw_min: f32,
    pub hip_view_kick_yaw_max: f32,
}

impl WeaponKickFacts {
    fn from_capture(c: fastfile_iw4::WeaponKickCapture) -> Self {
        Self {
            f_ads_view_kick_center_speed: c.f_ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: c.f_hip_view_kick_center_speed,
            gun_max_pitch: c.gun_max_pitch,
            gun_max_yaw: c.gun_max_yaw,
            ads_gun_kick_reduced_kick_bullets: c.ads_gun_kick_reduced_kick_bullets,
            ads_gun_kick_reduced_kick_percent: c.ads_gun_kick_reduced_kick_percent,
            ads_gun_kick_pitch_min: c.ads_gun_kick_pitch_min,
            ads_gun_kick_pitch_max: c.ads_gun_kick_pitch_max,
            ads_gun_kick_yaw_min: c.ads_gun_kick_yaw_min,
            ads_gun_kick_yaw_max: c.ads_gun_kick_yaw_max,
            ads_gun_kick_accel: c.ads_gun_kick_accel,
            ads_gun_kick_speed_max: c.ads_gun_kick_speed_max,
            ads_gun_kick_speed_decay: c.ads_gun_kick_speed_decay,
            ads_gun_kick_static_decay: c.ads_gun_kick_static_decay,
            ads_view_kick_pitch_min: c.ads_view_kick_pitch_min,
            ads_view_kick_pitch_max: c.ads_view_kick_pitch_max,
            ads_view_kick_yaw_min: c.ads_view_kick_yaw_min,
            ads_view_kick_yaw_max: c.ads_view_kick_yaw_max,
            hip_gun_kick_reduced_kick_bullets: c.hip_gun_kick_reduced_kick_bullets,
            hip_gun_kick_reduced_kick_percent: c.hip_gun_kick_reduced_kick_percent,
            hip_gun_kick_pitch_min: c.hip_gun_kick_pitch_min,
            hip_gun_kick_pitch_max: c.hip_gun_kick_pitch_max,
            hip_gun_kick_yaw_min: c.hip_gun_kick_yaw_min,
            hip_gun_kick_yaw_max: c.hip_gun_kick_yaw_max,
            hip_gun_kick_accel: c.hip_gun_kick_accel,
            hip_gun_kick_speed_max: c.hip_gun_kick_speed_max,
            hip_gun_kick_speed_decay: c.hip_gun_kick_speed_decay,
            hip_gun_kick_static_decay: c.hip_gun_kick_static_decay,
            hip_view_kick_pitch_min: c.hip_view_kick_pitch_min,
            hip_view_kick_pitch_max: c.hip_view_kick_pitch_max,
            hip_view_kick_yaw_min: c.hip_view_kick_yaw_min,
            hip_view_kick_yaw_max: c.hip_view_kick_yaw_max,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayFacts {
    pub sway_max_angle: f32,
    pub sway_lerp_speed: f32,
    pub sway_pitch_scale: f32,
    pub sway_yaw_scale: f32,
    pub sway_horiz_scale: f32,
    pub sway_vert_scale: f32,

    pub sway_shell_shock_scale: f32,
    pub ads_sway_max_angle: f32,
    pub ads_sway_lerp_speed: f32,
    pub ads_sway_pitch_scale: f32,
    pub ads_sway_yaw_scale: f32,
    pub ads_sway_horiz_scale: f32,
    pub ads_sway_vert_scale: f32,
}

impl WeaponSwayFacts {
    fn from_capture(c: fastfile_iw4::WeaponSwayCapture) -> Self {
        Self {
            sway_max_angle: c.sway_max_angle,
            sway_lerp_speed: c.sway_lerp_speed,
            sway_pitch_scale: c.sway_pitch_scale,
            sway_yaw_scale: c.sway_yaw_scale,
            sway_horiz_scale: c.sway_horiz_scale,
            sway_vert_scale: c.sway_vert_scale,
            sway_shell_shock_scale: c.sway_shell_shock_scale,
            ads_sway_max_angle: c.ads_sway_max_angle,
            ads_sway_lerp_speed: c.ads_sway_lerp_speed,
            ads_sway_pitch_scale: c.ads_sway_pitch_scale,
            ads_sway_yaw_scale: c.ads_sway_yaw_scale,
            ads_sway_horiz_scale: c.ads_sway_horiz_scale,
            ads_sway_vert_scale: c.ads_sway_vert_scale,
        }
    }

    pub fn hip_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.sway_max_angle,
            lerp_speed: self.sway_lerp_speed,
            pitch_scale: self.sway_pitch_scale,
            yaw_scale: self.sway_yaw_scale,
            horiz_scale: self.sway_horiz_scale,
            vert_scale: self.sway_vert_scale,
        }
    }

    pub fn ads_params(&self) -> weapon_iw4::WeaponSwayParams {
        weapon_iw4::WeaponSwayParams {
            max_angle: self.ads_sway_max_angle,
            lerp_speed: self.ads_sway_lerp_speed,
            pitch_scale: self.ads_sway_pitch_scale,
            yaw_scale: self.ads_sway_yaw_scale,
            horiz_scale: self.ads_sway_horiz_scale,
            vert_scale: self.ads_sway_vert_scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadoutCatalogKind {
    Primary,
    Secondary,

    Equipment { offhand_class: i32 },

    AttachmentVariant { base_id: u32 },

    NonPlayer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacOffhandBucket {
    Lethal,
    Tactical,
}

pub fn cac_offhand_bucket(offhand_class: i32) -> Option<CacOffhandBucket> {
    match offhand_class {
        1 | 4 | 5 => Some(CacOffhandBucket::Lethal),
        2 | 3 => Some(CacOffhandBucket::Tactical),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadoutCatalogRow {
    pub id: u32,
    pub key: crate::AssetKey,
    pub name: String,
    pub kind: LoadoutCatalogKind,

    pub weap_class: i32,

    pub item_group: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CatalogWeapon {
    pub name: String,

    pub weap_def: Option<(u8, u32)>,

    pub display_name_key: Option<String>,

    pub reticle: WeaponReticleAssets,

    pub hud_material_edges: WeaponHudMaterialEdges,

    pub overlay_material: Option<String>,

    pub overlay_image: Option<String>,
    pub overlay_material_slot: Option<Ptr>,

    pub scope_name: Option<String>,

    pub scope_viewmodel: Option<String>,

    pub scope_rows: [Iw5ScopeRow; 6],

    pub hud_icon: Option<String>,
    pub hud_icon_slot: Option<Ptr>,
    pub pickup_icon: Option<String>,
    pub pickup_icon_slot: Option<Ptr>,
    pub pickup_icon_image: Option<String>,
    pub pickup_icon_ratio: i32,
    pub hud_icon_ratio: i32,

    pub hud_icon_image: Option<String>,

    pub dpad_icon: Option<String>,
    pub dpad_icon_image: Option<String>,
    pub dpad_icon_atlas: Option<[u8; 2]>,
    pub dpad_icon_ratio: i32,
    pub kill_icon: Option<String>,
    pub kill_icon_slot: Option<Ptr>,

    pub kill_icon_image: Option<String>,

    pub proj_trail: Option<String>,
    pub proj_trail_slot: Option<Ptr>,

    pub proj_beacon: Option<String>,
    pub proj_beacon_slot: Option<Ptr>,

    pub proj_ignition: Option<String>,
    pub proj_ignition_slot: Option<Ptr>,

    pub projectile_fx: WeaponProjectileFx,

    pub gun_xmodel: Option<String>,

    pub hand_xmodel: Option<String>,

    pub world_model: Option<String>,

    pub projectile_model: Option<String>,

    pub rocket_model: Option<String>,

    pub sz_xanims: [Option<String>; WEAPON_ANIM_COUNT],

    pub sz_xanims_right: [Option<String>; WEAPON_ANIM_COUNT],

    pub sz_xanims_left: [Option<String>; WEAPON_ANIM_COUNT],

    pub hide_tags: Vec<String>,

    pub sounds: WeaponSoundAliases,

    pub combat_fx: WeaponCombatFx,
    pub facts: WeaponBodyFacts,
}

#[derive(Clone, Debug, Default)]
pub struct Iw5ScopeRow {
    pub scope: Option<String>,
    pub overlay: Option<String>,
    pub overlay_lowres: Option<String>,
    pub overlay_emp: Option<String>,
    pub overlay_emp_lowres: Option<String>,
    pub view_model: Option<String>,
    pub thermal: bool,
    pub width: f32,
    pub height: f32,
    pub ads_zoom_fov: f32,
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponReticleAssets {
    pub center_material: Option<String>,

    pub side_material: Option<String>,

    pub center_edge: AssetEdge<MaterialSpace>,

    pub side_edge: AssetEdge<MaterialSpace>,

    pub center_image: Option<String>,

    pub side_image: Option<String>,

    pub center_size: i32,

    pub side_size: i32,

    pub center_slot: Option<Ptr>,

    pub side_slot: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponHudMaterialEdges {
    pub overlay: AssetEdge<MaterialSpace>,
    pub hud_icon: AssetEdge<MaterialSpace>,
    pub pickup_icon: AssetEdge<MaterialSpace>,
    pub kill_icon: AssetEdge<MaterialSpace>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponProjectileFx {
    pub trail: AssetEdge<FxSpace>,
    pub beacon: AssetEdge<FxSpace>,
    pub ignition: AssetEdge<FxSpace>,
}

impl WeaponProjectileFx {
    pub fn edges(self) -> [AssetEdge<FxSpace>; 3] {
        [self.trail, self.beacon, self.ignition]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotetrackConvention {
    #[default]
    SoundMap,

    InlinePrefix,
}

impl NotetrackConvention {
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::SoundMap => "sound_map",
            Self::InlinePrefix => "inline_prefix",
        }
    }
}

pub const T5_NOTE_SOUND_PREFIX: &str = "sndnt#";

pub const T5_NOTE_RUMBLE_PREFIX: &str = "rmbnt#";

pub fn t5_inline_note_alias<'a>(note: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, tail) = note.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(tail)
        .filter(|alias| !alias.is_empty())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponSoundAliases {
    pub fire: Option<String>,
    pub fire_player: Option<String>,
    pub empty_fire: Option<String>,
    pub empty_fire_player: Option<String>,
    pub melee_swipe: Option<String>,
    pub melee_swipe_player: Option<String>,
    pub melee_hit: Option<String>,
    pub melee_miss: Option<String>,
    pub pickup: Option<String>,
    pub pickup_player: Option<String>,
    pub ammo_pickup: Option<String>,
    pub ammo_pickup_player: Option<String>,
    pub pullback: Option<String>,
    pub pullback_player: Option<String>,
    pub reload: Option<String>,
    pub reload_player: Option<String>,
    pub reload_empty: Option<String>,
    pub reload_empty_player: Option<String>,
    pub reload_start: Option<String>,
    pub reload_start_player: Option<String>,
    pub reload_end: Option<String>,
    pub reload_end_player: Option<String>,
    pub rechamber: Option<String>,
    pub rechamber_player: Option<String>,
    pub alt_switch: Option<String>,
    pub alt_switch_player: Option<String>,
    pub raise: Option<String>,
    pub raise_player: Option<String>,
    pub first_raise: Option<String>,
    pub first_raise_player: Option<String>,
    pub putaway: Option<String>,
    pub putaway_player: Option<String>,
    pub proj_explosion: Option<String>,

    pub projectile: Option<String>,

    pub proj_ignition_sound: Option<String>,

    pub bounce: [Option<String>; asset_iw4::size::SURF_TYPE_NUM],

    pub notetrack_sound_map: Vec<(String, String)>,

    pub notetrack_rumble_map: Vec<(String, String)>,

    pub fire_player_akimbo: Option<String>,

    pub fire_loop: Option<String>,
    pub fire_loop_player: Option<String>,

    pub fire_stop: Option<String>,
    pub fire_stop_player: Option<String>,

    pub fire_last: Option<String>,

    pub fire_last_player: Option<String>,

    pub leftover_sound_overrides: Vec<LeftoverSoundOverride>,

    pub fire_ptr_kind: Option<&'static str>,
    pub fire_player_ptr_kind: Option<&'static str>,
    pub reload_player_ptr_kind: Option<&'static str>,

    pub notetrack_convention: NotetrackConvention,
}

macro_rules! weapon_sound_slots {
    ($($variant:ident => $field:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        pub enum WeaponSoundSlot {
            $($variant),+
        }

        impl WeaponSoundSlot {
            pub const ALL: [Self; weapon_sound_slots!(@count $($variant),+)] = [
                $(Self::$variant),+
            ];
        }

        const WEAPON_SOUND_SLOT_COUNT: usize = weapon_sound_slots!(@count $($variant),+);

        impl WeaponSoundAliases {
            fn hint(&self, slot: WeaponSoundSlot) -> Option<&str> {
                match slot {
                    $(WeaponSoundSlot::$variant => self.$field.as_deref()),+
                }
            }
        }
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(weapon_sound_slots!(@unit $variant)),+])
    };
    (@unit $variant:ident) => { () };
}

weapon_sound_slots! {
    Fire => fire,
    FirePlayer => fire_player,
    EmptyFire => empty_fire,
    EmptyFirePlayer => empty_fire_player,
    MeleeSwipe => melee_swipe,
    MeleeSwipePlayer => melee_swipe_player,
    MeleeHit => melee_hit,
    MeleeMiss => melee_miss,
    Pickup => pickup,
    PickupPlayer => pickup_player,
    AmmoPickup => ammo_pickup,
    AmmoPickupPlayer => ammo_pickup_player,
    Pullback => pullback,
    PullbackPlayer => pullback_player,
    Reload => reload,
    ReloadPlayer => reload_player,
    ReloadEmpty => reload_empty,
    ReloadEmptyPlayer => reload_empty_player,
    ReloadStart => reload_start,
    ReloadStartPlayer => reload_start_player,
    ReloadEnd => reload_end,
    ReloadEndPlayer => reload_end_player,
    Rechamber => rechamber,
    RechamberPlayer => rechamber_player,
    AltSwitch => alt_switch,
    AltSwitchPlayer => alt_switch_player,
    Raise => raise,
    RaisePlayer => raise_player,
    FirstRaise => first_raise,
    FirstRaisePlayer => first_raise_player,
    Putaway => putaway,
    PutawayPlayer => putaway_player,
    ProjectileExplosion => proj_explosion,
    Projectile => projectile,
    ProjIgnition => proj_ignition_sound,
    FireLast => fire_last,
    FireLastPlayer => fire_last_player,
}

impl WeaponSoundAliases {
    pub fn reachable_aliases(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let slots = [
            self.fire.as_deref(),
            self.fire_player.as_deref(),
            self.empty_fire.as_deref(),
            self.empty_fire_player.as_deref(),
            self.melee_swipe.as_deref(),
            self.melee_swipe_player.as_deref(),
            self.melee_hit.as_deref(),
            self.melee_miss.as_deref(),
            self.pickup.as_deref(),
            self.pickup_player.as_deref(),
            self.ammo_pickup.as_deref(),
            self.ammo_pickup_player.as_deref(),
            self.pullback.as_deref(),
            self.pullback_player.as_deref(),
            self.reload.as_deref(),
            self.reload_player.as_deref(),
            self.reload_empty.as_deref(),
            self.reload_empty_player.as_deref(),
            self.reload_start.as_deref(),
            self.reload_start_player.as_deref(),
            self.reload_end.as_deref(),
            self.reload_end_player.as_deref(),
            self.rechamber.as_deref(),
            self.rechamber_player.as_deref(),
            self.alt_switch.as_deref(),
            self.alt_switch_player.as_deref(),
            self.raise.as_deref(),
            self.raise_player.as_deref(),
            self.first_raise.as_deref(),
            self.first_raise_player.as_deref(),
            self.putaway.as_deref(),
            self.putaway_player.as_deref(),
            self.proj_explosion.as_deref(),
            self.projectile.as_deref(),
            self.proj_ignition_sound.as_deref(),
            self.fire_player_akimbo.as_deref(),
            self.fire_loop.as_deref(),
            self.fire_loop_player.as_deref(),
            self.fire_stop.as_deref(),
            self.fire_stop_player.as_deref(),
            self.fire_last.as_deref(),
            self.fire_last_player.as_deref(),
        ];
        for slot in slots.into_iter().flatten() {
            if !slot.is_empty() {
                out.push(slot);
            }
        }
        for name in &self.bounce {
            if let Some(s) = name.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
        }
        for (_, alias) in &self.notetrack_sound_map {
            if !alias.is_empty() {
                out.push(alias.as_str());
            }
        }
        for ov in &self.leftover_sound_overrides {
            if let Some(s) = ov.override_sound.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
            if let Some(s) = ov.altmode_sound.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeftoverAnimOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub anim_tree_type: u32,
    pub override_anim: Option<String>,
    pub altmode_anim: Option<String>,
    pub anim_time_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeftoverSoundOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub sound_type: u32,
    pub override_sound: Option<String>,
    pub altmode_sound: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponCombatFx {
    pub view_flash: AssetEdge<FxSpace>,
    pub view_flash_hint: Option<String>,
    pub world_flash: AssetEdge<FxSpace>,
    pub world_flash_hint: Option<String>,
    pub view_shell_eject: AssetEdge<FxSpace>,
    pub view_shell_eject_hint: Option<String>,
    pub world_shell_eject: AssetEdge<FxSpace>,
    pub world_shell_eject_hint: Option<String>,
    pub view_last_shot_eject: AssetEdge<FxSpace>,
    pub view_last_shot_eject_hint: Option<String>,
    pub world_last_shot_eject: AssetEdge<FxSpace>,
    pub world_last_shot_eject_hint: Option<String>,
    pub explosion: AssetEdge<FxSpace>,
    pub explosion_hint: Option<String>,

    pub tracer: AssetEdge<TracerSpace>,

    pub tracer_hint: Option<String>,
    pub view_flash_slot: Option<Ptr>,
    pub world_flash_slot: Option<Ptr>,
    pub view_shell_eject_slot: Option<Ptr>,
    pub world_shell_eject_slot: Option<Ptr>,
    pub view_last_shot_eject_slot: Option<Ptr>,
    pub world_last_shot_eject_slot: Option<Ptr>,
    pub explosion_slot: Option<Ptr>,
    pub tracer_slot: Option<Ptr>,
}

fn present_bound<'a, S: crate::asset_graph::IndexSpace>(
    edge: AssetEdge<S>,
    hint: &'a Option<String>,
) -> Option<&'a str> {
    edge.is_bound()
        .then(|| hint.as_deref())
        .flatten()
        .filter(|name| !name.is_empty())
}

impl WeaponCombatFx {
    pub fn view_flash_present(&self) -> Option<&str> {
        present_bound(self.view_flash, &self.view_flash_hint)
    }

    pub fn world_flash_present(&self) -> Option<&str> {
        present_bound(self.world_flash, &self.world_flash_hint)
    }

    pub fn flash_present(&self, player_view: bool) -> Option<&str> {
        if player_view {
            self.view_flash_present()
        } else {
            self.world_flash_present()
        }
    }

    pub fn flash_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_flash
        } else {
            self.world_flash
        }
    }

    pub fn view_shell_eject_present(&self) -> Option<&str> {
        present_bound(self.view_shell_eject, &self.view_shell_eject_hint)
    }

    pub fn world_shell_eject_present(&self) -> Option<&str> {
        present_bound(self.world_shell_eject, &self.world_shell_eject_hint)
    }

    pub fn brass_present(&self, player_view: bool) -> Option<&str> {
        if player_view {
            self.view_shell_eject_present()
        } else {
            self.world_shell_eject_present()
        }
    }

    pub fn last_shot_eject_pair_authored(&self) -> bool {
        self.view_last_shot_eject_slot.is_some() && self.world_last_shot_eject_slot.is_some()
    }

    pub fn last_shot_eject_present(&self, player_view: bool) -> Option<&str> {
        if player_view {
            present_bound(self.view_last_shot_eject, &self.view_last_shot_eject_hint)
        } else {
            present_bound(self.world_last_shot_eject, &self.world_last_shot_eject_hint)
        }
    }

    pub fn brass_present_for_event(&self, player_view: bool, last_shot: bool) -> Option<&str> {
        if last_shot && self.last_shot_eject_pair_authored() {
            self.last_shot_eject_present(player_view)
        } else {
            self.brass_present(player_view)
        }
    }

    pub fn brass_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_shell_eject
        } else {
            self.world_shell_eject
        }
    }

    pub fn brass_edge_for_event(&self, player_view: bool, last_shot: bool) -> AssetEdge<FxSpace> {
        if last_shot && self.last_shot_eject_pair_authored() {
            if player_view {
                self.view_last_shot_eject
            } else {
                self.world_last_shot_eject
            }
        } else {
            self.brass_edge(player_view)
        }
    }

    pub fn explosion_present(&self) -> Option<&str> {
        present_bound(self.explosion, &self.explosion_hint)
    }

    pub fn fx_edges(&self) -> [AssetEdge<FxSpace>; 7] {
        [
            self.view_flash,
            self.world_flash,
            self.view_shell_eject,
            self.world_shell_eject,
            self.view_last_shot_eject,
            self.world_last_shot_eject,
            self.explosion,
        ]
    }
}

impl CatalogWeapon {
    pub fn with_timers(
        name: impl Into<String>,
        weap_def: Option<(u8, u32)>,
        gun_xmodel: Option<String>,
        sz_xanims: [Option<String>; WEAPON_ANIM_COUNT],
        fire_time_ms: i32,
        raise_time_ms: i32,
        move_speed_scale: f32,
        ads_move_speed_scale: f32,
    ) -> Self {
        Self {
            name: name.into(),
            weap_def,
            display_name_key: None,
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            overlay_material_slot: None,
            scope_name: None,
            scope_viewmodel: None,
            scope_rows: Default::default(),
            hud_icon: None,
            hud_icon_slot: None,
            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_slot: None,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel: None,
            world_model: None,
            projectile_model: None,
            rocket_model: None,
            sz_xanims,
            sz_xanims_right: [const { None }; WEAPON_ANIM_COUNT],
            sz_xanims_left: [const { None }; WEAPON_ANIM_COUNT],
            hide_tags: Vec::new(),
            sounds: WeaponSoundAliases::default(),
            combat_fx: WeaponCombatFx::default(),
            facts: WeaponBodyFacts {
                body_resolved: weap_def.is_some(),
                fire_time_ms,
                raise_time_ms,
                move_speed_scale,
                ads_move_speed_scale,
                ..WeaponBodyFacts::default()
            },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponCatalog {
    entries: Vec<CatalogWeapon>,
    strings: ScriptStrings,
}

impl WeaponCatalog {
    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>) {
        let Some(geometry) = stream.weapon() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = geometry
            .gun_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let hand_xmodel = geometry
            .hand_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let world_model = geometry
            .world_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let projectile_model = geometry
            .projectile_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let rocket_model = geometry
            .rocket_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_COUNT]);
        let sz_xanims_right = geometry
            .sz_xanims_right
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_COUNT]);
        let sz_xanims_left = geometry
            .sz_xanims_left
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_COUNT]);
        let hide_tags = read_hide_tags(stream, &self.strings, geometry.hide_tags);
        self.entries.push(CatalogWeapon {
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(ptr_key),
            display_name_key: geometry
                .display_name_at_0x8
                .and_then(|ptr| read_name(stream, ptr)),
            reticle: WeaponReticleAssets {
                center_material: None,
                side_material: None,
                center_edge: AssetEdge::Absent,
                side_edge: AssetEdge::Absent,
                center_image: None,
                side_image: None,
                center_size: geometry.reticle_center_size_at_0x128,
                side_size: geometry.i_reticle_side_size,
                center_slot: geometry.reticle_center_material_slot,
                side_slot: geometry.reticle_side_material_slot,
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            overlay_material_slot: geometry.overlay_material_slot,
            scope_name: None,
            scope_viewmodel: None,
            scope_rows: Default::default(),
            hud_icon: None,
            hud_icon_slot: geometry.hud_icon_slot,
            pickup_icon: None,
            pickup_icon_slot: geometry.pickup_icon_slot,
            pickup_icon_image: None,
            pickup_icon_ratio: geometry.pickup_icon_ratio,
            hud_icon_ratio: geometry.hud_icon_ratio,
            hud_icon_image: None,
            dpad_icon: geometry.dpad_icon_name.and_then(|p| read_name(stream, p)),
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: geometry.dpad_icon_ratio,
            kill_icon: geometry
                .kill_icon_name
                .and_then(|ptr| read_name(stream, ptr)),
            kill_icon_slot: geometry.kill_icon_slot,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: geometry.proj_trail_slot,
            proj_beacon: None,
            proj_beacon_slot: geometry.proj_beacon_slot,
            proj_ignition: None,
            proj_ignition_slot: geometry.proj_ignition_slot,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,
            world_model,
            projectile_model,
            rocket_model,
            sz_xanims,
            sz_xanims_right,
            sz_xanims_left,
            hide_tags,
            sounds: WeaponSoundAliases {
                notetrack_convention: NotetrackConvention::SoundMap,
                fire: geometry
                    .fire_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                fire_player: geometry
                    .fire_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                empty_fire: geometry
                    .empty_fire_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                empty_fire_player: geometry
                    .empty_fire_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_swipe: geometry
                    .melee_swipe_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_swipe_player: geometry
                    .melee_swipe_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_hit: geometry
                    .melee_hit_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_miss: geometry
                    .melee_miss_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pickup: geometry
                    .pickup_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pickup_player: geometry
                    .pickup_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                ammo_pickup: geometry
                    .ammo_pickup_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                ammo_pickup_player: geometry
                    .ammo_pickup_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pullback: geometry
                    .pullback_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pullback_player: geometry
                    .pullback_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload: geometry
                    .reload_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_player: geometry
                    .reload_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_empty: geometry
                    .reload_empty_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_empty_player: geometry
                    .reload_empty_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_start: geometry
                    .reload_start_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_start_player: geometry
                    .reload_start_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_end: geometry
                    .reload_end_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_end_player: geometry
                    .reload_end_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                rechamber: geometry
                    .rechamber_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                rechamber_player: geometry
                    .rechamber_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                alt_switch: geometry
                    .alt_switch_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                alt_switch_player: geometry
                    .alt_switch_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                raise: geometry
                    .raise_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                raise_player: geometry
                    .raise_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                first_raise: geometry
                    .first_raise_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                first_raise_player: geometry
                    .first_raise_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                putaway: geometry
                    .putaway_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                putaway_player: geometry
                    .putaway_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                proj_explosion: geometry
                    .proj_explosion_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                projectile: geometry
                    .projectile_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                proj_ignition_sound: geometry
                    .proj_ignition_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                bounce: geometry
                    .bounce_sound_names
                    .map(|slot| slot.and_then(|ptr| read_name(stream, ptr))),
                notetrack_sound_map: read_script_string_map(
                    stream,
                    &self.strings,
                    geometry.notetrack_sound_keys,
                    geometry.notetrack_sound_values,
                ),
                notetrack_rumble_map: read_script_string_map(
                    stream,
                    &self.strings,
                    geometry.notetrack_rumble_keys,
                    geometry.notetrack_rumble_values,
                ),
                fire_player_akimbo: None,
                fire_loop: None,
                fire_loop_player: None,
                fire_stop: None,
                fire_stop_player: None,
                fire_last: geometry
                    .fire_last_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                fire_last_player: geometry
                    .fire_last_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                leftover_sound_overrides: Vec::new(),
                fire_ptr_kind: None,
                fire_player_ptr_kind: None,
                reload_player_ptr_kind: None,
            },
            combat_fx: WeaponCombatFx {
                view_flash_slot: geometry.view_flash_slot,
                world_flash_slot: geometry.world_flash_slot,
                view_shell_eject_slot: geometry.view_shell_eject_slot,
                world_shell_eject_slot: geometry.world_shell_eject_slot,
                view_last_shot_eject_slot: geometry.view_last_shot_eject_slot,
                world_last_shot_eject_slot: geometry.world_last_shot_eject_slot,
                explosion_slot: geometry.explosion_slot,
                tracer_slot: geometry.tracer_slot,
                ..WeaponCombatFx::default()
            },
            facts: WeaponBodyFacts {
                body_resolved: geometry.weap_def.is_some(),
                fire_time_ms: geometry.fire_time_ms,
                impact_type: geometry.impact_type,
                raise_time_ms: geometry.raise_time_ms,
                drop_time_ms: geometry.drop_time_ms,
                fire_delay_ms: geometry.fire_delay_ms,
                hold_fire_time_ms: geometry.hold_fire_time_ms,
                weap_type: geometry.weap_type,
                weap_class: geometry.weap_class,
                player_anim_type: geometry.player_anim_type,
                offhand_class: geometry.offhand_class,
                shots_per_fire: geometry.shots_per_fire,
                ammo_index: geometry.ammo_index,
                clip_index: geometry.clip_index,
                ammo_counter_clip: geometry.ammo_counter_clip,
                low_ammo_warning_threshold: geometry.low_ammo_warning_threshold,
                hip_spread_stand_min: geometry.hip_spread_stand_min,
                hip_spread_ducked_min: geometry.hip_spread_ducked_min,
                hip_spread_prone_min: geometry.hip_spread_prone_min,
                hip_spread_stand_max: geometry.hip_spread_stand_max,
                hip_spread_ducked_max: geometry.hip_spread_ducked_max,
                hip_spread_prone_max: geometry.hip_spread_prone_max,
                hip_spread_decay_rate: geometry.hip_spread_decay_rate,
                hip_spread_fire_add: geometry.hip_spread_fire_add,
                hip_spread_turn_add: geometry.hip_spread_turn_add,
                hip_spread_move_add: geometry.hip_spread_move_add,
                hip_spread_ducked_decay: geometry.hip_spread_ducked_decay,
                hip_spread_prone_decay: geometry.hip_spread_prone_decay,
                i_reticle_side_size: geometry.i_reticle_side_size,
                i_reticle_min_ofs: geometry.i_reticle_min_ofs,
                hip_reticle_side_pos: geometry.hip_reticle_side_pos,
                ads_aim_pitch: geometry.ads_aim_pitch,
                ads_crosshair_in_frac: geometry.ads_crosshair_in_frac,
                ads_crosshair_out_frac: geometry.ads_crosshair_out_frac,
                ads_spread: geometry.ads_spread,
                aim_down_sight: geometry.aim_down_sight,
                ads_zoom_fov: geometry.ads_zoom_fov,
                ads_dof: Some(geometry.ads_dof),
                ads_zoom_in_frac: geometry.ads_zoom_in_frac,
                ads_zoom_out_frac: geometry.ads_zoom_out_frac,
                no_ads_when_mag_empty: geometry.no_ads_when_mag_empty,
                inherits_perks: geometry.inherits_perks,
                ads_in_rate: geometry.ads_in_rate,
                ads_out_rate: geometry.ads_out_rate,
                rechamber_while_ads: geometry.rechamber_while_ads,
                ads_fire_only: geometry.ads_fire_only,
                melee_damage: geometry.melee_damage,
                overlay_reticle: geometry.overlay_reticle,
                ads_overlay_width: geometry.ads_overlay_width,
                ads_overlay_height: geometry.ads_overlay_height,
                melee_time_ms: geometry.melee_time_ms,
                melee_delay_ms: geometry.melee_delay_ms,
                melee_charge_time_ms: geometry.melee_charge_time_ms,
                melee_charge_delay_ms: geometry.melee_charge_delay_ms,
                knife_model: geometry.knife_model,
                quick_raise_time_ms: geometry.quick_raise_time_ms,
                quick_drop_time_ms: geometry.quick_drop_time_ms,
                select_requires_ammo_at_0x667: geometry.select_requires_ammo_at_0x667,
                offhand_hold_is_cancelable_at_0x681: geometry.offhand_hold_is_cancelable_at_0x681,
                move_speed_scale: geometry.move_speed_scale,
                ads_move_speed_scale: geometry.ads_move_speed_scale,
                sprint_duration_scale: geometry.sprint_duration_scale,
                stance_ofs_at_0x168: geometry.stance_ofs_at_0x168,
                stance_ofs_at_0x18c: geometry.stance_ofs_at_0x18c,
                night_vision_wear_time: geometry.night_vision_wear_time,
                ads_bob_factor_at_0x330: geometry.ads_bob_factor_at_0x330,
                ads_view_bob_mult_at_0x334: geometry.ads_view_bob_mult_at_0x334,
                movement: movement_from_capture(geometry.movement),
                idle: idle_from_capture(geometry.idle),
                clip_size: geometry.clip_size,
                penetrate_type: geometry.penetrate_type,
                penetrate_multiplier: geometry.penetrate_multiplier,
                rifle_bullet: geometry.rifle_bullet,
                inventory_type: geometry.inventory_type,
                fire_type: geometry.fire_type,
                max_ammo: geometry.max_ammo,
                damage: geometry.damage,
                rechamber_time_ms: geometry.rechamber_time_ms,
                rechamber_bolt_time_ms: geometry.rechamber_bolt_time_ms,
                rechamber_bolt_delay_ms: geometry.rechamber_bolt_delay_ms,
                reload_time_ms: geometry.reload_time_ms,
                reload_show_rocket_time_ms: geometry.reload_show_rocket_time_ms,
                reload_empty_time_ms: geometry.reload_empty_time_ms,
                reload_add_time_ms: geometry.reload_add_time_ms,
                reload_empty_add_time_ms: 0,
                reload_start_time_ms: geometry.reload_start_time_ms,
                reload_start_add_time_ms: geometry.reload_start_add_time_ms,
                reload_end_time_ms: geometry.reload_end_time_ms,
                kill_icon_ratio: geometry.kill_icon_ratio,
                flip_kill_icon: geometry.flip_kill_icon,
                reload_ammo_add: geometry.reload_ammo_add,
                reload_start_add: geometry.reload_start_add,
                no_partial_reload: geometry.no_partial_reload,
                bolt_action: geometry.bolt_action,
                segmented_reload: geometry.segmented_reload,
                sprint_raise_time_ms: geometry.sprint_raise_time_ms,
                sprint_loop_time_ms: geometry.sprint_loop_time_ms,
                sprint_drop_time_ms: geometry.sprint_drop_time_ms,
                fuse_time_ms: geometry.fuse_time_ms,
                cook_off_hold: geometry.cook_off_hold,
                clip_only: geometry.clip_only,
                proj_impact_explode: geometry.proj_impact_explode,
                stick_to_players: geometry.stick_to_players,
                explosion_radius: geometry.explosion_radius,
                explosion_radius_min: geometry.explosion_radius_min,
                explosion_inner_damage: geometry.explosion_inner_damage,
                explosion_outer_damage: geometry.explosion_outer_damage,
                projectile_speed: geometry.projectile_speed,
                projectile_speed_up: geometry.projectile_speed_up,
                projectile_activate_dist: geometry.projectile_activate_dist,
                projectile_explosion_type: geometry.projectile_explosion_type,
                parallel_bounce: geometry.parallel_bounce,
                perpendicular_bounce: geometry.perpendicular_bounce,
                start_ammo: geometry.start_ammo,
                ammo_count_clip_relative: false,
                min_damage: geometry.min_damage,
                min_player_damage: geometry.min_player_damage,
                max_damage_range: geometry.max_damage_range,
                min_damage_range: geometry.min_damage_range,
                kick: WeaponKickFacts::from_capture(geometry.kick),
                sway: WeaponSwayFacts::from_capture(geometry.sway),
                dual_wield_view_model_offset: geometry.dual_wield_view_model_offset,
                no_dual_wield: geometry.no_dual_wield,
            },
        });
    }

    pub fn resolve_reticles(&mut self, materials: &crate::MaterialCatalog) {
        let names_of = |slot: Ptr| {
            let material = materials
                .material_index(slot)
                .and_then(|i| materials.materials.get(i.get()))?;
            let name = Some(material.name.to_string()).filter(|n| !n.is_empty())?;
            let image = materials.hud_image_name(material).map(str::to_owned);
            Some((name, image))
        };
        for entry in &mut self.entries {
            if let Some(slot) = entry.reticle.center_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.reticle.center_material = Some(name);
                    entry.reticle.center_image = image;
                }
            }
            if entry.reticle.center_image.is_none() {
                if let Some(name) = entry.reticle.center_material.as_deref() {
                    if let Some(index) = materials.material_index_by_name(name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.center_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.reticle.side_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.reticle.side_material = Some(name);
                    entry.reticle.side_image = image;
                }
            }
            if entry.reticle.side_image.is_none() {
                if let Some(name) = entry.reticle.side_material.as_deref() {
                    if let Some(index) = materials.material_index_by_name(name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.reticle.side_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.overlay_material_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.overlay_material = Some(name);
                    entry.overlay_image = image;
                }
            }
            if entry.overlay_image.is_none() {
                if let Some(name) = entry.overlay_material.as_deref() {
                    if let Some(index) = materials.material_index_by_name(name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.overlay_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.pickup_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.pickup_icon = Some(name);
                    entry.pickup_icon_image = image;
                }
            }
            if let Some(slot) = entry.hud_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    entry.hud_icon = Some(name);
                    if entry.hud_icon_image.is_none() {
                        entry.hud_icon_image = image;
                    }
                }
            }
            if entry.hud_icon_image.is_none() {
                if let Some(name) = entry.hud_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_name(name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.hud_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(slot) = entry.kill_icon_slot {
                if let Some((name, image)) = names_of(slot) {
                    if entry.kill_icon.is_none() {
                        entry.kill_icon = Some(name);
                    }
                    if entry.kill_icon_image.is_none() {
                        entry.kill_icon_image = image;
                    }
                }
            }
            if entry.kill_icon_image.is_none() {
                if let Some(name) = entry.kill_icon.as_deref() {
                    if let Some(index) = materials.material_index_by_name(name) {
                        if let Some(material) = materials.materials.get(index.order()) {
                            entry.kill_icon_image =
                                materials.hud_image_name(material).map(str::to_owned);
                        }
                    }
                }
            }
            if let Some(name) = entry.dpad_icon.as_deref()
                && let Some(index) = materials.material_index_by_name(name)
                && let Some(material) = materials.materials.get(index.order())
            {
                entry.dpad_icon_image = materials.hud_image_name(material).map(str::to_owned);
                entry.dpad_icon_atlas = material.texture_atlas;
            }
            entry.reticle.center_edge = material_hint_edge(
                entry.reticle.center_material.as_deref(),
                entry.reticle.center_slot.is_some(),
                materials,
            );
            entry.reticle.side_edge = material_hint_edge(
                entry.reticle.side_material.as_deref(),
                entry.reticle.side_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.overlay = material_hint_edge(
                entry.overlay_material.as_deref(),
                entry.overlay_material_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.hud_icon = material_hint_edge(
                entry.hud_icon.as_deref(),
                entry.hud_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.pickup_icon = material_hint_edge(
                entry.pickup_icon.as_deref(),
                entry.pickup_icon_slot.is_some(),
                materials,
            );
            entry.hud_material_edges.kill_icon = material_hint_edge(
                entry.kill_icon.as_deref(),
                entry.kill_icon_slot.is_some(),
                materials,
            );
        }
    }

    pub fn hud_material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            census.push(entry.reticle.center_edge);
            census.push(entry.reticle.side_edge);
            census.push(entry.hud_material_edges.overlay);
            census.push(entry.hud_material_edges.hud_icon);
            census.push(entry.hud_material_edges.pickup_icon);
            census.push(entry.hud_material_edges.kill_icon);
        }
        census
    }

    pub fn resolve_projectile_fx_edges(&mut self, fx: &crate::FxCatalog) {
        for entry in &mut self.entries {
            stamp_fx_edge(
                entry.proj_trail_slot,
                fx,
                &mut entry.projectile_fx.trail,
                &mut entry.proj_trail,
            );
            stamp_fx_edge(
                entry.proj_beacon_slot,
                fx,
                &mut entry.projectile_fx.beacon,
                &mut entry.proj_beacon,
            );
            stamp_fx_edge(
                entry.proj_ignition_slot,
                fx,
                &mut entry.projectile_fx.ignition,
                &mut entry.proj_ignition,
            );
        }
    }

    pub fn projectile_fx_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in entry.projectile_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        for entry in &mut self.entries {
            stamp_fx_edge(
                entry.combat_fx.view_flash_slot,
                fx,
                &mut entry.combat_fx.view_flash,
                &mut entry.combat_fx.view_flash_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.world_flash_slot,
                fx,
                &mut entry.combat_fx.world_flash,
                &mut entry.combat_fx.world_flash_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.view_shell_eject_slot,
                fx,
                &mut entry.combat_fx.view_shell_eject,
                &mut entry.combat_fx.view_shell_eject_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.world_shell_eject_slot,
                fx,
                &mut entry.combat_fx.world_shell_eject,
                &mut entry.combat_fx.world_shell_eject_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.view_last_shot_eject_slot,
                fx,
                &mut entry.combat_fx.view_last_shot_eject,
                &mut entry.combat_fx.view_last_shot_eject_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.world_last_shot_eject_slot,
                fx,
                &mut entry.combat_fx.world_last_shot_eject,
                &mut entry.combat_fx.world_last_shot_eject_hint,
            );
            stamp_fx_edge(
                entry.combat_fx.explosion_slot,
                fx,
                &mut entry.combat_fx.explosion,
                &mut entry.combat_fx.explosion_hint,
            );
            let tracer_name = entry
                .combat_fx
                .tracer_slot
                .and_then(|s| tracers.name_at_slot(s));
            entry.combat_fx.tracer_hint = tracer_name.map(str::to_owned);
            entry.combat_fx.tracer = match (entry.combat_fx.tracer_slot, tracer_name) {
                (None, _) => AssetEdge::Absent,
                (_, Some(name)) => match tracers.index_by_name(name) {
                    Some(index) => AssetEdge::bind_order(index, tracers.zone_of(index)),
                    None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                },
                (Some(_), None) => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            };
        }
    }

    pub fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
    ) {
        let Some(geometry) = stream.weapon() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = geometry
            .gun_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let hand_xmodel = geometry
            .hand_xmodel_name
            .and_then(|ptr| leftover_cstr_iw5(stream, ptr));
        let world_model = geometry
            .world_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let overlay_material = leftover_iw5_overlay_name(stream, &geometry);
        let overlay_image = None;
        let overlay_material_slot = leftover_iw5_weapdef_overlay_slot(stream, geometry.weap_def);
        let scope_name = geometry
            .scope0_name
            .and_then(|ptr| leftover_cstr_iw5(stream, ptr));
        let scope_rows = geometry.scope_overlays.map(|row| Iw5ScopeRow {
            scope: row
                .scope_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            overlay: row
                .overlay_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            overlay_lowres: row
                .overlay_lowres_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            overlay_emp: row
                .overlay_emp_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            overlay_emp_lowres: row
                .overlay_emp_lowres_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            view_model: row
                .view_model_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            thermal: row.thermal,
            width: row.width,
            height: row.height,
            ads_zoom_fov: row.ads_zoom_fov,
            ads_zoom_in_frac: row.ads_zoom_in_frac,
            ads_zoom_out_frac: row.ads_zoom_out_frac,
        });

        let scope_viewmodel = scope_rows
            .iter()
            .find(|row| !row.thermal && row.overlay.is_some() && row.view_model.is_some())
            .and_then(|row| row.view_model.clone());
        let mut sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_iw5(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_COUNT]);
        let leftover_anim_overrides = leftover_iw5_anim_overrides(stream, &geometry);
        apply_leftover_default_anim_overrides(&mut sz_xanims, &leftover_anim_overrides);
        self.entries.push(CatalogWeapon {
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(iw5_ptr_key),
            display_name_key: geometry
                .display_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: leftover_iw5_reticle(stream, geometry.weap_def),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material,
            overlay_image,
            overlay_material_slot,
            scope_name,
            scope_viewmodel,
            scope_rows,
            hud_icon: leftover_iw5_material_name(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),
            hud_icon_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),

            pickup_icon: None,
            pickup_icon_slot: leftover_iw5_asset_slot(stream, geometry.weap_def, 508, 808),
            pickup_icon_image: None,
            pickup_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 512, 816)),
            hud_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 504, 800)),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_slot: None,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,
            world_model,
            projectile_model: None,
            rocket_model: None,
            sz_xanims,
            sz_xanims_right: [const { None }; WEAPON_ANIM_COUNT],
            sz_xanims_left: [const { None }; WEAPON_ANIM_COUNT],
            hide_tags: read_hide_tags_iw5(stream, strings, geometry.hide_tags),
            sounds: leftover_iw5_sounds(stream, strings, &geometry),
            combat_fx: WeaponCombatFx::default(),
            facts: capture_iw5_body_facts(stream, &geometry),
        });
    }

    pub fn capture_t5(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
    ) {
        let Some(geometry) = stream.weapon() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = geometry
            .gun_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let hand_xmodel = geometry
            .hand_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_t5(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_COUNT]);
        self.entries.push(CatalogWeapon {
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(|p| (p.block, p.offset)),
            display_name_key: geometry
                .display_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: leftover_t5_reticle(stream, geometry.weap_def),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: leftover_t5_overlay_name(stream, &geometry),
            overlay_image: None,
            overlay_material_slot: leftover_t5_overlay_slot(stream, &geometry),
            scope_name: None,
            scope_viewmodel: None,
            scope_rows: Default::default(),
            hud_icon: leftover_t5_material_name_opt(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_HUD_ICON_OFF,
            ),
            hud_icon_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_HUD_ICON_OFF,
            ),

            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_t5(stream, body, 0x324)),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_slot: None,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,

            world_model: geometry
                .world_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            projectile_model: geometry
                .projectile_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            rocket_model: geometry
                .rocket_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            sz_xanims,
            sz_xanims_right: [const { None }; WEAPON_ANIM_COUNT],
            sz_xanims_left: [const { None }; WEAPON_ANIM_COUNT],
            hide_tags: read_hide_tags_t5(stream, strings, geometry.hide_tags),
            sounds: leftover_t5_sounds(stream, strings, &geometry),
            combat_fx: leftover_t5_combat_fx(stream, &geometry),
            facts: capture_t5_body_facts(stream, &geometry),
        });
    }

    pub fn push(&mut self, entry: CatalogWeapon) {
        self.entries.push(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn tracer_type_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            census.push(entry.combat_fx.tracer);
        }
        census
    }

    pub fn combat_fx_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in entry.combat_fx.fx_edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn projectile_model_hints(&self) -> HashSet<String> {
        self.entries
            .iter()
            .filter_map(|entry| entry.projectile_model.clone())
            .filter(|name| !name.is_empty())
            .collect()
    }

    pub fn rocket_model_hints(&self) -> HashSet<String> {
        self.entries
            .iter()
            .filter_map(|entry| entry.rocket_model.clone())
            .collect()
    }

    pub fn into_registry(self) -> WeaponRegistry {
        WeaponRegistry::from_catalog(self.entries)
    }
}

fn material_hint_edge(
    hint: Option<&str>,
    authored_slot: bool,
    materials: &crate::MaterialCatalog,
) -> AssetEdge<MaterialSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    if !authored_slot && hint.is_none() {
        return AssetEdge::Absent;
    }
    match hint.and_then(|name| materials.material_index_by_name(name)) {
        Some(index) => AssetEdge::bind(index, materials.zone_of(index.order())),
        None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    }
}

fn stamp_fx_edge(
    slot: Option<Ptr>,
    fx: &crate::FxCatalog,
    edge: &mut AssetEdge<FxSpace>,
    hint: &mut Option<String>,
) {
    let leftover = hint.as_deref().filter(|s| !s.is_empty()).map(str::to_owned);
    let slot_name = slot.and_then(|s| fx.name_at_slot(s)).map(str::to_owned);
    let name = leftover.or(slot_name);
    *hint = name.clone();
    *edge = match name.as_deref() {
        None if slot.is_none() => AssetEdge::Absent,
        Some(name) => match fx.index_by_name(name) {
            Some(index) => AssetEdge::bind_order(index, fx.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
        None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    };
}

fn fpv_zone_hint_edge(
    from_zone: bool,
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    fpv: &crate::FpvMeshCatalog,
) -> AssetEdge<FpvMeshSpace> {
    if !from_zone {
        return AssetEdge::Absent;
    }
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match fpv.index_by_name(ns, name) {
            Some(index) => AssetEdge::bind_order(index, fpv.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn world_zone_hint_edge(
    from_zone: bool,
    hint: Option<&str>,
    catalog: &crate::WorldWeaponCatalog,
) -> AssetEdge<WorldWeaponSpace> {
    if !from_zone {
        return AssetEdge::Absent;
    }
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match catalog.index_by_name(name) {
            Some(index) => AssetEdge::bind_order(index, catalog.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn bounce_hint_edge(
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    catalog: &crate::SoundCatalog,
) -> AssetEdge<SoundAliasSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match catalog.index_in(ns, name) {
            Some(index) => AssetEdge::bind_order(index, catalog.zone_of_alias(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn absent_bounce_edges() -> [AssetEdge<SoundAliasSpace>; SURF_TYPE_NUM] {
    [AssetEdge::Absent; SURF_TYPE_NUM]
}

fn absent_weapon_sound_edges() -> [AssetEdge<SoundAliasSpace>; WEAPON_SOUND_SLOT_COUNT] {
    [AssetEdge::Absent; WEAPON_SOUND_SLOT_COUNT]
}

fn xanim_hint_edge(
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    xanims: &crate::XAnimCatalog,
) -> AssetEdge<XAnimSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match xanims.index_by_name(ns, name) {
            Some(index) => AssetEdge::bind_order(index, xanims.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn fx_hint_edge(
    authored_slot: bool,
    hint: Option<&str>,
    fx: &crate::FxCatalog,
) -> AssetEdge<FxSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    if !authored_slot && hint.is_none() {
        return AssetEdge::Absent;
    }
    match hint.and_then(|name| fx.index_by_name(name)) {
        Some(index) => AssetEdge::bind_order(index, fx.zone_of(index)),
        None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    }
}

fn ptr_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

fn remap_iw5_weap_type(raw: i32) -> i32 {
    match raw {
        1 => 0,
        2 => 1,
        3 => 2,
        other => other,
    }
}

fn remap_t5_weap_class(raw: i32) -> i32 {
    match raw {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        5 => 6,
        6 => 7,
        7 => 8,
        8 => 10,
        10 => 11,
        other => other,
    }
}

fn read_sz_xanims_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    arr: fastfile_t5::Ptr,
) -> [Option<String>; WEAPON_ANIM_COUNT] {
    let mut t5 = [const { None }; fastfile_t5::size::WEAPON_XANIM_COUNT];
    for (i, slot) in t5.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * 4) {
            Ok(fastfile_t5::ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    remap_t5_sz_xanims(&t5)
}

fn remap_t5_sz_xanims(t5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_COUNT] {
    use fastfile_t5::size::weap_anim as t5_anim;
    const PAIRS: [(usize, usize); 32] = [
        (t5_anim::IDLE, weap_anim::IDLE),
        (t5_anim::EMPTY_IDLE, weap_anim::EMPTY_IDLE),
        (t5_anim::FIRE, weap_anim::FIRE),
        (t5_anim::HOLD_FIRE, weap_anim::HOLD_FIRE),
        (t5_anim::LASTSHOT, weap_anim::LASTSHOT),
        (t5_anim::RECHAMBER, weap_anim::RECHAMBER),
        (t5_anim::MELEE, weap_anim::MELEE),
        (t5_anim::MELEE_CHARGE, weap_anim::MELEE_CHARGE),
        (t5_anim::RELOAD, weap_anim::RELOAD),
        (t5_anim::RELOAD_EMPTY, weap_anim::RELOAD_EMPTY),
        (t5_anim::RELOAD_START, weap_anim::RELOAD_START),
        (t5_anim::RELOAD_END, weap_anim::RELOAD_END),
        (t5_anim::RAISE, weap_anim::RAISE),
        (t5_anim::FIRST_RAISE, weap_anim::FIRST_RAISE),
        (t5_anim::DROP, weap_anim::DROP),
        (t5_anim::ALT_RAISE, weap_anim::ALT_RAISE),
        (t5_anim::ALT_DROP, weap_anim::ALT_DROP),
        (t5_anim::QUICK_RAISE, weap_anim::QUICK_RAISE),
        (t5_anim::QUICK_DROP, weap_anim::QUICK_DROP),
        (t5_anim::EMPTY_RAISE, weap_anim::EMPTY_RAISE),
        (t5_anim::EMPTY_DROP, weap_anim::EMPTY_DROP),
        (t5_anim::SPRINT_IN, weap_anim::SPRINT_IN),
        (t5_anim::SPRINT_LOOP, weap_anim::SPRINT_LOOP),
        (t5_anim::SPRINT_OUT, weap_anim::SPRINT_OUT),
        (t5_anim::DETONATE, weap_anim::DETONATE),
        (t5_anim::NIGHTVISION_WEAR, weap_anim::NIGHTVISION_WEAR),
        (t5_anim::NIGHTVISION_REMOVE, weap_anim::NIGHTVISION_REMOVE),
        (t5_anim::ADS_FIRE, weap_anim::ADS_FIRE),
        (t5_anim::ADS_LASTSHOT, weap_anim::ADS_LASTSHOT),
        (t5_anim::ADS_RECHAMBER, weap_anim::ADS_RECHAMBER),
        (t5_anim::ADS_UP, weap_anim::ADS_UP),
        (t5_anim::ADS_DOWN, weap_anim::ADS_DOWN),
    ];
    let mut out = [const { None }; WEAPON_ANIM_COUNT];
    for (src, dst) in PAIRS {
        if src < t5.len() {
            out[dst] = t5[src].clone();
        }
    }
    out
}

fn t5_to_iw4_ptr(p: fastfile_t5::Ptr) -> Ptr {
    Ptr {
        block: p.block,
        offset: p.offset,
    }
}

fn i32_at_t5(stream: &fastfile_t5::ZoneStream<'_>, body: fastfile_t5::Ptr, off: usize) -> i32 {
    stream.i32_at(body, off).unwrap_or(0)
}

fn u8_at_t5(stream: &fastfile_t5::ZoneStream<'_>, body: fastfile_t5::Ptr, off: usize) -> u8 {
    stream.u8_at(body, off).unwrap_or(0)
}

fn f32_at_t5(stream: &fastfile_t5::ZoneStream<'_>, body: fastfile_t5::Ptr, off: usize) -> f32 {
    stream.f32_at(body, off).unwrap_or(0.0)
}

fn read_bounce_array_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<[f32; 31]> {
    let arr = match stream.ptr_at(body, off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return None,
    };
    let mut values = [0.0; 31];
    for (i, value) in values.iter_mut().enumerate() {
        *value = stream.f32_at(arr, i * 4).ok()?;
    }
    Some(values)
}

fn leftover_t5_offhand_class(raw: i32) -> i32 {
    match raw {
        4 => 5,
        other => other,
    }
}

fn leftover_t5_ads_rate(trans_ms: i32, stored: f32) -> f32 {
    if stored > 0.0 {
        stored
    } else if trans_ms > 0 {
        1.0 / trans_ms as f32
    } else {
        0.0
    }
}

fn leftover_t5_cstr(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    let name_ptr = match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    stream
        .cstr(name_ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn leftover_t5_asset_slot(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: Option<fastfile_t5::Ptr>,
    off: usize,
) -> Option<Ptr> {
    let body = body?;
    match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Null => None,
        _ => Some(t5_to_iw4_ptr(body.at(off))),
    }
}

fn leftover_t5_reticle(
    stream: &fastfile_t5::ZoneStream<'_>,
    weap_def: Option<fastfile_t5::Ptr>,
) -> WeaponReticleAssets {
    use fastfile_t5::size as sz;
    WeaponReticleAssets {
        center_material: leftover_t5_material_name_opt(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
        ),
        side_material: leftover_t5_material_name_opt(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
        ),
        center_slot: leftover_t5_asset_slot(stream, weap_def, sz::WEAPON_DEF_RETICLE_CENTER_OFF),
        side_slot: leftover_t5_asset_slot(stream, weap_def, sz::WEAPON_DEF_RETICLE_SIDE_OFF),
        center_size: weap_def
            .map(|body| i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF))
            .unwrap_or(0),
        side_size: weap_def
            .map(|body| i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF))
            .unwrap_or(0),
        ..WeaponReticleAssets::default()
    }
}

fn leftover_t5_header_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    leftover_t5_material_name(stream, body, off)
}

fn leftover_t5_material_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    let mat = match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    leftover_t5_cstr(stream, mat, 0)
}

fn leftover_t5_material_name_opt(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: Option<fastfile_t5::Ptr>,
    off: usize,
) -> Option<String> {
    leftover_t5_material_name(stream, body?, off)
}

fn leftover_t5_overlay_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<String> {
    leftover_t5_overlay_pick(stream, geometry).and_then(|(name, _)| name)
}

fn leftover_t5_overlay_slot(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<Ptr> {
    leftover_t5_overlay_pick(stream, geometry).and_then(|(_, slot)| slot)
}

fn leftover_t5_overlay_pick(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<(Option<String>, Option<Ptr>)> {
    let variant = geometry.variant?;
    use fastfile_t5::size as sz;
    let hi = leftover_t5_material_name(stream, variant, sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF);
    let lo = leftover_t5_material_name(
        stream,
        variant,
        sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF,
    );
    let hi_slot =
        leftover_t5_asset_slot(stream, Some(variant), sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF);
    let lo_slot = leftover_t5_asset_slot(
        stream,
        Some(variant),
        sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF,
    );
    if hi.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((hi, hi_slot));
    }
    if lo.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((lo, lo_slot));
    }
    Some((hi.or(lo), hi_slot.or(lo_slot)))
}

fn leftover_t5_zoom_fov(stream: &fastfile_t5::ZoneStream<'_>, variant: fastfile_t5::Ptr) -> f32 {
    use fastfile_t5::size as sz;
    leftover_t5_first_positive_fov(
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF),
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF),
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV3_OFF),
    )
}

fn leftover_t5_first_positive_fov(fov1: f32, fov2: f32, fov3: f32) -> f32 {
    for fov in [fov1, fov2, fov3] {
        if fov > 0.0 {
            return fov;
        }
    }
    0.0
}

fn leftover_t5_combat_fx(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponCombatFx {
    use fastfile_t5::size as sz;
    let body = geometry.weap_def;
    WeaponCombatFx {
        view_flash_slot: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_VIEW_FLASH_OFF),
        view_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_FLASH_OFF)),
        world_flash_slot: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_WORLD_FLASH_OFF),
        world_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_FLASH_OFF)),
        view_shell_eject_slot: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF,
        ),
        view_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF)),
        world_shell_eject_slot: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF,
        ),
        world_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF)),
        view_last_shot_eject_slot: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF,
        ),
        view_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF)
        }),
        world_last_shot_eject_slot: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF,
        ),
        world_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF)
        }),
        ..WeaponCombatFx::default()
    }
}

fn leftover_t5_sounds(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponSoundAliases {
    use fastfile_t5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponSoundAliases::default();
    };
    WeaponSoundAliases {
        notetrack_convention: NotetrackConvention::InlinePrefix,
        fire: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF),
        fire_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF),
        empty_fire: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF),
        empty_fire_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF),
        rechamber: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_OFF),
        rechamber_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF),
        reload: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_OFF),
        reload_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF),
        reload_empty: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF),
        reload_empty_player: leftover_t5_cstr(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF,
        ),
        reload_start: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_START_OFF),
        reload_start_player: leftover_t5_cstr(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF,
        ),
        reload_end: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_OFF),
        reload_end_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF),
        raise_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF),
        putaway_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF),
        notetrack_sound_map: leftover_t5_script_string_map(
            stream,
            strings,
            body,
            sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF,
            sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF,
        ),
        ..WeaponSoundAliases::default()
    }
}

fn leftover_t5_script_string_map(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    body: fastfile_t5::Ptr,
    keys_off: usize,
    values_off: usize,
) -> Vec<(String, String)> {
    let keys = match stream.ptr_at(body, keys_off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let values = match stream.ptr_at(body, values_off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for i in 0..fastfile_t5::size::WEAPON_NOTETRACK_COUNT {
        let Ok(key_id) = stream.u16_at(keys, i * 2) else {
            break;
        };
        if key_id == 0 {
            break;
        }
        let Ok(val_id) = stream.u16_at(values, i * 2) else {
            break;
        };
        let Some(key) = strings.get(stream, key_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(val) = strings.get(stream, val_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        out.push((key.to_owned(), val.to_owned()));
    }
    out
}

fn capture_t5_body_facts(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponBodyFacts {
    use fastfile_t5::size as sz;
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: geometry.weap_type,
        weap_class: remap_t5_weap_class(geometry.weap_class),
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        rechamber_time_ms: geometry.rechamber_time_ms,
        drop_time_ms: geometry.drop_time_ms,
        raise_time_ms: geometry.raise_time_ms,
        bolt_action: geometry.bolt_action,
        select_requires_ammo_at_0x667: Some(leftover_t5_select_requires_ammo()),
        ..WeaponBodyFacts::default()
    };
    if let Some(variant) = geometry.variant {
        facts.reload_time_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_TIME_OFF);
        facts.reload_empty_time_ms =
            i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_EMPTY_TIME_OFF);
        let ads_in_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_TRANS_IN_OFF);
        let ads_out_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_TRANS_OUT_OFF);
        let stored_in = f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_IN_RATE_OFF);
        let stored_out = f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_OUT_RATE_OFF);
        facts.ads_in_rate = leftover_t5_ads_rate(ads_in_ms, stored_in);
        facts.ads_out_rate = leftover_t5_ads_rate(ads_out_ms, stored_out);
        facts.ads_zoom_fov = leftover_t5_zoom_fov(stream, variant);
        facts.ads_zoom_in_frac =
            f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_IN_FRAC_OFF);
        facts.ads_zoom_out_frac =
            f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_OUT_FRAC_OFF);
    }
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.impact_type = i32_at_t5(stream, body, sz::WEAPON_DEF_IMPACT_TYPE_OFF);
    facts.ammo_counter_clip = i32_at_t5(stream, body, sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF);
    facts.start_ammo = i32_at_t5(stream, body, sz::WEAPON_DEF_START_AMMO_OFF);
    facts.max_ammo = i32_at_t5(stream, body, sz::WEAPON_DEF_MAX_AMMO_OFF);
    facts.ammo_count_clip_relative =
        u8_at_t5(stream, body, sz::WEAPON_DEF_AMMO_COUNT_CLIP_RELATIVE_OFF) != 0;
    facts.shots_per_fire = i32_at_t5(stream, body, sz::WEAPON_DEF_SHOT_COUNT_OFF);
    apply_leftover_hip_spread(
        &mut facts,
        leftover_hip_spread_block(
            |off| f32_at_t5(stream, body, off),
            sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF,
        ),
    );
    facts.damage = i32_at_t5(stream, body, sz::WEAPON_DEF_DAMAGE_OFF);
    facts.min_damage = i32_at_t5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_OFF);
    facts.max_damage_range = f32_at_t5(stream, body, sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF);
    facts.min_damage_range = f32_at_t5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF);
    facts.explosion_radius = i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF);
    facts.explosion_radius_min = i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF);
    facts.explosion_inner_damage =
        i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF);
    facts.explosion_outer_damage =
        i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF);
    facts.projectile_speed = i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_OFF);
    facts.projectile_speed_up = i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF);
    facts.projectile_activate_dist =
        i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF);
    facts.projectile_explosion_type =
        i32_at_t5(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF);
    facts.proj_impact_explode = u8_at_t5(stream, body, sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF) != 0;
    facts.offhand_class =
        leftover_t5_offhand_class(i32_at_t5(stream, body, sz::WEAPON_DEF_OFFHAND_CLASS_OFF));
    facts.hold_fire_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF);
    facts.fuse_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_FUSE_TIME_OFF);
    facts.cook_off_hold = u8_at_t5(stream, body, sz::WEAPON_DEF_COOK_OFF_HOLD_OFF) != 0;
    facts.offhand_hold_is_cancelable_at_0x681 =
        Some(u8_at_t5(stream, body, sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF) != 0);
    facts.parallel_bounce = read_bounce_array_t5(stream, body, sz::WEAPON_DEF_PARALLEL_BOUNCE_OFF);
    facts.perpendicular_bounce =
        read_bounce_array_t5(stream, body, sz::WEAPON_DEF_PERPENDICULAR_BOUNCE_OFF);
    facts.fire_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_FIRE_DELAY_OFF);
    facts.quick_drop_time_ms = i32_at_t5(stream, body, sz::WEAPON_QUICK_DROP_TIME_OFF);
    facts.quick_raise_time_ms = i32_at_t5(stream, body, sz::WEAPON_QUICK_RAISE_TIME_OFF);
    facts.rechamber_bolt_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF);

    facts.reload_show_rocket_time_ms = i32_at_t5(stream, body, 0x3d4);
    facts.reload_add_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF);
    facts.reload_empty_add_time_ms =
        i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_EMPTY_ADD_TIME_OFF);
    facts.reload_start_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF);
    facts.reload_start_add_time_ms =
        i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF);
    facts.reload_end_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF);
    facts.reload_ammo_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_AMMO_ADD_OFF);
    facts.reload_start_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_OFF);
    facts.overlay_reticle = i32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF);
    facts.ads_overlay_width = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_WIDTH_OFF);
    facts.ads_overlay_height = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_HEIGHT_OFF);
    facts.i_reticle_side_size = i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF);
    facts.i_reticle_min_ofs = i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF);
    facts.hip_reticle_side_pos = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF);
    facts.no_ads_when_mag_empty =
        u8_at_t5(stream, body, sz::WEAPON_DEF_NO_ADS_WHEN_MAG_EMPTY_OFF) != 0;
    facts.aim_down_sight = u8_at_t5(stream, body, sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF) != 0;
    facts.rechamber_while_ads = u8_at_t5(stream, body, sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF) != 0;
    facts.ads_fire_only = u8_at_t5(stream, body, sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF) != 0;
    facts.no_partial_reload = u8_at_t5(stream, body, sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF) != 0;
    facts.segmented_reload = u8_at_t5(stream, body, sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF) != 0;
    facts.idle = WeaponIdleInputs {
        ads_idle_amount_at_0x36c: f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF),
        hip_idle_amount_at_0x370: f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF),
        ads_idle_speed_at_0x374: f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF),
        hip_idle_speed_at_0x378: f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF),
        idle_crouch_factor_at_0x37c: f32_at_t5(stream, body, sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF),
        idle_prone_factor_at_0x380: f32_at_t5(stream, body, sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF),
    };
    facts.inherits_perks = leftover_t5_inherits_host_perks();
    facts.kick = leftover_t5_kick(stream, geometry);
    facts
}

fn leftover_t5_inherits_host_perks() -> bool {
    true
}

fn leftover_t5_select_requires_ammo() -> bool {
    false
}

fn leftover_t5_kick(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponKickFacts {
    use fastfile_t5::size as sz;
    let mut k = WeaponKickFacts::default();
    if let Some(variant) = geometry.variant {
        k.f_ads_view_kick_center_speed = f32_at_t5(
            stream,
            variant,
            sz::WEAPON_VARIANT_ADS_VIEW_KICK_CENTER_SPEED_OFF,
        );
        k.f_hip_view_kick_center_speed = f32_at_t5(
            stream,
            variant,
            sz::WEAPON_VARIANT_HIP_VIEW_KICK_CENTER_SPEED_OFF,
        );
    }
    let Some(body) = geometry.weap_def else {
        return k;
    };
    k.gun_max_pitch = f32_at_t5(stream, body, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF);
    k.gun_max_yaw = f32_at_t5(stream, body, sz::WEAPON_DEF_GUN_MAX_YAW_OFF);
    k.ads_gun_kick_reduced_kick_bullets = i32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_BULLETS_OFF,
    );
    k.ads_gun_kick_reduced_kick_percent = f32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_PERCENT_OFF,
    );
    k.ads_gun_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF);
    k.ads_gun_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF);
    k.ads_gun_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF);
    k.ads_gun_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF);
    k.ads_gun_kick_accel = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF);
    k.ads_gun_kick_speed_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF);
    k.ads_gun_kick_speed_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF);
    k.ads_gun_kick_static_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF);
    k.ads_view_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF);
    k.ads_view_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF);
    k.ads_view_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF);
    k.ads_view_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_reduced_kick_bullets = i32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_BULLETS_OFF,
    );
    k.hip_gun_kick_reduced_kick_percent = f32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_PERCENT_OFF,
    );
    k.hip_gun_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF);
    k.hip_gun_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF);
    k.hip_gun_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF);
    k.hip_gun_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_accel = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF);
    k.hip_gun_kick_speed_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF);
    k.hip_gun_kick_speed_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF);
    k.hip_gun_kick_static_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF);
    k.hip_view_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF);
    k.hip_view_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF);
    k.hip_view_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF);
    k.hip_view_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF);
    k
}

fn i32_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> i32 {
    stream.i32_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

fn u8_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> u8 {
    stream.u8_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

fn f32_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> f32 {
    stream.f32_at(body, stream.layout(x86, x64)).unwrap_or(0.0)
}

fn leftover_hip_spread_block(read: impl Fn(usize) -> f32, stand_min: usize) -> [f32; 12] {
    core::array::from_fn(|i| read(stand_min + i * 4))
}

fn apply_leftover_hip_spread(facts: &mut WeaponBodyFacts, block: [f32; 12]) {
    facts.hip_spread_stand_min = block[0];
    facts.hip_spread_ducked_min = block[1];
    facts.hip_spread_prone_min = block[2];
    facts.hip_spread_stand_max = block[3];
    facts.hip_spread_ducked_max = block[4];
    facts.hip_spread_prone_max = block[5];
    facts.hip_spread_decay_rate = block[6];
    facts.hip_spread_fire_add = block[7];
    facts.hip_spread_turn_add = block[8];
    facts.hip_spread_move_add = block[9];
    facts.hip_spread_ducked_decay = block[10];
    facts.hip_spread_prone_decay = block[11];
}

fn capture_iw5_body_facts(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> WeaponBodyFacts {
    use fastfile_iw5::size as sz;
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: remap_iw5_weap_type(geometry.weap_type),
        weap_class: geometry.weap_class,
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        ads_overlay_width: geometry.ads_overlay_width,
        ads_overlay_height: geometry.ads_overlay_height,
        overlay_reticle: geometry.overlay_reticle,
        ads_zoom_fov: geometry.ads_zoom_fov,
        ads_zoom_in_frac: geometry.ads_zoom_in_frac,
        ads_zoom_out_frac: geometry.ads_zoom_out_frac,
        ads_in_rate: geometry.ads_in_rate,
        ads_out_rate: geometry.ads_out_rate,
        impact_type: geometry.impact_type,
        ..WeaponBodyFacts::default()
    };
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.ammo_counter_clip = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF, 836);
    facts.start_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_START_AMMO_OFF, 840);
    facts.i_reticle_side_size = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580);
    facts.i_reticle_min_ofs = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF, 584);
    facts.ammo_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_INDEX_OFF, 856);
    facts.clip_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_CLIP_INDEX_OFF, 872);
    facts.max_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_AMMO_OFF, 876);
    facts.shots_per_fire = i32_at_iw5(stream, body, sz::WEAPON_DEF_SHOTS_PER_FIRE_OFF, 880);
    apply_leftover_hip_spread(
        &mut facts,
        leftover_hip_spread_block(
            |off| stream.f32_at(body, off).unwrap_or(0.0),
            stream.layout(sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF, 1420),
        ),
    );
    facts.fire_delay_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FIRE_DELAY_OFF, 920);
    facts.rechamber_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_TIME_OFF, 936);
    facts.rechamber_bolt_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF, 940);
    facts.rechamber_bolt_delay_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_DELAY_OFF, 944);
    facts.reload_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_TIME_OFF, 964);
    facts.reload_empty_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_EMPTY_TIME_OFF, 972);
    facts.reload_add_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF, 976);
    if facts.reload_add_time_ms <= 0 && geometry.reload_override_add_time_ms > 0 {
        facts.reload_add_time_ms = geometry.reload_override_add_time_ms;
    }
    facts.reload_start_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF, 980);
    facts.reload_start_add_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF, 984);
    facts.reload_end_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF, 988);
    facts.drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_DROP_TIME_OFF, 992);
    facts.raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RAISE_TIME_OFF, 996);
    facts.quick_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_DROP_TIME_OFF, 1004);
    facts.quick_raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_RAISE_TIME_OFF, 1008);
    facts.sprint_raise_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_RAISE_TIME_OFF, 1024);
    facts.sprint_loop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_LOOP_TIME_OFF, 1028);
    facts.sprint_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_DROP_TIME_OFF, 1032);
    facts.damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_DAMAGE_OFF, 904);
    facts.min_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_OFF, 2144);
    facts.min_player_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_PLAYER_DAMAGE_OFF, 2148);
    facts.max_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF, 2152);
    facts.min_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF, 2156);
    facts.inherits_perks = u8_at_iw5(stream, body, sz::WEAPON_DEF_INHERITS_PERKS_OFF, 2435) != 0;
    facts.rifle_bullet = u8_at_iw5(stream, body, sz::WEAPON_DEF_RIFLE_BULLET_OFF, 2437) != 0;
    facts.bolt_action = u8_at_iw5(stream, body, sz::WEAPON_DEF_BOLT_ACTION_OFF, 2439) != 0;
    facts.aim_down_sight = u8_at_iw5(stream, body, sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF, 2440) != 0;
    facts.rechamber_while_ads =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF, 2443) != 0;
    facts.ads_fire_only = u8_at_iw5(stream, body, sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF, 2448) != 0;
    facts.no_partial_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF, 2456) != 0;
    facts.segmented_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF, 2457) != 0;
    facts.select_requires_ammo_at_0x667 = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF,
            2450,
        ) != 0,
    );
    facts.offhand_hold_is_cancelable_at_0x681 = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF,
            2478,
        ) != 0,
    );
    facts.idle = WeaponIdleInputs {
        ads_idle_amount_at_0x36c: f32_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF,
            1472,
        ),
        hip_idle_amount_at_0x370: f32_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF,
            1476,
        ),
        ads_idle_speed_at_0x374: f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF, 1480),
        hip_idle_speed_at_0x378: f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF, 1484),
        idle_crouch_factor_at_0x37c: f32_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF,
            1488,
        ),
        idle_prone_factor_at_0x380: f32_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF,
            1492,
        ),
    };
    facts.offhand_class = i32_at_iw5(stream, body, sz::WEAPON_DEF_OFFHAND_CLASS_OFF, 104);
    facts.hold_fire_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF, 948);
    facts.fuse_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FUSE_TIME_OFF, 1072);
    facts.cook_off_hold = u8_at_iw5(stream, body, sz::WEAPON_DEF_COOK_OFF_HOLD_OFF, 2445) != 0;
    facts.explosion_radius = i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF, 1612);
    facts.explosion_radius_min =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF, 1616);
    facts.explosion_inner_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF,
        1620,
    );
    facts.explosion_outer_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF,
        1624,
    );
    facts.projectile_speed = i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_OFF, 1640);
    facts.projectile_speed_up =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF, 1644);
    facts.projectile_activate_dist = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF,
        1652,
    );
    facts.projectile_explosion_type =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF, 1680);
    facts.proj_impact_explode =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF, 2462) != 0;
    facts.stick_to_players =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_STICK_TO_PLAYERS_OFF, 2463) != 0;
    facts
}

fn read_name(stream: &ZoneStream<'_>, ptr: Ptr) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn iw5_ptr_key(p: fastfile_iw5::Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

fn read_sz_xanims_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    arr: fastfile_iw5::Ptr,
) -> [Option<String>; WEAPON_ANIM_COUNT] {
    let mut iw5 = [const { None }; fastfile_iw5::size::WEAPON_ANIM_COUNT];

    let step = stream.pointer_bytes();
    for (i, slot) in iw5.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * step) {
            Ok(fastfile_iw5::ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    remap_iw5_sz_xanims(&iw5)
}

fn remap_iw5_sz_xanims(iw5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_COUNT] {
    let mut out = [const { None }; WEAPON_ANIM_COUNT];
    for (i, name) in iw5.iter().enumerate() {
        let Some(slot) = iw5_anim_tree_type_to_iw4_slot(i as u32) else {
            continue;
        };
        out[slot] = name.clone();
    }
    out
}

fn iw5_anim_tree_type_to_iw4_slot(ty: u32) -> Option<usize> {
    let ty = ty as usize;
    if ty <= weap_anim::ADS_RECHAMBER {
        Some(ty)
    } else if ty == fastfile_iw5::size::WEAPON_ANIM_ADS_UP {
        Some(weap_anim::ADS_UP)
    } else if ty == fastfile_iw5::size::WEAPON_ANIM_ADS_DOWN {
        Some(weap_anim::ADS_DOWN)
    } else {
        None
    }
}

pub fn overlay_name_is_hud_iris(name: &str) -> bool {
    !name.starts_with("mc/")
}

fn leftover_iw5_overlay_name(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Option<String> {
    if let Some(name) = leftover_iw5_material_name(
        stream,
        geometry.weap_def,
        fastfile_iw5::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
        1352,
    ) {
        if overlay_name_is_hud_iris(&name) {
            return Some(name);
        }
    }
    let picked = geometry
        .scope_overlays
        .iter()
        .find(|row| row.overlay_name.is_some() && !row.thermal);
    let att = picked.and_then(|row| leftover_cstr_iw5(stream, row.overlay_name?));
    let lowres = picked.and_then(|row| leftover_cstr_iw5(stream, row.overlay_lowres_name?));
    for candidate in [att.clone(), lowres] {
        if candidate.as_deref().is_some_and(overlay_name_is_hud_iris) {
            return candidate;
        }
    }
    att
}

fn leftover_cstr_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    ptr: fastfile_iw5::Ptr,
) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn leftover_iw5_weapdef_overlay_slot(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
) -> Option<Ptr> {
    leftover_iw5_asset_slot(
        stream,
        weap_def,
        fastfile_iw5::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
        1352,
    )
}

fn leftover_iw5_material_name(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    let mat = match stream.ptr_at(body, field).ok()? {
        fastfile_iw5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    leftover_xstring_at_iw5(stream, mat, 0, 0)
}

fn leftover_iw5_asset_slot(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<Ptr> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    match stream.ptr_at(body, field).ok()? {
        fastfile_iw5::ZonePtr::Null => None,
        _ => {
            let cell = body.at(field);
            Some(Ptr {
                block: cell.block,
                offset: cell.offset,
            })
        }
    }
}

fn leftover_iw5_reticle(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
) -> WeaponReticleAssets {
    use fastfile_iw5::size as sz;
    WeaponReticleAssets {
        center_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        ),
        side_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        ),
        center_slot: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        ),
        side_slot: leftover_iw5_asset_slot(stream, weap_def, sz::WEAPON_DEF_RETICLE_SIDE_OFF, 568),
        center_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF, 576))
            .unwrap_or(0),
        side_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580))
            .unwrap_or(0),
        ..WeaponReticleAssets::default()
    }
}

fn leftover_iw5_sounds(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> WeaponSoundAliases {
    use fastfile_iw5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponSoundAliases::default();
    };
    let mut sounds = WeaponSoundAliases {
        notetrack_convention: NotetrackConvention::SoundMap,
        pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PICKUP_OFF, 128),
        pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PICKUP_PLAYER_OFF,
            136,
        ),
        ammo_pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_AMMO_PICKUP_OFF, 144),
        ammo_pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_AMMO_PICKUP_PLAYER_OFF,
            152,
        ),
        pullback: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PULLBACK_OFF, 168),
        pullback_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PULLBACK_PLAYER_OFF,
            176,
        ),
        fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF, 192),
        empty_fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF, 256),
        empty_fire_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF,
            264,
        ),
        melee_swipe: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_SWIPE_OFF, 272),
        melee_swipe_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF,
            280,
        ),
        melee_hit: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_HIT_OFF, 288),
        melee_miss: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_MISS_OFF, 296),
        rechamber: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_OFF, 304),
        rechamber_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF,
            312,
        ),
        reload: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_OFF, 320),
        reload_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
        reload_empty: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF,
            336,
        ),
        reload_empty_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF,
            344,
        ),
        reload_start: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_OFF,
            352,
        ),
        reload_start_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF,
            360,
        ),
        reload_end: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_OFF, 368),
        reload_end_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF,
            376,
        ),
        alt_switch: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_ALT_SWITCH_OFF, 432),
        alt_switch_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_ALT_SWITCH_PLAYER_OFF,
            440,
        ),
        raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RAISE_OFF, 448),
        raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF,
            456,
        ),
        first_raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRST_RAISE_OFF, 464),
        first_raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRST_RAISE_PLAYER_OFF,
            472,
        ),
        putaway: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PUTAWAY_OFF, 480),
        putaway_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF,
            488,
        ),
        proj_explosion: None,
        projectile: None,
        proj_ignition_sound: None,
        bounce: Default::default(),
        notetrack_sound_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF, 48),
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF, 56),
            sz::WEAPON_DEF_NOTE_SOUND_MAP_COUNT,
        ),
        notetrack_rumble_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_KEYS_OFF, 64),
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_VALUES_OFF, 72),
            sz::WEAPON_DEF_NOTE_RUMBLE_MAP_COUNT,
        ),
        fire_player_akimbo: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_AKIMBO_OFF,
            200,
        ),
        fire_loop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LOOP_OFF, 208),
        fire_loop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LOOP_PLAYER_OFF,
            216,
        ),
        fire_stop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_STOP_OFF, 224),
        fire_stop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_STOP_PLAYER_OFF,
            232,
        ),
        fire_last: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LAST_OFF, 240),
        fire_last_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LAST_PLAYER_OFF,
            248,
        ),
        leftover_sound_overrides: leftover_iw5_sound_overrides(stream, geometry),
        fire_ptr_kind: leftover_iw5_snd_ptr_kind(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF,
            192,
        ),
        reload_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
    };
    apply_leftover_default_sound_overrides(&mut sounds);
    sounds
}

fn leftover_iw5_anim_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<LeftoverAnimOverride> {
    use fastfile_iw5::size as sz;
    let Some(arr) = geometry.anim_overrides else {
        return Vec::new();
    };
    let n = geometry.anim_override_count.max(0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let row = arr.at(i * stream.layout(sz::ANIM_OVERRIDE_ENTRY, 40));
        let Ok(attachment1) = stream.u16_at(row, 0) else {
            break;
        };
        let Ok(attachment2) = stream.u16_at(row, 2) else {
            break;
        };
        let Ok(anim_tree_type) =
            stream.i32_at(row, stream.layout(sz::ANIM_OVERRIDE_ANIM_TREE_TYPE_OFF, 24))
        else {
            break;
        };
        let Ok(anim_time_ms) =
            stream.i32_at(row, stream.layout(sz::ANIM_OVERRIDE_ANIM_TIME_OFF, 28))
        else {
            break;
        };
        out.push(LeftoverAnimOverride {
            attachment1,
            attachment2,
            anim_tree_type: anim_tree_type as u32,
            override_anim: leftover_xstring_at_iw5(
                stream,
                row,
                sz::ANIM_OVERRIDE_OVERRIDE_ANIM_OFF,
                8,
            ),
            altmode_anim: leftover_xstring_at_iw5(
                stream,
                row,
                sz::ANIM_OVERRIDE_ALTMODE_ANIM_OFF,
                16,
            ),
            anim_time_ms,
        });
    }
    out
}

fn leftover_xstring_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    row: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    match stream.ptr_at(row, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => leftover_cstr_iw5(stream, stream.resolve_alias(q)),
        _ => None,
    }
}

fn apply_leftover_default_anim_overrides(
    sz: &mut [Option<String>; WEAPON_ANIM_COUNT],
    overrides: &[LeftoverAnimOverride],
) {
    for ov in overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        let Some(slot) = iw5_anim_tree_type_to_iw4_slot(ov.anim_tree_type) else {
            continue;
        };
        if let Some(name) = ov.override_anim.clone() {
            sz[slot] = Some(name);
        }
    }
}

fn leftover_iw5_sound_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<LeftoverSoundOverride> {
    use fastfile_iw5::size as sz;
    let Some(arr) = geometry.sound_overrides else {
        return Vec::new();
    };
    let n = geometry.sound_override_count.max(0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let row = arr.at(i * stream.layout(sz::SOUND_OVERRIDE_ENTRY, 32));
        let Ok(attachment1) = stream.u16_at(row, 0) else {
            break;
        };
        let Ok(attachment2) = stream.u16_at(row, 2) else {
            break;
        };
        let Ok(sound_type) = stream.i32_at(row, stream.layout(12, 24)) else {
            break;
        };
        out.push(LeftoverSoundOverride {
            attachment1,
            attachment2,
            sound_type: sound_type as u32,
            override_sound: leftover_iw5_snd_alias_at(stream, row, 4, 8),
            altmode_sound: leftover_iw5_snd_alias_at(stream, row, 8, 16),
        });
    }
    out
}

fn apply_leftover_default_sound_overrides(sounds: &mut WeaponSoundAliases) {
    use fastfile_iw5::size as sz;
    for ov in &sounds.leftover_sound_overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        match ov.sound_type {
            x if x == sz::SND_OVERRIDE_TYPE_FIRE => {
                if sounds.fire.is_none() {
                    sounds.fire = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_FIRE => {
                if sounds.fire_player.is_none() {
                    sounds.fire_player = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT => {
                if sounds.fire_last_player.is_none() {
                    sounds.fire_last_player = ov.override_sound.clone();
                }
            }
            _ => {}
        }
    }
}

fn leftover_iw5_snd_alias(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    leftover_iw5_snd_alias_at(stream, body, x86, x64)
}

fn leftover_iw5_snd_ptr_kind(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<&'static str> {
    match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Null) => Some("null"),
        Ok(fastfile_iw5::ZonePtr::Following) => Some("follow"),
        Ok(fastfile_iw5::ZonePtr::Insert) => Some("insert"),
        Ok(fastfile_iw5::ZonePtr::Offset(_)) => Some("offset"),
        Err(_) => Some("err"),
    }
}

fn leftover_iw5_snd_alias_at(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let wrapper = match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
        fastfile_iw5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    let name_ptr = match stream.ptr_at(wrapper, 0).ok()? {
        fastfile_iw5::ZonePtr::Offset(n) => stream.resolve_alias(n),
        _ => wrapper,
    };
    leftover_cstr_iw5(stream, name_ptr)
}

fn leftover_iw5_script_string_map(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    body: fastfile_iw5::Ptr,
    keys_off: usize,
    values_off: usize,
    cap: usize,
) -> Vec<(String, String)> {
    let keys = match stream.ptr_at(body, keys_off) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let values = match stream.ptr_at(body, values_off) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for i in 0..cap {
        let Ok(key_id) = stream.u16_at(keys, i * 2) else {
            break;
        };
        if key_id == 0 {
            break;
        }
        let Ok(val_id) = stream.u16_at(values, i * 2) else {
            break;
        };
        let Some(key) = strings.get(stream, key_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        let val = strings
            .get(stream, val_id)
            .filter(|s| !s.is_empty())
            .unwrap_or(key);
        out.push((key.to_owned(), val.to_owned()));
    }
    out
}

fn read_hide_tags_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    arr: Option<fastfile_iw5::Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..32usize {
        let Ok(id) = stream.u16_at(arr, i * 2) else {
            break;
        };
        if id == 0 {
            continue;
        }
        if let Some(name) = strings.get(stream, id) {
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn read_hide_tags_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    arr: Option<fastfile_t5::Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..fastfile_t5::size::WEAPON_HIDE_TAG_COUNT {
        let Ok(id) = stream.u16_at(arr, i * 2) else {
            break;
        };
        if id == 0 {
            continue;
        }
        if let Some(name) = strings.get(stream, id) {
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn read_hide_tags(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    arr: Option<Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..32usize {
        let Ok(id) = stream.u16_at(arr, i * 2) else {
            break;
        };
        if id == 0 {
            continue;
        }
        if let Some(name) = strings.get(stream, id) {
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn read_script_string_map(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    keys: Option<Ptr>,
    values: Option<Ptr>,
) -> Vec<(String, String)> {
    let (Some(keys), Some(values)) = (keys, values) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..16usize {
        let Ok(key_id) = stream.u16_at(keys, i * 2) else {
            break;
        };
        if key_id == 0 {
            break;
        }
        let Ok(val_id) = stream.u16_at(values, i * 2) else {
            break;
        };
        let Some(key) = strings.get(stream, key_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        let val = strings
            .get(stream, val_id)
            .filter(|s| !s.is_empty())
            .unwrap_or(key);
        out.push((key.to_owned(), val.to_owned()));
    }
    out
}

fn read_sz_xanims(stream: &ZoneStream<'_>, arr: Ptr) -> [Option<String>; WEAPON_ANIM_COUNT] {
    let mut out = [const { None }; WEAPON_ANIM_COUNT];
    for (i, slot) in out.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * stream.pointer_bytes()) {
            Ok(ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    out
}

fn merge_sz_xanims(
    dst: &mut [Option<String>; WEAPON_ANIM_COUNT],
    src: [Option<String>; WEAPON_ANIM_COUNT],
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if d.is_none() {
            *d = s;
        }
    }
}

fn xanims_idle(names: &[Option<String>; WEAPON_ANIM_COUNT]) -> Option<&str> {
    names
        .get(weap_anim::IDLE)
        .and_then(|s| s.as_deref())
        .filter(|s| !s.is_empty())
}

fn merge_sound_aliases(dst: &mut WeaponSoundAliases, src: &WeaponSoundAliases) {
    for (dst, src) in [
        (&mut dst.fire, &src.fire),
        (&mut dst.fire_player, &src.fire_player),
        (&mut dst.empty_fire, &src.empty_fire),
        (&mut dst.empty_fire_player, &src.empty_fire_player),
        (&mut dst.melee_swipe, &src.melee_swipe),
        (&mut dst.melee_swipe_player, &src.melee_swipe_player),
        (&mut dst.melee_hit, &src.melee_hit),
        (&mut dst.melee_miss, &src.melee_miss),
        (&mut dst.pickup, &src.pickup),
        (&mut dst.pickup_player, &src.pickup_player),
        (&mut dst.ammo_pickup, &src.ammo_pickup),
        (&mut dst.ammo_pickup_player, &src.ammo_pickup_player),
        (&mut dst.pullback, &src.pullback),
        (&mut dst.pullback_player, &src.pullback_player),
        (&mut dst.reload, &src.reload),
        (&mut dst.reload_player, &src.reload_player),
        (&mut dst.reload_empty, &src.reload_empty),
        (&mut dst.reload_empty_player, &src.reload_empty_player),
        (&mut dst.reload_start, &src.reload_start),
        (&mut dst.reload_start_player, &src.reload_start_player),
        (&mut dst.reload_end, &src.reload_end),
        (&mut dst.reload_end_player, &src.reload_end_player),
        (&mut dst.rechamber, &src.rechamber),
        (&mut dst.rechamber_player, &src.rechamber_player),
        (&mut dst.alt_switch, &src.alt_switch),
        (&mut dst.alt_switch_player, &src.alt_switch_player),
        (&mut dst.raise, &src.raise),
        (&mut dst.raise_player, &src.raise_player),
        (&mut dst.first_raise, &src.first_raise),
        (&mut dst.first_raise_player, &src.first_raise_player),
        (&mut dst.putaway, &src.putaway),
        (&mut dst.putaway_player, &src.putaway_player),
        (&mut dst.proj_explosion, &src.proj_explosion),
        (&mut dst.projectile, &src.projectile),
        (&mut dst.proj_ignition_sound, &src.proj_ignition_sound),
        (&mut dst.fire_player_akimbo, &src.fire_player_akimbo),
        (&mut dst.fire_loop, &src.fire_loop),
        (&mut dst.fire_loop_player, &src.fire_loop_player),
        (&mut dst.fire_stop, &src.fire_stop),
        (&mut dst.fire_stop_player, &src.fire_stop_player),
        (&mut dst.fire_last, &src.fire_last),
        (&mut dst.fire_last_player, &src.fire_last_player),
    ] {
        if dst.is_none() {
            *dst = src.clone();
        }
    }
    for (dst, src) in [
        (&mut dst.fire_ptr_kind, src.fire_ptr_kind),
        (&mut dst.fire_player_ptr_kind, src.fire_player_ptr_kind),
        (&mut dst.reload_player_ptr_kind, src.reload_player_ptr_kind),
    ] {
        if dst.is_none() {
            *dst = src;
        }
    }
    for (dst, src) in dst.bounce.iter_mut().zip(src.bounce.iter()) {
        if dst.is_none() {
            *dst = src.clone();
        }
    }
    if dst.notetrack_sound_map.is_empty() && !src.notetrack_sound_map.is_empty() {
        dst.notetrack_sound_map = src.notetrack_sound_map.clone();
    }
    if dst.notetrack_rumble_map.is_empty() && !src.notetrack_rumble_map.is_empty() {
        dst.notetrack_rumble_map = src.notetrack_rumble_map.clone();
    }
    if dst.leftover_sound_overrides.is_empty() && !src.leftover_sound_overrides.is_empty() {
        dst.leftover_sound_overrides = src.leftover_sound_overrides.clone();
    }
}

fn merge_combat_fx(dst: &mut WeaponCombatFx, src: &WeaponCombatFx) {
    merge_fx_edge(
        &mut dst.view_flash,
        &mut dst.view_flash_hint,
        src.view_flash,
        &src.view_flash_hint,
    );
    merge_fx_edge(
        &mut dst.world_flash,
        &mut dst.world_flash_hint,
        src.world_flash,
        &src.world_flash_hint,
    );
    merge_fx_edge(
        &mut dst.view_shell_eject,
        &mut dst.view_shell_eject_hint,
        src.view_shell_eject,
        &src.view_shell_eject_hint,
    );
    merge_fx_edge(
        &mut dst.world_shell_eject,
        &mut dst.world_shell_eject_hint,
        src.world_shell_eject,
        &src.world_shell_eject_hint,
    );
    merge_fx_edge(
        &mut dst.view_last_shot_eject,
        &mut dst.view_last_shot_eject_hint,
        src.view_last_shot_eject,
        &src.view_last_shot_eject_hint,
    );
    merge_fx_edge(
        &mut dst.world_last_shot_eject,
        &mut dst.world_last_shot_eject_hint,
        src.world_last_shot_eject,
        &src.world_last_shot_eject_hint,
    );
    merge_fx_edge(
        &mut dst.explosion,
        &mut dst.explosion_hint,
        src.explosion,
        &src.explosion_hint,
    );
    if dst.tracer.is_absent() {
        dst.tracer = src.tracer;
        if dst.tracer_hint.is_none() {
            dst.tracer_hint = src.tracer_hint.clone();
        }
    }
    if dst.view_flash_slot.is_none() {
        dst.view_flash_slot = src.view_flash_slot;
    }
    if dst.world_flash_slot.is_none() {
        dst.world_flash_slot = src.world_flash_slot;
    }
    if dst.view_shell_eject_slot.is_none() {
        dst.view_shell_eject_slot = src.view_shell_eject_slot;
    }
    if dst.world_shell_eject_slot.is_none() {
        dst.world_shell_eject_slot = src.world_shell_eject_slot;
    }
    if dst.view_last_shot_eject_slot.is_none() {
        dst.view_last_shot_eject_slot = src.view_last_shot_eject_slot;
    }
    if dst.world_last_shot_eject_slot.is_none() {
        dst.world_last_shot_eject_slot = src.world_last_shot_eject_slot;
    }
    if dst.explosion_slot.is_none() {
        dst.explosion_slot = src.explosion_slot;
    }
    if dst.tracer_slot.is_none() {
        dst.tracer_slot = src.tracer_slot;
    }
}

fn merge_fx_edge(
    dst_edge: &mut AssetEdge<FxSpace>,
    dst_hint: &mut Option<String>,
    src_edge: AssetEdge<FxSpace>,
    src_hint: &Option<String>,
) {
    if dst_edge.is_absent() {
        *dst_edge = src_edge;
        if dst_hint.is_none() {
            *dst_hint = src_hint.clone();
        }
    }
}

fn kick_body_captured(k: &WeaponKickFacts) -> bool {
    k.hip_view_kick_pitch_min != 0.0
        || k.hip_view_kick_pitch_max != 0.0
        || k.ads_view_kick_pitch_min != 0.0
        || k.ads_view_kick_pitch_max != 0.0
        || k.hip_gun_kick_pitch_min != 0.0
        || k.hip_gun_kick_pitch_max != 0.0
        || k.ads_gun_kick_pitch_min != 0.0
        || k.ads_gun_kick_pitch_max != 0.0
        || k.hip_gun_kick_accel != 0.0
        || k.ads_gun_kick_accel != 0.0
}

fn sway_body_captured(s: &WeaponSwayFacts) -> bool {
    s.sway_max_angle != 0.0
        || s.sway_lerp_speed != 0.0
        || s.sway_pitch_scale != 0.0
        || s.sway_yaw_scale != 0.0
        || s.ads_sway_max_angle != 0.0
        || s.ads_sway_lerp_speed != 0.0
}

fn stance_ofs_captured(duck: &[f32; 3], prone: &[f32; 3]) -> bool {
    duck.iter().any(|v| *v != 0.0) || prone.iter().any(|v| *v != 0.0)
}

fn movement_ofs_captured(m: &WeaponMovementOfsInputs) -> bool {
    m.stand_move_at_0x138.iter().any(|v| *v != 0.0)
        || m.stand_rot_at_0x144.iter().any(|v| *v != 0.0)
        || m.strafe_move_at_0x150.iter().any(|v| *v != 0.0)
        || m.strafe_rot_at_0x15c.iter().any(|v| *v != 0.0)
        || m.ducked_move_at_0x174.iter().any(|v| *v != 0.0)
        || m.ducked_rot_at_0x180.iter().any(|v| *v != 0.0)
        || m.prone_move_at_0x198.iter().any(|v| *v != 0.0)
        || m.prone_rot_at_0x1a4.iter().any(|v| *v != 0.0)
        || m.pos_move_rate_at_0x1b0 != 0.0
        || m.pos_prone_move_rate_at_0x1b4 != 0.0
        || m.stand_move_min_speed_at_0x1b8 != 0.0
        || m.ducked_move_min_speed_at_0x1bc != 0.0
        || m.prone_move_min_speed_at_0x1c0 != 0.0
        || m.pos_rot_rate_at_0x1c4 != 0.0
        || m.pos_prone_rot_rate_at_0x1c8 != 0.0
}

fn movement_from_capture(c: WeaponMovementOfsCapture) -> WeaponMovementOfsInputs {
    WeaponMovementOfsInputs {
        stand_move_at_0x138: c.stand_move_at_0x138,
        stand_rot_at_0x144: c.stand_rot_at_0x144,
        strafe_move_at_0x150: c.strafe_move_at_0x150,
        strafe_rot_at_0x15c: c.strafe_rot_at_0x15c,
        ducked_move_at_0x174: c.ducked_move_at_0x174,
        ducked_rot_at_0x180: c.ducked_rot_at_0x180,
        prone_move_at_0x198: c.prone_move_at_0x198,
        prone_rot_at_0x1a4: c.prone_rot_at_0x1a4,
        pos_move_rate_at_0x1b0: c.pos_move_rate_at_0x1b0,
        pos_prone_move_rate_at_0x1b4: c.pos_prone_move_rate_at_0x1b4,
        stand_move_min_speed_at_0x1b8: c.stand_move_min_speed_at_0x1b8,
        ducked_move_min_speed_at_0x1bc: c.ducked_move_min_speed_at_0x1bc,
        prone_move_min_speed_at_0x1c0: c.prone_move_min_speed_at_0x1c0,
        pos_rot_rate_at_0x1c4: c.pos_rot_rate_at_0x1c4,
        pos_prone_rot_rate_at_0x1c8: c.pos_prone_rot_rate_at_0x1c8,
    }
}

fn idle_captured(i: &WeaponIdleInputs) -> bool {
    i.ads_idle_amount_at_0x36c != 0.0
        || i.hip_idle_amount_at_0x370 != 0.0
        || i.ads_idle_speed_at_0x374 != 0.0
        || i.hip_idle_speed_at_0x378 != 0.0
        || i.idle_crouch_factor_at_0x37c != 0.0
        || i.idle_prone_factor_at_0x380 != 0.0
}

fn idle_from_capture(c: WeaponIdleCapture) -> WeaponIdleInputs {
    WeaponIdleInputs {
        ads_idle_amount_at_0x36c: c.ads_idle_amount_at_0x36c,
        hip_idle_amount_at_0x370: c.hip_idle_amount_at_0x370,
        ads_idle_speed_at_0x374: c.ads_idle_speed_at_0x374,
        hip_idle_speed_at_0x378: c.hip_idle_speed_at_0x378,
        idle_crouch_factor_at_0x37c: c.idle_crouch_factor_at_0x37c,
        idle_prone_factor_at_0x380: c.idle_prone_factor_at_0x380,
    }
}

fn merge_body_facts(dst: &mut WeaponBodyFacts, src: WeaponBodyFacts) {
    if dst.fire_time_ms == 0 {
        dst.fire_time_ms = src.fire_time_ms;
    }
    if dst.impact_type == 0 && src.impact_type != 0 {
        dst.impact_type = src.impact_type;
    }

    if !dst.body_resolved && src.body_resolved {
        let fire_time_ms = dst.fire_time_ms;
        let ads_zoom_fov = dst.ads_zoom_fov;
        let ads_dof = dst.ads_dof;
        let ads_cs = dst.kick.f_ads_view_kick_center_speed;
        let hip_cs = dst.kick.f_hip_view_kick_center_speed;
        *dst = src;
        dst.fire_time_ms = fire_time_ms;
        dst.ads_zoom_fov = ads_zoom_fov;
        dst.ads_dof = ads_dof;

        dst.kick.f_ads_view_kick_center_speed = ads_cs;
        dst.kick.f_hip_view_kick_center_speed = hip_cs;
        return;
    }

    if dst.raise_time_ms == 0 {
        dst.raise_time_ms = src.raise_time_ms;
    }

    if !kick_body_captured(&dst.kick) && kick_body_captured(&src.kick) {
        let ads_cs = dst.kick.f_ads_view_kick_center_speed;
        let hip_cs = dst.kick.f_hip_view_kick_center_speed;
        dst.kick = src.kick;
        if ads_cs != 0.0 {
            dst.kick.f_ads_view_kick_center_speed = ads_cs;
        }
        if hip_cs != 0.0 {
            dst.kick.f_hip_view_kick_center_speed = hip_cs;
        }
    }
    if dst.kick.f_ads_view_kick_center_speed == 0.0 {
        dst.kick.f_ads_view_kick_center_speed = src.kick.f_ads_view_kick_center_speed;
    }
    if dst.kick.f_hip_view_kick_center_speed == 0.0 {
        dst.kick.f_hip_view_kick_center_speed = src.kick.f_hip_view_kick_center_speed;
    }
    if !sway_body_captured(&dst.sway) && sway_body_captured(&src.sway) {
        dst.sway = src.sway;
    }
    if !stance_ofs_captured(&dst.stance_ofs_at_0x168, &dst.stance_ofs_at_0x18c)
        && stance_ofs_captured(&src.stance_ofs_at_0x168, &src.stance_ofs_at_0x18c)
    {
        dst.stance_ofs_at_0x168 = src.stance_ofs_at_0x168;
        dst.stance_ofs_at_0x18c = src.stance_ofs_at_0x18c;
    }
    if dst.night_vision_wear_time == 0 {
        dst.night_vision_wear_time = src.night_vision_wear_time;
    }

    if !movement_ofs_captured(&dst.movement) && movement_ofs_captured(&src.movement) {
        dst.movement = src.movement;
    }
    if !idle_captured(&dst.idle) && idle_captured(&src.idle) {
        dst.idle = src.idle;
    }
    if dst.select_requires_ammo_at_0x667.is_none() {
        dst.select_requires_ammo_at_0x667 = src.select_requires_ammo_at_0x667;
        dst.quick_drop_time_ms = src.quick_drop_time_ms;
    }
    if dst.offhand_hold_is_cancelable_at_0x681.is_none() {
        dst.offhand_hold_is_cancelable_at_0x681 = src.offhand_hold_is_cancelable_at_0x681;
    }
    if dst.drop_time_ms == 0 {
        dst.drop_time_ms = src.drop_time_ms;
    }
    if dst.offhand_class == 0 && src.offhand_class != 0 {
        dst.offhand_class = src.offhand_class;
    }
    if dst.move_speed_scale == 0.0 {
        dst.move_speed_scale = src.move_speed_scale;
    }
    if dst.ads_move_speed_scale == 0.0 {
        dst.ads_move_speed_scale = src.ads_move_speed_scale;
    }
    if dst.sprint_duration_scale == 0.0 {
        dst.sprint_duration_scale = src.sprint_duration_scale;
    }

    if dst.clip_size == 0 {
        dst.clip_size = src.clip_size;
    }
    if dst.fire_type == 0 && src.fire_type != 0 {
        dst.fire_type = src.fire_type;
    }
    if dst.max_ammo == 0 {
        dst.max_ammo = src.max_ammo;
    }
    if dst.damage == 0 {
        dst.damage = src.damage;
    }
    if dst.rechamber_time_ms == 0 {
        dst.rechamber_time_ms = src.rechamber_time_ms;
    }
    if dst.reload_time_ms == 0 {
        dst.reload_time_ms = src.reload_time_ms;
    }
    if dst.reload_show_rocket_time_ms == 0 {
        dst.reload_show_rocket_time_ms = src.reload_show_rocket_time_ms;
    }
    if dst.reload_empty_time_ms == 0 {
        dst.reload_empty_time_ms = src.reload_empty_time_ms;
    }
    if dst.reload_add_time_ms == 0 {
        dst.reload_add_time_ms = src.reload_add_time_ms;
    }
    if dst.reload_empty_add_time_ms == 0 {
        dst.reload_empty_add_time_ms = src.reload_empty_add_time_ms;
    }
    if dst.reload_start_time_ms == 0 {
        dst.reload_start_time_ms = src.reload_start_time_ms;
    }
    if dst.reload_start_add_time_ms == 0 {
        dst.reload_start_add_time_ms = src.reload_start_add_time_ms;
    }
    if dst.reload_end_time_ms == 0 {
        dst.reload_end_time_ms = src.reload_end_time_ms;
    }
    if dst.reload_ammo_add == 0 {
        dst.reload_ammo_add = src.reload_ammo_add;
    }
    if dst.reload_start_add == 0 {
        dst.reload_start_add = src.reload_start_add;
    }
    if dst.fuse_time_ms == 0 {
        dst.fuse_time_ms = src.fuse_time_ms;
    }
    if dst.sprint_raise_time_ms == 0 {
        dst.sprint_raise_time_ms = src.sprint_raise_time_ms;
    }
    if dst.sprint_loop_time_ms == 0 {
        dst.sprint_loop_time_ms = src.sprint_loop_time_ms;
    }
    if dst.sprint_drop_time_ms == 0 {
        dst.sprint_drop_time_ms = src.sprint_drop_time_ms;
    }
    if dst.hold_fire_time_ms == 0 {
        dst.hold_fire_time_ms = src.hold_fire_time_ms;
    }
    if !dst.cook_off_hold {
        dst.cook_off_hold = src.cook_off_hold;
    }
    if !dst.clip_only {
        dst.clip_only = src.clip_only;
    }
    if !dst.proj_impact_explode {
        dst.proj_impact_explode = src.proj_impact_explode;
    }
    if !dst.stick_to_players {
        dst.stick_to_players = src.stick_to_players;
    }
    if !dst.ads_fire_only {
        dst.ads_fire_only = src.ads_fire_only;
    }
    if dst.explosion_radius == 0 {
        dst.explosion_radius = src.explosion_radius;
    }
    if dst.explosion_radius_min == 0 {
        dst.explosion_radius_min = src.explosion_radius_min;
    }
    if dst.explosion_inner_damage == 0 {
        dst.explosion_inner_damage = src.explosion_inner_damage;
    }
    if dst.explosion_outer_damage == 0 {
        dst.explosion_outer_damage = src.explosion_outer_damage;
    }
    if dst.projectile_speed == 0 {
        dst.projectile_speed = src.projectile_speed;
    }
    if dst.projectile_speed_up == 0 {
        dst.projectile_speed_up = src.projectile_speed_up;
    }
    if dst.projectile_activate_dist == 0 {
        dst.projectile_activate_dist = src.projectile_activate_dist;
    }
    if dst.projectile_explosion_type == 0 {
        dst.projectile_explosion_type = src.projectile_explosion_type;
    }
    if dst.parallel_bounce.is_none() {
        dst.parallel_bounce = src.parallel_bounce;
    }
    if dst.perpendicular_bounce.is_none() {
        dst.perpendicular_bounce = src.perpendicular_bounce;
    }
    if dst.penetrate_type == 0 && src.penetrate_type != 0 {
        dst.penetrate_type = src.penetrate_type;
    }
    if dst.penetrate_multiplier == 0.0 && src.penetrate_multiplier != 0.0 {
        dst.penetrate_multiplier = src.penetrate_multiplier;
    }
    if !dst.rifle_bullet && src.rifle_bullet {
        dst.rifle_bullet = true;
    }
    if dst.inventory_type == 0 && src.inventory_type != 0 {
        dst.inventory_type = src.inventory_type;
    }
    if dst.start_ammo == 0 {
        dst.start_ammo = src.start_ammo;
    }
    if !dst.ammo_count_clip_relative && src.ammo_count_clip_relative {
        dst.ammo_count_clip_relative = true;
    }
    if dst.min_damage == 0 {
        dst.min_damage = src.min_damage;
    }
    if dst.min_player_damage == 0 {
        dst.min_player_damage = src.min_player_damage;
    }
    if dst.max_damage_range == 0.0 {
        dst.max_damage_range = src.max_damage_range;
    }
    if dst.min_damage_range == 0.0 {
        dst.min_damage_range = src.min_damage_range;
    }
    if !dst.inherits_perks && src.inherits_perks {
        dst.inherits_perks = true;
    }
    if !dst.no_partial_reload && src.no_partial_reload {
        dst.no_partial_reload = true;
    }
    if !dst.segmented_reload && src.segmented_reload {
        dst.segmented_reload = true;
    }
    if dst.rechamber_bolt_delay_ms == 0 && src.rechamber_bolt_delay_ms != 0 {
        dst.rechamber_bolt_delay_ms = src.rechamber_bolt_delay_ms;
    }
    if !dst.rechamber_while_ads && src.rechamber_while_ads {
        dst.rechamber_while_ads = true;
    }
    if dst.dual_wield_view_model_offset == 0.0 {
        dst.dual_wield_view_model_offset = src.dual_wield_view_model_offset;
    }
    if dst.overlay_reticle == 0 {
        dst.overlay_reticle = src.overlay_reticle;
        if dst.ads_overlay_width == 0.0 {
            dst.ads_overlay_width = src.ads_overlay_width;
        }
        if dst.ads_overlay_height == 0.0 {
            dst.ads_overlay_height = src.ads_overlay_height;
        }
    }
    if dst.i_reticle_side_size == 0 {
        dst.i_reticle_side_size = src.i_reticle_side_size;
    }
    if dst.i_reticle_min_ofs == 0 {
        dst.i_reticle_min_ofs = src.i_reticle_min_ofs;
    }
}

#[derive(Clone, Debug)]
struct WeaponRow {
    name: String,

    namespace: crate::AssetNamespace,

    facts: WeaponBodyFacts,

    weap_def: Option<(u8, u32)>,

    gun_xmodel: Option<String>,

    hand_xmodel: Option<String>,

    gun_xmodel_edge: AssetEdge<FpvMeshSpace>,

    hand_xmodel_edge: AssetEdge<FpvMeshSpace>,

    gun_xmodel_from_zone: bool,

    world_model: Option<String>,

    world_model_from_zone: bool,

    world_model_edge: AssetEdge<WorldWeaponSpace>,

    projectile_model: Option<String>,

    projectile_model_edge: AssetEdge<ProjectileModelSpace>,

    rocket_model: Option<String>,

    sz_xanims: [Option<String>; WEAPON_ANIM_COUNT],

    sz_xanim_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_COUNT],

    sz_xanims_right: [Option<String>; WEAPON_ANIM_COUNT],

    sz_xanims_left: [Option<String>; WEAPON_ANIM_COUNT],

    hide_tags: Vec<String>,

    scope_viewmodel: Option<String>,

    sounds: WeaponSoundAliases,

    sound_edges: [AssetEdge<SoundAliasSpace>; WEAPON_SOUND_SLOT_COUNT],

    bounce_sound_edges: [AssetEdge<SoundAliasSpace>; SURF_TYPE_NUM],

    combat_fx: WeaponCombatFx,

    reticle: WeaponReticleAssets,

    hud_material_edges: WeaponHudMaterialEdges,

    overlay_material: Option<String>,
    overlay_image: Option<String>,
    overlay_material_from_slot: bool,

    hud_icon: Option<String>,
    hud_icon_from_slot: bool,
    pickup_icon: Option<String>,
    pickup_icon_image: Option<String>,
    pickup_icon_authored: bool,
    pickup_icon_ratio: i32,
    hud_icon_ratio: i32,

    hud_icon_image: Option<String>,

    dpad_icon_image: Option<String>,
    dpad_icon_atlas: Option<[u8; 2]>,
    dpad_icon_ratio: i32,
    kill_icon: Option<String>,
    kill_icon_from_slot: bool,
    kill_icon_image: Option<String>,

    projectile_fx: WeaponProjectileFx,

    proj_trail: Option<String>,
    proj_trail_from_slot: bool,
    proj_beacon: Option<String>,
    proj_beacon_from_slot: bool,
    proj_ignition: Option<String>,
    proj_ignition_from_slot: bool,

    display_name_key: Option<String>,
}

impl Default for WeaponRow {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: crate::AssetNamespace::Iw4,
            facts: WeaponBodyFacts::default(),
            weap_def: None,
            gun_xmodel: None,
            hand_xmodel: None,
            gun_xmodel_edge: AssetEdge::Absent,
            hand_xmodel_edge: AssetEdge::Absent,
            gun_xmodel_from_zone: false,
            world_model: None,
            world_model_from_zone: false,
            world_model_edge: AssetEdge::Absent,
            projectile_model: None,
            projectile_model_edge: AssetEdge::Absent,
            rocket_model: None,
            sz_xanims: [const { None }; WEAPON_ANIM_COUNT],
            sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_COUNT],
            sz_xanims_right: [const { None }; WEAPON_ANIM_COUNT],
            sz_xanims_left: [const { None }; WEAPON_ANIM_COUNT],
            hide_tags: Vec::new(),
            scope_viewmodel: None,
            sounds: WeaponSoundAliases::default(),
            sound_edges: absent_weapon_sound_edges(),
            bounce_sound_edges: absent_bounce_edges(),
            combat_fx: WeaponCombatFx::default(),
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            overlay_material_from_slot: false,
            hud_icon: None,
            hud_icon_from_slot: false,
            pickup_icon: None,
            pickup_icon_image: None,
            pickup_icon_authored: false,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_from_slot: false,
            kill_icon_image: None,
            projectile_fx: WeaponProjectileFx::default(),
            proj_trail: None,
            proj_trail_from_slot: false,
            proj_beacon: None,
            proj_beacon_from_slot: false,
            proj_ignition: None,
            proj_ignition_from_slot: false,
            display_name_key: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponRegistry {
    rows: Vec<WeaponRow>,

    by_name: HashMap<String, u32>,

    by_namespaced: HashMap<(crate::AssetNamespace, String), u32>,

    item_groups: HashMap<(crate::AssetNamespace, String), String>,

    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownWeaponName {
    pub name: String,
}

impl core::fmt::Display for UnknownWeaponName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown weapon name `{}`", self.name)
    }
}

impl std::error::Error for UnknownWeaponName {}

impl WeaponRegistry {
    fn from_catalog(mut entries: Vec<CatalogWeapon>) -> Self {
        let mut gun_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut hand_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut world_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut projectile_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut rocket_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut sounds_by_def: HashMap<(u8, u32), WeaponSoundAliases> = HashMap::new();
        let mut combat_fx_by_def: HashMap<(u8, u32), WeaponCombatFx> = HashMap::new();
        let mut facts_by_def: HashMap<(u8, u32), WeaponBodyFacts> = HashMap::new();
        let mut right_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_COUNT]> =
            HashMap::new();
        let mut left_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_COUNT]> =
            HashMap::new();
        for entry in &entries {
            if let (Some(key), Some(gun)) = (entry.weap_def, entry.gun_xmodel.as_ref()) {
                gun_by_def.entry(key).or_insert_with(|| gun.clone());
            }
            if let (Some(key), Some(hand)) = (entry.weap_def, entry.hand_xmodel.as_ref()) {
                hand_by_def.entry(key).or_insert_with(|| hand.clone());
            }
            if let (Some(key), Some(world)) = (entry.weap_def, entry.world_model.as_ref()) {
                world_by_def.entry(key).or_insert_with(|| world.clone());
            }
            if let (Some(key), Some(proj)) = (entry.weap_def, entry.projectile_model.as_ref()) {
                projectile_by_def.entry(key).or_insert_with(|| proj.clone());
            }
            if let (Some(key), Some(rocket)) = (entry.weap_def, entry.rocket_model.as_ref()) {
                rocket_by_def.entry(key).or_insert_with(|| rocket.clone());
            }
            if let Some(key) = entry.weap_def {
                merge_sound_aliases(sounds_by_def.entry(key).or_default(), &entry.sounds);
                merge_combat_fx(combat_fx_by_def.entry(key).or_default(), &entry.combat_fx);
                let slot = facts_by_def.entry(key).or_default();
                merge_body_facts(slot, entry.facts);
                if xanims_idle(&entry.sz_xanims_right).is_some() {
                    right_by_def
                        .entry(key)
                        .or_insert_with(|| entry.sz_xanims_right.clone());
                }
                if xanims_idle(&entry.sz_xanims_left).is_some() {
                    left_by_def
                        .entry(key)
                        .or_insert_with(|| entry.sz_xanims_left.clone());
                }
            }
        }
        for entry in &mut entries {
            if entry.gun_xmodel.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.gun_xmodel = gun_by_def.get(&key).cloned();
                }
            }
            if entry.hand_xmodel.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.hand_xmodel = hand_by_def.get(&key).cloned();
                }
            }
            if entry.world_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.world_model = world_by_def.get(&key).cloned();
                }
            }
            if entry.projectile_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.projectile_model = projectile_by_def.get(&key).cloned();
                }
            }
            if entry.rocket_model.is_none() {
                if let Some(key) = entry.weap_def {
                    entry.rocket_model = rocket_by_def.get(&key).cloned();
                }
            }
            if let Some(key) = entry.weap_def {
                if let Some(shared) = sounds_by_def.get(&key) {
                    merge_sound_aliases(&mut entry.sounds, shared);
                }
                if let Some(shared) = combat_fx_by_def.get(&key) {
                    merge_combat_fx(&mut entry.combat_fx, shared);
                }
                if let Some(&shared) = facts_by_def.get(&key) {
                    merge_body_facts(&mut entry.facts, shared);
                }
                if xanims_idle(&entry.sz_xanims_right).is_none() {
                    if let Some(shared) = right_by_def.get(&key) {
                        entry.sz_xanims_right = shared.clone();
                    }
                }
                if xanims_idle(&entry.sz_xanims_left).is_none() {
                    if let Some(shared) = left_by_def.get(&key) {
                        entry.sz_xanims_left = shared.clone();
                    }
                }
            }
        }

        let mut by_name: HashMap<String, CatalogWeapon> = HashMap::new();
        for entry in entries {
            let key = normalize_weapon_name(&entry.name);
            match by_name.entry(key) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    let existing = slot.get_mut();
                    if existing.gun_xmodel.is_none() {
                        existing.gun_xmodel = entry.gun_xmodel;
                    }
                    if existing.hand_xmodel.is_none() {
                        existing.hand_xmodel = entry.hand_xmodel;
                    }
                    if existing.world_model.is_none() {
                        existing.world_model = entry.world_model;
                    }
                    if existing.projectile_model.is_none() {
                        existing.projectile_model = entry.projectile_model;
                    }
                    if existing.rocket_model.is_none() {
                        existing.rocket_model = entry.rocket_model;
                    }
                    if existing.overlay_material.is_none() {
                        existing.overlay_material = entry.overlay_material;
                        existing.overlay_image = entry.overlay_image;
                        existing.overlay_material_slot = entry.overlay_material_slot;
                    }
                    if existing.hud_icon.is_none() {
                        existing.hud_icon = entry.hud_icon;
                        existing.hud_icon_slot = entry.hud_icon_slot;
                        existing.hud_icon_image = entry.hud_icon_image;
                    }
                    if existing.reticle.center_slot.is_none()
                        && existing.reticle.side_slot.is_none()
                        && (entry.reticle.center_slot.is_some()
                            || entry.reticle.side_slot.is_some())
                    {
                        existing.reticle = entry.reticle;
                    }
                    if existing.kill_icon.is_none() {
                        existing.kill_icon = entry.kill_icon;
                        existing.kill_icon_slot = entry.kill_icon_slot;
                        existing.kill_icon_image = entry.kill_icon_image;
                    }
                    if existing.proj_trail.is_none() {
                        existing.proj_trail = entry.proj_trail;
                        existing.proj_trail_slot = entry.proj_trail_slot;
                        existing.projectile_fx.trail = entry.projectile_fx.trail;
                    }
                    if existing.proj_beacon.is_none() {
                        existing.proj_beacon = entry.proj_beacon;
                        existing.proj_beacon_slot = entry.proj_beacon_slot;
                        existing.projectile_fx.beacon = entry.projectile_fx.beacon;
                    }
                    if existing.proj_ignition.is_none() {
                        existing.proj_ignition = entry.proj_ignition;
                        existing.proj_ignition_slot = entry.proj_ignition_slot;
                        existing.projectile_fx.ignition = entry.projectile_fx.ignition;
                    }
                    merge_sz_xanims(&mut existing.sz_xanims, entry.sz_xanims);
                    merge_sz_xanims(&mut existing.sz_xanims_right, entry.sz_xanims_right);
                    merge_sz_xanims(&mut existing.sz_xanims_left, entry.sz_xanims_left);
                    if existing.hide_tags.is_empty() && !entry.hide_tags.is_empty() {
                        existing.hide_tags = entry.hide_tags;
                    }
                    if existing.scope_viewmodel.is_none() {
                        existing.scope_viewmodel = entry.scope_viewmodel;
                    }
                    merge_sound_aliases(&mut existing.sounds, &entry.sounds);
                    merge_combat_fx(&mut existing.combat_fx, &entry.combat_fx);
                    merge_body_facts(&mut existing.facts, entry.facts);
                }
            }
        }
        let mut names: Vec<String> = by_name.keys().cloned().collect();
        names.sort_unstable();

        let mut rows = Vec::with_capacity(names.len() + 1);
        let mut index_of = HashMap::with_capacity(names.len());
        rows.push(WeaponRow::default());
        for name in names {
            let entry = by_name.remove(&name).expect("key from map");
            let id = rows.len() as u32;
            index_of.insert(name.clone(), id);
            rows.push(WeaponRow {
                name,
                namespace: crate::AssetNamespace::Iw4,
                facts: entry.facts,
                weap_def: entry.weap_def,
                gun_xmodel_from_zone: entry.gun_xmodel.is_some(),
                gun_xmodel: entry.gun_xmodel,
                hand_xmodel: entry.hand_xmodel,
                gun_xmodel_edge: AssetEdge::Absent,
                hand_xmodel_edge: AssetEdge::Absent,
                world_model_from_zone: entry.world_model.is_some(),
                world_model: entry.world_model,
                world_model_edge: AssetEdge::Absent,
                projectile_model: entry.projectile_model,
                projectile_model_edge: AssetEdge::Absent,
                rocket_model: entry.rocket_model,
                sz_xanims: entry.sz_xanims,
                sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_COUNT],
                sz_xanims_right: entry.sz_xanims_right,
                sz_xanims_left: entry.sz_xanims_left,
                hide_tags: entry.hide_tags,
                scope_viewmodel: entry.scope_viewmodel,
                sounds: entry.sounds,
                sound_edges: absent_weapon_sound_edges(),
                bounce_sound_edges: absent_bounce_edges(),
                combat_fx: entry.combat_fx,
                reticle: entry.reticle,
                hud_material_edges: entry.hud_material_edges,
                overlay_material_from_slot: entry.overlay_material_slot.is_some(),
                overlay_material: entry.overlay_material,
                overlay_image: entry.overlay_image,
                hud_icon_from_slot: entry.hud_icon_slot.is_some(),
                hud_icon: entry.hud_icon,
                hud_icon_image: entry.hud_icon_image,
                pickup_icon: entry.pickup_icon,
                pickup_icon_image: entry.pickup_icon_image,
                pickup_icon_authored: entry.pickup_icon_slot.is_some(),
                pickup_icon_ratio: entry.pickup_icon_ratio,
                hud_icon_ratio: entry.hud_icon_ratio,
                kill_icon_from_slot: entry.kill_icon_slot.is_some(),
                dpad_icon_image: entry.dpad_icon_image,
                dpad_icon_atlas: entry.dpad_icon_atlas,
                dpad_icon_ratio: entry.dpad_icon_ratio,
                kill_icon: entry.kill_icon,
                kill_icon_image: entry.kill_icon_image,
                projectile_fx: entry.projectile_fx,
                proj_trail_from_slot: entry.proj_trail_slot.is_some(),
                proj_trail: entry.proj_trail,
                proj_beacon_from_slot: entry.proj_beacon_slot.is_some(),
                proj_beacon: entry.proj_beacon,
                proj_ignition_from_slot: entry.proj_ignition_slot.is_some(),
                proj_ignition: entry.proj_ignition,
                display_name_key: entry.display_name_key,
            });
        }
        let mut built = Self {
            rows,
            by_name: index_of,
            by_namespaced: HashMap::new(),
            item_groups: HashMap::new(),
            revision: mint_weapon_revision(),
        };
        built.rebuild_name_maps();
        built
    }

    pub fn stamp_namespace(&mut self, ns: crate::AssetNamespace) {
        for row in self.rows.iter_mut().skip(1) {
            row.namespace = ns;
        }
        self.rebuild_name_maps();
        self.revision = mint_weapon_revision();
    }

    fn rebuild_name_maps(&mut self) {
        self.by_name.clear();
        self.by_namespaced.clear();
        for (id, row) in self.rows.iter().enumerate().skip(1) {
            self.by_namespaced
                .insert((row.namespace, row.name.clone()), id as u32);
            self.by_name.entry(row.name.clone()).or_insert(id as u32);
        }
    }

    pub fn absorb(&mut self, other: WeaponRegistry) {
        if other.is_empty() {
            return;
        }
        if self.rows.is_empty() {
            *self = other;
            return;
        }
        self.rows.extend(other.rows.into_iter().skip(1));
        self.item_groups.extend(other.item_groups);
        self.rebuild_name_maps();
        self.revision = mint_weapon_revision();
    }

    pub fn reticle_of(&self, index: u32) -> Option<&WeaponReticleAssets> {
        self.rows.get(index as usize).map(|row| &row.reticle)
    }

    pub fn resolve_hud_material_edges(&mut self, materials: &crate::MaterialCatalog) {
        for row in &mut self.rows {
            row.reticle.center_edge = material_hint_edge(
                row.reticle.center_material.as_deref(),
                row.reticle.center_slot.is_some(),
                materials,
            );
            row.reticle.side_edge = material_hint_edge(
                row.reticle.side_material.as_deref(),
                row.reticle.side_slot.is_some(),
                materials,
            );
            row.hud_material_edges = WeaponHudMaterialEdges {
                overlay: material_hint_edge(
                    row.overlay_material.as_deref(),
                    row.overlay_material_from_slot,
                    materials,
                ),
                hud_icon: material_hint_edge(
                    row.hud_icon.as_deref(),
                    row.hud_icon_from_slot,
                    materials,
                ),
                pickup_icon: material_hint_edge(
                    row.pickup_icon.as_deref(),
                    row.pickup_icon_authored,
                    materials,
                ),
                kill_icon: material_hint_edge(
                    row.kill_icon.as_deref(),
                    row.kill_icon_from_slot,
                    materials,
                ),
            };
        }
    }

    pub fn resolve_projectile_fx_edges(&mut self, fx: &crate::FxCatalog) {
        for row in &mut self.rows {
            row.projectile_fx = WeaponProjectileFx {
                trail: fx_hint_edge(row.proj_trail_from_slot, row.proj_trail.as_deref(), fx),
                beacon: fx_hint_edge(row.proj_beacon_from_slot, row.proj_beacon.as_deref(), fx),
                ignition: fx_hint_edge(
                    row.proj_ignition_from_slot,
                    row.proj_ignition.as_deref(),
                    fx,
                ),
            };
        }
    }

    pub fn projectile_fx_of(&self, index: u32) -> Option<WeaponProjectileFx> {
        self.rows.get(index as usize).map(|row| row.projectile_fx)
    }

    pub fn projectile_fx_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.projectile_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn resolve_sz_xanim_edges(&mut self, xanims: &crate::XAnimCatalog) {
        for row in &mut self.rows {
            let mut edges = [AssetEdge::Absent; WEAPON_ANIM_COUNT];
            for (edge, hint) in edges.iter_mut().zip(row.sz_xanims.iter()) {
                *edge = xanim_hint_edge(hint.as_deref(), row.namespace, xanims);
            }
            row.sz_xanim_edges = edges;
        }
    }

    pub fn sz_xanim_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_COUNT]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanim_edges)
    }

    pub fn sz_xanim_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in &row.sz_xanim_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn resolve_fpv_mesh_edges(&mut self, fpv: &crate::FpvMeshCatalog) {
        for row in &mut self.rows {
            row.gun_xmodel_edge = fpv_zone_hint_edge(
                row.gun_xmodel_from_zone,
                row.gun_xmodel.as_deref(),
                row.namespace,
                fpv,
            );
            row.hand_xmodel_edge = fpv_zone_hint_edge(
                row.hand_xmodel.is_some(),
                row.hand_xmodel.as_deref(),
                row.namespace,
                fpv,
            );
        }
    }

    pub fn gun_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows.get(index as usize).map(|row| row.gun_xmodel_edge)
    }

    pub fn hand_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.hand_xmodel_edge)
    }

    pub fn gun_xmodel_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.gun_xmodel_edge);
        }
        census
    }

    pub fn resolve_world_model_edges(&mut self, catalog: &crate::WorldWeaponCatalog) {
        for row in &mut self.rows {
            row.world_model_edge = world_zone_hint_edge(
                row.world_model_from_zone,
                row.world_model.as_deref(),
                catalog,
            );
        }
    }

    pub fn world_model_edge_of(&self, index: u32) -> Option<AssetEdge<WorldWeaponSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.world_model_edge)
    }

    pub fn world_model_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.world_model_edge);
        }
        census
    }

    pub fn world_model_entry<'a>(
        &self,
        index: u32,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        if let Some(order) = self.world_model_edge_of(index)?.bound_index() {
            if let Some(entry) = catalog.get_at(order) {
                return Some(entry);
            }
        }
        self.world_model_of(index)
            .and_then(|name| catalog.get(name))
    }

    pub fn resolve_weapon_sound_edges(&mut self, catalog: &crate::SoundCatalog) {
        for row in &mut self.rows {
            let mut edges = absent_weapon_sound_edges();
            for slot in WeaponSoundSlot::ALL {
                edges[slot as usize] =
                    bounce_hint_edge(row.sounds.hint(slot), row.namespace, catalog);
            }
            row.sound_edges = edges;
        }
    }

    pub fn weapon_sound_edge_of(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
    ) -> Option<AssetEdge<SoundAliasSpace>> {
        self.rows
            .get(index as usize)?
            .sound_edges
            .get(slot as usize)
            .copied()
    }

    pub fn weapon_sound_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in &row.sound_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn weapon_sound_alias<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        self.weapon_sound_edge_of(index, slot)?
            .bound_index()
            .and_then(|order| catalog.name_at(order))
    }

    pub fn weapon_sound_key<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::AssetNamespace, &'a str)> {
        let order = self.weapon_sound_edge_of(index, slot)?.bound_index()?;
        Some((catalog.namespace_of_alias(order), catalog.name_at(order)?))
    }

    pub fn resolve_bounce_sound_edges(&mut self, catalog: &crate::SoundCatalog) {
        for row in &mut self.rows {
            let mut edges = absent_bounce_edges();
            for (surf, hint) in row.sounds.bounce.iter().enumerate() {
                edges[surf] = bounce_hint_edge(hint.as_deref(), row.namespace, catalog);
            }
            row.bounce_sound_edges = edges;
        }
    }

    pub fn bounce_sound_edge_of(
        &self,
        index: u32,
        surf: usize,
    ) -> Option<AssetEdge<SoundAliasSpace>> {
        self.rows
            .get(index as usize)?
            .bounce_sound_edges
            .get(surf)
            .copied()
    }

    pub fn bounce_sound_alias<'a>(
        &self,
        index: u32,
        surf: usize,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        match self.bounce_sound_edge_of(index, surf) {
            Some(edge) => edge.bound_index().and_then(|order| catalog.name_at(order)),
            None => self.bounce_sound_of(index, surf).and_then(|name| {
                let ns = self.namespace_of(index).unwrap_or_default();
                catalog
                    .index_in(ns, name)
                    .and_then(|order| catalog.name_at(order))
            }),
        }
    }

    pub fn hud_material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in &self.rows {
            census.push(row.reticle.center_edge);
            census.push(row.reticle.side_edge);
        }
        for row in &self.rows {
            census.push(row.hud_material_edges.overlay);
            census.push(row.hud_material_edges.hud_icon);
            census.push(row.hud_material_edges.pickup_icon);
            census.push(row.hud_material_edges.kill_icon);
        }
        census
    }

    pub fn display_name_key_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.display_name_key.as_deref())
    }

    pub fn facts_of(&self, index: u32) -> Option<WeaponBodyFacts> {
        self.rows.get(index as usize).map(|row| row.facts)
    }

    pub fn resolve_index(&self, name: &str) -> Result<Option<u32>, UnknownWeaponName> {
        if name.is_empty() {
            return Ok(None);
        }
        if name.contains(':') {
            match crate::AssetKey::parse(name) {
                Ok(key) => return self.resolve_key(&key),
                Err(_) => {
                    return Err(UnknownWeaponName {
                        name: name.to_owned(),
                    });
                }
            }
        }
        let norm = normalize_weapon_name(name);
        if let Some(id) = self.by_name.get(&norm).copied() {
            return Ok(Some(id));
        }
        if let Some(id) = self.unique_suffix_id(&norm) {
            return Ok(Some(id));
        }
        match unique_weapon_stem(&self.rows, &norm) {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName {
                name: norm.to_owned(),
            }),
        }
    }

    pub fn resolve_key(&self, key: &crate::AssetKey) -> Result<Option<u32>, UnknownWeaponName> {
        if key.kind != crate::AssetKind::Weapon {
            return Err(UnknownWeaponName {
                name: key.display(),
            });
        }
        let norm = normalize_weapon_name(key.logical_name());
        if let Some(id) = self
            .by_namespaced
            .get(&(key.namespace, norm.clone()))
            .copied()
        {
            return Ok(Some(id));
        }
        match self.unique_id_in_namespace(key.namespace, &norm) {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName {
                name: key.display(),
            }),
        }
    }

    fn unique_id_in_namespace(&self, ns: crate::AssetNamespace, norm: &str) -> Option<u32> {
        if norm.is_empty() {
            return None;
        }
        let prefixed = format!("{}_{norm}", ns.as_str());
        let tail = format!("_{norm}");
        let mut hit = None;
        for id in 1..=self.len() as u32 {
            if self.namespace_of(id) != Some(ns) {
                continue;
            }
            let candidate = normalize_weapon_name(self.name_of(id));
            if candidate == norm || candidate == prefixed || candidate.ends_with(&tail) {
                if hit.is_some() {
                    return None;
                }
                hit = Some(id);
            }
        }
        hit
    }

    pub fn namespace_of(&self, index: u32) -> Option<crate::AssetNamespace> {
        if index == 0 {
            return None;
        }
        self.rows.get(index as usize).map(|row| row.namespace)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn item_group_of(&self, index: u32) -> Option<&str> {
        let ns = self.namespace_of(index)?;
        let name = self.name_of(index);
        if name.is_empty() {
            return None;
        }
        self.item_groups
            .get(&(ns, name.to_owned()))
            .map(String::as_str)
    }

    pub fn apply_stats_item_groups(&mut self, table: &crate::CapturedStringTable) {
        for id in 1..=self.len() as u32 {
            let Some(ns) = self.namespace_of(id) else {
                continue;
            };
            let name = self.name_of(id).to_owned();
            if name.is_empty() {
                continue;
            }
            if let Some(group) = crate::item_group_for_weapon(table, &name) {
                self.item_groups.insert((ns, name), group.to_owned());
            }
        }
    }

    pub fn apply_stats_tables<'a>(
        &mut self,
        tables: impl IntoIterator<Item = &'a crate::CapturedStringTable>,
    ) {
        for table in tables {
            if crate::is_stats_table_name(&table.name) {
                self.apply_stats_item_groups(table);
            }
        }
    }

    #[must_use]
    pub fn item_group_count(&self) -> usize {
        self.item_groups.len()
    }

    pub fn namespaced_key_of(&self, index: u32) -> Option<String> {
        self.key_of(index).map(|key| key.to_string())
    }

    pub fn key_of(&self, index: u32) -> Option<crate::AssetKey> {
        let ns = self.namespace_of(index)?;
        let name = self.name_of(index);
        if name.is_empty() {
            return None;
        }
        crate::AssetKey::new(ns, crate::AssetKind::Weapon, gsc_weapon_script_name(name)).ok()
    }

    pub fn namespace_count(&self, ns: crate::AssetNamespace) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.namespace == ns)
            .count()
    }

    #[deprecated(note = "use resolve_index — unknown must not become 0")]
    pub fn index_of(&self, name: &str) -> u32 {
        match self.resolve_index(name) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => 0,
        }
    }

    pub fn name_of(&self, index: u32) -> &str {
        self.rows
            .get(index as usize)
            .map(|row| row.name.as_str())
            .unwrap_or("")
    }

    fn unique_suffix_id(&self, stem: &str) -> Option<u32> {
        if stem.is_empty() {
            return None;
        }
        let prefixed = format!("iw5_{stem}");
        let tail = format!("_{stem}");
        let mut hit = None;
        for id in 1..=self.len() as u32 {
            let name = self.name_of(id);
            if name == stem || name == prefixed || name.ends_with(&tail) {
                if hit.is_some() {
                    return None;
                }
                hit = Some(id);
            }
        }
        hit
    }

    pub fn script_name_of(&self, index: u32) -> String {
        gsc_weapon_script_name(self.name_of(index))
    }

    pub fn script_names_table(&self) -> Vec<String> {
        (0..self.rows.len())
            .map(|i| self.script_name_of(i as u32))
            .collect()
    }

    pub fn configuration_supported(&self, id: u32) -> bool {
        if self.namespace_of(id) != Some(crate::AssetNamespace::T5) {
            return true;
        }
        let dual_hand_model = self
            .gun_xmodel_of(id)
            .is_some_and(|name| name.ends_with("_dw_rh") || name.ends_with("_dw_lh"));
        !dual_hand_model
    }

    pub fn loadout_catalog(&self) -> Vec<LoadoutCatalogRow> {
        let mut base_by_def: HashMap<(crate::AssetNamespace, (u8, u32)), u32> = HashMap::new();
        for id in 1..=self.len() as u32 {
            let Some(key) = self.weap_def_of(id) else {
                continue;
            };
            let namespace = self.namespace_of(id).expect("nonzero weapon row");
            let entry = base_by_def.entry((namespace, key)).or_insert(id);
            let candidate = self.name_of(id);
            let current = self.name_of(*entry);
            if (self.item_group_of(id).is_none(), candidate.len(), candidate)
                < (self.item_group_of(*entry).is_none(), current.len(), current)
            {
                *entry = id;
            }
        }
        (1..=self.len() as u32)
            .map(|id| {
                let facts = self.facts_of(id).unwrap_or_default();
                let category = self
                    .item_group_of(id)
                    .and_then(crate::cac_category_from_item_group);
                let kind = if !self.configuration_supported(id) {
                    LoadoutCatalogKind::NonPlayer
                } else if let Some(category) = category {
                    use crate::CacAuthoredCategory as C;
                    match category {
                        C::Pistol | C::MachinePistol | C::Projectile | C::Special => {
                            LoadoutCatalogKind::Secondary
                        }
                        C::Shotgun if self.namespace_of(id) == Some(crate::AssetNamespace::Iw4) => {
                            LoadoutCatalogKind::Secondary
                        }
                        _ => LoadoutCatalogKind::Primary,
                    }
                } else if facts.offhand_class != 0 {
                    LoadoutCatalogKind::Equipment {
                        offhand_class: facts.offhand_class,
                    }
                } else if let Some(base_id) = self
                    .weap_def_of(id)
                    .and_then(|key| base_by_def.get(&(self.namespace_of(id)?, key)).copied())
                    .filter(|base_id| *base_id != id)
                {
                    LoadoutCatalogKind::AttachmentVariant { base_id }
                } else if self.gun_xmodel_of(id).is_none() || facts.fire_time_ms <= 0 {
                    LoadoutCatalogKind::NonPlayer
                } else if matches!(facts.weap_class, 4 | 5 | 7) {
                    LoadoutCatalogKind::Secondary
                } else if matches!(facts.weap_class, 0..=3) {
                    LoadoutCatalogKind::Primary
                } else {
                    LoadoutCatalogKind::NonPlayer
                };
                LoadoutCatalogRow {
                    id,
                    key: self
                        .key_of(id)
                        .expect("nonzero registry rows have durable weapon keys"),
                    name: self.name_of(id).to_owned(),
                    kind,
                    weap_class: facts.weap_class,
                    item_group: self.item_group_of(id).map(str::to_owned),
                }
            })
            .collect()
    }

    pub fn gun_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.gun_xmodel.as_deref())
    }

    pub fn hand_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hand_xmodel.as_deref())
    }

    pub fn world_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.world_model.as_deref())
    }

    pub fn projectile_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.projectile_model.as_deref())
    }

    pub fn rocket_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.rocket_model.as_deref())
    }

    pub fn anim_of(&self, index: u32, slot: usize) -> Option<&str> {
        self.rows
            .get(index as usize)?
            .sz_xanims
            .get(slot)?
            .as_deref()
    }

    pub fn idle_anim_of(&self, index: u32) -> Option<&str> {
        self.anim_of(index, weap_anim::IDLE)
    }

    pub fn hide_tags_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map(|row| row.hide_tags.as_slice())
            .unwrap_or(&[])
    }

    pub fn scope_viewmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.scope_viewmodel.as_deref())
    }

    pub fn sounds_of(&self, index: u32) -> Option<&WeaponSoundAliases> {
        self.rows.get(index as usize).map(|row| &row.sounds)
    }

    pub fn bounce_sound_of(&self, index: u32, surf: usize) -> Option<&str> {
        self.sounds_of(index)?.bounce.get(surf)?.as_deref()
    }

    pub fn combat_fx_of(&self, index: u32) -> Option<&WeaponCombatFx> {
        self.rows.get(index as usize).map(|row| &row.combat_fx)
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        for row in self.rows.iter_mut().skip(1) {
            let combat = &mut row.combat_fx;
            stamp_fx_edge(
                combat.view_flash_slot,
                fx,
                &mut combat.view_flash,
                &mut combat.view_flash_hint,
            );
            stamp_fx_edge(
                combat.world_flash_slot,
                fx,
                &mut combat.world_flash,
                &mut combat.world_flash_hint,
            );
            stamp_fx_edge(
                combat.view_shell_eject_slot,
                fx,
                &mut combat.view_shell_eject,
                &mut combat.view_shell_eject_hint,
            );
            stamp_fx_edge(
                combat.world_shell_eject_slot,
                fx,
                &mut combat.world_shell_eject,
                &mut combat.world_shell_eject_hint,
            );
            stamp_fx_edge(
                combat.view_last_shot_eject_slot,
                fx,
                &mut combat.view_last_shot_eject,
                &mut combat.view_last_shot_eject_hint,
            );
            stamp_fx_edge(
                combat.world_last_shot_eject_slot,
                fx,
                &mut combat.world_last_shot_eject,
                &mut combat.world_last_shot_eject_hint,
            );
            stamp_fx_edge(
                combat.explosion_slot,
                fx,
                &mut combat.explosion,
                &mut combat.explosion_hint,
            );
            let tracer_name = combat.tracer_slot.and_then(|s| tracers.name_at_slot(s));
            combat.tracer_hint = tracer_name.map(str::to_owned);
            combat.tracer = match (combat.tracer_slot, tracer_name) {
                (None, _) => AssetEdge::Absent,
                (_, Some(name)) => match tracers.index_by_name(name) {
                    Some(index) => AssetEdge::bind_order(index, tracers.zone_of(index)),
                    None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                },
                (Some(_), None) => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
            };
        }
    }

    pub fn tracer_type_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.combat_fx.tracer);
        }
        census
    }

    pub fn combat_fx_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.combat_fx.fx_edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn combat_fx_unresolved_names(&self) -> Vec<&str> {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| {
                row.combat_fx
                    .fx_edges()
                    .iter()
                    .any(|edge| edge.is_unresolved())
            })
            .map(|row| row.name.as_str())
            .collect()
    }

    pub fn tracer_type_unresolved_names(&self) -> Vec<&str> {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.combat_fx.tracer.is_unresolved())
            .map(|row| row.name.as_str())
            .collect()
    }

    pub fn weap_def_of(&self, index: u32) -> Option<(u8, u32)> {
        self.rows.get(index as usize).and_then(|row| row.weap_def)
    }

    pub fn sz_xanims_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_COUNT]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanims)
    }

    pub fn sz_xanims_right_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_COUNT]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanims_right)
    }

    pub fn sz_xanims_left_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_COUNT]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanims_left)
    }

    pub fn idle_anim_right_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.sz_xanims_right[weap_anim::IDLE].as_deref())
            .filter(|s| !s.is_empty())
    }

    pub fn idle_anim_left_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.sz_xanims_left[weap_anim::IDLE].as_deref())
            .filter(|s| !s.is_empty())
    }

    pub fn timers_of(&self, index: u32) -> (i32, i32) {
        self.facts_of(index)
            .map(|f| (f.fire_time_ms, f.raise_time_ms))
            .unwrap_or((0, 0))
    }

    pub fn switch_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| (f.drop_time_ms, f.quick_drop_time_ms, f.quick_raise_time_ms))
            .unwrap_or((0, 0, 0))
    }

    pub fn sprint_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.sprint_raise_time_ms,
                    f.sprint_loop_time_ms,
                    f.sprint_drop_time_ms,
                )
            })
            .unwrap_or((0, 0, 0))
    }

    pub fn reload_timers_of(&self, index: u32) -> (i32, i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.reload_time_ms,
                    f.reload_empty_time_ms,
                    f.reload_start_time_ms,
                    f.reload_end_time_ms,
                )
            })
            .unwrap_or((0, 0, 0, 0))
    }

    pub fn len(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn scales_table(&self) -> Vec<(f32, f32, f32)> {
        self.rows
            .iter()
            .map(|row| {
                (
                    row.facts.move_speed_scale,
                    row.facts.ads_move_speed_scale,
                    row.facts.sprint_duration_scale,
                )
            })
            .collect()
    }

    pub fn gun_xmodel_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.gun_xmodel.is_some())
            .count()
    }

    pub fn gun_xmodel_zone_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.gun_xmodel.is_some() && row.gun_xmodel_from_zone)
            .count()
    }

    pub fn gun_xmodel_src_of(&self, index: u32) -> Option<&'static str> {
        let row = self.rows.get(index as usize)?;
        row.gun_xmodel.as_ref()?;
        Some(mesh_name_src(row.gun_xmodel_from_zone))
    }

    pub fn overlay_material_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_material.as_deref())
    }

    pub fn overlay_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_image.as_deref())
    }

    pub fn overlay_is_hud_iris(&self, index: u32) -> bool {
        self.overlay_material_of(index)
            .is_some_and(overlay_name_is_hud_iris)
    }

    pub fn pickup_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        if row.pickup_icon_authored {
            Some((row.pickup_icon_image.as_deref()?, row.pickup_icon_ratio))
        } else {
            Some((row.hud_icon_image.as_deref()?, row.hud_icon_ratio))
        }
    }

    pub fn hud_icon_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hud_icon.as_deref())
    }

    pub fn hud_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hud_icon_image.as_deref())
    }

    pub fn dpad_icon_of(&self, index: u32) -> Option<(&str, i32, [u8; 2])> {
        let row = self.rows.get(index as usize)?;
        Some((
            row.dpad_icon_image.as_deref()?,
            row.dpad_icon_ratio,
            row.dpad_icon_atlas?,
        ))
    }

    pub fn kill_icon_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon.as_deref())
    }

    pub fn kill_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon_image.as_deref())
    }

    pub fn proj_trail_of(&self, index: u32) -> Option<&str> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.trail.is_bound().then_some(())?;
        row.proj_trail.as_deref()
    }

    pub fn proj_beacon_of(&self, index: u32) -> Option<&str> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.beacon.is_bound().then_some(())?;
        row.proj_beacon.as_deref()
    }

    pub fn proj_ignition_of(&self, index: u32) -> Option<&str> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.ignition.is_bound().then_some(())?;
        row.proj_ignition.as_deref()
    }

    pub fn world_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.world_model.is_some())
            .count()
    }

    pub fn projectile_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some())
            .count()
    }

    pub fn stamp_projectile_model_edges(
        &mut self,
        catalog: &crate::ProjectileMeshCatalog,
        zone: ZoneOwner,
    ) {
        for row in &mut self.rows {
            row.projectile_model_edge = match row.projectile_model.as_deref() {
                None | Some("") => AssetEdge::Absent,
                Some(name) => match catalog.index_by_name(name) {
                    Some(order) => AssetEdge::bind_order(order, zone),
                    None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                },
            };
        }
    }

    pub fn projectile_model_edge_of(&self, index: u32) -> Option<AssetEdge<ProjectileModelSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.projectile_model_edge)
    }

    pub fn projectile_model_bound_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn projectile_model_name_hint_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some() && !row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn projectile_model_unresolved_hints(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter(|row| !row.projectile_model_edge.is_bound())
            .filter_map(|row| row.projectile_model.as_deref())
            .collect()
    }

    pub fn world_model_zone_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.world_model.is_some() && row.world_model_from_zone)
            .count()
    }

    pub fn world_model_src_of(&self, index: u32) -> Option<&'static str> {
        let row = self.rows.get(index as usize)?;
        row.world_model.as_ref()?;
        Some(mesh_name_src(row.world_model_from_zone))
    }

    pub fn idle_anim_count(&self) -> usize {
        (1..=self.len() as u32)
            .filter(|&id| self.idle_anim_of(id).is_some())
            .count()
    }

    pub fn sz_xanims_count(&self) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.sz_xanims.iter().any(|n| n.is_some()))
            .count()
    }

    pub fn fill_missing_gun_xmodels(&mut self, mut mesh_exists: impl FnMut(&str) -> bool) -> usize {
        let mut filled = 0usize;
        for row in self.rows.iter_mut().skip(1) {
            if row.gun_xmodel.is_some() {
                continue;
            }
            let Some(idle) = row.sz_xanims[weap_anim::IDLE].as_deref() else {
                continue;
            };
            if let Some(gun) = gun_candidates_from_idle(idle)
                .into_iter()
                .find(|c| mesh_exists(c))
            {
                row.gun_xmodel = Some(gun);
                row.gun_xmodel_from_zone = false;
                filled += 1;
            }
        }
        filled
    }

    pub fn fill_missing_world_models(
        &mut self,
        mut mesh_exists: impl FnMut(&str) -> bool,
    ) -> usize {
        let mut filled = 0usize;
        for row in self.rows.iter_mut().skip(1) {
            if row.world_model.is_some() {
                continue;
            }
            let mut candidates = Vec::new();
            if let Some(gun) = row.gun_xmodel.as_deref() {
                candidates.extend(world_candidates_from_viewmodel(gun));
            }
            if let Some(idle) = row.sz_xanims[weap_anim::IDLE].as_deref() {
                for gun in gun_candidates_from_idle(idle) {
                    candidates.extend(world_candidates_from_viewmodel(&gun));
                }
            }
            if !row.name.is_empty() {
                candidates.push(format!("weapon_{}", row.name));
                candidates.push(format!("weapon_{}_mp", row.name));
                candidates.push(format!("weapon_{}_tactical", row.name));
            }
            if let Some(world) = candidates.into_iter().find(|c| mesh_exists(c)) {
                row.world_model = Some(world);
                row.world_model_from_zone = false;
                filled += 1;
            }
        }
        filled
    }
}

fn mesh_name_src(from_zone: bool) -> &'static str {
    if from_zone { "zone" } else { "fill" }
}

pub fn gun_candidates_from_idle(idle: &str) -> Vec<String> {
    let Some(stem) = idle
        .strip_suffix("_idle")
        .or_else(|| idle.strip_suffix("_Idle"))
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(3);
    out.push(stem.to_owned());
    if let Some(base) = stem.strip_suffix("_tac") {
        out.push(base.to_owned());
    }
    if let Some(base) = stem.strip_suffix("_HB") {
        out.push(base.to_owned());
    }
    if let Some(base) = stem.strip_suffix("_hb") {
        out.push(base.to_owned());
    }
    out
}

pub fn world_candidates_from_viewmodel(gun: &str) -> Vec<String> {
    let Some(stem) = gun.strip_prefix("viewmodel_") else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(8);
    out.push(format!("weapon_{stem}"));
    out.push(format!("weapon_{stem}_mp"));

    out.push(format!("weapon_{stem}_tactical"));
    if let Some(base) = stem.strip_suffix("_tac") {
        out.push(format!("weapon_{base}"));
        out.push(format!("weapon_{base}_mp"));
        out.push(format!("weapon_{base}_tactical"));
    }
    if let Some(base) = stem.strip_suffix("_HB") {
        out.push(format!("weapon_{base}"));
    }
    if let Some(base) = stem.strip_suffix("_hb") {
        out.push(format!("weapon_{base}"));
    }
    out
}

fn normalize_weapon_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix("_mp").unwrap_or(&lower).to_owned()
}

fn unique_weapon_stem(rows: &[WeaponRow], query: &str) -> Option<u32> {
    if query.is_empty() {
        return None;
    }
    let mut hits: Vec<(u32, &str)> = rows
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, row)| row.name == query || row.name.starts_with(query))
        .map(|(i, row)| (i as u32, row.name.as_str()))
        .collect();
    if hits.is_empty() {
        return None;
    }
    if hits.len() == 1 {
        return Some(hits[0].0);
    }
    hits.sort_by_key(|(_, n)| (n.len(), *n));
    let (id, base) = hits[0];
    let all_are_base_or_attachment = hits.iter().all(|(_, n)| {
        *n == base || (n.starts_with(base) && n.as_bytes().get(base.len()) == Some(&b'_'))
    });
    all_are_base_or_attachment.then_some(id)
}

pub fn gsc_weapon_script_name(catalog_bare: &str) -> String {
    if catalog_bare.is_empty() {
        return String::new();
    }
    if catalog_bare.ends_with("_mp") {
        catalog_bare.to_owned()
    } else {
        format!("{catalog_bare}_mp")
    }
}
