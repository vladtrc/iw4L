use crate::entries::{GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry};

pub const FRONTEND_DRAW_LISTS_DWORDS: usize = 0x74 / 4;

pub const SRC_WORLD_PTR: usize = 0x08 / 4;
pub const SRC_WORLD_COUNT: usize = 0x0c / 4;

pub const SRC_PRETESS_PTR: usize = SRC_WORLD_PTR;
pub const SRC_PRETESS_COUNT: usize = SRC_WORLD_COUNT;

pub const SRC_SMODEL_RIGID_PTR: usize = 0x58 / 4;
pub const SRC_SMODEL_RIGID_COUNT: usize = 0x5c / 4;

pub const SRC_XMODEL_RIGID_PTR: usize = 0x50 / 4;
pub const SRC_XMODEL_RIGID_COUNT: usize = 0x54 / 4;

pub const SRC_SMODEL_CACHED_BYTES: usize = 0x28 / 4;
pub const SRC_SMODEL_CACHED_PTR: usize = 0x2c / 4;

pub const SRC_SMODEL_PRETESS_BYTES: usize = 0x34 / 4;
pub const SRC_SMODEL_PRETESS_PTR: usize = 0x38 / 4;

pub const SRC_SMODEL_SKINNED_BYTES: usize = 0x1c / 4;
pub const SRC_SMODEL_SKINNED_PTR: usize = 0x20 / 4;

pub const LIST_TOKEN: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct PackedFrontendLists {
    pub src: [u32; FRONTEND_DRAW_LISTS_DWORDS],
    pub world: Vec<GfxTrianglesListEntry>,
    pub smodel: Vec<GfxSmodelRigidEntry>,
    pub xmodel: Vec<GfxXModelRigidEntry>,

    pub world_draw_indices: Vec<u32>,
    pub xmodel_draw_indices: Vec<u32>,

    pub smodel_draw_indices: Vec<u32>,

    pub smodel_cached: Vec<GfxSmodelRigidEntry>,
    pub smodel_pretess: Vec<GfxSmodelRigidEntry>,
    pub smodel_cached_draw_indices: Vec<u32>,
    pub smodel_pretess_draw_indices: Vec<u32>,

    pub smodel_skinned: Vec<GfxSmodelRigidEntry>,
    pub smodel_skinned_draw_indices: Vec<u32>,

    pub skipped_other: u32,
    pub skipped_empty_ib: u32,
}

impl PackedFrontendLists {
    #[must_use]
    pub fn work_entry_n(&self) -> u32 {
        let n = self.world.len()
            + self.smodel.len()
            + self.xmodel.len()
            + self.smodel_cached.len()
            + self.smodel_pretess.len()
            + self.smodel_skinned.len();
        n as u32
    }
}
