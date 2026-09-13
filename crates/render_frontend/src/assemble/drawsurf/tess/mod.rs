pub mod glass;
pub mod smodel;
pub mod world;
pub mod xmodel;

pub use render_fx::drawsurf::tess::{fx, mark, particle_cloud};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TessKind {
    World,

    Smodel,

    XModel,

    CodeMesh,

    ParticleCloud,

    MarkMesh,

    Glass,
}

pub use glass::{CgGlassTable, GfxGlassMeshPlan};
pub use render_fx::drawsurf::tess::mark::GfxMarkSubKey;
pub use render_fx::drawsurf::tess::mark::mark_mesh_surface_samplers;
pub use render_fx::{FxCodeMeshPlan, FxParticleCloudPlan, GfxMarkMeshPlan};
pub use smodel::{RetailPackedVertexRefusal, SmodelGpuPlan, SmodelPassMaterial, SmodelVertex};
pub use world::{RetailWorldVertexRefusal, WorldDrawGpuPlan, WorldPassMaterial, WorldVertex};
pub use xmodel::{
    DynEntDrawPlan, FpvDrawPlan, FxModelDrawPlan, ItemDrawPlan, MissileDrawPlan,
    RemoteBodyDrawPlan, ScriptModelDrawPlan, XModelDrawPlan,
};

use std::sync::Arc;

pub(crate) const SHARE_BANKS: usize = 3;

#[derive(Clone, Debug)]
pub(crate) struct ShareBanks<T> {
    banks: [Arc<Vec<T>>; SHARE_BANKS],
    write: usize,
}

impl<T> Default for ShareBanks<T> {
    fn default() -> Self {
        Self {
            banks: [
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
            ],
            write: 0,
        }
    }
}

impl<T> ShareBanks<T> {
    pub(crate) fn acquire_unique_keep(&mut self) -> (&mut Vec<T>, bool) {
        for i in 0..SHARE_BANKS {
            let idx = (self.write + i) % SHARE_BANKS;
            if Arc::strong_count(&self.banks[idx]) == 1 {
                self.write = idx;
                let vec = Arc::get_mut(&mut self.banks[idx]).expect("unique bank");
                return (vec, false);
            }
        }
        self.write = (self.write + 1) % SHARE_BANKS;
        self.banks[self.write] = Arc::new(Vec::new());
        (
            Arc::get_mut(&mut self.banks[self.write]).expect("new bank is unique"),
            true,
        )
    }

    pub(crate) fn published(&self) -> Arc<Vec<T>> {
        Arc::clone(&self.banks[self.write])
    }

    pub(crate) fn take_write(&mut self) -> Vec<T> {
        let (vec, _) = self.acquire_unique_keep();
        let mut rows = std::mem::take(vec);
        rows.clear();
        rows
    }

    pub(crate) fn take_write_keep(&mut self) -> (Vec<T>, bool) {
        let (vec, fresh) = self.acquire_unique_keep();
        (std::mem::take(vec), fresh)
    }

    pub(crate) fn write_index(&self) -> usize {
        self.write
    }

    pub(crate) fn put_write(&mut self, rows: Vec<T>) {
        let vec =
            Arc::get_mut(&mut self.banks[self.write]).expect("write bank unique after take_write");
        *vec = rows;
    }
}

pub(crate) fn steal_into_share<T>(src: &mut Vec<T>) -> Arc<Vec<T>> {
    Arc::new(std::mem::take(src))
}

pub(crate) fn reclaim_share<T>(share: &mut Option<Arc<Vec<T>>>, dst: &mut Vec<T>) {
    dst.clear();
    if let Some(arc) = share.take() {
        if let Ok(mut rows) = Arc::try_unwrap(arc) {
            rows.clear();
            *dst = rows;
        }
    }
}

pub(crate) fn published_or_live<'a, T>(share: Option<&'a Arc<Vec<T>>>, live: &'a [T]) -> &'a [T] {
    share.map(|rows| rows.as_slice()).unwrap_or(live)
}

pub(crate) fn publish_index_ranges(
    draws: impl IntoIterator<Item = (u32, u32)>,
) -> Arc<Vec<(u32, u32)>> {
    Arc::new(draws.into_iter().collect())
}

pub mod sky;
