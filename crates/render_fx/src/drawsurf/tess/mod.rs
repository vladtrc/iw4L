pub mod fx;
pub mod mark;
pub mod particle_cloud;

pub use fx::FxCodeMeshPlan;
pub use mark::GfxMarkMeshPlan;
pub use particle_cloud::FxParticleCloudPlan;

use std::sync::Arc;

pub(crate) fn reset_rows<T>(rows: &mut Arc<Vec<T>>) {
    match Arc::get_mut(rows) {
        Some(v) => v.clear(),
        None => *rows = Arc::new(Vec::new()),
    }
}

pub(crate) fn publish_index_ranges(
    draws: impl IntoIterator<Item = (u32, u32)>,
) -> Arc<Vec<(u32, u32)>> {
    Arc::new(draws.into_iter().collect())
}
