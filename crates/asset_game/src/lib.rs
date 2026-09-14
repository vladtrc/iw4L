use asset_anim::{ClipScheduler, ClipSchedulerError, XAnimCatalog};
use asset_audio::{SoundCatalog, game_nested_string_assignment};
use asset_core::*;
use asset_material::{MaterialCatalog, MaterialDefinitions, decode_ui_image};
use asset_model::{
    FpvMeshCatalog, ModelSkel, ProjectileMeshCatalog, WorldWeaponCatalog, WorldWeaponEntry,
    capture_xmodel_skel,
};
use asset_transport::{
    GamesRoot, Iw5ZoneMemory, T5ZoneMemory, ZoneMemory, find_zone_file, find_zone_for_tree,
    open_zone,
};
use asset_world::decode_rawfile_text;
pub mod arena;
mod attachment_hide;
mod cac_stats;
mod fx_catalog;
mod fx_model_catalog;
mod graph_support;
mod impact_fx_catalog;
mod localize;
mod lochit;
mod menu_catalog;
mod penetration;
mod tracer_catalog;
mod weapon_anim_dispatch;
mod weapon_animations;
mod weapon_catalog;

pub use arena::*;
pub use attachment_hide::*;
pub use cac_stats::*;
pub use fx_catalog::*;
pub use fx_model_catalog::*;
pub use graph_support::AuthoredRef;
pub use impact_fx_catalog::*;
pub use localize::*;
pub use lochit::*;
pub use menu_catalog::*;
pub use penetration::*;
pub use tracer_catalog::*;
pub use weapon_anim_dispatch::*;
pub use weapon_animations::*;
pub use weapon_catalog::*;

pub mod asset_graph {
    pub(crate) use crate::graph_support::*;
    pub use asset_core::*;
    pub use asset_model::link::*;
}
pub mod discover {
    pub use asset_transport::*;
}
pub mod material_catalog {
    pub use asset_material::*;
}
pub mod material_images {
    pub use asset_material::*;
}
pub mod model_skel {
    pub use asset_model::*;
}
pub mod zone {
    pub use asset_transport::*;
}
