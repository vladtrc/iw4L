mod weapon_model_preparation;
pub use weapon_model_preparation::*;
mod weapon_capture_input;
pub use weapon_capture_input::*;
mod map_presentation;
use asset_anim::{ClipScheduler, ClipSchedulerError, XAnimCatalog};
use asset_audio::SoundCatalog;
use asset_core::*;
use asset_material::{MaterialCatalog, MaterialDefinitions};
use asset_model::{
    FpvMeshCatalog, ModelSkel, ProjectileMeshCatalog, WorldWeaponCatalog, WorldWeaponEntry,
    capture_xmodel_skel,
};
use asset_transport::{ZoneMemory, open_zone};
pub use map_presentation::*;
pub mod arena;
mod attachment_hide;
mod cac_host;
mod cac_presentation;
mod cac_stats;
mod fpv_assembly;
mod fx_catalog;
mod fx_model_catalog;
mod graph_support;
mod impact_fx_catalog;
mod localize;
mod lochit;
mod string_table;
pub use string_table::*;
mod menu_catalog;
mod menu_source;
mod penetration;
mod soldier_presentation;
pub mod structured_data;
mod tracer_catalog;
mod weapon_anim_dispatch;
mod weapon_animations;
mod weapon_camo;
mod weapon_facts;
pub use weapon_facts::*;
mod weapon_visuals;
pub use weapon_visuals::*;
mod weapon_attachments;
pub use weapon_attachments::*;
mod weapon_catalog;
pub use weapon_camo::*;
mod weapon_families;
mod weapon_policy;
mod weapon_semantics;
pub use weapon_policy::*;

pub use arena::*;
pub use attachment_hide::*;
pub use cac_host::*;
pub use cac_presentation::*;
pub use cac_stats::*;
pub use fpv_assembly::*;
pub use fx_catalog::*;
pub use fx_model_catalog::*;
pub use graph_support::AuthoredRef;
pub use impact_fx_catalog::*;
pub use localize::*;
pub use lochit::*;
pub use menu_catalog::*;
pub use penetration::*;
pub use soldier_presentation::*;
pub use tracer_catalog::*;
pub use weapon_anim_dispatch::*;
pub use weapon_animations::*;
pub use weapon_catalog::*;
pub use weapon_families::*;
pub use weapon_semantics::*;

pub mod asset_graph {
    pub(crate) use crate::graph_support::*;
}

mod fpv_family;
pub use fpv_family::FpvFamilyConnection;

mod weapon_configuration;
pub use weapon_configuration::*;

mod weapon_selection;
pub use weapon_selection::*;

mod weapon_capture;
pub use weapon_capture::*;
mod weapon_sounds;
pub use weapon_sounds::*;

mod weapon_draft;
pub use weapon_draft::*;
mod weapon_effects;
pub use weapon_effects::*;

mod weapon_configuration_preparation;
pub use weapon_configuration_preparation::*;

mod weapon_tables;
pub use weapon_tables::*;

mod weapon_runtime_policy;
pub use weapon_runtime_policy::*;

mod weapon_unlock;
pub use weapon_unlock::*;
