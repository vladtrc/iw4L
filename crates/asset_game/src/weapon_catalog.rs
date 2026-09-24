use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn mint_weapon_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, FpvMeshSpace, FxSpace, MaterialSpace,
    ProjectileModelSpace, TracerSpace, WorldWeaponSpace, XAnimSpace, ZoneOwner,
};
use asset_iw4::size::{WEAPON_ANIM_COUNT, weap_anim};

use fastfile_iw4::{
    Ptr, ScriptStrings, WeaponIdleCapture, WeaponMovementOfsCapture, ZonePtr, ZoneStream,
};
use weapon_iw4::{WEAPON_ANIM_SLOTS, weap_anim_extra};
use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBodyFacts {
    pub body_resolved: bool,

    pub fire_time_ms: i32,

    pub impact_type: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,

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

    pub overlay_interface: i32,

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

    pub motion_tracker: bool,

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

    pub dual_mag: Option<weapon_iw4::DualMagTimes>,

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

    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
    pub location_damage_mult: Option<[f32; 20]>,
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

#[derive(Clone, Debug)]
pub struct CatalogWeapon {
    pub name: String,
    pub alternate_weapon: Option<String>,

    pub weap_def: Option<(u8, u32)>,

    pub display_name_key: Option<String>,

    pub reticle: WeaponReticleAssets,

    pub hud_material_edges: WeaponHudMaterialEdges,

    pub overlay_material: Option<String>,

    pub overlay_image: Option<String>,
    pub reticle_center_slot: Option<Ptr>,
    pub reticle_side_slot: Option<Ptr>,

    pub overlay_material_slot: Option<Ptr>,

    pub scope_name: Option<String>,

    pub scope_rows: [Iw5ScopeRow; 6],

    pub iw5_attachment_slots: [Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    pub iw5_reload_overrides: Vec<fastfile_iw5::ReloadOverride>,
    pub iw5_anim_overrides: Vec<LeftoverAnimOverride>,
    pub iw5_fx_overrides: Vec<Iw5FxOverride>,
    pub iw5_notetrack_overrides: Vec<Iw5NotetrackOverride>,

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

    pub sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    pub sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    pub hide_tags: Vec<String>,

    pub sounds: WeaponSoundAliases,

    pub combat_fx: WeaponCombatFx,

    pub(crate) combat_slots: CombatFxSlots,
    pub facts: WeaponBodyFacts,
}

#[derive(Clone, Debug, Default)]
pub struct Iw5ScopeRow {
    pub scope: Option<String>,
    pub display_name: Option<String>,
    pub attachment_type: i32,
    pub weapon_type: i32,
    pub weapon_class: i32,
    pub load_index: i32,
    pub overlay: Option<String>,
    pub overlay_lowres: Option<String>,
    pub overlay_emp: Option<String>,
    pub overlay_emp_lowres: Option<String>,
    pub view_model: Option<String>,
    pub world_model: Option<String>,
    pub view_models: [Option<String>; fastfile_iw5::size::ATTACH_MODEL_COUNT],
    pub world_models: [Option<String>; fastfile_iw5::size::ATTACH_MODEL_COUNT],
    pub reticle_models: [Option<String>; fastfile_iw5::size::ATTACH_RETICLE_COUNT],
    pub thermal: bool,
    pub width: f32,
    pub height: f32,
    pub ads_zoom_fov: f32,
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
    pub sight: Option<fastfile_iw5::AttachmentSight>,
    pub ammo_general: Option<fastfile_iw5::AttachmentAmmoGeneral>,
    pub reload: Option<fastfile_iw5::AttachmentReload>,
    pub add_ons: Option<fastfile_iw5::AttachmentAddOns>,
    pub general: Option<fastfile_iw5::AttachmentGeneral>,
    pub aim_assist: Option<fastfile_iw5::AttachmentAimAssist>,
    pub ammunition: Option<fastfile_iw5::AttachmentAmmunition>,
    pub damage: Option<fastfile_iw5::AttachmentDamage>,
    pub projectile: Option<fastfile_iw5::AttachmentProjectile>,
    pub projectile_model: Option<String>,
    pub reticle: Option<WeaponReticleAssets>,
    pub projectile_explosion_fx: Option<String>,
    pub projectile_trail_fx: Option<String>,
    pub projectile_ignition_fx: Option<String>,
    pub projectile_explosion_sound: Option<String>,
    pub projectile_ignition_sound: Option<String>,
    pub location_damage: Option<[f32; 19]>,
    pub idle_settings: Option<fastfile_iw5::AttachmentIdleSettings>,
    pub ads_settings: Option<fastfile_iw5::AttachmentAdsSettings>,
    pub ads_settings_main: Option<fastfile_iw5::AttachmentAdsSettings>,
    pub hip_spread: Option<fastfile_iw5::AttachmentHipSpread>,
    pub gun_kick: Option<fastfile_iw5::AttachmentGunKick>,
    pub view_kick: Option<[f32; 10]>,
    pub scales: fastfile_iw5::AttachmentScales,
    pub hide_iron_sights: bool,
    pub share_ammo_with_alt: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Iw5AttachmentSelection {
    pub scope: u8,
    pub underbarrel: u8,
    pub others: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Iw5ConfigurationCandidate {
    pub selection: crate::WeaponSelection,
    pub base_id: u32,
    pub native: Result<Iw5AttachmentSelection, crate::ConfigurationRefusal>,
    pub primary_assets: Vec<String>,
    pub primary_ads_zoom_fov: Option<f32>,
    pub primary_ads_aim_pitch: Option<f32>,
}

impl Iw5AttachmentSelection {
    pub fn fields(self) -> u16 {
        u16::from(self.scope) | (u16::from(self.underbarrel) << 3) | (u16::from(self.others) << 5)
    }

    fn override_candidates(self) -> [u16; 3] {
        let mut candidates = [0; 3];
        let mut count = 0;
        let mut push = |condition| {
            candidates[count.min(2)] = condition;
            count += 1;
        };
        if (1..=6).contains(&self.scope) {
            push(u16::from(self.scope));
        }
        if (1..=3).contains(&self.underbarrel) {
            push(u16::from(self.underbarrel) << 3);
        }
        for bit in 0..4 {
            if self.others & (1 << bit) != 0 {
                push(1 << (5 + bit));
            }
        }
        candidates
    }

    pub fn contains_condition(self, condition: u16) -> bool {
        if condition == 0 {
            return true;
        }
        self.override_candidates().contains(&condition)
    }
}

fn iw5_best_pair_override<'a, T>(
    rows: &'a [T],
    selection: Iw5AttachmentSelection,
    override_type: u32,
    fields: impl Fn(&T) -> (u16, u16, u32),
) -> Option<&'a T> {
    let candidates = selection.override_candidates();
    let mut best = None;
    let mut best_score = 0;
    for row in rows {
        let (first, second, row_type) = fields(row);
        if row_type != override_type {
            continue;
        }
        let score = u8::from(first != 0 && candidates.contains(&first))
            + u8::from(second != 0 && candidates.contains(&second));
        if score > best_score {
            best = Some(row);
            best_score = score;
        }
    }
    best
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

    /// Whether the zone authored a reticle material at all. The slot pointers
    /// that answered this during the walk stay on the build row: the HUD, and
    /// the edge stamping beside it, only ever asked whether there was one.
    pub center_authored: bool,

    pub side_authored: bool,
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

#[derive(Clone, Debug, Default)]
pub struct LinkedNotetrackAction {
    pub sound_alias: Option<String>,
    pub rumble_alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponDependencyGap {
    pub id: u32,
    pub kind: &'static str,
    pub name: String,
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
    pub alt_time_ms: i32,
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
pub struct Iw5FxOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub fx_type: u32,
    pub override_fx: Option<String>,
    pub altmode_fx: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Iw5NotetrackOverride {
    pub attachment: u16,
    pub sound_map: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CombatFxSlots {
    view_flash: Option<Ptr>,
    world_flash: Option<Ptr>,
    view_shell_eject: Option<Ptr>,
    world_shell_eject: Option<Ptr>,
    view_last_shot_eject: Option<Ptr>,
    world_last_shot_eject: Option<Ptr>,
    explosion: Option<Ptr>,
    tracer: Option<Ptr>,
}

impl CombatFxSlots {
    fn last_shot_pair_authored(self) -> bool {
        self.view_last_shot_eject.is_some() && self.world_last_shot_eject.is_some()
    }
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

    last_shot_eject_pair_authored: bool,
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
        self.last_shot_eject_pair_authored
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
        sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],
        fire_time_ms: i32,
        raise_time_ms: i32,
        move_speed_scale: f32,
        ads_move_speed_scale: f32,
    ) -> Self {
        Self {
            name: name.into(),
            alternate_weapon: None,
            weap_def,
            display_name_key: None,
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            reticle_center_slot: None,
            reticle_side_slot: None,
            overlay_material_slot: None,
            scope_name: None,
            scope_rows: Default::default(),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
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
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: Vec::new(),
            sounds: WeaponSoundAliases::default(),
            combat_fx: WeaponCombatFx::default(),
            combat_slots: CombatFxSlots::default(),
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
    iw5_attachments: HashMap<String, Iw5ScopeRow>,
}

impl WeaponCatalog {
    pub fn capture_iw5_attachment(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        geometry: &fastfile_iw5::AttachmentGeometry,
        fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
    ) {
        let Some(name) = geometry.name.and_then(|ptr| leftover_cstr_iw5(stream, ptr)) else {
            return;
        };
        let facts = &geometry.facts;
        let projectile_fx = |x86, x64| {
            let body = facts.projectile?.body;
            match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
                fastfile_iw5::ZonePtr::Offset(slot) => fx_name_at_slot(stream.resolve_alias(slot)),
                _ => None,
            }
        };
        let projectile_sound =
            |x86, x64| leftover_iw5_snd_alias(stream, facts.projectile?.body, x86, x64);
        let ads_settings = facts.ads_settings;
        self.iw5_attachments.insert(
            name.clone(),
            Iw5ScopeRow {
                scope: Some(name),
                display_name: facts
                    .display_name
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                attachment_type: facts.attachment_type,
                weapon_type: facts.weapon_type,
                weapon_class: facts.weapon_class,
                load_index: facts.load_index,
                overlay: geometry.overlay_names[0].and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                overlay_lowres: geometry.overlay_names[1]
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                overlay_emp: geometry.overlay_names[2]
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                overlay_emp_lowres: geometry.overlay_names[3]
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                view_model: geometry
                    .view_model_name
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                world_model: geometry
                    .world_model_name
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                view_models: geometry
                    .view_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                world_models: geometry
                    .world_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                reticle_models: geometry
                    .reticle_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                thermal: geometry.thermal,
                width: geometry.overlay_width,
                height: geometry.overlay_height,
                ads_zoom_fov: ads_settings.map_or(0.0, |ads| ads.ads_zoom_fov),
                ads_zoom_in_frac: ads_settings.map_or(0.0, |ads| ads.ads_zoom_in_frac),
                ads_zoom_out_frac: ads_settings.map_or(0.0, |ads| ads.ads_zoom_out_frac),
                sight: facts.sight,
                ammo_general: facts.ammo_general,
                reload: facts.reload,
                add_ons: facts.add_ons,
                general: facts.general,
                reticle: facts.general.and_then(|general| general.body).map(|body| {
                    let center = leftover_iw5_material_name(stream, Some(body), 8, 8);
                    let side = leftover_iw5_material_name(stream, Some(body), 12, 16);
                    WeaponReticleAssets {
                        center_authored: center.is_some(),
                        side_authored: side.is_some(),
                        center_material: center,
                        side_material: side,
                        center_size: stream.i32_at(body, stream.layout(16, 24)).unwrap_or(0),
                        side_size: stream.i32_at(body, stream.layout(20, 28)).unwrap_or(0),
                        ..Default::default()
                    }
                }),
                aim_assist: facts.aim_assist,
                ammunition: facts.ammunition,
                damage: facts.damage,
                projectile: facts.projectile,
                projectile_explosion_fx: projectile_fx(40, 48),
                projectile_trail_fx: projectile_fx(76, 104),
                projectile_ignition_fx: projectile_fx(84, 120),
                projectile_explosion_sound: projectile_sound(48, 64),
                projectile_ignition_sound: projectile_sound(88, 128),
                projectile_model: facts.projectile.and_then(|p| p.model).and_then(|model| {
                    match stream.ptr_at(model, 0).ok()? {
                        fastfile_iw5::ZonePtr::Offset(name) => {
                            leftover_cstr_iw5(stream, stream.resolve_alias(name))
                        }
                        _ => None,
                    }
                }),
                location_damage: facts.location_damage,
                idle_settings: facts.idle_settings,
                ads_settings,
                ads_settings_main: facts.ads_settings_main,
                hip_spread: facts.hip_spread,
                gun_kick: facts.gun_kick,
                view_kick: facts.view_kick,
                scales: facts.scales,
                hide_iron_sights: facts.hide_iron_sights,
                share_ammo_with_alt: facts.share_ammo_with_alt,
                ..Default::default()
            },
        );
    }

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
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let sz_xanims_right = geometry
            .sz_xanims_right
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let sz_xanims_left = geometry
            .sz_xanims_left
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let hide_tags = read_hide_tags(stream, &self.strings, geometry.hide_tags);
        self.entries.push(CatalogWeapon {
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| read_name(stream, p)),
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
                center_authored: geometry.reticle_center_material_slot.is_some(),
                side_authored: geometry.reticle_side_material_slot.is_some(),
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            reticle_center_slot: geometry.reticle_center_material_slot,
            reticle_side_slot: geometry.reticle_side_material_slot,
            overlay_material_slot: geometry.overlay_material_slot,
            scope_name: None,
            scope_rows: Default::default(),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
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
            combat_slots: CombatFxSlots {
                view_flash: geometry.view_flash_slot,
                world_flash: geometry.world_flash_slot,
                view_shell_eject: geometry.view_shell_eject_slot,
                world_shell_eject: geometry.world_shell_eject_slot,
                view_last_shot_eject: geometry.view_last_shot_eject_slot,
                world_last_shot_eject: geometry.world_last_shot_eject_slot,
                explosion: geometry.explosion_slot,
                tracer: geometry.tracer_slot,
            },
            combat_fx: WeaponCombatFx {
                last_shot_eject_pair_authored: geometry.view_last_shot_eject_slot.is_some()
                    && geometry.world_last_shot_eject_slot.is_some(),
                ..WeaponCombatFx::default()
            },
            facts: WeaponBodyFacts {
                body_resolved: geometry.weap_def.is_some(),
                fire_time_ms: geometry.fire_time_ms,
                impact_type: geometry.impact_type,
                raise_time_ms: geometry.raise_time_ms,
                drop_time_ms: geometry.drop_time_ms,
                alternate_raise_time_ms: geometry.alternate_raise_time_ms,
                alternate_drop_time_ms: geometry.alternate_drop_time_ms,
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
                overlay_interface: geometry.overlay_interface,
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
                motion_tracker: geometry.motion_tracker,
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
                dual_mag: None,
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
                timed_detonation: geometry.timed_detonation,
                proj_impact_explode: geometry.proj_impact_explode,
                stick_to_players: geometry.stick_to_players,
                explosion_radius: geometry.explosion_radius,
                explosion_radius_min: geometry.explosion_radius_min,
                explosion_inner_damage: geometry.explosion_inner_damage,
                explosion_outer_damage: geometry.explosion_outer_damage,
                projectile_speed: geometry.projectile_speed,
                projectile_speed_up: geometry.projectile_speed_up,
                projectile_speed_forward: geometry.projectile_speed_forward,
                projectile_activate_dist: geometry.projectile_activate_dist,
                projectile_explosion_type: geometry.projectile_explosion_type,
                parallel_bounce: geometry.parallel_bounce,
                perpendicular_bounce: geometry.perpendicular_bounce,
                location_damage_mult: geometry.location_damage_mult,
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
            if let Some(slot) = entry.reticle_center_slot {
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
            if let Some(slot) = entry.reticle_side_slot {
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
                entry.reticle.center_authored,
                materials,
            );
            entry.reticle.side_edge = material_hint_edge(
                entry.reticle.side_material.as_deref(),
                entry.reticle.side_authored,
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

    /// The half of reticle resolution that still has an answer once the walk is
    /// over: the slot lookups need the zone's pointer map, the name lookups do
    /// not. Running the slot half against a finished population was reading a
    /// map that `finalize` had already emptied, so every one of them was `None`.
    pub fn resolve_reticle_images(&mut self, materials: &crate::MaterialDefinitions) {
        for entry in &mut self.entries {
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
                entry.reticle.center_authored,
                materials,
            );
            entry.reticle.side_edge = material_hint_edge(
                entry.reticle.side_material.as_deref(),
                entry.reticle.side_authored,
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
            stamp_combat_fx(&mut entry.combat_fx, entry.combat_slots, fx, tracers);
        }
    }

    pub fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
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
        let scope_name = None;
        let scope_rows = std::array::from_fn(|index| {
            geometry.attachments[index]
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr))
                .and_then(|name| self.iw5_attachments.get(&name).cloned())
                .unwrap_or_default()
        });
        let iw5_attachment_slots = geometry
            .attachments
            .map(|name| name.and_then(|ptr| leftover_cstr_iw5(stream, ptr)));
        let iw5_reload_overrides = geometry.reload_overrides(stream).collect();
        let iw5_fx_overrides = read_iw5_fx_overrides(stream, &geometry, fx_name_at_slot);
        let iw5_notetrack_overrides = read_iw5_notetrack_overrides(stream, strings, &geometry);
        let mut sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_iw5(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let leftover_anim_overrides = leftover_iw5_anim_overrides(stream, &geometry);
        apply_leftover_default_anim_overrides(&mut sz_xanims, &leftover_anim_overrides);
        self.entries.push(CatalogWeapon {
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| leftover_cstr_iw5(stream, p)),
            reticle_center_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_RETICLE_CENTER_OFF,
                560,
            ),
            reticle_side_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_RETICLE_SIDE_OFF,
                568,
            ),
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
            scope_rows,
            iw5_attachment_slots,
            iw5_reload_overrides,
            iw5_anim_overrides: leftover_anim_overrides,
            iw5_fx_overrides,
            iw5_notetrack_overrides,
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
            kill_icon: geometry
                .kill_icon
                .and_then(|mat| leftover_xstring_at_iw5(stream, mat, 0, 0)),
            kill_icon_slot: geometry.kill_icon_slot.map(|slot| Ptr {
                block: slot.block,
                offset: slot.offset,
            }),
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
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: read_hide_tags_iw5(stream, strings, geometry.hide_tags),
            sounds: leftover_iw5_sounds(stream, strings, &geometry),
            combat_fx: read_iw5_combat_fx(stream, &geometry, fx_name_at_slot),
            combat_slots: CombatFxSlots::default(),
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
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        self.entries.push(CatalogWeapon {
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| stream.cstr(p).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle_center_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_RETICLE_CENTER_OFF,
            ),
            reticle_side_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_RETICLE_SIDE_OFF,
            ),
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
            scope_rows: Default::default(),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
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
            kill_icon: leftover_t5_material_name_opt(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_KILL_ICON_OFF,
            ),
            kill_icon_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_KILL_ICON_OFF,
            ),
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
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: read_hide_tags_t5(stream, strings, geometry.hide_tags),
            sounds: leftover_t5_sounds(stream, strings, &geometry),
            combat_fx: WeaponCombatFx::default(),
            combat_slots: CombatFxSlots::default(),
            facts: capture_t5_body_facts(stream, &geometry),
        });
        let last = self.entries.last_mut().expect("just pushed");
        let (fx, slots) = leftover_t5_combat_fx(stream, &geometry);
        last.combat_fx = fx;
        last.combat_slots = slots;
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

    pub fn into_build(self) -> WeaponBuild {
        WeaponBuild::from_catalog(self.entries, self.iw5_attachments)
    }
}

fn material_hint_edge(
    hint: Option<&str>,
    authored_slot: bool,
    materials: &crate::MaterialDefinitions,
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

fn stamp_combat_fx(
    combat: &mut WeaponCombatFx,
    slots: CombatFxSlots,
    fx: &crate::FxCatalog,
    tracers: &crate::TracerCatalog,
) {
    stamp_fx_edge(
        slots.view_flash,
        fx,
        &mut combat.view_flash,
        &mut combat.view_flash_hint,
    );
    stamp_fx_edge(
        slots.world_flash,
        fx,
        &mut combat.world_flash,
        &mut combat.world_flash_hint,
    );
    stamp_fx_edge(
        slots.view_shell_eject,
        fx,
        &mut combat.view_shell_eject,
        &mut combat.view_shell_eject_hint,
    );
    stamp_fx_edge(
        slots.world_shell_eject,
        fx,
        &mut combat.world_shell_eject,
        &mut combat.world_shell_eject_hint,
    );
    stamp_fx_edge(
        slots.view_last_shot_eject,
        fx,
        &mut combat.view_last_shot_eject,
        &mut combat.view_last_shot_eject_hint,
    );
    stamp_fx_edge(
        slots.world_last_shot_eject,
        fx,
        &mut combat.world_last_shot_eject,
        &mut combat.world_last_shot_eject_hint,
    );
    stamp_fx_edge(
        slots.explosion,
        fx,
        &mut combat.explosion,
        &mut combat.explosion_hint,
    );
    let tracer_name = slots.tracer.and_then(|s| tracers.name_at_slot(s));
    combat.tracer_hint = tracer_name.map(str::to_owned);
    combat.tracer = match (slots.tracer, tracer_name) {
        (None, _) => AssetEdge::Absent,
        (_, Some(name)) => match tracers.index_by_name(name) {
            Some(index) => AssetEdge::bind_order(index, tracers.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
        (Some(_), None) => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
    };
    combat.last_shot_eject_pair_authored = slots.last_shot_pair_authored();
}

fn fpv_model_edge(
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    fpv: &crate::FpvMeshCatalog,
) -> AssetEdge<FpvMeshSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match fpv.index_by_name(ns, name) {
            Some(index) => AssetEdge::bind_order(index, fpv.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn world_model_edge(
    hint: Option<&str>,
    catalog: &crate::WorldWeaponCatalog,
) -> AssetEdge<WorldWeaponSpace> {
    let hint = hint.filter(|name| !name.is_empty());
    match hint {
        None => AssetEdge::Absent,
        Some(name) => match catalog.index_by_name(name) {
            Some(index) => AssetEdge::bind_order(index, catalog.zone_of(index)),
            None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
        },
    }
}

fn sound_alias_in_bank<'a>(
    hint: Option<&str>,
    ns: crate::AssetNamespace,
    catalog: &'a crate::SoundCatalog,
) -> Option<(crate::AssetNamespace, &'a str)> {
    let name = hint.filter(|name| !name.is_empty())?;
    let order = catalog.index_in(ns, name)?;
    Some((catalog.namespace_of_alias(order), catalog.name_at(order)?))
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
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
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

fn remap_t5_sz_xanims(t5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    use fastfile_t5::size::weap_anim as t5_anim;
    const PAIRS: [(usize, usize); 34] = [
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
        (t5_anim::RELOAD_QUICK, weap_anim_extra::RELOAD_QUICK),
        (
            t5_anim::RELOAD_QUICK_EMPTY,
            weap_anim_extra::RELOAD_QUICK_EMPTY,
        ),
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
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
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
        center_authored: leftover_t5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
        )
        .is_some(),
        side_authored: leftover_t5_asset_slot(stream, weap_def, sz::WEAPON_DEF_RETICLE_SIDE_OFF)
            .is_some(),
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
) -> (WeaponCombatFx, CombatFxSlots) {
    use fastfile_t5::size as sz;
    let body = geometry.weap_def;
    let slots = CombatFxSlots {
        view_flash: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_VIEW_FLASH_OFF),
        world_flash: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_WORLD_FLASH_OFF),
        view_shell_eject: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF),
        world_shell_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF,
        ),
        view_last_shot_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF,
        ),
        world_last_shot_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF,
        ),
        explosion: None,
        tracer: None,
    };
    let fx = WeaponCombatFx {
        view_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_FLASH_OFF)),
        world_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_FLASH_OFF)),
        view_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF)),
        world_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF)),
        view_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF)
        }),
        world_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF)
        }),
        last_shot_eject_pair_authored: slots.last_shot_pair_authored(),
        ..WeaponCombatFx::default()
    };
    (fx, slots)
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
        alternate_raise_time_ms: geometry.alternate_raise_time_ms,
        alternate_drop_time_ms: geometry.alternate_drop_time_ms,
        raise_time_ms: geometry.raise_time_ms,
        bolt_action: geometry.bolt_action,
        select_requires_ammo_at_0x667: Some(leftover_t5_select_requires_ammo()),
        ..WeaponBodyFacts::default()
    };
    if let Some(body) = geometry.weap_def {
        facts.kill_icon_ratio = i32_at_t5(stream, body, sz::WEAPON_DEF_KILL_ICON_RATIO_OFF);
        facts.flip_kill_icon = u8_at_t5(stream, body, sz::WEAPON_DEF_FLIP_KILL_ICON_OFF) != 0;
    }
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
    if let Some(variant) = geometry.variant
        && u8_at_t5(stream, variant, sz::WEAPON_VARIANT_DUAL_MAG_OFF) != 0
    {
        facts.dual_mag = Some(weapon_iw4::DualMagTimes {
            reload_ms: i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_QUICK_TIME_OFF),
            reload_empty_ms: i32_at_t5(
                stream,
                variant,
                sz::WEAPON_VARIANT_RELOAD_QUICK_EMPTY_TIME_OFF,
            ),
            add_ms: i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_QUICK_ADD_TIME_OFF),
            empty_add_ms: i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_QUICK_EMPTY_ADD_TIME_OFF),
        });
    }
    facts.reload_ammo_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_AMMO_ADD_OFF);
    facts.reload_start_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_OFF);
    facts.overlay_reticle = i32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF);
    facts.overlay_interface = i32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_INTERFACE_OFF);
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
        overlay_interface: geometry.overlay_interface,
        ads_zoom_fov: geometry.ads_zoom_fov,
        ads_zoom_in_frac: geometry.ads_zoom_in_frac,
        ads_zoom_out_frac: geometry.ads_zoom_out_frac,
        ads_in_rate: geometry.ads_in_rate,
        ads_out_rate: geometry.ads_out_rate,
        impact_type: geometry.impact_type,
        penetrate_multiplier: geometry.penetrate_multiplier,
        motion_tracker: geometry.motion_tracker,
        kick: WeaponKickFacts {
            f_ads_view_kick_center_speed: geometry.ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: geometry.hip_view_kick_center_speed,
            ..Default::default()
        },
        ..WeaponBodyFacts::default()
    };
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.kill_icon_ratio = i32_at_iw5(stream, body, sz::WEAPON_DEF_KILL_ICON_RATIO_OFF, 1588);
    facts.flip_kill_icon = u8_at_iw5(stream, body, sz::WEAPON_DEF_FLIP_KILL_ICON_OFF, 2455) != 0;
    facts.ads_aim_pitch = f32_at_iw5(stream, body, 1400, 1816);
    facts.ads_crosshair_in_frac = f32_at_iw5(stream, body, 1404, 1820);
    facts.ads_crosshair_out_frac = f32_at_iw5(stream, body, 1408, 1824);
    facts.kick.gun_max_pitch = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF, 1496);
    facts.kick.gun_max_yaw = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_YAW_OFF, 1500);
    facts.kick.ads_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1828,
    );
    facts.kick.ads_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1832,
    );
    facts.kick.ads_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF,
        1836,
    );
    facts.kick.ads_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF,
        1840,
    );
    facts.kick.ads_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF, 1844);
    facts.kick.ads_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF, 1848);
    facts.kick.ads_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF, 1852);
    facts.kick.ads_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF,
        1856,
    );
    facts.kick.ads_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF,
        1860,
    );
    facts.kick.ads_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF,
        1864,
    );
    facts.kick.ads_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF,
        1868,
    );
    facts.kick.ads_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF,
        1872,
    );
    facts.kick.ads_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF, 1876);
    facts.kick.ads_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF, 1880);
    facts.kick.hip_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1896,
    );
    facts.kick.hip_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1900,
    );
    facts.kick.hip_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF,
        1904,
    );
    facts.kick.hip_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF,
        1908,
    );
    facts.kick.hip_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF, 1912);
    facts.kick.hip_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF, 1916);
    facts.kick.hip_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF, 1920);
    facts.kick.hip_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF,
        1924,
    );
    facts.kick.hip_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF,
        1928,
    );
    facts.kick.hip_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF,
        1932,
    );
    facts.kick.hip_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF,
        1936,
    );
    facts.kick.hip_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF,
        1940,
    );
    facts.kick.hip_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF, 1944);
    facts.kick.hip_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF, 1948);
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
    facts.reload_start_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF, 980);
    facts.reload_start_add_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF, 984);
    facts.reload_end_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF, 988);
    facts.alternate_raise_time_ms = geometry.alternate_raise_time_ms;
    facts.alternate_drop_time_ms = geometry.alternate_drop_time_ms;
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
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
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

fn remap_iw5_sz_xanims(iw5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
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
    None
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
        center_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        )
        .is_some(),
        side_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        )
        .is_some(),
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
    geometry
        .anim_overrides(stream)
        .map(|row| LeftoverAnimOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            anim_tree_type: row.anim_tree_type,
            override_anim: row
                .override_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_anim: row
                .altmode_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            anim_time_ms: row.anim_time_ms,
            alt_time_ms: row.alt_time_ms,
        })
        .collect()
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
    sz: &mut [Option<String>; WEAPON_ANIM_SLOTS],
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
    geometry
        .sound_overrides(stream)
        .map(|row| LeftoverSoundOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            sound_type: row.sound_type,
            override_sound: row
                .override_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_sound: row
                .altmode_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
        })
        .collect()
}

fn read_iw5_combat_fx(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
) -> WeaponCombatFx {
    use fastfile_iw5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponCombatFx::default();
    };
    let fx_name = |x86, x64| match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => fx_name_at_slot(stream.resolve_alias(q)),
        _ => None,
    };
    let view_last_shot_eject_hint = fx_name(sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF, 544);
    let world_last_shot_eject_hint = fx_name(sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF, 552);
    WeaponCombatFx {
        view_flash_hint: fx_name(sz::WEAPON_DEF_VIEW_FLASH_OFF, 112),
        world_flash_hint: fx_name(sz::WEAPON_DEF_WORLD_FLASH_OFF, 120),
        view_shell_eject_hint: fx_name(sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF, 528),
        world_shell_eject_hint: fx_name(sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF, 536),
        last_shot_eject_pair_authored: view_last_shot_eject_hint.is_some()
            && world_last_shot_eject_hint.is_some(),
        view_last_shot_eject_hint,
        world_last_shot_eject_hint,
        ..WeaponCombatFx::default()
    }
}

fn read_iw5_fx_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
) -> Vec<Iw5FxOverride> {
    geometry
        .fx_overrides(stream)
        .map(|row| Iw5FxOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            fx_type: row.fx_type,
            override_fx: row.override_fx.and_then(fx_name_at_slot),
            altmode_fx: row.altmode_fx.and_then(fx_name_at_slot),
        })
        .collect()
}

fn read_iw5_notetrack_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<Iw5NotetrackOverride> {
    geometry
        .note_track_overrides(stream)
        .map(|row| Iw5NotetrackOverride {
            attachment: row.attachment,
            sound_map: row
                .sound_map
                .map(|map| iw5_script_string_pairs(stream, strings, map, 24))
                .unwrap_or_default(),
        })
        .collect()
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
    fastfile_iw5::ScriptStringMap::at(stream, body, keys_off, values_off)
        .map(|map| iw5_script_string_pairs(stream, strings, map, cap))
        .unwrap_or_default()
}

fn iw5_script_string_pairs(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    map: fastfile_iw5::ScriptStringMap,
    cap: usize,
) -> Vec<(String, String)> {
    map.pairs(stream, cap)
        .filter_map(|(key_id, val_id)| {
            let key = strings.get(stream, key_id).filter(|s| !s.is_empty())?;
            let val = strings
                .get(stream, val_id)
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            Some((key.to_owned(), val.to_owned()))
        })
        .collect()
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

fn read_sz_xanims(stream: &ZoneStream<'_>, arr: Ptr) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (i, slot) in out.iter_mut().take(WEAPON_ANIM_COUNT).enumerate() {
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
    dst: &mut [Option<String>; WEAPON_ANIM_SLOTS],
    src: [Option<String>; WEAPON_ANIM_SLOTS],
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if d.is_none() {
            *d = s;
        }
    }
}

fn xanims_idle(names: &[Option<String>; WEAPON_ANIM_SLOTS]) -> Option<&str> {
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
    dst.last_shot_eject_pair_authored |= src.last_shot_eject_pair_authored;
}

fn merge_combat_slots(dst: &mut CombatFxSlots, src: &CombatFxSlots) {
    if dst.view_flash.is_none() {
        dst.view_flash = src.view_flash;
    }
    if dst.world_flash.is_none() {
        dst.world_flash = src.world_flash;
    }
    if dst.view_shell_eject.is_none() {
        dst.view_shell_eject = src.view_shell_eject;
    }
    if dst.world_shell_eject.is_none() {
        dst.world_shell_eject = src.world_shell_eject;
    }
    if dst.view_last_shot_eject.is_none() {
        dst.view_last_shot_eject = src.view_last_shot_eject;
    }
    if dst.world_last_shot_eject.is_none() {
        dst.world_last_shot_eject = src.world_last_shot_eject;
    }
    if dst.explosion.is_none() {
        dst.explosion = src.explosion;
    }
    if dst.tracer.is_none() {
        dst.tracer = src.tracer;
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
    if !dst.timed_detonation {
        dst.timed_detonation = src.timed_detonation;
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
    if dst.projectile_speed_forward == 0 {
        dst.projectile_speed_forward = src.projectile_speed_forward;
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
    if dst.location_damage_mult.is_none() {
        dst.location_damage_mult = src.location_damage_mult;
    }
    if dst.penetrate_type == 0 && src.penetrate_type != 0 {
        dst.penetrate_type = src.penetrate_type;
    }
    if dst.penetrate_multiplier == 0.0 && src.penetrate_multiplier != 0.0 {
        dst.penetrate_multiplier = src.penetrate_multiplier;
    }
    // motion_tracker belongs to the complete definition, not the shared body.
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
    if dst.overlay_interface == 0 {
        dst.overlay_interface = src.overlay_interface;
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
    alternate_weapon: Option<String>,
    alternate_index: u32,

    namespace: crate::AssetNamespace,

    facts: WeaponBodyFacts,

    weap_def: Option<(u8, u32)>,

    gun_xmodel: Option<String>,

    hand_xmodel: Option<String>,

    gun_xmodel_edge: AssetEdge<FpvMeshSpace>,

    hand_xmodel_edge: AssetEdge<FpvMeshSpace>,

    rocket_model_edge: AssetEdge<FpvMeshSpace>,

    attachment_view_model_edges: Vec<AssetEdge<FpvMeshSpace>>,

    fpv_hands: [Option<(asset_model::FpvHands, crate::FpvMeshIndex)>; 2],

    fpv_mount_plan: Option<Result<asset_model::FpvMountPlan, asset_model::FpvMountError>>,

    fpv_assemblies: [Option<Result<crate::FpvSideAssemblies, String>>; 2],

    world_model: Option<String>,

    world_model_edge: AssetEdge<WorldWeaponSpace>,

    attachment_world_model_edges: Vec<AssetEdge<WorldWeaponSpace>>,

    attachment_world_mounts: Vec<Option<String>>,

    projectile_model: Option<String>,

    projectile_model_edge: AssetEdge<ProjectileModelSpace>,

    rocket_model: Option<String>,

    sz_xanims: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanim_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_right_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    sz_xanim_left_edges: [AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS],

    notetrack_actions: HashMap<String, LinkedNotetrackAction>,

    sz_xanims_right: [Option<String>; WEAPON_ANIM_SLOTS],

    sz_xanims_left: [Option<String>; WEAPON_ANIM_SLOTS],

    hide_tags: Vec<String>,

    attachment_view_models: Vec<String>,
    attachment_world_models: Vec<String>,

    iw5_configuration: Option<(u32, Iw5AttachmentSelection)>,
    prepared_attachments: Vec<String>,

    iw5_attachment_slots: [Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    iw5_reload_overrides: Vec<fastfile_iw5::ReloadOverride>,
    iw5_anim_overrides: Vec<LeftoverAnimOverride>,
    iw5_fx_overrides: Vec<Iw5FxOverride>,
    iw5_notetrack_overrides: Vec<Iw5NotetrackOverride>,

    sounds: WeaponSoundAliases,

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
            alternate_weapon: None,
            alternate_index: 0,
            namespace: crate::AssetNamespace::Iw4,
            facts: WeaponBodyFacts::default(),
            weap_def: None,
            gun_xmodel: None,
            hand_xmodel: None,
            gun_xmodel_edge: AssetEdge::Absent,
            hand_xmodel_edge: AssetEdge::Absent,
            rocket_model_edge: AssetEdge::Absent,
            attachment_view_model_edges: Vec::new(),
            fpv_hands: [None, None],
            fpv_mount_plan: None,
            fpv_assemblies: [None, None],
            world_model: None,
            world_model_edge: AssetEdge::Absent,
            attachment_world_model_edges: Vec::new(),
            attachment_world_mounts: Vec::new(),
            projectile_model: None,
            projectile_model_edge: AssetEdge::Absent,
            rocket_model: None,
            sz_xanims: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_right_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            sz_xanim_left_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
            notetrack_actions: HashMap::new(),
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: Vec::new(),
            attachment_view_models: Vec::new(),
            attachment_world_models: Vec::new(),
            iw5_configuration: None,
            prepared_attachments: Vec::new(),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            sounds: WeaponSoundAliases::default(),
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

    world_catalog_identity: u64,

    iw5_attachments: HashMap<String, Iw5ScopeRow>,

    iw5_candidates: Vec<Iw5ConfigurationCandidate>,

    configurations: HashMap<crate::WeaponSelection, u32>,

    by_name: HashMap<String, u32>,

    by_namespaced: HashMap<(crate::AssetNamespace, String), u32>,

    item_groups: HashMap<(crate::AssetNamespace, String), String>,

    families: crate::WeaponFamilies,

    alternate_fpv: HashMap<(u32, u32), [Option<Result<crate::FpvSideAssemblies, String>>; 2]>,
    fpv_clip_tracks: Arc<crate::FpvClipTracks>,

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

#[derive(Clone, Debug, Default)]
pub struct Iw5PreparationCensus {
    pub prepared: usize,
    pub refused: Vec<crate::WeaponSelection>,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponBuild {
    registry: WeaponRegistry,
    combat_slots: Vec<CombatFxSlots>,
    family_tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
}

impl std::ops::Deref for WeaponBuild {
    type Target = WeaponRegistry;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

impl WeaponBuild {
    pub fn publish(self) -> WeaponRegistry {
        let mut registry = self.registry;
        registry.families = crate::WeaponFamilies::build(&self.family_tables, &registry);
        registry.build_iw5_candidates();
        registry
    }

    pub fn set_family_tables(
        &mut self,
        tables: Vec<(crate::AssetNamespace, crate::CapturedStringTable)>,
    ) {
        self.family_tables = tables;
    }

    pub fn prepare_iw5_configurations(&mut self) -> Iw5PreparationCensus {
        let families = crate::WeaponFamilies::build(&self.family_tables, &self.registry);
        let mut census = Iw5PreparationCensus::default();
        let mut prepared = Vec::new();
        for (base_id, selection) in families.iw5_candidate_selections() {
            if selection.attachments.is_empty() {
                continue;
            }
            let Some(namespace) = selection.family.as_ref().map(|key| key.namespace) else {
                continue;
            };
            let attachments = families.normalize(namespace, &selection.attachments);
            let selection = crate::WeaponSelection {
                attachments,
                ..selection
            };
            let row = self
                .registry
                .resolve_iw5_attachment_slots(base_id, &selection.attachments)
                .ok()
                .and_then(|native| {
                    self.registry
                        .compose_iw5_configuration(base_id, native, false)
                });
            match row {
                Some(row) => prepared.push((base_id, selection, row)),
                None => census.refused.push(selection),
            }
        }
        census.prepared = prepared.len();
        for (base_id, selection, mut row) in prepared {
            row.prepared_attachments = selection.attachments.clone();
            self.registry
                .configurations
                .insert(selection, self.registry.rows.len() as u32);
            let slots = self
                .combat_slots
                .get(base_id as usize)
                .copied()
                .unwrap_or_default();
            self.combat_slots
                .resize(self.registry.rows.len(), CombatFxSlots::default());
            self.combat_slots.push(slots);
            self.registry.rows.push(row);
        }
        let primary_count = self.registry.rows.len();
        for parent in 1..primary_count {
            let Some((base, native)) = self.registry.rows[parent].iw5_configuration else {
                continue;
            };
            let Some(mut alternate) = self.registry.compose_iw5_configuration(base, native, true)
            else {
                continue;
            };
            let index = self.registry.rows.len() as u32;
            let primary = &self.registry.rows[parent];
            let underbarrel = native.underbarrel as usize + 5;
            let shared = self.registry.rows[base as usize].iw5_attachment_slots[underbarrel]
                .as_deref()
                .and_then(|name| self.registry.iw5_attachments.get(name))
                .is_some_and(|a| a.share_ammo_with_alt);
            alternate.facts.ammo_index = if shared {
                if primary.facts.ammo_index != 0 {
                    primary.facts.ammo_index
                } else {
                    parent as i32
                }
            } else {
                (parent as i32) | 0x1000000
            };
            alternate.facts.clip_index = if shared {
                if primary.facts.clip_index != 0 {
                    primary.facts.clip_index
                } else {
                    parent as i32
                }
            } else {
                (parent as i32) | 0x1000000
            };
            alternate.prepared_attachments = primary.prepared_attachments.clone();
            alternate.alternate_weapon = None;
            alternate.alternate_index = 0;
            self.registry.rows[parent].alternate_index = index;
            self.combat_slots.push(
                self.combat_slots
                    .get(base as usize)
                    .copied()
                    .unwrap_or_default(),
            );
            self.registry.rows.push(alternate);
        }
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
        census
    }

    pub fn absorb(&mut self, other: Self) {
        if other.registry.is_empty() {
            self.registry
                .iw5_attachments
                .extend(other.registry.iw5_attachments);
            self.family_tables.extend(other.family_tables);
            return;
        }
        if self.registry.rows.is_empty() {
            *self = other;
            return;
        }
        self.registry
            .rows
            .extend(other.registry.rows.into_iter().skip(1));
        self.registry
            .iw5_attachments
            .extend(other.registry.iw5_attachments);
        self.combat_slots
            .extend(other.combat_slots.into_iter().skip(1));
        self.family_tables.extend(other.family_tables);
        self.registry.item_groups.extend(other.registry.item_groups);
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
    }

    pub fn resolve_combat_fx(&mut self, fx: &crate::FxCatalog, tracers: &crate::TracerCatalog) {
        let n = self.registry.rows.len().min(self.combat_slots.len());
        for i in 1..n {
            stamp_combat_fx(
                &mut self.registry.rows[i].combat_fx,
                self.combat_slots[i],
                fx,
                tracers,
            );
        }
    }

    pub fn stamp_namespace(&mut self, ns: crate::AssetNamespace) {
        let attachments = &self.registry.iw5_attachments;
        for row in self.registry.rows.iter_mut().skip(1) {
            row.namespace = ns;
            if ns == crate::AssetNamespace::Iw5 {
                if let Some((view, world)) =
                    iw5_default_scope_models(&row.iw5_attachment_slots, attachments)
                {
                    row.attachment_view_models.extend(view);
                    row.attachment_world_models.extend(world);
                }
            }
        }
        self.registry.rebuild_name_maps();
        self.registry.revision = mint_weapon_revision();
    }

    pub fn resolve_hud_material_edges(&mut self, materials: &crate::MaterialDefinitions) {
        for row in &mut self.registry.rows {
            if row.overlay_image.is_none()
                && let Some(name) = row.overlay_material.as_deref()
                && let Some(index) = materials.material_index_by_name(name)
                && let Some(material) = materials.materials.get(index.order())
            {
                row.overlay_image = materials.hud_image_name(material).map(str::to_owned);
            }
            row.reticle.center_edge = material_hint_edge(
                row.reticle.center_material.as_deref(),
                row.reticle.center_authored,
                materials,
            );
            row.reticle.side_edge = material_hint_edge(
                row.reticle.side_material.as_deref(),
                row.reticle.side_authored,
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
        for row in &mut self.registry.rows {
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

    pub fn resolve_sz_xanim_edges(&mut self, xanims: &crate::XAnimCatalog) {
        for row in &mut self.registry.rows {
            let mut edges = [AssetEdge::Absent; WEAPON_ANIM_SLOTS];
            for (edge, hint) in edges.iter_mut().zip(row.sz_xanims.iter()) {
                *edge = xanim_hint_edge(hint.as_deref(), row.namespace, xanims);
            }
            row.sz_xanim_edges = edges;
            row.sz_xanim_right_edges = std::array::from_fn(|slot| {
                xanim_hint_edge(row.sz_xanims_right[slot].as_deref(), row.namespace, xanims)
            });
            row.sz_xanim_left_edges = std::array::from_fn(|slot| {
                xanim_hint_edge(row.sz_xanims_left[slot].as_deref(), row.namespace, xanims)
            });
        }
    }

    pub fn resolve_notetrack_actions(&mut self, xanims: &crate::XAnimCatalog) -> (usize, usize) {
        let mut linked = 0;
        let mut inline = 0;
        for row in &mut self.registry.rows {
            let mut actions: HashMap<String, LinkedNotetrackAction> = HashMap::new();
            match row.sounds.notetrack_convention {
                NotetrackConvention::SoundMap => {
                    for (note, alias) in &row.sounds.notetrack_sound_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.sound_alias.is_none() {
                                action.sound_alias = Some(alias.clone());
                            }
                        }
                    }
                    for (note, alias) in &row.sounds.notetrack_rumble_map {
                        if !alias.is_empty() {
                            let action = actions.entry(note.to_ascii_lowercase()).or_default();
                            if action.rumble_alias.is_none() {
                                action.rumble_alias = Some(alias.clone());
                            }
                        }
                    }
                }
                NotetrackConvention::InlinePrefix => {
                    let indices: std::collections::BTreeSet<_> = row
                        .sz_xanim_edges
                        .iter()
                        .chain(&row.sz_xanim_right_edges)
                        .chain(&row.sz_xanim_left_edges)
                        .filter_map(|edge| edge.bound_index())
                        .collect();
                    for index in indices {
                        let Some(clip) = xanims.clip_at(index) else {
                            continue;
                        };
                        for notify in &clip.notifies {
                            let sound = t5_inline_note_alias(&notify.name, T5_NOTE_SOUND_PREFIX);
                            let rumble = t5_inline_note_alias(&notify.name, T5_NOTE_RUMBLE_PREFIX);
                            if sound.is_none() && rumble.is_none() {
                                continue;
                            }
                            let action =
                                actions.entry(notify.name.to_ascii_lowercase()).or_default();
                            action.sound_alias = sound.map(str::to_owned);
                            action.rumble_alias = rumble.map(str::to_owned);
                        }
                    }
                }
            }
            linked += actions.len();
            if row.sounds.notetrack_convention == NotetrackConvention::InlinePrefix {
                inline += actions.len();
            }
            row.notetrack_actions = actions;
        }
        (linked, inline)
    }

    pub fn resolve_fpv_mesh_edges(&mut self, fpv: &crate::FpvMeshCatalog) {
        for row in &mut self.registry.rows {
            row.gun_xmodel_edge = fpv_model_edge(row.gun_xmodel.as_deref(), row.namespace, fpv);
            row.hand_xmodel_edge = fpv_model_edge(row.hand_xmodel.as_deref(), row.namespace, fpv);
            row.rocket_model_edge = fpv_model_edge(row.rocket_model.as_deref(), row.namespace, fpv);
            row.attachment_view_model_edges = row
                .attachment_view_models
                .iter()
                .map(|name| fpv_model_edge(Some(name), row.namespace, fpv))
                .collect();
            row.fpv_mount_plan = row.gun_xmodel_edge.bound_index().and_then(|gun| {
                let attachments: Option<Vec<_>> = row
                    .attachment_view_model_edges
                    .iter()
                    .map(|edge| edge.bound_index().map(crate::FpvMeshIndex::from_order))
                    .collect();
                let rocket = if row.rocket_model_edge.is_absent() {
                    Some(None)
                } else {
                    row.rocket_model_edge
                        .bound_index()
                        .map(|index| Some(crate::FpvMeshIndex::from_order(index)))
                }?;
                Some(asset_model::plan_fpv_mounts(
                    fpv,
                    crate::FpvMeshIndex::from_order(gun),
                    &attachments?,
                    rocket,
                ))
            });
        }
    }

    pub fn resolve_fpv_hands(
        &mut self,
        fpv: &crate::FpvMeshCatalog,
        bodies: &asset_model::BodyMeshCatalog,
    ) {
        for row in &mut self.registry.rows {
            let map_ns = fpv.map_namespace.unwrap_or(row.namespace);
            let hand_name = row
                .hand_xmodel_edge
                .bound_index()
                .and_then(|index| fpv.get_at(index))
                .map(|entry| entry.skel.name.as_str());
            row.fpv_hands = std::array::from_fn(|side| {
                let kit = bodies.kits().kit(side == 1);
                let choice =
                    asset_model::FpvHands::resolve(fpv, map_ns, kit, hand_name, row.namespace);
                let (ns, name) = choice.key()?;
                let index = fpv.index_by_name(ns, name)?;
                let skel = &fpv.get_at(index)?.skel;
                if skel.pose.is_none() || !skel.bone_names.iter().any(|bone| bone == "tag_weapon") {
                    return None;
                }
                Some((choice, crate::FpvMeshIndex::from_order(index)))
            });
        }
    }

    pub fn resolve_fpv_assemblies(
        &mut self,
        fpv: &crate::FpvMeshCatalog,
        xanims: &crate::XAnimCatalog,
    ) -> FpvAssemblyCensus {
        let mut shared: HashMap<crate::FpvAssemblyKey, Result<Arc<crate::FpvAssembly>, String>> =
            HashMap::new();
        let mut tracks = crate::FpvClipTracks::default();
        let mut census = FpvAssemblyCensus::default();
        self.registry.alternate_fpv.clear();
        let mut variants = Vec::new();
        for parent in 1..self.registry.rows.len() as u32 {
            let alternate = self.registry.alternate_of(parent);
            if alternate != 0
                && self
                    .registry
                    .facts_of(alternate)
                    .is_some_and(|f| f.inventory_type == 3)
            {
                variants.push((alternate, parent));
            }
        }
        let preparations: Vec<_> = (0..self.registry.rows.len() as u32)
            .map(|id| (id, 0))
            .chain(variants)
            .map(|(id, parent)| {
                (
                    id,
                    parent,
                    crate::effective_hide_tags(
                        &self.registry,
                        if parent == 0 { id } else { parent },
                    ),
                )
            })
            .collect();
        for (id, parent, hide_tags) in preparations {
            let row = &mut self.registry.rows[id as usize];
            if parent == 0 {
                row.fpv_assemblies = [None, None];
            }
            let Some(Ok(mounts)) = &row.fpv_mount_plan else {
                continue;
            };
            let clips: std::collections::BTreeSet<usize> = row
                .sz_xanim_edges
                .iter()
                .chain(&row.sz_xanim_right_edges)
                .chain(&row.sz_xanim_left_edges)
                .filter_map(|edge| edge.bound_index())
                .collect();
            let mut assemble = |hands: crate::FpvMeshIndex, rocket: bool| {
                let key = crate::FpvAssemblyKey {
                    hands,
                    gun: mounts.gun,
                    attachments: mounts.attachments.iter().map(|mount| mount.model).collect(),
                    rocket: rocket
                        .then(|| mounts.rocket.as_ref().map(|mount| mount.model))
                        .flatten(),
                    hide_tags: hide_tags.clone(),
                };
                shared
                    .entry(key)
                    .or_insert_with(|| {
                        census.built += 1;
                        crate::FpvAssembly::build(fpv, hands, mounts, rocket, &hide_tags)
                            .map(Arc::new)
                            .map_err(|error| error.to_string())
                    })
                    .clone()
            };
            let sides: [Option<Result<crate::FpvSideAssemblies, String>>; 2] =
                std::array::from_fn(|side| {
                    let (_, hands) = row.fpv_hands[side].as_ref()?;
                    let bare = match assemble(*hands, false) {
                        Ok(bare) => bare,
                        Err(error) => return Some(Err(error)),
                    };
                    let rocket = match mounts.rocket.is_some().then(|| assemble(*hands, true)) {
                        None => None,
                        Some(Ok(rocket)) => Some(rocket),
                        Some(Err(error)) => return Some(Err(error)),
                    };
                    Some(Ok(crate::FpvSideAssemblies { bare, rocket }))
                });
            for side in sides.iter().flatten().flatten() {
                for assembly in std::iter::once(&side.bare).chain(&side.rocket) {
                    for &clip_index in &clips {
                        let Some(clip) = xanims.clip_at(clip_index) else {
                            continue;
                        };
                        for part in &assembly.parts {
                            tracks.bind(fpv, clip_index, &clip, part.model);
                        }
                    }
                }
            }
            for side in sides.iter().flatten() {
                match side {
                    Ok(_) => census.linked += 1,
                    Err(_) => census.refused += 1,
                }
            }
            if parent == 0 {
                row.fpv_assemblies = sides;
            } else {
                self.registry.alternate_fpv.insert((id, parent), sides);
            }
        }
        census.clip_tables = tracks.len();
        self.registry.fpv_clip_tracks = Arc::new(tracks);
        census
    }

    pub fn resolve_world_model_edges(&mut self, catalog: &crate::WorldWeaponCatalog) {
        self.registry.world_catalog_identity = catalog.identity();
        for row in &mut self.registry.rows {
            row.world_model_edge = world_model_edge(row.world_model.as_deref(), catalog);
            row.attachment_world_model_edges = row
                .attachment_world_models
                .iter()
                .map(|name| world_model_edge(Some(name), catalog))
                .collect();
            let gun = row
                .world_model_edge
                .bound_index()
                .and_then(|index| catalog.get_at(index));
            row.attachment_world_mounts = row
                .attachment_world_model_edges
                .iter()
                .map(|edge| {
                    let root = catalog
                        .get_at(edge.bound_index()?)?
                        .skel
                        .bone_names
                        .first()?;
                    gun?.skel
                        .bone_names
                        .iter()
                        .find(|bone| bone.eq_ignore_ascii_case(root))
                        .cloned()
                })
                .collect();
        }
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
                self.registry
                    .item_groups
                    .insert((ns, name), group.to_owned());
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

    pub fn stamp_projectile_model_edges(
        &mut self,
        catalog: &crate::ProjectileMeshCatalog,
        zone: ZoneOwner,
    ) {
        for row in &mut self.registry.rows {
            row.projectile_model_edge = match row.projectile_model.as_deref() {
                None | Some("") => AssetEdge::Absent,
                Some(name) => match catalog.index_by_name(name) {
                    Some(order) => AssetEdge::bind_order(order, zone),
                    None => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                },
            };
        }
    }

    fn from_catalog(
        mut entries: Vec<CatalogWeapon>,
        iw5_attachments: HashMap<String, Iw5ScopeRow>,
    ) -> Self {
        let mut gun_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut hand_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut world_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut projectile_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut rocket_by_def: HashMap<(u8, u32), String> = HashMap::new();
        let mut sounds_by_def: HashMap<(u8, u32), WeaponSoundAliases> = HashMap::new();
        let mut combat_fx_by_def: HashMap<(u8, u32), WeaponCombatFx> = HashMap::new();
        let mut combat_slots_by_def: HashMap<(u8, u32), CombatFxSlots> = HashMap::new();
        let mut right_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_SLOTS]> =
            HashMap::new();
        let mut left_by_def: HashMap<(u8, u32), [Option<String>; WEAPON_ANIM_SLOTS]> =
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
                merge_combat_slots(
                    combat_slots_by_def.entry(key).or_default(),
                    &entry.combat_slots,
                );
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
                if let Some(&shared) = combat_slots_by_def.get(&key) {
                    merge_combat_slots(&mut entry.combat_slots, &shared);
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
                    if existing.alternate_weapon.is_none() {
                        existing.alternate_weapon = entry.alternate_weapon;
                    }
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
                    if !existing.reticle.center_authored
                        && !existing.reticle.side_authored
                        && (entry.reticle.center_authored || entry.reticle.side_authored)
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
                    for (existing_slot, new_slot) in existing
                        .iw5_attachment_slots
                        .iter_mut()
                        .zip(entry.iw5_attachment_slots)
                    {
                        if existing_slot.is_none() {
                            *existing_slot = new_slot;
                        }
                    }
                    if existing.iw5_reload_overrides.is_empty() {
                        existing.iw5_reload_overrides = entry.iw5_reload_overrides;
                    }
                    if existing.iw5_anim_overrides.is_empty() {
                        existing.iw5_anim_overrides = entry.iw5_anim_overrides;
                    }
                    if existing.iw5_fx_overrides.is_empty() {
                        existing.iw5_fx_overrides = entry.iw5_fx_overrides;
                    }
                    if existing.iw5_notetrack_overrides.is_empty() {
                        existing.iw5_notetrack_overrides = entry.iw5_notetrack_overrides;
                    }
                    merge_sound_aliases(&mut existing.sounds, &entry.sounds);
                    if existing.sounds.leftover_sound_overrides.is_empty() {
                        existing.sounds.leftover_sound_overrides =
                            entry.sounds.leftover_sound_overrides.clone();
                    }
                    merge_combat_fx(&mut existing.combat_fx, &entry.combat_fx);
                    merge_combat_slots(&mut existing.combat_slots, &entry.combat_slots);
                    merge_body_facts(&mut existing.facts, entry.facts);
                }
            }
        }
        let mut names: Vec<String> = by_name.keys().cloned().collect();
        names.sort_unstable();

        let mut rows = Vec::with_capacity(names.len() + 1);
        let mut combat_slots = Vec::with_capacity(names.len() + 1);
        let mut index_of = HashMap::with_capacity(names.len());
        rows.push(WeaponRow::default());
        combat_slots.push(CombatFxSlots::default());
        for name in names {
            let entry = by_name.remove(&name).expect("key from map");
            let id = rows.len() as u32;
            index_of.insert(name.clone(), id);
            combat_slots.push(entry.combat_slots);
            rows.push(WeaponRow {
                name,
                alternate_weapon: entry.alternate_weapon,
                alternate_index: 0,
                namespace: crate::AssetNamespace::Iw4,
                facts: entry.facts,
                weap_def: entry.weap_def,
                gun_xmodel: entry.gun_xmodel,
                hand_xmodel: entry.hand_xmodel,
                gun_xmodel_edge: AssetEdge::Absent,
                hand_xmodel_edge: AssetEdge::Absent,
                rocket_model_edge: AssetEdge::Absent,
                attachment_view_model_edges: Vec::new(),
                fpv_hands: [None, None],
                fpv_mount_plan: None,
                fpv_assemblies: [None, None],
                world_model: entry.world_model,
                world_model_edge: AssetEdge::Absent,
                attachment_world_model_edges: Vec::new(),
                attachment_world_mounts: Vec::new(),
                projectile_model: entry.projectile_model,
                projectile_model_edge: AssetEdge::Absent,
                rocket_model: entry.rocket_model,
                sz_xanims: entry.sz_xanims,
                sz_xanim_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                sz_xanim_right_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                sz_xanim_left_edges: [AssetEdge::Absent; WEAPON_ANIM_SLOTS],
                notetrack_actions: HashMap::new(),
                sz_xanims_right: entry.sz_xanims_right,
                sz_xanims_left: entry.sz_xanims_left,
                hide_tags: entry.hide_tags,
                attachment_view_models: Vec::new(),
                attachment_world_models: Vec::new(),
                iw5_configuration: None,
                prepared_attachments: Vec::new(),
                iw5_attachment_slots: entry.iw5_attachment_slots,
                iw5_reload_overrides: entry.iw5_reload_overrides,
                iw5_anim_overrides: entry.iw5_anim_overrides,
                iw5_fx_overrides: entry.iw5_fx_overrides,
                iw5_notetrack_overrides: entry.iw5_notetrack_overrides,
                sounds: entry.sounds,
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
        let mut registry = WeaponRegistry {
            rows,
            world_catalog_identity: 0,
            iw5_attachments,
            iw5_candidates: Vec::new(),
            configurations: HashMap::new(),
            by_name: index_of,
            by_namespaced: HashMap::new(),
            item_groups: HashMap::new(),
            families: crate::WeaponFamilies::default(),
            alternate_fpv: HashMap::new(),
            fpv_clip_tracks: Arc::default(),
            revision: mint_weapon_revision(),
        };
        registry.rebuild_name_maps();
        Self {
            registry,
            combat_slots,
            family_tables: Vec::new(),
        }
    }
}

impl WeaponRegistry {
    pub fn world_catalog_identity(&self) -> u64 {
        self.world_catalog_identity
    }

    fn build_iw5_candidates(&mut self) {
        self.iw5_candidates = self
            .families
            .iw5_candidate_selections()
            .into_iter()
            .map(|(base_id, selection)| {
                let native = self.resolve_iw5_attachment_slots(base_id, &selection.attachments);
                let primary_assets = native
                    .as_ref()
                    .ok()
                    .and_then(|native| self.iw5_primary_attachment_assets(base_id, *native))
                    .map(|assets| assets.into_iter().filter_map(|a| a.scope.clone()).collect())
                    .unwrap_or_default();
                let primary_ads_zoom_fov = native
                    .as_ref()
                    .ok()
                    .and_then(|native| self.iw5_primary_ads_zoom_fov(base_id, *native));
                let primary_ads_aim_pitch = native
                    .as_ref()
                    .ok()
                    .and_then(|native| self.iw5_primary_ads_aim_pitch(base_id, *native));
                Iw5ConfigurationCandidate {
                    selection,
                    base_id,
                    native,
                    primary_assets,
                    primary_ads_zoom_fov,
                    primary_ads_aim_pitch,
                }
            })
            .collect();
    }

    pub fn iw5_configuration_candidates(&self) -> &[Iw5ConfigurationCandidate] {
        &self.iw5_candidates
    }

    pub fn iw5_attachment_slots_of(
        &self,
        id: u32,
    ) -> Option<&[Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(&row.iw5_attachment_slots)
    }

    pub fn iw5_attachment_asset(&self, native_name: &str) -> Option<&Iw5ScopeRow> {
        self.iw5_attachments.get(native_name)
    }

    pub fn iw5_reload_overrides_of(&self, id: u32) -> Option<&[fastfile_iw5::ReloadOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_reload_overrides.as_slice())
    }

    pub fn iw5_anim_overrides_of(&self, id: u32) -> Option<&[LeftoverAnimOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_anim_overrides.as_slice())
    }

    pub fn iw5_fx_overrides_of(&self, id: u32) -> Option<&[Iw5FxOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5).then_some(row.iw5_fx_overrides.as_slice())
    }

    pub fn iw5_notetrack_overrides_of(&self, id: u32) -> Option<&[Iw5NotetrackOverride]> {
        let row = self.rows.get(id as usize)?;
        (row.namespace == crate::AssetNamespace::Iw5)
            .then_some(row.iw5_notetrack_overrides.as_slice())
    }

    pub fn select_iw5_anim_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        anim_tree_type: u32,
    ) -> Option<&LeftoverAnimOverride> {
        iw5_best_pair_override(
            self.iw5_anim_overrides_of(id)?,
            selection,
            anim_tree_type,
            |row| (row.attachment1, row.attachment2, row.anim_tree_type),
        )
    }

    pub fn select_iw5_sound_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        sound_type: u32,
    ) -> Option<&LeftoverSoundOverride> {
        let row = self.rows.get(id as usize)?;
        if row.namespace != crate::AssetNamespace::Iw5 {
            return None;
        }
        iw5_best_pair_override(
            &row.sounds.leftover_sound_overrides,
            selection,
            sound_type,
            |row| (row.attachment1, row.attachment2, row.sound_type),
        )
    }

    pub fn select_iw5_fx_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
        fx_type: u32,
    ) -> Option<&Iw5FxOverride> {
        iw5_best_pair_override(self.iw5_fx_overrides_of(id)?, selection, fx_type, |row| {
            (row.attachment1, row.attachment2, row.fx_type)
        })
    }

    pub fn select_iw5_reload_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<&fastfile_iw5::ReloadOverride> {
        self.iw5_reload_overrides_of(id)?
            .iter()
            .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
    }

    pub fn select_iw5_notetrack_override(
        &self,
        id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<&Iw5NotetrackOverride> {
        self.iw5_notetrack_overrides_of(id)?
            .iter()
            .find(|row| row.attachment != 0 && selection.contains_condition(row.attachment))
    }

    pub fn resolve_iw5_attachment_slots(
        &self,
        base_id: u32,
        names: &[String],
    ) -> Result<Iw5AttachmentSelection, crate::ConfigurationRefusal> {
        let slots = self.iw5_attachment_slots_of(base_id).ok_or_else(|| {
            crate::ConfigurationRefusal::UnknownFamily(self.name_of(base_id).to_owned())
        })?;
        let mut selected = Iw5AttachmentSelection::default();
        for name in names {
            let mut matches = slots.iter().enumerate().filter(|(_, slot)| {
                slot.as_deref()
                    .is_some_and(|native| native.eq_ignore_ascii_case(name))
            });
            let direct = matches.next();
            if matches.next().is_some() {
                return Err(crate::ConfigurationRefusal::Unsupported(format!(
                    "`{name}` has more than one native IW5 slot in `{}`",
                    self.name_of(base_id)
                )));
            }
            let index = if let Some((index, _)) = direct {
                index
            } else {
                let display_key = format!("WEAPON_{}_ATTACHMENT", name.to_ascii_uppercase());
                let mut by_display = slots.iter().enumerate().filter(|(_, slot)| {
                    slot.as_deref()
                        .and_then(|native| self.iw5_attachments.get(native))
                        .and_then(|asset| asset.display_name.as_deref())
                        .is_some_and(|key| key.eq_ignore_ascii_case(&display_key))
                });
                let Some((index, _)) = by_display.next() else {
                    return Err(crate::ConfigurationRefusal::NotOffered(name.clone()));
                };
                if by_display.next().is_some() {
                    return Err(crate::ConfigurationRefusal::Unsupported(format!(
                        "`{name}` has an ambiguous IW5 display key in `{}`",
                        self.name_of(base_id)
                    )));
                }
                index
            };
            let native = slots[index].as_deref().expect("matched native slot");
            if !self.iw5_attachments.contains_key(native) {
                return Err(crate::ConfigurationRefusal::MissingContent(
                    native.to_owned(),
                ));
            }
            match index {
                0..=5 => {
                    if selected.scope != 0 && selected.scope != (index + 1) as u8 {
                        return Err(crate::ConfigurationRefusal::Incompatible {
                            a: slots[selected.scope as usize - 1]
                                .clone()
                                .unwrap_or_default(),
                            b: name.clone(),
                        });
                    }
                    selected.scope = (index + 1) as u8;
                }
                6..=8 => {
                    if selected.underbarrel != 0 && selected.underbarrel != (index - 5) as u8 {
                        return Err(crate::ConfigurationRefusal::Incompatible {
                            a: slots[selected.underbarrel as usize + 5]
                                .clone()
                                .unwrap_or_default(),
                            b: name.clone(),
                        });
                    }
                    selected.underbarrel = (index - 5) as u8;
                }
                9..=12 => selected.others |= 1 << (index - 9),
                _ => unreachable!(),
            }
        }
        Ok(selected)
    }

    pub fn iw5_primary_attachment_assets(
        &self,
        base_id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<Vec<&Iw5ScopeRow>> {
        let slots = self.iw5_attachment_slots_of(base_id)?;
        let slot_asset = |index: usize| {
            slots
                .get(index)?
                .as_deref()
                .and_then(|name| self.iw5_attachment_asset(name))
        };
        let mut assets = Vec::with_capacity(3);
        let mut push = |asset| {
            if assets.len() < 3 {
                assets.push(asset);
            } else {
                assets[2] = asset;
            }
        };
        if selection.scope != 0 {
            push(slot_asset(usize::from(selection.scope - 1))?);
        }
        for bit in 0..4 {
            if selection.others & (1 << bit) != 0 {
                push(slot_asset(9 + bit)?);
            }
        }
        if selection.underbarrel != 0 {
            let asset = slot_asset(usize::from(selection.underbarrel) + 5)?;
            if asset.weapon_class == 0
                || asset.ads_settings_main.is_some()
                || (asset.scales.ads_settings_main != 0.0 && asset.scales.ads_settings_main != 1.0)
            {
                push(asset);
            }
        }
        Some(assets)
    }

    pub fn iw5_primary_ads_aim_pitch(
        &self,
        base_id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<f32> {
        self.iw5_primary_ads_value(
            base_id,
            selection,
            |facts| facts.ads_aim_pitch,
            |settings| settings.ads_aim_pitch,
        )
    }

    pub fn iw5_primary_ads_zoom_fov(
        &self,
        base_id: u32,
        selection: Iw5AttachmentSelection,
    ) -> Option<f32> {
        self.iw5_primary_ads_value(
            base_id,
            selection,
            |facts| facts.ads_zoom_fov,
            |settings| settings.ads_zoom_fov,
        )
    }

    fn iw5_primary_ads_value(
        &self,
        base_id: u32,
        selection: Iw5AttachmentSelection,
        base_value: impl FnOnce(WeaponBodyFacts) -> f32,
        setting_value: impl Fn(fastfile_iw5::AttachmentAdsSettings) -> f32,
    ) -> Option<f32> {
        let base = base_value(self.facts_of(base_id)?);
        let assets = self.iw5_primary_attachment_assets(base_id, selection)?;
        let (settings, scale) = iw5_primary_ads(&assets);
        Some(settings.map_or(base, setting_value) * scale)
    }

    fn compose_iw5_configuration(
        &self,
        base_id: u32,
        native: Iw5AttachmentSelection,
        alternate: bool,
    ) -> Option<WeaponRow> {
        use fastfile_iw5::size as sz;
        let base = self.rows.get(base_id as usize)?;
        let slot_asset = |index: usize| {
            base.iw5_attachment_slots
                .get(index)?
                .as_deref()
                .and_then(|native| self.iw5_attachments.get(native))
        };
        let scope = match native.scope {
            0 => None,
            index => Some(slot_asset(usize::from(index) - 1)?),
        };
        let underbarrel = match native.underbarrel {
            0 => None,
            index => Some(slot_asset(usize::from(index) + 5)?),
        };
        let others = (0..4)
            .filter(|bit| native.others & (1 << bit) != 0)
            .map(|bit| slot_asset(9 + bit))
            .collect::<Option<Vec<_>>>()?;

        let mut row = base.clone();
        row.iw5_configuration = Some((base_id, native));

        let first = |models: &[Option<String>]| models.first().cloned().flatten();
        let mut view = if scope.is_none() {
            base.attachment_view_models.clone()
        } else {
            Vec::new()
        };
        let mut world = if scope.is_none() {
            base.attachment_world_models.clone()
        } else {
            Vec::new()
        };
        if let Some(scope) = scope {
            view.extend(first(&scope.view_models));
            view.extend(first(&scope.reticle_models));
            world.extend(first(&scope.world_models));
        }
        for asset in underbarrel.into_iter().chain(others.iter().copied()) {
            view.extend(first(&asset.view_models));
            world.extend(first(&asset.world_models));
        }
        row.attachment_view_models = view;
        row.attachment_world_models = world;

        let primary_assets = self.iw5_primary_attachment_assets(base_id, native)?;
        let alternate_assets;
        let assets = if alternate {
            let underbarrel = underbarrel.filter(|asset| asset.weapon_class != 0)?;
            row.facts.weap_type = remap_iw5_weap_type(underbarrel.weapon_type);
            row.facts.weap_class = underbarrel.weapon_class;
            row.facts.inventory_type = 3;
            alternate_assets = if underbarrel.share_ammo_with_alt {
                primary_assets
            } else {
                vec![underbarrel]
            };
            &alternate_assets
        } else {
            &primary_assets
        };
        apply_iw5_parameter_blocks(&mut row.facts, assets);
        if let Some(reticle) = assets.iter().find_map(|asset| asset.reticle.as_ref()) {
            row.reticle = reticle.clone();
        }
        if alternate {
            if let Some(projectile) = iw5_first_block(assets, |asset| asset.projectile) {
                row.facts.explosion_radius = projectile.explosion_radius;
                row.facts.explosion_inner_damage = projectile.explosion_inner_damage;
                row.facts.explosion_outer_damage = projectile.explosion_outer_damage;
                row.facts.projectile_speed = projectile.speed;
                row.facts.projectile_speed_up = projectile.speed_up;
                row.facts.projectile_activate_dist = projectile.activate_distance;
                row.facts.projectile_explosion_type = projectile.explosion_type;
                row.facts.proj_impact_explode = projectile.impact_explode;
                let asset = assets.iter().find(|asset| asset.projectile.is_some())?;
                row.projectile_model = asset.projectile_model.clone();
                row.combat_fx.explosion_hint = asset.projectile_explosion_fx.clone();
                row.proj_trail = asset.projectile_trail_fx.clone();
                row.proj_trail_from_slot = row.proj_trail.is_some();
                row.proj_ignition = asset.projectile_ignition_fx.clone();
                row.proj_ignition_from_slot = row.proj_ignition.is_some();
                row.sounds.proj_explosion = asset.projectile_explosion_sound.clone();
                row.sounds.proj_ignition_sound = asset.projectile_ignition_sound.clone();
            }
        }

        if let Some(scope) = assets.iter().find(|asset| asset.overlay.is_some()) {
            if let Some(overlay) = scope
                .overlay
                .as_deref()
                .filter(|name| !name.is_empty() && overlay_name_is_hud_iris(name))
            {
                row.overlay_material = Some(overlay.to_owned());
                row.overlay_image = None;
                row.overlay_material_from_slot = true;
                row.facts.ads_overlay_width = scope.width;
                row.facts.ads_overlay_height = scope.height;
            }
        }

        let mut anim_types: Vec<u32> = base
            .iw5_anim_overrides
            .iter()
            .map(|row| row.anim_tree_type)
            .collect();
        anim_types.sort_unstable();
        anim_types.dedup();
        for anim_type in anim_types {
            let Some(chosen) = self.select_iw5_anim_override(base_id, native, anim_type) else {
                continue;
            };
            let Some(slot) = iw5_anim_tree_type_to_iw4_slot(anim_type) else {
                continue;
            };
            if let Some(anim) = if alternate {
                chosen.altmode_anim.clone()
            } else {
                chosen.override_anim.clone()
            } {
                row.sz_xanims[slot] = Some(anim);
            }
            let time = if alternate {
                chosen.alt_time_ms
            } else {
                chosen.anim_time_ms
            };
            if time > 0 {
                if let Some(timer) = iw5_anim_timer(&mut row.facts, slot) {
                    *timer = time;
                }
            }
        }

        for (sound_type, target) in [
            (sz::SND_OVERRIDE_TYPE_FIRE, &mut row.sounds.fire),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_FIRE,
                &mut row.sounds.fire_player,
            ),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_AKIMBO,
                &mut row.sounds.fire_player_akimbo,
            ),
            (
                sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT,
                &mut row.sounds.fire_last_player,
            ),
        ] {
            if let Some(sound) = self
                .select_iw5_sound_override(base_id, native, sound_type)
                .and_then(|chosen| {
                    if alternate {
                        chosen.altmode_sound.clone()
                    } else {
                        chosen.override_sound.clone()
                    }
                })
            {
                *target = Some(sound);
            }
        }

        for (fx_type, hint) in [
            (1, &mut row.combat_fx.view_flash_hint),
            (2, &mut row.combat_fx.world_flash_hint),
            (3, &mut row.combat_fx.view_shell_eject_hint),
            (4, &mut row.combat_fx.world_shell_eject_hint),
        ] {
            if let Some(fx) = self
                .select_iw5_fx_override(base_id, native, fx_type)
                .and_then(|chosen| {
                    if alternate {
                        chosen.altmode_fx.clone()
                    } else {
                        chosen.override_fx.clone()
                    }
                })
            {
                *hint = Some(fx);
            }
        }

        if let Some(reload) = self.select_iw5_reload_override(base_id, native) {
            row.facts.reload_add_time_ms = reload.reload_add_time_ms;
            row.facts.reload_start_add_time_ms = reload.reload_start_add_time_ms;
        }
        if let Some(notetracks) = self.select_iw5_notetrack_override(base_id, native) {
            row.sounds.notetrack_sound_map = notetracks.sound_map.clone();
        }
        Some(row)
    }

    pub fn iw5_configuration_of(&self, id: u32) -> Option<(u32, Iw5AttachmentSelection)> {
        self.rows.get(id as usize)?.iw5_configuration
    }

    fn rebuild_name_maps(&mut self) {
        self.by_name.clear();
        self.by_namespaced.clear();
        for (id, row) in self.rows.iter().enumerate().skip(1) {
            if row.iw5_configuration.is_some() {
                continue;
            }
            self.by_namespaced
                .insert((row.namespace, row.name.clone()), id as u32);
            self.by_name.entry(row.name.clone()).or_insert(id as u32);
        }
        for row in &mut self.rows {
            if let Some(name) = row.alternate_weapon.as_deref() {
                row.alternate_index = self
                    .by_namespaced
                    .get(&(row.namespace, normalize_weapon_name(name)))
                    .copied()
                    .unwrap_or(0);
            }
        }
    }

    pub fn reticle_of(&self, index: u32) -> Option<&WeaponReticleAssets> {
        self.rows.get(index as usize).map(|row| &row.reticle)
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

    pub fn sz_xanim_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanim_edges)
    }

    pub fn notetrack_action_of(&self, index: u32, note: &str) -> Option<&LinkedNotetrackAction> {
        self.rows
            .get(index as usize)?
            .notetrack_actions
            .get(&note.to_ascii_lowercase())
    }

    pub fn notetrack_actions_of(
        &self,
        index: u32,
    ) -> impl Iterator<Item = (&str, &LinkedNotetrackAction)> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.iter())
            .map(|(note, action)| (note.as_str(), action))
    }

    pub fn notetrack_sound_aliases_of(&self, index: u32) -> impl Iterator<Item = &str> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.values())
            .filter_map(|action| action.sound_alias.as_deref())
    }

    pub fn sz_xanim_right_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_right_edges)
    }

    pub fn sz_xanim_left_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_left_edges)
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

    pub fn bound_weapon_xanim_indices(&self) -> Vec<usize> {
        let mut indices = std::collections::BTreeSet::new();
        for row in self.rows.iter().skip(1) {
            for edge in row
                .sz_xanim_edges
                .iter()
                .chain(&row.sz_xanim_right_edges)
                .chain(&row.sz_xanim_left_edges)
            {
                if let Some(index) = edge.bound_index() {
                    indices.insert(index);
                }
            }
        }
        indices.into_iter().collect()
    }

    pub fn gun_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows.get(index as usize).map(|row| row.gun_xmodel_edge)
    }

    pub fn hand_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.hand_xmodel_edge)
    }

    pub fn rocket_model_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.rocket_model_edge)
    }

    pub fn attachment_view_model_edges_of(&self, index: u32) -> &[AssetEdge<FpvMeshSpace>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_view_model_edges.as_slice())
    }

    pub fn fpv_hands_of(
        &self,
        index: u32,
        axis: bool,
    ) -> Option<(&asset_model::FpvHands, crate::FpvMeshIndex)> {
        self.rows.get(index as usize)?.fpv_hands[usize::from(axis)]
            .as_ref()
            .map(|(choice, index)| (choice, *index))
    }

    pub fn fpv_mount_plan_of(&self, index: u32) -> Option<&asset_model::FpvMountPlan> {
        self.rows
            .get(index as usize)?
            .fpv_mount_plan
            .as_ref()?
            .as_ref()
            .ok()
    }

    pub fn alternate_fpv_pairs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.alternate_fpv.keys().copied()
    }

    pub fn fpv_assemblies_for(
        &self,
        index: u32,
        parent: u32,
        axis: bool,
    ) -> Option<&crate::FpvSideAssemblies> {
        if parent != 0 && self.facts_of(index).is_some_and(|f| f.inventory_type == 3) {
            return self.alternate_fpv.get(&(index, parent))?[usize::from(axis)]
                .as_ref()?
                .as_ref()
                .ok();
        }
        self.fpv_assemblies_of(index, axis)
    }

    pub fn fpv_assemblies_of(&self, index: u32, axis: bool) -> Option<&crate::FpvSideAssemblies> {
        self.rows.get(index as usize)?.fpv_assemblies[usize::from(axis)]
            .as_ref()?
            .as_ref()
            .ok()
    }

    pub fn fpv_assembly_gap_of(&self, index: u32, axis: bool) -> String {
        let Some(row) = self.rows.get(index as usize) else {
            return "no such weapon".to_owned();
        };
        match (&row.fpv_mount_plan, &row.fpv_assemblies[usize::from(axis)]) {
            (None, _) => "unlinked selected models".to_owned(),
            (Some(Err(error)), _) => error.to_string(),
            (_, None) => "effective kit / weapon hands".to_owned(),
            (_, Some(Err(error))) => error.clone(),
            (_, Some(Ok(_))) => "linked".to_owned(),
        }
    }

    pub fn fpv_clip_tracks(&self) -> &crate::FpvClipTracks {
        &self.fpv_clip_tracks
    }

    pub fn fpv_mount_error_of(&self, index: u32) -> Option<&asset_model::FpvMountError> {
        self.rows
            .get(index as usize)?
            .fpv_mount_plan
            .as_ref()?
            .as_ref()
            .err()
    }

    pub fn gun_xmodel_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.gun_xmodel_edge);
        }
        census
    }

    pub fn world_model_edge_of(&self, index: u32) -> Option<AssetEdge<WorldWeaponSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.world_model_edge)
    }

    pub fn attachment_world_model_edges_of(&self, index: u32) -> &[AssetEdge<WorldWeaponSpace>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_model_edges.as_slice())
    }

    pub fn attachment_world_mounts_of(&self, index: u32) -> &[Option<String>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_mounts.as_slice())
    }

    pub fn world_model_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.world_model_edge);
        }
        census
    }

    pub fn dependency_gaps(&self) -> Vec<WeaponDependencyGap> {
        (1..self.rows.len() as u32)
            .flat_map(|id| self.dependency_gaps_of(id))
            .collect()
    }

    pub fn dependency_gaps_of(&self, id: u32) -> Vec<WeaponDependencyGap> {
        let Some(row) = self.rows.get(id as usize).filter(|_| id != 0) else {
            return Vec::new();
        };
        let named = [
            (
                row.gun_xmodel.is_some() && !row.gun_xmodel_edge.is_bound(),
                "view model",
                &row.gun_xmodel,
            ),
            (
                row.hand_xmodel.is_some() && !row.hand_xmodel_edge.is_bound(),
                "hands",
                &row.hand_xmodel,
            ),
            (
                row.rocket_model.is_some() && !row.rocket_model_edge.is_bound(),
                "FPV rocket model",
                &row.rocket_model,
            ),
            (
                row.world_model_edge.is_unresolved(),
                "world model",
                &row.world_model,
            ),
        ];
        let anims = row
            .sz_xanim_edges
            .iter()
            .zip(&row.sz_xanims)
            .map(|(edge, name)| (edge.is_unresolved(), "anim", name));
        let dual_anims = !row.facts.no_dual_wield
            && row.sz_xanims_right[weap_anim::IDLE]
                .as_deref()
                .is_some_and(|name| !name.is_empty());
        let right_anims = row
            .sz_xanim_right_edges
            .iter()
            .zip(&row.sz_xanims_right)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "right anim", name));
        let left_anims = row
            .sz_xanim_left_edges
            .iter()
            .zip(&row.sz_xanims_left)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "left anim", name));
        let mut gaps: Vec<WeaponDependencyGap> = named
            .into_iter()
            .chain(anims)
            .chain(right_anims)
            .chain(left_anims)
            .filter(|(unresolved, ..)| *unresolved)
            .map(|(_, kind, name)| WeaponDependencyGap {
                id,
                kind,
                name: name.clone().unwrap_or_default(),
            })
            .collect();
        if let Some(Err(error)) = &row.fpv_mount_plan {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV mount",
                name: error.to_string(),
            });
        }
        for (side, assembly) in row.fpv_assemblies.iter().enumerate() {
            if let Some(Err(error)) = assembly {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV skeleton",
                    name: format!("{}: {error}", if side == 0 { "allies" } else { "axis" }),
                });
            }
        }
        if row.gun_xmodel_edge.is_bound() {
            for (side, hands) in row.fpv_hands.iter().enumerate() {
                if hands.is_none() {
                    gaps.push(WeaponDependencyGap {
                        id,
                        kind: "FPV hands layout",
                        name: if side == 0 { "allies" } else { "axis" }.to_owned(),
                    });
                }
            }
        }
        for (name, edge) in row
            .attachment_view_models
            .iter()
            .zip(&row.attachment_view_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV attachment model",
                    name: name.clone(),
                });
            }
        }
        for (name, edge) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment model",
                    name: name.clone(),
                });
            }
        }
        for ((name, edge), mount) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
            .zip(&row.attachment_world_mounts)
        {
            if edge.is_bound() && mount.is_none() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment mount",
                    name: name.clone(),
                });
            }
        }
        if row.attachment_world_models.len() != row.attachment_world_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "world attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_world_models.len(),
                    row.attachment_world_model_edges.len()
                ),
            });
        }
        if row.attachment_view_models.len() != row.attachment_view_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_view_models.len(),
                    row.attachment_view_model_edges.len()
                ),
            });
        }
        gaps
    }

    pub fn configuration_admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        if !self.configuration_supported(id) {
            return Err(crate::ConfigurationRefusal::Unsupported(format!(
                "`{}` needs a runtime mechanism IW4L lacks",
                self.name_of(id)
            )));
        }
        match self.dependency_gaps_of(id).first() {
            Some(gap) => Err(crate::ConfigurationRefusal::MissingDependency {
                weapon: self.name_of(id).to_owned(),
                kind: gap.kind,
                name: gap.name.clone(),
            }),
            None => Ok(()),
        }
    }

    pub fn world_model_entry<'a>(
        &self,
        index: u32,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        if self.world_catalog_identity != catalog.identity() {
            return None;
        }
        catalog.get_at(self.world_model_edge_of(index)?.bound_index()?)
    }

    pub fn authored_weapon_sound(&self, index: u32, slot: WeaponSoundSlot) -> Option<&str> {
        self.sounds_of(index)?
            .hint(slot)
            .filter(|name| !name.is_empty())
    }

    pub fn weapon_sound_alias<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        self.weapon_sound_key(index, slot, catalog)
            .map(|(_, alias)| alias)
    }

    pub fn weapon_sound_key<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::AssetNamespace, &'a str)> {
        let ns = self.namespace_of(index).unwrap_or_default();
        sound_alias_in_bank(self.authored_weapon_sound(index, slot), ns, catalog)
    }

    pub fn bounce_sound_alias<'a>(
        &self,
        index: u32,
        surf: usize,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        let ns = self.namespace_of(index).unwrap_or_default();
        sound_alias_in_bank(self.bounce_sound_of(index, surf), ns, catalog).map(|(_, alias)| alias)
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

    pub fn alternate_name_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)?
            .alternate_weapon
            .as_deref()
            .filter(|name| !name.is_empty())
    }

    pub fn alternate_of(&self, index: u32) -> u32 {
        self.rows
            .get(index as usize)
            .map_or(0, |row| row.alternate_index)
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
        match self.by_name.get(&norm).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName { name: norm }),
        }
    }

    pub fn resolve_key(&self, key: &crate::AssetKey) -> Result<Option<u32>, UnknownWeaponName> {
        if key.kind != crate::AssetKind::Weapon {
            return Err(UnknownWeaponName {
                name: key.display(),
            });
        }
        let norm = normalize_weapon_name(key.logical_name());
        match self.by_namespaced.get(&(key.namespace, norm)).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName {
                name: key.display(),
            }),
        }
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

    pub fn runnable_table(&self) -> Vec<bool> {
        (0..self.rows.len() as u32)
            .map(|id| id != 0 && self.configuration_admission(id).is_ok())
            .collect()
    }

    pub fn weapon_families(&self) -> &crate::WeaponFamilies {
        &self.families
    }

    pub fn resolve_configuration(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<crate::ResolvedConfiguration, crate::ConfigurationRefusal> {
        self.families.resolve(selection, rules, self)
    }

    pub fn describe_configuration(&self, id: u32) -> Option<&crate::WeaponSelection> {
        self.families.describe(id)
    }

    pub fn prepared_attachments_of(&self, id: u32) -> &[String] {
        self.rows
            .get(id as usize)
            .map_or(&[], |row| row.prepared_attachments.as_slice())
    }

    pub fn configuration_label(&self, id: u32) -> String {
        match self
            .describe_configuration(id)
            .and_then(|selection| Some((selection.family.as_ref()?, &selection.attachments)))
        {
            Some((family, attachments)) => {
                let mut label = family.short();
                for name in attachments {
                    label.push_str(" +");
                    label.push_str(name);
                }
                label
            }
            None => self.namespaced_key_of(id).unwrap_or_default(),
        }
    }

    pub fn configuration_transition_groups(&self) -> Vec<u32> {
        let mut keys: Vec<_> = self
            .families
            .families()
            .iter()
            .map(|family| family.key.clone())
            .collect();
        keys.sort_by_key(|key| key.asset_key());
        keys.dedup();
        let groups: HashMap<_, _> = keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| (key, index as u32 + 1))
            .collect();
        let mut result = vec![0; self.rows.len()];
        for id in 1..self.rows.len() as u32 {
            let Some(selection) = self.describe_configuration(id) else {
                continue;
            };
            let Some(key) = selection.family.as_ref() else {
                continue;
            };
            if self
                .resolve_configuration(selection, crate::LoadoutRules::default())
                .is_ok_and(|resolved| resolved.id == id)
            {
                result[id as usize] = groups.get(key).copied().unwrap_or(0);
            }
        }
        result
    }

    pub fn list_attachment_choices(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<Vec<crate::AttachmentOption>, crate::ConfigurationRefusal> {
        self.families.attachment_options(selection, rules, self)
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

    pub fn attachment_view_models_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_view_models.as_slice())
    }

    pub fn attachment_world_models_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_models.as_slice())
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

    pub fn sz_xanims_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanims)
    }

    pub fn sz_xanims_right_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanims_right)
    }

    pub fn sz_xanims_left_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_SLOTS]> {
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

    pub fn quick_reload_timers_of(&self, index: u32) -> Option<(i32, i32)> {
        let dual_mag = self.facts_of(index)?.dual_mag?;
        Some((dual_mag.reload_ms, dual_mag.reload_empty_ms))
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

    pub fn dpad_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        Some((row.dpad_icon_image.as_deref()?, row.dpad_icon_ratio))
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
}

fn iw5_primary_ads(assets: &[&Iw5ScopeRow]) -> (Option<fastfile_iw5::AttachmentAdsSettings>, f32) {
    let mut chosen = None;
    let mut scale_product = 1.0;
    for asset in assets {
        let settings = if asset.share_ammo_with_alt {
            asset.ads_settings_main
        } else {
            asset.ads_settings
        };
        if chosen.is_none() {
            chosen = settings;
        }
        let scale = if asset.share_ammo_with_alt {
            (asset.ads_settings_main.is_none()
                && asset.scales.ads_settings_main != 0.0
                && asset.scales.ads_settings_main != 1.0)
                .then_some(asset.scales.ads_settings_main)
        } else {
            (asset.scales.ads_settings > 0.0).then_some(asset.scales.ads_settings)
        };
        if let Some(scale) = scale {
            scale_product *= scale;
        }
    }
    (chosen, scale_product)
}

fn iw5_default_scope_models(
    slots: &[Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    attachments: &HashMap<String, Iw5ScopeRow>,
) -> Option<(Option<String>, Option<String>)> {
    slots[..6]
        .iter()
        .filter_map(Option::as_deref)
        .find_map(|name| {
            if !name.ends_with("scope") || name.ends_with("vzscope") {
                return None;
            }
            let asset = attachments.get(name)?;
            let view = asset.view_models[0].clone();
            let world = asset.world_models[0].clone();
            (view.is_some() || world.is_some()).then_some((view, world))
        })
}

fn iw5_first_block<T: Copy>(
    assets: &[&Iw5ScopeRow],
    block: impl Fn(&Iw5ScopeRow) -> Option<T>,
) -> Option<T> {
    assets.iter().find_map(|asset| block(asset))
}

fn iw5_scale_product(
    assets: &[&Iw5ScopeRow],
    scale: impl Fn(&fastfile_iw5::AttachmentScales) -> f32,
) -> f32 {
    assets
        .iter()
        .map(|asset| scale(&asset.scales))
        .filter(|scale| *scale > 0.0)
        .product()
}

fn scale_i32(value: i32, scale: f32) -> i32 {
    if scale == 1.0 {
        value
    } else {
        (value as f32 * scale) as i32
    }
}

fn apply_iw5_parameter_blocks(facts: &mut WeaponBodyFacts, assets: &[&Iw5ScopeRow]) {
    let (ads, ads_scale) = iw5_primary_ads(assets);
    if let Some(ads) = ads {
        facts.ads_spread = ads.ads_spread;
        facts.ads_aim_pitch = ads.ads_aim_pitch;
        facts.ads_crosshair_in_frac = ads.ads_crosshair_in_frac;
        facts.ads_crosshair_out_frac = ads.ads_crosshair_out_frac;
        facts.ads_zoom_fov = ads.ads_zoom_fov;
        facts.ads_zoom_in_frac = ads.ads_zoom_in_frac;
        facts.ads_zoom_out_frac = ads.ads_zoom_out_frac;
        facts.ads_bob_factor_at_0x330 = ads.ads_bob_factor;
        facts.ads_view_bob_mult_at_0x334 = ads.ads_view_bob_mult;
        if ads.ads_trans_in_time > 0.0 {
            facts.ads_in_rate = 1.0 / ads.ads_trans_in_time;
        }
        if ads.ads_trans_out_time > 0.0 {
            facts.ads_out_rate = 1.0 / ads.ads_trans_out_time;
        }
    }
    if ads_scale != 1.0 {
        facts.ads_spread *= ads_scale;
        facts.ads_aim_pitch *= ads_scale;
        facts.ads_zoom_fov *= ads_scale;
        facts.ads_in_rate /= ads_scale;
        facts.ads_out_rate /= ads_scale;
    }

    if let Some(sight) = iw5_first_block(assets, |a| a.sight) {
        facts.aim_down_sight = sight.aim_down_sight;
        facts.ads_fire_only = sight.ads_fire;
        facts.rechamber_while_ads = sight.rechamber_while_ads;
        facts.no_ads_when_mag_empty = sight.no_ads_when_mag_empty;
    }
    if let Some(general) = iw5_first_block(assets, |a| a.ammo_general) {
        facts.penetrate_type = general.penetrate_type;
        facts.penetrate_multiplier = general.penetrate_multiplier;
        facts.impact_type = general.impact_type;
        facts.fire_type = general.fire_type;
        facts.rifle_bullet = general.rifle_bullet;
    }
    if let Some(reload) = iw5_first_block(assets, |a| a.reload) {
        facts.no_partial_reload = reload.no_partial_reload;
        facts.segmented_reload = reload.segmented_reload;
    }
    if let Some(add_ons) = iw5_first_block(assets, |a| a.add_ons) {
        facts.motion_tracker = add_ons.motion_tracker;
    }
    if let Some(general) = iw5_first_block(assets, |a| a.general) {
        facts.bolt_action = general.bolt_action;
        facts.inherits_perks = general.inherits_perks;
        facts.move_speed_scale = general.move_speed_scale;
        facts.ads_move_speed_scale = general.ads_move_speed_scale;
    }

    if let Some(ammo) = iw5_first_block(assets, |a| a.ammunition) {
        facts.max_ammo = ammo.max_ammo;
        facts.start_ammo = ammo.start_ammo;
        facts.clip_size = ammo.clip_size;
        facts.shots_per_fire = ammo.shot_count;
        facts.reload_ammo_add = ammo.reload_ammo_add;
        facts.reload_start_add = ammo.reload_start_add;
    }
    let ammo_scale = iw5_scale_product(assets, |s| s.ammunition);
    facts.max_ammo = scale_i32(facts.max_ammo, ammo_scale);
    facts.start_ammo = scale_i32(facts.start_ammo, ammo_scale);
    facts.clip_size = scale_i32(facts.clip_size, ammo_scale);

    if let Some(damage) = iw5_first_block(assets, |a| a.damage) {
        facts.damage = damage.damage;
        facts.min_damage = damage.min_damage;
        facts.melee_damage = damage.melee_damage;
        facts.max_damage_range = damage.max_damage_range;
        facts.min_damage_range = damage.min_damage_range;
        facts.min_player_damage = damage.min_player_damage;
    }
    facts.damage = scale_i32(facts.damage, iw5_scale_product(assets, |s| s.damage));
    let damage_min = iw5_scale_product(assets, |s| s.damage_min);
    facts.min_damage = scale_i32(facts.min_damage, damage_min);
    facts.min_player_damage = scale_i32(facts.min_player_damage, damage_min);

    if let Some(location) = iw5_first_block(assets, |a| a.location_damage) {
        let mut mult = facts.location_damage_mult.unwrap_or([1.0; 20]);
        mult[..19].copy_from_slice(&location);
        facts.location_damage_mult = Some(mult);
    }

    if let Some(idle) = iw5_first_block(assets, |a| a.idle_settings) {
        facts.idle.hip_idle_amount_at_0x370 = idle.hip_idle_amount;
        facts.idle.hip_idle_speed_at_0x378 = idle.hip_idle_speed;
        facts.idle.idle_crouch_factor_at_0x37c = idle.idle_crouch_factor;
        facts.idle.idle_prone_factor_at_0x380 = idle.idle_prone_factor;
    }
    let idle_scale = iw5_scale_product(assets, |s| s.idle_settings);
    facts.idle.hip_idle_amount_at_0x370 *= idle_scale;
    facts.idle.ads_idle_amount_at_0x36c *= idle_scale;

    if let Some(spread) = iw5_first_block(assets, |a| a.hip_spread) {
        let v = spread.values;
        apply_leftover_hip_spread(
            facts,
            [
                v[0], v[1], v[2], v[3], v[4], v[5], v[9], v[6], v[7], v[8], v[10], v[11],
            ],
        );
    }
    let spread_scale = iw5_scale_product(assets, |s| s.hip_spread);
    if spread_scale != 1.0 {
        for value in [
            &mut facts.hip_spread_stand_min,
            &mut facts.hip_spread_ducked_min,
            &mut facts.hip_spread_prone_min,
            &mut facts.hip_spread_stand_max,
            &mut facts.hip_spread_ducked_max,
            &mut facts.hip_spread_prone_max,
        ] {
            *value *= spread_scale;
        }
    }

    let kick = &mut facts.kick;
    if let Some(gun) = iw5_first_block(assets, |a| a.gun_kick) {
        kick.hip_gun_kick_reduced_kick_bullets = gun.hip_reduced_kick_bullets;
        [
            kick.hip_gun_kick_reduced_kick_percent,
            kick.hip_gun_kick_pitch_min,
            kick.hip_gun_kick_pitch_max,
            kick.hip_gun_kick_yaw_min,
            kick.hip_gun_kick_yaw_max,
            kick.hip_gun_kick_accel,
            kick.hip_gun_kick_speed_max,
            kick.hip_gun_kick_speed_decay,
            kick.hip_gun_kick_static_decay,
        ] = gun.hip;
        kick.ads_gun_kick_reduced_kick_bullets = gun.ads_reduced_kick_bullets;
        [
            kick.ads_gun_kick_reduced_kick_percent,
            kick.ads_gun_kick_pitch_min,
            kick.ads_gun_kick_pitch_max,
            kick.ads_gun_kick_yaw_min,
            kick.ads_gun_kick_yaw_max,
            kick.ads_gun_kick_accel,
            kick.ads_gun_kick_speed_max,
            kick.ads_gun_kick_speed_decay,
            kick.ads_gun_kick_static_decay,
        ] = gun.ads;
    }
    let gun_scale = iw5_scale_product(assets, |s| s.gun_kick);
    if gun_scale != 1.0 {
        for value in [
            &mut kick.hip_gun_kick_pitch_min,
            &mut kick.hip_gun_kick_pitch_max,
            &mut kick.hip_gun_kick_yaw_min,
            &mut kick.hip_gun_kick_yaw_max,
            &mut kick.ads_gun_kick_pitch_min,
            &mut kick.ads_gun_kick_pitch_max,
            &mut kick.ads_gun_kick_yaw_min,
            &mut kick.ads_gun_kick_yaw_max,
        ] {
            *value *= gun_scale;
        }
    }
    if let Some(view) = iw5_first_block(assets, |a| a.view_kick) {
        [
            kick.hip_view_kick_pitch_min,
            kick.hip_view_kick_pitch_max,
            kick.hip_view_kick_yaw_min,
            kick.hip_view_kick_yaw_max,
            kick.f_hip_view_kick_center_speed,
            kick.ads_view_kick_pitch_min,
            kick.ads_view_kick_pitch_max,
            kick.ads_view_kick_yaw_min,
            kick.ads_view_kick_yaw_max,
            kick.f_ads_view_kick_center_speed,
        ] = view;
    }
    let view_scale = iw5_scale_product(assets, |s| s.view_kick);
    if view_scale != 1.0 {
        for value in [
            &mut kick.hip_view_kick_pitch_min,
            &mut kick.hip_view_kick_pitch_max,
            &mut kick.hip_view_kick_yaw_min,
            &mut kick.hip_view_kick_yaw_max,
            &mut kick.ads_view_kick_pitch_min,
            &mut kick.ads_view_kick_pitch_max,
            &mut kick.ads_view_kick_yaw_min,
            &mut kick.ads_view_kick_yaw_max,
        ] {
            *value *= view_scale;
        }
    }
    let center_scale = iw5_scale_product(assets, |s| s.view_center);
    kick.f_hip_view_kick_center_speed *= center_scale;
    kick.f_ads_view_kick_center_speed *= center_scale;

    facts.fire_time_ms = scale_i32(
        facts.fire_time_ms,
        iw5_scale_product(assets, |s| s.fire_timers),
    );
    let state = iw5_scale_product(assets, |s| s.state_timers);
    if state != 1.0 {
        for value in [
            &mut facts.fire_delay_ms,
            &mut facts.melee_delay_ms,
            &mut facts.melee_charge_delay_ms,
            &mut facts.rechamber_time_ms,
            &mut facts.rechamber_bolt_time_ms,
            &mut facts.hold_fire_time_ms,
            &mut facts.melee_time_ms,
            &mut facts.melee_charge_time_ms,
            &mut facts.reload_time_ms,
            &mut facts.reload_show_rocket_time_ms,
            &mut facts.reload_empty_time_ms,
            &mut facts.reload_add_time_ms,
            &mut facts.reload_start_time_ms,
            &mut facts.reload_start_add_time_ms,
            &mut facts.reload_end_time_ms,
            &mut facts.drop_time_ms,
            &mut facts.raise_time_ms,
            &mut facts.quick_drop_time_ms,
            &mut facts.quick_raise_time_ms,
            &mut facts.sprint_raise_time_ms,
            &mut facts.sprint_loop_time_ms,
            &mut facts.sprint_drop_time_ms,
        ] {
            *value = scale_i32(*value, state);
        }
    }
}

fn iw5_anim_timer(facts: &mut WeaponBodyFacts, slot: usize) -> Option<&mut i32> {
    Some(match slot {
        weap_anim::FIRE => &mut facts.fire_time_ms,
        weap_anim::RECHAMBER => &mut facts.rechamber_time_ms,
        weap_anim::MELEE => &mut facts.melee_time_ms,
        weap_anim::MELEE_CHARGE => &mut facts.melee_charge_time_ms,
        weap_anim::RELOAD => &mut facts.reload_time_ms,
        weap_anim::RELOAD_EMPTY => &mut facts.reload_empty_time_ms,
        weap_anim::RELOAD_START => &mut facts.reload_start_time_ms,
        weap_anim::RELOAD_END => &mut facts.reload_end_time_ms,
        weap_anim::ALT_RAISE => &mut facts.alternate_raise_time_ms,
        weap_anim::ALT_DROP => &mut facts.alternate_drop_time_ms,
        weap_anim::RAISE => &mut facts.raise_time_ms,
        weap_anim::DROP => &mut facts.drop_time_ms,
        weap_anim::QUICK_RAISE => &mut facts.quick_raise_time_ms,
        weap_anim::QUICK_DROP => &mut facts.quick_drop_time_ms,
        weap_anim::SPRINT_IN => &mut facts.sprint_raise_time_ms,
        weap_anim::SPRINT_LOOP => &mut facts.sprint_loop_time_ms,
        weap_anim::SPRINT_OUT => &mut facts.sprint_drop_time_ms,
        _ => return None,
    })
}

fn normalize_weapon_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix("_mp").unwrap_or(&lower).to_owned()
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

impl crate::weapon_families::FamilyContent for WeaponRegistry {
    fn iw5_bind(
        &self,
        base_id: u32,
        attachments: &[String],
    ) -> Result<(), crate::ConfigurationRefusal> {
        let selection = self.resolve_iw5_attachment_slots(base_id, attachments)?;
        self.iw5_primary_attachment_assets(base_id, selection)
            .ok_or_else(|| {
                crate::ConfigurationRefusal::MissingContent(self.name_of(base_id).into())
            })?;
        Ok(())
    }

    fn lookup(&self, namespace: crate::AssetNamespace, name: &str) -> Option<u32> {
        self.by_namespaced
            .get(&(namespace, normalize_weapon_name(name)))
            .copied()
    }

    fn offhand_class(&self, id: u32) -> i32 {
        self.facts_of(id).map_or(0, |facts| facts.offhand_class)
    }

    fn admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        self.configuration_admission(id)
    }

    fn prepared(&self, selection: &crate::WeaponSelection) -> Option<u32> {
        self.configurations.get(selection).copied()
    }

    fn prepared_all(&self) -> Vec<(u32, crate::WeaponSelection)> {
        self.configurations
            .iter()
            .map(|(selection, &id)| (id, selection.clone()))
            .collect()
    }

    fn names_in(&self, namespace: crate::AssetNamespace) -> Vec<(u32, String)> {
        (1..=self.len() as u32)
            .filter(|&id| self.namespace_of(id) == Some(namespace))
            .filter(|&id| self.iw5_configuration_of(id).is_none())
            .map(|id| (id, normalize_weapon_name(self.name_of(id))))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvAssemblyCensus {
    pub built: usize,
    pub linked: usize,
    pub refused: usize,
    pub clip_tables: usize,
}
