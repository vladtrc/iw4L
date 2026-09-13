use std::collections::HashMap;

use assets::FxCatalog;
use bevy::prelude::*;
use fx::FxSystemHost;
use fx_iw4::FxPostLight;

use crate::present::FxElemInfoCache;

#[derive(Resource, Default)]
pub struct PreparedFxCatalog(pub FxCatalog);

#[derive(Resource, Default)]
pub struct PreparedFxModels(pub assets::FxModelCatalog);

#[derive(Resource, Default)]
pub struct PreparedFxElemInfos(pub FxElemInfoCache);

#[derive(Resource, Default)]
pub struct FxSoundStamp {
    pub fx_n: usize,
    pub bank_revision: u64,
}

#[derive(Resource, Default)]
pub struct PreparedImpactFx(pub Option<assets::OwnedFxImpactTable>);

#[derive(Resource, Default)]
pub struct FxWorldColorImages {
    pub colors: HashMap<String, Handle<Image>>,
    pub keys: HashMap<String, (u8, u16)>,
    pub colors_by_asset: HashMap<usize, Handle<Image>>,
    pub keys_by_asset: HashMap<usize, (u8, u16)>,
    pub mark_colors_by_asset: HashMap<usize, Handle<Image>>,
    pub mark_keys_by_asset: HashMap<usize, (u8, u16)>,
}

#[derive(Resource, Debug)]
pub struct FxDumpRequest {
    pub pending: bool,
    pub radius: f32,
}

impl Default for FxDumpRequest {
    fn default() -> Self {
        Self {
            pending: false,
            radius: 800.0,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct FxMarkDvars {
    pub fx_marks: bool,
    pub fx_marks_smodels: bool,
    pub fx_marks_ents: bool,
    pub fx_mark_profile: bool,

    pub fx_cull_elem_draw: bool,
}

impl Default for FxMarkDvars {
    fn default() -> Self {
        Self {
            fx_marks: true,
            fx_marks_smodels: true,
            fx_marks_ents: true,
            fx_mark_profile: false,
            fx_cull_elem_draw: true,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct LaserDvars {
    pub light: bool,
    pub range: f32,
    pub range_player: f32,
    pub radius: f32,
    pub end_offset: f32,
}

impl Default for LaserDvars {
    fn default() -> Self {
        Self {
            light: true,
            range: 1500.0,
            range_player: 1500.0,
            radius: 0.8,
            end_offset: 0.5,
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct HostFxDlights {
    pub scene: Vec<lighting_iw4::GfxLightPack>,
    pub cap_full: u32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct HostFxPostLights {
    pub queued: Vec<FxPostLight>,
    pub cap_full: u32,
    pub drawn: u32,
    pub skipped_short: u32,

    pub miss_material: u32,
}

impl HostFxPostLights {
    pub fn add(&mut self, light: FxPostLight) {
        if !fx_iw4::fx_post_light_add_allows(self.queued.len() as u32) {
            self.cap_full = self.cap_full.saturating_add(1);
            return;
        }
        self.queued.push(light);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresentedVehicleFxRow {
    pub fx_spawn: Option<&'static str>,
    pub fx_spawn_def: Option<String>,
}

#[derive(Resource, Default, Debug)]
pub struct PresentedVehicleFx {
    pub by_id: HashMap<u32, PresentedVehicleFxRow>,
}

#[derive(Resource)]
pub struct HostFxSystem(pub FxSystemHost);

impl Default for HostFxSystem {
    fn default() -> Self {
        Self(FxSystemHost::new())
    }
}

#[derive(Resource, Default)]
pub struct FxJournalCursor {
    pub logged: bool,
    pub impact_played: u32,
    pub impact_miss_table: u32,
    pub impact_miss_def: u32,
    pub impact_miss_def_warned: bool,
    pub boom_played: u32,
    pub muzzle_gap: u32,
    pub tracer_gap: u32,
    pub tracer_skip_interval: u32,
    pub tracer_skip_short: u32,
    pub tracer_skip_no_def: u32,
    pub fire_sound_gap: u32,
    pub explosion_sound_gap: u32,
    pub explosion_gap: u32,
    pub muzzle_played: u32,
    pub muzzle_bolted: u32,
    pub tracer_from_tag: u32,
    pub brass_gap: u32,
    pub brass_played: u32,

    pub pellet_played: u32,
    pub createfx_miss: u32,
    pub createfx_booted: bool,

    pub createfx_boot_msec: Option<i32>,
    pub draw_logged: bool,
    pub draw_miss_material: u32,
    pub skipped_no_ordinal: u32,
    pub skipped_not_emissive: u32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct CombatFxDump {
    pub muzzle_gap: u32,
    pub muzzle_played: u32,

    pub muzzle_bolted: u32,

    pub tracer_from_tag: u32,
    pub tracer_gap: u32,
    pub brass_gap: u32,
    pub brass_played: u32,
    pub last_brass_name: Option<String>,

    pub pellet_played: u32,
    pub explosion_played: u32,
    pub explosion_gap: u32,
    pub last_explosion_name: Option<String>,

    pub last_explosion_table: Option<String>,

    pub last_explosion_slot: Option<String>,
    pub impact_played: u32,
    pub impact_miss_table: u32,
    pub impact_miss_def: u32,
    pub last_surf: Option<i64>,
    pub last_row: Option<i64>,
    pub last_surf_flags: Option<i64>,
    pub last_surf_name: Option<String>,
    pub last_impact_def: Option<String>,

    pub last_impact_miss_why: Option<String>,

    pub last_impact_cell_empty: Option<i64>,

    pub last_weapon_tracer_edge: Option<String>,

    pub last_weapon_flash_edge: Option<String>,

    pub last_weapon_brass_edge: Option<String>,

    pub last_weapon_explosion_edge: Option<String>,
    pub last_tracer_name: Option<String>,
    pub last_muzzle_name: Option<String>,

    pub last_fire_player_view: Option<i64>,

    pub fire_sound_gap: u32,

    pub last_fire_alias: Option<String>,

    pub last_fire_lastshot: Option<i64>,

    pub last_brass_lastshot: Option<i64>,
    pub last_tracer_material: Option<String>,
    pub last_tracer_bind: Option<String>,

    pub last_tracer_mat_edge: Option<String>,

    pub last_tracer_mat_index: Option<i64>,
    pub last_tracer_has_color: Option<i64>,
    pub last_tracer_speed: Option<f32>,
    pub last_tracer_beam_length: Option<f32>,
    pub last_tracer_draw_interval: Option<i64>,

    pub muzzle_msec: Option<i32>,

    pub impact_msec: Option<i32>,

    pub explosion_msec: Option<i32>,
    pub tracer_spawned: u32,
    pub tracer_skip_interval: u32,
    pub tracer_skip_short: u32,
    pub tracer_skip_no_def: u32,
    pub tracer_live: u32,
    pub beam_queued: u32,
    pub beam_drawn: u32,
    pub beam_miss_material: u32,
    pub beam_miss_color: u32,
    pub beam_miss_ordinal: u32,
    pub beam_miss_emissive: u32,

    pub beam_miss_unprepared: u32,
}

#[derive(Resource, Default)]
pub struct FxCameraOrigin(pub [f32; 3]);
