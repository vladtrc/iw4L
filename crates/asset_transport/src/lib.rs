pub mod artifact_cache;
pub mod discover;
pub mod iwd;
pub mod load_jobs;
pub mod namespace_trees;
pub mod progress;
pub mod zone;

pub use artifact_cache::{
    CacheFlight, cache_flight, cache_get, cache_open, cache_put, cache_put_with, fnv1a64,
    fnv1a64_more,
};
pub use asset_core::FamilyId;
pub use discover::{
    GamesRoot, MapPack, ZoneFile, ensure_artifacts_dir, find_common_mp_for_envelope,
    find_common_mp_for_zone, find_game_install, find_localized_common_mp_for_zone,
    find_runtime_common_mp, find_runtime_zone, find_zone_file, find_zone_file_version,
    find_zone_for_tree, folder_holds_game, game_install_root, game_root_for_zone,
    games_content_report, games_root, games_root_report, group_mp_maps, list_mp_map_packs,
    list_mp_maps, map_load_title, peek_zone_version, search_roots, set_game_folders,
    split_zone_key, zone_game_for_path, zone_version,
};
pub use iwd::{
    IwdFile, IwdIndex, IwdSoundIndex, cached_iwd_dirs, game_main_for_zone, game_mains_by_root,
    game_mains_under, inflate_deflate, inflate_zlib, installation_revision, iwd_entry_reads,
    iwd_read_cost, read_iwd_named, read_text,
};
pub use load_jobs::{CacheResult, Job, JobKind};
pub use namespace_trees::{NamespaceSoundIwd, NamespaceTree, NamespaceTrees};
pub use progress::{
    LoadLaneTiming, LoadProgress, LoadSnapshot, StageEnd, StageHandle, StageId, StageKey,
    StageOutcome, StageScope, StageSnapshot, WorkCount, peak_resident_bytes,
    process_resident_bytes,
};
pub use zone::{
    Iw4WireFormat, ZoneDecoders, ZoneImage, ZoneMemory, ZoneOpenError, install_zone_decoders,
    open_zone, open_zone_shared, parse_zone_image, xfile_arena_row, zone_share_counts,
};

pub use discover::{GamesProvider, install_games_provider};
