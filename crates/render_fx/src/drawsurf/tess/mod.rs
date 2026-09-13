pub mod fx;
pub mod mark;
pub mod particle_cloud;

pub use fx::FxCodeMeshPlan;
pub use mark::GfxMarkMeshPlan;
pub use particle_cloud::FxParticleCloudPlan;

use std::sync::Arc;

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
