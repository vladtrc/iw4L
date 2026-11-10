mod common_sources;
pub use common_sources::*;
mod artifact_cache;
mod asset_graph;
mod gltf_export;
mod iwd;
mod lane;
mod lane_capability;
pub mod loading_screen;
mod map_load_process;
mod merged_map;
mod scene_recipe;
pub use scene_recipe::{
    SceneEndpoint, ScenePlayerclipOpening, ScenePortal, SceneRecipe, SceneSpace,
    parse_portal_replacement,
};
mod random_doors;
pub use merged_map::{LoadingWorld, MergedMap};
pub use random_doors::{
    MAX_DOOR_HEIGHT_TRIM, MAX_DOOR_WIDTH_TRIM, fit_portal_endpoint, shuffle_random_doors,
};
mod map_scripts;
mod match_load;
pub mod plugin;
pub mod prepared;
pub mod session_load;

pub use artifact_cache::{cache_flight, cache_get, cache_put, fnv1a64, fnv1a64_more};
pub(crate) use asset_graph::stamp_match_destructible_death;
pub use asset_graph::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, AssetGraphCensus, AssetGraphLinker,
    DESTRUCTIBLE_DEATH_HINTS, DeathClipEdge, DeathHuskEdge, DestructibleDeathHint,
    DestructibleDeathRow, FpvMeshIndex, FpvMeshSpace, FxIndex, FxModelIndex, FxModelSpace, FxSpace,
    LoadedSoundIndex, LoadedSoundSpace, MapXModelIndex, MapXModelSpace, MaterialIndex,
    MaterialSpace, ProjectileModelIndex, ProjectileModelSpace, SoundAliasIndex, SoundAliasSpace,
    TechniqueSetIndex, TechniqueSetSpace, TracerIndex, TracerSpace, WorldWeaponIndex,
    WorldWeaponSpace, XAnimIndex, XAnimSpace,
};
pub use fastfile_iw4::GlyphCapture;
pub use gltf_export::{GltfExportSummary, export_prepared_world_gltf};
pub use iwd::{IwdSoundIndex, NamespaceSoundIwd, NamespaceTree, NamespaceTrees};
mod lanes;
mod zone_lane;
pub use lane::REFERENCE_ZONE_LANE;
pub use lane_capability::{LaneStatus, PreparedCapability, lane_status};
pub use lanes::{ZoneLanes, install_zone_lanes, lane};
pub use lighting_iw4::MODEL_LIGHTING_TILE_BYTES;
pub use loading_screen::{LoadingPreviewSource, LoadingScreen};
pub use map_load_process::MapLoadProcess;
pub use match_load::{
    MapLoadApproval, MatchLoadAccepted, MatchLoadBusy, MatchLoadDispatch, MatchLoadRequest,
    PreparedMatchSound,
};
pub use plugin::AssetPlugin;
pub use prepared::{
    BoundWeapons, MapFacts, MatchMaterials, MatchType10SoundHints, PreparedBodies,
    PreparedBodyClips, PreparedDestructibleDeath, PreparedFpvMeshes, PreparedGaps,
    PreparedLocalizedStrings, PreparedMap, PreparedProjectileMeshes, PreparedWeapons,
    PreparedWorldWeapons, PreparedXAnims, PreparedXModelWalkCensus, SessionCompass,
};
pub use session_load::{
    MatchLoadOutcome, MatchMaterialSeed, PreparedAtlasWorld, PreparedMatch, ShellCommon,
    apply_match_material_map, load_match_material_catalog, load_match_material_seed, load_pool,
    load_prepared_match, load_shell_common, load_workers, publish_process_cpus, use_process_cpus,
};
pub use zone_lane::*;

pub mod image_handles;

mod script_sources;
pub use script_sources::{ScriptSourceOrigin, ScriptSources, ScriptTable};

mod map_model_catalog;
pub use map_model_catalog::{
    MapXModelCatalog, T5DestructibleInitial, link_model_placements, normalize_bomb_sites,
};

pub use lane::helpers::{
    decode_reflection_probes, report_dpvs, report_ffa_spawns, report_intermission,
    report_map_models, report_world_batches, smodel_lighting_samples,
};

mod prepared_world;
pub use prepared_world::PreparedWorld;

mod map_script_family;
pub use map_script_family::{
    MapScriptFamilies, MapScriptFamily, REFERENCE_MAP_SCRIPT_FAMILY, install_map_script_families,
    map_script_builtin_sources, map_script_family,
};

mod prepared_dependencies;
pub use prepared_dependencies::{
    DependencyImportance, DependencyRole, PreparedCapabilities, PreparedDependency,
};

pub use merged_map::validate_scene_version;
