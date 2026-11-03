mod admission;
mod atlas;
pub use atlas::replace_clip as prepare_scene_collision;
pub mod readiness;
pub use net::LocalAccount;
pub use readiness::SessionReadinessPolicy;
pub mod content_manifest;
pub mod life_front;
pub mod lifecycle;
pub mod loadout;
pub mod local_arm;
pub mod match_apply;
pub mod plugin;
pub mod startup;
pub mod view_subject;
mod weapon_content;
pub use weapon_content::{ClassWeaponAdmission, PreparedSimWeapons};

pub use content_manifest::{
    AuthorityWeaponProfile, ManifestFact, ManifestGap, RuntimeRuleset, SessionContentManifest,
    SessionManifestError, SessionWeaponId, SessionWeaponManifestRow,
};

pub use lifecycle::{
    LiveWorldIdentity, SessionSwapRequest, SessionSwapResult, SessionSwapTarget, TeardownRequest,
};
pub use loadout::{
    AuthoritativeClassProjection, ClassRow, perk_catalog_id, project_class, resolve_class_weapon,
    resolve_editor_class_weapon,
};
pub use local_arm::{
    arm_local_from_presented, join_local_on_class_select, sync_prediction_metrics_to_probe,
};
pub use match_apply::{apply_prepared_match, install_script_model_id, sim_clip_map};
pub use plugin::SessionPlugin;
pub use startup::{PendingConsoleLines, StartupCommands};

mod scope_control;
pub use scope_control::ScopeControl;
