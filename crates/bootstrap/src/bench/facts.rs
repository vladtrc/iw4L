//! Reading the manifest's runtime half out of the running app.
//!
//! The device, the surface and the pools do not exist when the bench is
//! inserted — the render plugin has not finished, the window has no size and
//! the task pools have not been asked for a thread. They all exist by the first
//! frame, and none of them changes after it, so this snapshots once and hands
//! the result to the exit hook, which runs with no `World` to ask.

use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::window::PrimaryWindow;

use crate::bench::manifest::{AdapterFacts, RuntimeFacts, WindowFacts};

static FACTS: OnceLock<RuntimeFacts> = OnceLock::new();

/// The workload, which is known when the bench is inserted rather than when
/// the device is. Kept beside the collected facts so `snapshot` returns one
/// complete answer.
static WORKLOAD: OnceLock<(String, Option<String>, String)> = OnceLock::new();

/// What was asked for, recorded at insert time: zone, demo path and role.
pub(crate) fn workload(zone: &str, demo: Option<&str>, role: &str) {
    let _ = WORKLOAD.set((zone.to_owned(), demo.map(str::to_owned), role.to_owned()));
}

/// What was collected, or an empty set if the run never reached a frame. Every
/// field is optional, so "never reached a frame" reads as a manifest of nulls
/// rather than as a manifest of wrong values.
pub(crate) fn snapshot() -> RuntimeFacts {
    let mut facts = match FACTS.get() {
        Some(facts) => facts.clone(),
        None => RuntimeFacts::default(),
    };
    if let Some((zone, demo, role)) = WORKLOAD.get() {
        facts.zone = Some(zone.clone());
        facts.demo = demo.clone();
        facts.role = Some(role.clone());
    }
    facts
}

/// Collect once, then do nothing. Runs every frame because the cost of the
/// `OnceLock` load is smaller than the cost of a run condition that has to be
/// evaluated to reach the same answer.
pub(crate) fn collect(
    adapter: Option<Res<RenderAdapterInfo>>,
    device: Option<Res<RenderDevice>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if FACTS.get().is_some() {
        return;
    }
    let facts = RuntimeFacts {
        zone: None,
        demo: None,
        role: None,
        adapter: adapter.map(|adapter| AdapterFacts {
            name: adapter.0.name.clone(),
            backend: format!("{:?}", adapter.0.backend),
            device_type: format!("{:?}", adapter.0.device_type),
            driver: adapter.0.driver.clone(),
            driver_info: adapter.0.driver_info.clone(),
            // Whether the *device* was created with the feature, not whether
            // the adapter could have offered it: a pass can only be timed if
            // the device we actually hold can resolve the query.
            timestamp_queries: device.map(|device| {
                device
                    .features()
                    .contains(bevy::render::render_resource::WgpuFeatures::TIMESTAMP_QUERY)
            }),
        }),
        window: windows.single().ok().map(|window| WindowFacts {
            width: window.resolution.physical_width(),
            height: window.resolution.physical_height(),
            scale: f64::from(window.resolution.scale_factor()),
        }),
        present_mode: windows
            .single()
            .ok()
            .map(|window| format!("{:?}", window.present_mode)),
        pools: pools(),
    };
    let _ = FACTS.set(facts);
}

/// Threads in each Bevy pool. The pools are global and already built by the
/// time a frame runs, so this reads them rather than constructing anything.
fn pools() -> Vec<(String, usize)> {
    use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};
    let mut out = Vec::new();
    if let Some(pool) = ComputeTaskPool::try_get() {
        out.push(("compute".to_owned(), pool.thread_num()));
    }
    if let Some(pool) = AsyncComputeTaskPool::try_get() {
        out.push(("async_compute".to_owned(), pool.thread_num()));
    }
    if let Some(pool) = IoTaskPool::try_get() {
        out.push(("io".to_owned(), pool.thread_num()));
    }
    out
}
