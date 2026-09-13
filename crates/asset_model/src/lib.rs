mod body_catalog;
mod fpv_catalog;
pub mod link;
mod model_kind;
pub mod model_lighting;
mod model_lod;
mod model_skel;
mod packed_vertex;
mod projectile_mesh_catalog;
mod soldiers;
mod world_weapon_catalog;

pub use asset_core::*;
pub use body_catalog::*;
pub use fpv_catalog::*;
pub use model_kind::*;
pub use model_lighting::*;
pub use model_lod::*;
pub use model_skel::*;
pub use packed_vertex::*;
pub use projectile_mesh_catalog::*;
pub use soldiers::*;
pub use world_weapon_catalog::*;

pub mod asset_graph {
    pub use crate::link::*;
    pub use asset_core::*;
}

pub mod dobj {
    pub use xmodel_runtime::ModelPoseSrc;
}
