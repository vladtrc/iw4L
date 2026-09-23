mod admission;
mod bot_loadout;
pub mod combat_table;
pub mod content_manifest;
pub mod life_front;
pub mod lifecycle;
pub mod local_arm;
pub mod match_apply;
pub mod plugin;
pub mod view_subject;

pub use content_manifest::{
    AuthorityWeaponProfile, ManifestFact, ManifestGap, RuntimeRuleset, SessionContentManifest,
    SessionManifestError, SessionWeaponId, SessionWeaponManifestRow,
};

pub use bot_loadout::{UniqueLoadoutProjection, project_unique_bot_classes};
pub use frame::{
    LifeEndCause, LifeEnded, LifeStartReason, LifeStarted, MatchInstalled, MatchTornDown,
    TeardownReason,
};
pub use life_front::{LifeFrontCensus, LifeNotifyCensus};
pub use lifecycle::{
    LiveWorldIdentity, SessionSwapCompletion, SessionSwapRequest, SessionSwapResult,
    SessionSwapTarget, TeardownGaps, TeardownRequest,
};
pub use local_arm::{
    arm_local_from_presented, join_local_on_class_select, reset_look_on_life_started,
    sync_prediction_metrics_to_probe,
};
pub use match_apply::{
    AuthoritativeClassProjection, ClassRow, PendingConsoleLines, PerkRuntimeContract,
    StartupCommands, apply_prepared_match, authoritative_class_lock_reason,
    deathstreak_lock_reason, deathstreak_runtime_contract, install_script_model_id,
    perk_catalog_id, perk_runtime_contract, project_class, resolve_class_weapon,
};
pub use plugin::SessionPlugin;

mod map_conveyer;
mod map_diggers;
mod map_doors;
mod map_lights;
mod map_moving_diggers;

mod objectives;
