use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::tasks::{TaskPool, TaskPoolBuilder};

use crate::{LoadedWorld, PreparedGaps, PreparedMap, PreparedWorld, lane};
use asset_anim::{XAnimBuild, XAnimCatalog};
use asset_game::{
    FxCatalog, LocalizeCatalog, WeaponBuild, WeaponRegistry, load_localize_catalog_in_lane,
};
use asset_material::{ImageDemandPlan, MaterialCatalog};
use asset_model::{
    BodyMeshCatalog, FpvMeshBuild, FpvMeshCatalog, ProjectileMeshBuild, WorldWeaponBuild,
    WorldWeaponCatalog,
};
use asset_transport::load_jobs::{self, JobKind};
use asset_transport::{
    LoadProgress, StageHandle, StageId, find_common_mp_for_envelope, find_runtime_common_mp,
    find_runtime_zone, find_zone_file_version, find_zone_for_tree, games_root, open_zone_shared,
    peek_zone_version,
};
use asset_world::{ClipCollision, WorldDraw};

pub use asset_world::WorldDrawPolicy;

mod atlas_worlds;
pub use atlas_worlds::AtlasLoad;
pub use atlas_worlds::atlas_load;
mod common_cache;
mod common_tables;
use common_tables::CommonTables;
mod common_walks;
mod map_common_payloads;
mod match_walk;

use common_cache::*;
use common_walks::*;

pub use common_cache::{CommonKey, CommonSet, ShellCommon, load_shell_common};
pub use common_walks::{
    MatchMaterialSeed, apply_match_material_map, load_match_material_catalog,
    load_match_material_seed,
};
pub use match_walk::{load_prepared_match, load_prepared_match_with_atlas};

static PROCESS_CPUS: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();

pub fn publish_process_cpus(cpus: Vec<usize>) {
    let _ = PROCESS_CPUS.set(cpus);
}

pub fn use_process_cpus() {
    if let Some(cpus) = PROCESS_CPUS.get() {
        set_thread_cpus(cpus);
    }
}

pub fn load_workers() -> usize {
    PROCESS_CPUS
        .get()
        .map(Vec::len)
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .map_or(1, |n| n.saturating_sub(2).max(1))
}

pub fn load_pool() -> &'static TaskPool {
    static POOL: std::sync::OnceLock<TaskPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(load_workers())
            // Native zone parsers carry bounded capture tables on the stack.
            // The debug IW4 world path exceeds the platform's 2 MiB default.
            .stack_size(8 * 1024 * 1024)
            .thread_name("iw4l load".to_owned())
            .on_thread_spawn(|| {
                if let Some(cpus) = PROCESS_CPUS.get() {
                    set_thread_cpus(cpus);
                }
            })
            .build()
    })
}

#[cfg(target_os = "linux")]
fn set_thread_cpus(cpus: &[usize]) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for cpu in cpus {
            if *cpu >= libc::CPU_SETSIZE as usize {
                return;
            }
            libc::CPU_SET(*cpu, &mut set);
        }
        libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &raw const set);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_thread_cpus(_cpus: &[usize]) {}

pub struct PreparedAtlasWorld {
    pub id: asset_core::atlas::WorldId,
    pub zone: String,
    pub collision: Arc<ClipCollision>,
}

pub struct PreparedMatch {
    pub ui_images: asset_material::UiImagePublication,
    pub scripts: crate::ScriptSources,
    pub world: PreparedWorld,

    pub fx: asset_game::FxDefinitions,
    pub materials: crate::MatchMaterials,
    pub clip: Option<Arc<ClipCollision>>,
    pub atlas_worlds: Vec<PreparedAtlasWorld>,
    pub weapons: Arc<WeaponRegistry>,
    pub fpv_meshes: Arc<FpvMeshCatalog>,
    pub bodies: Arc<BodyMeshCatalog>,
    pub soldiers: asset_game::SoldierPresentations,
    pub world_weapons: WorldWeaponCatalog,

    pub projectile_meshes: asset_model::ProjectileMeshCatalog,
    pub xanims: Arc<XAnimCatalog>,
    pub destructible_death: Vec<crate::DestructibleDeathRow>,
    pub player_anim_sources: asset_anim::PlayerAnimSources,

    pub tracers: asset_game::TracerDefinitions,

    pub strings: LocalizeCatalog,
    pub report: Vec<String>,

    pub prepared_map: PreparedMap,

    pub pen_table: weapon_iw4::PenetrationDepthTable,
    pub pen_table_loaded: bool,
    pub lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    pub sound: Option<Result<asset_audio::SoundCatalog, String>>,
    pub sound_gaps: usize,
    pub script_sound_aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
}

pub enum MatchLoadOutcome {
    Ready(Box<PreparedMatch>),
    Canceled,
    Refused(String),
}

mod atlas_dynents;
